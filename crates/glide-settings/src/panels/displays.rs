use glide_ipc::OutputInfo;
use iced::widget::{Row as IcedRow, button, column, container, row, text};
use iced::{Alignment, Element, Fill, Length, Task};
use toml_edit::Table;

use super::{Ctx, PageEvent, Pane};
use crate::i18n::Key as K;
use crate::icons::Icon;
use crate::services::config::ConfigStore;
use crate::services::display::{self, DisplayState, SCALES};
use crate::services::run::{Debounce, Generation, blocking};
use crate::theme;
use crate::widgets::{self, Group, Row};

mod tr {
    crate::i18n::catalogue! {
        Arrangement => "Arrangement", "Розташування";
        ArrangementHint => "Displays sit side by side from left to right. Select one to change it.", "Дисплеї стоять поруч зліва направо. Виберіть дисплей, щоб змінити його.";
        MoveLeft => "Move left", "Посунути ліворуч";
        MoveRight => "Move right", "Посунути праворуч";
        Recommended => "recommended", "рекомендовано";
        UseRecommended => "Use recommended", "Рекомендований режим";
        ModeHint => "Switches the display mode right away", "Режим дисплея змінюється одразу";
        ModeOfflineHint => "Applied the next time Glide starts", "Застосується під час наступного запуску Glide";
        Automatic => "Automatic", "Автоматично";
        FastestHint => "The fastest rate for this resolution", "Найвища частота для цієї роздільності";
        Brightness => "Brightness", "Яскравість";
        BrightnessHint => "Built-in screen backlight", "Підсвічування вбудованого екрана";
        Details => "Details", "Подробиці";
        Connector => "Connector", "Роз'єм";
        Model => "Model", "Модель";
        ScreenSize => "Screen size", "Розмір екрана";
        Workspace => "Workspace size", "Розмір робочої області";
        Position => "Position", "Позиція";
        BuiltIn => "Built-in display", "Вбудований дисплей";
        External => "External display", "Зовнішній дисплей";
        Current => "Current", "Поточний";
        KeepQuestion => "Keep these display settings?", "Залишити ці налаштування дисплея?";
        RevertingIn => "The previous settings return in", "Попередні налаштування повернуться через";
        Seconds => "s", "с";
        KeepChanges => "Keep changes", "Залишити зміни";
        Revert => "Revert", "Повернути";
    }
}

pub const CONFIRM_SECONDS: u8 = 15;

type Snapshot = Vec<(String, Option<Table>)>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Countdown {
    Stale,
    Running,
    Expired,
}

#[derive(Debug, Default)]
struct Guard {
    pending: Option<(Snapshot, u8)>,
    generation: Generation,
}

impl Guard {
    fn capture(config: &ConfigStore, names: &[String]) -> Snapshot {
        names.iter().map(|name| (name.clone(), config.output_snapshot(name))).collect()
    }

    fn changed(before: &Snapshot, config: &ConfigStore) -> bool {
        before.iter().any(|(name, table)| {
            config.output_snapshot(name).map(|t| t.to_string()) != table.as_ref().map(ToString::to_string)
        })
    }

    fn arm(&mut self, before: Snapshot) -> u64 {
        let snapshot = match self.pending.take() {
            Some((kept, _)) => kept,
            None => before,
        };
        self.pending = Some((snapshot, CONFIRM_SECONDS));
        self.generation.bump()
    }

    fn remaining(&self) -> Option<u8> {
        self.pending.as_ref().map(|(_, left)| *left)
    }

    fn tick(&mut self, generation: u64) -> Countdown {
        if !self.generation.is_current(generation) {
            return Countdown::Stale;
        }
        let Some((_, left)) = self.pending.as_mut() else {
            return Countdown::Stale;
        };
        *left = left.saturating_sub(1);
        if *left == 0 { Countdown::Expired } else { Countdown::Running }
    }

    fn keep(&mut self) {
        self.pending = None;
        self.generation.bump();
    }

    fn revert(&mut self, config: &mut ConfigStore) {
        if let Some((snapshot, _)) = self.pending.take() {
            for (name, table) in &snapshot {
                config.restore_output(name, table.as_ref());
            }
        }
        self.generation.bump();
    }
}

fn countdown(generation: u64) -> Task<Msg> {
    Task::perform(blocking(|| std::thread::sleep(std::time::Duration::from_secs(1))), move |_| {
        Msg::Countdown(generation)
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModeChoice {
    pub key: Option<String>,
    pub label: String,
}

impl std::fmt::Display for ModeChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

#[derive(Debug, Clone)]
pub enum Msg {
    Loaded(DisplayState),
    Refresh,
    Select(usize),
    Scale(f64),
    Mode(ModeChoice),
    Move(bool),
    Brightness(f64),
    SettleBrightness(u64),
    BrightnessDone(Option<Result<String, String>>),
    Countdown(u64),
    Keep,
    Revert,
}

#[derive(Debug, Default)]
struct Dimmer {
    value: Option<u8>,
    in_flight: bool,
    dirty: bool,
    settle: Debounce,
}

#[derive(Debug, Default)]
pub struct Page {
    state: DisplayState,
    loading: bool,
    loaded: bool,
    selected: usize,
    brightness: Dimmer,
    error: Option<String>,
    guard: Guard,
}

impl Pane for Page {
    type Msg = Msg;

    const KEYWORDS: &'static [&'static str] = &[
        "monitor",
        "screen",
        "resolution",
        "scale",
        "refresh",
        "brightness",
        "arrangement",
        "hz",
        "монітор",
        "екран",
        "роздільна",
        "масштаб",
        "частота",
        "яскравість",
        "розташування",
    ];

    fn open(&mut self, _c: Ctx, _config: &ConfigStore) -> Task<Msg> {
        if self.loaded || self.loading {
            return Task::none();
        }
        self.refresh()
    }

    fn update(&mut self, msg: Msg, _c: Ctx, config: &mut ConfigStore) -> Task<Msg> {
        Page::update(self, msg, config)
    }

    fn view<'a>(&'a self, c: Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        Page::view(self, c, config)
    }

    fn event(&mut self, event: PageEvent, _c: Ctx) -> Task<Msg> {
        match event {
            PageEvent::Reloaded | PageEvent::OutputsChanged => self.refresh(),
        }
    }
}

impl Page {
    pub fn refresh(&mut self) -> Task<Msg> {
        self.loading = true;
        Task::perform(blocking(display::read), |state| Msg::Loaded(state.unwrap_or_default()))
    }

    #[must_use]
    pub const fn outputs(&self) -> &DisplayState {
        &self.state
    }

    fn push_brightness(&mut self) -> Task<Msg> {
        let (Some(value), Some(backlight)) = (self.brightness.value, self.state.backlight.as_ref()) else {
            return Task::none();
        };
        if self.brightness.in_flight {
            self.brightness.dirty = true;
            return Task::none();
        }
        self.brightness.in_flight = true;
        self.brightness.dirty = false;
        let device = backlight.device.clone();
        Task::perform(blocking(move || display::set_brightness(&device, value)), Msg::BrightnessDone)
    }

    pub fn update(&mut self, msg: Msg, config: &mut ConfigStore) -> Task<Msg> {
        match msg {
            Msg::Loaded(state) => {
                self.state = state;
                self.loading = false;
                self.loaded = true;
                if self.selected >= self.state.outputs.len() {
                    self.selected = 0;
                }
                if !self.brightness.in_flight {
                    self.brightness.value = None;
                }
            }
            Msg::Refresh => return self.refresh(),
            Msg::Select(index) => self.selected = index,
            Msg::Scale(scale) => {
                let before = self.capture(config);
                if let Some(output) = self.state.outputs.get_mut(self.selected) {
                    config.set_output_scale(&output.name, scale);
                    config.tag_output(&output.name, &output.make, &output.model);
                    if !self.state.live {
                        output.scale = scale;
                    }
                }
                return self.guard_change(before, config);
            }
            Msg::Mode(choice) => {
                let before = self.capture(config);
                if let Some(output) = self.state.outputs.get(self.selected) {
                    config.set_output_mode(&output.name, choice.key.as_deref());
                    config.tag_output(&output.name, &output.make, &output.model);
                }
                return self.guard_change(before, config);
            }
            Msg::Countdown(generation) => match self.guard.tick(generation) {
                Countdown::Stale => {}
                Countdown::Running => return countdown(generation),
                Countdown::Expired => self.guard.revert(config),
            },
            Msg::Keep => self.guard.keep(),
            Msg::Revert => self.guard.revert(config),
            Msg::Move(right) => self.move_selected(right, config),
            Msg::Brightness(value) => {
                let value = value.round().clamp(1.0, 100.0) as u8;
                self.brightness.value = Some(value);
                let settle = self.brightness.settle.schedule(Msg::SettleBrightness);
                return Task::batch([self.push_brightness(), settle]);
            }
            Msg::SettleBrightness(generation) => {
                if self.brightness.settle.is_current(generation) && !self.brightness.in_flight {
                    return self.refresh();
                }
            }
            Msg::BrightnessDone(result) => {
                self.brightness.in_flight = false;
                self.error = match result {
                    Some(Err(err)) => Some(err),
                    _ => None,
                };
                if self.brightness.dirty {
                    return self.push_brightness();
                }
            }
        }
        Task::none()
    }

    fn capture(&self, config: &ConfigStore) -> Snapshot {
        let names: Vec<String> = self.state.outputs.iter().map(|output| output.name.clone()).collect();
        Guard::capture(config, &names)
    }

    fn guard_change(&mut self, before: Snapshot, config: &ConfigStore) -> Task<Msg> {
        if !self.state.live || !Guard::changed(&before, config) {
            return Task::none();
        }
        countdown(self.guard.arm(before))
    }

    fn move_selected(&mut self, right: bool, config: &mut ConfigStore) {
        let mut order = self.order(config);
        let Some(at) = order.iter().position(|index| *index == self.selected) else {
            return;
        };
        let target = if right { at + 1 } else { at.wrapping_sub(1) };
        if target >= order.len() {
            return;
        }
        order.swap(at, target);
        let widths: Vec<(String, f64)> = order
            .iter()
            .map(|index| {
                let output = &self.state.outputs[*index];
                (output.name.clone(), self.logical(config, *index).0)
            })
            .collect();
        let positions = display::row_positions(&widths);
        if !self.state.live {
            for (name, x, y) in &positions {
                if let Some(output) = self.state.outputs.iter_mut().find(|o| &o.name == name) {
                    output.x = i32::try_from(*x).unwrap_or(0);
                    output.y = i32::try_from(*y).unwrap_or(0);
                }
            }
        }
        config.set_output_positions(&positions);
        for output in &self.state.outputs {
            config.tag_output(&output.name, &output.make, &output.model);
        }
    }

    fn effective_scale(&self, config: &ConfigStore, index: usize) -> f64 {
        let Some(output) = self.state.outputs.get(index) else {
            return 1.0;
        };
        if self.state.live { output.scale } else { config.output_scale(&output.name).unwrap_or(output.scale) }
    }

    fn logical(&self, config: &ConfigStore, index: usize) -> (f64, f64) {
        let Some(output) = self.state.outputs.get(index) else {
            return (1920.0, 1080.0);
        };
        let mut logical = output.clone();
        logical.scale = self.effective_scale(config, index);
        let (w, h) = display::logical_size(&logical);
        if w <= 0.0 || h <= 0.0 { (1920.0, 1080.0) } else { (w, h) }
    }

    fn order(&self, config: &ConfigStore) -> Vec<usize> {
        let mut order: Vec<usize> = (0..self.state.outputs.len()).collect();
        let x = |index: usize| {
            let output = &self.state.outputs[index];
            if self.state.live {
                i64::from(output.x)
            } else {
                config.output_position(&output.name).map_or(i64::try_from(index).unwrap_or(0) * 100_000, |(x, _)| x)
            }
        };
        order.sort_by_key(|index| (x(*index), *index));
        order
    }

    #[must_use]
    pub fn view<'a>(&'a self, c: Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let t = c.theme;
        let mut page = column![].spacing(theme::SECTION_GAP);
        if !self.state.live && self.loaded {
            let reason = if self.state.compositor_running { K::DisplaysUnsupported } else { K::DisplaysOffline };
            page = page.push(widgets::banner(t, t.warn(), Icon::Warning, c.t(reason), None));
        }
        if let Some(error) = &self.error {
            page = page.push(widgets::banner(t, t.danger(), Icon::Warning, error.as_str(), None));
        }
        if let Some(left) = self.guard.remaining() {
            page = page.push(widgets::banner(
                t,
                t.warn(),
                Icon::Display,
                format!(
                    "{} {} {left} {}.",
                    c.t(tr::Key::KeepQuestion),
                    c.t(tr::Key::RevertingIn),
                    c.t(tr::Key::Seconds)
                ),
                Some(
                    row![
                        widgets::secondary(t, c.t(tr::Key::Revert), Some(Msg::Revert)),
                        widgets::primary(t, c.t(tr::Key::KeepChanges), Some(Msg::Keep)),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center)
                    .into(),
                ),
            ));
        }
        if self.state.outputs.is_empty() {
            return page
                .push(if self.loading || !self.loaded {
                    widgets::empty_state(t, Icon::Display, c.t(K::Loading), "")
                } else {
                    widgets::empty_state(t, Icon::Display, c.t(K::NoDisplays), c.t(K::NoDisplaysHint))
                })
                .into();
        }
        page.push(self.arrangement(&c, config))
            .push(self.output_group(&c, config))
            .push(self.details_group(&c, config))
            .into()
    }

    fn arrangement<'a>(&'a self, c: &Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let t = c.theme;
        let order = self.order(config);
        let sizes: Vec<(f64, f64)> = order.iter().map(|index| self.logical(config, *index)).collect();
        let total_w: f64 = sizes.iter().map(|(w, _)| w).sum();
        let max_h = sizes.iter().map(|(_, h)| *h).fold(1.0, f64::max);
        let factor = (130.0 / max_h).min(520.0 / total_w.max(1.0));
        let mut monitors = IcedRow::new().spacing(10).align_y(Alignment::End);
        for (slot, index) in order.iter().copied().enumerate() {
            let output = &self.state.outputs[index];
            let (w, h) = sizes[slot];
            let size = ((w * factor) as f32, (h * factor) as f32);
            monitors = monitors.push(monitor_tile(
                &t,
                output,
                index,
                self.effective_scale(config, index),
                size,
                index == self.selected,
            ));
        }
        let mut card = column![container(monitors).center_x(Fill)].spacing(14);
        if order.len() > 1 {
            let at = order.iter().position(|index| *index == self.selected).unwrap_or(0);
            let writable = c.writable;
            card = card.push(
                row![
                    text(c.t(tr::Key::ArrangementHint)).size(12.5).color(t.fg_2()).width(Fill),
                    widgets::secondary(t, c.t(tr::Key::MoveLeft), (writable && at > 0).then_some(Msg::Move(false)),),
                    widgets::secondary(
                        t,
                        c.t(tr::Key::MoveRight),
                        (writable && at + 1 < order.len()).then_some(Msg::Move(true)),
                    ),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            );
        }
        let body = container(card)
            .padding(theme::SECTION_GAP)
            .width(Fill)
            .style(move |_: &iced::Theme| theme::group_style(&t));
        column![
            Group::new(t)
                .title(c.t(tr::Key::Arrangement))
                .suffix(widgets::icon_button(t, Icon::Refresh, Some(Msg::Refresh)))
                .build(),
            body,
        ]
        .spacing(8)
        .into()
    }

    fn mode_rows<'a>(&'a self, c: &Ctx, config: &'a ConfigStore, output: &'a OutputInfo) -> Vec<Element<'a, Msg>> {
        let t = c.theme;
        let configured = config.output_mode(&output.name);
        let current_size = (output.width, output.height);
        let preferred = output.modes.iter().find(|mode| mode.preferred).copied();
        let hint = if self.state.live { c.t(tr::Key::ModeHint) } else { c.t(tr::Key::ModeOfflineHint) };
        let resolutions = display::resolutions(&output.modes);
        if resolutions.len() < 2 {
            return fixed_mode_rows(c, output);
        }
        let recommended = c.t(tr::Key::Recommended);
        let size_label = |(w, h): (i32, i32)| {
            let base = resolution_text(w, h);
            let base = match display::aspect_label(w, h) {
                Some(aspect) => format!("{base} ({aspect})"),
                None => base,
            };
            if preferred.is_some_and(|mode| (mode.width, mode.height) == (w, h)) {
                format!("{base} · {recommended}")
            } else {
                base
            }
        };
        let options: Vec<ModeChoice> = resolutions
            .iter()
            .map(|size| ModeChoice { key: Some(format!("{}x{}", size.0, size.1)), label: size_label(*size) })
            .collect();
        let selected = options
            .iter()
            .find(|choice| choice.key.as_deref() == Some(&format!("{}x{}", current_size.0, current_size.1)))
            .cloned();
        let mut subtitle = hint.to_string();
        if configured.is_some() {
            subtitle = format!("{subtitle} · {}", configured.as_deref().unwrap_or_default());
        }
        let mut rows = vec![
            Row::new(t, c.t(K::Resolution))
                .subtitle(subtitle)
                .suffix(widgets::inert(t, c.writable, widgets::dropdown(t, options, selected, Msg::Mode, 260.0)))
                .build(),
        ];
        rows.extend(refresh_row(c, output, configured.as_deref()));
        if configured.is_some() {
            rows.push(
                Row::new(t, c.t(tr::Key::UseRecommended))
                    .subtitle(preferred.map(|mode| display::mode_key(&mode)).unwrap_or_default())
                    .suffix(widgets::secondary(
                        t,
                        c.t(K::Reset),
                        c.writable.then_some(Msg::Mode(ModeChoice { key: None, label: String::new() })),
                    ))
                    .build(),
            );
        }
        rows
    }

    fn output_group<'a>(&'a self, c: &Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let t = c.theme;
        let Some(output) = self.state.outputs.get(self.selected) else {
            return widgets::gap(0.0);
        };
        let model = format!("{} {}", output.make, output.model).trim().to_string();
        let kind = if display::is_internal(&output.name) { c.t(tr::Key::BuiltIn) } else { c.t(tr::Key::External) };
        let scale = self.effective_scale(config, self.selected);
        let mut options: Vec<(u32, String)> =
            SCALES.iter().map(|value| (scale_id(*value), display::scale_label(*value))).collect();
        if !options.iter().any(|(id, _)| *id == scale_id(scale)) {
            options.push((scale_id(scale), display::scale_label(scale)));
        }
        let mut group = Group::new(t)
            .title(output.name.clone())
            .description(if model.is_empty() || model == "Unknown Unknown" {
                kind.to_string()
            } else {
                format!("{kind} · {model}")
            })
            .extend(self.mode_rows(c, config, output))
            .push(
                Row::new(t, c.t(K::Scale))
                    .subtitle(c.t(K::ScaleHint))
                    .suffix(widgets::inert(
                        t,
                        c.writable,
                        widgets::segmented(t, options, scale_id(scale), |id| Msg::Scale(f64::from(id) / 100.0)),
                    ))
                    .wide()
                    .build(),
            );
        let internal = display::is_internal(&output.name) || self.state.outputs.len() == 1;
        if let (Some(backlight), true) = (&self.state.backlight, internal) {
            let value = self.brightness.value.unwrap_or(backlight.percent);
            let control = row![
                widgets::icon(Icon::Sparkle, 16.0, t.fg_3()),
                widgets::value_slider(t, 1.0..=100.0, f64::from(value), 1.0, Msg::Brightness, None),
                text(format!("{value}%")).size(13).color(t.fg_2()).width(Length::Fixed(44.0)).align_x(Alignment::End),
            ]
            .spacing(10)
            .align_y(Alignment::Center);
            group = group.push(
                Row::new(t, c.t(tr::Key::Brightness))
                    .subtitle(c.t(tr::Key::BrightnessHint))
                    .suffix(widgets::flexible(control, 320.0))
                    .build(),
            );
        }
        group.build()
    }

    fn details_group<'a>(&'a self, c: &Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let t = c.theme;
        let Some(output) = self.state.outputs.get(self.selected) else {
            return widgets::gap(0.0);
        };
        let (w, h) = self.logical(config, self.selected);
        let mut rows = vec![(c.t(tr::Key::Connector).to_string(), output.name.clone())];
        let model = format!("{} {}", output.make, output.model).trim().to_string();
        if !model.is_empty() && model != "Unknown Unknown" {
            rows.push((c.t(tr::Key::Model).to_string(), model));
        }
        if let Some(inches) = display::diagonal_inches(output.physical_mm) {
            rows.push((
                c.t(tr::Key::ScreenSize).to_string(),
                format!("{inches}″ · {} × {} mm", output.physical_mm.0, output.physical_mm.1),
            ));
        }
        rows.push((c.t(tr::Key::Workspace).to_string(), format!("{w:.0} × {h:.0}")));
        if self.state.live {
            rows.push((c.t(tr::Key::Position).to_string(), format!("{}, {}", output.x, output.y)));
        }
        column![
            container(text(c.t(tr::Key::Details)).size(14).font(theme::SEMIBOLD).color(t.fg_1()))
                .padding(iced::Padding::ZERO.left(4)),
            widgets::info_grid(t, rows),
        ]
        .spacing(8)
        .into()
    }
}

fn monitor_tile<'a>(
    t: &theme::ThemeState, output: &OutputInfo, index: usize, scale: f64, (width, height): (f32, f32), selected: bool,
) -> Element<'a, Msg> {
    let t = *t;
    let (name_color, scale_color) =
        if selected { (t.acc_fg(), iced::Color { a: 0.8, ..t.acc_fg() }) } else { (t.fg_1(), t.fg_2()) };
    let label = column![
        text(output.name.clone()).size(12).font(theme::SEMIBOLD).color(name_color),
        text(format!("{} · {}", resolution_text(output.width, output.height), display::scale_label(scale)))
            .size(11)
            .color(scale_color),
    ]
    .spacing(2)
    .align_x(Alignment::Center);
    column![
        button(
            container(label)
                .center_x(Length::Fixed(width.max(72.0)))
                .center_y(Length::Fixed(height.max(44.0)))
                .style(move |_: &iced::Theme| theme::monitor_style(&t, selected)),
        )
        .padding(0)
        .on_press(Msg::Select(index))
        .style(move |_: &iced::Theme, status| theme::plain_button_style(&t, status)),
        container(iced::widget::space())
            .width(Length::Fixed((width * 0.28).max(18.0)))
            .height(Length::Fixed(4.0))
            .style(move |_: &iced::Theme| theme::solid(t.fill_3(), theme::radius_r1())),
    ]
    .spacing(3)
    .align_x(Alignment::Center)
    .into()
}

fn fixed_mode_rows<'a>(c: &Ctx, output: &OutputInfo) -> Vec<Element<'a, Msg>> {
    let t = c.theme;
    let mut rows = vec![
        Row::new(t, c.t(K::Resolution))
            .subtitle(c.t(K::ResolutionHint))
            .suffix(text(resolution_label(output)).size(13).color(t.fg_2()))
            .build(),
    ];
    if output.refresh_mhz > 0 {
        rows.push(
            Row::new(t, c.t(K::RefreshRate))
                .suffix(text(format!("{} Hz", display::refresh_text(output.refresh_mhz))).size(13).color(t.fg_2()))
                .build(),
        );
    }
    rows
}

fn refresh_row<'a>(c: &Ctx, output: &OutputInfo, configured: Option<&str>) -> Option<Element<'a, Msg>> {
    let t = c.theme;
    let rates = display::refresh_rates(&output.modes, (output.width, output.height));
    if rates.is_empty() {
        return None;
    }
    let base = format!("{}x{}", output.width, output.height);
    let mut choices = vec![ModeChoice { key: Some(base.clone()), label: c.t(tr::Key::Automatic).to_string() }];
    choices.extend(rates.iter().map(|rate| ModeChoice {
        key: Some(format!("{base}@{}", display::refresh_text(*rate))),
        label: format!("{} Hz", display::refresh_text(*rate)),
    }));
    let current = format!("{base}@{}", display::refresh_text(output.refresh_mhz));
    let selected = match configured {
        Some(mode) if mode.contains('@') => choices.iter().find(|choice| choice.key.as_deref() == Some(mode)).cloned(),
        Some(_) => choices.first().cloned(),
        None => choices.iter().skip(1).find(|choice| choice.key.as_deref() == Some(current.as_str())).cloned(),
    };
    Some(
        Row::new(t, c.t(K::RefreshRate))
            .subtitle(if output.refresh_mhz > 0 {
                format!("{}: {} Hz", c.t(tr::Key::Current), display::refresh_text(output.refresh_mhz))
            } else {
                c.t(tr::Key::FastestHint).to_string()
            })
            .suffix(widgets::inert(t, c.writable, widgets::dropdown(t, choices, selected, Msg::Mode, 200.0)))
            .build(),
    )
}

fn resolution_text(width: i32, height: i32) -> String {
    if width > 0 { format!("{width} × {height}") } else { String::from("—") }
}

fn resolution_label(output: &OutputInfo) -> String {
    match display::aspect_label(output.width, output.height) {
        Some(aspect) if output.width > 0 => {
            format!("{} ({aspect})", resolution_text(output.width, output.height))
        }
        _ => resolution_text(output.width, output.height),
    }
}

#[must_use]
pub fn scale_id(scale: f64) -> u32 {
    (scale * 100.0).round().max(0.0) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names() -> Vec<String> {
        vec!["DP-1".to_string(), "eDP-1".to_string()]
    }

    fn change(guard: &mut Guard, config: &mut ConfigStore, edit: impl FnOnce(&mut ConfigStore)) {
        let before = Guard::capture(config, &names());
        edit(config);
        assert!(Guard::changed(&before, config));
        guard.arm(before);
    }

    #[test]
    fn a_written_output_is_tagged_with_its_monitor_once_known() {
        let mut config = ConfigStore::parse("");
        config.tag_output("DP-1", "Dell", "U2720Q");
        assert_eq!(config.to_text(), "", "only an existing section is tagged");
        config.set_output_scale("DP-1", 1.5);
        config.tag_output("DP-1", "Unknown", "Unknown");
        assert!(!config.to_text().contains("make"), "an unreadable EDID is not written");
        config.tag_output("DP-1", "Dell", "U2720Q");
        let text = config.to_text();
        assert!(text.contains("make = \"Dell\"") && text.contains("model = \"U2720Q\""), "{text}");
    }

    #[test]
    fn revert_removes_an_output_table_that_did_not_exist() {
        let original = "[general]\nlanguage = \"en\"\n";
        let mut config = ConfigStore::parse(original);
        let mut guard = Guard::default();
        change(&mut guard, &mut config, |config| {
            config.set_output_mode("DP-1", Some("1920x1080@60"));
            config.set_output_scale("DP-1", 1.5);
        });
        guard.revert(&mut config);
        assert_eq!(config.to_text(), original);
        assert_eq!(guard.remaining(), None);
    }

    #[test]
    fn revert_restores_values_and_drops_keys_that_were_absent() {
        let original =
            "[[output]]\nname = \"DP-1\"\nmode = \"2560x1440\"\n\n[[output]]\nname = \"eDP-1\"\nscale = 2.0\n";
        let mut config = ConfigStore::parse(original);
        let mut guard = Guard::default();
        change(&mut guard, &mut config, |config| {
            config.set_output_mode("DP-1", Some("3840x2160@144"));
            config.set_output_scale("DP-1", 1.25);
            config.set_output_positions(&[("eDP-1".to_string(), 0, 0), ("DP-1".to_string(), 1280, 0)]);
        });
        guard.revert(&mut config);
        assert_eq!(config.to_text(), original);
        assert_eq!(config.output_scale("DP-1"), None);
        assert_eq!(config.output_position("DP-1"), None);
    }

    #[test]
    fn a_second_change_keeps_the_first_snapshot() {
        let original = "[[output]]\nname = \"DP-1\"\nscale = 1.0\n";
        let mut config = ConfigStore::parse(original);
        let mut guard = Guard::default();
        change(&mut guard, &mut config, |config| {
            config.set_output_scale("DP-1", 1.5);
        });
        change(&mut guard, &mut config, |config| {
            config.set_output_mode("DP-1", Some("1280x720"));
        });
        guard.revert(&mut config);
        assert_eq!(config.to_text(), original);
    }

    #[test]
    fn countdown_expires_after_the_confirm_window() {
        let mut config = ConfigStore::parse("");
        let mut guard = Guard::default();
        let before = Guard::capture(&config, &names());
        config.set_output_scale("DP-1", 2.0);
        let generation = guard.arm(before);
        assert_eq!(guard.remaining(), Some(CONFIRM_SECONDS));
        for _ in 1..CONFIRM_SECONDS {
            assert_eq!(guard.tick(generation), Countdown::Running);
        }
        assert_eq!(guard.tick(generation), Countdown::Expired);
        guard.revert(&mut config);
        assert_eq!(config.to_text(), "");
    }

    #[test]
    fn rearming_restarts_the_countdown_and_stales_old_ticks() {
        let mut config = ConfigStore::parse("");
        let mut guard = Guard::default();
        let first = guard.arm(Guard::capture(&config, &names()));
        assert_eq!(guard.tick(first), Countdown::Running);
        config.set_output_scale("DP-1", 2.0);
        let second = guard.arm(Guard::capture(&config, &names()));
        assert_eq!(guard.tick(first), Countdown::Stale);
        assert_eq!(guard.remaining(), Some(CONFIRM_SECONDS));
        assert_eq!(guard.tick(second), Countdown::Running);
    }

    #[test]
    fn keeping_clears_the_prompt_and_leaves_the_config() {
        let mut config = ConfigStore::parse("");
        let mut guard = Guard::default();
        let before = Guard::capture(&config, &names());
        config.set_output_scale("DP-1", 2.0);
        let generation = guard.arm(before);
        let applied = config.to_text();
        guard.keep();
        assert_eq!(guard.remaining(), None);
        assert_eq!(guard.tick(generation), Countdown::Stale);
        guard.revert(&mut config);
        assert_eq!(config.to_text(), applied);
    }

    #[test]
    fn an_unchanged_value_is_not_a_change() {
        let mut config = ConfigStore::parse("[[output]]\nname = \"DP-1\"\nscale = 1.5\n");
        let before = Guard::capture(&config, &names());
        config.set_output_scale("DP-1", 1.5);
        assert!(!Guard::changed(&before, &config));
    }
}

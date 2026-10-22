use iced::widget::{Row as IcedRow, column, container, row, space, text};
use iced::{Alignment, Color, Element, Fill, Length, Task};

use super::common::{error_banner, gated, toggle};
use super::{Ctx, PageEvent, Pane, Panel};
use crate::i18n::Key as K;
use crate::icons::Icon;
use crate::services::config::{ConfigStore, DEFAULT_NIGHT_LIGHT_KELVIN, parse_clock};
use crate::services::display::{self, NIGHT_MAX_KELVIN, NIGHT_MIN_KELVIN};
use crate::services::run::{Debounce, blocking};
use crate::theme;
use crate::widgets::{self, Group, Row};

mod tr {
    crate::i18n::catalogue! {
        On => "On", "Увімкнено";
        Off => "Off", "Вимкнено";
        HeroOn => "The screen is tinted warmer to ease your eyes in the evening", "Екран має тепліший відтінок, щоб очам було легше ввечері";
        HeroOff => "Turn it on to take the blue out of the screen", "Увімкніть, щоб прибрати з екрана синій відтінок";
        HeroOffline => "Glide is not running, so night light can't be switched from here", "Glide не запущено, тож звідси нічне світло не ввімкнути";
        Temperature => "Colour temperature", "Колірна температура";
        TemperatureHint => "Lower is warmer. Applies whenever night light is on.", "Що нижче значення, то тепліше. Діє, коли нічне світло ввімкнено.";
        Warmer => "Warmer", "Тепліше";
        Cooler => "Cooler", "Холодніше";
        Day => "Day", "День";
        Night => "Night", "Ніч";
        Schedule => "Schedule", "Розклад";
        ScheduleHint => "Switching night light by hand keeps it that way until the next scheduled change", "Якщо перемкнути нічне світло вручну, воно так і залишиться до наступної зміни за розкладом";
        Presets => "Quick presets", "Швидкі варіанти";
        Timeline => "Active hours", "Години роботи";
        AllNight => "20:00 – 07:00", "20:00 – 07:00";
        LateNight => "22:00 – 06:00", "22:00 – 06:00";
        Evening => "18:00 – 08:00", "18:00 – 08:00";
        NoWindow => "Start and end are the same, so the schedule never turns it on", "Початок і кінець збігаються, тож за розкладом воно не ввімкнеться";
    }
}

pub const PRESETS: &[(&str, &str)] = &[("20:00", "07:00"), ("22:00", "06:00"), ("18:00", "08:00")];

#[derive(Debug, Clone)]
pub enum Msg {
    Loaded(Option<bool>),
    Toggle(bool),
    Toggled(Option<Result<bool, String>>),
    Temperature(f64),
    CommitTemperature,
    SettleTemperature(u64),
    ResetTemperature,
    NightSchedule(bool),
    NightFrom(String),
    NightTo(String),
    CommitNight,
    Preset(usize),
}

#[derive(Debug, Default)]
pub struct Page {
    live: Option<bool>,
    loading: bool,
    busy: bool,
    error: Option<String>,
    temperature: Option<f64>,
    settle: Debounce,
    night_from: Option<String>,
    night_to: Option<String>,
}

impl Pane for Page {
    type Msg = Msg;

    const KEYWORDS: &'static [&'static str] = &[
        "night light",
        "night",
        "warm",
        "blue light",
        "colour temperature",
        "color temperature",
        "kelvin",
        "нічне світло",
        "ніч",
        "тепл",
        "синє світло",
        "температура",
    ];
    const POLLS: bool = true;

    fn open(&mut self, _c: Ctx, _config: &ConfigStore) -> Task<Msg> {
        self.load()
    }

    fn update(&mut self, msg: Msg, _c: Ctx, config: &mut ConfigStore) -> Task<Msg> {
        match msg {
            Msg::Loaded(state) => {
                self.loading = false;
                if !self.busy {
                    self.live = state;
                }
            }
            Msg::Toggle(on) => {
                self.busy = true;
                self.live = Some(on);
                return Task::perform(blocking(move || display::night_light(Some(on))), Msg::Toggled);
            }
            Msg::Toggled(result) => {
                self.busy = false;
                match result {
                    Some(Ok(on)) => {
                        self.live = Some(on);
                        self.error = None;
                    }
                    Some(Err(err)) => self.error = Some(err),
                    None => {}
                }
            }
            Msg::Temperature(kelvin) => {
                self.temperature = Some(kelvin);
                return self.settle.schedule(Msg::SettleTemperature);
            }
            Msg::SettleTemperature(generation) if !self.settle.is_current(generation) => {}
            Msg::CommitTemperature | Msg::SettleTemperature(_) => {
                self.settle.cancel();
                if let Some(kelvin) = self.temperature.take() {
                    let kelvin = (kelvin / 100.0).round() as i64 * 100;
                    config.set_int("night-light", "temperature", kelvin.clamp(NIGHT_MIN_KELVIN, NIGHT_MAX_KELVIN));
                }
            }
            Msg::ResetTemperature => {
                self.settle.cancel();
                self.temperature = None;
                config.remove("night-light", "temperature");
            }
            Msg::NightSchedule(on) => config.set_bool("night-light", "schedule", on),
            Msg::NightFrom(value) => {
                self.night_from = Some(value);
                self.commit_night(config);
            }
            Msg::NightTo(value) => {
                self.night_to = Some(value);
                self.commit_night(config);
            }
            Msg::CommitNight => self.commit_night(config),
            Msg::Preset(index) => {
                if let Some((from, to)) = PRESETS.get(index) {
                    self.night_from = None;
                    self.night_to = None;
                    config.set_bool("night-light", "schedule", true);
                    config.set_str("night-light", "from", from);
                    config.set_str("night-light", "to", to);
                }
            }
        }
        Task::none()
    }

    fn view<'a>(&'a self, c: Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        column![
            self.hero(&c),
            error_banner(c.theme, self.error.as_deref()),
            self.temperature_group(&c, config),
            self.schedule_group(&c, config),
        ]
        .spacing(theme::SECTION_GAP)
        .into()
    }

    fn tick(&mut self, _c: Ctx) -> Task<Msg> {
        self.load()
    }

    fn event(&mut self, event: PageEvent, _c: Ctx) -> Task<Msg> {
        match event {
            PageEvent::Reloaded => self.load(),
            PageEvent::OutputsChanged => Task::none(),
        }
    }
}

impl Page {
    fn load(&mut self) -> Task<Msg> {
        if self.loading || self.busy {
            return Task::none();
        }
        self.loading = true;
        Task::perform(blocking(|| display::night_light(None).ok()), |state| Msg::Loaded(state.flatten()))
    }

    fn commit_night(&mut self, config: &mut ConfigStore) {
        for (draft, key) in [(&mut self.night_from, "from"), (&mut self.night_to, "to")] {
            if let Some(value) = draft
                && parse_clock(value).is_some()
            {
                config.set_str("night-light", key, value.trim());
                *draft = None;
            }
        }
    }

    fn kelvin(&self, config: &ConfigStore) -> i64 {
        self.temperature
            .map_or_else(|| config.night_light_temperature(), |k| k.round() as i64)
            .clamp(NIGHT_MIN_KELVIN, NIGHT_MAX_KELVIN)
    }

    fn hero(&self, c: &Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        let (heading, description) = match self.live {
            Some(on) => {
                let (state, hint) = if on { (tr::Key::On, tr::Key::HeroOn) } else { (tr::Key::Off, tr::Key::HeroOff) };
                (format!("{} · {}", c.t(K::NightLight), c.t(state)), c.t(hint))
            }
            None => (c.t(K::NightLight).to_string(), c.t(tr::Key::HeroOffline)),
        };
        let trailing = match self.live {
            Some(on) if !self.busy => widgets::switch(t, on, Msg::Toggle),
            live => widgets::switch_disabled(t, live == Some(true)),
        };
        widgets::section_hero(t, Icon::Moon, Panel::NightLight.color(), heading, description, Some(trailing))
    }

    fn temperature_group<'a>(&'a self, c: &Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let t = c.theme;
        let kelvin = self.kelvin(config);
        let tint = display::night_tint(kelvin);
        let preview = row![
            preview_window(&t, (1.0, 1.0, 1.0), c.t(tr::Key::Day)),
            widgets::icon(Icon::ChevronRight, 18.0, t.fg_3()),
            preview_window(&t, tint, c.t(tr::Key::Night)),
        ]
        .spacing(16)
        .align_y(Alignment::Center);
        let warm = tinted(display::night_tint(NIGHT_MIN_KELVIN), 1.0);
        let cool = tinted(display::night_tint(NIGHT_MAX_KELVIN), 1.0);
        let strip = container(space()).width(Fill).height(Length::Fixed(8.0)).style(move |_: &iced::Theme| {
            iced::widget::container::Style {
                background: Some(iced::Background::Gradient(iced::Gradient::Linear(
                    iced::gradient::Linear::new(std::f32::consts::FRAC_PI_2).add_stop(0.0, warm).add_stop(1.0, cool),
                ))),
                border: iced::Border { color: t.line(), width: 1.0, radius: theme::radius_round() },
                ..iced::widget::container::Style::default()
            }
        });
        let slider = column![
            strip,
            widgets::value_slider(
                t,
                NIGHT_MIN_KELVIN as f64..=NIGHT_MAX_KELVIN as f64,
                kelvin as f64,
                100.0,
                Msg::Temperature,
                Some(Msg::CommitTemperature),
            ),
            row![
                text(c.t(tr::Key::Warmer)).size(12).color(t.fg_3()).width(Fill),
                text(c.t(tr::Key::Cooler)).size(12).color(t.fg_3()),
            ],
        ]
        .spacing(6);
        let card = container(column![container(preview).center_x(Fill), slider].spacing(18))
            .padding(18)
            .width(Fill)
            .style(move |_: &iced::Theme| theme::card_style(&t));
        let reset = widgets::secondary(
            t,
            c.t(K::Reset),
            (c.writable && kelvin != DEFAULT_NIGHT_LIGHT_KELVIN).then_some(Msg::ResetTemperature),
        );
        column![
            Group::new(t)
                .title(format!("{} · {kelvin} K", c.t(tr::Key::Temperature)))
                .description(c.t(tr::Key::TemperatureHint))
                .suffix(reset)
                .build(),
            gated(*c, card),
        ]
        .spacing(8)
        .into()
    }

    fn schedule_group<'a>(&'a self, c: &Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let t = c.theme;
        let scheduled = config.night_light_schedule();
        let from = self.night_from.clone().unwrap_or_else(|| config.night_light_from());
        let to = self.night_to.clone().unwrap_or_else(|| config.night_light_to());
        let mut group = Group::new(t).title(c.t(tr::Key::Schedule)).description(c.t(tr::Key::ScheduleHint)).push(
            Row::new(t, c.t(K::NightSchedule))
                .subtitle(c.t(K::NightScheduleHint))
                .prefix(widgets::tile(
                    Icon::Clock,
                    theme::ramp_color(theme::Accent::Amber, 3),
                    theme::SMALL_CONTROL_HEIGHT,
                ))
                .suffix(toggle(*c, scheduled, Msg::NightSchedule))
                .build(),
        );
        if scheduled {
            let time_row = |label: K, value: &str, on_input: fn(String) -> Msg| {
                let invalid = parse_clock(value).is_none();
                let field = widgets::field(t, "HH:MM", value, on_input, Some(Msg::CommitNight), invalid)
                    .width(Length::Fixed(92.0));
                Row::new(t, c.t(label))
                    .subtitle(if invalid { c.t(K::TimeInvalid) } else { "" })
                    .suffix(gated(*c, field))
                    .build()
            };
            group = group.push(time_row(K::From, &from, Msg::NightFrom)).push(time_row(K::To, &to, Msg::NightTo));
            let window = parse_clock(&from).zip(parse_clock(&to));
            if let Some((start, end)) = window {
                group = group.push(timeline(c, start, end, self.kelvin(config)));
            }
            let labels = [tr::Key::AllNight, tr::Key::LateNight, tr::Key::Evening];
            let mut chips = row![].spacing(6).align_y(Alignment::Center);
            for (index, (preset_from, preset_to)) in PRESETS.iter().enumerate() {
                let active = from.trim() == *preset_from && to.trim() == *preset_to;
                chips = chips.push(widgets::chip(
                    t,
                    c.t(labels[index]),
                    active,
                    (c.writable && !active).then_some(Msg::Preset(index)),
                ));
            }
            group = group.push(Row::new(t, c.t(tr::Key::Presets)).suffix(chips).build());
        }
        group.build()
    }
}

fn tinted((r, g, b): (f32, f32, f32), level: f32) -> Color {
    Color::from_rgb(r * level, g * level, b * level)
}

fn preview_window<'a>(t: &theme::ThemeState, tint: (f32, f32, f32), label: &'a str) -> Element<'a, Msg> {
    let t = *t;
    let paper = tinted(tint, 0.97);
    let bar = tinted(tint, 0.86);
    let ink = tinted(tint, 0.62);
    let accent = {
        let base = theme::ramp_color(theme::Accent::Azurit, 3);
        Color::from_rgb(base.r * tint.0, base.g * tint.1, base.b * tint.2)
    };
    let line = move |width: f32, color: Color| {
        container(space())
            .width(Length::Fixed(width))
            .height(Length::Fixed(5.0))
            .style(move |_: &iced::Theme| theme::solid(color, theme::radius_round()))
    };
    let window = container(
        column![
            container(space())
                .width(Fill)
                .height(Length::Fixed(14.0))
                .style(move |_: &iced::Theme| theme::solid(bar, theme::radius_r1())),
            column![line(70.0, ink), line(96.0, ink), line(52.0, accent)].spacing(6).padding([4, 10]),
        ]
        .spacing(6),
    )
    .width(Length::Fixed(140.0))
    .height(Length::Fixed(86.0))
    .style(move |_: &iced::Theme| iced::widget::container::Style {
        background: Some(paper.into()),
        border: iced::Border { color: t.line_2(), width: 1.0, radius: theme::radius_r2() },
        shadow: theme::elevation(&t, glide_theme::elevation::Level::El1),
        ..iced::widget::container::Style::default()
    });
    column![window, text(label).size(12).color(t.fg_2())].spacing(6).align_x(Alignment::Center).into()
}

fn timeline<'a>(c: &Ctx, start: u16, end: u16, kelvin: i64) -> Element<'a, Msg> {
    let t = c.theme;
    let segments = display::schedule_segments(start, end);
    let on = tinted(display::night_tint(kelvin), 0.95);
    let on = Color { r: on.r * 0.92, g: on.g * 0.72, b: on.b * 0.4, a: 1.0 };
    let off = t.fill_2();
    let mut bar = IcedRow::new().height(Length::Fixed(10.0)).width(Fill);
    let mut cursor = 0u16;
    let push = |bar: IcedRow<'a, Msg>, from: u16, to: u16, color: Color| {
        if to <= from {
            return bar;
        }
        bar.push(
            container(space())
                .width(Length::FillPortion(to - from))
                .height(Fill)
                .style(move |_: &iced::Theme| theme::solid(color, theme::radius_r1())),
        )
    };
    for (from, to) in &segments {
        bar = push(bar, cursor, *from, off);
        bar = push(bar, *from, *to, on);
        cursor = *to;
    }
    bar = push(bar, cursor, 24 * 60, off);
    let mut ticks = row![].width(Fill);
    for (index, label) in ["00", "06", "12", "18", "24"].iter().enumerate() {
        let tick = text(format!("{label}:00")).size(11).color(t.fg_3());
        ticks = ticks.push(if index == 0 {
            container(tick).width(Fill)
        } else if index == 4 {
            container(tick).align_right(Length::Shrink)
        } else {
            container(tick).center_x(Fill)
        });
    }
    let mut body = column![bar, ticks].spacing(4);
    if segments.is_empty() {
        body = body.push(text(c.t(tr::Key::NoWindow)).size(12).color(t.warn()));
    }
    container(column![text(c.t(tr::Key::Timeline)).size(14).color(t.fg_1()), body,].spacing(8))
        .padding([10.0, theme::ROW_PADDING_X])
        .width(Fill)
        .into()
}

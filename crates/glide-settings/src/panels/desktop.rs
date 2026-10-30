use std::collections::HashMap;
use std::path::PathBuf;

use glide_sdk::widget::mock as sketch;
use iced::widget::{Column, container, row};
use iced::{Alignment, Element, Length, Task};

use super::apps::app_icon;
use super::common::{gated, switch_row};
use super::{Ctx, Pane};
use crate::i18n::{Key as K, Lang};
use crate::icons::Icon;
use crate::services::apps::{self, DesktopApp};
use crate::services::config::{ConfigStore, DEFAULT_ITEM_SIZE, icon_size_for};
use crate::services::dock::{self, Pin};
use crate::services::ipc;
use crate::services::run::{Debounce, Generation, blocking};
use crate::theme::{self, ThemeState};
use crate::widgets::{self, Group, Row};

mod tr {
    crate::i18n::catalogue! {
        Preview => "Preview", "Попередній перегляд";
        PreviewHint => "Hover magnification and size as they will look", "Так виглядатимуть розмір і збільшення під вказівником";
        BehaviourHint => "When the dock gets out of the way", "Коли док поступається місцем вікнам";
        SizeAndMotion => "Size and motion", "Розмір і рух";
        SizeHint => "Tiles and icons together", "Плитки й іконки разом";
        Magnification => "Magnification", "Збільшення";
        MagnificationHint => "How much an icon grows under the pointer", "Наскільки збільшується іконка під вказівником";
        Lift => "Lift on hover", "Підйом під вказівником";
        LiftHint => "How far an icon rises under the pointer", "Наскільки піднімається іконка під вказівником";
        Margin => "Distance from the edge", "Відступ від краю";
        MarginHint => "Gap between the dock and the bottom of the screen", "Проміжок між доком і нижнім краєм екрана";
        Off => "Off", "Вимкнено";
        Pinned => "Pinned apps", "Закріплені програми";
        PinnedHint => "Always in the dock, in this order. Running apps appear after them.", "Завжди в доку в цьому порядку. Запущені програми з'являються після них.";
        NoPins => "Nothing is pinned", "Нічого не закріплено";
        NoPinsHint => "Add an app below or right-click one in the dock", "Додайте програму нижче або клацніть правою кнопкою на програмі в доку";
        NoPinsFile => "Glide writes the list the first time it starts", "Glide створить список під час першого запуску";
        AddPin => "Pin an app", "Закріпити програму";
        PinsFailed => "Could not update pins.toml:", "Не вдалося оновити pins.toml:";
        DockHidden => "The dock is turned off", "Док вимкнено";
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewWindow {
    NewWorkspace,
    ActiveWorkspace,
}

impl NewWindow {
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::NewWorkspace => "new-workspace",
            Self::ActiveWorkspace => "active-workspace",
        }
    }

    #[must_use]
    pub fn from_config(config: &ConfigStore) -> Self {
        match config.get_str("workspaces", "new-window") {
            Some("active-workspace") => Self::ActiveWorkspace,
            _ => Self::NewWorkspace,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Autohide {
    Smart,
    Always,
    Never,
}

impl Autohide {
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Smart => "smart",
            Self::Always => "always",
            Self::Never => "never",
        }
    }

    #[must_use]
    pub fn from_config(config: &ConfigStore) -> Self {
        match config.get_str("dock", "autohide") {
            Some("always") => Self::Always,
            Some("never") => Self::Never,
            _ => Self::Smart,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Knob {
    Size,
    Magnification,
    Lift,
    Margin,
}

pub(crate) const DEFAULT_HOVER_SCALE: f64 = 1.06;
pub(crate) const DEFAULT_HOVER_LIFT: i64 = 4;
pub(crate) const DEFAULT_MARGIN: i64 = 12;

impl Knob {
    #[must_use]
    pub const fn range(self) -> (f64, f64, f64) {
        match self {
            Self::Size => (40.0, 80.0, 2.0),
            Self::Magnification => (1.0, 1.5, 0.02),
            Self::Lift => (0.0, 12.0, 1.0),
            Self::Margin => (0.0, 32.0, 2.0),
        }
    }

    #[must_use]
    pub fn current(self, config: &ConfigStore) -> f64 {
        match self {
            Self::Size => config.int_or("dock", "item-size", DEFAULT_ITEM_SIZE) as f64,
            Self::Magnification => config.get_float("dock", "hover-scale").unwrap_or(DEFAULT_HOVER_SCALE),
            Self::Lift => config.int_or("dock", "hover-lift", DEFAULT_HOVER_LIFT) as f64,
            Self::Margin => config.int_or("dock", "bottom-margin", DEFAULT_MARGIN) as f64,
        }
    }

    fn write(self, config: &mut ConfigStore, value: f64) {
        let (min, max, _) = self.range();
        let value = value.clamp(min, max);
        match self {
            Self::Size => {
                let size = value.round() as i64;
                config.set_int("dock", "item-size", size);
                config.set_int("dock", "icon-size", icon_size_for(size));
            }
            Self::Magnification => config.set_float("dock", "hover-scale", value),
            Self::Lift => config.set_int("dock", "hover-lift", value.round() as i64),
            Self::Margin => config.set_int("dock", "bottom-margin", value.round() as i64),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Msg {
    Dock(bool),
    Autohide(Autohide),
    Slide(Knob, f64),
    Commit,
    Settle(u64),
    Pins(u64, Result<Option<Vec<Pin>>, String>),
    Apps(Vec<DesktopApp>, HashMap<String, PathBuf>),
    MovePin(usize, bool),
    Unpin(usize),
    AddPin(DesktopApp),
    PinsSaved(Result<(), String>),
}

#[derive(Debug, Default)]
pub struct Page {
    pending: Option<(Knob, f64)>,
    settle: Debounce,
    pins: Option<Vec<Pin>>,
    pins_error: Option<String>,
    pins_loaded: bool,
    generation: Generation,
    apps_key: Option<(Lang, Option<String>)>,
    apps: Vec<DesktopApp>,
    icons: HashMap<String, PathBuf>,
}

impl Pane for Page {
    type Msg = Msg;

    const KEYWORDS: &'static [&'static str] = &[
        "dock",
        "pin",
        "pinned",
        "autohide",
        "magnification",
        "icon size",
        "док",
        "закріп",
        "ховати",
        "збільшення",
        "розмір іконок",
    ];
    const POLLS: bool = true;

    fn open(&mut self, c: Ctx, config: &ConfigStore) -> Task<Msg> {
        let lang = c.lang.key().to_string();
        let theme = Some(config.icon_theme());
        let key = (c.lang, theme.clone());
        let apps = if self.apps_key.as_ref() == Some(&key) {
            Task::none()
        } else {
            self.apps_key = Some(key);
            Task::perform(
                blocking(move || {
                    let apps = apps::visible(&apps::installed(&lang));
                    let icons = apps::icon_paths(apps.iter().map(|app| app.icon.as_str()), theme.as_deref());
                    (apps, icons)
                }),
                |loaded| {
                    let (apps, icons) = loaded.unwrap_or_default();
                    Msg::Apps(apps, icons)
                },
            )
        };
        Task::batch([self.load_pins(), apps])
    }

    fn tick(&mut self, _c: Ctx) -> Task<Msg> {
        self.load_pins()
    }

    fn update(&mut self, msg: Msg, _c: Ctx, config: &mut ConfigStore) -> Task<Msg> {
        match msg {
            Msg::Dock(on) => config.set_bool("dock", "enabled", on),
            Msg::Autohide(mode) => config.set_str("dock", "autohide", mode.key()),
            Msg::Slide(knob, value) => {
                if self.pending.is_some_and(|(pending, _)| pending != knob) {
                    self.flush(config);
                }
                self.pending = Some((knob, value));
                return self.settle.schedule(Msg::Settle);
            }
            Msg::Settle(generation) if !self.settle.is_current(generation) => {}
            Msg::Commit | Msg::Settle(_) => self.flush(config),
            Msg::Pins(generation, result) => {
                if !self.generation.is_current(generation) {
                    return Task::none();
                }
                self.pins_loaded = true;
                match result {
                    Ok(pins) => {
                        self.pins = pins;
                        self.pins_error = None;
                    }
                    Err(err) => {
                        self.pins = None;
                        self.pins_error = Some(err);
                    }
                }
            }
            Msg::Apps(apps, icons) => {
                self.apps = apps;
                self.icons = icons;
            }
            Msg::MovePin(index, up) => {
                if let Some(pins) = &self.pins {
                    let next = dock::moved(pins, index, up);
                    if &next != pins {
                        return self.save_pins(next);
                    }
                }
            }
            Msg::Unpin(index) => {
                if let Some(pins) = &self.pins
                    && index < pins.len()
                {
                    let mut next = pins.clone();
                    next.remove(index);
                    return self.save_pins(next);
                }
            }
            Msg::AddPin(app) => {
                let app_id = app.id.trim_end_matches(".desktop").to_string();
                let mut next = self.pins.clone().unwrap_or_default();
                if !next.iter().any(|pin| dock::same_app(&pin.app_id, &app_id)) {
                    next.push(Pin { app_id, name: app.name, command: apps::strip_field_codes(&app.exec) });
                    return self.save_pins(next);
                }
            }
            Msg::PinsSaved(result) => self.pins_error = result.err(),
        }
        Task::none()
    }

    fn view<'a>(&'a self, c: Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let dock_on = config.bool_or("dock", "enabled", true);
        let mut page = Column::new().spacing(theme::SECTION_GAP).push(self.preview_group(&c, config, dock_on));
        if dock_on {
            page = page.push(Self::behaviour_group(&c, config)).push(self.size_group(&c, config));
        }
        page.push(self.pins_group(&c)).into()
    }
}

impl Page {
    fn icon_of(&self, pin: &Pin) -> Option<&PathBuf> {
        let app = self.apps.iter().find(|app| dock::same_app(&pin.app_id, app.id.trim_end_matches(".desktop")))?;
        self.icons.get(&app.icon)
    }

    fn load_pins(&mut self) -> Task<Msg> {
        let generation = self.generation.current();
        Task::perform(blocking(dock::load), move |pins| {
            Msg::Pins(generation, pins.unwrap_or_else(|| Err(String::from("cancelled"))))
        })
    }

    fn save_pins(&mut self, pins: Vec<Pin>) -> Task<Msg> {
        static ORDER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        static WRITTEN: std::sync::Mutex<u64> = std::sync::Mutex::new(0);
        self.generation.bump();
        self.pins = Some(pins.clone());
        let order = ORDER.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        Task::perform(
            blocking(move || {
                let mut written = WRITTEN.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                if order < *written {
                    return Ok(());
                }
                *written = order;
                dock::save(&pins)?;
                let _ = ipc::reload();
                Ok(())
            }),
            |result| Msg::PinsSaved(result.unwrap_or_else(|| Err(String::from("cancelled")))),
        )
    }

    fn value(&self, knob: Knob, config: &ConfigStore) -> f64 {
        match self.pending {
            Some((pending, value)) if pending == knob => value,
            _ => knob.current(config),
        }
    }

    fn flush(&mut self, config: &mut ConfigStore) {
        self.settle.cancel();
        if let Some((knob, value)) = self.pending.take() {
            knob.write(config, value);
        }
    }

    fn preview_group<'a>(&'a self, c: &Ctx, config: &'a ConfigStore, on: bool) -> Element<'a, Msg> {
        let t = c.theme;
        let size = self.value(Knob::Size, config);
        let scale = self.value(Knob::Magnification, config);
        let lift = self.value(Knob::Lift, config);
        let margin = self.value(Knob::Margin, config);
        let pins: Vec<(&str, Option<&PathBuf>)> =
            self.pins.iter().flatten().map(|pin| (pin.name.as_str(), self.icon_of(pin))).collect();
        let illustration: Element<'a, Msg> = if on {
            dock_preview(&t, &pins, size, scale, lift, margin)
        } else {
            sketch::screen(t, Length::Fill, PREVIEW_HEIGHT, vec![sketch::caption(t, c.t(tr::Key::DockHidden))])
        };
        Group::new(t)
            .title(c.t(tr::Key::Preview))
            .description(c.t(tr::Key::PreviewHint))
            .push(container(illustration).padding(8))
            .push(switch_row(*c, c.t(K::ShowDock), c.t(K::ShowDockHint), on, Msg::Dock))
            .build()
    }

    fn behaviour_group<'a>(c: &Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let t = c.theme;
        let current = Autohide::from_config(config);
        let card = |mode: Autohide, heading: K, hint: K| {
            widgets::radio_card(
                t,
                autohide_sketch(&t, mode),
                c.t(heading),
                Some(c.t(hint).to_string()),
                current == mode,
                Msg::Autohide(mode),
            )
        };
        let cards = row![
            card(Autohide::Smart, K::AutohideSmart, K::AutohideSmartHint),
            card(Autohide::Always, K::AutohideAlways, K::AutohideAlwaysHint),
            card(Autohide::Never, K::AutohideNever, K::AutohideNeverHint),
        ]
        .spacing(12);
        Group::new(t)
            .title(c.t(K::AutoHide))
            .description(c.t(tr::Key::BehaviourHint))
            .push(container(gated(*c, cards)).padding(8))
            .build()
    }

    fn slider_row<'a>(&'a self, c: &Ctx, config: &'a ConfigStore, knob: Knob) -> Element<'a, Msg> {
        let t = c.theme;
        let (min, max, step) = knob.range();
        let value = self.value(knob, config);
        let label = match knob {
            Knob::Magnification if value <= 1.001 => c.t(tr::Key::Off).to_string(),
            Knob::Magnification => format!("+{:.0}%", (value - 1.0) * 100.0),
            _ => format!("{value:.0} px"),
        };
        let (title, hint) = match knob {
            Knob::Size => (c.t(K::DockSize), c.t(tr::Key::SizeHint)),
            Knob::Magnification => (c.t(tr::Key::Magnification), c.t(tr::Key::MagnificationHint)),
            Knob::Lift => (c.t(tr::Key::Lift), c.t(tr::Key::LiftHint)),
            Knob::Margin => (c.t(tr::Key::Margin), c.t(tr::Key::MarginHint)),
        };
        Row::new(t, title)
            .subtitle(format!("{hint} · {label}"))
            .suffix(gated(
                *c,
                widgets::flexible(
                    widgets::value_slider(
                        t,
                        min..=max,
                        value.clamp(min, max),
                        step,
                        move |value| Msg::Slide(knob, value),
                        Some(Msg::Commit),
                    ),
                    220.0,
                ),
            ))
            .build()
    }

    fn size_group<'a>(&'a self, c: &Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let knobs = [Knob::Size, Knob::Magnification, Knob::Lift, Knob::Margin];
        Group::new(c.theme)
            .title(c.t(tr::Key::SizeAndMotion))
            .extend(knobs.map(|knob| self.slider_row(c, config, knob)))
            .build()
    }

    fn pins_group<'a>(&'a self, c: &Ctx) -> Element<'a, Msg> {
        let t = c.theme;
        let mut group = Group::new(t).title(c.t(tr::Key::Pinned)).description(c.t(tr::Key::PinnedHint));
        if !self.pins_loaded {
            group = group.suffix(widgets::spinner_label(t, c.t(K::Loading)));
        }
        let pins = self.pins.as_deref().unwrap_or_default();
        if pins.is_empty() && self.pins_loaded {
            group = group.push(widgets::list_empty(
                t,
                Icon::Dock,
                c.t(tr::Key::NoPins),
                if self.pins.is_none() { c.t(tr::Key::NoPinsFile) } else { c.t(tr::Key::NoPinsHint) },
            ));
        }
        let last = pins.len().saturating_sub(1);
        for (index, pin) in pins.iter().enumerate() {
            let controls = row![
                widgets::icon_button(t, Icon::ChevronUp, (index > 0).then_some(Msg::MovePin(index, true))),
                widgets::icon_button(t, Icon::ChevronDown, (index < last).then_some(Msg::MovePin(index, false)),),
                widgets::icon_button(t, Icon::Trash, Some(Msg::Unpin(index))),
            ]
            .spacing(2)
            .align_y(Alignment::Center);
            group = group.push(
                Row::new(t, pin.name.clone())
                    .subtitle(pin.app_id.clone())
                    .prefix(app_icon(&t, self.icon_of(pin), &pin.name, 32.0))
                    .suffix(controls)
                    .build(),
            );
        }
        let available: Vec<DesktopApp> = self
            .apps
            .iter()
            .filter(|app| {
                let id = app.id.trim_end_matches(".desktop");
                !pins.iter().any(|pin| dock::same_app(&pin.app_id, id))
            })
            .cloned()
            .collect();
        if self.pins_loaded && self.pins_error.is_none() {
            group = group.push(
                Row::new(t, c.t(tr::Key::AddPin))
                    .prefix(widgets::icon(Icon::Plus, 18.0, t.fg_2()))
                    .suffix(widgets::chooser(t, available, c.t(K::ChooseApp), Msg::AddPin, 260.0))
                    .build(),
            );
        }
        let mut column = Column::new().spacing(12).push(group.build());
        if let Some(err) = &self.pins_error {
            column = column.push(widgets::banner(
                t,
                crate::theme::Tone::Danger,
                Icon::Warning,
                format!("{} {err}", c.t(tr::Key::PinsFailed)),
                None,
            ));
        }
        column.into()
    }
}

pub(crate) const PREVIEW_HEIGHT: f32 = 150.0;

fn dock_preview<'a>(
    t: &ThemeState, pins: &[(&str, Option<&PathBuf>)], size: f64, scale: f64, lift: f64, margin: f64,
) -> Element<'a, Msg> {
    let unit = (size * 0.62) as f32;
    let placeholder = ["Files", "Terminal", "Web", "Mail", "Music"].map(|name| (name, None));
    let shown = if pins.is_empty() { &placeholder[..] } else { &pins[..pins.len().min(8)] };
    let hovered = shown.len() / 2;
    let tiles = shown
        .iter()
        .enumerate()
        .map(|(index, (name, icon))| {
            let (tile_size, rise) =
                if index == hovered { ((f64::from(unit) * scale) as f32, (lift * 0.62) as f32) } else { (unit, 0.0) };
            (app_icon(t, *icon, name, tile_size.round()), rise)
        })
        .collect();
    sketch::dock_scene(*t, tiles, (margin * 0.62) as f32 + 4.0, PREVIEW_HEIGHT)
}

fn autohide_sketch<'a>(t: &ThemeState, mode: Autohide) -> Element<'a, Msg> {
    let hiding = match mode {
        Autohide::Smart => sketch::DockHiding::WhenCovered,
        Autohide::Always => sketch::DockHiding::Always,
        Autohide::Never => sketch::DockHiding::Never,
    };
    sketch::dock_hiding(*t, hiding)
}

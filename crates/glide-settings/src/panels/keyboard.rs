use glide_sdk::theme::TextStyle;
use glide_sdk::widget::text;
use iced::widget::{column, container, row};
use iced::{Alignment, Element, Length, Task};

use super::common::{gated, switch_row};
use super::{Ctx, Pane, Panel};
use crate::i18n::{Key as K, Lang};
use crate::icons::Icon;
use crate::services::config::{ConfigStore, DEFAULT_REPEAT_DELAY, DEFAULT_REPEAT_RATE};
use crate::services::input::{
    self, BOTH_SHIFTS_CAPS, CAPS_OPTIONS, COMPOSE_OPTIONS, Layout, NUMPAD_DIGITS, SWAP_ALT_SUPER, SWITCH_OPTIONS,
    Source,
};
use crate::services::run::{Debounce, blocking};
use crate::theme;
use crate::widgets::{self, Group, Row};

mod tr {
    crate::i18n::catalogue! {
        HeroLayouts => "input sources", "джерела вводу";
        HeroOneLayout => "One input source", "Одне джерело вводу";
        HeroSwitch => "switch with", "перемикання:";
        SpecialKeys => "Special keys", "Особливі клавіші";
        SpecialKeysHint => "Change what individual keys do. Applies to every keyboard.", "Змініть призначення окремих клавіш. Діє для всіх клавіатур.";
        ComposeKey => "Compose key", "Клавіша Compose";
        ComposeKeyHint => "Press it, then two characters to type é, ß, € and more", "Натисніть її, а потім два символи, щоб ввести é, ß, € тощо";
        CapsLock => "Caps Lock key", "Клавіша Caps Lock";
        CapsLockHint => "What Caps Lock does when pressed", "Що робить Caps Lock під час натискання";
        CapsLockBusy => "Caps Lock switches layouts, so it has no other role", "Caps Lock перемикає розкладки, тож іншої ролі не має";
        SwapAltSuper => "Swap Alt and Super", "Поміняти Alt і Super";
        SwapAltSuperHint => "Handy on keyboards made for a Mac", "Зручно на клавіатурах для Mac";
        BothShifts => "Both Shift keys toggle Caps Lock", "Обидва Shift перемикають Caps Lock";
        BothShiftsHint => "A single Shift still works as usual", "Один Shift працює як зазвичай";
        NumpadDigits => "Number pad always types digits", "Цифровий блок завжди вводить цифри";
        NumpadDigitsHint => "Ignores Num Lock, as on a Mac", "Не зважає на Num Lock, як на Mac";
        NumLock => "Num Lock on at sign-in", "Num Lock увімкнено під час входу";
        NumLockHint => "The keypad types digits and its light shows the real state; off turns the keypad into arrows", "Цифровий блок вводить цифри, а індикатор показує справжній стан; вимкнено — блок працює як стрілки";
        Off => "Off", "Вимкнено";
        RightAlt => "Right Alt", "Правий Alt";
        RightSuper => "Right Super", "Правий Super";
        MenuKey => "Menu key", "Клавіша меню";
        RightCtrl => "Right Ctrl", "Правий Ctrl";
        CapsDefault => "Caps Lock (default)", "Caps Lock (типово)";
        CapsEscape => "Escape", "Escape";
        CapsSwapEscape => "Swap with Escape", "Поміняти з Escape";
        CapsCtrl => "Ctrl", "Ctrl";
        CapsSwapCtrl => "Swap with left Ctrl", "Поміняти з лівим Ctrl";
        CapsBackspace => "Backspace", "Backspace";
        CapsNone => "Does nothing", "Нічого не робить";
        RepeatHint => "Hold a key in the field below to try the delay and rate", "Затисніть клавішу в полі нижче, щоб перевірити затримку й частоту";
        Characters => "characters", "символів";
        ScopeTitle => "Remember the layout for", "Пам'ятати розкладку для";
        ScopeHint => "Glide restores the layout you last used there when you come back", "Glide повертає розкладку, якою ви там користувалися, коли повертаєтесь";
        ScopeGlobal => "Nothing, one layout everywhere", "Нічого, одна розкладка всюди";
        ScopeDesktop => "Each desktop", "Кожного робочого стола";
        ScopeApplication => "Each application", "Кожного застосунку";
        ScopeWindow => "Each window", "Кожного вікна";
        LayoutIndicator => "Show the layout when it changes", "Показувати розкладку під час перемикання";
        LayoutIndicatorHint => "A large indicator above the dock rolls from the old layout to the new one", "Велике вікно над доком прокручує стару розкладку до нової";
        NewWindowTitle => "New windows start with", "Нові вікна починають з";
        NewWindowHint => "The layout a desktop, app or window gets on its first visit", "Розкладка, яку робочий стіл, застосунок чи вікно отримує вперше";
        NewWindowDefault => "The first input source", "Першого джерела вводу";
        NewWindowCurrent => "The layout in use", "Поточної розкладки";
    }
}

pub const LAYOUT_SCOPE: &str = "layout-scope";
pub(crate) const NEW_WINDOW_LAYOUT: &str = "new-window-layout";
pub(crate) const LAYOUT_INDICATOR: &str = "layout-indicator";

pub const SCOPES: [(&str, tr::Key); 4] = [
    ("global", tr::Key::ScopeGlobal),
    ("desktop", tr::Key::ScopeDesktop),
    ("application", tr::Key::ScopeApplication),
    ("window", tr::Key::ScopeWindow),
];

pub(crate) const NEW_WINDOW_LAYOUTS: [(&str, tr::Key); 2] =
    [("default", tr::Key::NewWindowDefault), ("current", tr::Key::NewWindowCurrent)];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputChoice {
    pub key: &'static str,
    pub value: &'static str,
    first: bool,
    label: String,
}

impl std::fmt::Display for InputChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

#[must_use]
pub fn input_choices(
    lang: Lang, config: &ConfigStore, key: &'static str, options: &[(&'static str, tr::Key)],
) -> (Vec<InputChoice>, InputChoice) {
    let choices: Vec<InputChoice> = options
        .iter()
        .enumerate()
        .map(|(index, (value, label))| InputChoice {
            key,
            value,
            first: index == 0,
            label: label.text(lang).to_string(),
        })
        .collect();
    let current = config.get_str("input", key);
    let selected = choices.iter().find(|choice| Some(choice.value) == current).unwrap_or(&choices[0]).clone();
    (choices, selected)
}

#[must_use]
pub fn shortcut_choices(c: Ctx, options: &str) -> (Vec<Shortcut>, Shortcut) {
    let off = Shortcut { option: String::new(), label: c.t(tr::Key::Off).to_string() };
    let shortcut = |option: &str| Shortcut { option: option.to_string(), label: shortcut_label(option).to_string() };
    let mut shortcuts: Vec<Shortcut> =
        std::iter::once(off).chain(SWITCH_OPTIONS.iter().map(|option| shortcut(option))).collect();
    let option = input::switch_option(options).unwrap_or_default();
    let current = shortcuts.iter().find(|known| known.option == option).cloned().unwrap_or_else(|| {
        let unknown = Shortcut { option: option.to_string(), label: option.to_string() };
        shortcuts.push(unknown.clone());
        unknown
    });
    (shortcuts, current)
}

fn shortcut_row<'a>(c: &Ctx, config: &ConfigStore) -> Element<'a, Msg> {
    let (shortcuts, current) = shortcut_choices(*c, &config.xkb_options());
    let dropdown = widgets::dropdown(c.theme, shortcuts, Some(current), Msg::Shortcut, 200.0);
    Row::new(c.theme, c.t(K::SwitchLayouts)).subtitle(c.t(K::SwitchLayoutsHint)).suffix(gated(*c, dropdown)).build()
}

fn pick_row<'a>(
    c: &Ctx, (title, hint): (tr::Key, tr::Key), (options, current): (Vec<InputChoice>, InputChoice),
) -> Element<'a, Msg> {
    let dropdown = widgets::dropdown(c.theme, options, Some(current), Msg::Pick, 220.0);
    Row::new(c.theme, c.t(title)).subtitle(c.t(hint)).suffix(gated(*c, dropdown)).build()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shortcut {
    pub option: String,
    pub label: String,
}

impl std::fmt::Display for Shortcut {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

#[must_use]
pub fn shortcut_label(option: &str) -> &'static str {
    match option {
        "grp:win_space_toggle" => "Super+Space",
        "grp:alt_shift_toggle" => "Alt+Shift",
        "grp:ctrl_shift_toggle" => "Ctrl+Shift",
        "grp:caps_toggle" => "Caps Lock",
        "grp:alt_space_toggle" => "Alt+Space",
        _ => "—",
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionChoice {
    pub option: Option<String>,
    pub label: String,
}

impl std::fmt::Display for OptionChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

fn compose_label(option: &str) -> Option<tr::Key> {
    Some(match option {
        "compose:ralt" => tr::Key::RightAlt,
        "compose:rwin" => tr::Key::RightSuper,
        "compose:menu" => tr::Key::MenuKey,
        "compose:rctrl" => tr::Key::RightCtrl,
        _ => return None,
    })
}

fn compose_text(c: &Ctx, option: &str) -> String {
    match option {
        "compose:caps" => String::from("Caps Lock"),
        "compose:prsc" => String::from("Print Screen"),
        "compose:sclk" => String::from("Scroll Lock"),
        other => compose_label(other).map_or_else(|| other.to_string(), |key| c.t(key).to_string()),
    }
}

fn caps_text(c: &Ctx, option: &str) -> String {
    let key = match option {
        "caps:escape" => tr::Key::CapsEscape,
        "caps:swapescape" => tr::Key::CapsSwapEscape,
        "ctrl:nocaps" => tr::Key::CapsCtrl,
        "ctrl:swapcaps" => tr::Key::CapsSwapCtrl,
        "caps:backspace" => tr::Key::CapsBackspace,
        "caps:none" => tr::Key::CapsNone,
        other => return other.to_string(),
    };
    c.t(key).to_string()
}

#[must_use]
pub fn compose_choices(c: Ctx, options: &str) -> (Vec<OptionChoice>, OptionChoice) {
    option_choices(
        c.t(tr::Key::Off),
        COMPOSE_OPTIONS,
        input::find_option(options, input::is_compose_option),
        |option| compose_text(&c, option),
    )
}

#[must_use]
pub fn caps_choices(c: Ctx, options: &str) -> (Vec<OptionChoice>, OptionChoice) {
    option_choices(
        c.t(tr::Key::CapsDefault),
        CAPS_OPTIONS,
        input::find_option(options, input::is_caps_option),
        |option| caps_text(&c, option),
    )
}

fn option_choices(
    default: &str, known: &[&str], current: Option<&str>, label: impl Fn(&str) -> String,
) -> (Vec<OptionChoice>, OptionChoice) {
    let choice = |option: Option<&str>| OptionChoice {
        option: option.map(str::to_string),
        label: option.map_or_else(|| default.to_string(), &label),
    };
    let mut choices: Vec<_> = std::iter::once(None).chain(known.iter().copied().map(Some)).map(choice).collect();
    let found = choices.iter().find(|choice| choice.option.as_deref() == current).cloned();
    let selected = found.unwrap_or_else(|| {
        let extra = choice(current);
        choices.push(extra.clone());
        extra
    });
    (choices, selected)
}

#[derive(Debug, Clone)]
pub enum Msg {
    MoveUp(usize),
    MoveDown(usize),
    Remove(usize),
    Add(Layout),
    Shortcut(Shortcut),
    Pick(InputChoice),
    Compose(OptionChoice),
    Caps(OptionChoice),
    SwapAltSuper(bool),
    BothShifts(bool),
    NumpadDigits(bool),
    NumLock(bool),
    LayoutIndicator(bool),
    Delay(f64),
    Rate(f64),
    CommitRepeat,
    SettleRepeat(u64),
    TestInput(String),
    ResetRepeat,
    Layouts(Vec<Layout>),
}

#[derive(Debug, Default)]
pub struct Page {
    layouts: Vec<Layout>,
    loading_layouts: bool,
    delay: Option<f64>,
    rate: Option<f64>,
    test: String,
    settle: Debounce,
}

impl Pane for Page {
    type Msg = Msg;

    const KEYWORDS: &'static [&'static str] = &[
        "layout",
        "input source",
        "repeat",
        "xkb",
        "compose",
        "caps lock",
        "num lock",
        "розкладка",
        "повтор",
        "клавіші",
    ];

    fn open(&mut self, _c: Ctx, _config: &ConfigStore) -> Task<Msg> {
        if !self.layouts.is_empty() || self.loading_layouts {
            return Task::none();
        }
        self.loading_layouts = true;
        Task::perform(blocking(input::available_layouts), |layouts| Msg::Layouts(layouts.unwrap_or_default()))
    }

    fn update(&mut self, msg: Msg, _c: Ctx, config: &mut ConfigStore) -> Task<Msg> {
        match msg {
            Msg::MoveUp(index) => {
                let mut sources = Self::sources(config);
                if index > 0 && index < sources.len() {
                    sources.swap(index, index - 1);
                    Self::store(config, &sources);
                }
            }
            Msg::MoveDown(index) => {
                let mut sources = Self::sources(config);
                if index + 1 < sources.len() {
                    sources.swap(index, index + 1);
                    Self::store(config, &sources);
                }
            }
            Msg::Remove(index) => {
                let mut sources = Self::sources(config);
                if sources.len() > 1 && index < sources.len() {
                    sources.remove(index);
                    Self::store(config, &sources);
                }
            }
            Msg::Add(layout) => {
                let mut sources = Self::sources(config);
                if !sources.iter().any(|source| source.layout == layout.code) {
                    sources.push(Source { layout: layout.code, variant: String::new() });
                    Self::store(config, &sources);
                }
            }
            Msg::Shortcut(shortcut) => {
                Self::set_options(config, |options| input::replace_switch_option(options, &shortcut.option));
            }
            Msg::Pick(choice) if choice.first => config.remove("input", choice.key),
            Msg::Pick(choice) => config.set_str("input", choice.key, choice.value),
            Msg::Compose(choice) => Self::set_options(config, |options| {
                input::replace_option(options, input::is_compose_option, choice.option.as_deref())
            }),
            Msg::Caps(choice) => Self::set_options(config, |options| {
                input::replace_option(options, input::is_caps_option, choice.option.as_deref())
            }),
            Msg::SwapAltSuper(on) => {
                Self::set_options(config, |options| input::set_option(options, SWAP_ALT_SUPER, on));
            }
            Msg::BothShifts(on) => {
                Self::set_options(config, |options| input::set_option(options, BOTH_SHIFTS_CAPS, on));
            }
            Msg::NumpadDigits(on) => Self::set_options(config, |options| input::set_option(options, NUMPAD_DIGITS, on)),
            Msg::NumLock(on) => config.set_bool("input", "numlock", on),
            Msg::LayoutIndicator(true) => config.remove("input", LAYOUT_INDICATOR),
            Msg::LayoutIndicator(false) => config.set_bool("input", LAYOUT_INDICATOR, false),
            Msg::Delay(delay) => {
                self.delay = Some(delay);
                return self.settle.schedule(Msg::SettleRepeat);
            }
            Msg::Rate(rate) => {
                self.rate = Some(rate);
                return self.settle.schedule(Msg::SettleRepeat);
            }
            Msg::SettleRepeat(generation) if !self.settle.is_current(generation) => {}
            Msg::CommitRepeat | Msg::SettleRepeat(_) => {
                self.settle.cancel();
                {
                    if let Some(delay) = self.delay.take() {
                        config.set_int("input", "repeat-delay", delay.round() as i64);
                    }
                    if let Some(rate) = self.rate.take() {
                        config.set_int("input", "repeat-rate", rate.round() as i64);
                    }
                }
            }
            Msg::ResetRepeat => {
                self.settle.cancel();
                self.delay = None;
                self.rate = None;
                config.set_int("input", "repeat-delay", DEFAULT_REPEAT_DELAY);
                config.set_int("input", "repeat-rate", DEFAULT_REPEAT_RATE);
            }
            Msg::TestInput(value) => self.test = value,
            Msg::Layouts(layouts) => {
                self.loading_layouts = false;
                self.layouts = layouts;
            }
        }
        Task::none()
    }

    fn view<'a>(&'a self, c: Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        column![
            self.hero(&c, config),
            self.sources_group(&c, config),
            Page::special_group(&c, config),
            self.repeat_group(&c, config),
        ]
        .spacing(theme::SECTION_GAP)
        .into()
    }
}

impl Page {
    fn sources(config: &ConfigStore) -> Vec<Source> {
        input::split_sources(&config.xkb_layout(), &config.xkb_variant())
    }

    fn store(config: &mut ConfigStore, sources: &[Source]) {
        let (layout, variant) = input::join_sources(sources);
        config.set_str("input", "xkb-layout", &layout);
        if variant.is_empty() {
            config.remove("input", "xkb-variant");
        } else {
            config.set_str("input", "xkb-variant", &variant);
        }
    }

    fn set_options(config: &mut ConfigStore, edit: impl FnOnce(&str) -> String) {
        let options = edit(&config.xkb_options());
        config.set_str("input", "xkb-options", &options);
    }

    fn layout_name(&self, code: &str) -> String {
        self.layouts
            .iter()
            .find(|layout| layout.code == code)
            .map_or_else(|| code.to_uppercase(), |layout| layout.name.clone())
    }

    fn hero<'a>(&self, c: &Ctx, config: &ConfigStore) -> Element<'a, Msg> {
        let t = c.theme;
        let sources = Self::sources(config);
        let mut caps = row![].spacing(6).align_y(Alignment::Center);
        for (index, source) in sources.iter().take(4).enumerate() {
            let code = source.layout.to_uppercase();
            caps = caps.push(if index == 0 {
                widgets::pill(t, theme::Tone::Accent, code)
            } else {
                widgets::key_cap(t, &code)
            });
        }
        let options = config.xkb_options();
        let switch = input::switch_option(&options).map(shortcut_label);
        let heading = if sources.len() == 1 {
            self.layout_name(&sources[0].layout)
        } else {
            format!("{} {}", sources.len(), c.t(tr::Key::HeroLayouts))
        };
        let names = |separator: &str| {
            sources.iter().map(|source| self.layout_name(&source.layout)).collect::<Vec<_>>().join(separator)
        };
        let description = match switch {
            _ if sources.len() == 1 => c.t(tr::Key::HeroOneLayout).to_string(),
            None => names(" · "),
            Some(label) => format!("{} · {} {label}", names(", "), c.t(tr::Key::HeroSwitch)),
        };
        widgets::section_hero(t, Icon::Keyboard, Panel::Keyboard.color(), heading, description, Some(caps.into()))
    }

    fn sources_group<'a>(&'a self, c: &Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let t = c.theme;
        let sources = Self::sources(config);
        let count = sources.len();
        let mut group = Group::new(t).title(c.t(K::InputSources)).description(c.t(K::InputSourcesHint));
        for (index, source) in sources.iter().enumerate() {
            let code = if source.variant.is_empty() {
                source.layout.to_uppercase()
            } else {
                format!("{} · {}", source.layout.to_uppercase(), source.variant)
            };
            let badge = container(widgets::pill(t, crate::theme::Tone::Accent, source.layout.to_uppercase()))
                .width(Length::Fixed(44.0));
            let actions = gated(
                *c,
                row![
                    widgets::icon_button(t, Icon::ChevronUp, (index > 0).then_some(Msg::MoveUp(index))),
                    widgets::icon_button(t, Icon::ChevronDown, (index + 1 < count).then_some(Msg::MoveDown(index)),),
                    widgets::icon_button(t, Icon::Trash, (count > 1).then_some(Msg::Remove(index))),
                ]
                .spacing(2),
            );
            group = group.push(
                Row::new(t, self.layout_name(&source.layout))
                    .subtitle(if index == 0 { format!("{code} · {}", c.t(K::Default)) } else { code })
                    .prefix(badge)
                    .suffix(actions)
                    .build(),
            );
        }
        let available: Vec<Layout> = self
            .layouts
            .iter()
            .filter(|layout| !sources.iter().any(|source| source.layout == layout.code))
            .cloned()
            .collect();
        let (scopes, scope) = input_choices(c.lang, config, LAYOUT_SCOPE, &SCOPES);
        let fresh = (!scope.first).then(|| input_choices(c.lang, config, NEW_WINDOW_LAYOUT, &NEW_WINDOW_LAYOUTS));
        group
            .push(
                Row::new(t, c.t(K::AddInputSource))
                    .prefix(widgets::icon(Icon::Plus, 18.0, t.acc_text()))
                    .suffix(gated(*c, widgets::chooser(t, available, c.t(K::ChooseLayout), Msg::Add, 260.0)))
                    .build(),
            )
            .push(shortcut_row(c, config))
            .push(pick_row(c, (tr::Key::ScopeTitle, tr::Key::ScopeHint), (scopes, scope)))
            .push_maybe(fresh.map(|choices| pick_row(c, (tr::Key::NewWindowTitle, tr::Key::NewWindowHint), choices)))
            .push(switch_row(
                *c,
                c.t(tr::Key::LayoutIndicator),
                c.t(tr::Key::LayoutIndicatorHint),
                config.bool_or("input", LAYOUT_INDICATOR, true),
                Msg::LayoutIndicator,
            ))
            .build()
    }

    fn special_group<'a>(c: &Ctx, config: &ConfigStore) -> Element<'a, Msg> {
        let t = c.theme;
        let options = config.xkb_options();
        let (compose, compose_current) = compose_choices(*c, &options);
        let (caps, caps_current) = caps_choices(*c, &options);
        let caps_switches = input::switch_option(&options) == Some("grp:caps_toggle");
        let toggle = |title, hint, value: bool, on_toggle: fn(bool) -> Msg| {
            switch_row(*c, c.t(title), c.t(hint), value, on_toggle)
        };
        Group::new(t)
            .title(c.t(tr::Key::SpecialKeys))
            .description(c.t(tr::Key::SpecialKeysHint))
            .push(
                Row::new(t, c.t(tr::Key::ComposeKey))
                    .subtitle(c.t(tr::Key::ComposeKeyHint))
                    .suffix(gated(*c, widgets::dropdown(t, compose, Some(compose_current), Msg::Compose, 200.0)))
                    .build(),
            )
            .push(
                Row::new(t, c.t(tr::Key::CapsLock))
                    .subtitle(c.t(if caps_switches { tr::Key::CapsLockBusy } else { tr::Key::CapsLockHint }))
                    .suffix(widgets::inert(
                        t,
                        c.writable && !caps_switches,
                        widgets::dropdown(t, caps, Some(caps_current), Msg::Caps, 200.0),
                    ))
                    .build(),
            )
            .push(toggle(
                tr::Key::SwapAltSuper,
                tr::Key::SwapAltSuperHint,
                input::has_option(&options, SWAP_ALT_SUPER),
                Msg::SwapAltSuper,
            ))
            .push(toggle(
                tr::Key::BothShifts,
                tr::Key::BothShiftsHint,
                input::has_option(&options, BOTH_SHIFTS_CAPS),
                Msg::BothShifts,
            ))
            .push(toggle(
                tr::Key::NumpadDigits,
                tr::Key::NumpadDigitsHint,
                input::has_option(&options, NUMPAD_DIGITS),
                Msg::NumpadDigits,
            ))
            .push(toggle(
                tr::Key::NumLock,
                tr::Key::NumLockHint,
                config.bool_or("input", "numlock", true),
                Msg::NumLock,
            ))
            .build()
    }

    fn repeat_group<'a>(&'a self, c: &Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let t = c.theme;
        let delay = self.delay.unwrap_or(config.repeat_delay() as f64);
        let rate = self.rate.unwrap_or(config.repeat_rate() as f64);
        let slider = |value: f64, range, step, on_change: fn(f64) -> Msg| {
            gated(
                *c,
                widgets::flexible(
                    widgets::value_slider(t, range, value, step, on_change, Some(Msg::CommitRepeat)),
                    240.0,
                ),
            )
        };
        let typed = self.test.chars().count();
        Group::new(t)
            .title(c.t(K::KeyRepeat))
            .description(c.t(tr::Key::RepeatHint))
            .suffix(widgets::secondary(t, c.t(K::Reset), c.writable.then_some(Msg::ResetRepeat)))
            .push(
                Row::new(t, c.t(K::RepeatDelay))
                    .subtitle(format!("{delay:.0} {}", c.t(K::Milliseconds)))
                    .suffix(slider(delay, 100.0..=1000.0, 10.0, Msg::Delay))
                    .build(),
            )
            .push(
                Row::new(t, c.t(K::RepeatRate))
                    .subtitle(format!("{rate:.0} {}", c.t(K::PerSecond)))
                    .suffix(slider(rate, 5.0..=80.0, 1.0, Msg::Rate))
                    .build(),
            )
            .push(
                container(
                    row![
                        widgets::field(t, c.t(K::TestHere), &self.test, Msg::TestInput, None, false)
                            .width(Length::Fill),
                        text(format!("{typed} {}", c.t(tr::Key::Characters)), TextStyle::CAP).color(t.fg_3()),
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center),
                )
                .padding([8.0, theme::ROW_PADDING_X]),
            )
            .build()
    }
}

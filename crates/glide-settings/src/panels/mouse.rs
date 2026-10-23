use std::time::{Duration, Instant};

use glide_sdk::theme::TextStyle;
use glide_sdk::widget::{mock as sketch, text};
use iced::widget::{Column, column, container, row, space};
use iced::{Alignment, Color, Element, Fill, Length, Task};

use super::common::{choice, gated, switch_row, toggle};
use super::{Ctx, Pane, Panel};
use crate::i18n::Key as K;
use crate::icons::Icon;
use crate::services::config::{AccelProfile, ConfigStore};
use crate::services::pointer::{self, PointerKind, PointerState};
use crate::services::run::{Debounce, blocking};
use crate::theme::{self, ThemeState, Tone};
use crate::widgets::{self, Group, Row};

mod tr {
    crate::i18n::catalogue! {
        Devices => "Pointing devices", "Вказівні пристрої";
        MouseTitle => "Mouse", "Миша";
        TouchpadTitle => "Touchpad", "Тачпад";
        Detected => "Connected", "Під'єднано";
        NotDetected => "Not found", "Не знайдено";
        DevicesUnknown => "Could not read the list of input devices", "Не вдалося прочитати список пристроїв вводу";
        MouseHint => "External mice and trackballs", "Зовнішні миші та трекболи";
        TouchpadHint => "Built-in and external touchpads", "Вбудовані й зовнішні тачпади";
        NoTouchpadHint => "No touchpad is connected. These settings apply as soon as one is.", "Тачпад не під'єднано. Ці налаштування запрацюють, щойно його під'єднають.";
        PrimaryButton => "Primary button", "Основна кнопка";
        PrimaryButtonHint => "Also swaps the touchpad buttons", "Також міняє місцями кнопки тачпада";
        ButtonLeft => "Left", "Ліва";
        ButtonRight => "Right", "Права";
        ScrollSpeed => "Scroll speed", "Швидкість прокрутки";
        ScrollSpeedHint => "How far one wheel notch or swipe moves content", "Наскільки зсувається вміст за один крок коліщатка чи рух пальців";
        SmoothWheel => "Smooth wheel scrolling", "Плавна прокрутка коліщатком";
        SmoothWheelHint => "Glide each notch instead of jumping, in apps that do not animate it themselves", "Плавно прокручувати кожен крок замість стрибка в застосунках, що не анімують його самі";
        MiddleEmulation => "Middle click by pressing both buttons", "Середній клік натисканням обох кнопок";
        MiddleEmulationHint => "For mice without a wheel button", "Для мишей без кнопки-коліщатка";
        SameSpeed => "Same speed as the mouse", "Та сама швидкість, що й у миші";
        SameSpeedHint => "Follow the mouse speed slider", "Дотримуватися повзунка швидкості миші";
        TouchpadSpeed => "Touchpad speed", "Швидкість тачпада";
        TapAndDrag => "Tap and drag", "Дотик і перетягування";
        TapAndDragHint => "Tap twice and keep the finger down to drag", "Торкніться двічі й не відпускайте палець, щоб перетягнути";
        DisableWhileTyping => "Disable while typing", "Вимикати під час набору";
        DisableWhileTypingHint => "Ignore accidental touches next to the keyboard", "Ігнорувати випадкові дотики біля клавіатури";
        ScrollWith => "Scroll with", "Прокрутка";
        ScrollWithHint => "How the touchpad scrolls", "Як прокручувати тачпадом";
        TwoFinger => "Two fingers", "Двома пальцями";
        TwoFingerHint => "Slide two fingers anywhere", "Проведіть двома пальцями будь-де";
        Edge => "Edge", "Край";
        EdgeHint => "Slide one finger along the right edge", "Проведіть одним пальцем уздовж правого краю";
        SecondaryClick => "Secondary click", "Додатковий клік";
        SecondaryClickHint => "How to right-click on a touchpad that is one big button", "Як клацнути правою кнопкою на тачпаді, що є однією великою кнопкою";
        ClickDefault => "Device default", "Як у пристрою";
        ClickDefaultHint => "Whatever the touchpad prefers", "Як задумав виробник";
        ClickAreas => "Bottom-right corner", "Правий нижній кут";
        ClickAreasHint => "Press the corner", "Натисніть у кутку";
        ClickFingers => "Two-finger click", "Клік двома пальцями";
        ClickFingersHint => "Press with two fingers", "Натисніть двома пальцями";
        TryIt => "Try it", "Перевірка";
        TryItHint => "Nothing here changes a setting", "Тут нічого не змінює налаштувань";
        DoubleClickTarget => "Double-click the circle", "Двічі клацніть по колу";
        DoubleClickHint => "Tests the primary button and your clicking speed", "Перевіряє основну кнопку й швидкість клацання";
        DoubleClickDone => "Double-click recognised", "Подвійний клік розпізнано";
        SingleClick => "Single click — try a little faster", "Одинарний клік — спробуйте трохи швидше";
        ScrollHere => "Scroll here to test the speed", "Прокрутіть тут, щоб перевірити швидкість";
        Line => "Line", "Рядок";
    }
}

use tr::Key as T;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slider {
    MouseScroll,
    TouchpadSpeed,
    TouchpadScroll,
}

impl Slider {
    const fn index(self) -> usize {
        match self {
            Self::MouseScroll => 0,
            Self::TouchpadSpeed => 1,
            Self::TouchpadScroll => 2,
        }
    }

    const fn key(self) -> &'static str {
        match self {
            Self::MouseScroll => "scroll-factor",
            Self::TouchpadSpeed => "touchpad-accel-speed",
            Self::TouchpadScroll => "touchpad-scroll-factor",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollMethod {
    TwoFinger,
    Edge,
}

impl ScrollMethod {
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::TwoFinger => "two-finger",
            Self::Edge => "edge",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClickMethod {
    Default,
    Areas,
    Clickfinger,
}

impl ClickMethod {
    #[must_use]
    pub const fn key(self) -> Option<&'static str> {
        match self {
            Self::Default => None,
            Self::Areas => Some("areas"),
            Self::Clickfinger => Some("clickfinger"),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Msg {
    Speed(f64),
    CommitSpeed,
    SettleSpeed(u64),
    Profile(AccelProfile),
    Natural(bool),
    Tap(bool),
    LeftHanded(bool),
    MiddleEmulation(bool),
    SmoothWheel(bool),
    Slide(Slider, f64),
    Commit(Slider),
    Settle(Slider, u64),
    TouchpadOwnSpeed(bool),
    TouchpadProfile(AccelProfile),
    TouchpadNatural(bool),
    TapAndDrag(bool),
    DisableWhileTyping(bool),
    Scroll(ScrollMethod),
    Click(ClickMethod),
    Devices(PointerState),
    Refresh,
    TestClick,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Test {
    #[default]
    Idle,
    Single,
    Double,
}

#[derive(Debug, Default)]
pub struct Page {
    speed: Option<f64>,
    settle: Debounce,
    pending: [Option<f64>; 3],
    settles: [Debounce; 3],
    devices: PointerState,
    loading: bool,
    last_click: Option<Instant>,
    test: Test,
}

pub const DOUBLE_CLICK: Duration = Duration::from_millis(400);
pub const SCROLL_RANGE: std::ops::RangeInclusive<f64> = 0.25..=3.0;

const TOUCHPAD_SPEED: &str = "touchpad-accel-speed";
const TOUCHPAD_PROFILE: &str = "touchpad-accel-profile";
const TOUCHPAD_NATURAL: &str = "touchpad-natural-scroll";
const TOUCHPAD_SCROLL: &str = "touchpad-scroll-factor";

#[must_use]
pub fn mouse_scroll_factor(config: &ConfigStore) -> f64 {
    config.get_float("input", "scroll-factor").unwrap_or(1.0)
}

#[must_use]
pub fn touchpad_speed(config: &ConfigStore) -> Option<f64> {
    config.get_float("input", TOUCHPAD_SPEED)
}

#[must_use]
pub fn touchpad_natural_scroll(config: &ConfigStore) -> bool {
    config.get_bool("input", TOUCHPAD_NATURAL).unwrap_or_else(|| config.natural_scroll())
}

#[must_use]
pub fn touchpad_scroll_factor(config: &ConfigStore) -> f64 {
    config.get_float("input", TOUCHPAD_SCROLL).unwrap_or_else(|| mouse_scroll_factor(config))
}

fn profile_from(key: Option<&str>) -> Option<AccelProfile> {
    match key? {
        "flat" => Some(AccelProfile::Flat),
        "adaptive" => Some(AccelProfile::Adaptive),
        _ => None,
    }
}

#[must_use]
pub fn touchpad_profile(config: &ConfigStore) -> AccelProfile {
    profile_from(config.get_str("input", TOUCHPAD_PROFILE)).unwrap_or_else(|| config.accel_profile())
}

#[must_use]
pub fn scroll_method(config: &ConfigStore) -> ScrollMethod {
    match config.get_str("input", "scroll-method") {
        Some("edge") => ScrollMethod::Edge,
        _ => ScrollMethod::TwoFinger,
    }
}

#[must_use]
pub fn click_method(config: &ConfigStore) -> ClickMethod {
    match config.get_str("input", "click-method") {
        Some("areas") => ClickMethod::Areas,
        Some("clickfinger") => ClickMethod::Clickfinger,
        _ => ClickMethod::Default,
    }
}

impl Pane for Page {
    type Msg = Msg;

    const KEYWORDS: &'static [&'static str] = &[
        "touchpad",
        "pointer",
        "trackpad",
        "scroll",
        "smooth",
        "tap",
        "cursor",
        "speed",
        "left-handed",
        "middle click",
        "right click",
        "тачпад",
        "вказівник",
        "прокрутка",
        "плавна",
        "дотик",
        "курсор",
        "швидкість",
        "лівша",
        "кнопка",
    ];

    fn open(&mut self, _c: Ctx, _config: &ConfigStore) -> Task<Msg> {
        self.refresh()
    }

    fn update(&mut self, msg: Msg, _c: Ctx, config: &mut ConfigStore) -> Task<Msg> {
        match msg {
            Msg::Speed(speed) => {
                self.speed = Some(speed);
                return self.settle.schedule(Msg::SettleSpeed);
            }
            Msg::SettleSpeed(generation) if !self.settle.is_current(generation) => {}
            Msg::CommitSpeed | Msg::SettleSpeed(_) => {
                self.settle.cancel();
                if let Some(speed) = self.speed.take() {
                    config.set_float("input", "accel-speed", speed);
                }
            }
            Msg::Profile(profile) => {
                if config.get_str("input", TOUCHPAD_PROFILE).is_none() {
                    let current = config.accel_profile();
                    if current != profile {
                        config.set_str("input", TOUCHPAD_PROFILE, current.key());
                    }
                }
                config.set_str("input", "accel-profile", profile.key());
            }
            Msg::Natural(on) => {
                if config.get_bool("input", TOUCHPAD_NATURAL).is_none() {
                    let current = config.natural_scroll();
                    if current != on {
                        config.set_bool("input", TOUCHPAD_NATURAL, current);
                    }
                }
                config.set_bool("input", "natural-scroll", on);
            }
            Msg::Tap(on) => config.set_bool("input", "tap-to-click", on),
            Msg::LeftHanded(on) => config.set_bool("input", "left-handed", on),
            Msg::MiddleEmulation(on) => config.set_bool("input", "middle-emulation", on),
            Msg::SmoothWheel(on) => config.set_bool("input", "smooth-wheel", on),
            Msg::Slide(slider, value) => {
                self.pending[slider.index()] = Some(value);
                return self.settles[slider.index()].schedule(move |generation| Msg::Settle(slider, generation));
            }
            Msg::Settle(slider, generation) if !self.settles[slider.index()].is_current(generation) => {}
            Msg::Commit(slider) | Msg::Settle(slider, _) => self.commit_slider(slider, config),
            Msg::TouchpadOwnSpeed(own) => {
                self.pending[Slider::TouchpadSpeed.index()] = None;
                self.settles[Slider::TouchpadSpeed.index()].cancel();
                if own {
                    let speed = config.accel_speed();
                    config.set_float("input", TOUCHPAD_SPEED, speed);
                } else {
                    config.remove("input", TOUCHPAD_SPEED);
                }
            }
            Msg::TouchpadProfile(profile) => {
                config.set_str("input", TOUCHPAD_PROFILE, profile.key());
            }
            Msg::TouchpadNatural(on) => config.set_bool("input", TOUCHPAD_NATURAL, on),
            Msg::TapAndDrag(on) => config.set_bool("input", "tap-and-drag", on),
            Msg::DisableWhileTyping(on) => config.set_bool("input", "disable-while-typing", on),
            Msg::Scroll(method) => config.set_str("input", "scroll-method", method.key()),
            Msg::Click(method) => match method.key() {
                Some(key) => config.set_str("input", "click-method", key),
                None => config.remove("input", "click-method"),
            },
            Msg::Devices(state) => {
                self.loading = false;
                self.devices = state;
            }
            Msg::Refresh => return self.refresh(),
            Msg::TestClick => {
                let now = Instant::now();
                let double = self.last_click.is_some_and(|last| now.duration_since(last) <= DOUBLE_CLICK);
                if double {
                    self.test = Test::Double;
                    self.last_click = None;
                } else {
                    self.test = Test::Single;
                    self.last_click = Some(now);
                }
            }
        }
        Task::none()
    }

    fn view<'a>(&'a self, c: Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        column![
            self.devices_card(&c, config),
            self.mouse_group(&c, config),
            self.touchpad_group(&c, config),
            choice_section(&c, (T::ScrollWith, T::ScrollWithHint), &SCROLL_CARDS, scroll_method(config), Msg::Scroll,),
            choice_section(
                &c,
                (T::SecondaryClick, T::SecondaryClickHint),
                &CLICK_CARDS,
                click_method(config),
                Msg::Click,
            ),
            self.test_group(&c),
        ]
        .spacing(theme::SECTION_GAP)
        .into()
    }
}

fn freeze_float(config: &mut ConfigStore, key: &str, current: f64) {
    if config.get_float("input", key).is_none() {
        config.set_float("input", key, current);
    }
}

impl Page {
    fn refresh(&mut self) -> Task<Msg> {
        if self.loading {
            return Task::none();
        }
        self.loading = true;
        Task::perform(blocking(pointer::read), |state| Msg::Devices(state.unwrap_or_default()))
    }

    fn commit_slider(&mut self, slider: Slider, config: &mut ConfigStore) {
        self.settles[slider.index()].cancel();
        let Some(value) = self.pending[slider.index()].take() else {
            return;
        };
        if slider == Slider::MouseScroll {
            let touchpad = touchpad_scroll_factor(config);
            freeze_float(config, TOUCHPAD_SCROLL, touchpad);
        }
        config.set_float("input", slider.key(), value);
    }

    fn pending(&self, slider: Slider) -> Option<f64> {
        self.pending[slider.index()]
    }

    fn devices_card<'a>(&'a self, c: &Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let t = c.theme;
        let left_handed = config.get_bool("input", "left-handed").unwrap_or(false);
        let entry = |art: Element<'a, Msg>, heading: &'a str, kind: PointerKind| {
            let names = self.devices.names(kind);
            let (tone, status) = if !self.devices.read {
                (Tone::Neutral, String::from("—"))
            } else if names.is_empty() {
                (Tone::Neutral, c.t(T::NotDetected).to_string())
            } else {
                (Tone::Ok, c.t(T::Detected).to_string())
            };
            let detail = names.join(", ");
            column![
                container(art).center_x(Fill).height(Length::Fixed(80.0)),
                text(heading, TextStyle::HEAD).color(t.fg_1()),
                text(detail, TextStyle::CAP).color(t.fg_2()).align_x(Alignment::Center),
                widgets::status_pill(t, tone, status),
            ]
            .spacing(6)
            .align_x(Alignment::Center)
            .width(Fill)
        };
        let mut body = column![
            row![
                entry(mouse_art(&t, left_handed), c.t(T::MouseTitle), PointerKind::Mouse),
                entry(pad_art(&t, PadArt::Plain), c.t(T::TouchpadTitle), PointerKind::Touchpad),
            ]
            .spacing(16),
        ]
        .spacing(10);
        if !self.devices.read && !self.loading {
            body = body.push(
                text(c.t(T::DevicesUnknown), TextStyle::CAP).color(t.fg_3()).width(Fill).align_x(Alignment::Center),
            );
        }
        let content = column![
            row![
                text(c.t(T::Devices), TextStyle::HEAD).color(t.fg_1()).width(Fill),
                widgets::icon_button(t, Icon::Refresh, Some(Msg::Refresh)),
            ]
            .align_y(Alignment::Center),
            body,
        ]
        .spacing(12);
        glide_sdk::widget::tinted_card(&widgets::paint(&t), Panel::Mouse.color(), 16, content)
    }

    fn mouse_group<'a>(&'a self, c: &Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let t = c.theme;
        let speed = self.speed.unwrap_or_else(|| config.accel_speed());
        let buttons = [(false, c.t(T::ButtonLeft)), (true, c.t(T::ButtonRight))];
        let scroll = self.pending(Slider::MouseScroll).unwrap_or_else(|| mouse_scroll_factor(config));
        Group::new(t)
            .title(c.t(T::MouseTitle))
            .description(c.t(T::MouseHint))
            .push(
                Row::new(t, c.t(T::PrimaryButton))
                    .subtitle(c.t(T::PrimaryButtonHint))
                    .suffix(choice(
                        *c,
                        buttons,
                        config.get_bool("input", "left-handed").unwrap_or(false),
                        Msg::LeftHanded,
                    ))
                    .build(),
            )
            .push(speed_row(c, c.t(K::PointerSpeed), speed, Msg::Speed, Msg::CommitSpeed))
            .push(profile_row(c, config.accel_profile(), Msg::Profile))
            .push(natural_row(c, config.natural_scroll(), Msg::Natural))
            .push(scroll_row(c, scroll, Slider::MouseScroll))
            .push(switch_row(
                *c,
                c.t(T::SmoothWheel),
                c.t(T::SmoothWheelHint),
                config.get_bool("input", "smooth-wheel").unwrap_or(true),
                Msg::SmoothWheel,
            ))
            .push(switch_row(
                *c,
                c.t(T::MiddleEmulation),
                c.t(T::MiddleEmulationHint),
                config.get_bool("input", "middle-emulation").unwrap_or(false),
                Msg::MiddleEmulation,
            ))
            .build()
    }

    fn touchpad_group<'a>(&'a self, c: &Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let t = c.theme;
        let own_speed = touchpad_speed(config);
        let description = if self.devices.read && !self.devices.has(PointerKind::Touchpad) {
            c.t(T::NoTouchpadHint)
        } else {
            c.t(T::TouchpadHint)
        };
        let mut group =
            Group::new(t).title(c.t(T::TouchpadTitle)).description(description).extend(touch_rows(c, config)).push(
                switch_row(*c, c.t(T::SameSpeed), c.t(T::SameSpeedHint), own_speed.is_none(), |same| {
                    Msg::TouchpadOwnSpeed(!same)
                }),
            );
        if let Some(stored) = own_speed {
            group = group.push(speed_row(
                c,
                c.t(T::TouchpadSpeed),
                self.pending(Slider::TouchpadSpeed).unwrap_or(stored),
                |value| Msg::Slide(Slider::TouchpadSpeed, value),
                Msg::Commit(Slider::TouchpadSpeed),
            ));
        }
        let scroll = self.pending(Slider::TouchpadScroll).unwrap_or_else(|| touchpad_scroll_factor(config));
        group
            .push(profile_row(c, touchpad_profile(config), Msg::TouchpadProfile))
            .push(natural_row(c, touchpad_natural_scroll(config), Msg::TouchpadNatural))
            .push(scroll_row(c, scroll, Slider::TouchpadScroll))
            .build()
    }

    fn test_group(&self, c: &Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        let (ring, fill, status) = match self.test {
            Test::Idle => (t.line_2(), t.fill_1(), c.t(T::DoubleClickTarget)),
            Test::Single => (t.acc_ring(), t.acc_soft(), c.t(T::SingleClick)),
            Test::Double => (t.ok(), t.ok_soft(), c.t(T::DoubleClickDone)),
        };
        let (glyph, tint) = if self.test == Test::Double { (Icon::Check, t.ok()) } else { (Icon::Mouse, t.fg_2()) };
        let target = sketch::click_target(glyph, tint, fill, ring, 56.0, Msg::TestClick);
        let mut lines = Column::new().spacing(6).padding([6, 12]);
        for index in 1..=40 {
            let ink = if index % 5 == 0 { t.fg_1() } else { t.fg_3() };
            lines = lines.push(text(format!("{} {index}", c.t(T::Line)), TextStyle::CALL).color(ink));
        }
        let paint = widgets::paint(&t);
        let scroller = sketch::surface(
            t.sunken_bg(),
            theme::R2,
            glide_sdk::widget::scrollable(&paint, lines).height(Length::Fixed(120.0)).width(Fill),
        )
        .width(Fill);
        Group::new(t)
            .title(c.t(T::TryIt))
            .description(c.t(T::TryItHint))
            .push(Row::new(t, status).subtitle(c.t(T::DoubleClickHint)).suffix(target).build())
            .push(
                container(column![text(c.t(T::ScrollHere), TextStyle::CAP).color(t.fg_2()), scroller].spacing(6))
                    .padding([10.0, theme::ROW_PADDING_X]),
            )
            .build()
    }
}

type Card<V> = (V, PadArt, T, T);

const SCROLL_CARDS: [Card<ScrollMethod>; 2] = [
    (ScrollMethod::TwoFinger, PadArt::TwoFinger, T::TwoFinger, T::TwoFingerHint),
    (ScrollMethod::Edge, PadArt::Edge, T::Edge, T::EdgeHint),
];

const CLICK_CARDS: [Card<ClickMethod>; 3] = [
    (ClickMethod::Default, PadArt::Plain, T::ClickDefault, T::ClickDefaultHint),
    (ClickMethod::Areas, PadArt::Areas, T::ClickAreas, T::ClickAreasHint),
    (ClickMethod::Clickfinger, PadArt::Fingers, T::ClickFingers, T::ClickFingersHint),
];

fn choice_section<'a, V: Copy + PartialEq>(
    c: &Ctx, (heading, description): (T, T), cards: &[Card<V>], current: V, on_select: fn(V) -> Msg,
) -> Element<'a, Msg> {
    let t = c.theme;
    let cards = cards.iter().map(|&(value, art, title, hint)| {
        widgets::radio_card(
            t,
            pad_art(&t, art),
            c.t(title),
            Some(c.t(hint).to_string()),
            current == value,
            on_select(value),
        )
    });
    column![
        Group::new(t).title(c.t(heading)).description(c.t(description)).build(),
        gated(*c, iced::widget::Row::with_children(cards).spacing(12)),
    ]
    .spacing(8)
    .into()
}

fn touch_rows<'a>(c: &Ctx, config: &ConfigStore) -> Vec<Element<'a, Msg>> {
    let t = c.theme;
    let tap = config.tap_to_click();
    vec![
        Row::new(t, c.t(K::TapToClick)).subtitle(c.t(K::TapToClickHint)).suffix(toggle(*c, tap, Msg::Tap)).build(),
        Row::new(t, c.t(T::TapAndDrag))
            .subtitle(c.t(T::TapAndDragHint))
            .dimmed(!tap)
            .suffix(widgets::inert(
                t,
                c.writable && tap,
                widgets::switch(t, config.get_bool("input", "tap-and-drag").unwrap_or(true), Msg::TapAndDrag),
            ))
            .build(),
        switch_row(
            *c,
            c.t(T::DisableWhileTyping),
            c.t(T::DisableWhileTypingHint),
            config.get_bool("input", "disable-while-typing").unwrap_or(true),
            Msg::DisableWhileTyping,
        ),
    ]
}

fn speed_row<'a>(
    c: &Ctx, title: &str, speed: f64, on_change: impl Fn(f64) -> Msg + 'a, on_release: Msg,
) -> Element<'a, Msg> {
    let t = c.theme;
    let control = glide_sdk::widget::range_slider(
        &widgets::paint(&t),
        c.t(K::Slow),
        widgets::value_slider(t, -1.0..=1.0, speed, 0.05, on_change, Some(on_release)),
        c.t(K::Fast),
    );
    Row::new(t, title).suffix(gated(*c, widgets::flexible(control, 300.0))).build()
}

fn profile_row<'a>(c: &Ctx, current: AccelProfile, on_select: fn(AccelProfile) -> Msg) -> Element<'a, Msg> {
    let profiles = [(AccelProfile::Adaptive, c.t(K::Adaptive)), (AccelProfile::Flat, c.t(K::Flat))];
    Row::new(c.theme, c.t(K::Acceleration))
        .subtitle(c.t(K::AccelerationHint))
        .suffix(choice(*c, profiles, current, on_select))
        .build()
}

fn natural_row<'a>(c: &Ctx, value: bool, on_toggle: fn(bool) -> Msg) -> Element<'a, Msg> {
    switch_row(*c, c.t(K::NaturalScrolling), c.t(K::NaturalScrollingHint), value, on_toggle)
}

fn scroll_row<'a>(c: &Ctx, factor: f64, slider: Slider) -> Element<'a, Msg> {
    let t = c.theme;
    let control = glide_sdk::widget::slider_readout(
        &widgets::paint(&t),
        None,
        widgets::value_slider(
            t,
            SCROLL_RANGE,
            factor.clamp(*SCROLL_RANGE.start(), *SCROLL_RANGE.end()),
            0.05,
            move |value| Msg::Slide(slider, value),
            Some(Msg::Commit(slider)),
        ),
        format!("{factor:.2}×"),
        false,
    );
    Row::new(t, c.t(T::ScrollSpeed))
        .subtitle(c.t(T::ScrollSpeedHint))
        .suffix(gated(*c, widgets::flexible(control, 300.0)))
        .build()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PadArt {
    Plain,
    TwoFinger,
    Edge,
    Areas,
    Fingers,
}

fn finger<'a>(color: Color) -> Element<'a, Msg> {
    sketch::ring_dot(color, 12.0)
}

fn pad_art<'a>(t: &ThemeState, art: PadArt) -> Element<'a, Msg> {
    use sketch::block;
    let dot = t.acc_bg();
    let overlay: Element<'a, Msg> = match art {
        PadArt::Plain => container(block(t.line_2(), Fill, Length::Fixed(1.0), 0.0))
            .padding(iced::Padding::ZERO.top(50).left(6).right(6))
            .into(),
        PadArt::TwoFinger => container(
            column![
                widgets::icon(Icon::ChevronUp, 12.0, t.acc_text()),
                row![finger(dot), finger(dot)].spacing(6),
                widgets::icon(Icon::ChevronDown, 12.0, t.acc_text()),
            ]
            .spacing(1)
            .align_x(Alignment::Center),
        )
        .center(Fill)
        .into(),
        PadArt::Edge => row![
            space().width(Fill),
            sketch::surface(t.acc_soft(), theme::R1, finger(dot)).center_x(Length::Fixed(18.0)).center_y(Fill),
        ]
        .padding(4)
        .into(),
        PadArt::Areas => column![
            space().height(Fill),
            row![
                block(t.fill_3(), Fill, Length::Fixed(18.0), theme::R1),
                block(t.acc_bg(), Fill, Length::Fixed(18.0), theme::R1),
            ]
            .spacing(2),
        ]
        .padding(4)
        .into(),
        PadArt::Fingers => container(row![finger(dot), finger(dot)].spacing(6))
            .center_x(Fill)
            .align_bottom(Fill)
            .padding(iced::Padding::ZERO.bottom(10))
            .into(),
    };
    sketch::touchpad(*t, overlay)
}

fn mouse_art<'a>(t: &ThemeState, left_handed: bool) -> Element<'a, Msg> {
    sketch::mouse(*t, left_handed)
}

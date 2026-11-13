use glide_sdk::theme::TextStyle;
use glide_sdk::widget::text;
use iced::widget::{Column, container, row};
use iced::{Alignment, Element, Task};

use super::common::switch_row;
use super::{Ctx, Pane};
use crate::i18n::{Key as K, Lang};
use crate::icons::Icon;
use crate::services::config::ConfigStore;
use crate::theme;
use crate::widgets::{self, Group, Row};
use glide_sdk::widget::mock as sketch;

mod tr {
    crate::i18n::catalogue! {
        Preview => "Preview", "Попередній перегляд";
        PreviewHint => "A sample of the bar with the options below", "Зразок панелі з налаштуваннями нижче";
        Clock => "Clock", "Годинник";
        ClockFormatHint => "Set on the Date & Time page", "Налаштовується на сторінці «Дата й час»";
        WithDate => ", with the date", ", з датою";
        Auto => "By region", "За регіоном";
        Hours24 => "24-hour", "24 години";
        Hours12 => "12-hour", "12 годин";
        BatteryPercent => "Battery percentage", "Відсоток заряду";
        BatteryPercentHint => "Show the charge as a number next to the battery icon", "Показувати заряд числом біля значка батареї";
        Tray => "App status icons", "Значки стану програм";
        TrayHint => "Tray icons of apps such as Telegram, Steam or Nextcloud", "Значки в треї від програм на кшталт Telegram, Steam чи Nextcloud";
        SampleApp => "Files", "Файли";
        SampleMenus => "File   Edit   View", "Файл   Зміни   Вигляд";
        SampleDate => "Thu 24 Sep", "чт 24 вер.";
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeFormat {
    Auto,
    Hours24,
    Hours12,
}

impl TimeFormat {
    #[must_use]
    pub fn from_config(config: &ConfigStore) -> Self {
        match config.get_bool("topbar", "clock-24h") {
            Some(true) => Self::Hours24,
            Some(false) => Self::Hours12,
            None => Self::Auto,
        }
    }

    #[must_use]
    pub(crate) fn uses_24h(self) -> bool {
        match self {
            Self::Hours24 => true,
            Self::Hours12 => false,
            Self::Auto => !crate::services::timedate::env_uses_12_hour(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Msg {
    Open(super::Panel),
    Battery(bool),
    Tray(bool),
    GlobalMenu(bool),
    Registrar(bool),
}

#[derive(Debug, Default)]
pub struct Page;

impl Pane for Page {
    type Msg = Msg;

    const KEYWORDS: &'static [&'static str] = &[
        "top bar",
        "topbar",
        "panel",
        "clock",
        "date",
        "24",
        "tray",
        "battery",
        "global menu",
        "menu",
        "панель",
        "годинник",
        "дата",
        "трей",
        "батаре",
        "меню",
    ];

    fn update(&mut self, msg: Msg, _c: Ctx, config: &mut ConfigStore) -> Task<Msg> {
        match msg {
            Msg::Open(_) => {}
            Msg::Battery(on) => config.set_bool("topbar", "battery-percentage", on),
            Msg::Tray(on) => config.set_bool("topbar", "tray", on),
            Msg::GlobalMenu(on) => config.set_bool("topbar", "global-menu", on),
            Msg::Registrar(on) => config.set_bool("topbar", "appmenu-registrar", on),
        }
        Task::none()
    }

    fn view<'a>(&'a self, c: Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let t = c.theme;
        let switch_row = |title: &str, hint: &str, key: &str, msg: fn(bool) -> Msg| {
            switch_row(c, title, hint, config.bool_or("topbar", key, true), msg)
        };
        let preview = Group::new(t)
            .title(c.t(tr::Key::Preview))
            .description(c.t(tr::Key::PreviewHint))
            .stacked_header(t.compact)
            .push(container(bar_preview(&c, config)).padding(8));
        let page = Column::new().spacing(theme::SECTION_GAP).push(preview.build());
        let format = TimeFormat::from_config(config);
        let clock_value = format!(
            "{}{}",
            c.t(match format {
                TimeFormat::Auto => tr::Key::Auto,
                TimeFormat::Hours24 => tr::Key::Hours24,
                TimeFormat::Hours12 => tr::Key::Hours12,
            }),
            if config.bool_or("topbar", "clock-date", true) { c.t(tr::Key::WithDate) } else { "" }
        );
        let settings = Group::new(t)
            .title(c.t(K::TopBar))
            .push(
                Row::new(t, c.t(tr::Key::Clock))
                    .subtitle(c.t(tr::Key::ClockFormatHint))
                    .suffix(widgets::disclosure(t, Some(clock_value)))
                    .on_press(Msg::Open(super::Panel::DateTime))
                    .build(),
            )
            .push(switch_row(
                c.t(tr::Key::BatteryPercent),
                c.t(tr::Key::BatteryPercentHint),
                "battery-percentage",
                Msg::Battery,
            ))
            .push(switch_row(c.t(tr::Key::Tray), c.t(tr::Key::TrayHint), "tray", Msg::Tray))
            .push(switch_row(c.t(K::GlobalMenu), c.t(K::GlobalMenuHint), "global-menu", Msg::GlobalMenu))
            .push(switch_row(
                c.t(K::AppMenuRegistrar),
                c.t(K::AppMenuRegistrarHint),
                "appmenu-registrar",
                Msg::Registrar,
            ));
        page.push(settings.build()).into()
    }
}

#[must_use]
pub fn sample_clock(format: TimeFormat, lang: Lang, date: bool) -> String {
    let time = if format.uses_24h() { "14:05" } else { "2:05 PM" };
    if date { format!("{}  {time}", tr::Key::SampleDate.text(lang)) } else { time.to_string() }
}

fn bar_preview<'a>(c: &Ctx, config: &ConfigStore) -> Element<'a, Msg> {
    let t = c.theme;
    let fg = t.fg_1();
    let dim = t.fg_2();
    let strong = TextStyle { weight: 600, ..TextStyle::CAP };
    let mut left = row![widgets::icon(Icon::Apps, 13.0, fg), text(c.t(tr::Key::SampleApp), strong).color(fg)]
        .spacing(6)
        .align_y(Alignment::Center);
    if config.bool_or("topbar", "global-menu", true) {
        left = left.push(text(c.t(tr::Key::SampleMenus), TextStyle::CAP).color(dim));
    }
    let clock = text(
        sample_clock(TimeFormat::from_config(config), c.lang, config.bool_or("topbar", "clock-date", true)),
        strong,
    )
    .color(fg);
    let mut right = row![].spacing(8).align_y(Alignment::Center);
    if config.bool_or("topbar", "tray", true) {
        let ramp = t.accent().ramp();
        for step in [2, 4] {
            right = right.push(sketch::block(theme::to_iced_color(ramp[step]), 11.0, 11.0, theme::R1));
        }
    }
    right = right.push(widgets::icon(Icon::Volume, 13.0, fg)).push(widgets::icon(Icon::Wifi, 13.0, fg));
    let mut battery = row![widgets::icon(Icon::Battery, 14.0, fg)].spacing(3).align_y(Alignment::Center);
    if config.bool_or("topbar", "battery-percentage", true) {
        battery = battery.push(text("80%", TextStyle::CAP).color(fg));
    }
    right = right.push(battery).push(widgets::icon(Icon::Grid, 13.0, fg));
    sketch::bar_scene(t, left, clock, right, 96.0)
}

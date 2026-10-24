use glide_sdk::theme::TextStyle;
use glide_sdk::widget::text;
use iced::widget::{Column, container, row, space};
use iced::{Alignment, Element, Fill, Length, Task};

use super::common::{gated, switch_row};
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
        ClockHint => "Click the clock to open the calendar and notifications", "Клацніть годинник, щоб відкрити календар і сповіщення";
        ShowDate => "Show the date", "Показувати дату";
        ShowDateHint => "Weekday, day and month next to the time", "День тижня, число й місяць біля часу";
        TimeFormat => "Time format", "Формат часу";
        TimeFormatAuto => "Follows the region: 12-hour only for US English", "За регіоном: 12-годинний лише для американської англійської";
        TimeFormat24 => "Always 24-hour, like 14:05", "Завжди 24-годинний, як-от 14:05";
        TimeFormat12 => "Always 12-hour, like 2:05 PM", "Завжди 12-годинний, як-от 2:05 PM";
        Auto => "Automatic", "Автоматично";
        Hours24 => "24-hour", "24 години";
        Hours12 => "12-hour", "12 годин";
        Status => "Status area", "Область стану";
        StatusHint => "The right-hand side of the bar", "Права частина панелі";
        BatteryPercent => "Battery percentage", "Відсоток заряду";
        BatteryPercentHint => "Show the charge as a number next to the battery icon", "Показувати заряд числом біля значка батареї";
        Tray => "App status icons", "Значки стану програм";
        TrayHint => "Tray icons of apps such as Telegram, Steam or Nextcloud", "Значки в треї від програм на кшталт Telegram, Steam чи Nextcloud";
        Menus => "App menus", "Меню програм";
        MenusHint => "Menus of the focused app sit next to its name, as on macOS", "Меню активної програми стоять поруч із її назвою, як у macOS";
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
    pub fn uses_24h(self) -> bool {
        match self {
            Self::Hours24 => true,
            Self::Hours12 => false,
            Self::Auto => !crate::services::timedate::env_uses_12_hour(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Msg {
    ShowDate(bool),
    Format(TimeFormat),
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
            Msg::ShowDate(on) => config.set_bool("topbar", "clock-date", on),
            Msg::Format(TimeFormat::Auto) => config.remove("topbar", "clock-24h"),
            Msg::Format(TimeFormat::Hours24) => config.set_bool("topbar", "clock-24h", true),
            Msg::Format(TimeFormat::Hours12) => config.set_bool("topbar", "clock-24h", false),
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
            .push(container(bar_preview(&c, config)).padding(8));
        let page = Column::new().spacing(theme::SECTION_GAP).push(preview.build());
        let format = TimeFormat::from_config(config);
        let formats = vec![
            (TimeFormat::Auto, c.t(tr::Key::Auto).to_string()),
            (TimeFormat::Hours24, c.t(tr::Key::Hours24).to_string()),
            (TimeFormat::Hours12, c.t(tr::Key::Hours12).to_string()),
        ];
        let clock = Group::new(t)
            .title(c.t(tr::Key::Clock))
            .description(c.t(tr::Key::ClockHint))
            .push(switch_row(c.t(tr::Key::ShowDate), c.t(tr::Key::ShowDateHint), "clock-date", Msg::ShowDate))
            .push(
                Row::new(t, c.t(tr::Key::TimeFormat))
                    .subtitle(c.t(match format {
                        TimeFormat::Auto => tr::Key::TimeFormatAuto,
                        TimeFormat::Hours24 => tr::Key::TimeFormat24,
                        TimeFormat::Hours12 => tr::Key::TimeFormat12,
                    }))
                    .suffix(gated(c, widgets::segmented(t, formats, format, Msg::Format)))
                    .build(),
            );
        let status = Group::new(t)
            .title(c.t(tr::Key::Status))
            .description(c.t(tr::Key::StatusHint))
            .push(switch_row(
                c.t(tr::Key::BatteryPercent),
                c.t(tr::Key::BatteryPercentHint),
                "battery-percentage",
                Msg::Battery,
            ))
            .push(switch_row(c.t(tr::Key::Tray), c.t(tr::Key::TrayHint), "tray", Msg::Tray));
        let menus = Group::new(t)
            .title(c.t(tr::Key::Menus))
            .description(c.t(tr::Key::MenusHint))
            .push(switch_row(c.t(K::GlobalMenu), c.t(K::GlobalMenuHint), "global-menu", Msg::GlobalMenu))
            .push(switch_row(
                c.t(K::AppMenuRegistrar),
                c.t(K::AppMenuRegistrarHint),
                "appmenu-registrar",
                Msg::Registrar,
            ));
        page.push(clock.build()).push(status.build()).push(menus.build()).into()
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
    let bar = sketch::bar(
        t,
        30.0,
        row![
            container(left).width(Length::FillPortion(1)),
            container(clock).center_x(Length::Shrink),
            container(right).width(Length::FillPortion(1)).align_x(Alignment::End),
        ]
        .spacing(12)
        .align_y(Alignment::Center),
    );
    sketch::screen(
        t,
        Length::Fill,
        96.0,
        vec![
            iced::widget::column![bar, space().height(Fill)].into(),
            sketch::at(40.0, 44.0, sketch::window(t, 180.0, 70.0)),
        ],
    )
}

use glide_sdk::theme::TextStyle;
use glide_sdk::widget::text;
use iced::widget::{Row as IcedRow, column, row};
use iced::{Alignment, Element, Fill, Task};

use super::common::{error_banner, gated, toggle};
use super::{Ctx, Pane, Panel};
use crate::i18n::Key as K;
use crate::icons::Icon;
use crate::services::config::{ConfigStore, DEFAULT_BATTERY_LOW, DEFAULT_BLANK_AFTER};
use crate::services::power::{self, BatteryDetails, LidAction, PowerState, Profile};
use crate::services::run::{Debounce, Jobs, blocking};
use crate::theme::{self, Accent, Tone};
use crate::widgets::{self, Group, Row};

mod tr {
    crate::i18n::catalogue! {
        Left => "left", "залишилося";
        UntilFull => "until full", "до повного заряду";
        NotCharging => "Not charging", "Не заряджається";
        Health => "Health", "Стан";
        Cycles => "Charge cycles", "Цикли заряду";
        Draw => "Power draw", "Споживання";
        Capacity => "Capacity", "Ємність";
        BatteryInfo => "Battery", "Батарея";
        Manufacturer => "Manufacturer", "Виробник";
        Model => "Model", "Модель";
        Technology => "Chemistry", "Тип";
        DesignCapacity => "Design capacity", "Паспортна ємність";
        FullCapacity => "Full charge capacity", "Ємність повного заряду";
        SaverHint => "Longer battery life, slower", "Довше від батареї, але повільніше";
        BalancedHint => "The usual trade-off", "Звичайний баланс";
        PerformanceHint => "Fastest, uses more power", "Найшвидше, але більше енергії";
        ShowPercentage => "Battery percentage in the top bar", "Відсоток заряду у верхній панелі";
        ShowPercentageHint => "Next to the battery icon", "Поруч зі значком батареї";
        LidAndButtons => "Lid & power button", "Кришка й кнопка живлення";
        LidHint => "The lid is handled by systemd-logind (/etc/systemd/logind.conf). The power button is handled by Glide.", "Кришкою керує systemd-logind (/etc/systemd/logind.conf). Кнопку живлення обробляє Glide.";
        ActAsk => "Asks what to do", "Запитує, що зробити";
        SleepIdle => "Sleep when idle", "Сон за бездіяльності";
        SleepIdleHint => "While plugged in", "Під час живлення від мережі";
        SleepIdleBattery => "Sleep when idle on battery", "Сон за бездіяльності від батареї";
        SleepIdleBatteryHint => "Saves charge when you step away", "Заощаджує заряд, коли ви відійшли";
        LidClosed => "Closing the lid", "Закриття кришки";
        LidClosedAc => "Closing the lid while plugged in", "Закриття кришки під час заряджання";
        LidDocked => "Closing the lid with a monitor attached", "Закриття кришки з під'єднаним монітором";
        PowerButton => "Pressing the power button", "Натискання кнопки живлення";
        ActIgnore => "Does nothing", "Нічого не відбувається";
        ActSuspend => "Sleep", "Сон";
        ActHibernate => "Hibernate", "Глибокий сон";
        ActHybrid => "Hybrid sleep", "Гібридний сон";
        ActSuspendHibernate => "Sleep, then hibernate", "Сон, потім глибокий сон";
        ActLock => "Lock the screen", "Блокування екрана";
        ActPowerOff => "Shut down", "Вимкнення";
        ActOther => "Custom", "Інша дія";
        Hours => "h", "год";
        Watts => "W", "Вт";
        WattHours => "Wh", "Вт·год";
        NoBattery => "This computer runs on mains power", "Цей комп'ютер живиться від мережі";
        NoBatteryHint => "There is no battery to show", "Батареї немає";
        Charging => "Charging", "Заряджання";
        PreserveHealth => "Preserve battery health", "Берегти батарею";
        PreserveHealthHint => "Stops charging at {}% so the battery wears out more slowly", "Зупиняє заряджання на {}%, щоб батарея повільніше зношувалася";
    }
}

pub(crate) const BLANK_CHOICES: &[i64] = &[60, 120, 300, 600, 900, 1800, 0];
pub(crate) const SUSPEND_CHOICES: &[i64] = &[900, 1800, 3600, 7200, 0];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Blank {
    pub seconds: i64,
    pub label: String,
}

impl std::fmt::Display for Blank {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

#[derive(Debug, Clone)]
pub enum Msg {
    Loaded(u64, PowerState),
    Blank(Blank),
    Suspend(Blank),
    SuspendOnBattery(Blank),
    BatteryLow(f64),
    CommitBatteryLow,
    SettleBatteryLow(u64),
    Profile(Profile),
    ShowPercentage(bool),
    ChargeLimit(bool),
    Done(Option<Result<String, String>>),
}

#[derive(Debug, Default)]
pub struct Page {
    state: PowerState,
    loaded: bool,
    jobs: Jobs,
    battery_low: Option<f64>,
    settle: Debounce,
}

#[must_use]
pub fn blank_label(seconds: i64, c: Ctx) -> String {
    if seconds <= 0 {
        c.t(K::Never).to_string()
    } else if seconds % 3600 == 0 {
        format!("{} {}", seconds / 3600, c.t(tr::Key::Hours))
    } else {
        format!("{} {}", seconds / 60, c.t(K::MinutesShort))
    }
}

#[must_use]
pub fn duration_label(hours: f64, c: Ctx) -> String {
    let minutes = (hours * 60.0).round().max(0.0) as u64;
    let (h, m) = (minutes / 60, minutes % 60);
    if h == 0 {
        format!("{m} {}", c.t(K::MinutesShort))
    } else {
        format!("{h} {} {m:02} {}", c.t(tr::Key::Hours), c.t(K::MinutesShort))
    }
}

impl Pane for Page {
    type Msg = Msg;

    const KEYWORDS: &'static [&'static str] = &[
        "power",
        "battery",
        "blank",
        "screen off",
        "sleep",
        "profile",
        "lid",
        "health",
        "cycles",
        "живлення",
        "батаре",
        "акумулятор",
        "вимкнення екрана",
        "енергія",
        "кришка",
        "заряд",
    ];
    const POLLS: bool = true;

    fn open(&mut self, _c: Ctx, _config: &ConfigStore) -> Task<Msg> {
        if self.loaded || self.jobs.loading {
            return Task::none();
        }
        self.refresh()
    }

    fn tick(&mut self, _c: Ctx) -> Task<Msg> {
        self.refresh()
    }

    fn update(&mut self, msg: Msg, _c: Ctx, config: &mut ConfigStore) -> Task<Msg> {
        match msg {
            Msg::Loaded(generation, _) if !self.jobs.generation.is_current(generation) => {}
            Msg::Loaded(_, state) => {
                self.state = state;
                self.loaded = true;
                self.jobs.loading = false;
            }
            Msg::Blank(choice) => config.set_int("general", "blank-after-seconds", choice.seconds),
            Msg::Suspend(choice) => config.set_int("general", "suspend-after-seconds", choice.seconds),
            Msg::SuspendOnBattery(choice) => {
                config.set_int("general", "suspend-on-battery-after-seconds", choice.seconds);
            }
            Msg::BatteryLow(value) => {
                self.battery_low = Some(value);
                return self.settle.schedule(Msg::SettleBatteryLow);
            }
            Msg::SettleBatteryLow(generation) if !self.settle.is_current(generation) => {}
            Msg::CommitBatteryLow | Msg::SettleBatteryLow(_) => {
                self.settle.cancel();
                if let Some(value) = self.battery_low.take() {
                    config.set_int("topbar", "battery-low-pct", value.round() as i64);
                }
            }
            Msg::ShowPercentage(on) => config.set_bool("topbar", "battery-percentage", on),
            Msg::Profile(profile) => {
                self.state.profile = Some(profile);
                self.jobs.begin(None);
                return Task::perform(blocking(move || power::set_profile(profile)), Msg::Done);
            }
            Msg::ChargeLimit(enabled) => {
                let Some(limit) = self.state.charge_limit.as_mut() else {
                    return Task::none();
                };
                limit.enabled = enabled;
                let path = limit.path.clone();
                self.jobs.begin(None);
                return Task::perform(blocking(move || power::set_charge_limit(&path, enabled)), Msg::Done);
            }
            Msg::Done(result) => {
                self.jobs.finish(result);
                return self.refresh();
            }
        }
        Task::none()
    }

    fn view<'a>(&'a self, c: Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let t = c.theme;
        let mut page = column![error_banner(t, self.jobs.error.as_deref())].spacing(theme::SECTION_GAP);
        if self.state.battery.is_some() {
            page = page.push(self.battery_hero(&c, config));
            if let Some(limit) = &self.state.charge_limit {
                let hint = c.t(tr::Key::PreserveHealthHint).replace("{}", &limit.end.to_string());
                let row = Row::new(t, c.t(tr::Key::PreserveHealth))
                    .subtitle(hint)
                    .suffix(toggle(c, limit.enabled, Msg::ChargeLimit))
                    .build();
                page = page.push(Group::new(t).title(c.t(tr::Key::Charging)).push(row).build());
            }
            if let Some(details) = &self.state.details {
                if let Some(stats) = stats(&c, details) {
                    page = page.push(stats);
                }
                if let Some(info) = info(&c, details) {
                    page = page.push(info);
                }
            }
        } else if self.loaded {
            page = page.push(widgets::section_hero(
                t,
                Icon::Battery,
                Panel::Power.color(),
                c.t(tr::Key::NoBattery),
                c.t(tr::Key::NoBatteryHint),
                None,
            ));
        }
        if self.state.profiles_available {
            page = page.push(self.profile_group(&c));
        }
        page = page.push(self.saving_group(&c, config));
        if let Some(policy) = self.state.logind {
            page = page.push(self.lid_group(&c, policy));
        }
        page.into()
    }
}

impl Page {
    fn refresh(&mut self) -> Task<Msg> {
        let Some(generation) = self.jobs.start_loading() else { return Task::none() };
        Task::perform(blocking(power::read_state), move |state| Msg::Loaded(generation, state.unwrap_or_default()))
    }

    fn battery_hero<'a>(&'a self, c: &Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let t = c.theme;
        let Some(battery) = self.state.battery else {
            return widgets::gap(0.0);
        };
        let low = config.int_or("topbar", "battery-low-pct", DEFAULT_BATTERY_LOW);
        let is_low = i64::from(battery.percent) <= low && !battery.charging;
        let status_text = self.state.details.as_ref().map(|d| d.status.as_str());
        let (status, tone) = if battery.full {
            (c.t(K::BatteryFull), Tone::Ok)
        } else if battery.charging {
            (c.t(K::Charging), Tone::Accent)
        } else if status_text == Some("Not charging") {
            (c.t(tr::Key::NotCharging), Tone::Neutral)
        } else if self.state.on_ac {
            (c.t(K::PluggedIn), Tone::Neutral)
        } else {
            (c.t(K::OnBattery), if is_low { Tone::Danger } else { Tone::Neutral })
        };
        let color = if is_low { theme::ramp_color(Accent::Cinnabar, 3) } else { Panel::Power.color() };
        let estimate = self.state.details.as_ref().and_then(BatteryDetails::hours_left).map(|hours| {
            let suffix = if battery.charging { c.t(tr::Key::UntilFull) } else { c.t(tr::Key::Left) };
            format!("{} {suffix}", duration_label(hours, *c))
        });
        let source = if self.state.on_ac { K::PluggedIn } else { K::OnBattery };
        let subtitle = estimate.unwrap_or_else(|| c.t(source).to_string());
        let meter = glide_sdk::widget::mock::battery(t, f32::from(battery.percent) / 100.0, color);
        let mut body = column![
            row![
                text(
                    format!("{}%", battery.percent),
                    TextStyle { size_px: 34.0, line_height_px: 40.0, ..TextStyle::DISPLAY }
                )
                .color(t.fg_1()),
                widgets::status_pill(t, tone, status),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
        ]
        .spacing(4)
        .width(Fill);
        if subtitle != status {
            body = body.push(text(subtitle, TextStyle::CALL).color(t.fg_2()));
        }
        glide_sdk::widget::tinted_card(
            &widgets::paint(&t),
            color,
            20,
            row![meter, body].spacing(20).align_y(Alignment::Center),
        )
    }

    fn profile_group(&self, c: &Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        let current = self.state.profile.unwrap_or(Profile::Balanced);
        let cards = [
            (
                Profile::PowerSaver,
                Icon::Battery,
                theme::ramp_color(Accent::Conifer, 3),
                K::PowerSaver,
                tr::Key::SaverHint,
            ),
            (
                Profile::Balanced,
                Icon::Sparkle,
                theme::ramp_color(Accent::Azurit, 3),
                K::Balanced,
                tr::Key::BalancedHint,
            ),
            (
                Profile::Performance,
                Icon::Rocket,
                theme::ramp_color(Accent::Terracotta, 3),
                K::Performance,
                tr::Key::PerformanceHint,
            ),
        ];
        let mut line = IcedRow::new().spacing(12);
        for (profile, glyph, color, title, hint) in cards {
            line = line.push(widgets::radio_card(
                t,
                widgets::tile(glyph, color, 40.0),
                c.t(title),
                Some(c.t(hint).to_string()),
                profile == current,
                Msg::Profile(profile),
            ));
        }
        column![Group::new(t).title(c.t(K::PowerMode)).description(c.t(K::PowerModeHint)).build(), line,]
            .spacing(8)
            .into()
    }

    fn saving_group<'a>(&'a self, c: &Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let t = c.theme;
        let timeout = |key: &str, fallback: i64, choices: &[i64]| {
            let current = config.int_or("general", key, fallback);
            let labels: Vec<Blank> = (!choices.contains(&current))
                .then_some(current)
                .into_iter()
                .chain(choices.iter().copied())
                .map(|seconds| Blank { seconds, label: blank_label(seconds, *c) })
                .collect();
            let selected = labels.iter().find(|blank| blank.seconds == current).cloned();
            (labels, selected)
        };
        let (labels, selected) = timeout("blank-after-seconds", DEFAULT_BLANK_AFTER, BLANK_CHOICES);
        let (sleep_labels, sleep_selected) = timeout("suspend-after-seconds", 0, SUSPEND_CHOICES);
        let low = self.battery_low.unwrap_or(config.int_or("topbar", "battery-low-pct", DEFAULT_BATTERY_LOW) as f64);
        let moon = || widgets::tile(Icon::Moon, theme::ramp_color(Accent::Azurit, 4), theme::SMALL_CONTROL_HEIGHT);
        let mut saving = Group::new(t)
            .title(c.t(K::PowerSaving))
            .push(
                Row::new(t, c.t(K::BlankScreen))
                    .subtitle(c.t(K::BlankScreenHint))
                    .prefix(widgets::tile(
                        Icon::Display,
                        theme::ramp_color(Accent::Malachite, 3),
                        theme::SMALL_CONTROL_HEIGHT,
                    ))
                    .suffix(gated(*c, widgets::dropdown(t, labels, selected, Msg::Blank, 160.0)))
                    .build(),
            )
            .push(
                Row::new(t, c.t(tr::Key::SleepIdle))
                    .subtitle(c.t(tr::Key::SleepIdleHint))
                    .prefix(moon())
                    .suffix(gated(*c, widgets::dropdown(t, sleep_labels, sleep_selected, Msg::Suspend, 160.0)))
                    .build(),
            );
        if self.state.battery.is_some() || !self.loaded {
            let (labels, selected) = timeout("suspend-on-battery-after-seconds", 0, SUSPEND_CHOICES);
            saving = saving
                .push(
                    Row::new(t, c.t(tr::Key::SleepIdleBattery))
                        .subtitle(c.t(tr::Key::SleepIdleBatteryHint))
                        .prefix(moon())
                        .suffix(gated(*c, widgets::dropdown(t, labels, selected, Msg::SuspendOnBattery, 160.0)))
                        .build(),
                )
                .push(
                    Row::new(t, c.t(K::BatteryWarning))
                        .subtitle(format!("{low:.0}%"))
                        .prefix(widgets::tile(
                            Icon::Warning,
                            theme::ramp_color(Accent::Amber, 3),
                            theme::SMALL_CONTROL_HEIGHT,
                        ))
                        .suffix(gated(
                            *c,
                            widgets::flexible(
                                widgets::value_slider(
                                    t,
                                    5.0..=50.0,
                                    low,
                                    1.0,
                                    Msg::BatteryLow,
                                    Some(Msg::CommitBatteryLow),
                                ),
                                240.0,
                            ),
                        ))
                        .build(),
                )
                .push(
                    Row::new(t, c.t(tr::Key::ShowPercentage))
                        .subtitle(c.t(tr::Key::ShowPercentageHint))
                        .prefix(widgets::tile(
                            Icon::TopBar,
                            theme::ramp_color(Accent::Amethyst, 4),
                            theme::SMALL_CONTROL_HEIGHT,
                        ))
                        .suffix(toggle(*c, config.bool_or("topbar", "battery-percentage", true), Msg::ShowPercentage))
                        .build(),
                );
        }
        saving.build()
    }

    fn lid_group(&self, c: &Ctx, policy: power::LogindPolicy) -> Element<'_, Msg> {
        let t = c.theme;
        let lid = [
            (tr::Key::LidClosed, policy.lid),
            (tr::Key::LidClosedAc, policy.lid_on_ac),
            (tr::Key::LidDocked, policy.lid_docked),
        ];
        let rows = lid
            .into_iter()
            .filter(|_| self.state.has_lid)
            .map(|(key, action)| (key, action_label(action, c)))
            .chain([(tr::Key::PowerButton, c.t(tr::Key::ActAsk))])
            .map(|(key, label)| Row::new(t, c.t(key)).suffix(text(label, TextStyle::CALL).color(t.fg_2())).build());
        Group::new(t).title(c.t(tr::Key::LidAndButtons)).description(c.t(tr::Key::LidHint)).extend(rows).build()
    }
}

fn action_label(action: LidAction, c: &Ctx) -> &'static str {
    c.t(match action {
        LidAction::Ignore => tr::Key::ActIgnore,
        LidAction::Suspend => tr::Key::ActSuspend,
        LidAction::Hibernate => tr::Key::ActHibernate,
        LidAction::HybridSleep => tr::Key::ActHybrid,
        LidAction::SuspendThenHibernate => tr::Key::ActSuspendHibernate,
        LidAction::Lock => tr::Key::ActLock,
        LidAction::PowerOff => tr::Key::ActPowerOff,
        LidAction::Other => tr::Key::ActOther,
    })
}

fn stats<'a>(c: &Ctx, details: &BatteryDetails) -> Option<Element<'a, Msg>> {
    let health = details.health().map(|health| {
        let tone = if health >= 80.0 {
            Accent::Conifer
        } else if health >= 60.0 {
            Accent::Amber
        } else {
            Accent::Cinnabar
        };
        (Icon::Shield, tone, format!("{health:.0}%"), tr::Key::Health)
    });
    let tiles: Vec<Element<'a, Msg>> = [
        health,
        details.cycles.map(|cycles| (Icon::Refresh, Accent::Azurit, cycles.to_string(), tr::Key::Cycles)),
        details.power_w.filter(|power| *power > 0.05).map(|power| {
            let value = format!("{power:.1} {}", c.t(tr::Key::Watts));
            (Icon::Sparkle, Accent::Amber, value, tr::Key::Draw)
        }),
        details.energy_full_wh.map(|full| {
            let value = format!("{full:.1} {}", c.t(tr::Key::WattHours));
            (Icon::Battery, Accent::Malachite, value, tr::Key::Capacity)
        }),
    ]
    .into_iter()
    .flatten()
    .map(|(glyph, accent, value, label)| {
        widgets::stat_tile(c.theme, glyph, theme::ramp_color(accent, 3), value, c.t(label))
    })
    .collect();
    (!tiles.is_empty()).then(|| widgets::stat_row(c.theme, tiles))
}

fn info<'a>(c: &Ctx, details: &BatteryDetails) -> Option<Element<'a, Msg>> {
    let t = c.theme;
    let wh = c.t(tr::Key::WattHours);
    let rows: Vec<(String, String)> = [
        (tr::Key::Manufacturer, details.manufacturer.clone()),
        (tr::Key::Model, details.model.clone()),
        (tr::Key::Technology, details.technology.clone()),
    ]
    .into_iter()
    .filter(|(_, value)| !value.trim().is_empty())
    .chain(
        [(tr::Key::DesignCapacity, details.energy_design_wh), (tr::Key::FullCapacity, details.energy_full_wh)]
            .into_iter()
            .filter_map(|(key, energy)| energy.map(|energy| (key, format!("{energy:.1} {wh}")))),
    )
    .map(|(key, value)| (c.t(key).to_string(), value))
    .collect();
    (!rows.is_empty()).then(|| {
        column![Group::new(t).title(c.t(tr::Key::BatteryInfo)).build(), widgets::info_grid(t, rows),].spacing(8).into()
    })
}

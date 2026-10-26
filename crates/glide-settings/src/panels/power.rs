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
        ShowPercentageHint => "Set on the Top Bar page", "Налаштовується на сторінці «Верхня панель»";
        Shown => "Shown", "Показується";
        Hidden => "Hidden", "Приховано";
        BlankLink => "Turning off the screen and locking", "Гасіння екрана і блокування";
        BlankLinkHint => "Set on the Lock Screen page", "Налаштовується на сторінці «Екран блокування»";
        Away => "What happens when you step away", "Що відбувається, коли ви відходите";
        ChargingWarnings => "Charging and warnings", "Заряджання й попередження";
        NeedsAdmin => "asks for an administrator password", "потрібен пароль адміністратора";
        BatteryWarningHint => "A critical warning always comes at 5 %", "Критичне попередження завжди на 5 %";
        AwayHint => "One chain: the screen goes dark, the session locks, the computer sleeps", "Один ланцюжок: екран гасне, сеанс блокується, комп’ютер засинає";
        StageBlank => "screen off", "екран гасне";
        StageLock => "lock", "блокування";
        BlankAndLock => "screen off and lock", "гасне й блокується";
        StageSleep => "sleep", "сон";
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
    Suspend(Blank),
    SuspendOnBattery(Blank),
    BatteryLow(f64),
    CommitBatteryLow,
    SettleBatteryLow(u64),
    Profile(Profile),
    ChargeLimit(bool),
    Done(Option<Result<String, String>>),
    Open(Panel),
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
            Msg::Open(_) => {}
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
        let battery = self.state.battery.is_some();
        if battery {
            page = page.push(self.battery_hero(&c, config));
            if let Some(stats) = self.state.details.as_ref().and_then(|details| stats(&c, details)) {
                page = page.push(stats);
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
        page = page.push(self.away_group(&c, config));
        if battery || !self.loaded {
            page = page.push(self.charging_group(&c, config));
        }
        if let Some(policy) = self.state.logind {
            page = page.push(self.lid_group(&c, policy));
        }
        if let Some(info) = self.state.details.as_ref().filter(|_| battery).and_then(|details| info(&c, details)) {
            page = page.push(info);
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

    fn sleep_rows<'a>(&'a self, c: &Ctx, config: &'a ConfigStore) -> Vec<Element<'a, Msg>> {
        let t = c.theme;
        let timeout = |key: &str, choices: &[i64]| {
            let current = config.int_or("general", key, 0);
            let labels: Vec<Blank> = (!choices.contains(&current))
                .then_some(current)
                .into_iter()
                .chain(choices.iter().copied())
                .map(|seconds| Blank { seconds, label: blank_label(seconds, *c) })
                .collect();
            let selected = labels.iter().find(|blank| blank.seconds == current).cloned();
            (labels, selected, current)
        };
        let (labels, selected, current) = timeout("suspend-after-seconds", SUSPEND_CHOICES);
        let mut rows = vec![
            Row::new(t, c.t(tr::Key::SleepIdle))
                .subtitle(c.t(tr::Key::SleepIdleHint))
                .changed(current != 0)
                .suffix(gated(*c, widgets::dropdown(t, labels, selected, Msg::Suspend, 160.0)))
                .build(),
        ];
        if self.state.battery.is_some() || !self.loaded {
            let (labels, selected, current) = timeout("suspend-on-battery-after-seconds", SUSPEND_CHOICES);
            rows.push(
                Row::new(t, c.t(tr::Key::SleepIdleBattery))
                    .subtitle(c.t(tr::Key::SleepIdleBatteryHint))
                    .changed(current != 0)
                    .suffix(gated(*c, widgets::dropdown(t, labels, selected, Msg::SuspendOnBattery, 160.0)))
                    .build(),
            );
        }
        rows.push(lock_link(c, config));
        rows
    }

    fn charging_group<'a>(&'a self, c: &Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let t = c.theme;
        let stored = config.int_or("topbar", "battery-low-pct", DEFAULT_BATTERY_LOW);
        let low = self.battery_low.unwrap_or(stored as f64);
        let mut group = Group::new(t).title(c.t(tr::Key::ChargingWarnings));
        if let Some(limit) = &self.state.charge_limit {
            let hint = c.t(tr::Key::PreserveHealthHint).replace("{}", &limit.end.to_string());
            group = group.push(
                Row::new(t, c.t(tr::Key::PreserveHealth))
                    .subtitle(hint)
                    .suffix(
                        row![
                            glide_sdk::widget::applies_chip(&widgets::paint(&t), c.t(tr::Key::NeedsAdmin), false),
                            toggle(*c, limit.enabled, Msg::ChargeLimit),
                        ]
                        .spacing(8)
                        .align_y(Alignment::Center),
                    )
                    .build(),
            );
        }
        group
            .push(
                Row::new(t, c.t(K::BatteryWarning))
                    .subtitle(c.t(tr::Key::BatteryWarningHint))
                    .changed(stored != DEFAULT_BATTERY_LOW)
                    .suffix(gated(
                        *c,
                        glide_sdk::widget::slider_readout(
                            &widgets::paint(&t),
                            None,
                            widgets::value_slider(
                                t,
                                5.0..=50.0,
                                low,
                                1.0,
                                Msg::BatteryLow,
                                Some(Msg::CommitBatteryLow),
                            ),
                            format!("{low:.0} %"),
                            false,
                        ),
                    ))
                    .stacked(t.compact)
                    .build(),
            )
            .push(percentage_link(c, config))
            .build()
    }

    fn away_group<'a>(&'a self, c: &Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let t = c.theme;
        let blank = config.int_or("general", "blank-after-seconds", DEFAULT_BLANK_AFTER).max(0);
        let lock_after = config.int_or("general", "lock-after-seconds", 0);
        let on_ac = config.int_or("general", "suspend-after-seconds", 0).max(0);
        let on_battery = config.int_or("general", "suspend-on-battery-after-seconds", 0).max(0);
        let locks_at = (blank > 0 && lock_after >= 0).then_some(blank + lock_after.max(0));
        let span =
            [blank, locks_at.unwrap_or(0), on_ac, on_battery].into_iter().max().unwrap_or(0).max(60) as f32 * 1.2;
        let lane = |suspend: i64| {
            let mut stages = Vec::new();
            if blank > 0 {
                let label = if locks_at == Some(blank) {
                    format!("{} · {}", blank_label(blank, *c), c.t(tr::Key::BlankAndLock))
                } else {
                    format!("{} · {}", blank_label(blank, *c), c.t(tr::Key::StageBlank))
                };
                stages.push(glide_sdk::widget::Stage::new(blank as f32 / span, label, t.acc_text()));
            }
            if let Some(at) = locks_at.filter(|at| *at != blank) {
                let label = format!("+{} · {}", blank_label(at - blank, *c), c.t(tr::Key::StageLock));
                stages.push(glide_sdk::widget::Stage::new(
                    at as f32 / span,
                    label,
                    theme::ramp_color(Accent::Amethyst, 3),
                ));
            }
            if suspend > 0 {
                let label = format!("{} · {}", blank_label(suspend, *c), c.t(tr::Key::StageSleep));
                stages.push(glide_sdk::widget::Stage::new(suspend as f32 / span, label, t.ok()));
            }
            glide_sdk::widget::stage_track(&widgets::paint(&t), &stages)
        };
        let mut group = Group::new(t)
            .title(c.t(tr::Key::Away))
            .description(c.t(tr::Key::AwayHint))
            .push(Row::new(t, c.t(K::PluggedIn)).stacked(true).suffix(lane(on_ac)).build());
        if self.state.battery.is_some() {
            group = group.push(Row::new(t, c.t(K::OnBattery)).stacked(true).suffix(lane(on_battery)).build());
        }
        group.extend(self.sleep_rows(c, config)).build()
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

fn lock_link<'a>(c: &Ctx, config: &ConfigStore) -> Element<'a, Msg> {
    let t = c.theme;
    let blank = config.int_or("general", "blank-after-seconds", DEFAULT_BLANK_AFTER);
    let lock_after = config.int_or("general", "lock-after-seconds", 0);
    let value = format!(
        "{} · {}",
        crate::panels::lock_screen::blank_label(c, blank),
        crate::panels::lock_screen::timeout_label(c, lock_after)
    );
    Row::new(t, c.t(tr::Key::BlankLink))
        .subtitle(c.t(tr::Key::BlankLinkHint))
        .prefix(widgets::tile(Icon::Lock, theme::ramp_color(Accent::Cinnabar, 2), theme::SMALL_CONTROL_HEIGHT))
        .suffix(widgets::disclosure(t, Some(value)))
        .on_press(Msg::Open(Panel::LockScreen))
        .build()
}

fn percentage_link<'a>(c: &Ctx, config: &ConfigStore) -> Element<'a, Msg> {
    let t = c.theme;
    let shown = config.bool_or("topbar", "battery-percentage", true);
    Row::new(t, c.t(tr::Key::ShowPercentage))
        .subtitle(c.t(tr::Key::ShowPercentageHint))
        .prefix(widgets::tile(Icon::TopBar, theme::ramp_color(Accent::Amethyst, 4), theme::SMALL_CONTROL_HEIGHT))
        .suffix(widgets::disclosure(t, Some(c.t(if shown { tr::Key::Shown } else { tr::Key::Hidden }).to_string())))
        .on_press(Msg::Open(Panel::TopBar))
        .build()
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

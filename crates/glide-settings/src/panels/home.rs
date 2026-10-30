use glide_config::toolkits::Support;
use glide_sdk::widget::{self as sdk, Glance, Issue, Severity};
use iced::widget::Column;
use iced::{Element, Task};

use super::{Ctx, Pane, Panel};
use crate::icons::Icon;
use crate::services::audio::{self, AudioState};
use crate::services::config::ConfigStore;
use crate::services::network::{self, NetworkState, Rescan};
use crate::services::power::{self, PowerState};
use crate::services::run::{Generation, blocking};
use crate::services::storage::{self, Volume};
use crate::services::system::format_bytes;
use crate::theme;
use crate::widgets::{self, Group, Row};

mod tr {
    crate::i18n::catalogue! {
        Attention => "Needs attention", "Потребує уваги";
        AllGood => "Everything is in order", "Усе гаразд";
        AllGoodHint => "Nothing on this computer needs your attention", "На цьому комп’ютері нічого не потребує уваги";
        Now => "Now", "Зараз";
        More => "Details", "Детальніше";
        Less => "Hide details", "Сховати подробиці";
        ReadOnly => "Settings cannot be saved", "Налаштування не вдається зберегти";
        ReadOnlyHint => "The settings file has a mistake, so changes made here would be lost.", "У файлі налаштувань помилка, тож зміни звідси загубилися б.";
        Gtk3 => "Some apps do not take Glide's colours", "Деякі програми не підхоплюють кольори Glide";
        Gtk3Hint => "Chrome, Electron and older GTK apps stay in their stock colours.", "Chrome, Electron і старіші GTK-програми лишаються у стандартних кольорах.";
        Gtk3Detail => "The adw-gtk3 theme is missing. Install the package adw-gtk-theme.", "Бракує теми adw-gtk3. Встановіть пакет adw-gtk-theme.";
        Qt => "Qt apps do not take Glide's colours", "Програми Qt не підхоплюють кольори Glide";
        QtHint => "KDE and other Qt apps keep their own palette.", "KDE та інші Qt-програми лишаються зі своєю палітрою.";
        QtDetail => "Install qt6ct, and qt5ct for older Qt 5 apps.", "Встановіть qt6ct, а для старіших програм Qt 5 — qt5ct.";
        LowDisk => "The system disk is almost full", "Системний диск майже заповнений";
        LowDiskHint => "Apps may fail to save files.", "Програми можуть не зберегти файли.";
        Battery => "The battery has worn down", "Батарея зносилася";
        BatteryHint => "It holds noticeably less charge than when new.", "Вона тримає помітно менше заряду, ніж нова.";
        Open => "Open", "Відкрити";
        Network => "Network", "Мережа";
        Sound => "Sound", "Звук";
        Power => "Power", "Живлення";
        Storage => "Storage", "Сховище";
        Offline => "Not connected", "Не підключено";
        WifiOff => "Wi-Fi is off", "Wi-Fi вимкнено";
        Wired => "Wired connection", "Дротове підключення";
        Signal => "Signal", "Сигнал";
        Secured => "secured", "захищена";
        NoSound => "No sound output", "Немає виходу звуку";
        Muted => "Muted", "Вимкнено";
        Mains => "Mains power", "Від мережі";
        Charging => "Charging", "Заряджається";
        OnBattery => "On battery", "Від батареї";
        Full => "Fully charged", "Повністю заряджено";
        Left => "left", "залишилось";
        UntilFull => "until full", "до повного заряду";
        HoursShort => "h", "год";
        MinutesShort => "min", "хв";
        Free => "free", "вільно";
        Of => "of", "з";
        Health => "Battery health", "Стан батареї";
        Loading => "Loading…", "Завантаження…";
    }
}

use tr::Key as T;

#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    network: NetworkState,
    audio: AudioState,
    power: PowerState,
    disk: Option<Volume>,
    toolkits: Option<Support>,
}

fn snapshot() -> Snapshot {
    Snapshot {
        network: network::read(Rescan::No),
        audio: audio::read(),
        power: power::read_state(),
        disk: storage::volumes().into_iter().find(|volume| volume.mount == "/"),
        toolkits: Some(Support::detect()),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Problem {
    ReadOnly,
    Gtk3,
    Qt,
    LowDisk,
    Battery,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Loaded(u64, Box<Snapshot>),
    Toggle(Problem),
    Open(Panel),
}

#[derive(Debug, Default)]
pub struct Page {
    snapshot: Option<Snapshot>,
    open: Vec<Problem>,
    generation: Generation,
    loading: bool,
}

const LOW_DISK: f32 = 0.92;
const WORN_BATTERY: f64 = 70.0;

impl Page {
    fn load(&mut self) -> Task<Msg> {
        self.loading = true;
        let generation = self.generation.bump();
        Task::perform(blocking(snapshot), move |found| Msg::Loaded(generation, Box::new(found.unwrap_or_default())))
    }

    #[must_use]
    pub fn problems(&self, config: &ConfigStore) -> Vec<Problem> {
        let mut found = Vec::new();
        if config.error().is_some() {
            found.push(Problem::ReadOnly);
        }
        found.extend(self.found_problems());
        found
    }

    fn found_problems(&self) -> Vec<Problem> {
        let mut found = Vec::new();
        let Some(snapshot) = &self.snapshot else { return found };
        if let Some(toolkits) = snapshot.toolkits {
            if !toolkits.adw_gtk3 {
                found.push(Problem::Gtk3);
            }
            if toolkits.qt.is_none() {
                found.push(Problem::Qt);
            }
        }
        if snapshot.disk.as_ref().is_some_and(|disk| disk.fraction() >= LOW_DISK) {
            found.push(Problem::LowDisk);
        }
        if snapshot
            .power
            .details
            .as_ref()
            .and_then(power::BatteryDetails::health)
            .is_some_and(|health| health < WORN_BATTERY)
        {
            found.push(Problem::Battery);
        }
        found
    }
}

fn duration(c: &Ctx, hours: f64) -> String {
    let minutes = (hours * 60.0).round() as u64;
    match (minutes / 60, minutes % 60) {
        (0, rest) => format!("{rest} {}", c.t(T::MinutesShort)),
        (whole, 0) => format!("{whole} {}", c.t(T::HoursShort)),
        (whole, rest) => format!("{whole} {} {rest} {}", c.t(T::HoursShort), c.t(T::MinutesShort)),
    }
}

fn issue<'a>(c: &Ctx, config: &'a ConfigStore, problem: Problem, open: bool) -> Element<'a, Msg> {
    let paint = widgets::paint(&c.theme);
    let (title, summary, details, target): (T, T, Vec<String>, Option<Panel>) = match problem {
        Problem::ReadOnly => {
            (T::ReadOnly, T::ReadOnlyHint, config.error().map(str::to_string).into_iter().collect(), None)
        }
        Problem::Gtk3 => (T::Gtk3, T::Gtk3Hint, vec![c.t(T::Gtk3Detail).to_string()], Some(Panel::Appearance)),
        Problem::Qt => (T::Qt, T::QtHint, vec![c.t(T::QtDetail).to_string()], Some(Panel::Appearance)),
        Problem::LowDisk => (T::LowDisk, T::LowDiskHint, Vec::new(), Some(Panel::Storage)),
        Problem::Battery => (T::Battery, T::BatteryHint, Vec::new(), Some(Panel::Power)),
    };
    let severity =
        if matches!(problem, Problem::ReadOnly | Problem::LowDisk) { Severity::Danger } else { Severity::Warn };
    let mut item = Issue::new(&paint, c.t(title))
        .summary(c.t(summary))
        .details(details, open)
        .toggle(Msg::Toggle(problem), c.t(T::More), c.t(T::Less))
        .severity(severity);
    if let Some(panel) = target {
        item = item.action(widgets::secondary(c.theme, c.t(T::Open), Some(Msg::Open(panel))));
    }
    item.build()
}

fn network_tile(c: &Ctx, state: &NetworkState) -> Element<'static, Msg> {
    let paint = widgets::paint(&c.theme);
    let tile =
        Glance::new(&paint, Icon::Wifi, Panel::Network.color(), c.t(T::Network)).on_press(Msg::Open(Panel::Network));
    if let Some(active) = state.active() {
        let level = active.signal.div_ceil(25).min(4);
        let secured = if active.secured { format!(" · {}", c.t(T::Secured)) } else { String::new() };
        return tile
            .aside(sdk::signal_bars(&paint, level, 4))
            .headline(active.ssid.clone())
            .detail(format!("{} {} %{secured}", c.t(T::Signal), active.signal))
            .build();
    }
    if let Some(wired) = state.wired.iter().find(|wired| wired.connected) {
        return tile.headline(c.t(T::Wired)).detail(wired.connection.clone()).build();
    }
    let headline = if state.wifi_enabled { T::Offline } else { T::WifiOff };
    tile.headline(c.t(headline)).build()
}

fn sound_tile(c: &Ctx, state: &AudioState) -> Element<'static, Msg> {
    let paint = widgets::paint(&c.theme);
    let tile = Glance::new(&paint, Icon::Volume, Panel::Sound.color(), c.t(T::Sound)).on_press(Msg::Open(Panel::Sound));
    let Some(sink) = state.default_sink() else { return tile.headline(c.t(T::NoSound)).build() };
    let tile = tile.headline(sink.name.clone());
    match state.output {
        Some(level) if level.muted => tile.detail(c.t(T::Muted)).build(),
        Some(level) => tile
            .meter(level.volume.min(1.0), c.theme.acc_bg())
            .detail(format!("{} %", (level.volume * 100.0).round()))
            .build(),
        None => tile.build(),
    }
}

fn power_tile(c: &Ctx, state: &PowerState) -> Option<Element<'static, Msg>> {
    let paint = widgets::paint(&c.theme);
    let battery = state.battery?;
    let mut headline = format!("{} %", battery.percent);
    if let Some(hours) = state.details.as_ref().and_then(power::BatteryDetails::hours_left).filter(|_| !battery.full) {
        let until = if battery.charging { T::UntilFull } else { T::Left };
        headline = format!("{headline} · {} {}", duration(c, hours), c.t(until));
    }
    let status = if battery.full {
        T::Full
    } else if battery.charging {
        T::Charging
    } else if state.on_ac {
        T::Mains
    } else {
        T::OnBattery
    };
    let mut detail = c.t(status).to_string();
    if let Some(health) = state.details.as_ref().and_then(power::BatteryDetails::health) {
        detail = format!("{detail} · {} {} %", c.t(T::Health), health.round());
    }
    let color = if battery.percent <= 20 { c.theme.danger() } else { c.theme.ok() };
    Some(
        Glance::new(&paint, Icon::Battery, Panel::Power.color(), c.t(T::Power))
            .headline(headline)
            .meter(f32::from(battery.percent) / 100.0, color)
            .detail(detail)
            .on_press(Msg::Open(Panel::Power))
            .build(),
    )
}

fn storage_tile(c: &Ctx, disk: &Volume) -> Element<'static, Msg> {
    let paint = widgets::paint(&c.theme);
    let color = if disk.fraction() >= LOW_DISK { c.theme.danger() } else { c.theme.acc_bg() };
    Glance::new(&paint, Icon::Drive, Panel::Storage.color(), c.t(T::Storage))
        .headline(format!("{} {}", format_bytes(disk.avail, c.lang), c.t(T::Free)))
        .meter(disk.fraction(), color)
        .detail(format!("{} {}", c.t(T::Of), format_bytes(disk.total, c.lang)))
        .on_press(Msg::Open(Panel::Storage))
        .build()
}

impl Page {
    #[must_use]
    pub fn status_of(&self, panel: Panel, c: Ctx) -> Option<String> {
        let snapshot = self.snapshot.as_ref()?;
        match panel {
            Panel::Network => snapshot.network.active().map(|active| active.ssid.clone()).or_else(|| {
                snapshot.network.wired.iter().any(|wired| wired.connected).then(|| c.t(T::Wired).to_string())
            }),
            Panel::Sound => snapshot.audio.output.map(|level| {
                if level.muted { c.t(T::Muted).to_string() } else { format!("{} %", (level.volume * 100.0).round()) }
            }),
            Panel::Power => snapshot.power.battery.map(|battery| format!("{} %", battery.percent)),
            Panel::Storage => snapshot.disk.as_ref().map(|disk| format_bytes(disk.avail, c.lang)),
            _ => None,
        }
    }
}

impl Pane for Page {
    type Msg = Msg;

    const KEYWORDS: &'static [&'static str] =
        &["overview", "home", "status", "summary", "огляд", "стан", "головна", "проблеми"];
    const POLLS: bool = true;

    fn open(&mut self, _c: Ctx, _config: &ConfigStore) -> Task<Msg> {
        self.load()
    }

    fn tick(&mut self, _c: Ctx) -> Task<Msg> {
        if self.loading {
            return Task::none();
        }
        self.load()
    }

    fn update(&mut self, msg: Msg, _c: Ctx, _config: &mut ConfigStore) -> Task<Msg> {
        match msg {
            Msg::Loaded(generation, snapshot) => {
                if self.generation.is_current(generation) {
                    self.loading = false;
                    self.snapshot = Some(*snapshot);
                }
            }
            Msg::Toggle(problem) => {
                if let Some(index) = self.open.iter().position(|open| *open == problem) {
                    self.open.remove(index);
                } else {
                    self.open.push(problem);
                }
            }
            Msg::Open(_) => {}
        }
        Task::none()
    }

    fn sidebar_status(&self, _c: Ctx) -> Option<String> {
        let count = self.found_problems().len();
        (count > 0).then(|| count.to_string())
    }

    fn view<'a>(&'a self, c: Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let t = c.theme;
        let problems = self.problems(config);
        let attention = if problems.is_empty() {
            Group::new(t)
                .title(c.t(T::Attention))
                .push(
                    Row::new(t, c.t(T::AllGood))
                        .subtitle(c.t(T::AllGoodHint))
                        .prefix(widgets::status_dot(t.ok(), 10.0))
                        .build(),
                )
                .build()
        } else {
            Group::new(t)
                .title(c.t(T::Attention))
                .extend(problems.iter().map(|problem| issue(&c, config, *problem, self.open.contains(problem))))
                .build()
        };
        let page = Column::new().spacing(theme::SECTION_GAP).push(attention);
        let Some(snapshot) = &self.snapshot else {
            return page.push(widgets::spinner_label(t, c.t(T::Loading))).into();
        };
        let mut tiles = vec![network_tile(&c, &snapshot.network), sound_tile(&c, &snapshot.audio)];
        tiles.extend(power_tile(&c, &snapshot.power));
        tiles.extend(snapshot.disk.as_ref().map(|disk| storage_tile(&c, disk)));
        let per_line = if t.compact { 1 } else { 2 };
        let now = Column::new()
            .spacing(8)
            .push(sdk::section_heading(&widgets::paint(&t), c.t(T::Now)))
            .push(sdk::grid(tiles, per_line, 12.0));
        page.push(now).into()
    }
}

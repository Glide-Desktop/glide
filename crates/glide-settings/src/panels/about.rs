use glide_sdk::theme::TextStyle;
use glide_sdk::widget::text;
use iced::widget::{column, container, row};
use iced::{Alignment, Element, Fill, Task};

use super::common::error_banner;
use super::{Ctx, Pane};
use crate::i18n::{Key as K, Lang};
use crate::icons::Icon;
use crate::services::config::ConfigStore;
use crate::services::run::{Generation, blocking};
use crate::services::system::{self, Details, format_bytes, format_memory};
use crate::theme::{self, Accent, Tone, ramp_color};
use crate::widgets::{self, Group, Row};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

mod tr {
    crate::i18n::catalogue! {
        CopyReport => "Copy system report", "Скопіювати звіт";
        Display => "Display", "Дисплей";
        CyclesShort => "cycles", "циклів";
        Desktop => "Desktop", "Робочий стіл";
        Copied => "Copied to the clipboard", "Скопійовано в буфер обміну";
        DeviceName => "Device name", "Назва пристрою";
        DeviceNameHint => "Shown to other devices on the network and over Bluetooth", "Її бачать інші пристрої в мережі та через Bluetooth";
        Name => "Name", "Назва";
        NetworkName => "Network name", "Мережева назва";
        Rename => "Rename…", "Перейменувати…";
        Save => "Save", "Зберегти";
        Cancel => "Cancel", "Скасувати";
        InvalidName => "Use letters, digits, spaces or dashes", "Використовуйте літери, цифри, пробіли або дефіси";
        Saving => "Saving…", "Збереження…";
        MemoryInUse => "memory in use", "пам'яті зайнято";
        DiskFree => "free on the system disk", "вільно на системному диску";
        Threads => "threads", "потоків";
        Running => "since the last restart", "без перезавантаження";
        Chassis => "Type", "Тип";
        ChassisLaptop => "Laptop", "Ноутбук";
        ChassisDesktop => "Desktop", "Настільний комп'ютер";
        ChassisTablet => "Tablet", "Планшет";
        ChassisVm => "Virtual machine", "Віртуальна машина";
        ChassisServer => "Server", "Сервер";
        Available => "free", "вільно";
        Battery => "Battery", "Батарея";
        Health => "Health", "Стан";
        Firmware => "Firmware", "Прошивка";
        BiosDate => "Firmware date", "Дата прошивки";
        Board => "Motherboard", "Материнська плата";
    }
}

use tr::Key as T;

#[derive(Debug, Clone)]
pub enum Msg {
    Loaded(u64, Box<Details>),
    CopyReport,
    StartRename,
    Name(String),
    SaveName,
    CancelRename,
    Renamed(Option<Result<(), String>>),
}

#[derive(Debug, Default)]
pub struct Page {
    details: Option<Details>,
    loading: bool,
    editing: Option<String>,
    saving: bool,
    error: Option<String>,
    copied: bool,
    generation: Generation,
}

#[must_use]
pub(crate) fn format_uptime(secs: u64, c: &Ctx) -> String {
    let days = secs / 86_400;
    let hours = secs % 86_400 / 3600;
    let minutes = secs % 3600 / 60;
    if days > 0 {
        format!("{days} {} {hours} {}", c.t(K::DaysShort), c.t(K::HoursShort))
    } else if hours > 0 {
        format!("{hours} {} {minutes} {}", c.t(K::HoursShort), c.t(K::MinutesShort))
    } else {
        format!("{minutes} {}", c.t(K::MinutesShort))
    }
}

fn chassis_key(chassis: &str) -> Option<T> {
    match chassis {
        "laptop" | "convertible" => Some(T::ChassisLaptop),
        "desktop" => Some(T::ChassisDesktop),
        "tablet" | "handset" => Some(T::ChassisTablet),
        "vm" | "container" => Some(T::ChassisVm),
        "server" => Some(T::ChassisServer),
        _ => None,
    }
}

#[must_use]
pub(crate) fn display_line(output: &glide_ipc::OutputInfo, lang: Lang) -> String {
    let mut parts = vec![format!("{}×{}", output.width, output.height)];
    if output.refresh_mhz > 0 {
        parts.push(format!(
            "@ {:.0} {}",
            f64::from(output.refresh_mhz) / 1000.0,
            crate::services::display::hertz(lang)
        ));
    }
    let mut line = parts.join(" ");
    if (output.scale - 1.0).abs() > f64::EPSILON && output.scale > 0.0 {
        line = format!("{line} · {:.0}%", output.scale * 100.0);
    }
    line
}

#[must_use]
pub fn device_name(details: &Details) -> String {
    [details.names.pretty.as_str(), details.names.hostname.as_str(), details.info.hostname.as_str()]
        .into_iter()
        .find(|name| !name.trim().is_empty())
        .unwrap_or("Linux")
        .to_string()
}

impl Page {
    #[must_use]
    pub fn report(&self) -> String {
        let Some(details) = &self.details else {
            return String::new();
        };
        let info = &details.info;
        let firmware = &details.firmware;
        let mut lines = vec![String::from("## System")];
        let mut push = |key: &str, value: String| {
            if !value.trim().is_empty() {
                lines.push(format!("{key}: {value}"));
            }
        };
        push("Glide Settings", VERSION.to_string());
        push("Device name", device_name(details));
        push("Hostname", details.names.hostname.clone());
        push("OS", info.os_name.clone());
        push("OS version", details.os_version.clone());
        push("Kernel", info.kernel.clone());
        push("Session", info.session.clone());
        push("Uptime (s)", info.uptime_secs.to_string());
        push("Model", format!("{} {} {}", firmware.vendor, firmware.product, firmware.version));
        push("Chassis", details.names.chassis.clone());
        push("CPU", format!("{} ({} threads)", info.cpu, info.cores));
        push(
            "Memory",
            format!(
                "{} ({} available)",
                format_memory(info.memory_bytes, Lang::En),
                format_memory(details.memory_available, Lang::En)
            ),
        );
        for gpu in &info.gpus {
            push("GPU", gpu.clone());
        }
        for output in &details.displays {
            push("Display", format!("{} {}", output.name, display_line(output, Lang::En)));
        }
        push(
            "Disk",
            format!("{} free of {}", format_bytes(info.disk_free, Lang::En), format_bytes(info.disk_total, Lang::En)),
        );
        push("Firmware", format!("{} {} ({})", firmware.bios_vendor, firmware.bios_version, firmware.bios_date));
        push("Board", firmware.board.clone());
        for battery in &details.batteries {
            push(
                "Battery",
                format!(
                    "{} {} {} health {} cycles {}",
                    battery.name,
                    battery.manufacturer,
                    battery.model,
                    battery.health.map_or_else(|| String::from("?"), |health| format!("{health}%")),
                    battery.cycles.map_or_else(|| String::from("?"), |cycles| cycles.to_string()),
                ),
            );
        }
        lines.join("\n")
    }

    fn load(&mut self) -> Task<Msg> {
        self.loading = true;
        let generation = self.generation.bump();
        Task::perform(blocking(system::collect_details), move |details| {
            Msg::Loaded(generation, Box::new(details.unwrap_or_default()))
        })
    }
}

impl Pane for Page {
    type Msg = Msg;

    const KEYWORDS: &'static [&'static str] = &[
        "system",
        "version",
        "hardware",
        "memory",
        "cpu",
        "processor",
        "gpu",
        "disk",
        "kernel",
        "hostname",
        "device name",
        "battery health",
        "bios",
        "firmware",
        "система",
        "версія",
        "пам'ять",
        "процесор",
        "диск",
        "ядро",
        "назва пристрою",
        "мікропрограма",
    ];

    fn open(&mut self, _c: Ctx, _config: &ConfigStore) -> Task<Msg> {
        self.copied = false;
        if self.loading || self.saving {
            return Task::none();
        }
        self.load()
    }

    fn update(&mut self, msg: Msg, _c: Ctx, _config: &mut ConfigStore) -> Task<Msg> {
        match msg {
            Msg::Loaded(generation, details) => {
                if self.generation.is_current(generation) {
                    self.details = Some(*details);
                    self.loading = false;
                }
            }
            Msg::CopyReport => {
                self.copied = true;
                return iced::clipboard::write(self.report());
            }
            Msg::StartRename => {
                self.error = None;
                self.editing = self.details.as_ref().map(device_name);
            }
            Msg::Name(name) => {
                if self.editing.is_some() {
                    self.editing = Some(name);
                }
            }
            Msg::CancelRename => self.editing = None,
            Msg::SaveName => {
                let Some(name) = self.editing.clone() else {
                    return Task::none();
                };
                let current =
                    self.details.as_ref().map(|details| details.names.static_name.clone()).unwrap_or_default();
                if system::plan_host_names(&name, &current).is_none() {
                    return Task::none();
                }
                self.saving = true;
                self.error = None;
                return Task::perform(blocking(move || system::set_host_names(&name, &current)), Msg::Renamed);
            }
            Msg::Renamed(result) => {
                self.saving = false;
                match result {
                    Some(Err(err)) => self.error = Some(err),
                    _ => self.editing = None,
                }
                return self.load();
            }
        }
        Task::none()
    }

    fn view<'a>(&'a self, c: Ctx, _config: &'a ConfigStore) -> Element<'a, Msg> {
        let c = &c;
        let t = c.theme;
        let Some(details) = &self.details else {
            return widgets::empty_state(t, Icon::Info, c.t(K::Loading), "");
        };
        let mut page =
            column![self.hero(c, details)].spacing(theme::SECTION_GAP).push(error_banner(t, self.error.as_deref()));
        if self.editing.is_some() {
            page = page.push(self.name_group(c, details));
        }
        page.push(stats(c, details))
            .push(facts_section(c, c.t(K::Hardware), hardware_facts(c, details)))
            .push(facts_section(c, c.t(K::Software), software_facts(c, details)))
            .into()
    }
}

impl Page {
    fn hero<'a>(&'a self, c: &Ctx, details: &'a Details) -> Element<'a, Msg> {
        let t = c.theme;
        let info = &details.info;
        let logo: Element<'a, Msg> = match &details.os_logo {
            Some(path) => glide_sdk::widget::photo(path, 72.0, 0.0),
            None => widgets::tile(Icon::Logo, t.acc_bg(), 72.0),
        };
        let os = if info.os_name.is_empty() { String::from("Linux") } else { info.os_name.clone() };
        let model = joined(&details.firmware.vendor, &details.firmware.product);
        let line = [model, os, format!("Glide {VERSION}")]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" · ");
        let copy: Element<'a, Msg> = if self.copied {
            widgets::status_pill(t, Tone::Ok, c.t(T::Copied))
        } else {
            widgets::secondary(t, c.t(T::CopyReport), Some(Msg::CopyReport))
        };
        let mut actions = row![].spacing(8);
        if details.names.available {
            let rename = self.editing.is_none().then_some(Msg::StartRename);
            actions = actions.push(widgets::secondary(t, c.t(T::Rename), rename));
        }
        let actions = actions.push(copy);
        let body = column![
            logo,
            widgets::gap(8.0),
            text(device_name(details), TextStyle::T1).color(t.fg_1()),
            text(line, TextStyle::CALL).color(t.fg_2()),
            widgets::gap(12.0),
            actions,
        ]
        .spacing(2)
        .align_x(Alignment::Center);
        let body = container(body).center_x(Fill);
        glide_sdk::widget::tinted_card(&widgets::paint(&t), super::Panel::About.color(), [24, 16], body)
    }

    fn name_group<'a>(&'a self, c: &Ctx, details: &'a Details) -> Element<'a, Msg> {
        let t = c.theme;
        let mut group = Group::new(t).title(c.t(T::DeviceName)).description(c.t(T::DeviceNameHint));
        if let Some(draft) = &self.editing {
            let plan = system::plan_host_names(draft, &details.names.static_name);
            let valid = plan.is_some();
            let save = (valid && !self.saving).then_some(Msg::SaveName);
            let subtitle = match &plan {
                Some((static_name, _)) => format!("{}: {static_name}", c.t(T::NetworkName)),
                None => c.t(T::InvalidName).to_string(),
            };
            group = group.push(
                Row::new(t, c.t(T::Name))
                    .subtitle(subtitle)
                    .suffix(widgets::flexible(
                        row![
                            widgets::field(t, "", draft, Msg::Name, save.clone(), !valid).width(Fill),
                            widgets::secondary(t, c.t(T::Cancel), Some(Msg::CancelRename)),
                            if self.saving {
                                widgets::spinner_label(t, c.t(T::Saving))
                            } else {
                                widgets::primary(t, c.t(T::Save), save)
                            },
                        ]
                        .spacing(8)
                        .align_y(Alignment::Center),
                        400.0,
                    ))
                    .build(),
            );
        }
        group.build()
    }
}

fn stats<'a>(c: &Ctx, details: &'a Details) -> Element<'a, Msg> {
    let t = c.theme;
    let info = &details.info;
    let used = info.memory_bytes.saturating_sub(details.memory_available);
    let mut tiles = vec![
        widgets::stat_tile(t, Icon::Cpu, ramp_color(Accent::Azurit, 3), info.cores.to_string(), c.t(T::Threads)),
        widgets::stat_tile(
            t,
            Icon::Memory,
            ramp_color(Accent::Amethyst, 3),
            format!("{:.0}%", used as f64 * 100.0 / info.memory_bytes.max(1) as f64),
            c.t(T::MemoryInUse),
        ),
    ];
    if info.disk_total > 0 {
        tiles.push(widgets::stat_tile(
            t,
            Icon::Drive,
            ramp_color(Accent::Malachite, 3),
            format_bytes(info.disk_free, c.lang),
            c.t(T::DiskFree),
        ));
    }
    tiles.push(widgets::stat_tile(
        t,
        Icon::Clock,
        ramp_color(Accent::Amber, 3),
        format_uptime(info.uptime_secs, c),
        c.t(T::Running),
    ));
    widgets::stat_row(t, tiles)
}

fn joined(first: &str, second: &str) -> String {
    format!("{first} {second}").trim().to_string()
}

fn facts_section<'a>(c: &Ctx, title: &'a str, facts: Vec<(String, String)>) -> Element<'a, Msg> {
    let paint = widgets::paint(&c.theme);
    column![
        glide_sdk::widget::section_heading(&paint, title),
        glide_sdk::widget::fact_grid(&paint, facts, if c.theme.compact { 1 } else { 2 }),
    ]
    .spacing(8)
    .into()
}

fn hardware_facts(c: &Ctx, details: &Details) -> Vec<(String, String)> {
    let info = &details.info;
    let mut facts = Vec::new();
    let mut push = |label: &str, value: String| {
        if !value.trim().is_empty() {
            facts.push((label.to_string(), value));
        }
    };
    push(c.t(K::Processor), info.cpu.clone());
    push(
        c.t(K::Memory),
        format!(
            "{} · {} {}",
            format_memory(info.memory_bytes, c.lang),
            format_memory(details.memory_available, c.lang),
            c.t(T::Available)
        ),
    );
    for gpu in &info.gpus {
        push(c.t(K::Graphics), gpu.clone());
    }
    if info.disk_total > 0 {
        let free = format_bytes(info.disk_free, c.lang);
        push(c.t(K::Disk), format!("{} · {free} {}", format_bytes(info.disk_total, c.lang), c.t(T::Available)));
    }
    for output in &details.displays {
        push(c.t(T::Display), display_line(output, c.lang));
    }
    for battery in &details.batteries {
        let mut parts = Vec::new();
        if let Some(health) = battery.health {
            parts.push(format!("{} {health}%", c.t(T::Health)));
        }
        if let Some(cycles) = battery.cycles {
            parts.push(format!("{cycles} {}", c.t(T::CyclesShort)));
        }
        push(c.t(T::Battery), parts.join(" · "));
    }
    if let Some(key) = chassis_key(&details.names.chassis) {
        push(c.t(T::Chassis), c.t(key).to_string());
    }
    facts
}

fn software_facts(c: &Ctx, details: &Details) -> Vec<(String, String)> {
    let info = &details.info;
    let firmware = &details.firmware;
    let mut facts = Vec::new();
    let mut push = |label: &str, value: String| {
        if !value.trim().is_empty() {
            facts.push((label.to_string(), value));
        }
    };
    push(c.t(K::OperatingSystem), joined(&info.os_name, &details.os_version));
    push(c.t(K::Kernel), info.kernel.clone());
    let session = if info.session.is_empty() { String::new() } else { format!(" · {}", info.session) };
    push(c.t(T::Desktop), format!("Glide {VERSION}{session}"));
    push(c.t(T::Firmware), joined(&firmware.bios_vendor, &firmware.bios_version));
    push(c.t(T::BiosDate), firmware.bios_date.clone());
    push(c.t(T::Board), firmware.board.clone());
    facts
}

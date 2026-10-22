use iced::widget::{column, container, image, row, svg, text};
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
        Copy => "Copy", "Копіювати";
        CopyReport => "Copy system report", "Копіювати звіт";
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
        NoHostnamed => "hostnamed is not available, so the name can only be viewed", "Служба hostnamed недоступна, тому назву можна лише переглядати";
        MemoryInUse => "memory in use", "пам'яті зайнято";
        DiskFree => "free on the system disk", "вільно на системному диску";
        Threads => "threads", "потоків";
        Running => "running", "працює";
        Model => "Model", "Модель";
        Chassis => "Type", "Тип";
        ChassisLaptop => "Laptop", "Ноутбук";
        ChassisDesktop => "Desktop", "Настільний комп'ютер";
        ChassisTablet => "Tablet", "Планшет";
        ChassisVm => "Virtual machine", "Віртуальна машина";
        ChassisServer => "Server", "Сервер";
        Displays => "Displays", "Дисплеї";
        Available => "available", "доступно";
        Battery => "Battery", "Батарея";
        Health => "Health", "Стан";
        HealthHint => "Capacity compared with a new battery", "Ємність порівняно з новою батареєю";
        Cycles => "Charge cycles", "Цикли заряджання";
        Technology => "Chemistry", "Тип елементів";
        Firmware => "Firmware", "Мікропрограма";
        BiosVendor => "Vendor", "Постачальник";
        BiosVersion => "Version", "Версія";
        BiosDate => "Release date", "Дата випуску";
        Board => "Motherboard", "Материнська плата";
        GlideVersion => "Glide Settings", "Налаштування Glide";
        OsVersion => "Version", "Версія";
        Website => "Website", "Вебсайт";
    }
}

use tr::Key as T;

#[derive(Debug, Clone)]
pub enum Msg {
    Loaded(u64, Box<Details>),
    Copy,
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
pub fn format_uptime(secs: u64, c: &Ctx) -> String {
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
pub fn display_line(output: &glide_ipc::OutputInfo, lang: Lang) -> String {
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
    pub fn summary(&self) -> String {
        let Some(details) = &self.details else {
            return String::new();
        };
        let info = &details.info;
        let mut lines = vec![
            format!("Glide {VERSION}"),
            format!("Host: {}", device_name(details)),
            format!("OS: {}", info.os_name),
            format!("Kernel: {}", info.kernel),
            format!("CPU: {} ({} threads)", info.cpu, info.cores),
            format!("Memory: {}", format_memory(info.memory_bytes, Lang::En)),
        ];
        lines.extend(info.gpus.iter().map(|gpu| format!("GPU: {gpu}")));
        lines.push(format!(
            "Disk: {} free of {}",
            format_bytes(info.disk_free, Lang::En),
            format_bytes(info.disk_total, Lang::En)
        ));
        lines.push(format!("Session: {}", info.session));
        lines.join("\n")
    }

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
            Msg::Copy => {
                self.copied = true;
                return iced::clipboard::write(self.summary());
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

    fn view<'a>(&'a self, c: Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let c = &c;
        let t = c.theme;
        let Some(details) = &self.details else {
            return widgets::empty_state(t, Icon::Info, c.t(K::Loading), "");
        };
        let mut page = column![self.hero(c, details)]
            .spacing(theme::SECTION_GAP)
            .push(error_banner(t, self.error.as_deref()))
            .push(self.name_group(c, details))
            .push(stats(c, details))
            .push(hardware_group(c, details));
        if !details.batteries.is_empty() {
            page = page.push(battery_group(c, details));
        }
        if let Some(firmware) = firmware_group(c, details) {
            page = page.push(firmware);
        }
        page.push(software_group(c, details, config)).into()
    }
}

impl Page {
    fn hero<'a>(&'a self, c: &Ctx, details: &'a Details) -> Element<'a, Msg> {
        let t = c.theme;
        let info = &details.info;
        let logo: Element<'a, Msg> = match &details.os_logo {
            Some(path) if path.extension().is_some_and(|ext| ext == "svg") => {
                svg(svg::Handle::from_path(path)).width(72).height(72).into()
            }
            Some(path) => image(image::Handle::from_path(path)).width(72).height(72).into(),
            None => widgets::tile(Icon::Logo, t.acc_bg(), 72.0),
        };
        let spec = |glyph: Icon, label: String| {
            row![widgets::icon(glyph, 14.0, t.fg_3()), text(label).size(12).color(t.fg_2()),]
                .spacing(6)
                .align_y(Alignment::Center)
        };
        let mut specs = row![].spacing(20).align_y(Alignment::Center);
        if !info.cpu.is_empty() {
            specs = specs.push(spec(Icon::Cpu, info.cpu.clone()));
        }
        if info.memory_bytes > 0 {
            specs = specs.push(spec(Icon::Memory, format_memory(info.memory_bytes, c.lang)));
        }
        if let Some(gpu) = info.gpus.first() {
            specs = specs.push(spec(Icon::Display, gpu.clone()));
        }
        let os = if info.os_name.is_empty() { String::from("Linux") } else { info.os_name.clone() };
        let actions: Element<'a, Msg> = if self.copied {
            widgets::status_pill(t, Tone::Ok, c.t(T::Copied))
        } else {
            row![
                widgets::secondary(t, c.t(T::Copy), Some(Msg::Copy)),
                widgets::tonal(t, c.t(T::CopyReport), Some(Msg::CopyReport)),
            ]
            .spacing(8)
            .into()
        };
        let color = super::Panel::About.color();
        container(
            column![
                logo,
                widgets::gap(8.0),
                text(device_name(details)).size(theme::PAGE_TITLE_SIZE).font(theme::SEMIBOLD).color(t.fg_1()),
                text(format!("Glide {VERSION} · {os}")).size(13).color(t.fg_2()),
                widgets::gap(8.0),
                specs.wrap().vertical_spacing(6),
                widgets::gap(12.0),
                actions,
            ]
            .spacing(2)
            .align_x(Alignment::Center),
        )
        .padding([24, 16])
        .center_x(Fill)
        .style(move |_: &iced::Theme| glide_sdk::widget::tinted_card_style(&widgets::paint(&t), color))
        .into()
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
        } else {
            group = group.push(
                Row::new(t, device_name(details))
                    .subtitle(format!(
                        "{}: {}",
                        c.t(T::NetworkName),
                        if details.names.static_name.is_empty() {
                            details.names.hostname.clone()
                        } else {
                            details.names.static_name.clone()
                        }
                    ))
                    .prefix(widgets::tile(Icon::Pencil, t.fill_3(), 28.0))
                    .suffix(widgets::secondary(t, c.t(T::Rename), details.names.available.then_some(Msg::StartRename)))
                    .build(),
            );
        }
        let mut column = column![group.build()].spacing(12);
        if !details.names.available {
            column = column.push(widgets::banner(t, Tone::Warn, Icon::Info, c.t(T::NoHostnamed), None));
        }
        column.into()
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

fn info_row<'a>(c: &Ctx, title: impl Into<String>, value: String) -> Row<'a, Msg> {
    Row::new(c.theme, title).suffix(text(value).size(13).color(c.theme.fg_2()))
}

fn joined(first: &str, second: &str) -> String {
    format!("{first} {second}").trim().to_string()
}

fn hardware_group<'a>(c: &Ctx, details: &'a Details) -> Element<'a, Msg> {
    let info = &details.info;
    let firmware = &details.firmware;
    let mut group = Group::new(c.theme).title(c.t(K::Hardware));
    let model = joined(&firmware.vendor, &firmware.product);
    if !model.is_empty() {
        group = group.push(info_row(c, c.t(T::Model), model).subtitle(firmware.version.clone()).build());
    }
    if let Some(key) = chassis_key(&details.names.chassis) {
        group = group.push(info_row(c, c.t(T::Chassis), c.t(key).to_string()).build());
    }
    group = group
        .push(
            info_row(c, c.t(K::Processor), info.cpu.clone())
                .subtitle(format!("{} {}", info.cores, c.t(K::Threads)))
                .build(),
        )
        .push(
            info_row(c, c.t(K::Memory), format_memory(info.memory_bytes, c.lang))
                .subtitle(format!("{} {}", format_memory(details.memory_available, c.lang), c.t(T::Available)))
                .build(),
        )
        .extend(info.gpus.iter().map(|gpu| info_row(c, c.t(K::Graphics), gpu.clone()).build()));
    for output in &details.displays {
        let name = joined(&output.make, &output.model);
        group = group.push(
            info_row(c, c.t(T::Displays), display_line(output, c.lang))
                .subtitle(if name.is_empty() { output.name.clone() } else { format!("{} · {name}", output.name) })
                .build(),
        );
    }
    if info.disk_total > 0 {
        let disk = format!(
            "{} {} {}",
            format_bytes(info.disk_free, c.lang),
            c.t(K::FreeOf),
            format_bytes(info.disk_total, c.lang)
        );
        group = group.push(info_row(c, c.t(K::Disk), disk).build());
    }
    group.build()
}

fn battery_group<'a>(c: &Ctx, details: &'a Details) -> Element<'a, Msg> {
    let t = c.theme;
    let mut group = Group::new(t).title(c.t(T::Battery));
    for battery in &details.batteries {
        let title = joined(&battery.manufacturer, &battery.model);
        let mut line = Row::new(t, if title.is_empty() { battery.name.clone() } else { title }).prefix(widgets::tile(
            Icon::Battery,
            ramp_color(Accent::Conifer, 3),
            28.0,
        ));
        if !battery.technology.is_empty() {
            line = line.subtitle(format!("{}: {}", c.t(T::Technology), battery.technology));
        }
        if let Some(percent) = battery.percent {
            line = line.suffix(
                column![
                    text(format!("{percent}%")).size(13).font(theme::SEMIBOLD).color(t.fg_1()),
                    container(widgets::progress_bar(t, f32::from(percent) / 100.0)).width(96),
                ]
                .spacing(4)
                .align_x(Alignment::End),
            );
        }
        group = group.push(line.build());
        if let Some(health) = battery.health {
            let tone = match health {
                80.. => Tone::Ok,
                60..=79 => Tone::Warn,
                _ => Tone::Danger,
            };
            group = group.push(
                Row::new(t, c.t(T::Health))
                    .subtitle(c.t(T::HealthHint))
                    .suffix(widgets::pill(t, tone, format!("{health}%")))
                    .build(),
            );
        }
        if let Some(cycles) = battery.cycles {
            group = group.push(info_row(c, c.t(T::Cycles), cycles.to_string()).build());
        }
    }
    group.build()
}

fn firmware_group<'a>(c: &Ctx, details: &'a Details) -> Option<Element<'a, Msg>> {
    let t = c.theme;
    let firmware = &details.firmware;
    let rows: Vec<(String, String)> = [
        (T::BiosVendor, &firmware.bios_vendor),
        (T::BiosVersion, &firmware.bios_version),
        (T::BiosDate, &firmware.bios_date),
        (T::Board, &firmware.board),
    ]
    .into_iter()
    .filter(|(_, value)| !value.is_empty())
    .map(|(key, value)| (c.t(key).to_string(), value.clone()))
    .collect();
    if rows.is_empty() {
        return None;
    }
    Some(
        column![
            container(text(c.t(T::Firmware)).size(14).font(theme::SEMIBOLD).color(t.fg_1()))
                .padding(iced::Padding::ZERO.left(4)),
            widgets::info_grid(t, rows),
        ]
        .spacing(8)
        .into(),
    )
}

fn software_group<'a>(c: &Ctx, details: &'a Details, config: &'a ConfigStore) -> Element<'a, Msg> {
    let t = c.theme;
    let info = &details.info;
    let config_path = config.path().map_or_else(|| String::from("—"), |path| path.display().to_string());
    let mut group = Group::new(t).title(c.t(K::Software)).push(
        info_row(c, c.t(K::OperatingSystem), info.os_name.clone())
            .subtitle(if details.os_version.is_empty() {
                String::new()
            } else {
                format!("{} {}", c.t(T::OsVersion), details.os_version)
            })
            .build(),
    );
    if !details.os_url.is_empty() {
        group = group.push(info_row(c, c.t(T::Website), details.os_url.clone()).build());
    }
    group
        .extend([
            info_row(c, c.t(K::Kernel), info.kernel.clone()).build(),
            info_row(c, c.t(K::WindowingSystem), format!("{} · Glide", info.session)).build(),
            info_row(c, c.t(T::GlideVersion), VERSION.to_string()).build(),
            info_row(c, c.t(K::Uptime), format_uptime(info.uptime_secs, c)).build(),
        ])
        .push(
            Row::new(t, c.t(K::ConfigFile))
                .subtitle(config_path)
                .suffix(if c.online {
                    widgets::pill(t, Tone::Ok, c.t(K::CompositorOnline))
                } else {
                    widgets::pill(t, Tone::Warn, c.t(K::CompositorOffline))
                })
                .build(),
        )
        .build()
}

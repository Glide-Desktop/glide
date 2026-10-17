use iced::widget::{column, container, row};
use iced::{Alignment, Element, Fill, Task};

use super::common::error_banner;
use super::network::{actions, chevron, form_row, lines, note, sheet};
use super::{Ctx, Pane};
use crate::i18n::Key as K;
use crate::icons::Icon;
use crate::services::bluetooth::{self, BluetoothState, BtDevice, DeviceKind};
use crate::services::config::ConfigStore;
use crate::services::run::{Generation, blocking};
use crate::theme::{self, Tone};
use crate::widgets::{self, Group, Row};

mod tr {
    crate::i18n::catalogue! {
        ThisComputer => "This computer", "Цей комп'ютер";
        ThisComputerHint => "How other devices see this computer", "Як цей комп'ютер бачать інші пристрої";
        Name => "Name", "Назва";
        Rename => "Rename", "Перейменувати";
        Save => "Save", "Зберегти";
        Discoverable => "Visible to nearby devices", "Видимий для пристроїв поблизу";
        DiscoverableHint => "Lets phones and other computers find it", "Дозволяє телефонам і комп'ютерам знайти його";
        DiscoverableFor => "Hides again after", "Знову ховається через";
        Minutes => "min", "хв";
        Pairable => "Accept pairing requests", "Приймати запити на спарювання";
        PairableHint => "Other devices may ask to pair with this computer", "Інші пристрої можуть просити спарювання з цим комп'ютером";
        Address => "Address", "Адреса";
        Kind => "Type", "Тип";
        Battery => "Battery", "Заряд";
        Trusted => "Trusted", "Довірений";
        TrustedHint => "Connects without asking and reconnects automatically", "Під'єднується без запиту й автоматично перепід'єднується";
        Blocked => "Blocked", "Заблокований";
        BlockedHint => "Refuse every connection from this device", "Відхиляти будь-які з'єднання від цього пристрою";
        DeviceName => "Device name", "Назва пристрою";
        Headphones => "Headphones", "Навушники";
        Speaker => "Speaker", "Колонка";
        Keyboard => "Keyboard", "Клавіатура";
        Mouse => "Mouse", "Миша";
        Tablet => "Graphics tablet", "Графічний планшет";
        Gamepad => "Game controller", "Ігровий контролер";
        Phone => "Phone", "Телефон";
        Computer => "Computer", "Комп'ютер";
        Other => "Device", "Пристрій";
        Searching => "Searching…", "Пошук…";
        SearchHint => "Searching for 20 seconds. New devices appear as they are found.", "Пошук триває 20 секунд. Нові пристрої з'являються щойно їх знайдено.";
        PinHint => "Glide is not running, so devices that ask for a PIN code cannot be paired", "Glide не запущено, тож пристрої, що просять PIN-код, не вдасться спарити";
        Paired => "Paired", "Спарений";
        NameInvalid => "Enter a name", "Введіть назву";
    }
}

use tr::Key as T;

#[derive(Debug, Clone)]
pub enum Msg {
    Loaded(u64, BluetoothState),
    Refresh,
    Power(bool),
    Scan,
    Connect(String),
    Disconnect(String),
    Pair(String),
    Remove(String),
    Expand(String),
    Trust(String, bool),
    Block(String, bool),
    Discoverable(bool),
    Pairable(bool),
    EditAdapterName,
    AdapterName(String),
    SaveAdapterName,
    DeviceName(String),
    SaveDeviceName(String),
    CancelEdit,
    Done(Option<Result<String, String>>),
    Scanned(Option<Result<String, String>>),
}

#[derive(Debug, Default)]
pub struct Page {
    state: BluetoothState,
    loading: bool,
    loaded: bool,
    scanning: bool,
    busy: Option<String>,
    error: Option<String>,
    generation: Generation,
    pending: u32,
    expanded: Option<String>,
    adapter_name: Option<String>,
    device_name: String,
}

impl Pane for Page {
    type Msg = Msg;

    const KEYWORDS: &'static [&'static str] =
        &["bluetooth", "headphones", "pair", "mouse", "keyboard", "навушники", "блютуз", "спарити", "пристрої"];
    const POLLS: bool = true;

    fn open(&mut self, _c: Ctx, _config: &ConfigStore) -> Task<Msg> {
        if self.loaded || self.loading {
            return Task::none();
        }
        self.refresh()
    }

    #[allow(clippy::too_many_lines)] // One match arm per message keeps the panel's state machine in one place
    fn update(&mut self, msg: Msg, _c: Ctx, _config: &mut ConfigStore) -> Task<Msg> {
        match msg {
            Msg::Loaded(generation, _) if !self.generation.is_current(generation) => {}
            Msg::Loaded(_, state) => {
                self.state = state;
                self.loading = false;
                self.loaded = true;
                if let Some(address) = &self.expanded
                    && self.state.device(address).is_none_or(|device| !device.paired)
                {
                    self.expanded = None;
                }
            }
            Msg::Refresh => return self.refresh(),
            Msg::Power(on) => {
                self.state.powered = on;
                return self.run(None, move || bluetooth::set_powered(on));
            }
            Msg::Scan => {
                self.scanning = true;
                self.error = None;
                return Task::batch([Task::perform(blocking(bluetooth::scan), Msg::Scanned), self.refresh()]);
            }
            Msg::Scanned(result) => {
                self.scanning = false;
                if let Some(Err(err)) = result {
                    self.error = Some(err);
                }
                return self.refresh();
            }
            Msg::Connect(address) => return self.run_on(address, bluetooth::connect),
            Msg::Disconnect(address) => return self.run_on(address, bluetooth::disconnect),
            Msg::Pair(address) => return self.run_on(address, bluetooth::pair_and_connect),
            Msg::Remove(address) => {
                if self.expanded.as_ref() == Some(&address) {
                    self.expanded = None;
                }
                return self.run_on(address, bluetooth::remove);
            }
            Msg::Expand(address) => {
                if self.expanded.as_ref() == Some(&address) {
                    self.expanded = None;
                } else {
                    self.device_name =
                        self.state.device(&address).map(|device| device.name.clone()).unwrap_or_default();
                    self.expanded = Some(address);
                }
            }
            Msg::Trust(address, on) => {
                if let Some(device) = self.device_mut(&address) {
                    device.trusted = on;
                }
                return self.run(None, move || bluetooth::set_trusted(&address, on));
            }
            Msg::Block(address, on) => {
                if let Some(device) = self.device_mut(&address) {
                    device.blocked = on;
                }
                return self.run(None, move || bluetooth::set_blocked(&address, on));
            }
            Msg::Discoverable(on) => {
                self.state.adapter.discoverable = on;
                return self.run(None, move || bluetooth::set_discoverable(on));
            }
            Msg::Pairable(on) => {
                self.state.adapter.pairable = on;
                return self.run(None, move || bluetooth::set_pairable(on));
            }
            Msg::EditAdapterName => self.adapter_name = Some(self.state.alias.clone()),
            Msg::AdapterName(name) => self.adapter_name = Some(name),
            Msg::SaveAdapterName => {
                let Some(name) = self.adapter_name.as_deref().and_then(bluetooth::clean_alias) else {
                    return Task::none();
                };
                self.adapter_name = None;
                self.state.alias.clone_from(&name);
                return self.run(None, move || bluetooth::set_adapter_alias(&name));
            }
            Msg::DeviceName(name) => self.device_name = name,
            Msg::SaveDeviceName(address) => {
                let Some(name) = bluetooth::clean_alias(&self.device_name) else {
                    return Task::none();
                };
                if let Some(device) = self.device_mut(&address) {
                    device.name.clone_from(&name);
                }
                return self.run(None, move || bluetooth::set_device_alias(&address, &name));
            }
            Msg::CancelEdit => self.adapter_name = None,
            Msg::Done(result) => {
                self.pending = self.pending.saturating_sub(1);
                if self.pending == 0 {
                    self.busy = None;
                }
                if let Some(Err(err)) = result {
                    self.error = Some(err);
                }
                self.generation.bump();
                self.loading = false;
                return self.refresh();
            }
        }
        Task::none()
    }

    fn view<'a>(&'a self, c: Ctx, _config: &'a ConfigStore) -> Element<'a, Msg> {
        let t = c.theme;
        if !self.state.available {
            let (title, hint) = if self.loaded {
                (c.t(K::BluetoothUnavailable), c.t(K::BluetoothUnavailableHint))
            } else {
                (c.t(K::Loading), "")
            };
            return widgets::empty_state(t, Icon::Bluetooth, title, hint);
        }
        let status = if self.state.powered {
            format!("{} “{}”", c.t(K::VisibleAs), self.state.alias)
        } else {
            c.t(K::BluetoothOff).to_string()
        };
        let blocked = (self.state.blocked && !self.state.powered)
            .then(|| widgets::banner(t, t.warn(), Icon::Warning, c.t(K::BluetoothBlocked), None));
        let page = column![
            widgets::section_hero(
                t,
                Icon::Bluetooth,
                super::Panel::Bluetooth.color(),
                "Bluetooth",
                status,
                Some(widgets::switch(t, self.state.powered, Msg::Power)),
            ),
            blocked,
            error_banner(t, self.error.as_deref()),
        ]
        .spacing(theme::SECTION_GAP);
        if !self.state.powered {
            return page.into();
        }
        let page = page.push(self.adapter(&c));
        let mut mine = Group::new(t).title(c.t(K::MyDevices));
        let paired: Vec<&BtDevice> = self.state.paired().collect();
        if paired.is_empty() {
            mine = mine.push(widgets::list_empty(t, Icon::Headphones, c.t(K::NoPairedDevices), c.t(K::NearbyHint)));
        }
        for device in paired {
            mine = mine.push(self.device_row(&c, device));
            if self.expanded.as_deref() == Some(device.address.as_str()) {
                mine = mine.push(self.device_sheet(&c, device));
            }
        }
        let scan: Element<'_, Msg> = if self.scanning {
            widgets::spinner_label(t, c.t(T::Searching))
        } else {
            widgets::tonal(t, c.t(K::Search), Some(Msg::Scan))
        };
        let mut nearby = Group::new(t)
            .title(c.t(K::NearbyDevices))
            .description(if self.scanning { c.t(T::SearchHint) } else { c.t(K::NearbyHint) })
            .suffix(scan);
        let others: Vec<&BtDevice> = self.state.nearby().collect();
        if others.is_empty() {
            nearby = nearby.push(widgets::list_empty(t, Icon::Search, c.t(K::NoNearbyDevices), ""));
        }
        for device in others {
            nearby = nearby.push(self.device_row(&c, device));
        }
        if !c.online && (self.scanning || self.state.nearby().next().is_some()) {
            nearby = nearby.push(note(t, Icon::Info, c.t(T::PinHint)));
        }
        page.push(mine.build()).push(nearby.build()).into()
    }

    fn tick(&mut self, _c: Ctx) -> Task<Msg> {
        self.refresh()
    }
}

impl Page {
    fn refresh(&mut self) -> Task<Msg> {
        if self.loading || self.pending > 0 {
            return Task::none();
        }
        self.loading = true;
        let generation = self.generation.current();
        Task::perform(blocking(bluetooth::read), move |state| Msg::Loaded(generation, state.unwrap_or_default()))
    }

    fn run(
        &mut self, busy: Option<String>, job: impl FnOnce() -> Result<String, String> + Send + 'static,
    ) -> Task<Msg> {
        self.busy = busy;
        self.error = None;
        self.generation.bump();
        self.loading = false;
        self.pending += 1;
        Task::perform(blocking(job), Msg::Done)
    }

    fn run_on(&mut self, address: String, job: fn(&str) -> Result<String, String>) -> Task<Msg> {
        let target = address.clone();
        self.run(Some(address), move || job(&target))
    }

    fn device_mut(&mut self, address: &str) -> Option<&mut BtDevice> {
        self.state.devices.iter_mut().find(|device| device.address == address)
    }

    fn adapter(&self, c: &Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        let adapter = &self.state.adapter;
        let name_row = match &self.adapter_name {
            Some(draft) => {
                let valid = bluetooth::clean_alias(draft).is_some();
                form_row(
                    t,
                    c.t(T::Name),
                    (!valid).then(|| c.t(T::NameInvalid)),
                    row![
                        widgets::field(
                            t,
                            c.t(T::Name),
                            draft,
                            Msg::AdapterName,
                            valid.then_some(Msg::SaveAdapterName),
                            !valid,
                        )
                        .width(Fill),
                        widgets::secondary(t, c.t(K::Cancel), Some(Msg::CancelEdit)),
                        widgets::primary(t, c.t(T::Save), valid.then_some(Msg::SaveAdapterName)),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center)
                    .into(),
                )
            }
            None => Row::new(t, c.t(T::Name))
                .subtitle(self.state.alias.clone())
                .suffix(widgets::secondary(t, c.t(T::Rename), Some(Msg::EditAdapterName)))
                .build(),
        };
        let discoverable_hint = if adapter.discoverable && adapter.discoverable_timeout > 0 {
            format!("{} {} {}", c.t(T::DiscoverableFor), adapter.discoverable_timeout.div_ceil(60), c.t(T::Minutes))
        } else {
            c.t(T::DiscoverableHint).to_string()
        };
        let mut group = Group::new(t)
            .title(c.t(T::ThisComputer))
            .description(c.t(T::ThisComputerHint))
            .push(name_row)
            .push(
                Row::new(t, c.t(T::Discoverable))
                    .subtitle(discoverable_hint)
                    .suffix(widgets::switch(t, adapter.discoverable, Msg::Discoverable))
                    .build(),
            )
            .push(
                Row::new(t, c.t(T::Pairable))
                    .subtitle(c.t(T::PairableHint))
                    .suffix(widgets::switch(t, adapter.pairable, Msg::Pairable))
                    .build(),
            );
        if !adapter.address.is_empty() {
            group = group.push(
                Row::new(t, c.t(T::Address))
                    .suffix(iced::widget::text(adapter.address.clone()).size(13).color(t.fg_2()))
                    .build(),
            );
        }
        group.build()
    }

    fn device_row<'a>(&'a self, c: &Ctx, device: &'a BtDevice) -> Element<'a, Msg> {
        let t = c.theme;
        let busy = self.busy.as_deref() == Some(device.address.as_str());
        let idle = self.busy.is_none();
        let kind = kind_label(c, bluetooth::device_kind(&device.icon));
        let subtitle = if busy {
            c.t(K::Working).to_string()
        } else if device.connected {
            match device.battery {
                Some(level) => format!("{} · {level}%", c.t(K::Connected)),
                None => c.t(K::Connected).to_string(),
            }
        } else if device.paired {
            format!("{kind} · {}", c.t(K::NotConnected))
        } else {
            format!("{kind} · {}", device.address)
        };
        let address = device.address.clone();
        let mut suffix = row![].spacing(8).align_y(Alignment::Center);
        if device.paired {
            if device.blocked {
                suffix = suffix.push(widgets::pill(t, Tone::Danger, c.t(T::Blocked)));
            } else {
                suffix = suffix.push(if device.connected {
                    widgets::secondary(t, c.t(K::Disconnect), idle.then(|| Msg::Disconnect(address.clone())))
                } else {
                    widgets::primary(t, c.t(K::Connect), idle.then(|| Msg::Connect(address.clone())))
                });
            }
            let open = self.expanded.as_ref() == Some(&device.address);
            suffix = suffix.push(chevron(t, open));
        } else {
            suffix = suffix.push(widgets::tonal(t, c.t(K::Pair), idle.then(|| Msg::Pair(address.clone()))));
        }
        let glyph = Icon::for_bluetooth(&device.icon);
        let prefix: Element<'a, Msg> = if device.connected {
            widgets::tile(glyph, super::Panel::Bluetooth.color(), 28.0)
        } else {
            container(widgets::icon(glyph, 18.0, t.fg_2())).center_x(28.0).center_y(28.0).into()
        };
        let mut line = Row::new(t, device.name.clone()).subtitle(subtitle).prefix(prefix).suffix(suffix);
        if device.paired {
            line = line.on_press(Msg::Expand(device.address.clone()));
        }
        line.build()
    }

    fn device_sheet<'a>(&'a self, c: &Ctx, device: &'a BtDevice) -> Element<'a, Msg> {
        let t = c.theme;
        let address = device.address.clone();
        let mut rows: Vec<Element<'a, Msg>> = Vec::new();
        let mut info = vec![
            (c.t(T::Kind).to_string(), kind_label(c, bluetooth::device_kind(&device.icon)).to_string()),
            (c.t(T::Address).to_string(), device.address.clone()),
            (
                c.t(T::Paired).to_string(),
                c.t(if device.connected { K::Connected } else { K::NotConnected }).to_string(),
            ),
        ];
        if let Some(level) = device.battery {
            info.push((c.t(T::Battery).to_string(), format!("{level}%")));
        }
        rows.push(lines(t, info));
        if let Some(level) = device.battery {
            rows.push(container(widgets::progress_bar(t, f32::from(level) / 100.0)).padding([2, 8]).into());
        }
        let valid = bluetooth::clean_alias(&self.device_name).is_some();
        let changed = self.device_name.trim() != device.name;
        let save = (valid && changed).then(|| Msg::SaveDeviceName(address.clone()));
        rows.push(form_row(
            t,
            c.t(T::DeviceName),
            None,
            row![
                widgets::field(t, c.t(T::DeviceName), &self.device_name, Msg::DeviceName, save.clone(), !valid,)
                    .width(Fill),
                widgets::secondary(t, c.t(T::Save), save),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
            .into(),
        ));
        for (title, hint, value, msg) in [
            (T::Trusted, T::TrustedHint, device.trusted, Msg::Trust as fn(_, _) -> _),
            (T::Blocked, T::BlockedHint, device.blocked, Msg::Block),
        ] {
            let address = address.clone();
            rows.push(
                Row::new(t, c.t(title))
                    .subtitle(c.t(hint))
                    .suffix(widgets::switch(t, value, move |on| msg(address.clone(), on)))
                    .build(),
            );
        }
        rows.push(actions(vec![widgets::danger(
            t,
            c.t(K::Forget),
            self.busy.is_none().then_some(Msg::Remove(address)),
        )]));
        sheet(t, rows)
    }
}

fn kind_label(c: &Ctx, kind: DeviceKind) -> &'static str {
    c.t(match kind {
        DeviceKind::Headphones => T::Headphones,
        DeviceKind::Speaker => T::Speaker,
        DeviceKind::Keyboard => T::Keyboard,
        DeviceKind::Mouse => T::Mouse,
        DeviceKind::Tablet => T::Tablet,
        DeviceKind::Gamepad => T::Gamepad,
        DeviceKind::Phone => T::Phone,
        DeviceKind::Computer => T::Computer,
        DeviceKind::Other => T::Other,
    })
}

use glide_sdk::widget::{self as sdk, Fact};
use iced::widget::{column, container, row};
use iced::{Alignment, Element, Fill, Task};

use super::common::error_banner;
use super::{Ctx, Pane};
use crate::i18n::Key as K;
use crate::icons::Icon;
use crate::services::bluetooth::{self, BluetoothState, BtDevice, DeviceKind};
use crate::services::config::ConfigStore;
use crate::services::run::{Jobs, blocking};
use crate::theme::{self, Tone};
use crate::widgets::{self, Group, Row};
use crate::widgets::{chevron, form_row, note};

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
        BlockAction => "Block", "Заблокувати";
        UnblockAction => "Unblock", "Розблокувати";
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
        Status => "Status", "Стан";
        NameInvalid => "Enter a name", "Введіть назву";
        Enabled => "Enabled", "Увімкнено";
        AdapterName => "Computer name", "Назва комп’ютера";
        ConnectedOne => "device connected", "пристрій підключено";
        ConnectedFew => "devices connected", "пристрої підключено";
        ConnectedMany => "devices connected", "пристроїв підключено";
    }
}

use tr::Key as T;

fn hero_text(c: &Ctx, state: &BluetoothState) -> (String, String) {
    if !state.powered {
        return ("Bluetooth".to_string(), c.t(K::BluetoothOff).to_string());
    }
    let connected = state.devices.iter().filter(|device| device.connected).count();
    let title = if connected == 0 {
        c.t(T::Enabled).to_string()
    } else {
        let count = glide_sdk::i18n::count(
            u64::try_from(connected).unwrap_or(u64::MAX),
            c.lang,
            [c.t(T::ConnectedOne), c.t(T::ConnectedFew), c.t(T::ConnectedMany)],
        );
        format!("{} · {count}", c.t(T::Enabled))
    };
    let label = if state.adapter.discoverable { c.t(K::VisibleAs) } else { c.t(T::AdapterName) };
    let description = if state.alias.is_empty() { String::new() } else { format!("{label} “{}”", state.alias) };
    (title, description)
}

fn device_facts(c: &Ctx, device: &BtDevice) -> Vec<Fact> {
    vec![
        Fact::new(c.t(T::Kind), kind_label(c, bluetooth::device_kind(&device.icon))),
        Fact::new(c.t(T::Status), c.t(if device.connected { K::Connected } else { K::NotConnected })),
        Fact::new(c.t(T::Address), if device.address.is_empty() { "—" } else { &device.address }).mono(),
    ]
}

fn connection_action(device: &BtDevice, idle: bool) -> Option<Msg> {
    if !idle {
        return None;
    }
    if device.connected {
        Some(Msg::Disconnect(device.address.clone()))
    } else if !device.blocked {
        Some(Msg::Connect(device.address.clone()))
    } else {
        None
    }
}

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
    jobs: Jobs,
    loaded: bool,
    scanning: bool,
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
        if self.loaded || self.jobs.loading {
            return Task::none();
        }
        self.refresh()
    }

    #[allow(clippy::too_many_lines)] // One match arm per message keeps the panel's state machine in one place
    fn update(&mut self, msg: Msg, _c: Ctx, _config: &mut ConfigStore) -> Task<Msg> {
        match msg {
            Msg::Loaded(generation, _) if !self.jobs.generation.is_current(generation) => {}
            Msg::Loaded(_, state) => {
                self.state = state;
                self.jobs.loading = false;
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
                self.jobs.error = None;
                return Task::batch([Task::perform(blocking(bluetooth::scan), Msg::Scanned), self.refresh()]);
            }
            Msg::Scanned(result) => {
                self.scanning = false;
                if let Some(Err(err)) = result {
                    self.jobs.error = Some(err);
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
                self.jobs.finish(result);
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
        let (title, status) = hero_text(&c, &self.state);
        let blocked = (self.state.blocked && !self.state.powered)
            .then(|| widgets::banner(t, Tone::Warn, Icon::Warning, c.t(K::BluetoothBlocked), None));
        let page = column![
            widgets::section_hero(
                t,
                Icon::Bluetooth,
                super::Panel::Bluetooth.color(),
                title,
                status,
                Some(widgets::switch(t, self.state.powered, Msg::Power)),
            ),
            blocked,
            error_banner(t, self.jobs.error.as_deref()),
        ]
        .spacing(theme::SECTION_GAP);
        if !self.state.powered {
            return page.into();
        }
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
        page.push(mine.build()).push(nearby.build()).push(self.adapter(&c)).into()
    }

    fn tick(&mut self, _c: Ctx) -> Task<Msg> {
        self.refresh()
    }
}

impl Page {
    fn refresh(&mut self) -> Task<Msg> {
        let Some(generation) = self.jobs.start_loading() else { return Task::none() };
        Task::perform(blocking(bluetooth::read), move |state| Msg::Loaded(generation, state.unwrap_or_default()))
    }

    fn run(
        &mut self, busy: Option<String>, job: impl FnOnce() -> Result<String, String> + Send + 'static,
    ) -> Task<Msg> {
        self.jobs.begin(busy);
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
                    .suffix(
                        glide_sdk::widget::text(adapter.address.clone(), glide_sdk::theme::TextStyle::CALL)
                            .color(t.fg_2()),
                    )
                    .build(),
            );
        }
        group.build()
    }

    fn device_row<'a>(&'a self, c: &Ctx, device: &'a BtDevice) -> Element<'a, Msg> {
        let t = c.theme;
        let busy = self.jobs.busy.as_deref() == Some(device.address.as_str());
        let idle = self.jobs.busy.is_none();
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
        rows.push(sdk::kv_grid(&t.paint, device_facts(c, device), if t.compact { 2 } else { 3 }));
        if let Some(level) = device.battery {
            let bar = container(widgets::progress_bar(t, f32::from(level) / 100.0)).width(140);
            let value = sdk::text(format!("{level}%"), glide_sdk::theme::TextStyle::CALL);
            rows.push(
                Row::new(t, c.t(T::Battery)).suffix(row![bar, value].spacing(8).align_y(Alignment::Center)).build(),
            );
        }
        let trusted_address = address.clone();
        rows.push(
            Row::new(t, c.t(T::Trusted))
                .subtitle(c.t(T::TrustedHint))
                .suffix(widgets::switch(t, device.trusted, move |on| Msg::Trust(trusted_address.clone(), on)))
                .build(),
        );
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
        let idle = self.jobs.busy.is_none();
        let connection = sdk::compact_button(
            &t.paint,
            c.t(if device.connected { K::Disconnect } else { K::Connect }),
            sdk::Tone::Neutral,
            connection_action(device, idle),
        );
        let forget = sdk::compact_button(
            &t.paint,
            c.t(K::Forget),
            sdk::Tone::Neutral,
            idle.then(|| Msg::Remove(address.clone())),
        );
        let block = sdk::compact_button(
            &t.paint,
            c.t(if device.blocked { T::UnblockAction } else { T::BlockAction }),
            if device.blocked { sdk::Tone::Neutral } else { sdk::Tone::Danger },
            idle.then(|| Msg::Block(address, !device.blocked)),
        );
        rows.push(sdk::split_actions(vec![connection, forget], vec![block]));
        sdk::detail_band(&t.paint, rows)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Lang;
    use crate::theme::{Accent, Appearance, ThemeState};

    #[test]
    fn hero_reports_actual_visibility_and_connected_device_count() {
        let c = Ctx {
            theme: ThemeState::new(Appearance::Light, Accent::Azurit),
            lang: Lang::Uk,
            writable: true,
            online: true,
        };
        let mut state = BluetoothState { powered: true, alias: "laptop".into(), ..BluetoothState::default() };
        assert_eq!(hero_text(&c, &state), ("Увімкнено".into(), "Назва комп’ютера “laptop”".into()));
        state.devices = vec![BtDevice { connected: true, ..BtDevice::default() }; 2];
        state.adapter.discoverable = true;
        assert_eq!(hero_text(&c, &state), ("Увімкнено · 2 пристрої підключено".into(), "Видимий як “laptop”".into()));
        state.devices.push(BtDevice { connected: false, ..BtDevice::default() });
        assert_eq!(hero_text(&c, &state).0, "Увімкнено · 2 пристрої підключено");
        state.devices.extend(vec![BtDevice { connected: true, ..BtDevice::default() }; 3]);
        assert_eq!(hero_text(&c, &state).0, "Увімкнено · 5 пристроїв підключено");
        let english = Ctx { lang: Lang::En, ..c };
        assert_eq!(hero_text(&english, &state).0, "Enabled · 5 devices connected");
    }

    #[test]
    fn device_facts_show_real_state_and_unknown_address() {
        let c = Ctx {
            theme: ThemeState::new(Appearance::Light, Accent::Azurit),
            lang: Lang::Uk,
            writable: true,
            online: true,
        };
        let device = BtDevice { icon: "audio-headphones".into(), battery: Some(45), ..BtDevice::default() };
        assert_eq!(
            device_facts(&c, &device),
            vec![
                Fact::new("Тип", "Навушники"),
                Fact::new("Стан", c.t(K::NotConnected)),
                Fact::new("Адреса", "—").mono(),
            ]
        );
    }

    #[test]
    fn expanded_device_keeps_rename_trust_and_block_actions() {
        let c = Ctx {
            theme: ThemeState::new(Appearance::Light, Accent::Azurit),
            lang: Lang::Uk,
            writable: true,
            online: true,
        };
        let address = "94:DB:00:00:00:3E".to_string();
        let device =
            BtDevice { address: address.clone(), name: "Headphones".into(), paired: true, ..BtDevice::default() };
        let mut page =
            Page { state: BluetoothState { devices: vec![device], ..BluetoothState::default() }, ..Page::default() };
        let mut config = ConfigStore::parse("");
        let _ = page.update(Msg::Expand(address.clone()), c, &mut config);
        assert_eq!(page.expanded.as_deref(), Some(address.as_str()));
        assert_eq!(page.device_name, "Headphones");
        let _ = page.update(Msg::DeviceName("New name".into()), c, &mut config);
        assert_eq!(page.device_name, "New name");
        let _ = page.update(Msg::Trust(address.clone(), true), c, &mut config);
        let _ = page.update(Msg::Block(address.clone(), true), c, &mut config);
        let device = page.state.device(&address).unwrap();
        assert!(device.trusted && device.blocked);
    }

    #[test]
    fn connection_action_respects_connection_block_and_busy_state() {
        let mut device = BtDevice { address: "device".into(), paired: true, ..BtDevice::default() };
        assert!(matches!(connection_action(&device, true), Some(Msg::Connect(address)) if address == "device"));
        assert!(connection_action(&device, false).is_none());
        device.blocked = true;
        assert!(connection_action(&device, true).is_none());
        device.connected = true;
        assert!(matches!(connection_action(&device, true), Some(Msg::Disconnect(address)) if address == "device"));
    }
}

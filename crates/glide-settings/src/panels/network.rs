use iced::widget::{column, container, row};
use iced::{Alignment, Element, Fill, Length, Task};

use super::common::error_banner;
use super::{Ctx, Pane};
use crate::i18n::Key as K;
use crate::icons::Icon;
use crate::services::config::ConfigStore;
use crate::services::network::{
    self, AccessPoint, Details, Metered, NetworkState, Profile, ProxyMode, ProxySettings, Rescan, Secret,
};
use crate::services::run::{Generation, blocking};
use crate::theme::{self, Tone};
use crate::widgets::{self, Group, Row, actions, chevron, form_row, lines, note, sheet};

mod tr {
    crate::i18n::catalogue! {
        Airplane => "Airplane mode", "Режим польоту";
        AirplaneHint => "Turns off Wi-Fi, Bluetooth and mobile radios", "Вимикає Wi-Fi, Bluetooth і мобільний зв'язок";
        AirplaneOn => "Airplane mode is on", "Увімкнено режим польоту";
        HardwareOff => "Turned off by a hardware switch", "Вимкнено апаратним перемикачем";
        Hotspot => "Wi-Fi hotspot", "Точка доступу Wi-Fi";
        HotspotHint => "Share this computer's connection with other devices", "Поділитися з'єднанням цього комп'ютера з іншими пристроями";
        HotspotOn => "Sharing as", "Роздає як";
        HotspotName => "Network name", "Назва мережі";
        HotspotPassword => "Password", "Пароль";
        HotspotPasswordHint => "8 to 63 characters. Other devices type it to join.", "Від 8 до 63 символів. Інші пристрої вводять його для під'єднання.";
        HotspotWarning => "Wi-Fi leaves its current network while the hotspot is on", "Поки точка доступу працює, Wi-Fi від'єднується від поточної мережі";
        Generate => "Generate", "Згенерувати";
        Start => "Turn on", "Увімкнути";
        Stop => "Turn off", "Вимкнути";
        IpAddress => "IP address", "IP-адреса";
        Ipv6Address => "IPv6 address", "IPv6-адреса";
        Gateway => "Router", "Маршрутизатор";
        Dns => "DNS", "DNS";
        MacAddress => "Hardware address", "Апаратна адреса";
        LinkSpeed => "Link speed", "Швидкість з'єднання";
        Frequency => "Frequency", "Частота";
        Channel => "channel", "канал";
        Security => "Security", "Безпека";
        SecurityNone => "None", "Немає";
        AutoConnect => "Connect automatically", "Під'єднуватися автоматично";
        AutoConnectHint => "Join this network whenever it is in range", "Під'єднуватися щоразу, коли мережа поруч";
        MeteredTitle => "Metered connection", "Лімітне з'єднання";
        MeteredHint => "Apps that respect it avoid large downloads and updates", "Програми, що це враховують, уникають великих завантажень і оновлень";
        MeteredGuess => "NetworkManager decides automatically", "NetworkManager визначає автоматично";
        ShowPassword => "Show", "Показати";
        HidePassword => "Hide", "Сховати";
        NetworkPassword => "Network password", "Пароль мережі";
        CustomDns => "DNS servers", "DNS-сервери";
        CustomDnsHint => "Comma-separated. Leave empty to use the router's.", "Через кому. Залиште порожнім, щоб брати від маршрутизатора.";
        DnsInvalid => "Enter IP addresses separated by commas", "Введіть IP-адреси через кому";
        JoinHidden => "Join a hidden network…", "Під'єднатися до прихованої мережі…";
        JoinHiddenHint => "For networks that do not broadcast their name", "Для мереж, що не показують свою назву";
        Ssid => "Network name (SSID)", "Назва мережі (SSID)";
        WrongPassword => "Wrong password. Check it and try again.", "Неправильний пароль. Перевірте його і спробуйте ще раз.";
        SsidTooLong =>"Too long: a network name fits in 32 bytes", "Задовга: назва мережі вміщує до 32 байтів";
        Join => "Join", "Під'єднати";
        WpaPersonal => "WPA/WPA2 Personal", "WPA/WPA2 Personal";
        KnownNetworks => "Known networks", "Відомі мережі";
        KnownNetworksHint => "Saved Wi-Fi networks. The switch controls automatic joining.", "Збережені мережі Wi-Fi. Перемикач керує автоматичним під'єднанням.";
        NoKnownNetworks => "No saved networks", "Немає збережених мереж";
        InRange => "In range", "Поруч";
        OutOfRange => "Not in range", "Не поруч";
        Automatic => "Automatic", "Автоматично";
        Manual => "Manual", "Вручну";
        Proxy => "Proxy", "Проксі";
        ProxyHint => "Used by apps that follow the GNOME proxy settings", "Для програм, що використовують налаштування проксі GNOME";
        ProxyModeTitle => "Mode", "Режим";
        ProxyOff => "Off", "Вимкнено";
        ProxyUrl => "Configuration URL", "Адреса конфігурації";
        ProxyIgnore => "Ignore hosts", "Не використовувати для";
        ProxyPortInvalid => "Ports are numbers from 1 to 65535", "Порт — це число від 1 до 65535";
        Host => "Host", "Хост";
        Port => "Port", "Порт";
        Applied => "Saved", "Збережено";
        Busy => "Working…", "Виконується…";
    }
}

use tr::Key as T;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expanded {
    Wifi(String),
    Wired(String),
    Hidden,
    Hotspot,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Loaded(u64, NetworkState),
    Refresh,
    Rescan,
    ToggleWifi(bool),
    Airplane(bool),
    Select(String),
    SelectWired(String),
    Cancel,
    Password(Secret),
    Connect(String),
    Disconnect,
    DisconnectDevice(String),
    Forget(String),
    DetailsLoaded(u64, Details),
    Autoconnect(String, bool),
    Metered(String, bool),
    Reveal(String),
    Revealed(Option<Result<String, String>>),
    Conceal,
    DnsInput(String),
    ApplyDns(String, Option<String>),
    ToggleHidden,
    HiddenSsid(String),
    HiddenSecure(bool),
    HiddenPassword(Secret),
    JoinHidden,
    ToggleHotspot,
    HotspotSsid(String),
    HotspotPassword(Secret),
    GeneratePassword,
    StartHotspot,
    StopHotspot(String),
    ProxyLoaded(ProxySettings),
    ProxyMode(ProxyMode),
    ProxyHost(usize, String),
    ProxyPort(usize, String),
    ProxyIgnore(String),
    ProxyUrl(String),
    ApplyProxy,
    ProxyApplied(Option<Result<String, String>>),
    Done(Option<Result<String, String>>),
}

#[allow(clippy::struct_excessive_bools)] // Independent loading, scanning and validation flags
#[derive(Debug, Default)]
pub struct Page {
    state: NetworkState,
    loading: bool,
    loaded: bool,
    scanning: bool,
    expanded: Option<Expanded>,
    password: Secret,
    busy: Option<String>,
    error: Option<String>,
    generation: Generation,
    pending: u32,
    details: Option<Details>,
    details_generation: Generation,
    details_loading: bool,
    revealed: Option<Secret>,
    dns: String,
    dns_invalid: bool,
    hidden_ssid: String,
    hidden_secure: bool,
    hidden_password: Secret,
    hotspot_ssid: String,
    hotspot_password: Secret,
    proxy: ProxySettings,
    proxy_draft: ProxySettings,
    proxy_saved: bool,
}

impl Pane for Page {
    type Msg = Msg;

    const KEYWORDS: &'static [&'static str] = &[
        "wifi",
        "wi-fi",
        "wireless",
        "internet",
        "ethernet",
        "wired",
        "hotspot",
        "airplane",
        "proxy",
        "dns",
        "мережа",
        "інтернет",
        "дротова",
        "бездротова",
        "вайфай",
        "точка доступу",
        "політ",
        "проксі",
    ];
    const POLLS: bool = true;

    fn open(&mut self, _c: Ctx, _config: &ConfigStore) -> Task<Msg> {
        if self.loaded || self.loading {
            return Task::none();
        }
        if self.hotspot_ssid.is_empty() {
            self.hotspot_ssid = crate::services::system::hostname();
        }
        Task::batch([
            self.refresh(Rescan::Auto),
            Task::perform(blocking(network::read_proxy), |proxy| Msg::ProxyLoaded(proxy.unwrap_or_default())),
        ])
    }

    fn update(&mut self, msg: Msg, _c: Ctx, _config: &mut ConfigStore) -> Task<Msg> {
        Page::update(self, msg)
    }

    fn view<'a>(&'a self, c: Ctx, _config: &'a ConfigStore) -> Element<'a, Msg> {
        Page::view(self, &c)
    }

    fn tick(&mut self, _c: Ctx) -> Task<Msg> {
        Task::batch([self.refresh(Rescan::No), self.load_details()])
    }
}

impl Page {
    fn refresh(&mut self, rescan: Rescan) -> Task<Msg> {
        if self.loading || self.pending > 0 {
            return Task::none();
        }
        self.loading = true;
        self.scanning = rescan == Rescan::Yes;
        let generation = self.generation.current();
        Task::perform(blocking(move || network::read(rescan)), move |state| {
            Msg::Loaded(generation, state.unwrap_or_default())
        })
    }

    #[must_use]
    pub const fn proxy_draft(&self) -> &ProxySettings {
        &self.proxy_draft
    }

    fn active_uuid(&self, point: &AccessPoint) -> Option<String> {
        if point.in_use {
            self.state
                .wifi_device
                .as_deref()
                .and_then(|device| self.state.active_profile(device))
                .map(|profile| profile.uuid.clone())
                .or_else(|| point.profile.clone())
        } else {
            point.profile.clone()
        }
    }

    fn target(&self) -> Option<(Option<String>, Option<String>, bool)> {
        match self.expanded.as_ref()? {
            Expanded::Wifi(ssid) => {
                let point = self.find(ssid)?;
                point.in_use.then(|| (self.state.wifi_device.clone(), self.active_uuid(point), true))
            }
            Expanded::Wired(device) => {
                let uuid = self.state.active_profile(device).map(|profile| profile.uuid.clone());
                Some((Some(device.clone()), uuid, false))
            }
            Expanded::Hidden | Expanded::Hotspot => None,
        }
    }

    fn load_details(&mut self) -> Task<Msg> {
        let Some((device, uuid, wifi)) = self.target() else {
            return Task::none();
        };
        if self.details_loading {
            return Task::none();
        }
        self.details_loading = true;
        let generation = self.details_generation.current();
        Task::perform(
            blocking(move || network::read_details(device.as_deref(), uuid.as_deref(), wifi)),
            move |details| Msg::DetailsLoaded(generation, details.unwrap_or_default()),
        )
    }

    fn expand(&mut self, target: Option<Expanded>) -> Task<Msg> {
        self.expanded = target;
        self.details = None;
        self.details_loading = false;
        self.details_generation.bump();
        self.revealed = None;
        self.password.clear();
        self.dns.clear();
        self.dns_invalid = false;
        self.load_details()
    }

    fn toggle(&mut self, target: Expanded) -> Task<Msg> {
        let next = (self.expanded.as_ref() != Some(&target)).then_some(target);
        self.expand(next)
    }

    fn settings_mut(&mut self) -> Option<&mut network::ProfileSettings> {
        self.details.as_mut().and_then(|d| d.settings.as_mut())
    }

    fn run(
        &mut self, busy: Option<String>, job: impl FnOnce() -> Result<String, String> + Send + 'static,
    ) -> Task<Msg> {
        self.busy = busy;
        self.error = None;
        self.generation.bump();
        self.loading = false;
        self.scanning = false;
        self.pending += 1;
        Task::perform(blocking(job), Msg::Done)
    }

    fn settings_changed(&mut self) {
        self.details_generation.bump();
        self.details_loading = false;
    }

    #[allow(clippy::too_many_lines)] // One arm per message keeps the panel's state machine in one place
    pub fn update(&mut self, msg: Msg) -> Task<Msg> {
        if matches!(
            msg,
            Msg::ProxyMode(_) | Msg::ProxyHost(..) | Msg::ProxyPort(..) | Msg::ProxyIgnore(_) | Msg::ProxyUrl(_)
        ) {
            self.proxy_saved = false;
        }
        match msg {
            Msg::Loaded(generation, _) if !self.generation.is_current(generation) => {}
            Msg::Loaded(_, state) => {
                self.state = state;
                self.loading = false;
                self.loaded = true;
                self.scanning = false;
                if let Some(Expanded::Wifi(ssid)) = &self.expanded
                    && self.find(ssid).is_none()
                {
                    self.expanded = None;
                }
            }
            Msg::Refresh => return self.refresh(Rescan::Auto),
            Msg::Rescan => {
                if self.pending == 0 {
                    self.generation.bump();
                    self.loading = false;
                }
                return self.refresh(Rescan::Yes);
            }
            Msg::ToggleWifi(on) => {
                self.state.wifi_enabled = on;
                return self.run(None, move || network::set_wifi(on));
            }
            Msg::Airplane(on) => {
                self.state.airplane = on;
                return self.run(None, move || network::set_airplane(on));
            }
            Msg::Select(ssid) => {
                let Some(point) = self.find(&ssid).cloned() else {
                    return Task::none();
                };
                if !expands(&point) {
                    let name = point.ssid.clone();
                    let profile = point.profile.clone();
                    return self.run(Some(point.ssid), move || network::connect(&name, profile.as_deref(), None));
                }
                return self.toggle(Expanded::Wifi(ssid));
            }
            Msg::SelectWired(device) => return self.toggle(Expanded::Wired(device)),
            Msg::Cancel => return self.expand(None),
            Msg::Password(password) => self.password = password,
            Msg::Connect(ssid) => {
                let password = self.password.take();
                let profile = self.find(&ssid).and_then(|point| point.profile.clone());
                self.expanded = None;
                let name = ssid.clone();
                return self.run(Some(ssid), move || network::connect(&name, profile.as_deref(), Some(&password)));
            }
            Msg::Disconnect => {
                self.expanded = None;
                let Some(device) = self.state.wifi_device.clone() else {
                    return Task::none();
                };
                return self.run(None, move || network::disconnect(&device));
            }
            Msg::DisconnectDevice(device) => {
                self.expanded = None;
                return self.run(None, move || network::disconnect(&device));
            }
            Msg::Forget(uuid) => {
                if matches!(self.expanded, Some(Expanded::Wifi(_))) {
                    self.expanded = None;
                }
                self.state.profiles.retain(|profile| profile.uuid != uuid);
                return self.run(None, move || network::forget(&uuid));
            }
            Msg::DetailsLoaded(generation, _) if !self.details_generation.is_current(generation) => {}
            Msg::DetailsLoaded(_, details) => {
                self.details_loading = false;
                if !self.dns_edited() {
                    self.dns = saved_dns(Some(&details));
                    self.dns_invalid = false;
                }
                self.details = Some(details);
            }
            Msg::Autoconnect(uuid, on) => {
                if let Some(settings) = self.settings_mut() {
                    settings.autoconnect = on;
                }
                if let Some(profile) = self.state.profiles.iter_mut().find(|p| p.uuid == uuid) {
                    profile.autoconnect = on;
                }
                self.settings_changed();
                return self.run(None, move || network::set_autoconnect(&uuid, on));
            }
            Msg::Metered(uuid, on) => {
                let metered = if on { Metered::Yes } else { Metered::No };
                if let Some(settings) = self.settings_mut() {
                    settings.metered = metered;
                }
                self.settings_changed();
                return self.run(None, move || network::set_metered(&uuid, metered));
            }
            Msg::Reveal(uuid) => {
                self.error = None;
                return Task::perform(blocking(move || network::reveal_password(&uuid)), Msg::Revealed);
            }
            Msg::Revealed(result) => match result {
                Some(Ok(secret)) => self.revealed = Some(Secret(secret)),
                Some(Err(err)) => self.error = Some(err),
                None => {}
            },
            Msg::Conceal => self.revealed = None,
            Msg::DnsInput(value) => {
                self.dns_invalid = network::parse_dns_list(&value).is_none();
                self.dns = value;
            }
            Msg::ApplyDns(uuid, device) => {
                let Some(servers) = network::parse_dns_list(&self.dns) else {
                    self.dns_invalid = true;
                    return Task::none();
                };
                if let Some(settings) = self.settings_mut() {
                    settings.custom_dns.clone_from(&servers);
                }
                self.settings_changed();
                return self.run(None, move || network::set_dns(&uuid, device.as_deref(), &servers));
            }
            Msg::ToggleHidden => return self.toggle(Expanded::Hidden),
            Msg::HiddenSsid(ssid) => self.hidden_ssid = ssid,
            Msg::HiddenSecure(secure) => self.hidden_secure = secure,
            Msg::HiddenPassword(password) => self.hidden_password = password,
            Msg::JoinHidden => {
                let ssid = self.hidden_ssid.trim().to_string();
                if !self.hidden_ready() {
                    return Task::none();
                }
                let password = self.hidden_secure.then(|| self.hidden_password.take());
                self.expanded = None;
                self.hidden_ssid.clear();
                return self.run(Some(ssid.clone()), move || network::connect_hidden(&ssid, password.as_deref()));
            }
            Msg::ToggleHotspot => return self.toggle(Expanded::Hotspot),
            Msg::HotspotSsid(ssid) => self.hotspot_ssid = ssid,
            Msg::HotspotPassword(password) => self.hotspot_password = password,
            Msg::GeneratePassword => {
                if let Some(password) = network::generate_password() {
                    self.hotspot_password = Secret(password);
                }
            }
            Msg::StartHotspot => {
                if !self.hotspot_ready() {
                    return Task::none();
                }
                let ssid = self.hotspot_ssid.trim().to_string();
                let password = self.hotspot_password.as_str().to_string();
                self.expanded = None;
                return self
                    .run(Some(network::HOTSPOT_NAME.to_string()), move || network::start_hotspot(&ssid, &password));
            }
            Msg::StopHotspot(uuid) => {
                self.revealed = None;
                return self.run(None, move || network::stop_profile(&uuid));
            }
            Msg::ProxyLoaded(proxy) => {
                self.proxy_draft = proxy.clone();
                self.proxy = proxy;
            }
            Msg::ProxyMode(mode) => {
                self.proxy_draft.mode = mode;
                if mode == ProxyMode::None {
                    return self.update(Msg::ApplyProxy);
                }
            }
            Msg::ProxyHost(kind, host) => self.proxy_draft.endpoint_mut(kind).host = host,
            Msg::ProxyPort(kind, port) => {
                self.proxy_draft.endpoint_mut(kind).port = port.chars().filter(char::is_ascii_digit).take(5).collect();
            }
            Msg::ProxyIgnore(hosts) => self.proxy_draft.ignore_hosts = hosts,
            Msg::ProxyUrl(url) => self.proxy_draft.autoconfig_url = url,
            Msg::ApplyProxy => {
                if self.proxy_draft.invalid_port().is_some() {
                    return Task::none();
                }
                let draft = self.proxy_draft.clone();
                self.error = None;
                return Task::perform(blocking(move || network::write_proxy(&draft)), Msg::ProxyApplied);
            }
            Msg::ProxyApplied(result) => match result {
                Some(Ok(_)) => {
                    self.proxy = self.proxy_draft.clone();
                    self.proxy_saved = true;
                }
                Some(Err(err)) => self.error = Some(err),
                None => {}
            },
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
                return Task::batch([self.refresh(Rescan::Auto), self.load_details()]);
            }
        }
        Task::none()
    }

    #[must_use]
    pub fn hidden_ready(&self) -> bool {
        network::valid_ssid(&self.hidden_ssid)
            && (!self.hidden_secure || network::valid_psk(self.hidden_password.as_str()))
    }

    #[must_use]
    pub fn hotspot_ready(&self) -> bool {
        network::valid_ssid(&self.hotspot_ssid) && network::valid_psk(self.hotspot_password.as_str())
    }

    fn dns_edited(&self) -> bool {
        self.details.is_some() && self.dns != saved_dns(self.details.as_ref())
    }

    fn find(&self, ssid: &str) -> Option<&AccessPoint> {
        self.state.networks.iter().find(|point| point.ssid == ssid)
    }

    fn sharing_as(&self) -> &str {
        self.state.hotspot_ssid.as_deref().unwrap_or(&self.hotspot_ssid)
    }

    fn view(&self, c: &Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        if !self.state.available {
            return if self.loaded {
                widgets::empty_state(t, Icon::WifiOff, c.t(K::NetworkUnavailable), c.t(K::NetworkUnavailableHint))
            } else {
                widgets::empty_state(t, Icon::Wifi, c.t(K::Loading), "")
            };
        }
        let status = if self.state.airplane {
            c.t(T::AirplaneOn).to_string()
        } else if !self.state.wifi_hardware {
            c.t(T::HardwareOff).to_string()
        } else if !self.state.wifi_enabled {
            c.t(K::WifiOff).to_string()
        } else if self.state.hotspot().is_some() {
            format!("{} “{}”", c.t(T::HotspotOn), self.sharing_as())
        } else if let Some(active) = self.state.active() {
            format!("{} {}", c.t(K::ConnectedTo), active.ssid)
        } else {
            c.t(K::NotConnected).to_string()
        };
        let radio_on = self.state.wifi_enabled && !self.state.airplane;
        let error = self
            .error
            .as_deref()
            .map(|error| if error == network::WRONG_PASSWORD { c.t(T::WrongPassword) } else { error });
        column![
            widgets::section_hero(
                t,
                if radio_on { Icon::Wifi } else { Icon::WifiOff },
                super::Panel::Network.color(),
                "Wi-Fi",
                status,
                Some(widgets::switch(t, self.state.wifi_enabled, Msg::ToggleWifi)),
            ),
            error_banner(t, error),
            self.radios(c),
            radio_on.then(|| self.networks(c)),
            (!self.state.wired.is_empty()).then(|| self.wired(c)),
            self.known(c),
            self.proxy.available.then(|| self.proxy_group(c)),
        ]
        .spacing(theme::SECTION_GAP)
        .into()
    }

    fn radios(&self, c: &Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        let mut group = Group::new(t);
        if self.state.rfkill {
            group = group.push(
                Row::new(t, c.t(T::Airplane))
                    .subtitle(c.t(T::AirplaneHint))
                    .prefix(widgets::tile(Icon::Airplane, theme::ramp_color(theme::Accent::Amber, 3), 28.0))
                    .suffix(widgets::switch(t, self.state.airplane, Msg::Airplane))
                    .build(),
            );
        }
        if self.state.wifi_device.is_some() {
            let hotspot = self.state.hotspot();
            let subtitle = match hotspot {
                Some(_) => format!("{} “{}”", c.t(T::HotspotOn), self.sharing_as()),
                None => c.t(T::HotspotHint).to_string(),
            };
            let busy = self.busy.as_deref() == Some(network::HOTSPOT_NAME);
            let usable = self.state.wifi_enabled && !self.state.airplane;
            let suffix: Element<'_, Msg> = if busy {
                widgets::spinner_label(t, c.t(T::Busy))
            } else if let Some(profile) = hotspot {
                row![
                    widgets::status_pill(t, Tone::Ok, c.t(K::Connected)),
                    widgets::secondary(t, c.t(T::Stop), Some(Msg::StopHotspot(profile.uuid.clone()))),
                ]
                .spacing(8)
                .align_y(Alignment::Center)
                .into()
            } else if usable {
                chevron(t, self.expanded == Some(Expanded::Hotspot))
            } else {
                column![].into()
            };
            let mut line = Row::new(t, c.t(T::Hotspot))
                .subtitle(subtitle)
                .prefix(widgets::tile(Icon::Hotspot, theme::ramp_color(theme::Accent::Malachite, 3), 28.0))
                .suffix(suffix)
                .dimmed(!usable && hotspot.is_none());
            if hotspot.is_none() && !busy && usable {
                line = line.on_press(Msg::ToggleHotspot);
            }
            group = group.push(line.build());
            if let Some(profile) = hotspot {
                group = group.push(self.password_row(c, &profile.uuid));
            } else if self.expanded == Some(Expanded::Hotspot) && usable {
                group = group.push(self.hotspot_form(c));
            }
        }
        if group.is_empty() {
            return column![].into();
        }
        group.build()
    }

    fn hotspot_form(&self, c: &Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        let ready = self.hotspot_ready();
        let invalid_password = !self.hotspot_password.is_empty() && !network::valid_psk(self.hotspot_password.as_str());
        let long_ssid = ssid_too_long(&self.hotspot_ssid);
        sheet(
            t,
            vec![
                form_row(
                    t,
                    c.t(T::HotspotName),
                    long_ssid.then(|| c.t(T::SsidTooLong)),
                    widgets::field(t, "Glide", &self.hotspot_ssid, Msg::HotspotSsid, None, long_ssid)
                        .width(Fill)
                        .into(),
                ),
                form_row(
                    t,
                    c.t(T::HotspotPassword),
                    Some(c.t(T::HotspotPasswordHint)),
                    row![
                        widgets::field(
                            t,
                            c.t(K::Password),
                            self.hotspot_password.as_str(),
                            |value| Msg::HotspotPassword(Secret(value)),
                            ready.then_some(Msg::StartHotspot),
                            invalid_password,
                        )
                        .width(Fill),
                        widgets::secondary(t, c.t(T::Generate), Some(Msg::GeneratePassword)),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center)
                    .into(),
                ),
                note(t, Icon::Info, c.t(T::HotspotWarning)),
                actions(vec![
                    widgets::secondary(t, c.t(K::Cancel), Some(Msg::ToggleHotspot)),
                    widgets::primary(t, c.t(T::Start), ready.then_some(Msg::StartHotspot)),
                ]),
            ],
        )
    }

    fn networks(&self, c: &Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        let action: Element<'_, Msg> = if self.scanning {
            widgets::spinner_label(t, c.t(K::Scanning))
        } else {
            widgets::icon_button(t, Icon::Refresh, Some(Msg::Rescan))
        };
        let mut group = Group::new(t).title(c.t(K::Networks)).suffix(action);
        if self.state.networks.is_empty() {
            group = group.push(widgets::list_empty(t, Icon::WifiOff, c.t(K::NoNetworks), ""));
        }
        for point in &self.state.networks {
            let busy = self.busy.as_deref() == Some(point.ssid.as_str());
            let subtitle = if busy {
                c.t(K::Connecting)
            } else if point.in_use {
                c.t(K::Connected)
            } else if point.known {
                c.t(K::Saved)
            } else if point.secured {
                c.t(K::Secured)
            } else {
                c.t(K::Open)
            };
            let expanded = self.expanded.as_ref() == Some(&Expanded::Wifi(point.ssid.clone()));
            let mut suffix = row![].spacing(10).align_y(Alignment::Center);
            if point.in_use {
                suffix = suffix.push(widgets::icon(Icon::Check, 18.0, t.acc_text()));
            }
            if point.secured {
                suffix = suffix.push(widgets::icon(Icon::Lock, 14.0, t.fg_3()));
            }
            if expands(point) {
                suffix = suffix.push(chevron(t, expanded));
            }
            group = group.push(
                Row::new(t, point.ssid.clone())
                    .subtitle(subtitle)
                    .prefix(widgets::icon(
                        Icon::for_signal(point.signal),
                        20.0,
                        if point.in_use { t.acc_text() } else { t.fg_2() },
                    ))
                    .suffix(suffix)
                    .on_press(Msg::Select(point.ssid.clone()))
                    .build(),
            );
            if expanded {
                group = group.push(if point.in_use {
                    self.connected_sheet(c, point)
                } else {
                    self.password_prompt(c, point)
                });
            }
        }
        let hidden_open = self.expanded == Some(Expanded::Hidden);
        group = group.push(
            Row::new(t, c.t(T::JoinHidden))
                .subtitle(c.t(T::JoinHiddenHint))
                .prefix(widgets::icon(Icon::Plus, 18.0, t.fg_2()))
                .suffix(chevron(t, hidden_open))
                .on_press(Msg::ToggleHidden)
                .build(),
        );
        if hidden_open {
            group = group.push(self.hidden_form(c));
        }
        group.build()
    }

    fn hidden_form(&self, c: &Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        let ready = self.hidden_ready();
        let long_ssid = ssid_too_long(&self.hidden_ssid);
        let mut rows = vec![
            form_row(
                t,
                c.t(T::Ssid),
                long_ssid.then(|| c.t(T::SsidTooLong)),
                widgets::field(t, "", &self.hidden_ssid, Msg::HiddenSsid, None, long_ssid).width(Fill).into(),
            ),
            form_row(
                t,
                c.t(T::Security),
                None,
                widgets::segmented(
                    t,
                    vec![(false, c.t(T::SecurityNone).to_string()), (true, c.t(T::WpaPersonal).to_string())],
                    self.hidden_secure,
                    Msg::HiddenSecure,
                ),
            ),
        ];
        if self.hidden_secure {
            rows.push(form_row(
                t,
                c.t(K::Password),
                None,
                widgets::field(
                    t,
                    c.t(K::Password),
                    self.hidden_password.as_str(),
                    |value| Msg::HiddenPassword(Secret(value)),
                    ready.then_some(Msg::JoinHidden),
                    false,
                )
                .secure(true)
                .width(Fill)
                .into(),
            ));
        }
        rows.push(actions(vec![
            widgets::secondary(t, c.t(K::Cancel), Some(Msg::ToggleHidden)),
            widgets::primary(t, c.t(T::Join), ready.then_some(Msg::JoinHidden)),
        ]));
        sheet(t, rows)
    }

    fn connected_sheet<'a>(&'a self, c: &Ctx, point: &'a AccessPoint) -> Element<'a, Msg> {
        let t = c.theme;
        let uuid = self.active_uuid(point);
        let details = self.details.as_ref();
        let mut info = vec![(c.t(K::Signal).to_string(), format!("{}%", point.signal))];
        if let Some(details) = details {
            if !details.frequency.is_empty() {
                let channel = if details.channel.is_empty() {
                    String::new()
                } else {
                    format!(" · {} {}", c.t(T::Channel), details.channel)
                };
                info.push((c.t(T::Frequency).to_string(), format!("{}{channel}", details.frequency)));
            }
            let security = if details.security.is_empty() {
                details.settings.as_ref().map(|settings| security_label(c, &settings.key_mgmt)).unwrap_or_default()
            } else {
                details.security.clone()
            };
            if !security.is_empty() {
                info.push((c.t(T::Security).to_string(), security));
            }
            info.extend(address_lines(c, details));
        }
        let mut rows: Vec<Element<'a, Msg>> = vec![lines(t, info)];
        if details.is_none() {
            rows.push(loading_row(c));
        }
        if let Some(uuid) = &uuid {
            rows.extend(self.profile_controls(c, uuid, details, true));
            if point.secured {
                rows.push(self.password_row(c, uuid));
            }
        }
        let mut buttons = vec![widgets::secondary(t, c.t(K::Disconnect), Some(Msg::Disconnect))];
        if let Some(uuid) = uuid {
            buttons.push(widgets::danger(t, c.t(K::Forget), Some(Msg::Forget(uuid))));
        }
        rows.push(actions(buttons));
        sheet(t, rows)
    }

    fn profile_controls<'a>(
        &'a self, c: &Ctx, uuid: &str, details: Option<&'a Details>, active: bool,
    ) -> Vec<Element<'a, Msg>> {
        let t = c.theme;
        let Some(settings) = details.and_then(|details| details.settings.as_ref()) else {
            return Vec::new();
        };
        let metered_subtitle = if settings.metered == Metered::Auto { T::MeteredGuess } else { T::MeteredHint };
        let mut rows: Vec<Element<'a, Msg>> = [
            (T::AutoConnect, T::AutoConnectHint, settings.autoconnect, Msg::Autoconnect as fn(_, _) -> _),
            (T::MeteredTitle, metered_subtitle, settings.metered == Metered::Yes, Msg::Metered),
        ]
        .into_iter()
        .map(|(title, hint, value, msg)| {
            let uuid = uuid.to_string();
            Row::new(t, c.t(title))
                .subtitle(c.t(hint))
                .suffix(widgets::switch(t, value, move |on| msg(uuid.clone(), on)))
                .build()
        })
        .collect();
        if settings.ipv4_method != "shared" {
            let device = if active {
                details.map(|details| details.device.clone()).filter(|device| !device.is_empty())
            } else {
                None
            };
            let apply = (!self.dns_invalid && self.dns_edited()).then(|| Msg::ApplyDns(uuid.to_string(), device));
            rows.push(form_row(
                t,
                c.t(T::CustomDns),
                Some(if self.dns_invalid { c.t(T::DnsInvalid) } else { c.t(T::CustomDnsHint) }),
                row![
                    widgets::field(t, c.t(T::Automatic), &self.dns, Msg::DnsInput, apply.clone(), self.dns_invalid,)
                        .width(Fill),
                    widgets::secondary(t, c.t(K::Apply), apply),
                ]
                .spacing(8)
                .align_y(Alignment::Center)
                .into(),
            ));
        }
        rows
    }

    fn password_row<'a>(&'a self, c: &Ctx, uuid: &str) -> Element<'a, Msg> {
        let t = c.theme;
        let (value, button) = match &self.revealed {
            Some(secret) => {
                (secret.as_str().to_string(), widgets::secondary(t, c.t(T::HidePassword), Some(Msg::Conceal)))
            }
            None => (
                "••••••••".to_string(),
                widgets::secondary(t, c.t(T::ShowPassword), Some(Msg::Reveal(uuid.to_string()))),
            ),
        };
        Row::new(t, c.t(T::NetworkPassword))
            .subtitle(value)
            .prefix(widgets::icon(Icon::Key, 18.0, t.fg_2()))
            .suffix(button)
            .build()
    }

    fn password_prompt<'a>(&'a self, c: &Ctx, point: &'a AccessPoint) -> Element<'a, Msg> {
        let t = c.theme;
        let ready = !self.password.is_empty();
        let connect = Msg::Connect(point.ssid.clone());
        sheet(
            t,
            vec![
                form_row(
                    t,
                    c.t(K::Password),
                    Some(c.t(K::PasswordHint)),
                    widgets::field(
                        t,
                        c.t(K::Password),
                        self.password.as_str(),
                        |value| Msg::Password(Secret(value)),
                        ready.then(|| connect.clone()),
                        false,
                    )
                    .secure(true)
                    .width(Length::Fill)
                    .into(),
                ),
                actions(vec![
                    widgets::secondary(t, c.t(K::Cancel), Some(Msg::Cancel)),
                    widgets::primary(t, c.t(K::Connect), ready.then_some(connect)),
                ]),
            ],
        )
    }

    fn wired(&self, c: &Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        let mut wired = Group::new(t).title(c.t(K::Wired));
        for device in &self.state.wired {
            let expanded = self.expanded.as_ref() == Some(&Expanded::Wired(device.device.clone()));
            let mut line = Row::new(t, device.device.clone())
                .subtitle(if device.connected { device.connection.clone() } else { c.t(K::CableUnplugged).to_string() })
                .prefix(widgets::icon(Icon::Ethernet, 20.0, if device.connected { t.acc_text() } else { t.fg_2() }));
            line = if device.connected {
                line.suffix(
                    row![widgets::status_pill(t, Tone::Ok, c.t(K::Connected)), chevron(t, expanded)]
                        .spacing(10)
                        .align_y(Alignment::Center),
                )
                .on_press(Msg::SelectWired(device.device.clone()))
            } else {
                line.suffix(widgets::pill(t, Tone::Neutral, c.t(K::Disconnected)))
            };
            wired = wired.push(line.build());
            if expanded && device.connected {
                let mut rows = vec![match &self.details {
                    Some(details) => lines(t, address_lines(c, details)),
                    None => loading_row(c),
                }];
                if let Some(profile) = self.state.active_profile(&device.device) {
                    rows.extend(self.profile_controls(c, &profile.uuid, self.details.as_ref(), true));
                }
                rows.push(actions(vec![widgets::secondary(
                    t,
                    c.t(K::Disconnect),
                    Some(Msg::DisconnectDevice(device.device.clone())),
                )]));
                wired = wired.push(sheet(t, rows));
            }
        }
        wired.build()
    }

    fn known(&self, c: &Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        let mut group = Group::new(t).title(c.t(T::KnownNetworks)).description(c.t(T::KnownNetworksHint));
        let saved: Vec<&Profile> = self.state.saved_wifi().collect();
        if saved.is_empty() {
            return group.push(widgets::list_empty(t, Icon::Wifi, c.t(T::NoKnownNetworks), "")).build();
        }
        for profile in saved {
            let in_range =
                self.state.networks.iter().any(|point| point.profile.as_deref() == Some(profile.uuid.as_str()));
            let status = if profile.active {
                c.t(K::Connected)
            } else if in_range {
                c.t(T::InRange)
            } else {
                c.t(T::OutOfRange)
            };
            let auto = if profile.autoconnect { c.t(T::Automatic) } else { c.t(T::Manual) };
            let uuid = profile.uuid.clone();
            let toggle_uuid = uuid.clone();
            group = group.push(
                Row::new(t, profile.name.clone())
                    .subtitle(format!("{status} · {auto}"))
                    .prefix(widgets::icon(Icon::Wifi, 18.0, if profile.active { t.acc_text() } else { t.fg_3() }))
                    .suffix(
                        row![
                            widgets::switch(t, profile.autoconnect, move |on| {
                                Msg::Autoconnect(toggle_uuid.clone(), on)
                            }),
                            widgets::icon_button(t, Icon::Trash, Some(Msg::Forget(uuid))),
                        ]
                        .spacing(10)
                        .align_y(Alignment::Center),
                    )
                    .build(),
            );
        }
        group.build()
    }

    #[allow(clippy::too_many_lines)] // Three proxy modes, each with its own form rows
    fn proxy_group(&self, c: &Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        let draft = &self.proxy_draft;
        let mut group = Group::new(t).title(c.t(T::Proxy)).description(c.t(T::ProxyHint)).push(
            Row::new(t, c.t(T::ProxyModeTitle))
                .prefix(widgets::icon(Icon::Globe, 18.0, t.fg_2()))
                .suffix(widgets::segmented(
                    t,
                    vec![
                        (ProxyMode::None, c.t(T::ProxyOff).to_string()),
                        (ProxyMode::Manual, c.t(T::Manual).to_string()),
                        (ProxyMode::Auto, c.t(T::Automatic).to_string()),
                    ],
                    draft.mode,
                    Msg::ProxyMode,
                ))
                .build(),
        );
        let dirty = *draft != self.proxy;
        match draft.mode {
            ProxyMode::None => return group.build(),
            ProxyMode::Auto => {
                group = group.push(form_row(
                    t,
                    c.t(T::ProxyUrl),
                    None,
                    widgets::field(
                        t,
                        "http://wpad/wpad.dat",
                        &draft.autoconfig_url,
                        Msg::ProxyUrl,
                        Some(Msg::ApplyProxy),
                        false,
                    )
                    .width(Fill)
                    .into(),
                ));
            }
            ProxyMode::Manual => {
                let invalid = draft.invalid_port();
                for (index, label) in ["HTTP", "HTTPS", "SOCKS"].into_iter().enumerate() {
                    let endpoint = draft.endpoint(index);
                    group = group.push(form_row(
                        t,
                        label,
                        None,
                        row![
                            widgets::field(
                                t,
                                c.t(T::Host),
                                &endpoint.host,
                                move |host| Msg::ProxyHost(index, host),
                                Some(Msg::ApplyProxy),
                                false,
                            )
                            .width(Length::FillPortion(3)),
                            widgets::field(
                                t,
                                c.t(T::Port),
                                &endpoint.port,
                                move |port| Msg::ProxyPort(index, port),
                                Some(Msg::ApplyProxy),
                                invalid == Some(index),
                            )
                            .width(Length::FillPortion(1)),
                        ]
                        .spacing(8)
                        .into(),
                    ));
                }
                group = group.push(form_row(
                    t,
                    c.t(T::ProxyIgnore),
                    invalid.map(|_| c.t(T::ProxyPortInvalid)),
                    widgets::field(
                        t,
                        "localhost, 127.0.0.0/8",
                        &draft.ignore_hosts,
                        Msg::ProxyIgnore,
                        Some(Msg::ApplyProxy),
                        false,
                    )
                    .width(Fill)
                    .into(),
                ));
            }
        }
        let status: Element<'_, Msg> = if self.proxy_saved && !dirty {
            widgets::status_pill(t, Tone::Ok, c.t(T::Applied))
        } else {
            column![].into()
        };
        group
            .push(
                container(
                    row![
                        container(status).width(Fill),
                        widgets::primary(
                            t,
                            c.t(K::Apply),
                            (dirty && draft.invalid_port().is_none()).then_some(Msg::ApplyProxy),
                        )
                    ]
                    .align_y(Alignment::Center),
                )
                .padding([6.0, theme::ROW_PADDING_X]),
            )
            .build()
    }
}

fn saved_dns(details: Option<&Details>) -> String {
    details
        .and_then(|details| details.settings.as_ref())
        .map(|settings| settings.custom_dns.join(", "))
        .unwrap_or_default()
}

fn loading_row<'a>(c: &Ctx) -> Element<'a, Msg> {
    container(widgets::spinner_label(c.theme, c.t(K::Loading))).padding([4.0, theme::ROW_PADDING_X]).into()
}

fn address_lines(c: &Ctx, details: &Details) -> Vec<(String, String)> {
    [
        (T::IpAddress, details.ipv4.join(", ")),
        (T::Ipv6Address, details.ipv6.join(", ")),
        (T::Gateway, details.gateway.clone()),
        (T::Dns, details.dns.join(", ")),
        (T::MacAddress, details.mac.clone()),
        (T::LinkSpeed, details.speed.clone()),
    ]
    .into_iter()
    .filter(|(_, value)| !value.is_empty())
    .map(|(key, value)| (c.t(key).to_string(), value))
    .collect()
}

fn security_label(c: &Ctx, key_mgmt: &str) -> String {
    match key_mgmt {
        "" | "none" => c.t(T::SecurityNone).to_string(),
        "wpa-psk" => "WPA2 Personal".to_string(),
        "sae" => "WPA3 Personal".to_string(),
        "wpa-eap" => "WPA2 Enterprise".to_string(),
        "owe" => "Enhanced Open".to_string(),
        other => other.to_uppercase(),
    }
}

#[must_use]
pub fn ssid_too_long(ssid: &str) -> bool {
    ssid.trim().len() > network::MAX_SSID_BYTES
}

#[must_use]
pub const fn expands(point: &AccessPoint) -> bool {
    point.in_use || (point.secured && !point.known)
}

use glide_sdk::widget::{self as sdk, Fact};
use iced::widget::{column, container, row};
use iced::{Alignment, Element, Task};

use super::common::{Choice, error_banner};
use super::{Category, Ctx, Pane};
use crate::i18n::{Key as K, Lang};
use crate::icons::Icon;
use crate::services::config::ConfigStore;
use crate::services::network::{
    self, AccessPoint, Band, Details, Metered, NetworkState, ProxyMode, ProxySettings, Rescan, Secret,
};
use crate::services::run::{Generation, Jobs, blocking};
use crate::theme::{self, Tone};
use crate::widgets::{self, Group, Row};

mod tr {
    crate::i18n::catalogue! {
        WifiOff => "Wi-Fi is off", "Wi-Fi вимкнено";
        TurnOnHint => "Turn Wi-Fi on to see networks nearby", "Увімкніть Wi-Fi, щоб побачити мережі поруч";
        HardwareOff => "Turned off by a hardware switch", "Вимкнено апаратним перемикачем";
        AirplaneOn => "Airplane mode is on", "Увімкнено режим польоту";
        ConnectedTo => "Connected to", "Підключено до";
        NotConnected => "Not connected", "Не підключено";
        ChooseNetwork => "Choose a network nearby", "Виберіть мережу поруч";
        Signal => "Signal", "Сигнал";
        Internet => "internet available", "інтернет доступний";
        Mbps => "Mbit/s", "Мбіт/с";
        Ghz2 => "2.4 GHz", "2,4 ГГц";
        Ghz5 => "5 GHz", "5 ГГц";
        Ghz6 => "6 GHz", "6 ГГц";
        Channel => "channel", "канал";
        Nearby => "Networks nearby", "Мережі поруч";
        Updated => "Updated", "Оновлено";
        NoNetworks => "No networks in range", "Немає мереж поблизу";
        Connected => "Connected", "Підключено";
        Connecting => "Connecting…", "Підключення…";
        Saved => "Saved", "Збережена";
        Secured => "Secured", "Захищена";
        OpenNetwork => "Open network", "Відкрита мережа";
        Unencrypted => "traffic is not encrypted", "трафік не шифрується";
        Enterprise => "Enterprise (802.1X)", "Корпоративна (802.1X)";
        NeedsAccount => "needs a user name and a certificate", "потрібні ім’я й сертифікат";
        Connect => "Connect", "Підключити";
        Setup => "Set up…", "Налаштувати…";
        IpAddress => "IP address", "IP-адреса";
        Gateway => "Router", "Маршрутизатор";
        Dns => "DNS", "DNS";
        Ipv6 => "IPv6", "IPv6";
        MacAddress => "Hardware address", "Апаратна адреса";
        LinkSpeed => "Link speed", "Швидкість каналу";
        AutoConnect => "Connect automatically", "Підключатися автоматично";
        AutoConnectHint => "As soon as the network is in range", "Щойно мережа з’являється поруч";
        MeteredTitle => "Metered connection", "Лімітне підключення";
        MeteredHint => "Apps that respect it put off large downloads and updates", "Програми, що це враховують, відкладають великі завантаження й оновлення";
        MeteredAuto => "Detect automatically", "Визначати автоматично";
        Yes => "Yes", "Так";
        No => "No", "Ні";
        CustomDns => "Custom DNS servers", "Власні DNS-сервери";
        CustomDnsHint => "Comma-separated. Empty means the router's", "Через кому. Порожньо — від маршрутизатора";
        DnsInvalid => "Enter IP addresses separated by commas", "Введіть IP-адреси через кому";
        Disconnect => "Disconnect", "Відключити";
        Forget => "Forget network", "Забути мережу";
        NetworkPassword => "Network password", "Пароль мережі";
        NetworkPasswordHint => "The network is protected", "Мережа захищена";
        JoinHidden => "Join a hidden network…", "Приєднатися до прихованої мережі…";
        JoinHiddenHint => "For networks that do not show their name", "Для мереж, що не показують своєї назви";
        Ssid => "Network name (SSID)", "Назва мережі (SSID)";
        SsidTooLong => "Too long: a network name fits in 32 bytes", "Задовга: назва мережі вміщує до 32 байтів";
        Security => "Security", "Безпека";
        SecurityNone => "None", "Немає";
        WpaPersonal => "WPA/WPA2 Personal", "WPA/WPA2 Personal";
        Password => "Password", "Пароль";
        Other => "Other", "Інше";
        Wired => "Wired network", "Дротова мережа";
        CableUnplugged => "No cable connected", "Кабель не під’єднано";
        CablePlugged => "Cable connected", "Кабель під’єднано";
        NoCable => "No cable", "Немає кабелю";
        Hotspot => "Wi-Fi hotspot", "Точка доступу Wi-Fi";
        HotspotHint => "Share this computer's internet", "Роздати інтернет цього комп’ютера";
        HotspotLeaves => "Meanwhile Wi-Fi disconnects from", "Wi-Fi на цей час відключиться від";
        HotspotOn => "Sharing as", "Роздає як";
        HotspotName => "Network name", "Назва мережі";
        HotspotPasswordHint => "8 to 63 characters. Other devices type it to join.", "Від 8 до 63 символів. Інші пристрої вводять його для під’єднання.";
        Generate => "Generate", "Згенерувати";
        TurnOn => "Turn on…", "Увімкнути…";
        Start => "Turn on", "Увімкнути";
        Stop => "Turn off", "Вимкнути";
        Busy => "Working…", "Виконується…";
        Airplane => "Airplane mode", "Режим польоту";
        AirplaneHint => "Turns off Wi-Fi, Bluetooth and mobile radios", "Вимикає Wi-Fi, Bluetooth і мобільний зв’язок";
        Proxy => "Proxy", "Проксі";
        ProxyHint => "For apps that read the GNOME settings", "Для програм, що читають налаштування GNOME";
        ProxyOff => "Off", "Вимкнено";
        Manual => "Manual", "Вручну";
        Automatic => "Automatic", "Автоматично";
        WrongPassword => "Wrong password. Check it and try again.", "Неправильний пароль. Перевірте його і спробуйте ще раз.";
        AuthFailed => "The Wi-Fi adapter could not sign in to the network", "Wi-Fi-адаптер не зміг увійти в мережу";
        NoAnswer => "The network did not answer in time", "Мережа не відповіла вчасно";
    }
}

use tr::Key as T;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Prompt {
    Password(String),
    Hidden,
    Hotspot,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Loaded(u64, NetworkState),
    DetailsLoaded(u64, Details),
    ToggleWifi(bool),
    Airplane(bool),
    ToggleDetails,
    Connect(String),
    Setup,
    Cancel,
    Password(Secret),
    TogglePasswordShown,
    SubmitPassword(String),
    Disconnect,
    Forget(String),
    Autoconnect(String, bool),
    Metered(String, Metered),
    DnsInput(String),
    ApplyDns(String),
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
    ProxyApplied(ProxySettings, Option<Result<String, String>>),
    Done(Option<Result<String, String>>),
}

#[allow(clippy::struct_excessive_bools)] // Independent loading, folding and validation flags
#[derive(Debug, Default)]
pub struct Page {
    state: NetworkState,
    networks_read_at: Option<std::time::Instant>,
    jobs: Jobs,
    loaded: bool,
    folded: bool,
    prompt: Option<Prompt>,
    password: Secret,
    password_shown: bool,
    details: Option<Details>,
    details_generation: Generation,
    details_loading: bool,
    dns: String,
    dns_invalid: bool,
    hidden_ssid: String,
    hidden_secure: bool,
    hidden_password: Secret,
    hotspot_ssid: String,
    hotspot_password: Secret,
    proxy: ProxySettings,
}

const METERED: [Metered; 3] = [Metered::Auto, Metered::Yes, Metered::No];
const PROXY_MODES: [ProxyMode; 3] = [ProxyMode::None, ProxyMode::Manual, ProxyMode::Auto];
const FIELD_WIDTH: f32 = 220.0;

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
        if self.loaded || self.jobs.loading {
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
        self.refresh(Rescan::No)
    }
}

impl Page {
    fn refresh(&mut self, rescan: Rescan) -> Task<Msg> {
        let Some(generation) = self.jobs.start_loading() else { return Task::none() };
        Task::perform(blocking(move || network::read(rescan)), move |state| {
            Msg::Loaded(generation, state.unwrap_or_default())
        })
    }

    #[must_use]
    pub const fn proxy(&self) -> &ProxySettings {
        &self.proxy
    }

    fn active_uuid(&self) -> Option<String> {
        let point = self.state.active()?;
        self.state
            .wifi_device
            .as_deref()
            .and_then(|device| self.state.active_profile(device))
            .map(|profile| profile.uuid.clone())
            .or_else(|| point.profile.clone())
    }

    fn load_details(&mut self) -> Task<Msg> {
        if self.folded || self.details_loading || self.state.active().is_none() {
            return Task::none();
        }
        let Some(device) = self.state.wifi_device.clone() else { return Task::none() };
        let uuid = self.active_uuid();
        self.details_loading = true;
        let generation = self.details_generation.current();
        Task::perform(blocking(move || network::read_details(&device, uuid.as_deref())), move |details| {
            Msg::DetailsLoaded(generation, details.unwrap_or_default())
        })
    }

    fn reset_details(&mut self) {
        self.details = None;
        self.details_loading = false;
        self.details_generation.bump();
        self.dns.clear();
        self.dns_invalid = false;
    }

    fn toggle_prompt(&mut self, prompt: Prompt) {
        self.prompt = (self.prompt.as_ref() != Some(&prompt)).then_some(prompt);
        self.password.clear();
    }

    fn settings_mut(&mut self) -> Option<&mut network::ProfileSettings> {
        self.details.as_mut().and_then(|d| d.settings.as_mut())
    }

    fn run(
        &mut self, busy: Option<String>, job: impl FnOnce() -> Result<String, String> + Send + 'static,
    ) -> Task<Msg> {
        self.jobs.begin(busy);
        Task::perform(blocking(job), Msg::Done)
    }

    fn save_setting(&mut self, job: impl FnOnce() -> Result<String, String> + Send + 'static) -> Task<Msg> {
        self.details_generation.bump();
        self.details_loading = false;
        self.run(None, job)
    }

    #[allow(clippy::too_many_lines)] // One arm per message keeps the panel's state machine in one place
    pub fn update(&mut self, msg: Msg) -> Task<Msg> {
        match msg {
            Msg::Loaded(generation, _) if !self.jobs.generation.is_current(generation) => {}
            Msg::Loaded(_, state) => {
                let before = self.state.active().map(|point| point.ssid.clone());
                self.networks_read_at = state.wifi_list_read.then(std::time::Instant::now);
                self.state = state;
                self.jobs.loading = false;
                self.loaded = true;
                if before.as_deref() != self.state.active().map(|point| point.ssid.as_str()) {
                    self.reset_details();
                }
                if let Some(Prompt::Password(ssid)) = &self.prompt
                    && self.find(ssid).is_none_or(|point| point.in_use)
                {
                    self.prompt = None;
                }
                return self.load_details();
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
            Msg::ToggleWifi(on) => {
                self.state.wifi_enabled = on;
                return self.run(None, move || network::set_wifi(on));
            }
            Msg::Airplane(on) => {
                self.state.airplane = on;
                return self.run(None, move || network::set_airplane(on));
            }
            Msg::ToggleDetails => {
                self.folded = !self.folded;
                return self.load_details();
            }
            Msg::Connect(ssid) => {
                let Some(point) = self.find(&ssid).cloned() else { return Task::none() };
                if point.secured && !point.known {
                    self.toggle_prompt(Prompt::Password(ssid));
                    return Task::none();
                }
                self.prompt = None;
                return self.run(Some(ssid), move || network::connect(&point.ssid, point.profile.as_deref(), None));
            }
            Msg::Setup => return self.run(None, network::open_connection_editor),
            Msg::Cancel => self.prompt = None,
            Msg::Password(password) => self.password = password,
            Msg::TogglePasswordShown => self.password_shown = !self.password_shown,
            Msg::SubmitPassword(ssid) => {
                let password = self.password.take();
                let profile = self.find(&ssid).and_then(|point| point.profile.clone());
                self.prompt = None;
                let name = ssid.clone();
                return self.run(Some(ssid), move || network::connect(&name, profile.as_deref(), Some(&password)));
            }
            Msg::Disconnect => {
                let Some(device) = self.state.wifi_device.clone() else { return Task::none() };
                return self.run(None, move || network::disconnect(&device));
            }
            Msg::Forget(uuid) => {
                self.reset_details();
                self.state.profiles.retain(|profile| profile.uuid != uuid);
                return self.run(None, move || network::forget(&uuid));
            }
            Msg::Autoconnect(uuid, on) => {
                if let Some(settings) = self.settings_mut() {
                    settings.autoconnect = on;
                }
                return self.save_setting(move || network::set_autoconnect(&uuid, on));
            }
            Msg::Metered(uuid, metered) => {
                if let Some(settings) = self.settings_mut() {
                    settings.metered = metered;
                }
                return self.save_setting(move || network::set_metered(&uuid, metered));
            }
            Msg::DnsInput(value) => {
                self.dns_invalid = network::parse_dns_list(&value).is_none();
                self.dns = value;
            }
            Msg::ApplyDns(uuid) => {
                let Some(servers) = network::parse_dns_list(&self.dns) else {
                    self.dns_invalid = true;
                    return Task::none();
                };
                if let Some(settings) = self.settings_mut() {
                    settings.custom_dns.clone_from(&servers);
                }
                let device = self.state.wifi_device.clone();
                return self.save_setting(move || network::set_dns(&uuid, device.as_deref(), &servers));
            }
            Msg::ToggleHidden => self.toggle_prompt(Prompt::Hidden),
            Msg::HiddenSsid(ssid) => self.hidden_ssid = ssid,
            Msg::HiddenSecure(secure) => self.hidden_secure = secure,
            Msg::HiddenPassword(password) => self.hidden_password = password,
            Msg::JoinHidden => {
                if !self.hidden_ready() {
                    return Task::none();
                }
                let ssid = self.hidden_ssid.trim().to_string();
                let password = self.hidden_secure.then(|| self.hidden_password.take());
                self.prompt = None;
                self.hidden_ssid.clear();
                return self.run(Some(ssid.clone()), move || network::connect_hidden(&ssid, password.as_deref()));
            }
            Msg::ToggleHotspot => self.toggle_prompt(Prompt::Hotspot),
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
                self.prompt = None;
                return self
                    .run(Some(network::HOTSPOT_NAME.to_string()), move || network::start_hotspot(&ssid, &password));
            }
            Msg::StopHotspot(uuid) => return self.run(None, move || network::stop_profile(&uuid)),
            Msg::ProxyLoaded(proxy) => self.proxy = proxy,
            Msg::ProxyMode(mode) => {
                let before = self.proxy.clone();
                self.proxy.mode = mode;
                let written = self.proxy.clone();
                self.jobs.error = None;
                return Task::perform(blocking(move || network::write_proxy(&written)), move |result| {
                    Msg::ProxyApplied(before, result)
                });
            }
            Msg::ProxyApplied(before, Some(Err(err))) => {
                self.proxy = before;
                self.jobs.error = Some(err);
            }
            Msg::ProxyApplied(..) => {}
            Msg::Done(result) => {
                self.jobs.finish(result);
                return self.refresh(Rescan::Auto);
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

    fn busy(&self, name: &str) -> bool {
        self.jobs.busy.as_deref() == Some(name)
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
        let error = self.jobs.error.as_deref().map(|error| match error {
            network::WRONG_PASSWORD => c.t(T::WrongPassword),
            network::AUTH_FAILED => c.t(T::AuthFailed),
            network::NO_ANSWER => c.t(T::NoAnswer),
            other => other,
        });
        column![self.hero(c), error_banner(t, error), self.radio_on().then(|| self.nearby(c)), self.other(c)]
            .spacing(theme::SECTION_GAP)
            .into()
    }

    const fn radio_on(&self) -> bool {
        self.state.wifi_enabled && !self.state.airplane
    }

    fn hero(&self, c: &Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        let state = &self.state;
        let (heading, hint) = if !state.wifi_hardware {
            (c.t(T::WifiOff).to_string(), c.t(T::HardwareOff).to_string())
        } else if state.airplane {
            (c.t(T::WifiOff).to_string(), c.t(T::AirplaneOn).to_string())
        } else if !state.wifi_enabled {
            (c.t(T::WifiOff).to_string(), c.t(T::TurnOnHint).to_string())
        } else if state.hotspot().is_some() {
            (format!("{} {}", c.t(T::HotspotOn), quoted(c, self.sharing_as())), c.t(T::HotspotHint).to_string())
        } else if let Some(active) = state.active() {
            let signal = format!("{} {} %", c.t(T::Signal), active.signal);
            let band = active.band().map(|band| band_label(c, band).to_string());
            let rate = rate_label(c, active);
            let internet = state.internet.then(|| c.t(T::Internet).to_string());
            let facts: Vec<String> = [Some(signal), band, rate, internet].into_iter().flatten().collect();
            (format!("{} {}", c.t(T::ConnectedTo), quoted(c, &active.ssid)), facts.join(" · "))
        } else {
            (c.t(T::NotConnected).to_string(), c.t(T::ChooseNetwork).to_string())
        };
        let toggle = row![sdk::text("Wi-Fi", sdk::ROW_TITLE), widgets::switch(t, state.wifi_enabled, Msg::ToggleWifi)]
            .spacing(18)
            .align_y(Alignment::Center);
        widgets::section_hero(t, Icon::Wifi, Category::Network.color(), heading, hint, Some(toggle.into()))
    }

    fn nearby(&self, c: &Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        let mut group = Group::new(t).title(c.t(T::Nearby));
        if let Some(read_at) = self.networks_read_at {
            let age = glide_sdk::i18n::ago(read_at.elapsed().as_secs(), c.lang);
            group = group.description(format!("{} {age}", c.t(T::Updated)));
        }
        if self.state.networks.is_empty() {
            group = group.push(widgets::list_empty(t, Icon::WifiOff, c.t(T::NoNetworks), ""));
        }
        for point in &self.state.networks {
            group = group.push(self.network_row(c, point));
            if point.in_use && !self.folded {
                group = group.push(self.details_band(c, point));
            }
            if matches!(&self.prompt, Some(Prompt::Password(ssid)) if *ssid == point.ssid) {
                group = group.push(self.password_form(c, point));
            }
        }
        group = group.push(
            Row::new(t, c.t(T::JoinHidden))
                .subtitle(c.t(T::JoinHiddenHint))
                .suffix(widgets::disclosure(t, None))
                .on_press(Msg::ToggleHidden)
                .build(),
        );
        if self.prompt == Some(Prompt::Hidden) {
            group = group.push(self.hidden_form(c));
        }
        group.build()
    }

    fn network_row<'a>(&self, c: &Ctx, point: &'a AccessPoint) -> Element<'a, Msg> {
        let t = c.theme;
        let busy = self.busy(&point.ssid);
        let line = Row::new(t, point.ssid.clone())
            .subtitle(if busy { c.t(T::Connecting).to_string() } else { describe(c, point) })
            .prefix(sdk::signal_bars(&t.paint, point.bars(), 4));
        if point.in_use {
            let suffix = row![widgets::pill(t, Tone::Ok, c.t(T::Connected)), sdk::fold_chevron(&t.paint, !self.folded)]
                .spacing(12)
                .align_y(Alignment::Center);
            return line.suffix(suffix).on_press(Msg::ToggleDetails).build();
        }
        if busy {
            return line.build();
        }
        let action = if point.enterprise() && !point.known {
            small(c, c.t(T::Setup), sdk::Tone::Neutral, Some(Msg::Setup))
        } else {
            small(c, c.t(T::Connect), sdk::Tone::Neutral, Some(Msg::Connect(point.ssid.clone())))
        };
        line.suffix(action).build()
    }

    fn details_band<'a>(&'a self, c: &Ctx, point: &AccessPoint) -> Element<'a, Msg> {
        let t = c.theme;
        let uuid = self.active_uuid();
        let mut rows = vec![match &self.details {
            Some(details) => sdk::kv_grid(&t.paint, facts(c, details, point), if t.compact { 2 } else { 3 }),
            None => container(widgets::spinner_label(t, c.t(K::Loading))).padding([10.0, theme::ROW_PADDING_X]).into(),
        }];
        if let (Some(uuid), Some(settings)) = (&uuid, self.details.as_ref().and_then(|d| d.settings.as_ref())) {
            let id = uuid.clone();
            rows.push(
                Row::new(t, c.t(T::AutoConnect))
                    .subtitle(c.t(T::AutoConnectHint))
                    .suffix(widgets::switch(t, settings.autoconnect, move |on| Msg::Autoconnect(id.clone(), on)))
                    .build(),
            );
            let id = uuid.clone();
            let labels = [c.t(T::MeteredAuto), c.t(T::Yes), c.t(T::No)];
            rows.push(
                Row::new(t, c.t(T::MeteredTitle))
                    .subtitle(c.t(T::MeteredHint))
                    .suffix(pick(c, &METERED, &labels, settings.metered, 200.0, move |m| Msg::Metered(id.clone(), m)))
                    .build(),
            );
            let apply = (!self.dns_invalid && self.dns_edited()).then(|| Msg::ApplyDns(uuid.clone()));
            rows.push(
                Row::new(t, c.t(T::CustomDns))
                    .subtitle(c.t(if self.dns_invalid { T::DnsInvalid } else { T::CustomDnsHint }))
                    .suffix(
                        widgets::field(t, "1.1.1.1, 9.9.9.9", &self.dns, Msg::DnsInput, apply, self.dns_invalid)
                            .width(FIELD_WIDTH),
                    )
                    .build(),
            );
        }
        let forget = uuid.map(|uuid| small(c, c.t(T::Forget), sdk::Tone::Danger, Some(Msg::Forget(uuid))));
        rows.push(sdk::split_actions(
            vec![small(c, c.t(T::Disconnect), sdk::Tone::Neutral, Some(Msg::Disconnect))],
            forget.into_iter().collect(),
        ));
        sdk::detail_band(&t.paint, rows)
    }

    fn password_form(&self, c: &Ctx, point: &AccessPoint) -> Element<'_, Msg> {
        let t = c.theme;
        let submit = (!self.password.is_empty()).then(|| Msg::SubmitPassword(point.ssid.clone()));
        let field = widgets::secret_field(
            t,
            c.t(T::Password),
            self.password.as_str(),
            |value| Msg::Password(Secret(value)),
            submit.clone(),
            false,
            (self.password_shown, Msg::TogglePasswordShown),
        );
        let password = Row::new(t, c.t(T::NetworkPassword))
            .subtitle(c.t(T::NetworkPasswordHint))
            .suffix(container(field).width(FIELD_WIDTH))
            .build();
        form(c, vec![password], confirm(c, c.t(T::Connect), submit))
    }

    fn hidden_form(&self, c: &Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        let long_ssid = ssid_too_long(&self.hidden_ssid);
        let join = self.hidden_ready().then_some(Msg::JoinHidden);
        let mut rows = vec![
            Row::new(t, c.t(T::Ssid))
                .subtitle(if long_ssid { c.t(T::SsidTooLong) } else { "" })
                .suffix(widgets::field(t, "", &self.hidden_ssid, Msg::HiddenSsid, None, long_ssid).width(FIELD_WIDTH))
                .build(),
            Row::new(t, c.t(T::Security))
                .suffix(widgets::segmented(
                    t,
                    vec![(false, c.t(T::SecurityNone).to_string()), (true, c.t(T::WpaPersonal).to_string())],
                    self.hidden_secure,
                    Msg::HiddenSecure,
                ))
                .build(),
        ];
        if self.hidden_secure {
            let field = widgets::secret_field(
                t,
                c.t(T::Password),
                self.hidden_password.as_str(),
                |value| Msg::HiddenPassword(Secret(value)),
                join.clone(),
                false,
                (self.password_shown, Msg::TogglePasswordShown),
            );
            rows.push(Row::new(t, c.t(T::Password)).suffix(container(field).width(FIELD_WIDTH)).build());
        }
        form(c, rows, confirm(c, c.t(T::Connect), join))
    }

    fn other(&self, c: &Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        let state = &self.state;
        let mut group = Group::new(t).title(c.t(T::Other));
        if !state.wired.is_empty() {
            let connected = state.wired.iter().find(|wired| wired.connected);
            let cable = state.wired.iter().any(|wired| wired.cable);
            let (subtitle, color, status) = match connected {
                Some(wired) => (wired.connection.clone(), t.ok(), c.t(T::Connected)),
                None if cable => (c.t(T::CablePlugged).to_string(), t.fg_4(), c.t(T::NotConnected)),
                None => (c.t(T::CableUnplugged).to_string(), t.fg_4(), c.t(T::NoCable)),
            };
            group = group.push(
                Row::new(t, c.t(T::Wired)).subtitle(subtitle).suffix(sdk::status_text(&t.paint, color, status)).build(),
            );
        }
        if state.wifi_device.is_some() {
            group = group.push(self.hotspot_row(c));
            if self.prompt == Some(Prompt::Hotspot) && self.radio_on() && state.hotspot().is_none() {
                group = group.push(self.hotspot_form(c));
            }
        }
        if state.rfkill {
            group = group.push(
                Row::new(t, c.t(T::Airplane))
                    .subtitle(c.t(T::AirplaneHint))
                    .suffix(widgets::switch(t, state.airplane, Msg::Airplane))
                    .build(),
            );
        }
        if self.proxy.available {
            let labels = [c.t(T::ProxyOff), c.t(T::Manual), c.t(T::Automatic)];
            group = group.push(
                Row::new(t, c.t(T::Proxy))
                    .subtitle(c.t(T::ProxyHint))
                    .suffix(pick(c, &PROXY_MODES, &labels, self.proxy.mode, 150.0, Msg::ProxyMode))
                    .build(),
            );
        }
        if group.is_empty() {
            return column![].into();
        }
        group.build()
    }

    fn hotspot_row(&self, c: &Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        let hotspot = self.state.hotspot();
        let subtitle = match (hotspot, self.state.active()) {
            (Some(_), _) => format!("{} {}", c.t(T::HotspotOn), quoted(c, self.sharing_as())),
            (None, Some(active)) => {
                format!("{}. {} {}", c.t(T::HotspotHint), c.t(T::HotspotLeaves), quoted(c, &active.ssid))
            }
            (None, None) => c.t(T::HotspotHint).to_string(),
        };
        let suffix = if self.busy(network::HOTSPOT_NAME) {
            widgets::spinner_label(t, c.t(T::Busy))
        } else if let Some(profile) = hotspot {
            small(c, c.t(T::Stop), sdk::Tone::Neutral, Some(Msg::StopHotspot(profile.uuid.clone())))
        } else {
            small(c, c.t(T::TurnOn), sdk::Tone::Neutral, self.radio_on().then_some(Msg::ToggleHotspot))
        };
        Row::new(t, c.t(T::Hotspot)).subtitle(subtitle).suffix(suffix).build()
    }

    fn hotspot_form(&self, c: &Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        let start = self.hotspot_ready().then_some(Msg::StartHotspot);
        let password = self.hotspot_password.as_str();
        let invalid_password = !password.is_empty() && !network::valid_psk(password);
        let long_ssid = ssid_too_long(&self.hotspot_ssid);
        let password_field = widgets::field(
            t,
            c.t(T::Password),
            password,
            |value| Msg::HotspotPassword(Secret(value)),
            start.clone(),
            invalid_password,
        )
        .width(FIELD_WIDTH);
        let rows = vec![
            Row::new(t, c.t(T::HotspotName))
                .subtitle(if long_ssid { c.t(T::SsidTooLong) } else { "" })
                .suffix(
                    widgets::field(t, "Glide", &self.hotspot_ssid, Msg::HotspotSsid, None, long_ssid)
                        .width(FIELD_WIDTH),
                )
                .build(),
            Row::new(t, c.t(T::Password))
                .subtitle(c.t(T::HotspotPasswordHint))
                .suffix(
                    row![password_field, small(c, c.t(T::Generate), sdk::Tone::Neutral, Some(Msg::GeneratePassword))]
                        .spacing(8)
                        .align_y(Alignment::Center),
                )
                .build(),
        ];
        form(c, rows, confirm(c, c.t(T::Start), start))
    }
}

fn small<'a>(c: &Ctx, label: &'a str, tone: sdk::Tone, message: Option<Msg>) -> Element<'a, Msg> {
    sdk::sized_button(&c.theme.paint, label, tone, sdk::Size::Small, message)
}

fn confirm<'a>(c: &Ctx, label: &'a str, message: Option<Msg>) -> Element<'a, Msg> {
    sdk::split_actions(
        Vec::new(),
        vec![
            small(c, c.t(K::Cancel), sdk::Tone::Neutral, Some(Msg::Cancel)),
            small(c, label, sdk::Tone::Accent, message),
        ],
    )
}

fn form<'a>(c: &Ctx, mut rows: Vec<Element<'a, Msg>>, actions: Element<'a, Msg>) -> Element<'a, Msg> {
    rows.push(actions);
    sdk::detail_band(&c.theme.paint, rows)
}

fn pick<'a, V: Copy + PartialEq + 'a>(
    c: &Ctx, values: &[V; 3], labels: &[&str; 3], selected: V, width: f32, on_select: impl Fn(V) -> Msg + 'a,
) -> Element<'a, Msg> {
    let values = *values;
    let options: Vec<Choice> =
        labels.iter().enumerate().map(|(index, label)| Choice::new(index as i64, label)).collect();
    let current = values.iter().position(|value| *value == selected).and_then(|index| options.get(index).cloned());
    let dropdown =
        widgets::dropdown(c.theme, options, current, move |choice| on_select(values[choice.value as usize]), width);
    container(dropdown).width(width).into()
}

fn quoted(c: &Ctx, name: &str) -> String {
    if c.lang == Lang::Uk { format!("«{name}»") } else { format!("“{name}”") }
}

pub(super) fn band_label(c: &Ctx, band: Band) -> &'static str {
    c.t(match band {
        Band::Ghz2 => T::Ghz2,
        Band::Ghz5 => T::Ghz5,
        Band::Ghz6 => T::Ghz6,
    })
}

pub(super) fn rate_label(c: &Ctx, point: &AccessPoint) -> Option<String> {
    (point.rate > 0).then(|| format!("{} {}", point.rate, c.t(T::Mbps)))
}

fn describe(c: &Ctx, point: &AccessPoint) -> String {
    let band = point.band().map(|band| band_label(c, band).to_string());
    let parts: Vec<String> = if point.in_use {
        let channel = band.map(|band| match point.channel {
            0 => band,
            channel => format!("{band}, {} {channel}", c.t(T::Channel)),
        });
        [Some(c.t(T::Connected).to_string()), point.protocol().map(str::to_string), channel]
            .into_iter()
            .flatten()
            .collect()
    } else if point.known {
        [Some(c.t(T::Saved).to_string()), point.protocol().map(str::to_string), band].into_iter().flatten().collect()
    } else if point.enterprise() {
        vec![c.t(T::Enterprise).to_string(), c.t(T::NeedsAccount).to_string()]
    } else if point.secured {
        [Some(c.t(T::Secured).to_string()), point.protocol().map(str::to_string)].into_iter().flatten().collect()
    } else {
        vec![c.t(T::OpenNetwork).to_string(), c.t(T::Unencrypted).to_string()]
    };
    parts.join(" · ")
}

fn facts(c: &Ctx, details: &Details, point: &AccessPoint) -> Vec<Fact> {
    let address =
        |list: &[String]| list.first().map(|address| address.split('/').next().unwrap_or(address).to_string());
    let dns = (!details.dns.is_empty()).then(|| details.dns.iter().take(2).cloned().collect::<Vec<_>>().join(", "));
    [
        (T::IpAddress, address(&details.ipv4)),
        (T::Gateway, Some(details.gateway.clone())),
        (T::Dns, dns),
        (T::Ipv6, address(&details.ipv6)),
        (T::MacAddress, Some(details.mac.clone())),
        (T::LinkSpeed, rate_label(c, point)),
    ]
    .into_iter()
    .filter_map(|(key, value)| value.filter(|value| !value.is_empty()).map(|value| (key, value)))
    .map(|(key, value)| {
        let fact = Fact::new(c.t(key), value);
        if key == T::MacAddress { fact.mono() } else { fact }
    })
    .collect()
}

fn saved_dns(details: Option<&Details>) -> String {
    details
        .and_then(|details| details.settings.as_ref())
        .map(|settings| settings.custom_dns.join(", "))
        .unwrap_or_default()
}

#[must_use]
pub(crate) fn ssid_too_long(ssid: &str) -> bool {
    ssid.trim().len() > network::MAX_SSID_BYTES
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn network_update_time_requires_a_successful_current_list_read() {
        let mut page = Page::default();
        let generation = page.jobs.generation.current();
        let read = NetworkState { wifi_list_read: true, ..NetworkState::default() };
        let _ = page.update(Msg::Loaded(generation.wrapping_add(1), read.clone()));
        assert!(page.networks_read_at.is_none());
        let _ = page.update(Msg::Loaded(generation, read));
        assert!(page.networks_read_at.is_some());
        let _ = page.update(Msg::Loaded(generation, NetworkState::default()));
        assert!(page.networks_read_at.is_none());
    }

    fn populated_page() -> Page {
        let point = AccessPoint {
            ssid: "Домашня".into(),
            signal: 78,
            secured: true,
            in_use: true,
            known: true,
            profile: Some("home".into()),
            security: "WPA2".into(),
            frequency: 5220,
            channel: 44,
            rate: 866,
        };
        let nearby = AccessPoint {
            ssid: "Кав’ярня".into(),
            signal: 45,
            secured: false,
            in_use: false,
            known: false,
            profile: None,
            security: String::new(),
            frequency: 0,
            channel: 0,
            rate: 0,
        };
        let saved =
            AccessPoint {
                ssid: "Домашня-2.4".into(), in_use: false, frequency: 2412, channel: 1, ..point.clone()
            };
        let protected = AccessPoint {
            ssid: "Kyivstar_5G_31A".into(),
            in_use: false,
            known: false,
            profile: None,
            security: "WPA3".into(),
            ..point
        };
        let enterprise = AccessPoint {
            ssid: "Office-Corp".into(),
            in_use: false,
            known: false,
            profile: None,
            security: "802.1X".into(),
            ..point
        };
        Page {
            loaded: true,
            networks_read_at: Some(std::time::Instant::now()),
            state: NetworkState {
                available: true,
                wifi_enabled: true,
                wifi_hardware: true,
                wifi_device: Some("wlan0".into()),
                wifi_list_read: true,
                networks: vec![point, saved, protected, nearby, enterprise],
                wired: vec![network::Wired {
                    device: "eth0".into(),
                    cable: false,
                    connected: false,
                    connection: String::new(),
                }],
                rfkill: true,
                internet: true,
                ..NetworkState::default()
            },
            details: Some(Details {
                device: "wlan0".into(),
                mac: "3C:9C:0F:00:00:1A".into(),
                ipv4: vec!["192.168.1.24".into()],
                gateway: "192.168.1.1".into(),
                dns: vec!["192.168.1.1".into()],
                ipv6: vec!["fd00::8a3c:1f2e".into()],
                settings: Some(network::ProfileSettings { autoconnect: true, ..network::ProfileSettings::default() }),
            }),
            proxy: ProxySettings { available: true, ..ProxySettings::default() },
            ..Page::default()
        }
    }

    #[test]
    #[ignore = "writes a populated network preview"]
    fn preview_populated_network() {
        use iced::advanced::renderer::{self, Headless};
        use iced::advanced::widget::Tree;
        use iced::advanced::{Layout, Shell, clipboard, layout};
        use iced::{Event, Font, Pixels, Point, Rectangle, Renderer, Size, mouse, window};

        glide_sdk::theme::motion::set_scale(0.0);
        glide_sdk::theme::use_ui_family(None);
        let appearance = if std::env::var("GLIDE_SETTINGS_APPEARANCE").as_deref() == Ok("dark") {
            crate::theme::Appearance::Dark
        } else {
            crate::theme::Appearance::Light
        };
        let c = Ctx {
            theme: crate::theme::ThemeState::new(appearance, crate::theme::Accent::Azurit),
            lang: Lang::Uk,
            writable: true,
            online: true,
        };
        let page = populated_page();
        let config = ConfigStore::parse("");
        let mut element = <Page as Pane>::view(&page, c, &config);
        let mut tree = Tree::new(&element);
        let mut renderer = Renderer::Secondary(iced_tiny_skia::Renderer::new(Font::DEFAULT, Pixels(14.0)));
        let size = Size::new(880.0, 1200.0);
        let node = element.as_widget_mut().layout(&mut tree, &renderer, &layout::Limits::new(size, size));
        let mut messages = Vec::new();
        let mut shell = Shell::new(&mut messages);
        element.as_widget_mut().update(
            &mut tree,
            &Event::Window(window::Event::RedrawRequested(std::time::Instant::now())),
            Layout::new(&node),
            mouse::Cursor::Unavailable,
            &renderer,
            &mut clipboard::Null,
            &mut shell,
            &Rectangle::with_size(size),
        );
        element.as_widget().draw(
            &tree,
            &mut renderer,
            &c.theme.paint.iced_theme(),
            &renderer::Style { text_color: c.theme.fg_1() },
            Layout::new(&node),
            mouse::Cursor::Unavailable,
            &Rectangle::with_size(size),
        );
        let cursor = mouse::Cursor::Available(Point::new(840.0, 44.0));
        for event in [
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        ] {
            let mut shell = Shell::new(&mut messages);
            element.as_widget_mut().update(
                &mut tree,
                &event,
                Layout::new(&node),
                cursor,
                &renderer,
                &mut clipboard::Null,
                &mut shell,
                &Rectangle::with_size(size),
            );
        }
        assert!(matches!(messages.as_slice(), [Msg::ToggleWifi(false)]));
        let Renderer::Secondary(renderer) = &mut renderer else { unreachable!() };
        let rgba = renderer.screenshot(Size::new(880, 1200), 1.0, c.theme.win_bg());
        let path = std::env::var_os("GLIDE_NETWORK_PREVIEW")
            .map_or_else(|| std::path::PathBuf::from("/tmp/opencode/network-populated.png"), std::path::PathBuf::from);
        image::save_buffer(path, &rgba, 880, 1200, image::ColorType::Rgba8).expect("preview is saved");
    }
}

use std::path::PathBuf;

use iced::widget::{column, container, row};
use iced::{Alignment, Element, Fill, Task};

use super::{Ctx, Pane};
use crate::i18n::Key as K;
use crate::icons::Icon;
use crate::services::config::ConfigStore;
use crate::services::run::{Generation, blocking};
use crate::services::vpn::{self, Candidate, Secret, Vpn, VpnDetails, VpnState};
use crate::theme::{self, Tone};
use crate::widgets::{self, Group, Row};
use crate::widgets::{actions, form_row, lines, note, sheet};

mod tr {
    crate::i18n::catalogue! {
        Title => "VPN", "VPN";
        NoneActive => "Not connected", "Не під'єднано";
        ActiveOne => "Connected through", "Під'єднано через";
        Unavailable => "VPN is unavailable", "VPN недоступний";
        UnavailableHint => "NetworkManager and nmcli are needed to manage VPN connections", "Для керування VPN потрібні NetworkManager і nmcli";
        Connections => "Connections", "З'єднання";
        ConnectionsHint => "Click a connection to see its details", "Натисніть на з'єднання, щоб побачити подробиці";
        Empty => "No VPN connections yet", "Ще немає VPN-з'єднань";
        EmptyHint => "Import a configuration file from your VPN provider below", "Імпортуйте файл конфігурації від вашого VPN-провайдера нижче";
        Activating => "Connecting…", "Під'єднання…";
        Server => "Server", "Сервер";
        UserName => "User name", "Ім'я користувача";
        Addresses => "Addresses", "Адреси";
        Dns => "DNS", "DNS";
        ListenPort => "Listen port", "Порт прослуховування";
        Protocol => "Protocol", "Протокол";
        Interface => "Interface", "Інтерфейс";
        Name => "Name", "Назва";
        Save => "Save", "Зберегти";
        AutoConnect => "Connect automatically", "Під'єднуватися автоматично";
        AutoConnectHint => "Bring the tunnel up whenever the network allows", "Піднімати тунель щоразу, коли дозволяє мережа";
        Delete => "Delete", "Видалити";
        DeleteConfirm => "Delete this connection and its saved secrets?", "Видалити це з'єднання та збережені секрети?";
        Import => "Import a configuration", "Імпорт конфігурації";
        ImportHint => "OpenVPN (.ovpn), WireGuard (.conf) or Cisco vpnc (.pcf) files", "Файли OpenVPN (.ovpn), WireGuard (.conf) або Cisco vpnc (.pcf)";
        Path => "File", "Файл";
        PathPlaceholder => "~/Downloads/office.ovpn", "~/Downloads/office.ovpn";
        Check => "Check", "Перевірити";
        ImportButton => "Import", "Імпортувати";
        Detected => "Detected", "Виявлено";
        Found => "Found on this computer", "Знайдено на цьому комп'ютері";
        MissingPlugin => "The NetworkManager plugin for this type is not installed", "Не встановлено плагін NetworkManager для цього типу";
        Plugins => "Installed VPN plugins", "Встановлені плагіни VPN";
        PluginsHint => "WireGuard is built into NetworkManager. Other types need a plugin such as network-manager-openvpn.", "WireGuard вбудовано в NetworkManager. Іншим типам потрібен плагін, наприклад network-manager-openvpn.";
        Imported => "Imported. The connection is listed above.", "Імпортовано. З'єднання з'явилося у списку вище.";
        SecretsHint => "If a connection needs a password that is not saved, you are asked for it here. It is used for that connection attempt only.", "Якщо з'єднанню потрібен незбережений пароль, його буде запитано тут. Він використовується лише для цієї спроби з'єднання.";
        PasswordNeeded => "This connection needs a password", "Для цього з'єднання потрібен пароль";
        PasswordRejected => "Could not connect with these credentials. Try again.", "Не вдалося під'єднатися з цими даними. Спробуйте ще раз.";
        Password => "Password", "Пароль";
        Otp => "One-time code", "Одноразовий код";
        OtpHint => "Leave empty if not required", "Залиште порожнім, якщо не потрібен";
        ConnectButton => "Connect", "Під'єднати";
        External => "Managed elsewhere", "Керується ззовні";
        ExternalHint => "Another program, such as NetBird, set this tunnel up, so it can only be changed there", "Цей тунель налаштувала інша програма, наприклад NetBird, тож змінити його можна лише там";
    }
}

use tr::Key as T;

#[derive(Debug, Clone)]
pub enum Msg {
    Loaded(u64, VpnState),
    DetailsLoaded(String, VpnDetails),
    Expand(String),
    Connect(String),
    Disconnect(String),
    Autoconnect(String, bool),
    NameInput(String),
    Rename(String),
    AskDelete(String),
    CancelDelete,
    Delete(String),
    PathInput(String),
    Inspect,
    Inspected(Option<Result<(PathBuf, String), String>>),
    Pick(PathBuf, String),
    Import,
    Imported(Option<Result<String, String>>),
    Done(Option<Result<String, String>>),
    ConnectDone(String, Option<Result<String, String>>),
    Rescan,
    Scanned(u64, Vec<Candidate>),
    SecretPassword(Secret),
    SecretOtp(Secret),
    SubmitSecrets,
    CancelSecrets,
}

#[derive(Debug, Default)]
struct Prompt {
    uuid: String,
    service: String,
    password: Secret,
    otp: Secret,
    rejected: bool,
}

#[derive(Debug, Default)]
pub struct Page {
    candidates: Vec<Candidate>,
    scan: Generation,
    secrets: Option<Prompt>,
    state: VpnState,
    loaded: bool,
    loading: bool,
    generation: Generation,
    pending: u32,
    busy: Option<String>,
    error: Option<String>,
    notice: Option<String>,
    expanded: Option<String>,
    details: Option<VpnDetails>,
    name: String,
    confirm_delete: Option<String>,
    path: String,
    selected: Option<(PathBuf, String)>,
    importing: bool,
}

impl Pane for Page {
    type Msg = Msg;

    const KEYWORDS: &'static [&'static str] =
        &["vpn", "wireguard", "openvpn", "tunnel", "тунель", "впн", "приватна мережа"];
    const POLLS: bool = true;

    fn open(&mut self, _c: Ctx, _config: &ConfigStore) -> Task<Msg> {
        let scan = self.scan();
        if self.loaded || self.loading {
            return scan;
        }
        Task::batch([self.refresh(), scan])
    }

    fn update(&mut self, msg: Msg, _c: Ctx, _config: &mut ConfigStore) -> Task<Msg> {
        Page::update(self, msg)
    }

    fn view<'a>(&'a self, c: Ctx, _config: &'a ConfigStore) -> Element<'a, Msg> {
        Page::view(self, c)
    }

    fn tick(&mut self, _c: Ctx) -> Task<Msg> {
        Task::batch([self.refresh(), self.load_details()])
    }
}

impl Page {
    pub fn refresh(&mut self) -> Task<Msg> {
        if self.loading || self.pending > 0 {
            return Task::none();
        }
        self.loading = true;
        let generation = self.generation.current();
        Task::perform(blocking(vpn::read), move |state| Msg::Loaded(generation, state.unwrap_or_default()))
    }

    fn scan(&mut self) -> Task<Msg> {
        let generation = self.scan.bump();
        Task::perform(blocking(vpn::find_candidates), move |found| Msg::Scanned(generation, found.unwrap_or_default()))
    }

    fn load_details(&self) -> Task<Msg> {
        let Some(uuid) = self.expanded.clone() else {
            return Task::none();
        };
        let key = uuid.clone();
        Task::perform(blocking(move || vpn::read_details(&uuid)), move |details| {
            Msg::DetailsLoaded(key, details.unwrap_or_default())
        })
    }

    #[must_use]
    pub const fn state(&self) -> &VpnState {
        &self.state
    }

    #[must_use]
    pub fn confirm_delete(&self) -> Option<&str> {
        self.confirm_delete.as_deref()
    }

    fn begin(&mut self, busy: Option<String>) {
        self.busy = busy;
        self.error = None;
        self.notice = None;
        self.generation.bump();
        self.loading = false;
        self.pending += 1;
    }

    fn run(
        &mut self, busy: Option<String>, job: impl FnOnce() -> Result<String, String> + Send + 'static,
    ) -> Task<Msg> {
        self.begin(busy);
        Task::perform(blocking(job), Msg::Done)
    }

    fn connect(&mut self, uuid: String, job: impl FnOnce() -> Result<String, String> + Send + 'static) -> Task<Msg> {
        self.begin(Some(uuid.clone()));
        Task::perform(blocking(job), move |result| Msg::ConnectDone(uuid, result))
    }

    fn connect_done(&mut self, uuid: String, result: Option<Result<String, String>>) -> Task<Msg> {
        let prompting = self.secrets.as_ref().is_some_and(|p| p.uuid == uuid);
        let wireguard = self.state.find(&uuid).is_some_and(Vpn::is_wireguard);
        let missing = matches!(&result, Some(Err(err)) if vpn::needs_secrets(err)) && !wireguard;
        match (&result, prompting) {
            (Some(Ok(_)), true) => self.secrets = None,
            (Some(Err(_)), true) => {
                if let Some(prompt) = self.secrets.as_mut() {
                    prompt.password = Secret::default();
                    prompt.otp = Secret::default();
                    prompt.rejected = true;
                }
            }
            _ if missing => {
                let service = self.state.find(&uuid).map(|vpn| vpn.service.clone()).unwrap_or_default();
                self.secrets = Some(Prompt { uuid, service, ..Prompt::default() });
            }
            _ => {}
        }
        self.update(Msg::Done(if missing { None } else { result }))
    }

    #[allow(clippy::too_many_lines)] // One match arm per message keeps the panel's state machine in one place
    pub fn update(&mut self, msg: Msg) -> Task<Msg> {
        match msg {
            Msg::Loaded(generation, _) if !self.generation.is_current(generation) => {}
            Msg::Loaded(_, state) => {
                self.state = state;
                self.loading = false;
                self.loaded = true;
                if let Some(uuid) = &self.expanded
                    && self.state.find(uuid).is_none()
                {
                    self.expanded = None;
                    self.details = None;
                }
                if let Some(prompt) = &self.secrets
                    && self.state.find(&prompt.uuid).is_none_or(|vpn| vpn.active)
                {
                    self.secrets = None;
                }
            }
            Msg::DetailsLoaded(uuid, details) => {
                if self.expanded.as_deref() == Some(uuid.as_str()) {
                    self.details = Some(details);
                }
            }
            Msg::Expand(uuid) => {
                self.confirm_delete = None;
                self.details = None;
                if self.expanded.as_deref() == Some(uuid.as_str()) {
                    self.expanded = None;
                } else {
                    self.name = self.state.find(&uuid).map(|vpn| vpn.name.clone()).unwrap_or_default();
                    self.expanded = Some(uuid);
                    return self.load_details();
                }
            }
            Msg::Connect(uuid) => {
                if self.secrets.as_ref().is_some_and(|p| p.uuid != uuid) {
                    self.secrets = None;
                }
                let target = uuid.clone();
                return self.connect(uuid, move || vpn::connect(&target));
            }
            Msg::ConnectDone(uuid, result) => return self.connect_done(uuid, result),
            Msg::SecretPassword(value) => {
                if let Some(prompt) = self.secrets.as_mut() {
                    prompt.password = value;
                }
            }
            Msg::SecretOtp(value) => {
                if let Some(prompt) = self.secrets.as_mut() {
                    prompt.otp = value;
                }
            }
            Msg::SubmitSecrets => {
                let Some(prompt) = self.secrets.as_mut() else {
                    return Task::none();
                };
                if prompt.password.is_empty() || self.busy.is_some() {
                    return Task::none();
                }
                let password = std::mem::take(&mut prompt.password);
                let otp = std::mem::take(&mut prompt.otp);
                let service = prompt.service.clone();
                let uuid = prompt.uuid.clone();
                let target = uuid.clone();
                return self.connect(uuid, move || vpn::connect_with_secrets(&target, &service, &password, &otp));
            }
            Msg::CancelSecrets => self.secrets = None,
            Msg::Rescan => return self.scan(),
            Msg::Scanned(generation, found) => {
                if self.scan.is_current(generation) {
                    self.candidates = found;
                }
            }
            Msg::Disconnect(uuid) => {
                if self.secrets.as_ref().is_some_and(|p| p.uuid == uuid) {
                    self.secrets = None;
                }
                let target = uuid.clone();
                return self.run(Some(uuid), move || vpn::disconnect(&target));
            }
            Msg::Autoconnect(uuid, on) => {
                if let Some(vpn) = self.state.connections.iter_mut().find(|vpn| vpn.uuid == uuid) {
                    vpn.autoconnect = on;
                }
                return self.run(None, move || vpn::set_autoconnect(&uuid, on));
            }
            Msg::NameInput(name) => self.name = name,
            Msg::Rename(uuid) => {
                let name = self.name.trim().to_string();
                if name.is_empty() {
                    return Task::none();
                }
                if let Some(vpn) = self.state.connections.iter_mut().find(|vpn| vpn.uuid == uuid) {
                    vpn.name.clone_from(&name);
                }
                return self.run(None, move || vpn::rename(&uuid, &name));
            }
            Msg::AskDelete(uuid) => self.confirm_delete = Some(uuid),
            Msg::CancelDelete => self.confirm_delete = None,
            Msg::Delete(uuid) => {
                self.confirm_delete = None;
                self.expanded = None;
                if self.secrets.as_ref().is_some_and(|p| p.uuid == uuid) {
                    self.secrets = None;
                }
                self.state.connections.retain(|vpn| vpn.uuid != uuid);
                return self.run(None, move || vpn::delete(&uuid));
            }
            Msg::PathInput(path) => {
                self.path = path;
                self.selected = None;
            }
            Msg::Inspect => {
                let path = self.path.clone();
                self.error = None;
                return Task::perform(blocking(move || vpn::inspect(&path)), Msg::Inspected);
            }
            Msg::Inspected(result) => match result {
                Some(Ok(selected)) => self.selected = Some(selected),
                Some(Err(err)) => self.error = Some(err),
                None => {}
            },
            Msg::Pick(path, kind) => {
                self.path = path.to_string_lossy().into_owned();
                self.selected = Some((path, kind));
            }
            Msg::Import => {
                let Some((path, kind)) = self.selected.clone() else {
                    return Task::none();
                };
                self.importing = true;
                self.error = None;
                self.notice = None;
                return Task::perform(blocking(move || vpn::import(&path, &kind)), Msg::Imported);
            }
            Msg::Imported(result) => {
                self.importing = false;
                match result {
                    Some(Ok(_)) => {
                        self.selected = None;
                        self.path.clear();
                        self.notice = Some(String::new());
                    }
                    Some(Err(err)) => self.error = Some(err),
                    None => {}
                }
                self.generation.bump();
                self.loading = false;
                return Task::batch([self.refresh(), self.scan()]);
            }
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
                return Task::batch([self.refresh(), self.load_details()]);
            }
        }
        Task::none()
    }

    #[must_use]
    pub fn view(&self, c: Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        if !self.state.available {
            return if self.loaded {
                widgets::empty_state(t, Icon::Shield, c.t(T::Unavailable), c.t(T::UnavailableHint))
            } else {
                widgets::empty_state(t, Icon::Shield, c.t(K::Loading), "")
            };
        }
        let active: Vec<&Vpn> = self.state.connections.iter().filter(|vpn| vpn.active).collect();
        let status = match active.as_slice() {
            [] => c.t(T::NoneActive).to_string(),
            list => format!(
                "{} {}",
                c.t(T::ActiveOne),
                list.iter().map(|vpn| vpn.name.as_str()).collect::<Vec<_>>().join(", ")
            ),
        };
        let pill: Element<'_, Msg> = if active.is_empty() {
            widgets::pill(t, Tone::Neutral, c.t(K::Disconnected))
        } else {
            widgets::status_pill(t, Tone::Ok, c.t(K::Connected))
        };
        let mut page = column![widgets::section_hero(
            t,
            Icon::Shield,
            super::Panel::Vpn.color(),
            c.t(T::Title),
            status,
            Some(pill),
        )]
        .spacing(theme::SECTION_GAP);
        if let Some(error) = &self.error {
            page = page.push(widgets::banner(t, crate::theme::Tone::Danger, Icon::Warning, error.as_str(), None));
        }
        if self.notice.is_some() {
            page = page.push(widgets::banner(t, crate::theme::Tone::Ok, Icon::Check, c.t(T::Imported), None));
        }
        let mut list = Group::new(t).title(c.t(T::Connections)).description(c.t(T::ConnectionsHint));
        if self.state.connections.is_empty() {
            list = list.push(widgets::list_empty(t, Icon::Shield, c.t(T::Empty), c.t(T::EmptyHint)));
        }
        for vpn in &self.state.connections {
            list = list.push(self.connection_row(&c, vpn));
            if let Some(prompt) = self.secrets.as_ref().filter(|p| p.uuid == vpn.uuid) {
                list = list.push(self.secrets_sheet(&c, prompt));
            }
            if self.expanded.as_deref() == Some(vpn.uuid.as_str()) {
                list = list.push(self.connection_sheet(&c, vpn));
            }
        }
        if self.state.connections.iter().any(|vpn| !vpn.is_wireguard() && !vpn.external) {
            list = list.push(note(t, Icon::Info, c.t(T::SecretsHint)));
        }
        page.push(list.build()).push(self.import(&c)).push(self.plugins(&c)).into()
    }

    fn connection_row<'a>(&'a self, c: &Ctx, vpn: &'a Vpn) -> Element<'a, Msg> {
        let t = c.theme;
        let busy = self.busy.as_deref() == Some(vpn.uuid.as_str());
        let idle = self.busy.is_none();
        let subtitle = if busy || vpn.activating() {
            c.t(T::Activating).to_string()
        } else if vpn.active {
            format!("{} · {}", vpn.protocol(), c.t(K::Connected))
        } else {
            vpn.protocol()
        };
        let uuid = vpn.uuid.clone();
        let toggle_uuid = uuid.clone();
        let open = self.expanded.as_deref() == Some(vpn.uuid.as_str());
        let control = if vpn.external {
            widgets::pill(t, Tone::Neutral, c.t(T::External))
        } else if idle {
            widgets::switch(t, vpn.active || vpn.activating(), move |on| {
                if on { Msg::Connect(toggle_uuid.clone()) } else { Msg::Disconnect(toggle_uuid.clone()) }
            })
        } else {
            widgets::switch_disabled(t, vpn.active)
        };
        Row::new(t, vpn.name.clone())
            .subtitle(subtitle)
            .prefix(if vpn.active {
                widgets::tile(Icon::Shield, super::Panel::Vpn.color(), 28.0)
            } else {
                container(widgets::icon(Icon::Shield, 18.0, t.fg_2())).center_x(28.0).center_y(28.0).into()
            })
            .suffix(
                row![control, widgets::icon(if open { Icon::ChevronUp } else { Icon::ChevronDown }, 14.0, t.fg_3()),]
                    .spacing(10)
                    .align_y(Alignment::Center),
            )
            .on_press(Msg::Expand(uuid))
            .build()
    }

    fn connection_sheet<'a>(&'a self, c: &Ctx, vpn: &'a Vpn) -> Element<'a, Msg> {
        let t = c.theme;
        let mut info = vec![(c.t(T::Protocol).to_string(), vpn.protocol())];
        if let Some(details) = &self.details {
            if !details.gateway.is_empty() {
                info.push((c.t(T::Server).to_string(), details.gateway.clone()));
            }
            if !details.user.is_empty() {
                info.push((c.t(T::UserName).to_string(), details.user.clone()));
            }
            if !details.addresses.is_empty() {
                info.push((c.t(T::Addresses).to_string(), details.addresses.join(", ")));
            }
            if !details.dns.is_empty() {
                info.push((c.t(T::Dns).to_string(), details.dns.join(", ")));
            }
            if !details.listen_port.is_empty() {
                info.push((c.t(T::ListenPort).to_string(), details.listen_port.clone()));
            }
        }
        if vpn.active && !vpn.device.is_empty() {
            info.push((c.t(T::Interface).to_string(), vpn.device.clone()));
        }
        let mut rows: Vec<Element<'a, Msg>> = vec![lines(t, info)];
        if vpn.external {
            rows.push(note(t, Icon::Info, c.t(T::ExternalHint)));
            return sheet(t, rows);
        }
        let valid = !self.name.trim().is_empty() && self.name.trim() != vpn.name;
        let rename = valid.then(|| Msg::Rename(vpn.uuid.clone()));
        rows.push(form_row(
            t,
            c.t(T::Name),
            None,
            row![
                widgets::field(t, c.t(T::Name), &self.name, Msg::NameInput, rename.clone(), false).width(Fill),
                widgets::secondary(t, c.t(T::Save), rename),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
            .into(),
        ));
        let uuid = vpn.uuid.clone();
        rows.push(
            Row::new(t, c.t(T::AutoConnect))
                .subtitle(c.t(T::AutoConnectHint))
                .suffix(widgets::switch(t, vpn.autoconnect, move |on| Msg::Autoconnect(uuid.clone(), on)))
                .build(),
        );
        if self.confirm_delete.as_deref() == Some(vpn.uuid.as_str()) {
            rows.push(note(t, Icon::Warning, c.t(T::DeleteConfirm)));
            rows.push(actions(vec![
                widgets::secondary(t, c.t(K::Cancel), Some(Msg::CancelDelete)),
                widgets::danger(t, c.t(T::Delete), Some(Msg::Delete(vpn.uuid.clone()))),
            ]));
        } else {
            rows.push(actions(vec![widgets::danger(t, c.t(T::Delete), Some(Msg::AskDelete(vpn.uuid.clone())))]));
        }
        sheet(t, rows)
    }

    fn secrets_sheet<'a>(&'a self, c: &Ctx, prompt: &'a Prompt) -> Element<'a, Msg> {
        let t = c.theme;
        let submit = (!prompt.password.is_empty() && self.busy.is_none()).then_some(Msg::SubmitSecrets);
        let mut rows: Vec<Element<'a, Msg>> =
            vec![note(t, Icon::Warning, c.t(if prompt.rejected { T::PasswordRejected } else { T::PasswordNeeded }))];
        rows.push(form_row(
            t,
            c.t(T::Password),
            None,
            widgets::field(
                t,
                c.t(T::Password),
                &prompt.password.0,
                |value| Msg::SecretPassword(Secret(value)),
                submit.clone(),
                prompt.rejected,
            )
            .secure(true)
            .width(Fill)
            .into(),
        ));
        if vpn::secret_keys(&prompt.service).1.is_some() {
            rows.push(form_row(
                t,
                c.t(T::Otp),
                Some(c.t(T::OtpHint)),
                widgets::field(
                    t,
                    c.t(T::Otp),
                    &prompt.otp.0,
                    |value| Msg::SecretOtp(Secret(value)),
                    submit.clone(),
                    false,
                )
                .secure(true)
                .width(Fill)
                .into(),
            ));
        }
        rows.push(actions(vec![
            widgets::secondary(t, c.t(K::Cancel), Some(Msg::CancelSecrets)),
            if self.busy.as_deref() == Some(prompt.uuid.as_str()) {
                widgets::spinner_label(t, c.t(T::Activating))
            } else {
                widgets::primary(t, c.t(T::ConnectButton), submit)
            },
        ]));
        sheet(t, rows)
    }

    fn import(&self, c: &Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        let mut group = Group::new(t)
            .title(c.t(T::Import))
            .description(c.t(T::ImportHint))
            .suffix(widgets::icon_button(t, Icon::Refresh, Some(Msg::Rescan)))
            .push(
                Row::new(t, c.t(T::Path))
                    .suffix(widgets::flexible(
                        row![
                            widgets::field(
                                t,
                                c.t(T::PathPlaceholder),
                                &self.path,
                                Msg::PathInput,
                                (!self.path.trim().is_empty()).then_some(Msg::Inspect),
                                false,
                            )
                            .width(Fill),
                            widgets::secondary(
                                t,
                                c.t(T::Check),
                                (!self.path.trim().is_empty()).then_some(Msg::Inspect)
                            ),
                        ]
                        .spacing(8)
                        .align_y(Alignment::Center),
                        420.0,
                    ))
                    .build(),
            );
        if let Some((path, kind)) = &self.selected {
            let supported = self.state.supports(kind);
            let file = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
            let mut line = Row::new(t, file)
                .subtitle(format!("{} {}", c.t(T::Detected), vpn::protocol_label(kind)))
                .prefix(widgets::icon(Icon::Download, 18.0, t.acc_text()));
            line = if supported {
                line.suffix(if self.importing {
                    widgets::spinner_label(t, c.t(K::Working))
                } else {
                    widgets::primary(t, c.t(T::ImportButton), Some(Msg::Import))
                })
            } else {
                line.suffix(widgets::pill(t, Tone::Warn, c.t(T::MissingPlugin)))
            };
            group = group.push(line.build());
        }
        let candidates: Vec<_> = self
            .candidates
            .iter()
            .filter(|candidate| self.selected.as_ref().is_none_or(|(path, _)| *path != candidate.path))
            .collect();
        if !candidates.is_empty() {
            group = group.push(container(widgets::micro_label(t, c.t(T::Found))).padding([8.0, theme::ROW_PADDING_X]));
            for candidate in candidates {
                let name =
                    candidate.path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
                let folder = candidate.path.parent().map(|dir| dir.to_string_lossy().into_owned()).unwrap_or_default();
                group = group.push(
                    Row::new(t, name)
                        .subtitle(format!("{} · {folder}", vpn::protocol_label(&candidate.kind)))
                        .prefix(widgets::icon(Icon::Shield, 18.0, t.fg_3()))
                        .suffix(widgets::disclosure(t, None))
                        .on_press(Msg::Pick(candidate.path.clone(), candidate.kind.clone()))
                        .build(),
                );
            }
        }
        group.build()
    }

    fn plugins(&self, c: &Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        let mut chips = row![widgets::pill(t, Tone::Accent, "WireGuard")].spacing(6);
        for plugin in &self.state.plugins {
            chips = chips.push(widgets::pill(t, Tone::Neutral, vpn::protocol_label(&plugin.name)));
        }
        Group::new(t)
            .title(c.t(T::Plugins))
            .description(c.t(T::PluginsHint))
            .push(container(chips.wrap().vertical_spacing(6)).padding([12.0, theme::ROW_PADDING_X]).width(Fill))
            .build()
    }
}

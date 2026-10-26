use glide_ipc::keychain::{ApiKey, Hidden, Login, Request, Response, SyncState, TwoFactor, TwoFactorMethod};
use glide_sdk::theme::{SP1, SP2, SP3, SP4, SP5, TextStyle};
use glide_sdk::widget::{self as ui, CodeSize, ListRow, Tone};
use glide_sdk::{Paint, Shell};
use iced::widget::{Column, Row, Space, column, container, row, text_input};
use iced::{Alignment, Element, Fill, Task};

use crate::app::{App, Message, ask};
use crate::i18n::Key;
use crate::icons::Icon;

pub const US: &str = "https://vault.bitwarden.com";
pub const EU: &str = "https://vault.bitwarden.eu";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Step {
    #[default]
    Kind,
    Login,
    TwoFactor,
    Sync,
}

impl Step {
    const fn number(self) -> usize {
        match self {
            Self::Kind => 1,
            Self::Login => 2,
            Self::TwoFactor => 3,
            Self::Sync => 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Kind {
    #[default]
    Bitwarden,
    Local,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Server {
    #[default]
    Us,
    Eu,
    Own,
}

#[derive(Debug, Clone, Default)]
pub struct Wizard {
    pub step: Step,
    pub kind: Kind,
    pub server: Server,
    pub url: String,
    pub email: String,
    pub password: String,
    pub shown: bool,
    pub code: String,
    pub method: Option<TwoFactorMethod>,
    pub methods: Vec<TwoFactorMethod>,
    pub remember: bool,
    pub name: String,
    pub busy: bool,
    pub error: Option<String>,
    pub vault: Option<String>,
    pub api: Option<ApiDraft>,
    pub prompt: Prompt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Prompt {
    #[default]
    TwoStep,
    Device,
}

#[derive(Debug, Clone, Default)]
pub struct ApiDraft {
    pub client_id: String,
    pub client_secret: String,
}

fn api_event(wizard: &mut Wizard, event: Event) {
    match event {
        Event::ApiKey(on) => {
            wizard.api = on.then(ApiDraft::default);
            wizard.error = None;
        }
        Event::ClientId(value) => {
            if let Some(api) = wizard.api.as_mut() {
                api.client_id = value;
            }
        }
        Event::ClientSecret(value) => {
            if let Some(api) = wizard.api.as_mut() {
                api.client_secret = value;
            }
        }
        _ => {}
    }
}

impl Wizard {
    #[must_use]
    pub fn server_url(&self) -> String {
        match self.server {
            Server::Us => US.to_string(),
            Server::Eu => EU.to_string(),
            Server::Own => self.url.trim().trim_end_matches('/').to_string(),
        }
    }

    #[must_use]
    pub fn login(&self) -> Login {
        Login {
            server: self.server_url(),
            email: self.email.trim().to_string(),
            password: Hidden::new(self.password.clone()),
            name: if self.name.trim().is_empty() { "Bitwarden".to_string() } else { self.name.trim().to_string() },
            two_factor: self
                .method
                .filter(|_| !self.code.is_empty() && self.prompt == Prompt::TwoStep)
                .map(|method| TwoFactor { method, code: self.code.clone(), remember: self.remember }),
            device_code: (self.prompt == Prompt::Device && !self.code.is_empty()).then(|| self.code.clone()),
            api_key: self.api.as_ref().map(|api| ApiKey {
                client_id: api.client_id.trim().to_string(),
                client_secret: Hidden::new(api.client_secret.trim().to_string()),
            }),
        }
    }

    #[must_use]
    pub fn can_sign_in(&self) -> bool {
        let address = self.server != Server::Own
            || self.url.trim().starts_with("https://")
            || self.url.trim().starts_with("http://localhost");
        let key = self
            .api
            .as_ref()
            .is_none_or(|api| api.client_id.trim().starts_with("user.") && !api.client_secret.trim().is_empty());
        address && key && self.email.contains('@') && !self.password.is_empty() && !self.busy
    }
}

#[derive(Debug, Clone)]
pub enum Event {
    Kind(Kind),
    Next,
    Back,
    Server(Server),
    Url(String),
    Email(String),
    Password(String),
    TogglePassword,
    SignIn,
    Reply(Result<Response, String>),
    EmailSent(Result<Response, String>),
    Code(String),
    Remember(bool),
    OtherMethod,
    Name(String),
    Finish,
    Close,
    ApiKey(bool),
    ClientId(String),
    ClientSecret(String),
}

fn reply(app: &mut App, result: Result<Response, String>) -> Task<Message> {
    let lang = app.lang;
    let Some(wizard) = app.wizard.as_mut() else {
        return Task::none();
    };
    wizard.busy = false;
    let retried = wizard.step == Step::TwoFactor && !wizard.code.is_empty();
    match result {
        Ok(Response::NeedsTwoFactor { .. }) if retried && wizard.prompt == Prompt::TwoStep => {
            wizard.error = Some(Key::WrongCode.text(lang).into());
            wizard.code.clear();
        }
        Ok(Response::NeedsTwoFactor { methods }) => {
            wizard.method = methods
                .iter()
                .copied()
                .find(|method| *method == TwoFactorMethod::Authenticator)
                .or_else(|| methods.first().copied());
            wizard.methods = methods;
            wizard.code.clear();
            wizard.remember |= wizard.step != Step::TwoFactor;
            wizard.prompt = Prompt::TwoStep;
            wizard.step = Step::TwoFactor;
        }
        Ok(Response::NeedsDeviceCode) => {
            if retried && wizard.prompt == Prompt::Device {
                wizard.error = Some(Key::WrongCode.text(lang).into());
            }
            wizard.prompt = Prompt::Device;
            wizard.method = None;
            wizard.methods.clear();
            wizard.code.clear();
            wizard.step = Step::TwoFactor;
        }
        Ok(Response::VaultAdded { vault }) => {
            wizard.vault = Some(vault);
            if wizard.name.trim().is_empty() {
                wizard.name = "Bitwarden".to_string();
            }
            wizard.step = Step::Sync;
            wizard.password.clear();
            return app.reload();
        }
        Ok(Response::WrongPassword) => {
            wizard.error = Some(
                lang.pick("Пошта або майстер-пароль не підходять", "The email or master password is wrong").into(),
            );
        }
        Ok(Response::Unsupported) => wizard.error = Some(Key::NotAvailable.text(lang).into()),
        Ok(Response::Error { message }) | Err(message) => wizard.error = Some(message),
        Ok(_) => {}
    }
    Task::none()
}

pub fn update(app: &mut App, event: Event, shell: &mut Shell<Message>) -> Task<Message> {
    let Some(wizard) = app.wizard.as_mut() else {
        return Task::none();
    };
    match event {
        Event::Kind(kind) => wizard.kind = kind,
        Event::Next => {
            if wizard.kind == Kind::Local {
                app.wizard = None;
                return Task::done(Message::CreateVault(CreateEvent::Open));
            }
            wizard.step = Step::Login;
        }
        Event::Back => {
            wizard.error = None;
            wizard.step = match wizard.step {
                Step::Login => Step::Kind,
                Step::TwoFactor => Step::Login,
                other => other,
            };
            wizard.prompt = Prompt::TwoStep;
            wizard.code.clear();
        }
        Event::Server(server) => wizard.server = server,
        Event::Url(value) => wizard.url = value,
        Event::Email(value) => wizard.email = value,
        Event::Password(value) => wizard.password = value,
        Event::TogglePassword => wizard.shown = !wizard.shown,
        event @ (Event::ApiKey(_) | Event::ClientId(_) | Event::ClientSecret(_)) => api_event(wizard, event),
        Event::SignIn => {
            wizard.busy = true;
            wizard.error = None;
            let login = wizard.login();
            return ask(Request::AddBitwarden { login: Box::new(login) }, |result| {
                Message::Wizard(Event::Reply(result))
            });
        }
        Event::Reply(result) => return reply(app, result),
        Event::Code(value) => {
            wizard.code = ui::only_digits(&value, 6);
            if wizard.code.len() == 6 && !wizard.busy {
                return Task::done(Message::Wizard(Event::SignIn));
            }
        }
        Event::Remember(on) => wizard.remember = on,
        Event::OtherMethod => {
            let next = wizard.methods.iter().copied().find(|method| Some(*method) != wizard.method);
            if let Some(method) = next {
                wizard.method = Some(method);
                wizard.code.clear();
                wizard.error = None;
                if method == TwoFactorMethod::Email {
                    let login = Box::new(wizard.login());
                    return ask(Request::SendLoginEmail { login }, |result| Message::Wizard(Event::EmailSent(result)));
                }
            }
        }
        Event::EmailSent(result) => match result {
            Ok(Response::Done) => shell.toast(Key::EmailSent.text(app.lang)),
            Ok(Response::Error { message }) | Err(message) => wizard.error = Some(message),
            Ok(_) => {}
        },
        Event::Name(value) => wizard.name = value,
        Event::Finish => {
            if let (Some(vault), false) = (wizard.vault.clone(), wizard.name.trim().is_empty()) {
                let name = wizard.name.trim().to_string();
                app.wizard = None;
                return ask(Request::RenameVault { vault, name }, Message::Done);
            }
            app.wizard = None;
        }
        Event::Close => app.wizard = None,
    }
    Task::none()
}

#[derive(Debug, Clone, Default)]
pub struct Create {
    pub password: String,
    pub repeat: String,
    pub mismatch: bool,
    pub busy: bool,
}

#[derive(Debug, Clone)]
pub enum CreateEvent {
    Open,
    Password(String),
    Repeat(String),
    Submit,
    Reply(Result<Response, String>),
    Close,
}

pub fn create(app: &mut App, event: CreateEvent, shell: &mut Shell<Message>) -> Task<Message> {
    if let CreateEvent::Open = event {
        app.create = Some(Create::default());
        return Task::none();
    }
    let Some(form) = app.create.as_mut() else {
        return Task::none();
    };
    match event {
        CreateEvent::Open => {}
        CreateEvent::Password(value) => {
            form.password = value;
            form.mismatch = false;
        }
        CreateEvent::Repeat(value) => {
            form.repeat = value;
            form.mismatch = false;
        }
        CreateEvent::Submit => {
            if form.password.is_empty() || form.password != form.repeat {
                form.mismatch = true;
                return Task::none();
            }
            form.busy = true;
            let password = Hidden::new(form.password.clone());
            return ask(Request::Create { password }, |result| Message::CreateVault(CreateEvent::Reply(result)));
        }
        CreateEvent::Reply(Ok(Response::Done)) => {
            app.create = None;
            return app.reload();
        }
        CreateEvent::Reply(result) => {
            form.busy = false;
            match result {
                Ok(Response::Error { message }) => shell.toast(message),
                Err(err) => shell.toast(err),
                _ => {}
            }
        }
        CreateEvent::Close => app.create = None,
    }
    Task::none()
}

fn dots<'a>(paint: &Paint, step: Step) -> Element<'a, Message> {
    let current = step.number();
    let dots = (1..=4).map(|index| {
        let color = if index <= current { paint.acc_bg() } else { paint.fill_3() };
        let width = if index == current { 22.0 } else { 6.0 };
        container(Space::new().width(width).height(6))
            .style(move |_| glide_sdk::glide_iced::style::rounded(color, 4.0))
            .into()
    });
    Row::with_children(dots).spacing(6).into()
}

fn choice_card<'a>(
    paint: &Paint, glyph: Icon, title: &'a str, body: &'a str, selected: bool, message: Option<Message>,
) -> Element<'a, Message> {
    ui::option_card(paint, ui::glyph_tile(paint, glyph, 40.0, selected), title, body, selected, message)
}

pub fn labelled<'a>(paint: &Paint, label: &'a str, control: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    column![ui::text(label, TextStyle { weight: 520, ..TextStyle::CAP }).color(paint.fg_2()), control.into()]
        .spacing(6)
        .into()
}

#[must_use]
pub fn view(app: &App) -> Option<Element<'_, Message>> {
    if let Some(form) = &app.create {
        return Some(create_view(app, form));
    }
    let wizard = app.wizard.as_ref()?;
    let paint = app.paint;
    let t = |key: Key| key.text(app.lang);
    let head = row![
        ui::text(t(Key::StepOf).replace("{n}", &wizard.step.number().to_string()), TextStyle::CAP)
            .color(paint.fg_3())
            .width(Fill),
        dots(&paint, wizard.step),
    ]
    .align_y(Alignment::Center);
    let body = match wizard.step {
        Step::Kind => kind_step(app, wizard),
        Step::Login => login_step(app, wizard),
        Step::TwoFactor => two_factor_step(app, wizard),
        Step::Sync => sync_step(app, wizard),
    };
    let content = column![head, body].spacing(SP5);
    Some(crate::view::dialog_frame(app, 560.0, content, Message::Wizard(Event::Close)))
}

fn title<'a>(app: &App, key: Key, body: String) -> Element<'a, Message> {
    column![ui::text(key.text(app.lang), TextStyle::T2), ui::text(body, TextStyle::CALL).color(app.paint.fg_2())]
        .spacing(SP1)
        .into()
}

fn kind_step<'a>(app: &'a App, wizard: &'a Wizard) -> Column<'a, Message> {
    let paint = app.paint;
    let t = |key: Key| key.text(app.lang);
    let mut content = Column::new().spacing(SP5);
    {
        content = content.push(title(app, Key::AddVault, t(Key::AddVaultBody).to_string()));
        content = content.push(
            row![
                choice_card(
                    &paint,
                    Icon::Cloud,
                    t(Key::Bitwarden),
                    t(Key::BitwardenBody),
                    wizard.kind == Kind::Bitwarden,
                    Some(Message::Wizard(Event::Kind(Kind::Bitwarden)))
                ),
                choice_card(
                    &paint,
                    Icon::Drive,
                    t(Key::OnDeviceTitle),
                    t(Key::OnDeviceBody),
                    wizard.kind == Kind::Local,
                    Some(Message::Wizard(Event::Kind(Kind::Local)))
                ),
                choice_card(&paint, Icon::Note, t(Key::KeePass), t(Key::KeePassBody), false, None),
            ]
            .spacing(SP3),
        );
        content = content.push(
            row![
                Space::new().width(Fill),
                ui::secondary(&paint, t(Key::Cancel), Message::Wizard(Event::Close)),
                ui::primary(&paint, t(Key::Next), Message::Wizard(Event::Next)),
            ]
            .spacing(SP2),
        );
    }
    content
}

fn api_key_fields<'a>(app: &'a App, api: &'a ApiDraft) -> Element<'a, Message> {
    let paint = app.paint;
    let t = |key: Key| key.text(app.lang);
    let id = labelled(
        &paint,
        "client_id",
        ui::field(
            &paint,
            text_input("user.xxxxxxxx-xxxx-…", &api.client_id)
                .on_input(|value| Message::Wizard(Event::ClientId(value)))
                .font(glide_sdk::glide_iced::font::TABULAR)
                .size(13),
        ),
    );
    let secret = labelled(
        &paint,
        "client_secret",
        crate::view::secret_input(&paint, "client_secret", &api.client_secret, |value| {
            Message::Wizard(Event::ClientSecret(value))
        }),
    );
    column![row![id, secret].spacing(SP3), ui::text(t(Key::ApiKeyHint), TextStyle::CAP).color(paint.fg_3()),]
        .spacing(6)
        .into()
}

fn login_step<'a>(app: &'a App, wizard: &'a Wizard) -> Column<'a, Message> {
    let paint = app.paint;
    let t = |key: Key| key.text(app.lang);
    let mut content = Column::new().spacing(SP5);
    {
        content = content.push(title(app, Key::SignInBitwarden, t(Key::MasterNote).to_string()));
        let mut server = Column::new()
            .spacing(6)
            .push(ui::text(t(Key::Server), TextStyle { weight: 520, ..TextStyle::CAP }).color(paint.fg_2()));
        server = server.push(ui::segmented_fill(
            &paint,
            &[(Server::Us, "bitwarden.com"), (Server::Eu, "bitwarden.eu"), (Server::Own, t(Key::OwnServer))],
            wizard.server,
            |server| Message::Wizard(Event::Server(server)),
        ));
        if wizard.server == Server::Own {
            server = server.push(ui::field_with(
                &paint,
                Some(ui::icon(Icon::Server, 16.0, paint.fg_3())),
                text_input("https://vault.example.com", &wizard.url)
                    .on_input(|value| Message::Wizard(Event::Url(value)))
                    .font(glide_sdk::glide_iced::font::TABULAR)
                    .size(13),
                Vec::new(),
            ));
            server = server.push(ui::text(t(Key::ServerHint), TextStyle::CAP).color(paint.fg_3()));
        }
        content = content.push(server);
        content = content.push(labelled(
            &paint,
            t(Key::Email),
            ui::field(
                &paint,
                text_input("name@example.com", &wizard.email).on_input(|value| Message::Wizard(Event::Email(value))),
            ),
        ));
        content = content.push(labelled(
            &paint,
            t(Key::MasterPassword),
            ui::password_field(
                &paint,
                t(Key::MasterPassword),
                &wizard.password,
                wizard.shown,
                |value| Message::Wizard(Event::Password(value)),
                Message::Wizard(Event::TogglePassword),
            ),
        ));
        if let Some(api) = wizard.api.as_ref() {
            content = content.push(api_key_fields(app, api));
        }
        content = content.push(ui::link(
            &paint,
            t(if wizard.api.is_some() { Key::WithoutApiKey } else { Key::WithApiKey }),
            Some(Message::Wizard(Event::ApiKey(wizard.api.is_none()))),
        ));
        if let Some(error) = &wizard.error {
            content = content.push(ui::text(error.as_str(), TextStyle::CAP).color(paint.danger()));
        }
        let action: Element<'_, Message> = if wizard.busy {
            ui::busy_label(&paint, t(Key::SignIn))
        } else {
            ui::button(
                &paint,
                t(Key::SignIn),
                Tone::Accent,
                wizard.can_sign_in().then_some(Message::Wizard(Event::SignIn)),
            )
        };
        content = content.push(
            row![ui::ghost(&paint, t(Key::Back), Message::Wizard(Event::Back)), Space::new().width(Fill), action]
                .spacing(SP2)
                .align_y(Alignment::Center),
        );
    }
    content
}

fn two_factor_step<'a>(app: &'a App, wizard: &'a Wizard) -> Column<'a, Message> {
    let paint = app.paint;
    let t = |key: Key| key.text(app.lang);
    let mut content = Column::new().spacing(SP5);
    {
        let (heading, body) = if wizard.prompt == Prompt::Device {
            (Key::ConfirmDevice, Key::DeviceCodeBody)
        } else if wizard.method == Some(TwoFactorMethod::Email) {
            (Key::ConfirmSignIn, Key::EmailCodeBody)
        } else {
            (Key::ConfirmSignIn, Key::ConfirmBody)
        };
        content = content.push(title(app, heading, t(body).replace("{email}", wizard.email.trim())));
        content = content.push(
            container(
                ui::code_input(&paint, &wizard.code, 6, |value| Message::Wizard(Event::Code(value)))
                    .size(CodeSize::Large)
                    .invalid(wizard.error.is_some())
                    .on_submit(Message::Wizard(Event::SignIn)),
            )
            .center_x(Fill),
        );
        if wizard.prompt == Prompt::TwoStep {
            content = content.push(
                row![
                    ui::text(t(Key::Remember), TextStyle::CALL).width(Fill),
                    ui::switch(&paint, wizard.remember, Some(|on| Message::Wizard(Event::Remember(on)))),
                ]
                .align_y(Alignment::Center),
            );
        }
        if wizard.methods.len() > 1 {
            content = content.push(ui::link(&paint, t(Key::OtherMethod), Some(Message::Wizard(Event::OtherMethod))));
        }
        if let Some(error) = &wizard.error {
            content = content.push(ui::text(error.as_str(), TextStyle::CAP).color(paint.danger()));
        }
        content = content.push(
            row![
                ui::ghost(&paint, t(Key::Back), Message::Wizard(Event::Back)),
                Space::new().width(Fill),
                ui::button(
                    &paint,
                    t(Key::Confirm),
                    Tone::Accent,
                    (wizard.code.len() == 6 && !wizard.busy).then_some(Message::Wizard(Event::SignIn))
                ),
            ]
            .spacing(SP2),
        );
    }
    content
}

fn sync_step<'a>(app: &'a App, wizard: &'a Wizard) -> Column<'a, Message> {
    let paint = app.paint;
    let t = |key: Key| key.text(app.lang);
    let mut content = Column::new().spacing(SP5);
    {
        let vault = wizard.vault.as_ref().and_then(|id| app.data.vaults.iter().find(|vault| &vault.id == id));
        let done = vault.is_some_and(|vault| matches!(vault.sync, SyncState::Synced { .. }));
        let count = vault.map_or(0, |vault| vault.count);
        content = content.push(title(app, Key::Loading, t(Key::LoadingBody).to_string()));
        content = content.push(ui::progress(&paint, if done { Some(1.0) } else { None }));
        let stages = [vault.is_some(), done, done, done];
        let first_open = stages.iter().position(|ok| !ok);
        let lead = |index: usize| -> Element<'_, Message> {
            if stages[index] {
                ui::icon(Icon::Check, 16.0, paint.ok())
            } else if first_open == Some(index) {
                ui::sized_spinner(&paint, 16.0)
            } else {
                container(ui::dot(paint.fg_4(), 8.0)).center(16).into()
            }
        };
        let waiting = |index: usize| !stages[index] && first_open != Some(index);
        content = content.push(ui::list(
            &paint,
            vec![
                ListRow::new(paint, t(Key::KeysDone)).prefix(lead(0)).into(),
                ListRow::new(paint, t(Key::ItemsLoaded))
                    .prefix(lead(1))
                    .dimmed(waiting(1))
                    .suffix(ui::text(count.to_string(), TextStyle::NUM).color(paint.fg_2()))
                    .into(),
                ListRow::new(paint, t(Key::FoldersLoaded)).prefix(lead(2)).dimmed(waiting(2)).into(),
                ListRow::new(paint, t(Key::SendsLoaded)).prefix(lead(3)).dimmed(waiting(3)).into(),
            ],
        ));
        content = content.push(labelled(
            &paint,
            t(Key::NameInApp),
            ui::field(
                &paint,
                text_input("Bitwarden", &wizard.name).on_input(|value| Message::Wizard(Event::Name(value))),
            ),
        ));
        content = content.push(row![
            Space::new().width(Fill),
            ui::button(&paint, t(Key::Done), Tone::Accent, done.then_some(Message::Wizard(Event::Finish)))
        ]);
    }
    content
}

fn create_view<'a>(app: &'a App, form: &'a Create) -> Element<'a, Message> {
    let paint = app.paint;
    let t = |key: Key| key.text(app.lang);
    let mut content = Column::new()
        .spacing(SP4)
        .push(
            column![
                ui::text(t(Key::CreateVault), TextStyle::T2),
                ui::text(t(Key::CreateVaultBody), TextStyle::CALL).color(paint.fg_2())
            ]
            .spacing(SP1),
        )
        .push(labelled(
            &paint,
            t(Key::VaultPassword),
            crate::view::secret_input(&paint, t(Key::VaultPassword), &form.password, |value| {
                Message::CreateVault(CreateEvent::Password(value))
            }),
        ))
        .push(labelled(
            &paint,
            t(Key::RepeatPassword),
            crate::view::secret_input(&paint, t(Key::RepeatPassword), &form.repeat, |value| {
                Message::CreateVault(CreateEvent::Repeat(value))
            }),
        ));
    if !form.password.is_empty() {
        let level = glide_ipc::keychain::strength(&form.password);
        if let Some(strength) = crate::view::strength(level) {
            content = content.push(ui::strength_line(
                &paint,
                strength,
                crate::view::strength_text(app, strength, &form.password),
            ));
        }
    }
    if form.mismatch {
        content = content.push(ui::text(t(Key::Mismatch), TextStyle::CAP).color(paint.danger()));
    }
    content = content.push(
        row![
            Space::new().width(Fill),
            ui::secondary(&paint, t(Key::Cancel), Message::CreateVault(CreateEvent::Close)),
            ui::button(
                &paint,
                t(Key::CreateVault),
                Tone::Accent,
                (!form.busy).then_some(Message::CreateVault(CreateEvent::Submit))
            ),
        ]
        .spacing(SP2),
    );
    crate::view::dialog_frame(app, 480.0, content, Message::CreateVault(CreateEvent::Close))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filled() -> Wizard {
        Wizard {
            email: " artem@example.com ".into(),
            password: "pw".into(),
            method: Some(TwoFactorMethod::Authenticator),
            code: "123456".into(),
            ..Wizard::default()
        }
    }

    #[test]
    fn a_two_step_code_goes_with_its_method() {
        let login = filled().login();
        assert_eq!(login.email, "artem@example.com");
        assert_eq!(
            login.two_factor.map(|two| (two.method, two.code)),
            Some((TwoFactorMethod::Authenticator, "123456".into()))
        );
        assert_eq!(login.device_code, None);
    }

    #[test]
    fn a_device_code_is_sent_as_one_and_not_as_a_two_step_code() {
        let login = Wizard { prompt: Prompt::Device, ..filled() }.login();
        assert_eq!(login.device_code.as_deref(), Some("123456"));
        assert!(login.two_factor.is_none());
    }

    #[test]
    fn an_own_server_needs_https_unless_it_is_this_machine() {
        let mut wizard =
            Wizard { server: Server::Own, url: "http://vault.lan".into(), code: String::new(), ..filled() };
        assert!(!wizard.can_sign_in());
        wizard.url = "https://vault.lan/".into();
        assert!(wizard.can_sign_in());
        assert_eq!(wizard.server_url(), "https://vault.lan");
    }
}

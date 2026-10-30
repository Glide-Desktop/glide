use glide_ipc::keychain::{Hidden, Request, Response, Send, SendDraft, Vault, VaultKind};
use glide_sdk::theme::{SP1, SP2, SP3, SP4, SP5, SP6, TextStyle};
use glide_sdk::widget::{self as ui, ListRow, Severity, Size, Tone};
use glide_sdk::{Lang, Shell};
use iced::widget::{Column, Space, column, container, row, text_editor, text_input};
use iced::{Alignment, Element, Fill, Padding, Task};

use crate::app::{App, Copied, Message, ask, unix_now};
use crate::i18n::Key;
use crate::icons::Icon;
use crate::view::{Labeled, labeled};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lifetime {
    Hour,
    Day,
    Week,
    Month,
    Custom(u32),
}

pub const CUSTOM_DAYS: (u32, u32) = (1, 31);

impl Lifetime {
    #[must_use]
    pub const fn hours(self) -> u32 {
        match self {
            Self::Hour => 1,
            Self::Day => 24,
            Self::Week => 24 * 7,
            Self::Month => 24 * 30,
            Self::Custom(days) => 24 * days,
        }
    }
}

#[allow(clippy::struct_excessive_bools)] // The form's switches, one field each
pub struct SendForm {
    pub open: bool,
    pub name: String,
    pub text: text_editor::Content,
    pub lifetime: Lifetime,
    pub opens: u32,
    pub protect: bool,
    pub password: String,
    pub shown: bool,
    pub hide_text: bool,
    pub hide_email: bool,
    pub vault: String,
    pub created: Option<Send>,
    pub selected: Option<(String, String)>,
    pub query: String,
    pub busy: bool,
    pub copied_at: Option<u64>,
    pub editing: Option<String>,
    pub had_password: bool,
    pub unlimited: bool,
    pub kept_hours: Option<u32>,
}

const COPIED_HOLD: u64 = 2;

impl Default for SendForm {
    fn default() -> Self {
        Self {
            open: true,
            name: String::new(),
            text: text_editor::Content::new(),
            lifetime: Lifetime::Day,
            opens: 5,
            protect: false,
            password: String::new(),
            shown: false,
            hide_text: true,
            hide_email: true,
            vault: String::new(),
            created: None,
            selected: None,
            query: String::new(),
            busy: false,
            copied_at: None,
            editing: None,
            had_password: false,
            unlimited: false,
            kept_hours: None,
        }
    }
}

fn unlocked_servers(vaults: &[Vault]) -> Vec<&Vault> {
    vaults.iter().filter(|vault| vault.kind == VaultKind::Bitwarden && vault.unlocked).collect()
}

fn chosen_server<'a>(servers: &[&'a Vault], wanted: &str) -> Option<&'a Vault> {
    servers.iter().copied().find(|server| server.id == wanted).or_else(|| servers.first().copied())
}

impl SendForm {
    fn target(&self, vaults: &[Vault]) -> String {
        if self.editing.is_some() && !self.vault.is_empty() {
            return self.vault.clone();
        }
        chosen_server(&unlocked_servers(vaults), &self.vault).map(|server| server.id.clone()).unwrap_or_default()
    }

    fn request(&self, vaults: &[Vault]) -> Request {
        let draft = Box::new(SendDraft {
            vault: self.target(vaults),
            name: self.name.trim().to_string(),
            text: Hidden::new(self.text.text().trim_end().to_string()),
            lifetime_hours: self.kept_hours.filter(|_| self.editing.is_some()).unwrap_or_else(|| self.lifetime.hours()),
            max_access: (!(self.unlimited && self.editing.is_some())).then_some(self.opens),
            password: (self.protect && !self.password.is_empty()).then(|| Hidden::new(self.password.clone())),
            hide_text: self.hide_text,
            hide_email: self.hide_email,
        });
        match self.editing.clone() {
            Some(id) => Request::EditSend { id, keep_password: self.protect && self.password.is_empty(), draft },
            None => Request::CreateSend { draft },
        }
    }

    fn edit(&mut self, send: Send, text: &str, now: u64) {
        self.name.clone_from(&send.name);
        self.text = text_editor::Content::with_text(text);
        self.lifetime = lifetime_left(&send, now);
        let left = send.expires.unwrap_or(send.deletion).saturating_sub(now).div_ceil(3600).max(1);
        self.kept_hours = Some(u32::try_from(left).unwrap_or(u32::MAX));
        self.opens = send.max_access.unwrap_or(5).clamp(1, 99);
        self.unlimited = send.max_access.is_none();
        self.protect = send.has_password;
        self.had_password = send.has_password;
        self.password.clear();
        self.shown = false;
        self.hide_text = send.hide_text;
        self.hide_email = send.hide_email;
        self.vault = send.vault;
        self.editing = Some(send.id);
        self.open = true;
    }
}

#[derive(Debug, Clone)]
pub enum Event {
    New,
    Name(String),
    Text(text_editor::Action),
    Lifetime(Lifetime),
    Opens(u32),
    Protect(bool),
    Password(String),
    Reveal,
    HideText(bool),
    HideEmail(bool),
    Vault(String),
    Create,
    Created(Result<Response, String>),
    Select(String, String),
    CopyLink,
    Mail,
    Disable,
    Edit,
    EditLoaded(String, String, Result<Response, String>),
    StopEditing,
    Delete,
    Done,
    Query(String),
    ClearQuery,
}

fn status(send: &Send, now: u64) -> (Severity, Key) {
    if send.disabled {
        (Severity::Warn, Key::DisabledBadge)
    } else if send.max_access.is_some_and(|max| send.access_count >= max) {
        (Severity::Neutral, Key::Exhausted)
    } else if send.expires.is_some_and(|expires| expires <= now) || send.deletion <= now {
        (Severity::Neutral, Key::Expired)
    } else {
        (Severity::Ok, Key::ActiveBadge)
    }
}

#[must_use]
pub fn lifetime_left(send: &Send, now: u64) -> Lifetime {
    let hours = send.expires.unwrap_or(send.deletion).saturating_sub(now).div_ceil(3600);
    match hours {
        0..=1 => Lifetime::Hour,
        2..=24 => Lifetime::Day,
        _ => match u32::try_from(hours.div_ceil(24)).unwrap_or(CUSTOM_DAYS.1) {
            7 => Lifetime::Week,
            30 => Lifetime::Month,
            days => Lifetime::Custom(days.clamp(CUSTOM_DAYS.0, CUSTOM_DAYS.1)),
        },
    }
}

fn active(send: &Send, now: u64) -> bool {
    status(send, now).1 == Key::ActiveBadge
}

fn describe(lang: Lang, send: &Send, now: u64) -> String {
    let t = |key: Key| key.text(lang);
    let mut parts = Vec::new();
    if send.has_password {
        parts.push(t(Key::WithPassword).to_string());
    }
    let ends = send.expires.unwrap_or(send.deletion);
    if ends > now {
        let hours = (ends - now).div_ceil(3600);
        parts.push(if hours <= 48 {
            t(Key::ExpiresHours).replace("{n}", &hours.to_string())
        } else {
            t(Key::ExpiresDays).replace("{n}", &hours.div_ceil(24).to_string())
        });
    } else if now - ends < 86_400 {
        parts.push(t(Key::ExpiredHoursAgo).replace("{n}", &((now - ends) / 3600).max(1).to_string()));
    } else {
        parts.push(t(Key::ExpiredAgo).replace("{n}", &((now - ends) / 86_400).to_string()));
    }
    parts.push(match send.max_access {
        Some(max) => t(Key::OpenedOf).replace("{a}", &send.access_count.to_string()).replace("{b}", &max.to_string()),
        None => t(Key::OpenedTimes).replace("{a}", &send.access_count.to_string()),
    });
    let mut text = parts.join(" · ");
    if let Some(first) = text.get(..1) {
        text = first.to_uppercase() + &text[1..];
    }
    text
}

fn failure_notice(app: &App, result: &Result<Response, String>) -> Option<String> {
    match result {
        Ok(Response::Unsupported) => Some(app.t(Key::NeedsServer).to_string()),
        other => app.reply_notice(other),
    }
}

pub fn update(app: &mut App, event: Event, shell: &mut Shell<Message>) -> Task<Message> {
    let form = &mut app.send;
    match event {
        Event::New | Event::Done => {
            let vault = std::mem::take(&mut form.vault);
            *form = SendForm { vault, ..SendForm::default() };
        }
        Event::Name(value) => form.name = value,
        Event::Text(action) => form.text.perform(action),
        Event::Lifetime(value) => {
            form.lifetime = value;
            form.kept_hours = None;
        }
        Event::Opens(value) => {
            form.opens = value.clamp(1, 99);
            form.unlimited = false;
        }
        Event::Protect(on) => form.protect = on,
        Event::Password(value) => form.password = value,
        Event::Reveal => form.shown = !form.shown,
        Event::HideText(on) => form.hide_text = on,
        Event::HideEmail(on) => form.hide_email = on,
        Event::Vault(value) => form.vault = value,
        Event::Create => {
            form.busy = true;
            return ask(form.request(&app.data.vaults), |result| Message::Sends(Event::Created(result)));
        }
        Event::Created(Ok(Response::SendCreated { send })) => {
            form.busy = false;
            form.open = false;
            form.editing = None;
            form.password.clear();
            form.shown = false;
            form.selected = Some((send.vault.clone(), send.id.clone()));
            form.created = Some(*send);
            return app.reload();
        }
        Event::Created(result) => {
            form.busy = false;
            if let Some(text) = failure_notice(app, &result) {
                shell.toast(text);
            }
        }
        Event::Select(vault, id) => {
            form.open = false;
            form.editing = None;
            form.password.clear();
            form.shown = false;
            form.created = app.data.sends.iter().find(|send| send.vault == vault && send.id == id).cloned();
            form.selected = Some((vault, id));
        }
        Event::CopyLink => {
            if let Some(send) = &form.created {
                let link = send.link.clone();
                form.copied_at = Some(app.now);
                let _ = Copied::Link;
                return iced::clipboard::write(link);
            }
        }
        Event::Mail => {
            if let Some(send) = &form.created {
                let link = format!("mailto:?subject={}&body={}", encode(&send.name), encode(&send.link));
                crate::app::open_detached(link);
            }
        }
        Event::Disable => {
            if let Some(send) = &form.created {
                return ask(Request::DisableSend { vault: send.vault.clone(), id: send.id.clone() }, Message::Done);
            }
        }
        Event::Edit | Event::EditLoaded(..) | Event::StopEditing => return editing(app, event, shell),
        Event::Delete => {
            if let Some(send) = form.created.take() {
                let (vault, query) = (std::mem::take(&mut form.vault), std::mem::take(&mut form.query));
                *form = SendForm { vault, query, ..SendForm::default() };
                return ask(Request::DeleteSend { vault: send.vault, id: send.id }, Message::Done);
            }
        }
        Event::Query(value) => form.query = value,
        Event::ClearQuery => form.query.clear(),
    }
    Task::none()
}

fn editing(app: &mut App, event: Event, shell: &mut Shell<Message>) -> Task<Message> {
    let now = app.now.max(unix_now());
    let form = &mut app.send;
    match event {
        Event::Edit => {
            if let Some(send) = &form.created {
                form.busy = true;
                let (vault, id) = (send.vault.clone(), send.id.clone());
                let request = Request::SendText { vault: vault.clone(), id: id.clone() };
                return ask(request, move |result| {
                    Message::Sends(Event::EditLoaded(vault.clone(), id.clone(), result))
                });
            }
        }
        Event::EditLoaded(vault, id, result) => {
            form.busy = false;
            let asked = form.created.clone().filter(|send| send.vault == vault && send.id == id);
            match (result, asked) {
                (Ok(Response::Secret { secret }), Some(send)) => form.edit(send, secret.expose(), now),
                (Ok(Response::Error { message }) | Err(message), _) => {
                    shell.toast(crate::app::humane(app.lang, &message));
                }
                _ => {}
            }
        }
        Event::StopEditing => {
            form.editing = None;
            form.open = false;
            form.password.clear();
            form.shown = false;
        }
        _ => {}
    }
    Task::none()
}

fn encode(text: &str) -> String {
    text.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => char::from(byte).to_string(),
            other => format!("%{other:02X}"),
        })
        .collect()
}

fn list(app: &App) -> Element<'_, Message> {
    let paint = app.paint;
    let t = |key: Key| key.text(app.lang);
    let now = app.now.max(unix_now());
    let form = &app.send;
    let query = form.query.to_lowercase();
    let sends: Vec<&Send> = app
        .data
        .sends
        .iter()
        .filter(|send| app.vault.as_ref().is_none_or(|vault| &send.vault == vault))
        .filter(|send| query.is_empty() || send.name.to_lowercase().contains(&query))
        .collect();
    let row_of = |send: &Send| -> Element<'_, Message> {
        let (severity, label) = status(send, now);
        let selected = form.selected.as_ref().is_some_and(|(vault, id)| vault == &send.vault && id == &send.id);
        let lead = if active(send, now) {
            ui::glyph_tile(&paint, Icon::Note, 40.0, selected || active(send, now))
        } else {
            ui::tinted_tile(Icon::Note, 40.0, paint.fill_2(), paint.fg_4())
        };
        ui::entry_row(
            &paint,
            lead,
            send.name.clone(),
            describe(app.lang, send, now),
            vec![ui::badge(&paint, label.text(app.lang), severity)],
            selected,
            Message::Sends(Event::Select(send.vault.clone(), send.id.clone())),
        )
    };
    let (live, done): (Vec<&Send>, Vec<&Send>) = sends.into_iter().partition(|send| active(send, now));
    let top = row![
        container(ui::search_field(
            &paint,
            t(Key::SearchSent),
            &form.query,
            |value| Message::Sends(Event::Query(value)),
            Message::Sends(Event::ClearQuery)
        ))
        .width(Fill),
        ui::glyph_button(
            &paint,
            Icon::Plus,
            t(Key::NewSecret),
            Tone::Accent,
            Size::Medium,
            Some(Message::Sends(Event::New))
        ),
    ]
    .spacing(SP2)
    .align_y(Alignment::Center);
    let mut column = Column::new().spacing(SP3).push(container(top).padding(Padding::from([0.0, SP1])));
    column = column.push(heading(&paint, t(Key::ActiveCount).replace("{n}", &live.len().to_string())));
    column = column.push(Column::with_children(live.into_iter().map(row_of)).spacing(2));
    if !done.is_empty() {
        column = column.push(heading(&paint, t(Key::FinishedCount).replace("{n}", &done.len().to_string())));
        column = column.push(Column::with_children(done.into_iter().map(row_of)).spacing(2));
    }
    let body = ui::scrollable(&paint, column).height(Fill);
    let note =
        container(ui::text(t(Key::SendNote), TextStyle::CAP).color(paint.fg_3())).padding(Padding::from([0.0, SP3]));
    ui::pane(paint.view_bg(), 0.0, column![body, note].spacing(SP3).padding(Padding::from([SP4, SP3])))
        .width(420)
        .into()
}

fn heading<'a>(paint: &glide_sdk::Paint, label: String) -> Element<'a, Message> {
    container(ui::text(label, TextStyle::MICRO).color(paint.fg_3())).padding(Padding::from([SP1, SP3])).into()
}

fn lifetime_rows(app: &App) -> Vec<Element<'_, Message>> {
    let paint = app.paint;
    let t = |key: Key| key.text(app.lang);
    let lifetime = app.send.lifetime;
    let custom = if let Lifetime::Custom(days) = lifetime { Lifetime::Custom(days) } else { Lifetime::Custom(3) };
    let lifetimes = [
        (Lifetime::Hour, t(Key::OneHour)),
        (Lifetime::Day, t(Key::OneDay)),
        (Lifetime::Week, t(Key::SevenDays)),
        (Lifetime::Month, t(Key::ThirtyDays)),
        (custom, t(Key::CustomLifetime)),
    ];
    let mut rows = vec![
        ListRow::new(paint, t(Key::Lifetime))
            .suffix(ui::segmented(&paint, &lifetimes, lifetime, |value| Message::Sends(Event::Lifetime(value))))
            .into(),
    ];
    if let Lifetime::Custom(days) = lifetime {
        rows.push(
            ListRow::new(paint, t(Key::CustomDays))
                .suffix(ui::stepper(
                    &paint,
                    days,
                    (days > CUSTOM_DAYS.0).then(|| days - 1),
                    (days < CUSTOM_DAYS.1).then(|| days + 1),
                    |value| Message::Sends(Event::Lifetime(Lifetime::Custom(value))),
                ))
                .into(),
        );
    }
    rows
}

fn form_view(app: &App) -> Element<'_, Message> {
    let paint = app.paint;
    let t = |key: Key| key.text(app.lang);
    let form = &app.send;
    let name_label = row![
        ui::text(t(Key::Name), TextStyle { weight: 520, ..TextStyle::CAP }).color(paint.fg_2()),
        ui::text(t(Key::OnlyYou), TextStyle::CAP).color(paint.fg_3()),
    ]
    .spacing(SP1);
    let name = column![
        name_label,
        ui::field(&paint, text_input(t(Key::Name), &form.name).on_input(|value| Message::Sends(Event::Name(value))))
    ]
    .spacing(6);
    let text = column![
        ui::text(t(Key::TextLabel), TextStyle { weight: 520, ..TextStyle::CAP }).color(paint.fg_2()),
        ui::text_area(&paint, &form.text)
            .on_action(|action| Message::Sends(Event::Text(action)))
            .font(glide_sdk::glide_iced::font::TABULAR)
            .height(96)
            .padding(Padding::from([SP2, SP3])),
    ]
    .spacing(6);
    let unlocked = unlocked_servers(&app.data.vaults);
    let target = form.target(&app.data.vaults);
    let servers: Vec<Labeled<String>> =
        unlocked.iter().map(|vault| labeled(vault.id.clone(), format!("{} · {}", vault.name, vault.account))).collect();
    let chosen = servers.iter().find(|server| server.0 == target).cloned();
    let mut options = lifetime_rows(app);
    options.extend([
        ListRow::new(paint, t(Key::MaxOpens))
            .suffix(ui::stepper(
                &paint,
                form.opens,
                (form.opens > 1).then(|| form.opens - 1),
                (form.opens < 99).then(|| form.opens + 1),
                |value| Message::Sends(Event::Opens(value)),
            ))
            .into(),
        ListRow::new(paint, t(Key::OpenPassword))
            .subtitle(t(Key::OtherChannel))
            .suffix(ui::switch(&paint, form.protect, Some(|on| Message::Sends(Event::Protect(on)))))
            .into(),
    ]);
    if form.protect {
        let hint = if form.had_password { t(Key::KeepSendPassword) } else { t(Key::OpenPassword) };
        options.push(
            container(ui::password_field(
                &paint,
                hint,
                &form.password,
                form.shown,
                |value| Message::Sends(Event::Password(value)),
                Message::Sends(Event::Reveal),
            ))
            .padding(Padding::from([SP1, SP3]))
            .into(),
        );
    }
    options.push(
        ListRow::new(paint, t(Key::HideText))
            .suffix(ui::switch(&paint, form.hide_text, Some(|on| Message::Sends(Event::HideText(on)))))
            .into(),
    );
    options.push(
        ListRow::new(paint, t(Key::HideEmail))
            .suffix(ui::switch(&paint, form.hide_email, Some(|on| Message::Sends(Event::HideEmail(on)))))
            .into(),
    );
    if form.editing.is_none() {
        options.push(
            ListRow::new(paint, t(Key::SendVia))
                .suffix(
                    container(ui::dropdown_with(
                        &paint,
                        Some(crate::view::vault_dot(app, chosen.as_ref().map_or("", |server| server.0.as_str()))),
                        TextStyle::CALL,
                        servers,
                        chosen.clone(),
                        t(Key::SendVia),
                        |picked| Message::Sends(Event::Vault(picked.0)),
                    ))
                    .width(320),
                )
                .into(),
        );
    }
    let title = if form.editing.is_some() { t(Key::EditSecret) } else { t(Key::NewSecret) };
    column![ui::text(title, TextStyle::T2), name, text, ui::list(&paint, options), form_footer(app)].spacing(SP4).into()
}

fn form_footer(app: &App) -> Element<'_, Message> {
    let paint = app.paint;
    let t = |key: Key| key.text(app.lang);
    let form = &app.send;
    let ready = (!form.text.text().trim().is_empty() && !form.busy && app.has_server())
        .then_some(Message::Sends(Event::Create));
    let (cancel, submit, delete) = if form.editing.is_some() {
        (
            Event::StopEditing,
            ui::sized_button(&paint, t(Key::Save), Tone::Accent, Size::Medium, ready),
            Some(ui::sized_button(
                &paint,
                t(Key::DeleteSend),
                Tone::Danger,
                Size::Medium,
                Some(Message::Sends(Event::Delete)),
            )),
        )
    } else {
        (Event::Done, ui::glyph_button(&paint, Icon::Link, t(Key::CreateLink), Tone::Accent, Size::Medium, ready), None)
    };
    row![delete, Space::new().width(Fill), ui::secondary(&paint, t(Key::Cancel), Message::Sends(cancel)), submit]
        .spacing(SP2)
        .into()
}

fn created_view<'a>(app: &'a App, send: &'a Send) -> Element<'a, Message> {
    let paint = app.paint;
    let t = |key: Key| key.text(app.lang);
    let ends = send.expires.unwrap_or(send.deletion);
    let summary = t(Key::LinkSummary)
        .replace("{name}", &send.name)
        .replace("{date}", &crate::i18n::moment(app.lang, ends, app.now))
        .replace("{n}", &send.max_access.unwrap_or_default().to_string());
    let head = row![
        ui::tinted_tile(Icon::Check, 56.0, paint.ok_soft(), paint.ok()),
        column![ui::text(t(Key::LinkReady), TextStyle::T2), ui::text(summary, TextStyle::CALL).color(paint.fg_2())]
            .spacing(2),
    ]
    .spacing(SP4)
    .align_y(Alignment::Center);
    let copied = app.send.copied_at.is_some_and(|at| app.now < at + COPIED_HOLD);
    let label = if copied { t(Key::CopiedShort) } else { t(Key::Copy) };
    let link = ui::link_field(&paint, &send.link, label, Some(Message::Sends(Event::CopyLink)));
    let qr = container(ui::card(
        &paint,
        column![
            ui::qr_code(&paint, &send.link, 148.0),
            ui::text(t(Key::PointCamera), TextStyle::CAP).color(paint.fg_3()),
        ]
        .spacing(SP2)
        .align_x(Alignment::Center),
    ))
    .width(192);
    let side = column![
        ui::text(t(Key::SendHeading), TextStyle::MICRO).color(paint.fg_3()),
        row![
            ui::glyph_button(
                &paint,
                Icon::Mail,
                t(Key::ByMail),
                Tone::Neutral,
                Size::Medium,
                Some(Message::Sends(Event::Mail))
            ),
            ui::glyph_button(
                &paint,
                Icon::Send,
                t(Key::ToApp),
                Tone::Neutral,
                Size::Medium,
                Some(Message::Sends(Event::CopyLink))
            ),
        ]
        .spacing(SP2),
        ui::noted_banner(&paint, Severity::Neutral, Icon::Shield, "", t(Key::LinkNote), Vec::new()),
        row![
            ui::sized_button(
                &paint,
                t(Key::DisableLink),
                Tone::Danger,
                Size::Small,
                (!send.disabled).then_some(Message::Sends(Event::Disable))
            ),
            Space::new().width(Fill),
            ui::sized_button(
                &paint,
                t(Key::EditSend),
                Tone::Neutral,
                Size::Small,
                (!app.send.busy).then_some(Message::Sends(Event::Edit))
            ),
            ui::sized_button(&paint, t(Key::Done), Tone::Accent, Size::Small, Some(Message::Sends(Event::Done))),
        ]
        .spacing(SP2)
        .align_y(Alignment::Center),
    ]
    .spacing(SP3)
    .width(Fill);
    column![head, link, row![qr, side].spacing(SP5).align_y(Alignment::Start)].spacing(SP4).into()
}

#[must_use]
pub fn view(app: &App) -> Element<'_, Message> {
    let paint = app.paint;
    let main: Element<'_, Message> = if app.data.vaults.iter().any(|vault| vault.kind == VaultKind::Bitwarden) {
        let inner = match (&app.send.created, app.send.open) {
            (Some(send), false) => created_view(app, send),
            _ => form_view(app),
        };
        ui::scrollable(&paint, container(inner).padding(Padding::from([SP5, SP6])).max_width(900)).height(Fill).into()
    } else {
        container(ui::empty_state(
            &paint,
            Icon::Send,
            app.t(Key::Sent),
            app.t(Key::NeedsServer),
            Some(ui::primary(&paint, app.t(Key::ConnectAccount), Message::OpenWizard)),
        ))
        .center(Fill)
        .into()
    };
    row![
        list(app),
        ui::vertical_separator(&paint),
        ui::pane(paint.win_bg(), 0.0, container(main).width(Fill).height(Fill))
    ]
    .into()
}

#[cfg(test)]
mod tests {
    use glide_ipc::keychain::SyncState;

    use super::*;

    fn vault(id: &str, kind: VaultKind, unlocked: bool) -> Vault {
        Vault {
            id: id.into(),
            name: id.into(),
            kind,
            account: String::new(),
            server: String::new(),
            exists: true,
            unlocked,
            sync: SyncState::Local,
            count: 0,
        }
    }

    fn created_in(form: &SendForm, vaults: &[Vault]) -> String {
        let Request::CreateSend { draft } = form.request(vaults) else {
            panic!("a form that is not editing creates a send");
        };
        draft.vault
    }

    #[test]
    fn a_new_send_goes_to_the_account_the_dropdown_shows_not_to_a_locked_one() {
        let vaults = [vault("a", VaultKind::Bitwarden, false), vault("b", VaultKind::Bitwarden, true)];
        let form = SendForm { vault: "a".into(), ..SendForm::default() };
        assert_eq!(created_in(&form, &vaults), "b");
    }

    #[test]
    fn a_chosen_unlocked_account_is_kept_and_an_empty_choice_means_the_first_unlocked_one() {
        let vaults = [
            vault("local", VaultKind::Local, true),
            vault("a", VaultKind::Bitwarden, true),
            vault("b", VaultKind::Bitwarden, true),
        ];
        let chosen = SendForm { vault: "b".into(), ..SendForm::default() };
        assert_eq!(created_in(&chosen, &vaults), "b");
        assert_eq!(created_in(&SendForm::default(), &vaults), "a");
        let removed = SendForm { vault: "gone".into(), ..SendForm::default() };
        assert_eq!(created_in(&removed, &vaults), "a", "a vault that no longer exists is not a target");
    }

    #[test]
    fn without_an_unlocked_server_there_is_no_target() {
        let vaults = [vault("local", VaultKind::Local, true), vault("a", VaultKind::Bitwarden, false)];
        assert_eq!(created_in(&SendForm { vault: "a".into(), ..SendForm::default() }, &vaults), "");
    }

    #[test]
    fn editing_a_send_stays_on_the_account_that_owns_it() {
        let vaults = [vault("a", VaultKind::Bitwarden, false), vault("b", VaultKind::Bitwarden, true)];
        let form = SendForm { vault: "a".into(), editing: Some("send-1".into()), ..SendForm::default() };
        let Request::EditSend { draft, .. } = form.request(&vaults) else { panic!("an edit form edits") };
        assert_eq!(draft.vault, "a");
    }

    #[test]
    fn the_target_is_what_the_dropdown_would_pick() {
        let vaults = [vault("a", VaultKind::Bitwarden, false), vault("b", VaultKind::Bitwarden, true)];
        let servers = unlocked_servers(&vaults);
        let shown = chosen_server(&servers, "a").map(|server| server.id.as_str());
        let form = SendForm { vault: "a".into(), ..SendForm::default() };
        assert_eq!(shown, Some(form.target(&vaults).as_str()));
    }

    #[test]
    fn a_locked_reply_to_a_create_is_reported_and_not_swallowed() {
        let harness =
            glide_sdk::Harness::<App>::new(crate::APP_ID, glide_sdk::Look::default(), iced::Size::new(1100.0, 720.0));
        let app = harness.app();
        assert_eq!(failure_notice(app, &Ok(Response::Locked)).as_deref(), Some(app.t(Key::VaultIsLocked)));
        assert_eq!(failure_notice(app, &Ok(Response::Unsupported)).as_deref(), Some(app.t(Key::NeedsServer)));
        assert!(failure_notice(app, &Ok(Response::Error { message: "boom".into() })).is_some());
        assert!(failure_notice(app, &Err("no bus".into())).is_some());
        assert!(failure_notice(app, &Ok(Response::Done)).is_none());
    }
}

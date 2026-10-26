use glide_ipc::keychain::{Hidden, Request, Response, Send, SendDraft, VaultKind};
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
    pub hide_text: bool,
    pub hide_email: bool,
    pub vault: String,
    pub created: Option<Send>,
    pub selected: Option<(String, String)>,
    pub query: String,
    pub busy: bool,
    pub copied_at: Option<u64>,
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
            hide_text: true,
            hide_email: true,
            vault: String::new(),
            created: None,
            selected: None,
            query: String::new(),
            busy: false,
            copied_at: None,
        }
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
    HideText(bool),
    HideEmail(bool),
    Vault(String),
    Create,
    Created(Result<Response, String>),
    Select(String, String),
    CopyLink,
    Mail,
    Disable,
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

pub fn update(app: &mut App, event: Event, shell: &mut Shell<Message>) -> Task<Message> {
    let form = &mut app.send;
    match event {
        Event::New | Event::Done => {
            let vault = std::mem::take(&mut form.vault);
            *form = SendForm { vault, ..SendForm::default() };
        }
        Event::Name(value) => form.name = value,
        Event::Text(action) => form.text.perform(action),
        Event::Lifetime(value) => form.lifetime = value,
        Event::Opens(value) => form.opens = value.clamp(1, 99),
        Event::Protect(on) => form.protect = on,
        Event::Password(value) => form.password = value,
        Event::HideText(on) => form.hide_text = on,
        Event::HideEmail(on) => form.hide_email = on,
        Event::Vault(value) => form.vault = value,
        Event::Create => {
            let vault = if form.vault.is_empty() {
                app.data
                    .vaults
                    .iter()
                    .find(|vault| vault.kind == VaultKind::Bitwarden && vault.unlocked)
                    .map(|vault| vault.id.clone())
                    .unwrap_or_default()
            } else {
                form.vault.clone()
            };
            let draft = SendDraft {
                vault,
                name: form.name.trim().to_string(),
                text: Hidden::new(form.text.text().trim_end().to_string()),
                lifetime_hours: form.lifetime.hours(),
                max_access: Some(form.opens),
                password: (form.protect && !form.password.is_empty()).then(|| Hidden::new(form.password.clone())),
                hide_text: form.hide_text,
                hide_email: form.hide_email,
            };
            form.busy = true;
            return ask(Request::CreateSend { draft: Box::new(draft) }, |result| {
                Message::Sends(Event::Created(result))
            });
        }
        Event::Created(Ok(Response::SendCreated { send })) => {
            form.busy = false;
            form.open = false;
            form.selected = Some((send.vault.clone(), send.id.clone()));
            form.created = Some(*send);
            return app.reload();
        }
        Event::Created(result) => {
            form.busy = false;
            app.send.busy = false;
            match result {
                Ok(Response::Unsupported) => shell.toast(app.t(Key::NeedsServer)),
                Ok(Response::Error { message }) => shell.toast(message),
                Err(err) => shell.toast(err),
                Ok(_) => {}
            }
        }
        Event::Select(vault, id) => {
            form.open = false;
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
                let _ = glide_sdk::system::open(&link, crate::APP_ID);
            }
        }
        Event::Disable => {
            if let Some(send) = &form.created {
                return ask(Request::DisableSend { vault: send.vault.clone(), id: send.id.clone() }, Message::Done);
            }
        }
        Event::Delete => {
            if let Some(send) = form.created.take() {
                form.open = true;
                form.selected = None;
                return ask(Request::DeleteSend { vault: send.vault, id: send.id }, Message::Done);
            }
        }
        Event::Query(value) => form.query = value,
        Event::ClearQuery => form.query.clear(),
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
    let servers: Vec<Labeled<String>> = app
        .data
        .vaults
        .iter()
        .filter(|vault| vault.kind == VaultKind::Bitwarden && vault.unlocked)
        .map(|vault| labeled(vault.id.clone(), format!("{} · {}", vault.name, vault.account)))
        .collect();
    let chosen = servers.iter().find(|server| server.0 == form.vault).cloned().or_else(|| servers.first().cloned());
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
        options.push(
            container(crate::view::secret_input(&paint, t(Key::OpenPassword), &form.password, |value| {
                Message::Sends(Event::Password(value))
            }))
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
    let ready = !form.text.text().trim().is_empty() && !form.busy && app.has_server();
    let footer = row![
        Space::new().width(Fill),
        ui::secondary(&paint, t(Key::Cancel), Message::Sends(Event::Done)),
        ui::glyph_button(
            &paint,
            Icon::Link,
            t(Key::CreateLink),
            Tone::Accent,
            Size::Medium,
            ready.then_some(Message::Sends(Event::Create))
        ),
    ]
    .spacing(SP2);
    column![ui::text(t(Key::NewSecret), TextStyle::T2), name, text, ui::list(&paint, options), footer]
        .spacing(SP4)
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
                t(Key::DeleteSend),
                Tone::Neutral,
                Size::Small,
                Some(Message::Sends(Event::Delete))
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

use glide_ipc::keychain::{
    AppLink, CardDraft, Detail, Draft, FieldDraft, Hidden, ItemKind, ItemRef, Matching, Request, Site,
};
use glide_sdk::theme::{SP2, SP3, SP4, SP5, SP6, TextStyle};
use glide_sdk::widget::{self as ui, Size, Tone};
use glide_sdk::{Paint, Shell};
use iced::widget::{Column, Space, column, container, row, text_editor, text_input};
use iced::{Alignment, Element, Fill, Length, Padding, Task};

use crate::app::{App, Generating, Message, ask};
use crate::i18n::Key;
use crate::icons::Icon;
use crate::view::{Labeled, labeled};

pub struct Editor {
    pub item: Option<ItemRef>,
    pub vault: String,
    pub kind: ItemKind,
    pub name: String,
    pub username: String,
    pub password: String,
    pub original_password: String,
    pub shown: bool,
    pub totp: String,
    pub original_totp: String,
    pub folder: String,
    pub sites: Vec<Site>,
    pub apps: Vec<AppLink>,
    pub fields: Vec<(String, String, bool)>,
    pub notes: text_editor::Content,
    pub favorite: bool,
    pub holder: String,
    pub number: String,
    pub code: String,
    pub expiry: String,
    pub brand: String,
    pub invalid: bool,
}

impl std::fmt::Debug for Editor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Editor").field("item", &self.item).field("kind", &self.kind).finish_non_exhaustive()
    }
}

impl Clone for Editor {
    fn clone(&self) -> Self {
        Self {
            item: self.item.clone(),
            vault: self.vault.clone(),
            kind: self.kind,
            name: self.name.clone(),
            username: self.username.clone(),
            password: self.password.clone(),
            original_password: self.original_password.clone(),
            shown: self.shown,
            totp: self.totp.clone(),
            original_totp: self.original_totp.clone(),
            folder: self.folder.clone(),
            sites: self.sites.clone(),
            apps: self.apps.clone(),
            fields: self.fields.clone(),
            notes: text_editor::Content::with_text(&self.notes.text()),
            favorite: self.favorite,
            holder: self.holder.clone(),
            number: self.number.clone(),
            code: self.code.clone(),
            expiry: self.expiry.clone(),
            brand: self.brand.clone(),
            invalid: self.invalid,
        }
    }
}

impl Editor {
    pub fn take_scan(&mut self, scanned: crate::scan::Scanned) {
        self.totp = scanned.key;
        if self.name.trim().is_empty() && !scanned.issuer.is_empty() {
            self.name = scanned.issuer;
            self.invalid = false;
        }
        if self.username.trim().is_empty() {
            self.username = scanned.account;
        }
    }

    #[must_use]
    pub fn blank(kind: ItemKind, vault: String) -> Self {
        Self {
            item: None,
            vault,
            kind,
            name: String::new(),
            username: String::new(),
            password: String::new(),
            original_password: String::new(),
            shown: true,
            totp: String::new(),
            original_totp: String::new(),
            folder: String::new(),
            sites: if kind == ItemKind::Login { vec![Site::default()] } else { Vec::new() },
            apps: Vec::new(),
            fields: Vec::new(),
            notes: text_editor::Content::new(),
            favorite: false,
            holder: String::new(),
            number: String::new(),
            code: String::new(),
            expiry: String::new(),
            brand: String::new(),
            invalid: false,
        }
    }

    #[must_use]
    pub fn from_detail(detail: &Detail, password: &Hidden, totp: &Hidden, hidden: &[(String, Hidden)]) -> Self {
        let summary = &detail.summary;
        let card = detail.card.clone().unwrap_or_default();
        let value_of = |field: &glide_ipc::keychain::Field| {
            hidden
                .iter()
                .find(|(name, _)| field.hidden && *name == field.name)
                .map_or_else(|| field.value.clone(), |(_, value)| value.expose().to_string())
        };
        Self {
            item: Some(summary.item.clone()),
            vault: summary.item.vault.clone(),
            kind: summary.kind,
            name: summary.name.clone(),
            username: summary.username.clone(),
            password: password.expose().to_string(),
            original_password: password.expose().to_string(),
            shown: true,
            totp: totp.expose().to_string(),
            original_totp: totp.expose().to_string(),
            folder: summary.folder.clone(),
            sites: detail.sites.clone(),
            apps: detail.apps.clone(),
            fields: detail.fields.iter().map(|field| (field.name.clone(), value_of(field), field.hidden)).collect(),
            notes: text_editor::Content::with_text(&detail.notes),
            favorite: summary.favorite,
            holder: card.holder,
            number: String::new(),
            code: String::new(),
            expiry: card.expiry,
            brand: card.brand,
            invalid: false,
        }
    }

    #[must_use]
    pub fn draft(&self) -> Draft {
        let card = (self.kind == ItemKind::Card).then(|| CardDraft {
            holder: self.holder.trim().to_string(),
            brand: self.brand.trim().to_string(),
            number: Hidden::new(self.number.trim().to_string()),
            code: Hidden::new(self.code.trim().to_string()),
            expiry: self.expiry.trim().to_string(),
        });
        Draft {
            item: self.item.clone(),
            vault: self.vault.clone(),
            kind: self.kind,
            name: self.name.trim().to_string(),
            username: self.username.clone(),
            password: (self.password != self.original_password).then(|| Hidden::new(self.password.clone())),
            totp: (self.totp != self.original_totp).then(|| Hidden::new(self.totp.clone())),
            folder: self.folder.clone(),
            sites: self.sites.clone(),
            apps: self.apps.clone(),
            fields: self
                .fields
                .iter()
                .map(|(name, value, hidden)| FieldDraft {
                    name: name.clone(),
                    value: Hidden::new(value.clone()),
                    hidden: *hidden,
                })
                .collect(),
            notes: self.notes.text().trim_end().to_string(),
            favorite: self.favorite,
            card,
        }
    }

    #[must_use]
    pub fn valid(&self) -> bool {
        !self.name.trim().is_empty()
    }
}

#[derive(Debug, Clone)]
pub enum Event {
    Name(String),
    Username(String),
    Password(String),
    TogglePassword,
    Totp(String),
    ScanQr,
    Scanned(Result<Vec<Hidden>, String>),
    Vault(String),
    Folder(String),
    SiteUri(usize, String),
    SiteMatch(usize, Matching),
    RemoveSite(usize),
    AddSite,
    AppId(usize, String),
    AppName(usize, String),
    RemoveApp(usize),
    AddApp,
    FieldName(usize, String),
    FieldValue(usize, String),
    RemoveField(usize),
    AddField,
    Notes(text_editor::Action),
    Holder(String),
    Number(String),
    Code(String),
    Expiry(String),
    Brand(String),
    Generate,
    Save,
    Cancel,
}

pub fn update(app: &mut App, event: Event, shell: &mut Shell<Message>) -> Task<Message> {
    let lang = app.lang;
    let Some(editor) = app.editor.as_mut() else {
        return Task::none();
    };
    match event {
        Event::ScanQr => return crate::scan::screen(|result| Message::Editor(Event::Scanned(result))),
        Event::Scanned(Ok(codes)) => match crate::scan::first_totp(&codes) {
            Some(scanned) => editor.take_scan(scanned),
            None => shell.toast(Key::NoQrOnScreen.text(lang)),
        },
        Event::Scanned(Err(message)) => shell.toast(message),
        Event::Name(value) => {
            editor.name = value;
            editor.invalid = false;
        }
        Event::Username(value) => editor.username = value,
        Event::Password(value) => editor.password = value,
        Event::TogglePassword => editor.shown = !editor.shown,
        Event::Totp(value) => editor.totp = value,
        Event::Vault(value) => editor.vault = value,
        Event::Folder(value) => editor.folder = value,
        Event::SiteUri(index, value) => {
            if let Some(site) = editor.sites.get_mut(index) {
                site.uri = value;
            }
        }
        Event::SiteMatch(index, value) => {
            if let Some(site) = editor.sites.get_mut(index) {
                site.matching = value;
            }
        }
        Event::RemoveSite(index) => {
            if index < editor.sites.len() {
                editor.sites.remove(index);
            }
        }
        Event::AddSite => editor.sites.push(Site::default()),
        Event::AppId(index, value) => {
            if let Some(link) = editor.apps.get_mut(index) {
                link.app_id = value;
            }
        }
        Event::AppName(index, value) => {
            if let Some(link) = editor.apps.get_mut(index) {
                link.name = value;
            }
        }
        Event::RemoveApp(index) => {
            if index < editor.apps.len() {
                editor.apps.remove(index);
            }
        }
        Event::AddApp => editor.apps.push(AppLink::default()),
        Event::FieldName(index, value) => {
            if let Some(field) = editor.fields.get_mut(index) {
                field.0 = value;
            }
        }
        Event::FieldValue(index, value) => {
            if let Some(field) = editor.fields.get_mut(index) {
                field.1 = value;
            }
        }
        Event::RemoveField(index) => {
            if index < editor.fields.len() {
                editor.fields.remove(index);
            }
        }
        Event::AddField => editor.fields.push((String::new(), String::new(), false)),
        Event::Notes(action) => editor.notes.perform(action),
        Event::Holder(value) => editor.holder = value,
        Event::Number(value) => editor.number = value,
        Event::Code(value) => editor.code = value,
        Event::Expiry(value) => editor.expiry = value,
        Event::Brand(value) => editor.brand = value,
        Event::Generate => {
            if app.generator.is_some() {
                app.generator = None;
            } else {
                app.generator = Some(Generating::fresh());
            }
        }
        Event::Save => {
            if !editor.valid() {
                editor.invalid = true;
                return Task::none();
            }
            let draft = editor.draft();
            return ask(Request::Save { draft: Box::new(draft) }, Message::Saved);
        }
        Event::Cancel => {
            app.editor = None;
            app.generator = None;
        }
    }
    Task::none()
}

fn labelled<'a>(label: &'a str, control: impl Into<Element<'a, Message>>, paint: &Paint) -> Element<'a, Message> {
    column![text_label(paint, label), control.into()].spacing(6).width(Fill).into()
}

fn grouped_key(key: &str) -> String {
    if key.contains("://") {
        return key.to_string();
    }
    let compact: Vec<char> = key.chars().filter(|ch| !ch.is_whitespace()).collect();
    compact.chunks(4).map(|chunk| chunk.iter().collect::<String>()).collect::<Vec<_>>().join(" ")
}

fn text_label<'a>(paint: &Paint, label: &'a str) -> Element<'a, Message> {
    ui::text(label, TextStyle { weight: 520, ..TextStyle::CAP }).color(paint.fg_2()).into()
}

fn input<'a>(
    paint: &Paint, placeholder: &'a str, value: &'a str, on_input: impl Fn(String) -> Event + 'a,
) -> Element<'a, Message> {
    ui::field(paint, text_input(placeholder, value).on_input(move |value| Message::Editor(on_input(value))))
}

fn plain_button<'a>(paint: &Paint, glyph: Icon, label: &'a str, message: Message) -> Element<'a, Message> {
    ui::glyph_button(paint, glyph, label, Tone::Neutral, Size::Small, Some(message))
}

#[allow(clippy::too_many_lines)] // The form is laid out top to bottom, one block per field group of the design
pub fn view(app: &App) -> Element<'_, Message> {
    let paint = app.paint;
    let Some(editor) = app.editor.as_ref() else {
        return Space::new().into();
    };
    let t = |key: Key| key.text(app.lang);
    let title = if editor.item.is_some() { t(Key::EditTitle) } else { t(Key::NewTitle) };
    let head = container(
        row![
            ui::text(title, TextStyle::T3).width(Fill),
            ui::text(t(Key::EditHint), TextStyle::CAP).color(paint.fg_3()),
        ]
        .align_y(Alignment::Center),
    )
    .padding(Padding::from([0.0, SP6]))
    .height(64)
    .center_y(64)
    .width(Fill)
    .style(move |_| glide_sdk::glide_iced::style::rounded(paint.bar_bg(), 0.0));

    let name_field = ui::validated_field(
        &paint,
        text_input(t(Key::Name), &editor.name).on_input(|value| Message::Editor(Event::Name(value))).size(16),
        if editor.invalid { ui::Validity::Invalid } else { ui::Validity::Neutral },
    );
    let name_row = row![
        ui::mark_tile(&paint, if editor.name.is_empty() { "?" } else { &editor.name }, 56.0, true),
        labelled(t(Key::Name), name_field, &paint),
    ]
    .spacing(SP4)
    .align_y(Alignment::End);

    let vaults: Vec<Labeled<String>> = app
        .data
        .vaults
        .iter()
        .filter(|vault| vault.unlocked)
        .map(|vault| labeled(vault.id.clone(), vault.name.clone()))
        .collect();
    let chosen = vaults.iter().find(|vault| vault.0 == editor.vault).cloned();
    let vault_pick = ui::dropdown_with(
        &paint,
        Some(crate::view::vault_dot(app, &editor.vault)),
        TextStyle::CALL,
        vaults,
        chosen,
        t(Key::Vault),
        |picked| Message::Editor(Event::Vault(picked.0)),
    );
    let mut folders: Vec<Labeled<String>> = vec![labeled(String::new(), t(Key::NoFolder).to_string())];
    for folder in app.data.folders.iter().filter(|folder| folder.vault == editor.vault) {
        if !folders.iter().any(|known| known.0 == folder.name) {
            folders.push(labeled(folder.name.clone(), folder.name.clone()));
        }
    }
    if !editor.folder.is_empty() && !folders.iter().any(|known| known.0 == editor.folder) {
        folders.push(labeled(editor.folder.clone(), editor.folder.clone()));
    }
    let folder_chosen = folders.iter().find(|folder| folder.0 == editor.folder).cloned();
    let folder_pick = ui::dropdown_with(
        &paint,
        Some(ui::icon(Icon::Folder, 14.0, paint.fg_3())),
        TextStyle::CALL,
        folders,
        folder_chosen,
        t(Key::Folder),
        |picked| Message::Editor(Event::Folder(picked.0)),
    );

    let mut form = Column::new().spacing(SP4).push(name_row).push(
        row![labelled(t(Key::Vault), vault_pick, &paint), labelled(t(Key::Folder), folder_pick, &paint)].spacing(SP4),
    );

    match editor.kind {
        ItemKind::Login | ItemKind::Identity => {
            let user =
                labelled(t(Key::Username), input(&paint, t(Key::Username), &editor.username, Event::Username), &paint);
            let totp = labelled(
                t(Key::TotpKey),
                ui::field_with(
                    &paint,
                    None,
                    text_input("JBSW Y3DP EHPK 3PXP", &grouped_key(&editor.totp))
                        .on_input(|value| Message::Editor(Event::Totp(value.split_whitespace().collect())))
                        .size(13),
                    vec![ui::icon_button(
                        &paint,
                        Icon::Qr,
                        t(Key::ScanQr),
                        Some(Message::Editor(Event::ScanQr)),
                        false,
                    )],
                ),
                &paint,
            );
            form = form.push(row![user, totp].spacing(SP4));
            form = form.push(password_block(app, editor));
            form = form.push(links_block(app, editor));
        }
        ItemKind::Card => {
            form = form.push(
                row![
                    labelled(
                        t(Key::CardHolder),
                        input(&paint, t(Key::CardHolder), &editor.holder, Event::Holder),
                        &paint
                    ),
                    labelled(t(Key::CardBrand), input(&paint, "Visa", &editor.brand, Event::Brand), &paint),
                ]
                .spacing(SP4),
            );
            form = form.push(
                row![
                    labelled(
                        t(Key::CardNumber),
                        input(&paint, "•••• •••• •••• ••••", &editor.number, Event::Number),
                        &paint
                    ),
                    labelled(t(Key::CardExpiry), input(&paint, "MM/YY", &editor.expiry, Event::Expiry), &paint),
                    labelled(t(Key::CardCode), input(&paint, "•••", &editor.code, Event::Code), &paint),
                ]
                .spacing(SP4),
            );
        }
        ItemKind::Note => {}
    }
    for (index, (name, value, _)) in editor.fields.iter().enumerate() {
        form = form.push(
            row![
                container(input(&paint, t(Key::FieldName), name, move |value| Event::FieldName(index, value)))
                    .width(200),
                container(input(&paint, t(Key::FieldValue), value, move |value| Event::FieldValue(index, value)))
                    .width(Fill),
                ui::icon_button(
                    &paint,
                    Icon::Close,
                    t(Key::Remove),
                    Some(Message::Editor(Event::RemoveField(index))),
                    false
                ),
            ]
            .spacing(SP2)
            .align_y(Alignment::Center),
        );
    }
    let notes = ui::text_area(&paint, &editor.notes)
        .placeholder(t(Key::NotesPlaceholder))
        .on_action(|action| Message::Editor(Event::Notes(action)))
        .size(TextStyle::BODY.size_px)
        .height(if editor.kind == ItemKind::Note { 260.0 } else { 84.0 })
        .padding(Padding::from([SP2, SP3]));
    form = form.push(labelled(t(Key::NotesPlaceholder), notes, &paint));

    let body = ui::scrollable(&paint, container(form).padding(Padding::from([SP5, SP6])).width(Fill)).height(Fill);
    let mut footer = row![].spacing(SP2).align_y(Alignment::Center);
    if editor.item.is_some() {
        footer = footer.push(ui::glyph_button(
            &paint,
            Icon::Trash,
            t(Key::ToTrash),
            Tone::Danger,
            Size::Small,
            Some(Message::Trash),
        ));
    }
    footer = footer
        .push(Space::new().width(Fill))
        .push(ui::secondary(&paint, t(Key::Cancel), Message::Editor(Event::Cancel)))
        .push(ui::primary(&paint, t(Key::Save), Message::Editor(Event::Save)));
    let footer = container(footer)
        .padding(Padding::from([0.0, SP6]))
        .height(64)
        .center_y(64)
        .width(Fill)
        .style(move |_| glide_sdk::glide_iced::style::rounded(paint.bar_bg(), 0.0));
    column![head, ui::separator(&paint), body, ui::separator(&paint), footer].height(Fill).into()
}

fn password_block<'a>(app: &'a App, editor: &'a Editor) -> Element<'a, Message> {
    let paint = app.paint;
    let t = |key: Key| key.text(app.lang);
    let reveal_label = if editor.shown { t(Key::HidePassword) } else { t(Key::ShowPassword) };
    let reveal_icon = if editor.shown { Icon::EyeOff } else { Icon::Eye };
    let generate = ui::glyph_button(
        &paint,
        Icon::Wand,
        t(Key::Generate),
        Tone::Tonal,
        Size::Small,
        Some(Message::Editor(Event::Generate)),
    );
    let input = text_input(t(Key::Password), &editor.password)
        .secure(!editor.shown)
        .size(14.0)
        .on_input(|value| Message::Editor(Event::Password(value)));
    let field = ui::field_with(
        &paint,
        None,
        input,
        vec![
            ui::icon_button(&paint, reveal_icon, reveal_label, Some(Message::Editor(Event::TogglePassword)), false),
            generate,
        ],
    );
    let bits = glide_ipc::keychain::strength(&editor.password);
    let mut column = Column::new().spacing(6).push(text_label(&paint, t(Key::Password))).push(field);
    if let Some(strength) = crate::view::strength(bits) {
        let mut line =
            row![ui::strength_line(&paint, strength, crate::view::strength_text(app, strength, &editor.password))]
                .spacing(SP2)
                .align_y(Alignment::Center);
        if editor.item.is_some() && editor.password != editor.original_password && !editor.original_password.is_empty()
        {
            line = line
                .push(Space::new().width(Fill))
                .push(ui::text(t(Key::GoesToHistory), TextStyle::CAP).color(paint.fg_3()));
        }
        column = column.push(line);
    }
    if app.generator.is_some() {
        column = column.push(crate::generator::view(app));
    }
    column.into()
}

fn links_block<'a>(app: &'a App, editor: &'a Editor) -> Element<'a, Message> {
    let paint = app.paint;
    let t = |key: Key| key.text(app.lang);
    let matchings: Vec<Labeled<Matching>> = [
        (Matching::Domain, Key::Domain),
        (Matching::Host, Key::MatchHost),
        (Matching::StartsWith, Key::MatchStarts),
        (Matching::Exact, Key::MatchExact),
        (Matching::Regex, Key::MatchRegex),
        (Matching::Never, Key::MatchNever),
    ]
    .into_iter()
    .map(|(value, key)| labeled(value, t(key).to_string()))
    .collect();
    let mut column = Column::new().spacing(SP2).push(text_label(&paint, t(Key::SitesApps)));
    for (index, site) in editor.sites.iter().enumerate() {
        let chosen = matchings.iter().find(|choice| choice.0 == site.matching).cloned();
        column = column.push(
            row![
                ui::field_with(
                    &paint,
                    Some(ui::icon(Icon::Globe, 16.0, paint.fg_3())),
                    text_input("example.com", &site.uri)
                        .on_input(move |value| Message::Editor(Event::SiteUri(index, value))),
                    Vec::new(),
                ),
                container(ui::dropdown(&paint, matchings.clone(), chosen, t(Key::Domain), move |picked| {
                    Message::Editor(Event::SiteMatch(index, picked.0))
                }))
                .width(150),
                ui::icon_button(
                    &paint,
                    Icon::Close,
                    t(Key::Remove),
                    Some(Message::Editor(Event::RemoveSite(index))),
                    false
                ),
            ]
            .spacing(SP2)
            .align_y(Alignment::Center),
        );
    }
    for (index, link) in editor.apps.iter().enumerate() {
        column = column.push(
            row![
                ui::field_with(
                    &paint,
                    Some(ui::icon(Icon::App, 16.0, paint.fg_3())),
                    text_input(t(Key::AppTitle), &link.name)
                        .on_input(move |value| Message::Editor(Event::AppName(index, value))),
                    vec![
                        container(
                            text_input("org.example.App", &link.app_id)
                                .on_input(move |value| Message::Editor(Event::AppId(index, value)))
                                .font(glide_sdk::glide_iced::font::TABULAR)
                                .align_x(iced::alignment::Horizontal::Right)
                                .size(12)
                                .style(move |_, status| ui::input_style(&paint, status)),
                        )
                        .width(Fill)
                        .into(),
                    ],
                ),
                container(ui::text(t(Key::AppKind), TextStyle::CALL).color(paint.fg_3()))
                    .width(150)
                    .height(glide_sdk::theme::component::CONTROL)
                    .center_y(glide_sdk::theme::component::CONTROL)
                    .padding(Padding::from([0.0, SP3]))
                    .style(move |_| glide_sdk::glide_iced::style::rounded(paint.fill_1(), glide_sdk::theme::R2)),
                ui::icon_button(
                    &paint,
                    Icon::Close,
                    t(Key::Remove),
                    Some(Message::Editor(Event::RemoveApp(index))),
                    false
                ),
            ]
            .spacing(SP2)
            .align_y(Alignment::Center),
        );
    }
    column = column.push(
        row![
            plain_button(&paint, Icon::Plus, t(Key::Site), Message::Editor(Event::AddSite)),
            plain_button(&paint, Icon::Plus, t(Key::AddApp), Message::Editor(Event::AddApp)),
            plain_button(&paint, Icon::Plus, t(Key::AddField), Message::Editor(Event::AddField)),
        ]
        .spacing(SP2),
    );
    column.width(Length::Fill).into()
}

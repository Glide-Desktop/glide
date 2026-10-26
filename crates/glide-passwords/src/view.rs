use std::fmt;

use glide_ipc::keychain::{ItemKind, Matching, Summary, SyncState};
use glide_sdk::theme::{R3, SP1, SP2, SP3, SP4, SP5, SP6, TextStyle};
use glide_sdk::widget::{self as ui, ListRow, Severity, Size, Strength, SyncLine, Tone, VaultState};
use glide_sdk::window::{Header, header};
use glide_sdk::{Item, Lang, Menu, Paint};
use iced::widget::{Column, Space, column, container, row, stack, text_input};
use iced::{Alignment, Element, Fill, Length, Padding};

use crate::app::{App, Copied, Message};
use crate::i18n::{Key, plural};
use crate::icons::Icon;
use crate::model::{self, Section};

pub const SIDEBAR: f32 = 232.0;
pub const LIST: f32 = 344.0;
pub const HEADER: f32 = 48.0;

#[derive(Debug, Clone)]
pub struct Labeled<T>(pub T, pub String);

impl<T> fmt::Display for Labeled<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.1)
    }
}

impl<T: PartialEq> PartialEq for Labeled<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

#[must_use]
pub fn labeled<T>(value: T, label: String) -> Labeled<T> {
    Labeled(value, label)
}

#[must_use]
pub const fn strength(level: u8) -> Option<Strength> {
    match level {
        0 => None,
        1 => Some(Strength::Weak),
        2 => Some(Strength::Fair),
        3 => Some(Strength::Strong),
        _ => Some(Strength::VeryStrong),
    }
}

#[must_use]
pub fn strength_name(lang: Lang, strength: Strength) -> &'static str {
    match strength {
        Strength::Weak => Key::Weak,
        Strength::Fair => Key::Fair,
        Strength::Strong => Key::Strong,
        Strength::VeryStrong => Key::VeryStrong,
    }
    .text(lang)
}

#[must_use]
pub fn strength_text(app: &App, strength: Strength, password: &str) -> String {
    format!(
        "{} · {}",
        strength_name(app.lang, strength),
        plural(app.lang, password.chars().count() as u64, [Key::CharsOne, Key::CharsFew, Key::CharsMany])
    )
}

fn summary_strength_text(app: &App, summary: &Summary, length: u32) -> String {
    let Some(strength) = strength(summary.strength) else {
        return String::new();
    };
    let tail = if summary.reused > 0 {
        app.t(Key::ReusedIn).replace("{n}", &(summary.reused + 1).to_string())
    } else {
        plural(app.lang, u64::from(length), [Key::CharsOne, Key::CharsFew, Key::CharsMany])
    };
    format!("{} · {tail}", strength_name(app.lang, strength))
}

fn section_label(section: Section) -> Key {
    match section {
        Section::All => Key::All,
        Section::Logins => Key::Logins,
        Section::Passkeys => Key::Passkeys,
        Section::Codes => Key::Codes,
        Section::Cards => Key::Cards,
        Section::Notes => Key::Notes,
        Section::Sends => Key::Sent,
        Section::Check => Key::Check,
        Section::Trash => Key::Trash,
    }
}

const fn section_icon(section: Section) -> Icon {
    match section {
        Section::All => Icon::Grid,
        Section::Logins => Icon::Key,
        Section::Passkeys => Icon::Passkey,
        Section::Codes => Icon::Timer,
        Section::Cards => Icon::Card,
        Section::Notes => Icon::Note,
        Section::Sends => Icon::Send,
        Section::Check => Icon::Shield,
        Section::Trash => Icon::Trash,
    }
}

fn count_text<'a>(paint: &Paint, count: usize) -> Element<'a, Message> {
    ui::text(count.to_string(), TextStyle { size_px: 11.0, line_height_px: 14.0, weight: 600, ..TextStyle::NUM })
        .color(paint.fg_3())
        .into()
}

fn title_bar(app: &App) -> Element<'_, Message> {
    let paint = app.paint;
    let controls = app.window.controls_paint(&paint);
    let count = model::count(&app.data.items, None, Section::All);
    let total = plural(app.lang, count as u64, [Key::ItemsOne, Key::ItemsFew, Key::ItemsMany]);
    let center = container(
        row![
            ui::text(app.t(Key::AppName), TextStyle::WIN_TITLE).color(if app.window.focused {
                paint.fg_1()
            } else {
                paint.fg_3()
            }),
            ui::text(format!("· {total}"), TextStyle::CAP).color(paint.fg_3()),
        ]
        .spacing(SP2)
        .align_y(Alignment::Center),
    )
    .center_x(Fill);
    let unlocked = app.data.vaults.iter().any(|vault| vault.unlocked);
    let bar = Header::new(String::new())
        .center(center)
        .labels(app.t(Key::Maximize), app.t(Key::CloseWindow))
        .height(HEADER)
        .trailing(ui::icon_button(&controls, Icon::Sync, app.t(Key::Sync), unlocked.then_some(Message::Sync), false))
        .trailing(ui::icon_button(
            &controls,
            Icon::Lock,
            app.t(Key::LockAll),
            unlocked.then_some(Message::LockAll),
            false,
        ))
        .trailing(ui::glyph_button(
            &controls,
            Icon::Plus,
            app.t(Key::New),
            Tone::Accent,
            Size::Small,
            unlocked.then_some(Message::New(ItemKind::Login)),
        ));
    header(&paint, &app.window, bar, Message::Window, Message::Quit, 0.0)
}

fn sidebar(app: &App) -> Element<'_, Message> {
    let paint = app.paint;
    let mut vaults: Vec<Labeled<Option<String>>> = vec![labeled(None, app.t(Key::AllVaults).to_string())];
    vaults.extend(
        app.data
            .vaults
            .iter()
            .filter(|vault| vault.exists)
            .map(|vault| labeled(Some(vault.id.clone()), vault.name.clone())),
    );
    let chosen = vaults.iter().find(|choice| choice.0 == app.vault).cloned();
    let glyph = match app.vault.as_ref().and_then(|id| app.data.vaults.iter().find(|vault| &vault.id == id)) {
        None => Icon::Grid,
        Some(vault) if vault.kind == glide_ipc::keychain::VaultKind::Local => Icon::Drive,
        Some(vault) if vault.kind == glide_ipc::keychain::VaultKind::KeePass => Icon::Key,
        Some(_) => Icon::Cloud,
    };
    let total = model::count(&app.data.items, None, Section::All);
    let marks: Vec<(Option<String>, Option<iced::Color>, String)> = std::iter::once((None, None, total.to_string()))
        .chain(app.data.vaults.iter().filter(|vault| vault.exists).map(|vault| {
            let colour = match model::vault_state(vault) {
                VaultState::Synced => paint.ok(),
                VaultState::Pending => paint.warn(),
                _ => paint.fg_4(),
            };
            (Some(vault.id.clone()), Some(colour), vault.count.to_string())
        }))
        .collect();
    let picker = ui::dropdown_decorated(
        &paint,
        Some(ui::glyph_tile(&paint, glyph, 22.0, true)),
        TextStyle { weight: 600, ..TextStyle::CALL },
        vaults,
        chosen,
        |picked| Message::Vault(picked.0),
        move |option: &Labeled<Option<String>>| {
            marks
                .iter()
                .find(|(id, ..)| *id == option.0)
                .map_or((None, None), |(_, colour, count)| (*colour, Some(count.clone())))
        },
    );
    let mut column = Column::new().spacing(2).push(container(picker).padding(Padding::from([0.0, SP1]).bottom(SP1)));
    column = column.push(heading(&paint, app.t(Key::Categories), SP3));
    for section in Section::CATEGORIES {
        let count = if section == Section::Sends {
            app.data.sends.iter().filter(|send| app.vault.as_ref().is_none_or(|vault| &send.vault == vault)).count()
        } else {
            model::count(&app.data.items, app.vault.as_deref(), section)
        };
        column = column.push(
            ui::SidebarItem::new(&paint, app.t(section_label(section)))
                .glyph(section_icon(section))
                .accessory(count_text(&paint, count))
                .selected(app.section == section)
                .current(app.section == section)
                .on_press(Some(Message::Section(section))),
        );
    }
    column = column.push(heading(&paint, app.t(Key::Security), SP4));
    let problems = model::problems(&app.data.report, app.vault.as_deref());
    let flagged: Element<'_, Message> =
        if problems > 0 { ui::badge(&paint, problems.to_string(), Severity::Warn) } else { count_text(&paint, 0) };
    column = column.push(
        ui::SidebarItem::new(&paint, app.t(Key::Check))
            .glyph(Icon::Shield)
            .accessory(flagged)
            .selected(app.section == Section::Check)
            .current(app.section == Section::Check)
            .on_press(Some(Message::Section(Section::Check))),
    );
    column = column.push(
        ui::SidebarItem::new(&paint, app.t(Key::Trash))
            .glyph(Icon::Trash)
            .accessory(count_text(&paint, model::count(&app.data.items, app.vault.as_deref(), Section::Trash)))
            .selected(app.section == Section::Trash)
            .current(app.section == Section::Trash)
            .on_press(Some(Message::Section(Section::Trash))),
    );
    column = column.push(Space::new().height(Fill)).push(container(sync_card(app)).padding(Padding::from([0.0, SP1])));
    ui::pane(paint.sidebar_bg(), 0.0, container(column).padding(Padding::from([SP3, SP2])).height(Fill))
        .width(SIDEBAR)
        .into()
}

fn heading<'a>(paint: &Paint, label: &'a str, top: f32) -> Element<'a, Message> {
    container(ui::text(label, TextStyle::MICRO).color(paint.fg_3()))
        .padding(Padding::from([0.0, 14.0]).top(top).bottom(6.0))
        .into()
}

pub fn vault_dot<'a>(app: &App, vault: &str) -> Element<'a, Message> {
    let paint = app.paint;
    let colour = match app.data.vaults.iter().find(|known| known.id == vault).map(model::vault_state) {
        Some(VaultState::Synced) => paint.ok(),
        Some(VaultState::Pending) => paint.warn(),
        _ => paint.fg_4(),
    };
    ui::dot(colour, 8.0)
}

fn sync_card(app: &App) -> Element<'_, Message> {
    let mut vaults: Vec<&glide_ipc::keychain::Vault> = app.data.vaults.iter().filter(|vault| vault.exists).collect();
    vaults.sort_by_key(|vault| vault.id != app.data.default);
    let lines = vaults
        .into_iter()
        .map(|vault| {
            let state = model::vault_state(vault);
            let status = match (&vault.sync, vault.unlocked) {
                (_, false) => app.t(Key::LockedStatus),
                (SyncState::Local, true) => app.t(Key::OnDevice),
                (SyncState::Syncing, true) => app.t(Key::Syncing),
                (SyncState::Synced { .. }, true) => app.t(Key::JustNow),
                (SyncState::Offline { .. }, true) => app.t(Key::OfflineStatus),
                (SyncState::Failed { .. }, true) => app.t(Key::FailedStatus),
            };
            SyncLine { label: vault.name.clone().into(), status: status.into(), state }
        })
        .collect();
    let pending = model::pending(&app.data.vaults);
    let note = (pending > 0)
        .then(|| plural(app.lang, u64::from(pending), [Key::WaitingOne, Key::WaitingFew, Key::WaitingMany]).into());
    ui::sync_card(&app.paint, lines, note)
}

fn list_column(app: &App) -> Element<'_, Message> {
    let paint = app.paint;
    let total = model::count(&app.data.items, app.vault.as_deref(), app.section);
    let phrase = match app.section {
        Section::Logins => Key::SearchLogins,
        Section::Passkeys => Key::SearchPasskeys,
        Section::Codes => Key::SearchCodes,
        Section::Cards => Key::SearchCards,
        Section::Notes => Key::SearchNotes,
        Section::Trash => Key::SearchTrash,
        _ => Key::SearchIn,
    };
    let placeholder = app.t(phrase).replace("{n}", &total.to_string());
    let search = ui::search_field(&paint, &placeholder, &app.query, Message::Query, Message::ClearQuery);
    let top = row![
        container(search).width(Fill),
        ui::icon_button(&paint, Icon::Sort, app.t(Key::Sorting), Some(Message::ToggleSort), app.prefs.sort_by_name),
    ]
    .spacing(SP2)
    .align_y(Alignment::Center);
    let (current, other) = if app.prefs.sort_by_name { (Key::ByName, Key::Recent) } else { (Key::Recent, Key::ByName) };
    let items = model::visible(&app.data.items, app.vault.as_deref(), app.section, &app.query, app.prefs.sort_by_name);
    let body: Element<'_, Message> = if items.is_empty() {
        let (title, text) = if app.query.is_empty() {
            (app.t(Key::NothingHere), app.t(Key::NothingHereBody))
        } else {
            (app.t(Key::NoMatches), "")
        };
        container(ui::empty_note(&paint, Icon::Search, title, text)).padding(SP4).into()
    } else {
        let rows: Vec<Element<'_, Message>> = items.into_iter().map(|summary| entry(app, summary)).collect();
        ui::scrollable(&paint, Column::with_children(rows).spacing(2).padding(Padding::from([0.0, SP2])))
            .height(Fill)
            .into()
    };
    let column = column![
        container(top).padding(Padding::from([SP3, SP3]).bottom(SP2)),
        container(ui::caption_row(&paint, app.t(current), app.t(other))).padding(Padding::from([0.0, SP3])),
        body,
    ];
    ui::pane(paint.view_bg(), 0.0, column).width(LIST).into()
}

fn entry<'a>(app: &'a App, summary: &'a Summary) -> Element<'a, Message> {
    let paint = app.paint;
    let selected = app.selected.as_ref() == Some(&summary.item);
    let mut trailing: Vec<Element<'a, Message>> = Vec::new();
    if summary.has_totp {
        trailing.push(ui::icon(Icon::Timer, 14.0, paint.fg_3()));
    }
    if summary.has_passkey {
        trailing.push(ui::icon(Icon::Passkey, 14.0, paint.fg_3()));
    }
    let vault = app.data.vaults.iter().find(|vault| vault.id == summary.item.vault);
    let dot = match vault.map(model::vault_state) {
        Some(VaultState::Synced) => paint.ok(),
        Some(VaultState::Pending) => paint.warn(),
        _ => paint.fg_4(),
    };
    trailing.push(ui::dot(dot, 6.0));
    let subtitle = match summary.kind {
        ItemKind::Login | ItemKind::Identity => summary.username.clone(),
        ItemKind::Card | ItemKind::Note => summary.folder.clone(),
    };
    let lead: Element<'a, Message> = match summary.kind {
        ItemKind::Note => ui::glyph_tile(&paint, Icon::Note, 32.0, selected),
        ItemKind::Card => ui::glyph_tile(&paint, Icon::Card, 32.0, selected),
        _ => ui::mark_tile(&paint, &summary.name, 32.0, selected),
    };
    ui::entry_row(
        &paint,
        lead,
        summary.name.as_str(),
        subtitle,
        trailing,
        selected,
        Message::Select(summary.item.clone()),
    )
}

fn detail(app: &App) -> Element<'_, Message> {
    let paint = app.paint;
    let Some(detail) = app.detail.as_ref().filter(|detail| Some(&detail.summary.item) == app.selected.as_ref()) else {
        return container(ui::empty_note(&paint, Icon::Key, app.t(Key::SelectItem), "")).center(Fill).into();
    };
    let summary = &detail.summary;
    let t = |key: Key| key.text(app.lang);
    let mut page = Column::new().spacing(SP5).push(detail_head(app, detail));
    let rows = detail_rows(app, detail);
    if !rows.is_empty() {
        page = page.push(ui::list(&paint, rows));
    }
    if matches!(summary.kind, ItemKind::Login | ItemKind::Identity) {
        page = page.push(detail_links(app, detail));
    }
    if !detail.notes.trim().is_empty() {
        let notes = container(ui::text(detail.notes.as_str(), TextStyle::CALL).color(paint.fg_2()))
            .padding(Padding::from([SP2, SP3]))
            .width(Fill);
        page = page.push(ui::section(&paint, t(Key::Notes), ui::list(&paint, vec![notes.into()])));
    }
    let created = t(Key::Created).replace("{date}", &crate::i18n::date(app.lang, summary.created, app.now));
    let mut foot = row![ui::text(created, TextStyle::CAP).color(paint.fg_3())].spacing(SP4).align_y(Alignment::Center);
    if detail.history > 0 {
        foot = foot.push(
            ui::text(t(Key::HistoryCount).replace("{n}", &detail.history.to_string()), TextStyle::CAP)
                .color(paint.acc_text()),
        );
    }
    page = page.push(foot);
    ui::scrollable(&paint, container(page).padding(Padding::from([SP5, SP6])).width(Fill)).height(Fill).into()
}

fn detail_head<'a>(app: &'a App, detail: &'a glide_ipc::keychain::Detail) -> Element<'a, Message> {
    let paint = app.paint;
    let summary = &detail.summary;
    let t = |key: Key| key.text(app.lang);
    let vault = app.data.vaults.iter().find(|vault| vault.id == summary.item.vault);
    let chip_state = vault.map_or(VaultState::Local, model::vault_state);
    let vault_label = if summary.folder.is_empty() {
        app.vault_name(&summary.item.vault).to_string()
    } else {
        format!("{} · {}", app.vault_name(&summary.item.vault), summary.folder)
    };
    let mut meta = row![ui::vault_chip(&paint, vault_label, chip_state)].spacing(SP2).align_y(Alignment::Center);
    meta = meta.push(
        ui::text(
            t(Key::Changed).replace("{date}", &crate::i18n::date(app.lang, summary.modified, app.now)),
            TextStyle::CAP,
        )
        .color(paint.fg_3()),
    );
    let tile: Element<'_, Message> = match summary.kind {
        ItemKind::Note => ui::glyph_tile(&paint, Icon::Note, 56.0, true),
        ItemKind::Card => ui::glyph_tile(&paint, Icon::Card, 56.0, true),
        _ => ui::mark_tile(&paint, &summary.name, 56.0, true),
    };
    let mut head = row![tile, column![ui::text(summary.name.as_str(), TextStyle::T2), meta].spacing(SP1).width(Fill)]
        .spacing(SP4)
        .align_y(Alignment::Center);
    if summary.deleted {
        head = head
            .push(ui::glyph_button(
                &paint,
                Icon::Restore,
                t(Key::Restore),
                Tone::Outline,
                Size::Small,
                Some(Message::Restore),
            ))
            .push(ui::sized_button(&paint, t(Key::DeleteForever), Tone::Danger, Size::Small, Some(Message::AskPurge)));
    } else {
        head = head
            .push(ui::glyph_button(
                &paint,
                Icon::Edit,
                t(Key::EditItem),
                Tone::Outline,
                Size::Small,
                Some(Message::Edit),
            ))
            .push(ui::icon_button(&paint, Icon::More, t(Key::More), Some(Message::MoreMenu), false));
    }
    head.into()
}

fn detail_rows<'a>(app: &'a App, detail: &'a glide_ipc::keychain::Detail) -> Vec<Element<'a, Message>> {
    let paint = app.paint;
    let summary = &detail.summary;
    let t = |key: Key| key.text(app.lang);
    let mut rows: Vec<Element<'a, Message>> = Vec::new();
    if matches!(summary.kind, ItemKind::Login | ItemKind::Identity) {
        rows.push(ui::field_row(
            &paint,
            t(Key::Username),
            ui::text(summary.username.as_str(), TextStyle::BODY),
            vec![ui::copy_button(&paint, t(Key::CopyUsername), Message::Copy(Copied::Username))],
        ));
        if summary.has_password {
            let shown = match &app.revealed {
                Some(secret) => secret.expose().to_string(),
                None => ui::masked(&"•".repeat(detail.password_length as usize)),
            };
            let mut value = Column::new().spacing(6).push(ui::text(shown, ui::SECRET).wrapping(ui::Wrapping::None));
            if let Some(level) = strength(summary.strength) {
                value = value.push(ui::strength_line(
                    &paint,
                    level,
                    summary_strength_text(app, summary, detail.password_length),
                ));
            }
            let (glyph, label) = if app.revealed.is_some() {
                (Icon::EyeOff, t(Key::HidePassword))
            } else {
                (Icon::Eye, t(Key::ShowPassword))
            };
            rows.push(ui::field_row(
                &paint,
                t(Key::Password),
                value,
                vec![
                    ui::icon_button(&paint, glyph, label, Some(Message::Reveal), false),
                    ui::copy_button(&paint, t(Key::CopyPassword), Message::Copy(Copied::Password)),
                ],
            ));
        }
        if let Some(code) = app.code.as_ref().filter(|_| summary.has_totp) {
            let urgent = code.seconds_left <= 5;
            rows.push(ui::field_row(
                &paint,
                t(Key::Code),
                ui::text(ui::grouped_code(&code.code), ui::CODE)
                    .font(glide_sdk::glide_iced::font::TABULAR)
                    .color(if urgent { paint.danger() } else { paint.acc_text() }),
                vec![
                    ui::countdown_ring(&paint, code.seconds_left, code.period),
                    ui::copy_button(&paint, t(Key::CopyCode), Message::Copy(Copied::Code)),
                ],
            ));
        }
        for passkey in &detail.passkeys {
            let created = crate::i18n::date(app.lang, passkey.created, app.now);
            rows.push(
                ListRow::new(paint, t(Key::Passkey))
                    .strong()
                    .subtitle(t(Key::PasskeyCreated).replace("{site}", &passkey.rp_id).replace("{date}", &created))
                    .prefix(ui::glyph_tile(&paint, Icon::Passkey, 32.0, true))
                    .suffix(ui::badge(&paint, t(Key::PasskeySynced), Severity::Accent))
                    .into(),
            );
        }
    }
    if let Some(card) = &detail.card {
        rows.push(ui::field_row(
            &paint,
            t(Key::CardHolder),
            ui::text(card.holder.as_str(), TextStyle::BODY),
            Vec::new(),
        ));
        rows.push(ui::field_row(
            &paint,
            t(Key::CardNumber),
            ui::text(format!("•••• {}", card.last_digits), ui::SECRET),
            Vec::new(),
        ));
        rows.push(ui::field_row(
            &paint,
            t(Key::CardExpiry),
            ui::text(card.expiry.as_str(), TextStyle::BODY),
            Vec::new(),
        ));
    }
    for field in &detail.fields {
        let value = if field.hidden { ui::masked("••••••••") } else { field.value.clone() };
        rows.push(ui::field_row(&paint, field.name.as_str(), ui::text(value, TextStyle::BODY), Vec::new()));
    }
    rows
}

fn detail_links<'a>(app: &'a App, detail: &'a glide_ipc::keychain::Detail) -> Element<'a, Message> {
    let paint = app.paint;
    let summary = &detail.summary;
    let t = |key: Key| key.text(app.lang);
    {
        let mut links: Vec<Element<'a, Message>> = Vec::new();
        for site in &detail.sites {
            let kind = match site.matching {
                Matching::Domain => t(Key::Domain),
                Matching::Host => t(Key::MatchHost),
                Matching::StartsWith => t(Key::MatchStarts),
                Matching::Exact => t(Key::MatchExact),
                Matching::Regex => t(Key::MatchRegex),
                Matching::Never => t(Key::MatchNever),
            };
            links.push(
                ListRow::new(paint, site.uri.clone())
                    .prefix(ui::icon(Icon::Globe, 18.0, paint.fg_3()))
                    .suffix(
                        row![
                            ui::text(kind, TextStyle::CAP).color(paint.fg_3()),
                            ui::icon_button(
                                &paint,
                                Icon::External,
                                t(Key::OpenSite),
                                Some(Message::OpenLink(site.uri.clone())),
                                false
                            ),
                        ]
                        .spacing(SP2)
                        .align_y(Alignment::Center),
                    )
                    .into(),
            );
        }
        for link in &detail.apps {
            let name = if link.name.is_empty() { link.app_id.clone() } else { link.name.clone() };
            links.push(
                ListRow::new(paint, name)
                    .beside(
                        ui::text(link.app_id.clone(), TextStyle::CAP)
                            .font(glide_sdk::glide_iced::font::TABULAR)
                            .color(paint.fg_3()),
                    )
                    .prefix(ui::icon(Icon::App, 18.0, paint.fg_3()))
                    .suffix(ui::text(t(Key::AppKind), TextStyle::CAP).color(paint.fg_3()))
                    .into(),
            );
        }
        if !summary.deleted {
            links.push(
                ListRow::new(paint, t(Key::Bind))
                    .accent()
                    .prefix(ui::icon(Icon::Plus, 18.0, paint.acc_text()))
                    .on_press(Message::Edit)
                    .into(),
            );
        }
        ui::section(&paint, t(Key::SitesApps), ui::list(&paint, links))
    }
}

fn locked(app: &App) -> Element<'_, Message> {
    let paint = app.paint;
    let action = column![
        ui::primary(&paint, app.t(Key::Unlock), Message::RequestUnlock),
        ui::text(app.t(Key::PromptAbove), TextStyle::CAP).color(paint.fg_3()),
    ]
    .spacing(SP2)
    .align_x(Alignment::Center);
    container(ui::empty_state(&paint, Icon::Lock, app.t(Key::LockedTitle), app.t(Key::LockedBody), Some(action.into())))
        .center(Fill)
        .into()
}

fn first_run(app: &App) -> Element<'_, Message> {
    let paint = app.paint;
    let actions = row![
        ui::button(&paint, app.t(Key::ConnectAccount), Tone::Outline, Some(Message::OpenWizard)),
        ui::primary(&paint, app.t(Key::CreateVault), Message::CreateVault(crate::wizard::CreateEvent::Open)),
    ]
    .spacing(SP2);
    container(ui::empty_state(
        &paint,
        Icon::Key,
        app.t(Key::NoVaultTitle),
        app.t(Key::NoVaultBody),
        Some(actions.into()),
    ))
    .center(Fill)
    .into()
}

fn content(app: &App) -> Element<'_, Message> {
    let paint = app.paint;
    if !app.loaded {
        return container(ui::spinner(&paint, true)).center(Fill).into();
    }
    if let Some(failure) = &app.failure {
        let body = if crate::app::daemon_absent(failure) { app.t(Key::DaemonMissingBody) } else { failure.as_str() };
        let retry = ui::sized_button(&paint, app.t(Key::TryAgain), Tone::Neutral, Size::Small, Some(Message::Reload));
        return container(ui::empty_state(&paint, Icon::Warning, app.t(Key::DaemonMissing), body, Some(retry)))
            .center(Fill)
            .into();
    }
    if !app.any_vault() {
        return first_run(app);
    }
    if app.all_locked() {
        return locked(app);
    }
    let main: Element<'_, Message> = match app.section {
        Section::Sends => crate::sends::view(app),
        Section::Check => crate::security::view(app),
        _ => {
            let right: Element<'_, Message> = if app.editor.is_some() { crate::editor::view(app) } else { detail(app) };
            row![
                list_column(app),
                ui::vertical_separator(&paint),
                ui::pane(paint.win_bg(), 0.0, container(right).width(Fill).height(Fill)),
            ]
            .into()
        }
    };
    row![sidebar(app), ui::vertical_separator(&paint), container(main).width(Fill).height(Fill)].into()
}

#[must_use]
pub fn view(app: &App) -> Element<'_, Message> {
    let paint = app.paint;
    let corner = app.window.corner();
    let body = column![title_bar(app), ui::separator(&paint), container(content(app)).width(Fill).height(Fill)];
    let framed = ui::pane(paint.win_bg(), iced::border::Radius::new(corner), body);
    let mut layers: Vec<Element<'_, Message>> = vec![framed.into()];
    if !app.window.edge_to_edge() {
        layers.push(glide_sdk::window::resize_handles(Message::Window));
    }
    if app.conflict_open
        && let Some(sheet) = crate::conflict::view(app)
    {
        layers.push(sheet);
    }
    if app.settings {
        layers.push(crate::settings::view(app));
    }
    if let Some(sheet) = crate::wizard::view(app) {
        layers.push(sheet);
    }
    if let Some(sheet) = crate::transfer::view(app) {
        layers.push(sheet);
    }
    stack(layers).into()
}

#[must_use]
pub fn menus(app: &App) -> Vec<Menu<Message>> {
    let t = |key: Key| key.text(app.lang);
    let unlocked = app.data.vaults.iter().any(|vault| vault.unlocked);
    let selected = app.selected.is_some() && app.editor.is_none();
    vec![
        Menu::new(
            t(Key::File),
            vec![
                Item::action(t(Key::NewLogin), Message::New(ItemKind::Login)).shortcut("Ctrl+N").enabled(unlocked),
                Item::action(t(Key::NewNote), Message::New(ItemKind::Note)).enabled(unlocked),
                Item::action(t(Key::NewCard), Message::New(ItemKind::Card)).enabled(unlocked),
                Item::separator(),
                Item::action(t(Key::AddVaultEllipsis), Message::OpenWizard),
                Item::action(t(Key::Sync), Message::Sync).enabled(unlocked),
                Item::action(t(Key::LockAll), Message::LockAll).shortcut("Ctrl+L").enabled(unlocked),
                Item::separator(),
                Item::action(t(Key::Settings), Message::OpenSettings).shortcut("Ctrl+,"),
                Item::action(t(Key::Quit), Message::Quit).shortcut("Ctrl+Q"),
            ],
        ),
        Menu::new(
            t(Key::Edit),
            vec![
                Item::action(t(Key::CopyUsername), Message::Copy(Copied::Username))
                    .shortcut("Ctrl+B")
                    .enabled(selected),
                Item::action(t(Key::CopyPassword), Message::Copy(Copied::Password))
                    .shortcut("Ctrl+Shift+C")
                    .enabled(selected),
                Item::separator(),
                Item::action(t(Key::EditItem), Message::Edit).shortcut("Ctrl+E").enabled(selected),
                Item::action(t(Key::ToTrash), Message::Trash).enabled(selected),
            ],
        ),
        Menu::new(
            t(Key::View),
            vec![
                Item::action(t(Key::SortRecent), Message::ToggleSort).checked(!app.prefs.sort_by_name),
                Item::action(t(Key::SortName), Message::ToggleSort).checked(app.prefs.sort_by_name),
                Item::separator(),
                Item::action(t(Key::Check), Message::Section(Section::Check)),
                Item::action(t(Key::Sent), Message::Section(Section::Sends)),
                Item::action(t(Key::Trash), Message::Section(Section::Trash)),
            ],
        ),
    ]
}

pub fn dialog_frame<'a>(
    app: &App, width: f32, content: impl Into<Element<'a, Message>>, dismiss: Message,
) -> Element<'a, Message> {
    let paint = app.paint;
    let card = container(content)
        .padding(SP5)
        .width(Length::Fixed(width))
        .style(move |_| glide_sdk::glide_iced::style::popover(&paint));
    glide_sdk::overlay::modal(&paint, 1.0, card, Some(dismiss), app.window.corner())
}

#[must_use]
pub fn secret_input<'a>(
    paint: &Paint, placeholder: &'a str, value: &'a str, on_input: impl Fn(String) -> Message + 'a,
) -> Element<'a, Message> {
    ui::field(paint, text_input(placeholder, value).secure(true).on_input(on_input))
}

pub const DIALOG_RADIUS: f32 = R3;

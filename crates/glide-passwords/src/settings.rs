use glide_ipc::keychain::{Hidden, Request, SyncState, VaultKind};
use glide_sdk::theme::{SP2, SP3, SP4, SP5, SP6, TextStyle};
use glide_sdk::widget::{self as ui, ListRow, Severity, Size, Tone};
use glide_sdk::{Item, Shell};
use iced::widget::{Column, column, container, row};
use iced::{Alignment, Element, Fill, Length, Padding, Task};

use crate::app::{App, Message, ask};
use crate::i18n::{Key, plural};
use crate::icons::Icon;
use crate::prefs::{CLEAR_CHOICES, ClickAction, LOCK_CHOICES};
use crate::view::{Labeled, labeled};

#[derive(Debug, Clone)]
pub enum Event {
    Click(ClickAction),
    Offer(bool),
    TypeFallback(bool),
    ClearAfter(u32),
    PrivateClipboard(bool),
    LockWithScreen(bool),
    LockAfter(u32),
    SecretService(bool),
    LeakCheck(bool),
    Default(String),
    VaultMenu(String),
    Sync(String),
    Remove(String),
    ChangePassword(String),
    Old(String),
    New(String),
    Repeat(String),
    SubmitPassword,
    CancelPassword,
    Soon,
}

#[derive(Debug, Clone, Default)]
pub struct PasswordChange {
    pub vault: String,
    pub old: String,
    pub new: String,
    pub repeat: String,
}

pub fn update(app: &mut App, event: Event, shell: &mut Shell<Message>) -> Task<Message> {
    let mut save = true;
    let leak_check_turned_on = matches!(event, Event::LeakCheck(true)) && !app.prefs.leak_check;
    let secret_service = match event {
        Event::SecretService(on) if on != app.prefs.secret_service => Some(on),
        _ => None,
    };
    match event {
        Event::Click(action) => app.prefs.click = action,
        Event::Offer(on) => app.prefs.offer = on,
        Event::TypeFallback(on) => app.prefs.type_fallback = on,
        Event::ClearAfter(seconds) => app.prefs.clear_after = seconds,
        Event::PrivateClipboard(on) => app.prefs.private_clipboard = on,
        Event::LockWithScreen(on) => app.prefs.lock_with_screen = on,
        Event::LockAfter(minutes) => app.prefs.lock_after = minutes,
        Event::SecretService(on) => app.prefs.secret_service = on,
        Event::LeakCheck(on) => app.prefs.leak_check = on,
        Event::Default(vault) => return ask(Request::SetDefault { vault }, Message::Done),
        Event::VaultMenu(vault) => {
            let items = vec![
                Item::action(app.t(Key::Sync), Message::Settings(Event::Sync(vault.clone()))),
                Item::action(app.t(Key::ChangeVaultPassword), Message::Settings(Event::ChangePassword(vault.clone()))),
                Item::separator(),
                Item::action(app.t(Key::RemoveVault), Message::Settings(Event::Remove(vault))),
            ];
            shell.context_menu(items);
            save = false;
        }
        Event::Sync(vault) => return ask(Request::Sync { vault: Some(vault) }, Message::Done),
        Event::Remove(vault) => return ask(Request::RemoveVault { vault }, Message::Done),
        Event::ChangePassword(vault) => {
            app.password_change = Some(PasswordChange { vault, ..PasswordChange::default() });
            save = false;
        }
        Event::Old(value) => {
            if let Some(change) = app.password_change.as_mut() {
                change.old = value;
            }
            save = false;
        }
        Event::New(value) => {
            if let Some(change) = app.password_change.as_mut() {
                change.new = value;
            }
            save = false;
        }
        Event::Repeat(value) => {
            if let Some(change) = app.password_change.as_mut() {
                change.repeat = value;
            }
            save = false;
        }
        Event::SubmitPassword => {
            let Some(change) = app.password_change.take() else {
                return Task::none();
            };
            if change.new != change.repeat || change.new.is_empty() {
                shell.toast(app.t(Key::Mismatch));
                app.password_change = Some(change);
                return Task::none();
            }
            return ask(
                Request::ChangePassword {
                    vault: change.vault,
                    old: Hidden::new(change.old),
                    new: Hidden::new(change.new),
                },
                Message::Done,
            );
        }
        Event::CancelPassword => {
            app.password_change = None;
            save = false;
        }
        Event::Soon => {
            shell.toast(app.t(Key::Soon));
            save = false;
        }
    }
    if save {
        app.prefs.save();
        if leak_check_turned_on {
            return app.check_leaks();
        }
        if let Some(enabled) = secret_service {
            if !enabled {
                shell.toast(app.t(Key::SecretServiceOff));
            }
            return ask(Request::SecretService { enabled }, Message::Done);
        }
    }
    Task::none()
}

fn heading<'a>(app: &App, key: Key) -> Element<'a, Message> {
    container(ui::text(key.text(app.lang), TextStyle::MICRO).color(app.paint.fg_3()))
        .padding(Padding::from([0.0, SP2]).bottom(SP2))
        .into()
}

fn choice<'a, T: PartialEq + Clone + 'a>(
    app: &App, options: Vec<Labeled<T>>, value: &T, on_pick: impl Fn(T) -> Message + 'a,
) -> Element<'a, Message> {
    let chosen = options.iter().find(|option| &option.0 == value).cloned();
    container(ui::dropdown(&app.paint, options, chosen, "", move |picked: Labeled<T>| on_pick(picked.0)))
        .width(200)
        .into()
}

fn vault_rows(app: &App) -> Vec<Element<'_, Message>> {
    let paint = app.paint;
    let t = |key: Key| key.text(app.lang);
    let mut rows: Vec<Element<'_, Message>> = Vec::new();
    for vault in app.data.vaults.iter().filter(|vault| vault.exists) {
        let (glyph, highlighted) = match vault.kind {
            VaultKind::Local => (Icon::Drive, false),
            VaultKind::Bitwarden if vault.server.contains("bitwarden.") || vault.server.is_empty() => {
                (Icon::Cloud, true)
            }
            VaultKind::Bitwarden => (Icon::Server, false),
        };
        let items = plural(app.lang, u64::from(vault.count), [Key::ItemsOne, Key::ItemsFew, Key::ItemsMany]);
        let mut subtitle = match vault.kind {
            VaultKind::Local => format!("{} · {items}", t(Key::OnDeviceTitle)),
            VaultKind::Bitwarden => format!("{} · {} · {items}", vault.account, vault.server),
        };
        if let SyncState::Offline { pending } = vault.sync
            && pending > 0
        {
            let queued = plural(app.lang, u64::from(pending), [Key::QueuedOne, Key::QueuedFew, Key::QueuedMany]);
            subtitle = format!("{subtitle} · {queued}");
        }
        let status: Element<'_, Message> = if vault.unlocked {
            match vault.sync {
                SyncState::Local => ui::sized_button(
                    &paint,
                    t(Key::ChangeVaultPassword),
                    Tone::Neutral,
                    Size::Small,
                    Some(Message::Settings(Event::ChangePassword(vault.id.clone()))),
                ),
                SyncState::Synced { .. } | SyncState::Syncing => ui::badge(&paint, t(Key::SyncedBadge), Severity::Ok),
                SyncState::Offline { .. } | SyncState::Failed { .. } => {
                    ui::badge(&paint, t(Key::OfflineBadge), Severity::Warn)
                }
            }
        } else {
            ui::badge(&paint, t(Key::LockedBadge), Severity::Neutral)
        };
        let mut suffix = row![status].spacing(SP2).align_y(Alignment::Center);
        if vault.kind == VaultKind::Bitwarden {
            suffix = suffix.push(ui::icon_button(
                &paint,
                Icon::More,
                t(Key::More),
                Some(Message::Settings(Event::VaultMenu(vault.id.clone()))),
                false,
            ));
        }
        rows.push(
            ListRow::new(paint, vault.name.clone())
                .strong()
                .subtitle(subtitle)
                .prefix(ui::glyph_tile(&paint, glyph, 32.0, highlighted))
                .suffix(suffix)
                .into(),
        );
    }
    let defaults: Vec<Labeled<String>> = app
        .data
        .vaults
        .iter()
        .filter(|vault| vault.exists)
        .map(|vault| labeled(vault.id.clone(), vault.name.clone()))
        .collect();
    let chosen = defaults.iter().find(|option| option.0 == app.data.default).cloned();
    let default_pick = ui::dropdown_with(
        &paint,
        Some(crate::view::vault_dot(app, &app.data.default)),
        TextStyle::CALL,
        defaults,
        chosen,
        "",
        |picked: Labeled<String>| Message::Settings(Event::Default(picked.0)),
    );
    rows.push(ListRow::new(paint, t(Key::SaveNewIn)).suffix(container(default_pick).width(200)).into());
    rows.push(
        ListRow::new(paint, t(Key::AddVaultEllipsis))
            .accent()
            .prefix(ui::icon(Icon::Plus, 18.0, paint.acc_text()))
            .on_press(Message::OpenWizard)
            .into(),
    );
    rows
}

fn password_form(app: &App) -> Option<Element<'_, Message>> {
    let change = app.password_change.as_ref()?;
    let paint = app.paint;
    let t = |key: Key| key.text(app.lang);
    Some(ui::list(
        &paint,
        vec![
            container(
                column![
                    ui::text(
                        format!(
                            "{} · {}",
                            t(Key::ChangeVaultPassword).trim_end_matches('…'),
                            app.vault_name(&change.vault)
                        ),
                        TextStyle::HEAD
                    ),
                    crate::view::secret_input(&paint, t(Key::VaultPassword), &change.old, |value| Message::Settings(
                        Event::Old(value)
                    )),
                    crate::view::secret_input(
                        &paint,
                        t(Key::NewVaultPassword),
                        &change.new,
                        |value| Message::Settings(Event::New(value))
                    ),
                    crate::view::secret_input(&paint, t(Key::RepeatPassword), &change.repeat, |value| {
                        Message::Settings(Event::Repeat(value))
                    }),
                    row![
                        iced::widget::Space::new().width(Fill),
                        ui::secondary(&paint, t(Key::Cancel), Message::Settings(Event::CancelPassword)),
                        ui::primary(&paint, t(Key::Save), Message::Settings(Event::SubmitPassword)),
                    ]
                    .spacing(SP2),
                ]
                .spacing(SP3),
            )
            .padding(SP3)
            .into(),
        ],
    ))
}

fn switch<'a>(app: &App, on: bool, event: fn(bool) -> Event) -> Element<'a, Message> {
    ui::switch(&app.paint, on, Some(move |on| Message::Settings(event(on))))
}

fn upper(app: &App) -> Column<'_, Message> {
    let paint = app.paint;
    let t = |key: Key| key.text(app.lang);
    let prefs = &app.prefs;
    let clicks: Vec<Labeled<ClickAction>> = ClickAction::ALL
        .into_iter()
        .map(|action| {
            let key = match action {
                ClickAction::Password => Key::InsertPassword,
                ClickAction::Username => Key::InsertUsername,
                ClickAction::Both => Key::InsertBoth,
                ClickAction::Code => Key::InsertCode,
            };
            labeled(action, t(key).to_string())
        })
        .collect();
    let mut page = Column::new().spacing(SP5);
    page = page.push(column![heading(app, Key::Vaults), ui::list(&paint, vault_rows(app))]);
    if let Some(form) = password_form(app) {
        page = page.push(form);
    }
    page = page.push(column![
        heading(app, Key::Filling),
        ui::list(
            &paint,
            vec![
                ListRow::new(paint, t(Key::ClickTopbar))
                    .subtitle(t(Key::ClickHint))
                    .suffix(choice(app, clicks, &prefs.click, |action| Message::Settings(Event::Click(action))))
                    .into(),
                ListRow::new(paint, t(Key::OfferLogin))
                    .subtitle(t(Key::OfferHint))
                    .suffix(switch(app, prefs.offer, Event::Offer))
                    .into(),
                ListRow::new(paint, t(Key::TypeFallback))
                    .subtitle(t(Key::TypeHint))
                    .suffix(switch(app, prefs.type_fallback, Event::TypeFallback))
                    .into(),
            ],
        ),
    ]);
    page
}

fn lower(app: &App) -> Column<'_, Message> {
    let paint = app.paint;
    let t = |key: Key| key.text(app.lang);
    let prefs = &app.prefs;
    let seconds = |n: u32| -> String {
        if n == 0 {
            t(Key::NeverClear).to_string()
        } else {
            plural(app.lang, u64::from(n), [Key::SecondsOne, Key::SecondsFew, Key::SecondsMany])
        }
    };
    let minutes = |n: u32| -> String {
        if n == 0 {
            t(Key::NeverClear).to_string()
        } else {
            plural(app.lang, u64::from(n), [Key::MinutesOne, Key::MinutesFew, Key::MinutesMany])
        }
    };
    let mut page = Column::new().spacing(SP5);
    let clears: Vec<Labeled<u32>> = CLEAR_CHOICES.into_iter().map(|n| labeled(n, seconds(n))).collect();
    page = page.push(column![
        heading(app, Key::Clipboard),
        ui::list(
            &paint,
            vec![
                ListRow::new(paint, t(Key::ClearAfter))
                    .suffix(choice(app, clears, &prefs.clear_after, |n| Message::Settings(Event::ClearAfter(n))))
                    .into(),
                ListRow::new(paint, t(Key::NoHistory))
                    .subtitle(t(Key::NoHistoryHint))
                    .suffix(switch(app, prefs.private_clipboard, Event::PrivateClipboard))
                    .into(),
            ],
        ),
    ]);
    let locks: Vec<Labeled<u32>> = LOCK_CHOICES.into_iter().map(|n| labeled(n, minutes(n))).collect();
    page = page.push(column![
        heading(app, Key::Locking),
        ui::list(
            &paint,
            vec![
                ListRow::new(paint, t(Key::LockWithScreen))
                    .suffix(switch(app, prefs.lock_with_screen, Event::LockWithScreen))
                    .into(),
                ListRow::new(paint, t(Key::LockIdle))
                    .suffix(choice(app, locks, &prefs.lock_after, |n| Message::Settings(Event::LockAfter(n))))
                    .into(),
                ListRow::new(paint, t(Key::Fingerprint))
                    .subtitle(t(Key::LaterNote))
                    .dimmed(true)
                    .suffix(ui::switch(&paint, false, None::<fn(bool) -> Message>))
                    .into(),
            ],
        ),
    ]);
    page = page.push(column![
        heading(app, Key::OtherApps),
        ui::list(
            &paint,
            vec![
                ListRow::new(paint, t(Key::SecretService))
                    .subtitle(t(Key::SecretServiceHint))
                    .suffix(switch(app, prefs.secret_service, Event::SecretService))
                    .into(),
                ListRow::new(paint, t(Key::ImportLabel))
                    .suffix(ui::glyph_button(
                        &paint,
                        Icon::Import,
                        t(Key::ImportButton),
                        Tone::Neutral,
                        Size::Small,
                        Some(Message::Transfer(crate::transfer::Event::Import))
                    ))
                    .into(),
                ListRow::new(paint, t(Key::ExportLabel))
                    .suffix(ui::glyph_button(
                        &paint,
                        Icon::Export,
                        t(Key::ExportButton),
                        Tone::Neutral,
                        Size::Small,
                        Some(Message::Transfer(crate::transfer::Event::Export))
                    ))
                    .into(),
            ],
        ),
    ]);
    page
}

#[must_use]
pub fn view(app: &App) -> Element<'_, Message> {
    let paint = app.paint;
    let t = |key: Key| key.text(app.lang);
    let page = column![upper(app), lower(app)].spacing(SP5);
    let head = row![
        iced::widget::Space::new().width(26),
        container(ui::text(t(Key::SettingsTitle), TextStyle::WIN_TITLE)).center_x(Fill),
        glide_sdk::window::close_button(&paint, &app.window, t(Key::CloseWindow), Message::CloseSettings),
    ]
    .align_y(Alignment::Center)
    .padding(Padding::from([0.0, 11.0]))
    .height(48);
    let height = (app.window.size.height - 2.0 * SP5).max(320.0);
    let body = ui::scrollable(&paint, container(page).padding(Padding::from([SP5, SP6 + SP4]))).height(Fill);
    let sheet = container(column![head, ui::separator(&paint), ui::pane(paint.win_bg(), 0.0, body)])
        .width(Length::Fixed(800.0_f32.min(app.window.size.width - 2.0 * SP4)))
        .height(Length::Fixed(height.min(1000.0)));
    glide_sdk::overlay::modal(
        &paint,
        1.0,
        container(sheet).style(move |_| glide_sdk::glide_iced::style::popover(&paint)),
        Some(Message::CloseSettings),
        app.window.corner(),
    )
}

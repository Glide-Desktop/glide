use glide_ipc::keychain::{Conflict, Detail, Hidden, Keep};
use glide_sdk::theme::{SP1, SP2, SP3, SP4, TextStyle};
use glide_sdk::widget::{self as ui, DiffField};
use iced::widget::{Space, column, row};
use iced::{Alignment, Element, Fill};

use crate::app::{App, Message};
use crate::i18n::Key;
use crate::icons::Icon;

fn fields(app: &App, own: &Detail, other: &Detail, password: &Hidden, other_password: &Hidden) -> Vec<DiffField> {
    let t = |key: Key| key.text(app.lang);
    let mut fields = vec![DiffField {
        label: t(Key::Username).to_string(),
        value: own.summary.username.clone(),
        differs: own.summary.username != other.summary.username,
        extra: None,
    }];
    let differs = password.expose() != other_password.expose();
    fields.push(DiffField {
        label: if differs { t(Key::PasswordDiffers).to_string() } else { t(Key::Password).to_string() },
        value: password.expose().to_string(),
        differs,
        extra: crate::view::strength(glide_ipc::keychain::strength(password.expose())),
    });
    if !own.notes.is_empty() || !other.notes.is_empty() {
        let differs = own.notes != other.notes;
        fields.push(DiffField {
            label: if differs { t(Key::NotesDiffer).to_string() } else { t(Key::Notes).to_string() },
            value: own.notes.clone(),
            differs,
            extra: None,
        });
    }
    let sites = |detail: &Detail| detail.sites.iter().map(|site| site.uri.clone()).collect::<Vec<_>>().join(", ");
    fields.push(DiffField {
        label: t(Key::SitesApps).to_string(),
        value: sites(own),
        differs: sites(own) != sites(other),
        extra: None,
    });
    fields
}

#[must_use]
pub fn view(app: &App) -> Option<Element<'_, Message>> {
    let conflict: &Conflict = app.data.conflicts.first()?;
    let paint = app.paint;
    let t = |key: Key| key.text(app.lang);
    let date = |unix: u64| crate::i18n::date(app.lang, unix, app.now);
    let head = row![
        ui::tinted_tile(Icon::Warning, 40.0, paint.warn_soft(), paint.warn()),
        column![
            ui::text(t(Key::ConflictTitle).replace("{name}", &conflict.local.summary.name), TextStyle::T3),
            ui::text(t(Key::ConflictBody), TextStyle::CALL).color(paint.fg_2()),
        ]
        .spacing(SP1)
        .width(Fill),
    ]
    .spacing(SP3)
    .align_y(Alignment::Start);
    let newer = conflict.remote_modified >= conflict.local_modified;
    let local = ui::diff_card(
        &paint,
        t(Key::ThisDevice),
        date(conflict.local_modified),
        (!newer).then_some(t(Key::Newer)),
        fields(app, &conflict.local, &conflict.remote, &conflict.local_password, &conflict.remote_password),
        app.conflict_pick == 0,
        Message::ConflictPick(0),
    );
    let remote = ui::diff_card(
        &paint,
        t(Key::ServerVersion).replace("{vault}", app.vault_name(&conflict.item.vault)),
        date(conflict.remote_modified),
        newer.then_some(t(Key::Newer)),
        fields(app, &conflict.remote, &conflict.local, &conflict.remote_password, &conflict.local_password),
        app.conflict_pick == 1,
        Message::ConflictPick(1),
    );
    let keep = if app.conflict_pick == 0 { (Keep::Local, Key::KeepLocal) } else { (Keep::Remote, Key::KeepServer) };
    let buttons = row![
        ui::ghost(&paint, t(Key::DecideLater), Message::CloseConflict),
        Space::new().width(Fill),
        ui::secondary(&paint, t(Key::KeepBoth), Message::Resolve(conflict.item.clone(), Keep::Both)),
        ui::primary(&paint, t(keep.1), Message::Resolve(conflict.item.clone(), keep.0)),
    ]
    .spacing(SP2);
    let content = column![
        head,
        row![local, remote].spacing(SP3),
        ui::text(t(Key::HistoryKeeps), TextStyle::CAP).color(paint.fg_3()),
        buttons,
    ]
    .spacing(SP4);
    let _ = SP2;
    Some(crate::view::dialog_frame(app, 680.0, content, Message::CloseConflict))
}

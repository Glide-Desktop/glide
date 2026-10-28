use std::path::Path;
use std::time::SystemTime;

use glide_sdk::Tone;
use glide_sdk::widget::{INSPECTOR_PREVIEW, Inspector, SWATCH, count_badge, full_button, swatch};
use iced::widget::{Row, Stack, container};
use iced::{Element, Length};

use super::content::tag_color;
use super::dialogs::tilde;
use super::{file_icon, icon, paint};
use crate::app::{App, Message, volume_icon};
use crate::fs::{FileEntry, format_bytes, format_time, permissions_string, tags};
use crate::i18n::{self, Key};
use crate::icons::Icon;
use crate::theme::SP2;

const PREVIEW_ICON: f32 = 120.0;
const PLACE_GLYPH: f32 = 72.0;

#[must_use]
pub fn view(app: &App) -> Element<'_, Message> {
    let picked: Vec<&FileEntry> = app.selected_entries().collect();
    match picked.as_slice() {
        [] => place(app),
        [entry] => single(app, entry),
        many => several(app, many),
    }
}

fn swatches<'a>(app: &App, chosen: &[tags::Color]) -> Element<'a, Message> {
    let paint = paint(&app.theme);
    Row::with_children(tags::Color::ALL.into_iter().map(|color| {
        swatch(&paint, tag_color(color), chosen.contains(&color), Some(Message::ToggleTag(color)), SWATCH)
    }))
    .spacing(SP2)
    .into()
}

fn open_and_properties<'a>(app: &App, inspector: Inspector<'a, Message>) -> Inspector<'a, Message> {
    let paint = paint(&app.theme);
    let lang = app.lang;
    let open = (!app.in_trash()).then_some(Message::OpenSelection);
    inspector.action(full_button(&paint, Key::Open.text(lang), Tone::Accent, open)).action(full_button(
        &paint,
        Key::Properties.text(lang),
        Tone::Outline,
        Some(Message::Properties),
    ))
}

fn single<'a>(app: &'a App, entry: &'a FileEntry) -> Element<'a, Message> {
    let paint = paint(&app.theme);
    let lang = app.lang;
    let now = SystemTime::now();
    let kind = entry.category.name(lang);
    let subtitle =
        if entry.is_dir { kind.to_string() } else { format!("{kind} · {}", format_bytes(entry.size, lang)) };
    let mut inspector =
        Inspector::new(&paint, entry.name.clone()).preview(file_icon(app, entry, PREVIEW_ICON)).subtitle(subtitle);
    if !entry.is_dir {
        inspector = inspector.figure(Key::Size.text(lang), format_bytes(entry.size, lang));
    }
    if let Some(time) = entry.modified {
        inspector = inspector.figure(Key::ModifiedDate.text(lang), format_time(time, now, lang));
    }
    if let Some(time) = entry.created {
        inspector = inspector.figure(Key::CreatedDate.text(lang), format_time(time, now, lang));
    }
    if let Some(parent) = entry.path.parent() {
        inspector = inspector.figure(Key::Where.text(lang), tilde(parent));
    }
    inspector = inspector.figure(Key::Permissions.text(lang), permissions_string(entry.mode));
    if !app.in_trash() {
        inspector = inspector.section(Key::Tags.text(lang), swatches(app, &tags::colors(&entry.tags)));
    }
    open_and_properties(app, inspector).into()
}

fn several<'a>(app: &'a App, picked: &[&'a FileEntry]) -> Element<'a, Message> {
    let paint = paint(&app.theme);
    let lang = app.lang;
    let bytes: u64 = picked.iter().filter(|entry| !entry.is_dir).map(|entry| entry.size).sum();
    let folders = picked.iter().filter(|entry| entry.is_dir).count();
    let stack = Stack::with_children([
        file_icon(app, picked[0], PREVIEW_ICON),
        container(count_badge(&paint, u32::try_from(picked.len()).unwrap_or(u32::MAX)))
            .align_right(Length::Fixed(PREVIEW_ICON))
            .into(),
    ]);
    let mut inspector = Inspector::new(&paint, i18n::items(picked.len(), lang))
        .preview(stack)
        .subtitle(format_bytes(bytes, lang))
        .figure(Key::Size.text(lang), format_bytes(bytes, lang));
    if folders > 0 {
        inspector = inspector.figure(Key::Folders.text(lang), folders.to_string());
    }
    if !app.in_trash() {
        let shared: Vec<tags::Color> = tags::Color::ALL
            .into_iter()
            .filter(|color| picked.iter().all(|entry| tags::colors(&entry.tags).contains(color)))
            .collect();
        inspector = inspector.section(Key::Tags.text(lang), swatches(app, &shared));
    }
    open_and_properties(app, inspector).into()
}

fn place_glyph<'a>(app: &App, glyph: Icon) -> Element<'a, Message> {
    container(icon(glyph, PLACE_GLYPH, app.theme.fg_2())).center(Length::Fixed(INSPECTOR_PREVIEW)).into()
}

fn place(app: &App) -> Element<'_, Message> {
    let paint = paint(&app.theme);
    let lang = app.lang;
    let current: &Path = &app.current;
    let title = app.location_name(current);
    let count = app.entries.len();
    if app.in_trash() {
        let empty = (app.trash_count > 0).then_some(Message::EmptyTrash);
        return Inspector::new(&paint, title)
            .preview(place_glyph(app, Icon::Trash))
            .subtitle(i18n::in_trash(app.trash_count, lang))
            .figure(Key::Where.text(lang), tilde(&app.trash.files()))
            .action(full_button(&paint, Key::EmptyTrash.text(lang), Tone::Danger, empty))
            .into();
    }
    let volume = app.volumes.iter().find(|volume| volume.mount.as_deref() == Some(current));
    let glyph = volume
        .map(volume_icon)
        .or_else(|| app.places.iter().chain(&app.drives).find(|place| place.path == current).map(|place| place.icon));
    let subtitle = if app.is_searching() {
        super::status::summary(app)
    } else {
        format!("{} · {}", Key::Folder.text(lang), i18n::items(count, lang))
    };
    let mut inspector = Inspector::new(&paint, title)
        .preview(place_glyph(app, glyph.unwrap_or(Icon::Folder)))
        .subtitle(subtitle)
        .figure(Key::Where.text(lang), tilde(current));
    if let Some(free) = app.free_bytes {
        inspector = inspector.figure(Key::FreeSpace.text(lang), format_bytes(free, lang));
    }
    if let Some(volume) = volume {
        inspector = inspector
            .fact(Key::FileSystem.text(lang), volume.fs_type.clone())
            .figure(Key::Capacity.text(lang), format_bytes(volume.size, lang));
        if volume.can_remove() {
            let eject = Some(Message::Eject(volume.object.clone()));
            inspector = inspector.action(full_button(&paint, Key::Eject.text(lang), Tone::Outline, eject));
        }
        return inspector.into();
    }
    let properties = Some(Message::PropertiesOf(current.to_path_buf()));
    inspector.action(full_button(&paint, Key::FolderProperties.text(lang), Tone::Outline, properties)).into()
}

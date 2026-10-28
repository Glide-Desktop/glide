use glide_sdk::theme::TextStyle;
use glide_sdk::widget::{Wrapping, text, zoom_control};
use iced::Element;

use super::STATUS_HEIGHT;
use crate::app::{App, Message};
use crate::fs::format_bytes;
use crate::i18n::{self, Key};
use crate::theme;

#[must_use]
pub fn summary(app: &App) -> String {
    let lang = app.lang;
    if app.search.as_ref().is_some_and(|search| search.pending) {
        return Key::Searching.text(lang).to_string();
    }
    let selected = app.selected_entries().count();
    if selected == 0 {
        return match app.is_searching() {
            true if app.entries.is_empty() => Key::NoSearchResults.text(lang).to_string(),
            true => i18n::found(app.entries.len(), lang),
            false if app.in_trash() => i18n::in_trash(app.entries.len(), lang),
            false => i18n::items(app.entries.len(), lang),
        };
    }
    let bytes: u64 = app.selected_entries().filter(|entry| !entry.is_dir).map(|entry| entry.size).sum();
    let count = i18n::selected(selected, lang);
    if bytes > 0 { format!("{count} · {}", format_bytes(bytes, lang)) } else { count }
}

#[must_use]
pub fn free(app: &App) -> Option<String> {
    let free = app.free_bytes.filter(|_| !app.in_trash())?;
    Some(i18n::free_space(&format_bytes(free, app.lang), app.lang))
}

#[must_use]
pub fn view(app: &App) -> Element<'_, Message> {
    let t = &app.theme;
    let paint = super::paint(t);
    let lang = app.lang;
    let corners = theme::window_corners(theme::Side::BottomRight, app.corner());
    let (level, levels) = app.zoom_level();
    let mut right: Vec<Element<'_, Message>> = Vec::new();
    if let Some(free) = free(app) {
        let style = TextStyle { size_px: 12.0, line_height_px: 16.0, ..TextStyle::NUM };
        right.push(text(free, style).color(t.fg_2()).wrapping(Wrapping::None).into());
        right.push(iced::widget::space().width(theme::SP2).into());
    }
    right.push(zoom_control(
        &paint,
        level,
        levels,
        |level| Message::SetZoom(u8::try_from(level).unwrap_or(0)),
        (Key::ZoomOut.text(lang), Key::ZoomIn.text(lang)),
    ));
    glide_sdk::widget::status_strip(
        super::header::bar_background(app),
        STATUS_HEIGHT,
        vec![text(summary(app), TextStyle::CAP).color(t.fg_2()).wrapping(Wrapping::None).into()],
        right,
        corners,
    )
}

use std::path::{Path, PathBuf};

use iced::widget::{Row, button, container, mouse_area, row, space, text};
use iced::{Alignment, Element, Fill, Padding};

use super::{PATHBAR_HEIGHT, icon};
use crate::app::{App, Hover, MenuKind, Message};
use crate::fs::{format_bytes, home_dir};
use crate::i18n::{self, Key};
use crate::icons::Icon;
use crate::theme;

const STATUS_BUDGET: f32 = 240.0;
const CHAR_WIDTH: f32 = 6.8;
const CRUMB_CHROME: f32 = 40.0;

struct Crumb {
    glyph: Icon,
    label: String,
    path: PathBuf,
}

fn crumbs(app: &App) -> Vec<Crumb> {
    let current = &app.current;
    let trash = app.trash.files();
    let (glyph, label, root) = if current.starts_with(&trash) {
        (Icon::Trash, Key::Trash.text(app.lang).to_string(), trash)
    } else if let Some(home) = home_dir().filter(|home| current.starts_with(home)) {
        (Icon::Home, Key::Home.text(app.lang).to_string(), home)
    } else {
        (Icon::Drive, Key::Root.text(app.lang).to_string(), PathBuf::from("/"))
    };
    let mut path = root.clone();
    let mut crumbs = vec![Crumb { glyph, label, path: root.clone() }];
    for component in current.strip_prefix(&root).unwrap_or(Path::new("")) {
        path.push(component);
        crumbs.push(Crumb { glyph: Icon::Folder, label: component.to_string_lossy().into_owned(), path: path.clone() });
    }
    crumbs
}

fn width(crumb: &Crumb) -> f32 {
    crumb.label.chars().count() as f32 * CHAR_WIDTH + CRUMB_CHROME
}

fn status(app: &App) -> String {
    let lang = app.lang;
    if app.search.as_ref().is_some_and(|search| search.pending) {
        return Key::Searching.text(lang).to_string();
    }
    let mut parts = vec![if app.selection.is_empty() {
        i18n::items(app.entries.len(), lang)
    } else {
        i18n::selected(app.selection.len(), app.entries.len(), lang)
    }];
    let bytes: u64 = app.selected_entries().filter(|entry| !entry.is_dir).map(|entry| entry.size).sum();
    if bytes > 0 {
        parts.push(format_bytes(bytes, lang));
    }
    if let Some(free) = app.free_bytes {
        parts.push(format!("{} {}", format_bytes(free, lang), Key::Free.text(lang)));
    }
    parts.join(" · ")
}

#[must_use]
pub fn view(app: &App) -> Element<'_, Message> {
    let t = &app.theme;
    let mut crumbs = crumbs(app);
    let budget = app.pane.width - STATUS_BUDGET;
    while crumbs.len() > 2 && crumbs.iter().map(width).sum::<f32>() > budget {
        crumbs.remove(1);
    }

    let last = crumbs.len() - 1;
    let mut trail = Row::new().align_y(Alignment::Center);
    for (index, crumb) in crumbs.into_iter().enumerate() {
        let current = index == last;
        if index > 0 {
            trail = trail.push(icon(Icon::ChevronRight, 10.0, t.fg_3()));
        }
        let tint = if crumb.glyph == Icon::Folder { t.acc_bg() } else { t.fg_2() };
        let key = Hover::Crumb(crumb.path.clone());
        let hover = if app.is_drop_target(&crumb.path) { 1.0 } else { app.hover(&key) };
        let target = MenuKind::Location(crumb.path.clone());
        trail = trail.push(
            mouse_area(
                button(
                    row![icon(crumb.glyph, 13.0, tint), text(crumb.label).size(12).wrapping(text::Wrapping::None),]
                        .spacing(5)
                        .align_y(Alignment::Center),
                )
                .on_press(Message::Navigate(crumb.path))
                .padding(Padding::from([2, 6]))
                .style(move |_, status| theme::crumb(t, current, hover, status)),
            )
            .on_right_press(Message::MenuTarget(target))
            .on_enter(Message::Hover(key.clone(), true))
            .on_exit(Message::Hover(key, false)),
        );
    }

    container(
        row![
            container(trail).width(Fill).clip(true),
            space().width(12),
            text(status(app)).size(12).color(t.fg_2()).wrapping(text::Wrapping::None),
        ]
        .align_y(Alignment::Center),
    )
    .padding(Padding::from([0, 10]))
    .width(Fill)
    .height(PATHBAR_HEIGHT)
    .align_y(Alignment::Center)
    .style(move |_| theme::rounded(t.bar_bg(), theme::window_corners(theme::Side::BottomRight)))
    .into()
}

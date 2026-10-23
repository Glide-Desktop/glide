use std::path::{Path, PathBuf};

use glide_sdk::theme::TextStyle;
use glide_sdk::widget::{Wrapping, elide, measure, text};
use iced::Element;
use iced::widget::mouse_area;

use super::PATHBAR_HEIGHT;
use crate::app::{App, Hover, MenuKind, Message};
use crate::fs::{format_bytes, home_dir};
use crate::i18n::{self, Key};
use crate::icons::Icon;
use crate::theme;

const CRUMB_CHROME: f32 = 44.0;
const CRUMB_LABEL: f32 = 200.0;

pub struct Crumb {
    pub glyph: Icon,
    pub label: String,
    pub path: PathBuf,
}

#[must_use]
pub fn trail(app: &App) -> Vec<Crumb> {
    let mut crumbs = crumbs(app);
    let status = measure(&status(app), TextStyle::CAP);
    let budget = app.pane.width - theme::SP2 - 2.0 * theme::SP3 - status;
    if crumbs.iter().map(width).sum::<f32>() > budget {
        let mut hidden = None;
        while crumbs.len() > 2 && CRUMB_CHROME + crumbs.iter().map(width).sum::<f32>() > budget {
            hidden = Some(crumbs.remove(1).path);
        }
        if let Some(path) = hidden {
            crumbs.insert(1, Crumb { glyph: Icon::More, label: String::new(), path });
        }
    }
    crumbs
}

fn crumbs(app: &App) -> Vec<Crumb> {
    let current = &app.current;
    let trash = current.ancestors().find(|dir| app.trash.is_files_dir(dir)).map(Path::to_path_buf);
    let (glyph, label, root, first) = if let Some(files) = trash {
        (Icon::Trash, Key::Trash.text(app.lang).to_string(), files, app.trash.files())
    } else if let Some(home) = home_dir().filter(|home| current.starts_with(home)) {
        (Icon::Home, Key::Home.text(app.lang).to_string(), home.clone(), home)
    } else {
        (Icon::Drive, Key::Root.text(app.lang).to_string(), PathBuf::from("/"), PathBuf::from("/"))
    };
    let mut path = root.clone();
    let mut crumbs = vec![Crumb { glyph, label, path: first }];
    for component in current.strip_prefix(&root).unwrap_or(Path::new("")) {
        path.push(component);
        let label = elide(&app.location_name(&path), CRUMB_LABEL, TextStyle::CALL);
        crumbs.push(Crumb { glyph: Icon::Folder, label, path: path.clone() });
    }
    crumbs
}

fn width(crumb: &Crumb) -> f32 {
    measure(&crumb.label, TextStyle { weight: 600, ..TextStyle::CALL }) + CRUMB_CHROME
}

#[must_use]
pub fn status(app: &App) -> String {
    let lang = app.lang;
    if app.search.as_ref().is_some_and(|search| search.pending) {
        return Key::Searching.text(lang).to_string();
    }
    let selected = app.selected_entries().count();
    let mut parts = vec![if selected == 0 {
        i18n::items(app.entries.len(), lang)
    } else {
        i18n::selected(selected, app.entries.len(), lang)
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
    let corners = theme::window_corners(theme::Side::BottomRight, app.corner());
    glide_sdk::widget::status_strip(
        super::header::bar_background(app),
        PATHBAR_HEIGHT,
        vec![crumbs_row(app)],
        vec![text(status(app), TextStyle::CAP).color(t.fg_2()).wrapping(Wrapping::None).into()],
        corners,
    )
}

#[must_use]
pub fn crumbs_row(app: &App) -> Element<'_, Message> {
    let t = &app.theme;
    let paint = super::paint(t);
    let crumbs = trail(app);
    let last = crumbs.len() - 1;
    let items = crumbs.into_iter().enumerate().map(|(index, crumb)| {
        let tint = if crumb.glyph == Icon::Folder { t.acc_bg() } else { t.fg_2() };
        let key = Hover::Crumb(crumb.path.clone());
        let target = MenuKind::Location(crumb.path.clone());
        let face = glide_sdk::widget::Crumb::new(&paint, crumb.label)
            .glyph(crumb.glyph, tint)
            .current(index == last)
            .hovered(app.is_drop_target(&crumb.path))
            .on_press(Some(Message::Navigate(crumb.path)));
        mouse_area(Element::from(face))
            .on_right_press(Message::MenuTarget(target))
            .on_enter(Message::Hover(key.clone(), true))
            .on_exit(Message::Hover(key, false))
            .into()
    });
    glide_sdk::widget::crumb_trail(&paint, items.collect())
}

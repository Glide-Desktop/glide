mod library;
mod organizer;
mod present;
mod reader;
mod side;
mod states;

use glide_sdk::Paint;
use glide_sdk::window;
use iced::widget::stack;
use iced::{Color, Element};

use crate::app::{App, Message, Screen};
use crate::doc::{Format, Hue};
use crate::store::Paper;

pub const SIDEBAR: f32 = 232.0;
pub const PAGES_ID: &str = "glide-folio-pages";
pub const THUMBS_ID: &str = "glide-folio-thumbs";
pub const FIND_ID: &str = "glide-folio-find";
pub const DRAFT_ID: &str = "glide-folio-draft";
pub const PASSWORD_ID: &str = "glide-folio-password";
pub const PAGE_FIELD_ID: &str = "glide-folio-page";
pub const SEARCH_ID: &str = "glide-folio-search";

#[must_use]
pub fn hue_color(hue: Hue) -> Color {
    let [r, g, b] = hue.rgb();
    Color::from_rgb8(r, g, b)
}

#[must_use]
pub fn paper(paper: Paper) -> glide_sdk::widget::Paper {
    match paper {
        Paper::White => glide_sdk::widget::Paper { face: Color::WHITE, ink: Color::from_rgb8(0x1b, 0x1a, 0x17) },
        Paper::Sepia => glide_sdk::widget::Paper {
            face: Color::from_rgb8(0xf6, 0xef, 0xdf),
            ink: Color::from_rgb8(0x3b, 0x30, 0x22),
        },
        Paper::Night => glide_sdk::widget::Paper {
            face: Color::from_rgb8(0x1d, 0x1c, 0x20),
            ink: Color::from_rgb8(0xde, 0xdb, 0xd5),
        },
    }
}

#[must_use]
pub fn cover_colors(format: Format, name: &str) -> (Color, Color) {
    let seed = name.bytes().fold(7u32, |h, b| h.wrapping_mul(31).wrapping_add(u32::from(b)));
    let ramps = [
        (Color::WHITE, Color::from_rgb8(0x16, 0x3e, 0x79)),
        (Color::from_rgb8(0x2d, 0x72, 0x29), Color::from_rgb8(0xf2, 0xf0, 0xed)),
        (Color::from_rgb8(0xfa, 0xec, 0xd2), Color::from_rgb8(0x63, 0x43, 0x0a)),
        (Color::from_rgb8(0x16, 0x3e, 0x79), Color::from_rgb8(0xfa, 0xec, 0xd2)),
        (Color::from_rgb8(0x26, 0x25, 0x1f), Color::from_rgb8(0xf7, 0xf6, 0xf3)),
        (Color::from_rgb8(0xe5, 0xe2, 0xdb), Color::from_rgb8(0x35, 0x31, 0x2c)),
    ];
    match format {
        Format::Pdf | Format::Office | Format::Xps | Format::PostScript => {
            (Color::WHITE, ramps[(seed % 2) as usize * 4 % 6].1)
        }
        _ => ramps[(seed % ramps.len() as u32) as usize],
    }
}

#[must_use]
pub fn ink(app: &App) -> Color {
    if app.window.focused { app.paint.fg_1() } else { app.paint.fg_3() }
}

#[must_use]
pub fn view(app: &App) -> Element<'_, Message> {
    let base: Element<'_, Message> = match &app.screen {
        Screen::Library => library::view(app),
        Screen::Loading(path) => states::loading(app, path),
        Screen::Locked { .. } => states::locked(app),
        Screen::Failed { path, error } => states::failed(app, path, error),
        Screen::Reading => match &app.doc {
            Some(doc) if doc.present.is_some() => present::view(app, doc),
            Some(doc) if doc.bare => present::bare(app, doc),
            Some(doc) if doc.organizer.is_some() => organizer::view(app, doc),
            Some(doc) => reader::view(app, doc),
            None => library::view(app),
        },
    };
    let mut layers = vec![base];
    if let (Some(tab), Some(doc)) = (app.props, app.doc.as_ref()) {
        layers.push(states::properties(app, doc, tab));
    }
    if !app.window.edge_to_edge() {
        layers.push(window::resize_handles(Message::Window));
    }
    stack(layers).into()
}

#[must_use]
pub fn paint_of(app: &App) -> Paint {
    app.paint
}

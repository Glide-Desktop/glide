pub mod edit;
pub mod export;
pub mod player;
pub mod states;
pub mod welcome;

use glide_sdk::theme::TextStyle;
use glide_sdk::widget::{self as ui};
use glide_sdk::{Dialog, Tone};
use iced::widget::{column, stack, text_input};
use iced::{Element, Rectangle, Size};

use crate::app::{App, Message};
use crate::i18n::Key;

pub const ADDRESS_FIELD: &str = "glide-clips-address";
pub const HUD_WIDTH: f32 = 640.0;
pub const HUD_HEIGHT: f32 = 82.0;
pub const HUD_BOTTOM: f32 = 24.0;

#[must_use]
pub fn hud_rect(size: Size) -> Rectangle {
    let width = HUD_WIDTH.min((size.width - 32.0).max(320.0));
    Rectangle {
        x: ((size.width - width) / 2.0).round(),
        y: size.height - HUD_BOTTOM - HUD_HEIGHT,
        width,
        height: HUD_HEIGHT,
    }
}

#[must_use]
pub fn view(app: &App) -> Element<'_, Message> {
    let content = match &app.media {
        None => welcome::view(app),
        Some(media) => player::view(app, media),
    };
    let mut layers = vec![content];
    if let Some(form) = export::layer(app) {
        layers.push(form);
    }
    if let Some(address) = &app.address {
        layers.push(address_dialog(app, address));
    }
    if !app.window.edge_to_edge() {
        layers.push(glide_sdk::window::resize_handles(Message::Window));
    }
    stack(layers).into()
}

fn address_dialog<'a>(app: &'a App, address: &str) -> Element<'a, Message> {
    let paint = app.paint;
    let input = text_input(app.t(Key::AddressPlaceholder), address)
        .id(ADDRESS_FIELD)
        .on_input(Message::AddressEdited)
        .on_submit(Message::AddressConfirm);
    let body = column![
        ui::text(app.t(Key::AddressBody), TextStyle::CALL).color(paint.fg_2()).wrapping(ui::Wrapping::WordOrGlyph),
        ui::field(&paint, input),
    ]
    .spacing(12);
    let buttons = glide_sdk::overlay::dialog_buttons(
        &paint,
        vec![
            (app.t(Key::Cancel), Tone::Neutral, Some(Message::AddressCancel)),
            (app.t(Key::Choose), Tone::Accent, (!address.trim().is_empty()).then_some(Message::AddressConfirm)),
        ],
        None,
    );
    let card = glide_sdk::overlay::dialog_card(
        &paint,
        app.t(Key::AddressTitle),
        Some(body.into()),
        buttons,
        460.0,
        app.window.size,
    );
    glide_sdk::overlay::modal(&paint, 1.0, card, Some(Message::AddressCancel), app.window.corner())
}

#[must_use]
pub fn shortcuts(app: &App) -> Dialog<Message> {
    let rows = [
        ("Пробіл, K", "Space, K", Key::KeyPlay),
        ("J / L", "J / L", Key::KeyShuttle),
        ("← / →", "← / →", Key::KeySkip),
        ("↑ / ↓", "↑ / ↓", Key::KeyVolume),
        ("F", "F", Key::KeyFullscreen),
        ("C", "C", Key::KeyTracks),
        ("[ / ]", "[ / ]", Key::KeySpeed),
        (", / .", ", / .", Key::KeyChapters),
        ("Ctrl+T, Ctrl+Y, Ctrl+E", "Ctrl+T, Ctrl+Y, Ctrl+E", Key::KeyEdit),
        ("Ctrl+I, Ctrl+Shift+P", "Ctrl+I, Ctrl+Shift+P", Key::KeyInspector),
        ("Ctrl+O, Ctrl+L", "Ctrl+O, Ctrl+L", Key::KeyOpen),
    ];
    let body = rows
        .iter()
        .map(|(uk, en, key)| format!("{} — {}", app.lang.pick(uk, en), app.t(*key)))
        .collect::<Vec<_>>()
        .join("\n");
    Dialog::new(app.t(Key::ShortcutsTitle), body).cancel(app.t(Key::Close))
}

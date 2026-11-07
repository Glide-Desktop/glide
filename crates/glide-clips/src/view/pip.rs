use glide_sdk::anim::{Speed, animate};
use glide_sdk::widget::player::{glass_disc, hidden_cursor, stage};
use glide_sdk::widget::{self as ui};
use iced::widget::{Space, column, container, row, stack};
use iced::{Alignment, Background, Color, Element, Fill, Padding, Vector};

use crate::app::{App, Media, Message};
use crate::i18n::Key;
use crate::icons::Icon;
use crate::view::player::{play_disc, skip_button};

pub const LINE: f32 = 3.0;
const TOP_TRAVEL: f32 = 48.0;
const MID_SCALE: f32 = 0.9;

fn shade<'a>(value: glide_sdk::anim::Value) -> Element<'a, Message> {
    container(Space::new().width(Fill).height(Fill))
        .style(move |_| {
            let strength = value.get();
            container::Style {
                background: Some(Background::Gradient(iced::Gradient::Linear(
                    iced::gradient::Linear::new(std::f32::consts::PI)
                        .add_stop(0.0, Color { a: 0.45 * strength, ..Color::BLACK })
                        .add_stop(0.4, Color::TRANSPARENT)
                        .add_stop(0.7, Color::TRANSPARENT)
                        .add_stop(1.0, Color { a: 0.4 * strength, ..Color::BLACK }),
                ))),
                ..container::Style::default()
            }
        })
        .into()
}

fn progress<'a>(media: &Media, width: f32) -> Element<'a, Message> {
    let filled = (width * media.fraction()).round();
    container(container(Space::new().width(filled).height(LINE)).style(|_| container::Style {
        background: Some(Background::Color(crate::view::player::ink())),
        ..container::Style::default()
    }))
    .width(Fill)
    .height(LINE)
    .style(|_| container::Style {
        background: Some(Background::Color(Color { a: 0.25, ..Color::WHITE })),
        ..container::Style::default()
    })
    .into()
}

fn controls<'a>(app: &'a App, media: &'a Media) -> Element<'a, Message> {
    let playing = media.playing();
    let back = ui::tooltip(
        &glide_sdk::Paint::default(),
        glass_disc(Icon::Pip, 28.0, false, Some(Message::Pip)),
        app.t(Key::BackToWindow),
    );
    let close = ui::tooltip(
        &glide_sdk::Paint::default(),
        glass_disc(Icon::Close, 28.0, true, Some(Message::Close)),
        app.t(Key::CloseWindow),
    );
    let top = row![back, Space::new().width(Fill), close].padding(8);
    let middle = row![
        skip_button(true, app.t(Key::Back10)),
        play_disc(playing, app.t(if playing { Key::Pause } else { Key::Play })),
        skip_button(false, app.t(Key::Forward10)),
    ]
    .spacing(14)
    .align_y(Alignment::Center);
    let target = if app.chrome.shown { 1.0 } else { 0.0 };
    let top = animate(target, Speed::SLIDE, move |_| top).offset(|value| Vector::new(0.0, -TOP_TRAVEL * (1.0 - value)));
    let middle = animate(target, Speed::OVERSHOOT, move |_| container(middle).center(Fill))
        .scale(|value| (MID_SCALE * value + (1.0 - MID_SCALE) * value * value).max(0.001));
    let shade = animate(target, Speed::FADE, |value| shade(value.clone()));
    stack![shade, column![top, Space::new().height(Fill)], middle].width(Fill).height(Fill).into()
}

#[must_use]
pub fn view<'a>(app: &'a App, media: &'a Media) -> Element<'a, Message> {
    let width = app.window.size.width;
    let content = stack![
        Space::new().width(Fill).height(Fill),
        controls(app, media),
        container(progress(media, width)).height(Fill).align_y(Alignment::End),
    ]
    .width(Fill)
    .height(Fill);
    let pane = container(content).width(Fill).height(Fill).clip(true).padding(Padding::ZERO);
    let pane = hidden_cursor(!app.chrome.shown, pane);
    stage(pane)
        .on_drag(Message::WindowDrag)
        .on_double_click(Message::Pip)
        .on_move((!app.chrome.shown).then_some(Message::Wake))
        .on_leave(Message::Leave)
        .into()
}

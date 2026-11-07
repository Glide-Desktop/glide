use glide_sdk::anim::{Speed, Transform, hover};
use glide_sdk::theme::TextStyle;
use glide_sdk::widget::player::stage;
use glide_sdk::widget::{self as ui, Size};
use glide_sdk::window::Action;
use glide_sdk::{Tone, theme};
use iced::widget::{Space, column, container, image, mouse_area, row, stack};
use iced::{Alignment, Background, Border, Color, Element, Fill, Padding, Shadow, Vector};

use crate::app::{App, Message, Recent};
use crate::i18n::{self, Key};
use crate::icons::Icon;

const CARD_GAP: f32 = 20.0;
const SIDE: f32 = 120.0;
const TRACK_INSET: f32 = 3.0;

fn mark<'a>(app: &App) -> Element<'a, Message> {
    let paint = app.paint;
    let accent = paint.acc_bg();
    container(ui::icon(Icon::Mark, 30.0, Color::WHITE))
        .center(64.0)
        .style(move |_| container::Style {
            background: Some(Background::Color(accent)),
            border: Border { radius: theme::R3.into(), ..Border::default() },
            shadow: Shadow {
                color: Color { a: 0.18, ..Color::BLACK },
                offset: Vector::new(0.0, 6.0),
                blur_radius: 16.0,
            },
            ..container::Style::default()
        })
        .into()
}

fn card<'a>(app: &'a App, recent: &'a Recent, width: f32) -> Element<'a, Message> {
    let paint = app.paint;
    let height = (width * 9.0 / 16.0).round();
    let picture: Element<'a, Message> = match &recent.cover {
        Some(handle) => image(handle.clone())
            .width(width)
            .height(height)
            .content_fit(iced::ContentFit::Cover)
            .border_radius(theme::R2)
            .into(),
        None => container(ui::icon(Icon::Mark, 22.0, paint.fg_3()))
            .center_x(width)
            .center_y(height)
            .style(move |_| container::Style {
                background: Some(Background::Color(paint.sunken_bg())),
                border: Border { radius: theme::R2.into(), ..Border::default() },
                ..container::Style::default()
            })
            .into(),
    };
    let progress = recent.entry.progress();
    let duration = glide_sdk::widget::player::clock(std::time::Duration::from_secs_f64(recent.entry.duration), false);
    let badge = container(
        ui::text(duration, TextStyle { size_px: 11.0, line_height_px: 14.0, ..TextStyle::NUM }).color(Color::WHITE),
    )
    .padding([1.0, 5.0])
    .style(|_| container::Style {
        background: Some(Background::Color(Color { a: 0.62, ..Color::BLACK })),
        border: Border { radius: 4.0.into(), ..Border::default() },
        ..container::Style::default()
    });
    let inset = TRACK_INSET;
    let span = (width - 2.0 * inset).max(0.0);
    let filled = (span * progress.clamp(0.0, 1.0)).round();
    let track = container(container(Space::new().width(filled).height(3)).style(move |_| container::Style {
        background: Some(Background::Color(paint.acc_bg())),
        border: Border { radius: iced::border::Radius::default().bottom_left(1.5), ..Border::default() },
        ..container::Style::default()
    }))
    .width(span)
    .height(3)
    .style(|_| container::Style {
        background: Some(Background::Color(Color { a: 0.3, ..Color::WHITE })),
        border: Border { radius: iced::border::Radius::default().bottom(1.5), ..Border::default() },
        ..container::Style::default()
    });
    let track = container(track).padding(Padding::from([0.0, inset]));
    let frame = stack![
        picture,
        container(badge)
            .width(width)
            .height(height)
            .align_x(Alignment::End)
            .align_y(Alignment::End)
            .padding(Padding::from([0.0, 6.0]).bottom(8.0)),
        container(track).width(width).height(height).align_y(Alignment::End),
    ];
    let ring = container(Space::new().width(width).height(height)).style(move |_| container::Style {
        border: Border { radius: theme::R2.into(), color: paint.line(), width: 1.0 },
        ..container::Style::default()
    });
    let frame = container(stack![frame, ring]).width(width).height(height).style(move |_| container::Style {
        border: Border { radius: theme::R2.into(), ..Border::default() },
        shadow: Shadow { color: Color { a: 0.07, ..Color::BLACK }, offset: Vector::new(0.0, 1.0), blur_radius: 2.0 },
        ..container::Style::default()
    });
    let lifted = hover(Speed::OVERSHOOT, move |_| frame)
        .transform(|value| Transform { scale: 1.0 + 0.03 * value, offset: Vector::new(0.0, -4.0 * value) });
    let name = recent.path.file_name().map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    let meta = format!(
        "{} · {}",
        i18n::left(app.lang, recent.entry.left()),
        i18n::when(app.lang, recent.entry.opened, crate::memory::now())
    );
    let content = column![
        lifted,
        ui::text(name, TextStyle { size_px: 13.0, line_height_px: 17.0, weight: 500, ..TextStyle::CALL })
            .wrapping(ui::Wrapping::None),
        ui::text(meta, TextStyle::CAP).color(paint.fg_2()).wrapping(ui::Wrapping::None),
    ]
    .spacing(8)
    .width(width);
    mouse_area(content)
        .on_press(Message::OpenRecent(recent.path.clone()))
        .interaction(iced::mouse::Interaction::Pointer)
        .into()
}

#[must_use]
pub fn view(app: &App) -> Element<'_, Message> {
    let paint = app.paint;
    let window = app.window;
    let width = window.size.width;
    let lead = column![
        mark(app),
        container(ui::text(app.t(Key::OpenVideo), TextStyle::T1).color(paint.fg_1()))
            .padding(Padding::default().top(8)),
        container(
            ui::text(app.t(Key::OpenHint), TextStyle::CALL)
                .color(paint.fg_2())
                .wrapping(ui::Wrapping::WordOrGlyph)
                .align_x(Alignment::Center)
        )
        .max_width(520),
        container(
            row![
                ui::glyph_button(
                    &paint,
                    Icon::Folder,
                    app.t(Key::OpenFile),
                    Tone::Accent,
                    Size::Medium,
                    Some(Message::Open)
                ),
                ui::glyph_button(
                    &paint,
                    Icon::Link,
                    app.t(Key::OpenAddress),
                    Tone::Outline,
                    Size::Medium,
                    Some(Message::AddressDialog)
                ),
            ]
            .spacing(8)
        )
        .padding(Padding::default().top(12)),
    ]
    .spacing(12)
    .align_x(Alignment::Center);
    let mut body = column![Space::new().height(72), container(lead).center_x(Fill)].width(Fill);
    if !app.recents.is_empty() {
        let usable = (width - 2.0 * SIDE).clamp(320.0, 1040.0);
        let card_width = ((usable - CARD_GAP * 3.0) / 4.0).floor();
        let cards = app
            .recents
            .iter()
            .take(4)
            .fold(row![].spacing(CARD_GAP), |line, recent| line.push(card(app, recent, card_width)));
        let heading = row![
            ui::text(app.t(Key::ContinueWatching), TextStyle::HEAD).color(paint.fg_1()),
            Space::new().width(Fill),
            ui::text(app.t(Key::MemoryNote), TextStyle::CAP).color(paint.fg_2()),
        ]
        .align_y(Alignment::Center)
        .width(usable);
        body = body.push(Space::new().height(12)).push(container(column![heading, cards].spacing(18)).center_x(Fill));
    }
    let foot = row![
        ui::text(app.t(Key::PhotosNote), TextStyle::CAP).color(paint.fg_2()),
        container(Space::new().width(1).height(14)).style(move |_| container::Style {
            background: Some(Background::Color(paint.line_2())),
            ..container::Style::default()
        }),
        ui::glyph_button(
            &paint,
            Icon::Trash,
            app.t(Key::ForgetHistory),
            Tone::Ghost,
            Size::Small,
            (!app.recents.is_empty()).then_some(Message::ForgetHistory)
        ),
    ]
    .spacing(16)
    .align_y(Alignment::Center);
    let page = column![body.height(Fill), container(foot).center_x(Fill).padding(Padding::from([0.0, 0.0]).bottom(28))];
    let controls = row![
        glide_sdk::window::maximize_button(
            &paint,
            &window,
            app.t(Key::Maximize),
            Message::Window(Action::ToggleMaximize)
        ),
        Space::new().width(Fill),
        glide_sdk::window::close_button(&paint, &window, app.t(Key::CloseWindow), Message::Close),
    ]
    .padding(Padding::from([11.0, 12.0]));
    let background = paint.view_bg();
    let corner = window.corner();
    let surface = ui::pane(background, corner, stack![page, controls]);
    let surface = stage(surface).on_drag(Message::WindowDrag).on_double_click(Message::Window(Action::ToggleMaximize));
    let mut layers: Vec<Element<'_, Message>> = vec![surface.into()];
    if app.dropping {
        layers.push(drop_overlay(app));
    }
    stack(layers).into()
}

fn drop_overlay(app: &App) -> Element<'_, Message> {
    let paint = app.paint;
    let content = column![
        mark(app),
        ui::text(app.t(Key::DropToPlay), TextStyle::T3).color(paint.acc_soft_fg()),
        ui::text(app.t(Key::DropAspect), TextStyle::CAP).color(paint.fg_2()),
    ]
    .spacing(10)
    .align_x(Alignment::Center);
    let ring = paint.acc_bg();
    let soft = Color { a: 0.88 * paint.acc_soft().a, ..paint.acc_soft() };
    container(container(content).center(Fill).style(move |_| container::Style {
        background: Some(Background::Color(soft)),
        border: Border { radius: theme::R3.into(), color: ring, width: 2.0 },
        ..container::Style::default()
    }))
    .padding(12)
    .into()
}

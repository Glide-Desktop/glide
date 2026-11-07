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
const NARROW_SIDE: f32 = 24.0;
const CARD_LEAST: f32 = 168.0;
const ROOMY_HEIGHT: f32 = 640.0;
const HINT_HEIGHT: f32 = 480.0;

fn mark<'a>(app: &App) -> Element<'a, Message> {
    mark_with(&app.paint, Icon::Mark, 30.0)
}

fn mark_with<'a>(paint: &glide_sdk::Paint, glyph: Icon, size: f32) -> Element<'a, Message> {
    let accent = paint.acc_bg();
    container(ui::icon(glyph, size, Color::WHITE))
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

fn hero(app: &App) -> Element<'_, Message> {
    let paint = app.paint;
    let window = app.window;
    let hint: Element<'_, Message> = if window.size.height >= HINT_HEIGHT {
        container(
            ui::text(app.t(Key::OpenHint), TextStyle::CALL)
                .color(paint.fg_2())
                .wrapping(ui::Wrapping::WordOrGlyph)
                .align_x(Alignment::Center),
        )
        .max_width(520)
        .into()
    } else {
        Space::new().into()
    };
    let lead = column![
        mark(app),
        container(ui::text(app.t(Key::OpenVideo), TextStyle::T1).color(paint.fg_1()))
            .padding(Padding::default().top(8)),
        hint,
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
    container(lead).center_x(Fill).padding([0.0, NARROW_SIDE]).into()
}

fn recents(app: &App) -> Element<'_, Message> {
    let paint = app.paint;
    let width = app.window.size.width;
    let side = if width >= 2.0 * SIDE + 4.0 * CARD_LEAST + 3.0 * CARD_GAP { SIDE } else { NARROW_SIDE };
    let usable = (width - 2.0 * side).clamp(CARD_LEAST, 1040.0);
    let columns = ((usable + CARD_GAP) / (CARD_LEAST + CARD_GAP)).floor().clamp(1.0, 4.0);
    let card_width = ((usable - CARD_GAP * (columns - 1.0)) / columns).floor();
    let cards = app
        .recents
        .iter()
        .take(columns as usize)
        .fold(row![].spacing(CARD_GAP), |line, recent| line.push(card(app, recent, card_width)));
    let mut heading = row![
        ui::text(app.t(Key::ContinueWatching), TextStyle::HEAD).color(paint.fg_1()).wrapping(ui::Wrapping::None),
        Space::new().width(Fill),
    ]
    .align_y(Alignment::Center)
    .width(usable);
    if columns >= 4.0 {
        heading = heading.push(ui::text(app.t(Key::MemoryNote), TextStyle::CAP).color(paint.fg_2()));
    }
    container(column![heading, cards].spacing(18)).center_x(Fill).into()
}

fn footer(app: &App) -> Element<'_, Message> {
    let paint = app.paint;
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
    container(foot).center_x(Fill).padding(Padding::from([0.0, 0.0]).bottom(28)).into()
}

#[must_use]
pub fn view(app: &App) -> Element<'_, Message> {
    let paint = app.paint;
    let window = app.window;
    let roomy = window.size.height >= ROOMY_HEIGHT;
    let top = if roomy { 72.0 } else { 48.0 };
    let mut body = column![Space::new().height(top), hero(app)].width(Fill);
    if !app.recents.is_empty() {
        body = body.push(Space::new().height(12)).push(recents(app));
    }
    let foot = footer(app);
    let page: Element<'_, Message> = if roomy {
        column![body.height(Fill), foot].into()
    } else {
        let content = body.push(Space::new().height(24)).push(foot);
        iced::widget::scrollable(content)
            .direction(iced::widget::scrollable::Direction::Vertical(
                iced::widget::scrollable::Scrollbar::new().width(0).scroller_width(0),
            ))
            .height(Fill)
            .into()
    };
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
    surface.into()
}

#[must_use]
pub fn drop_overlay(app: &App) -> Element<'_, Message> {
    let paint = if app.media.is_some() { app.dark } else { app.paint };
    let adding = matches!(app.mode, crate::app::Mode::Clips { .. }) && app.media.is_some();
    let (title, note) = if adding { (Key::DropToAdd, Key::DropAddNote) } else { (Key::DropToPlay, Key::DropAspect) };
    let content = column![
        container(mark_with(&paint, Icon::DropIn, 28.0)).padding(Padding::default().bottom(4)),
        ui::text(app.t(title), TextStyle::T3).color(paint.acc_soft_fg()),
        ui::text(app.t(note), TextStyle::CAP).color(paint.fg_2()),
    ]
    .spacing(10)
    .align_x(Alignment::Center);
    let ring = paint.acc_bg();
    let (over, base) = (paint.acc_soft(), paint.view_bg());
    let mix = |top: f32, under: f32| top * over.a + under * (1.0 - over.a);
    let soft = Color { r: mix(over.r, base.r), g: mix(over.g, base.g), b: mix(over.b, base.b), a: 0.88 };
    container(container(content).center(Fill).style(move |_| container::Style {
        background: Some(Background::Color(soft)),
        border: Border { radius: theme::R3.into(), color: ring, width: 2.0 },
        ..container::Style::default()
    }))
    .padding(12)
    .into()
}

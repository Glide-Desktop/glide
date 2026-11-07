use std::time::Duration;

use glide_sdk::anim::{Speed, animate};
use glide_sdk::theme::TextStyle;
use glide_sdk::widget::player::{self, Mark, glass_disc, glass_slider, glass_tag, hud, scrubber, stage, subtitle};
use glide_sdk::widget::{self as ui};
use glide_sdk::window::{Action, Frame};
use iced::widget::{Space, button, column, container, mouse_area, row, stack};
use iced::{Alignment, Background, Border, Color, Element, Fill, Padding, Vector};

use crate::app::{App, Media, Message, Mode};
use crate::i18n::{self, Key};
use crate::icons::Icon;

const SCRIM_TOP: f32 = 136.0;
const SCRIM_BOTTOM: f32 = 220.0;
const TOP_TRAVEL: f32 = 24.0;
const SUBTITLE_SHOWN: f32 = 140.0;
const SUBTITLE_HIDDEN: f32 = 52.0;
const BUTTON: f32 = 32.0;

#[must_use]
pub fn ink() -> Color {
    ui::GLASS_INK
}

fn scrim<'a>(shown: bool, height: f32, top: bool) -> Element<'a, Message> {
    animate(if shown { 1.0 } else { 0.0 }, Speed::FADE, move |value| {
        let value = value.clone();
        container(Space::new().width(Fill).height(height)).style(move |_| {
            let strength = if top { 0.56 } else { 0.6 } * value.get();
            let (start, end) = if top {
                (Color { a: strength, ..Color::BLACK }, Color::TRANSPARENT)
            } else {
                (Color::TRANSPARENT, Color { a: strength, ..Color::BLACK })
            };
            container::Style {
                background: Some(Background::Gradient(iced::Gradient::Linear(
                    iced::gradient::Linear::new(std::f32::consts::PI).add_stop(0.0, start).add_stop(1.0, end),
                ))),
                ..container::Style::default()
            }
        })
    })
    .into()
}

fn hud_button(glyph: Icon, label: &str, on: bool, message: Message) -> Element<'_, Message> {
    let tint = if on { Color::from_rgb8(0xa7, 0xc8, 0xf6) } else { ink() };
    let face = button(container(ui::icon(glyph, 20.0, tint)).center(BUTTON))
        .padding(0)
        .width(BUTTON)
        .height(BUTTON)
        .on_press(message)
        .style(move |_, status| {
            let layer = match status {
                button::Status::Hovered => 0.12,
                button::Status::Pressed => 0.18,
                _ => 0.0,
            };
            let background = if on {
                Color { a: 0.22, ..Color::from_rgb8(91, 155, 236) }
            } else {
                Color { a: layer, ..Color::WHITE }
            };
            button::Style {
                background: Some(Background::Color(background)),
                text_color: ink(),
                border: Border { radius: 8.0.into(), ..Border::default() },
                ..button::Style::default()
            }
        });
    ui::tooltip(&glide_sdk::Paint::default(), face, label)
}

pub(crate) fn skip_button(back: bool, label: &str) -> Element<'_, Message> {
    let glyph = if back { Icon::Back10 } else { Icon::Forward10 };
    let number =
        ui::text("10", TextStyle { size_px: 7.5, line_height_px: 8.0, weight: 700, ..TextStyle::CAP }).color(ink());
    let face = stack![
        container(ui::icon(glyph, 22.0, ink())).center(BUTTON),
        container(number).center(BUTTON).padding(Padding::from([0.0, 0.0]).top(2.0).left(if back {
            1.0
        } else {
            -1.0
        })),
    ];
    let message = Message::Skip(if back { -10 } else { 10 });
    let face =
        button(face).padding(0).width(BUTTON).height(BUTTON).on_press(message).style(|_, status| button::Style {
            background: matches!(status, button::Status::Hovered | button::Status::Pressed)
                .then_some(Background::Color(Color { a: 0.12, ..Color::WHITE })),
            text_color: ink(),
            border: Border { radius: 8.0.into(), ..Border::default() },
            ..button::Style::default()
        });
    ui::tooltip(&glide_sdk::Paint::default(), face, label)
}

pub(crate) fn play_disc(playing: bool, label: &str) -> Element<'_, Message> {
    let glyph = if playing { Icon::Pause } else { Icon::Play };
    let face = button(container(ui::icon(glyph, 18.0, Color::from_rgb8(0x18, 0x17, 0x1b))).center(42.0))
        .padding(0)
        .width(42)
        .height(42)
        .on_press(Message::TogglePlay)
        .style(|_, status| button::Style {
            background: Some(Background::Color(if matches!(status, button::Status::Hovered) {
                Color::WHITE
            } else {
                ink()
            })),
            border: Border { radius: 21.0.into(), ..Border::default() },
            ..button::Style::default()
        });
    ui::tooltip(&glide_sdk::Paint::default(), face, label)
}

fn time_button<'a>(label: String, align: Alignment, message: Option<Message>) -> Element<'a, Message> {
    let text = ui::text(label, TextStyle { size_px: 12.0, line_height_px: 16.0, ..TextStyle::NUM })
        .color(Color { a: 0.88, ..ink() });
    let cell = container(text).width(60).align_x(align);
    match message {
        Some(message) => mouse_area(cell).on_press(message).interaction(iced::mouse::Interaction::Pointer).into(),
        None => cell.into(),
    }
}

fn controls<'a>(app: &'a App, media: &'a Media) -> Element<'a, Message> {
    let playing = media.playing();
    let volume = if media.muted { 0.0 } else { media.volume as f32 };
    let mute_glyph = if media.muted || volume <= 0.0 { Icon::Muted } else { Icon::Volume };
    let left = row![
        hud_button(mute_glyph, app.t(if media.muted { Key::Unmute } else { Key::Mute }), false, Message::ToggleMute),
        glass_slider(volume, 76.0, Message::Volume),
    ]
    .spacing(6)
    .align_y(Alignment::Center);
    let center = row![
        skip_button(true, app.t(Key::Back10)),
        play_disc(playing, app.t(if playing { Key::Pause } else { Key::Play })),
        skip_button(false, app.t(Key::Forward10))
    ]
    .spacing(10)
    .align_y(Alignment::Center);
    let speed_on = (media.rate - 1.0).abs() > 0.01;
    let speed_label = ui::text(
        i18n::speed(app.lang, media.rate),
        TextStyle { size_px: 12.5, line_height_px: 16.0, weight: 600, ..TextStyle::NUM },
    )
    .color(if speed_on { Color::from_rgb8(0xa7, 0xc8, 0xf6) } else { ink() });
    let speed = button(container(speed_label).center_y(BUTTON).padding([0.0, 6.0]))
        .padding(0)
        .height(BUTTON)
        .on_press(Message::SpeedMenu)
        .style(move |_, status| button::Style {
            background: Some(Background::Color(if speed_on {
                Color { a: 0.22, ..Color::from_rgb8(91, 155, 236) }
            } else {
                Color {
                    a: if matches!(status, button::Status::Hovered | button::Status::Pressed) { 0.12 } else { 0.0 },
                    ..Color::WHITE
                }
            })),
            border: Border { radius: 8.0.into(), ..Border::default() },
            ..button::Style::default()
        });
    let subtitles_on = media.selected_of(glide_video::media::TrackKind::Text).is_some();
    let fullscreen = app.window.frame == Frame::Fullscreen;
    let right = row![
        ui::tooltip(&glide_sdk::Paint::default(), speed, app.t(Key::SpeedLabel)),
        hud_button(Icon::Subtitles, app.t(Key::SubtitlesSound), subtitles_on, Message::TracksMenu),
        hud_button(Icon::More, app.t(Key::MoreActions), false, Message::MoreMenu),
        hud_button(
            if fullscreen { Icon::LeaveFullscreen } else { Icon::Fullscreen },
            app.t(if fullscreen { Key::LeaveFullscreen } else { Key::FullScreen }),
            false,
            Message::Window(Action::ToggleFullscreen),
        ),
    ]
    .spacing(2)
    .align_y(Alignment::Center);
    let top = row![container(left).width(Fill), center, container(right).width(Fill).align_x(Alignment::End),]
        .height(42)
        .align_y(Alignment::Center);
    let length = media.length();
    let hours = length >= Duration::from_hours(1);
    let marks: Vec<Mark> = if media.timeline.edited() || length.is_zero() {
        Vec::new()
    } else {
        media
            .chapters
            .iter()
            .map(|chapter| Mark {
                at: (chapter.start.as_secs_f64() / length.as_secs_f64()) as f32,
                title: chapter.title.clone(),
            })
            .collect()
    };
    let preview = media.hover.and_then(|bucket| media.frames.get(&bucket).cloned());
    let rail = scrubber(media.fraction(), length, Message::Hover, Message::Seek)
        .buffered(media.buffered)
        .marks(marks)
        .buckets(crate::app::BUCKETS)
        .preview(preview)
        .unreadable(unreadable_from(media))
        .on_release(Message::SeekReleased);
    let elapsed = player::clock(media.at(), hours);
    let right_time = if app.remaining || media.live {
        format!("−{}", player::clock(length.saturating_sub(media.at()), hours))
    } else {
        player::clock(length, hours)
    };
    let bottom = row![
        time_button(elapsed, Alignment::Start, None),
        rail,
        time_button(right_time, Alignment::End, Some(Message::ToggleRemaining)),
    ]
    .spacing(10)
    .height(20)
    .align_y(Alignment::Center);
    let width = crate::view::hud_rect(app.window.size).width;
    hud(column![top, bottom].spacing(2)).width(width).into()
}

fn topline<'a>(app: &'a App, media: &'a Media) -> Element<'a, Message> {
    let fullscreen = app.window.frame == Frame::Fullscreen;
    let lead = if fullscreen {
        glass_disc(Icon::LeaveFullscreen, 28.0, false, Some(Message::Window(Action::ToggleFullscreen)))
    } else {
        glass_disc(Icon::Maximize, 28.0, false, Some(Message::Window(Action::ToggleMaximize)))
    };
    let lead = ui::tooltip(
        &glide_sdk::Paint::default(),
        lead,
        app.t(if fullscreen { Key::LeaveFullscreen } else { Key::Maximize }),
    );
    let mut tags = row![].spacing(6).align_y(Alignment::Center);
    if let Some(info) = &media.info {
        if let Some(label) = info.resolution_label().filter(|_| media.showing_video()) {
            tags = tags.push(glass_tag(&label));
        }
        match info.video.as_ref().map(|video| video.hdr) {
            Some(glide_video::info::Hdr::Hdr10) => tags = tags.push(glass_tag("HDR10")),
            Some(glide_video::info::Hdr::Hlg) => tags = tags.push(glass_tag("HLG")),
            _ => {}
        }
    }
    if media.live {
        tags = tags.push(glass_tag(app.t(Key::Live)));
    }
    let sub_style = TextStyle { size_px: 12.0, line_height_px: 16.0, ..TextStyle::CAP };
    if let Some(index) = media.chapter().filter(|_| media.chapters.len() > 1) {
        let chapter = &media.chapters[index];
        let title = if chapter.title.is_empty() {
            app.t(Key::ChapterNumber).replace("{n}", &(index + 1).to_string())
        } else {
            app.t(Key::ChapterLine).replace("{n}", &(index + 1).to_string()).replace("{title}", &chapter.title)
        };
        let line =
            mouse_area(ui::text(title, sub_style).color(Color { a: 0.74, ..ink() }).wrapping(ui::Wrapping::None))
                .on_press(Message::ChaptersMenu)
                .interaction(iced::mouse::Interaction::Pointer);
        tags = tags.push(line);
    }
    let title = column![
        ui::text(media.title.clone(), TextStyle { size_px: 13.5, line_height_px: 20.0, ..TextStyle::WIN_TITLE })
            .color(ink())
            .wrapping(ui::Wrapping::None),
        tags,
    ];
    let pip = ui::tooltip(
        &glide_sdk::Paint::default(),
        glass_disc(Icon::Pip, 28.0, false, Some(Message::Pip)),
        app.t(Key::PictureInPicture),
    );
    let close = ui::tooltip(
        &glide_sdk::Paint::default(),
        glass_disc(Icon::Close, 28.0, true, Some(Message::Close)),
        app.t(Key::CloseWindow),
    );
    row![lead, container(title).width(Fill).padding(Padding::from([0.0, 4.0])), pip, close]
        .spacing(8)
        .align_y(Alignment::Center)
        .padding(Padding::from([14.0, 14.0]))
        .into()
}

#[must_use]
pub fn unreadable_from(media: &Media) -> Option<f32> {
    let glide_video::media::Failure::Broken(_) = media.failure.as_ref()? else { return None };
    let length = media.length().as_secs_f64();
    (length > 0.0).then(|| (media.reached.as_secs_f64() / length).clamp(0.0, 1.0) as f32)
}

fn subtitles(media: &Media, shown: bool) -> Element<'_, Message> {
    let Some((cue, _)) = media
        .cue
        .as_ref()
        .filter(|_| media.failure.is_none() && media.selected_of(glide_video::media::TrackKind::Text).is_some())
    else {
        return Space::new().into();
    };
    let lines = subtitle(&cue.text);
    let travel = SUBTITLE_SHOWN - SUBTITLE_HIDDEN;
    let lifted = animate(if shown { 1.0 } else { 0.0 }, Speed::EXPAND, move |_| container(lines).center_x(Fill))
        .offset(move |value| Vector::new(0.0, -travel * value));
    container(lifted)
        .height(Fill)
        .width(Fill)
        .align_y(Alignment::End)
        .padding(Padding::from([0.0, 24.0]).bottom(SUBTITLE_HIDDEN))
        .into()
}

#[must_use]
pub fn view<'a>(app: &'a App, media: &'a Media) -> Element<'a, Message> {
    let covered = matches!(app.exporting, Some(crate::app::Exporting::Form(_))) || app.ocr.is_some();
    let chrome = app.chrome.shown && !covered;
    let shown = (app.chrome.shown || app.mode != Mode::Play) && !covered;
    let editing = app.mode != Mode::Play;
    let base: Element<'a, Message> = if let Some(state) = crate::view::states::failure(app, media) {
        state
    } else if media.showing_video() {
        Space::new().width(Fill).height(Fill).into()
    } else {
        crate::view::states::audio(app, media)
    };
    let bars = column![scrim(shown, SCRIM_TOP, true), Space::new().height(Fill), scrim(shown, SCRIM_BOTTOM, false)];
    let top = animate(if shown { 1.0 } else { 0.0 }, Speed::EXPAND, move |_| topline(app, media))
        .offset(|value| Vector::new(0.0, -TOP_TRAVEL * (1.0 - value)));
    let mut layers: Vec<Element<'a, Message>> = vec![base, bars.into()];
    if app.ocr.is_none() {
        layers.push(subtitles(media, shown || editing));
    }
    if !editing {
        layers.push(top.into());
    }
    if let Some(chip) = crate::view::states::buffering(app, media) {
        layers.push(chip);
    }
    match app.mode {
        Mode::Play => {
            let hide = HUD_TRAVEL;
            let panel = animate(if chrome { 1.0 } else { 0.0 }, Speed::EXPAND, move |_| controls(app, media))
                .offset(move |value| Vector::new(0.0, hide * (1.0 - value)));
            layers.push(
                container(panel)
                    .width(Fill)
                    .height(Fill)
                    .align_x(Alignment::Center)
                    .align_y(Alignment::End)
                    .padding(Padding::from([0.0, 0.0]).bottom(crate::view::HUD_BOTTOM))
                    .into(),
            );
        }
        Mode::Trim { .. } => layers.push(crate::view::edit::trim(app, media)),
        Mode::Clips { .. } => layers.push(crate::view::edit::clips(app, media)),
    }
    if app.inspector {
        layers.push(crate::view::states::inspector(app));
    }
    if let Some((_, ocr)) = &app.ocr {
        layers.push(crate::view::states::ocr(app, media, ocr));
    }
    let content = stack(layers).width(Fill).height(Fill);
    let corner = app.window.corner();
    let stage_background = if media.showing_video() && media.failure.is_none() {
        Color::TRANSPARENT
    } else {
        Color::from_rgb8(0x11, 0x10, 0x14)
    };
    let pane = container(content).width(Fill).height(Fill).clip(true).style(move |_| container::Style {
        background: Some(Background::Color(stage_background)),
        border: Border { radius: corner.into(), ..Border::default() },
        ..container::Style::default()
    });
    let pane = player::hidden_cursor(!shown && !covered, pane);
    stage(pane)
        .on_drag(Message::WindowDrag)
        .on_double_click(Message::Window(Action::ToggleFullscreen))
        .on_right_click(Message::ContextMenu)
        .on_move((!app.chrome.shown).then_some(Message::Wake))
        .on_leave(Message::Leave)
        .into()
}

pub const HUD_TRAVEL: f32 = crate::view::HUD_HEIGHT + crate::view::HUD_BOTTOM + 8.0;

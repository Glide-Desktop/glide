use std::path::{Path, PathBuf};
use std::time::Duration;

use glide_sdk::theme::TextStyle;
use glide_sdk::widget::player::clock;
use glide_sdk::widget::{self as ui, Size};
use glide_sdk::{Paint, Tone};
use glide_video::export::{self, Codec, Quality};
use iced::widget::{Space, button, column, container, row, text_input};
use iced::{Alignment, Background, Border, Element, Fill, Length, Padding};

use crate::app::{App, ExportForm, Exporting, Media, Message};
use crate::i18n::{self, Key};
use crate::icons::Icon;

const WIDTH: f32 = 520.0;
const PRESET_HEIGHT: f32 = 68.0;

fn preset<'a>(paint: &Paint, name: String, size: String, on: bool, message: Message) -> Element<'a, Message> {
    let paint = *paint;
    let content = column![
        ui::text(name, TextStyle { size_px: 13.5, line_height_px: 18.0, weight: 600, ..TextStyle::CALL })
            .color(if on { paint.acc_soft_fg() } else { paint.fg_1() }),
        ui::text(size, TextStyle { size_px: 11.5, line_height_px: 15.0, ..TextStyle::CAP })
            .color(paint.fg_2())
            .wrapping(ui::Wrapping::WordOrGlyph),
    ]
    .spacing(2);
    button(container(content).padding([8.0, 10.0]).width(Fill).height(Fill))
        .padding(0)
        .width(Fill)
        .height(PRESET_HEIGHT)
        .on_press(message)
        .style(move |_, status| {
            let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
            button::Style {
                background: Some(Background::Color(if on {
                    paint.acc_soft()
                } else if hovered {
                    paint.fill_1()
                } else {
                    paint.view_bg()
                })),
                border: Border {
                    radius: 8.0.into(),
                    color: if on { paint.acc_bg() } else { paint.line_2() },
                    width: 1.0,
                },
                shadow: if on {
                    iced::Shadow { color: paint.acc_ring(), offset: iced::Vector::ZERO, blur_radius: 3.0 }
                } else {
                    iced::Shadow::default()
                },
                ..button::Style::default()
            }
        })
        .into()
}

fn switch_row<'a>(
    paint: &Paint, title: &'a str, note: &'a str, on: bool, enabled: bool, toggle: fn(bool) -> Message,
) -> Element<'a, Message> {
    let paint = *paint;
    row![
        column![
            ui::text(title, TextStyle::BODY).color(if enabled { paint.fg_1() } else { paint.fg_2() }),
            ui::text(note, TextStyle::CAP).color(paint.fg_2()).wrapping(ui::Wrapping::WordOrGlyph),
        ]
        .spacing(1)
        .width(Fill),
        ui::switch(&paint, on && enabled, enabled.then_some(toggle)),
    ]
    .spacing(12)
    .align_y(Alignment::Center)
    .padding(Padding::from([6.0, 0.0]))
    .into()
}

fn lead_text(app: &App, media: &Media) -> String {
    let source = media
        .path
        .as_deref()
        .and_then(std::path::Path::file_name)
        .map_or_else(|| media.title.clone(), |name| name.to_string_lossy().into_owned());
    let mut lead = app.t(Key::ExportBody).replace("{name}", &source);
    if media.timeline.edited() {
        lead = format!("{lead} {}", app.t(Key::TrimmedTo).replace("{time}", &clock(media.timeline.total(), true)));
        if media.timeline.clips.iter().any(|clip| clip.transform.turn != export::Turn::None) {
            lead = format!("{lead} {}", app.t(Key::ClipsTurned));
        }
    }
    lead
}

fn actions<'a>(app: &'a App, media: &'a Media, form: &'a ExportForm, lossless_possible: bool) -> Element<'a, Message> {
    let paint = app.paint;
    let length = media.timeline.total();
    let shown = if form.lossless && lossless_possible {
        (copy_size(media), Duration::from_secs(5))
    } else {
        let bytes = export::estimate(length, form.quality, form.codec);
        (bytes, Duration::from_secs_f64((length.as_secs_f64() / 2.5).max(5.0)))
    };
    let estimate = app
        .t(Key::Estimate)
        .replace("{size}", &i18n::bytes(app.lang, shown.0))
        .replace("{time}", &i18n::span(app.lang, shown.1));
    row![
        ui::text(estimate, TextStyle::CAP).color(paint.fg_2()),
        Space::new().width(Fill),
        ui::button(&paint, app.t(Key::Cancel), Tone::Neutral, Some(Message::ExportDismiss)),
        ui::button(&paint, app.t(Key::ExportButton), Tone::Accent, Some(Message::ExportStart)),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .into()
}

fn switches<'a>(app: &'a App, media: &'a Media, form: &'a ExportForm, lossless_possible: bool) -> Element<'a, Message> {
    let paint = app.paint;
    let lossless_note = if lossless_possible { app.t(Key::LosslessNote) } else { app.t(Key::LosslessUnavailable) };
    let hdr_source = media
        .info
        .as_ref()
        .and_then(|info| info.video.as_ref())
        .is_some_and(|video| video.hdr != glide_video::info::Hdr::Sdr);
    column![
        switch_row(
            &paint,
            app.t(Key::Lossless),
            lossless_note,
            form.lossless,
            lossless_possible,
            Message::ExportLossless
        ),
        switch_row(
            &paint,
            app.t(Key::KeepHdr),
            app.t(Key::KeepHdrNote),
            form.hdr,
            hdr_source && form.codec != Codec::H264,
            Message::ExportHdr
        ),
    ]
    .into()
}

#[derive(Debug, Clone, PartialEq)]
enum Place {
    Folder(PathBuf, String),
    Other(String),
}

impl std::fmt::Display for Place {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Folder(_, name) | Self::Other(name) => formatter.write_str(name),
        }
    }
}

fn folder_name(app: &App, folder: &Path) -> String {
    if crate::update::user_dir("XDG_VIDEOS_DIR", "Videos").as_deref() == Some(folder) {
        return app.t(Key::VideosFolder).to_string();
    }
    folder.file_name().map_or_else(|| folder.to_string_lossy().into_owned(), |name| name.to_string_lossy().into_owned())
}

fn places<'a>(app: &'a App, media: &'a Media, form: &'a ExportForm) -> Element<'a, Message> {
    let paint = app.paint;
    let mut folders: Vec<PathBuf> = vec![form.folder.clone()];
    let candidates = [
        crate::update::user_dir("XDG_VIDEOS_DIR", "Videos"),
        media.path.as_deref().and_then(Path::parent).map(Path::to_path_buf),
    ];
    for folder in candidates.into_iter().flatten() {
        if !folders.contains(&folder) {
            folders.push(folder);
        }
    }
    let mut options: Vec<Place> =
        folders.into_iter().map(|folder| Place::Folder(folder.clone(), folder_name(app, &folder))).collect();
    let current = options.first().cloned();
    options.push(Place::Other(app.t(Key::OtherFolder).to_string()));
    let lead = ui::icon(Icon::Folder, 16.0, paint.fg_2());
    ui::dropdown_with(&paint, Some(lead), TextStyle::CALL, options, current, "", |place| match place {
        Place::Folder(folder, _) => Message::ExportFolderChosen(folder),
        Place::Other(_) => Message::ExportFolder,
    })
}

fn form<'a>(app: &'a App, media: &'a Media, form: &'a ExportForm) -> Element<'a, Message> {
    let paint = app.paint;
    let lang = app.lang;
    let lead = lead_text(app, media);
    let segments = media.timeline.segments(&media.uri);
    let lossless_possible = export::lossless_possible(&segments);
    let extension = if form.quality == Quality::AudioOnly {
        "m4a"
    } else if form.lossless && lossless_possible {
        export::container_of(&media.uri)
    } else {
        "mp4"
    };
    let name = ui::field_with(
        &paint,
        None,
        text_input("", &form.name).on_input(Message::ExportName).on_submit(Message::ExportStart),
        vec![ui::text(format!(".{extension}"), TextStyle::BODY).color(paint.fg_3()).into()],
    );
    let place = places(app, media, form);
    let labelled = |label: &'a str, content: Element<'a, Message>| -> Element<'a, Message> {
        column![
            ui::text(label, TextStyle { size_px: 13.0, line_height_px: 19.0, weight: 500, ..TextStyle::CALL })
                .color(paint.fg_1()),
            content
        ]
        .spacing(6)
        .into()
    };
    let top = row![
        container(labelled(app.t(Key::Name), name)).width(Fill),
        container(labelled(app.t(Key::Where), place)).width(Length::Fixed(168.0)),
    ]
    .spacing(12);
    let length = media.timeline.total();
    let presets = [
        (Quality::P2160, "2160p".to_string(), "4K"),
        (Quality::P1080, "1080p".to_string(), "Full HD"),
        (Quality::P720, "720p".to_string(), "HD"),
        (Quality::AudioOnly, app.t(Key::AudioOnly).to_string(), "AAC"),
    ];
    let cards = presets.into_iter().fold(row![].spacing(8), |line, (quality, name, kind)| {
        let size = i18n::bytes(lang, export::estimate(length, quality, form.codec));
        line.push(
            container(preset(
                &paint,
                name,
                format!("{kind} · ≈ {size}"),
                form.quality == quality,
                Message::ExportQuality(quality),
            ))
            .width(Fill),
        )
    });
    let codecs = ui::segmented_spread(
        &paint,
        &[(Codec::H264, app.t(Key::H264)), (Codec::Hevc, app.t(Key::Hevc)), (Codec::Av1, app.t(Key::Av1))],
        form.codec,
        Message::ExportCodec,
    );
    let codec = column![codecs, ui::text(app.t(Key::EncoderNote), TextStyle::CAP).color(paint.fg_2())].spacing(6);
    let switches = switches(app, media, form, lossless_possible);
    let line = container(Space::new().width(Fill).height(1)).style(move |_| container::Style {
        background: Some(Background::Color(paint.line())),
        ..container::Style::default()
    });
    let actions = actions(app, media, form, lossless_possible);
    let body = column![
        column![
            ui::text(app.t(Key::ExportTitle), TextStyle::T3).color(paint.fg_1()),
            ui::text(lead, TextStyle::CALL).color(paint.fg_2()).wrapping(ui::Wrapping::WordOrGlyph),
        ]
        .spacing(4),
        top,
        labelled(app.t(Key::Quality), cards.into()),
        labelled(app.t(Key::Codec), codec.into()),
        column![line, switches],
        actions,
    ]
    .spacing(16);
    let card = container(body).padding(24).width(WIDTH).style(move |_| glide_sdk::glide_iced::style::popover(&paint));
    glide_sdk::overlay::modal(&paint, 1.0, card, Some(Message::ExportDismiss), app.window.corner())
}

#[must_use]
pub fn copy_size(media: &Media) -> u64 {
    let (Some(bytes), Some(duration)) = (media.info.as_ref().and_then(|info| info.bytes), media.duration) else {
        return 0;
    };
    let share = media.timeline.total().as_secs_f64() / duration.as_secs_f64().max(0.001);
    (bytes as f64 * share.clamp(0.0, 1.0)) as u64
}

fn running<'a>(
    app: &'a App, progress: f64, started: std::time::Instant, hardware: bool, name: &'a str,
) -> Element<'a, Message> {
    let paint = app.paint;
    let percent = (progress * 100.0).round() as u32;
    let elapsed = started.elapsed().as_secs_f64();
    let left =
        if progress > 0.02 { Duration::from_secs_f64(elapsed / progress * (1.0 - progress)) } else { Duration::ZERO };
    let who = app.t(if hardware { Key::ByCard } else { Key::ByProcessor });
    let detail = format!(
        "{} · {}",
        app.t(Key::ExportProgress)
            .replace("{n}", &percent.to_string())
            .replace("{left}", &clock(left, false))
            .replace("{who}", who),
        app.t(Key::DockProgress)
    );
    let head = row![
        ui::icon(Icon::Export, 16.0, paint.acc_text()),
        ui::text(app.t(Key::Exporting).replace("{name}", name), TextStyle::HEAD)
            .color(paint.fg_1())
            .wrapping(ui::Wrapping::None)
            .width(Fill),
        ui::sized_button(&paint, app.t(Key::Cancel), Tone::Ghost, Size::Small, Some(Message::ExportCancel)),
    ]
    .spacing(10)
    .align_y(Alignment::Center);
    let card = column![
        head,
        ui::progress(&paint, Some(progress as f32)),
        ui::text(detail, TextStyle::CAP).color(paint.fg_2())
    ]
    .spacing(8);
    let card = container(card)
        .padding(Padding::from([14.0, 14.0]).left(16.0))
        .width(480)
        .style(move |_| glide_sdk::glide_iced::style::popover(&paint));
    let card = glide_sdk::glide_iced::widgets::surface(
        card,
        &paint,
        glide_sdk::glide_theme::elevation::Level::El3,
        glide_sdk::theme::R3,
    );
    let card = glide_sdk::widget::rising(card);
    container(card)
        .width(Fill)
        .height(Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::End)
        .padding(Padding::from([0.0, 0.0]).bottom(28))
        .into()
}

fn done<'a>(app: &'a App, path: &std::path::Path) -> Element<'a, Message> {
    let paint = app.paint;
    let folder = path
        .parent()
        .and_then(std::path::Path::file_name)
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    let content = row![
        ui::icon(Icon::Export, 16.0, paint.ok()),
        ui::text(app.t(Key::Saved).replace("{folder}", &folder), TextStyle::CALL).color(paint.fg_1()),
        ui::sized_button(&paint, app.t(Key::ShowInFiles), Tone::Ghost, Size::Small, Some(Message::RevealSaved)),
        ui::sized_button(&paint, app.t(Key::OpenSaved), Tone::Tonal, Size::Small, Some(Message::OpenSaved)),
    ]
    .spacing(12)
    .align_y(Alignment::Center);
    let card = container(content)
        .padding(Padding::from([8.0, 8.0]).left(14.0))
        .style(move |_| glide_sdk::glide_iced::style::popover(&paint));
    let card = glide_sdk::glide_iced::widgets::surface(
        card,
        &paint,
        glide_sdk::glide_theme::elevation::Level::El3,
        glide_sdk::theme::R3,
    );
    let card = glide_sdk::widget::rising(card);
    container(card)
        .width(Fill)
        .height(Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::End)
        .padding(Padding::from([0.0, 0.0]).bottom(28))
        .into()
}

#[must_use]
pub fn layer(app: &App) -> Option<Element<'_, Message>> {
    let media = app.media.as_ref()?;
    match app.exporting.as_ref()? {
        Exporting::Form(state) => Some(form(app, media, state)),
        Exporting::Running { progress, started, hardware, name, .. } => {
            Some(running(app, *progress, *started, *hardware, name))
        }
        Exporting::Done(path) => Some(done(app, path)),
        Exporting::Failed => None,
    }
}

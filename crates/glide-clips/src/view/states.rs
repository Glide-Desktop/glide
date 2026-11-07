use glide_sdk::theme::TextStyle;
use glide_sdk::widget::player::{clock, fit, glass_disc};
use glide_sdk::widget::{self as ui, glass_style};
use glide_video::info::Hdr;
use glide_video::media::{Failure, TrackKind};
use iced::widget::{Space, button, column, container, image, mouse_area, row, stack};
use iced::{Alignment, Background, Border, Color, Element, Fill, Padding, Size};

use crate::app::{App, Media, Message, Ocr};
use crate::i18n::{self, Key};
use crate::icons::Icon;
use crate::view::player::ink;

fn glass_panel<'a>(content: impl Into<Element<'a, Message>>, padding: Padding) -> container::Container<'a, Message> {
    container(content)
        .padding(padding)
        .style(|_| container::Style { border: Border { radius: 12.0.into(), ..glass_style().border }, ..glass_style() })
}

fn glass_button(label: &str, primary: bool, message: Message) -> Element<'_, Message> {
    let accent = glide_sdk::Paint::default().acc_bg();
    button(
        container(
            ui::text(label, TextStyle { size_px: 13.0, line_height_px: 16.0, weight: 500, ..TextStyle::CALL })
                .color(if primary { Color::WHITE } else { ink() }),
        )
        .center_y(30.0)
        .padding([0.0, 12.0]),
    )
    .padding(0)
    .height(30)
    .on_press(message)
    .style(move |_, status| {
        let hover = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let background = match (primary, hover) {
            (true, _) => accent,
            (false, false) => Color { a: 0.12, ..Color::WHITE },
            (false, true) => Color { a: 0.2, ..Color::WHITE },
        };
        button::Style {
            background: Some(Background::Color(background)),
            border: Border { radius: 8.0.into(), ..Border::default() },
            ..button::Style::default()
        }
    })
    .into()
}

fn centre<'a>(
    glyph: Icon, tint: Color, title: String, body: String, actions: Vec<Element<'a, Message>>,
    extra: Option<Element<'a, Message>>,
) -> Element<'a, Message> {
    let mut content = column![
        ui::icon(glyph, 36.0, tint),
        ui::text(title, TextStyle::T3).color(ink()),
        container(
            ui::text(body, TextStyle::CALL)
                .color(Color { a: 0.72, ..ink() })
                .wrapping(ui::Wrapping::WordOrGlyph)
                .align_x(Alignment::Center)
        )
        .max_width(460),
    ]
    .spacing(8)
    .align_x(Alignment::Center);
    if !actions.is_empty() {
        content = content.push(Space::new().height(4)).push(row(actions).spacing(8));
    }
    if let Some(extra) = extra {
        content = content.push(extra);
    }
    container(content).center(Fill).padding(28).into()
}

#[must_use]
pub fn failure<'a>(app: &'a App, media: &'a Media) -> Option<Element<'a, Message>> {
    let failure = media.failure.as_ref()?;
    let warn = Color::from_rgb8(0xe5, 0xb3, 0x55);
    let danger = Color::from_rgb8(0xf0, 0x79, 0x6f);
    Some(match failure {
        Failure::Missing { what, detail } => {
            let package = detail.as_deref().and_then(package_of);
            let codec = codec_name(what);
            let title = match &codec {
                Some(codec) => app.t(Key::MissingTitle).replace("{what}", codec),
                None => app.t(Key::MissingSome).to_string(),
            };
            let command = package.map(|package| format!("sudo pacman -S {package}"));
            let details = (media.details && command.is_some()).then(|| {
                let command = command.clone().unwrap_or_default();
                let line = row![
                    ui::text(command.clone(), TextStyle { size_px: 11.5, line_height_px: 16.0, ..TextStyle::NUM })
                        .color(Color { a: 0.86, ..ink() })
                        .width(Fill),
                    glass_button(app.t(Key::CopyCommand), false, Message::CopyCommand(command)),
                ]
                .spacing(8)
                .align_y(Alignment::Center);
                container(line)
                    .padding([8.0, 10.0])
                    .width(420)
                    .style(|_| container::Style {
                        background: Some(Background::Color(Color { a: 0.08, ..Color::WHITE })),
                        border: Border { radius: 8.0.into(), ..Border::default() },
                        ..container::Style::default()
                    })
                    .into()
            });
            let mut actions = Vec::new();
            if media.tracks.of(TrackKind::Audio).next().is_some()
                || media.info.as_ref().is_some_and(|info| !info.audio.is_empty())
            {
                actions.push(glass_button(app.t(Key::SoundOnly), false, Message::SoundOnly));
            }
            if command.is_some() {
                actions.push(glass_button(app.t(Key::Details), true, Message::ToggleDetails));
            }
            centre(Icon::NoVideo, warn, title, app.t(Key::MissingBody).to_string(), actions, details)
        }
        Failure::NotFound => {
            centre(Icon::Warn, danger, app.t(Key::NotFoundTitle).into(), app.t(Key::NotFoundBody).into(), vec![], None)
        }
        Failure::Network(_) => centre(
            Icon::Warn,
            danger,
            app.t(Key::NetworkTitle).into(),
            app.t(Key::NetworkBody).into(),
            vec![glass_button(app.t(Key::TryAgain), true, Message::Retry)],
            None,
        ),
        Failure::Broken(_) if media.reached > std::time::Duration::from_secs(2) => {
            let body = app.t(Key::BrokenBody).replace("{time}", &clock(media.reached, true));
            let mut actions = Vec::new();
            if media.path.is_some() {
                actions.push(glass_button(app.t(Key::ShowInFiles), false, Message::ShowInFiles));
            }
            actions.push(glass_button(app.t(Key::FromStart), true, Message::FromStart));
            centre(Icon::Warn, danger, app.t(Key::BrokenTitle).into(), body, actions, None)
        }
        Failure::Broken(_) => {
            centre(Icon::Warn, danger, app.t(Key::BrokenStart).into(), app.t(Key::BrokenStartBody).into(), vec![], None)
        }
        Failure::Other(reason) => centre(
            Icon::Warn,
            danger,
            app.t(Key::OtherTitle).into(),
            reason.clone(),
            vec![glass_button(app.t(Key::TryAgain), true, Message::Retry)],
            None,
        ),
    })
}

#[must_use]
pub fn codec_name(what: &str) -> Option<String> {
    let lower = what.to_lowercase();
    let known = [
        ("h.265", "HEVC"),
        ("hevc", "HEVC"),
        ("h.264", "H.264"),
        ("av1", "AV1"),
        ("vp9", "VP9"),
        ("vp8", "VP8"),
        ("mpeg-2", "MPEG-2"),
        ("prores", "ProRes"),
        ("dts", "DTS"),
        ("ac-3", "AC-3"),
        ("aac", "AAC"),
        ("wmv", "WMV"),
        ("windows media", "WMV"),
    ];
    known.iter().find(|(key, _)| lower.contains(key)).map(|(_, name)| (*name).to_string())
}

#[must_use]
pub fn package_of(detail: &str) -> Option<&'static str> {
    let lower = detail.to_ascii_lowercase();
    if lower.contains("h265")
        || lower.contains("x-h265")
        || lower.contains("h264")
        || lower.contains("x-h264")
        || lower.contains("libav")
        || lower.contains("x-wmv")
        || lower.contains("prores")
    {
        Some("gst-libav")
    } else if lower.contains("x-ac3") || lower.contains("x-dts") || lower.contains("mpeg2") || lower.contains("dvd") {
        Some("gst-plugins-ugly")
    } else if lower.contains("decoder") || lower.contains("demux") {
        Some("gst-plugins-bad")
    } else {
        None
    }
}

#[must_use]
pub fn audio<'a>(app: &'a App, media: &'a Media) -> Element<'a, Message> {
    let info = media.info.as_ref();
    let title = info.and_then(|info| info.title.clone()).unwrap_or_else(|| media.title.clone());
    let artist = info
        .and_then(|info| info.artist.clone().or_else(|| info.album.clone()))
        .unwrap_or_else(|| app.t(Key::Podcast).to_string());
    let format = info
        .and_then(|info| info.audio.first())
        .map(|audio| {
            let mut parts = vec![audio.codec.clone()];
            if audio.rate > 0 {
                parts.push(format!(
                    "{} {}",
                    i18n::decimal(app.lang, f64::from(audio.rate) / 1000.0, 1),
                    app.lang.pick("кГц", "kHz")
                ));
            }
            if audio.bitrate > 0 {
                parts.push(format!("{} {}", audio.bitrate / 1000, app.lang.pick("кбіт/с", "kbit/s")));
            }
            parts.into_iter().filter(|part| !part.is_empty()).collect::<Vec<_>>().join(" · ")
        })
        .unwrap_or_default();
    let cover: Element<'a, Message> = match &media.cover {
        Some(handle) => {
            image(handle.clone()).width(160).height(160).content_fit(iced::ContentFit::Cover).border_radius(8.0).into()
        }
        None => container(ui::icon(Icon::Pitch, 44.0, Color { a: 0.7, ..ink() }))
            .center(160.0)
            .style(|_| container::Style {
                background: Some(Background::Color(Color::from_rgb8(0x2f, 0x2e, 0x34))),
                border: Border { radius: 8.0.into(), ..Border::default() },
                ..container::Style::default()
            })
            .into(),
    };
    let text = column![
        ui::text(title, TextStyle::T2).color(ink()).wrapping(ui::Wrapping::WordOrGlyph),
        ui::text(artist, TextStyle::CALL).color(Color { a: 0.72, ..ink() }),
        ui::text(format, TextStyle::CAP).color(Color { a: 0.55, ..ink() }),
    ]
    .spacing(4);
    let wave = waveform(media);
    let content = column![row![cover, text].spacing(24).align_y(Alignment::Center), wave].spacing(28).max_width(720);
    container(content)
        .center(Fill)
        .padding(Padding::from([0.0, 48.0]).bottom(140.0))
        .style(|_| container::Style {
            background: Some(Background::Color(Color::from_rgb8(0x18, 0x17, 0x1b))),
            ..container::Style::default()
        })
        .into()
}

fn waveform(media: &Media) -> Element<'_, Message> {
    let bars = 96;
    let played = (media.fraction() * bars as f32) as usize;
    let seed = media.title.bytes().fold(7u32, |hash, byte| hash.wrapping_mul(31).wrapping_add(u32::from(byte)));
    let line = (0..bars).fold(row![].spacing(2).align_y(Alignment::Center), |line, index| {
        let x = index as f32;
        let wobble = ((x * 0.7 + seed as f32 * 0.01).sin() * 18.0
            + (x * 0.23).sin() * 14.0
            + (x * 1.9 + seed as f32).cos() * 6.0)
            .abs();
        let height = (6.0 + wobble).min(44.0);
        let colour = if index < played { ink() } else { Color { a: 0.28, ..Color::WHITE } };
        line.push(container(Space::new().width(3).height(height)).style(move |_| container::Style {
            background: Some(Background::Color(colour)),
            border: Border { radius: 2.0.into(), ..Border::default() },
            ..container::Style::default()
        }))
    });
    container(line).height(44).center_y(44).into()
}

#[must_use]
pub fn buffering<'a>(app: &'a App, media: &'a Media) -> Option<Element<'a, Message>> {
    let percent = media.buffering?;
    if media.failure.is_some() {
        return None;
    }
    let mut content = column![
        ui::spinner(&app.dark, true),
        ui::text(app.t(Key::Buffering).replace("{n}", &percent.to_string()), TextStyle::CALL).color(ink()),
    ]
    .spacing(10)
    .align_x(Alignment::Center);
    if media.live {
        content = content.push(glass_button(app.t(Key::ToLive), false, Message::ToLive));
    }
    Some(container(glass_panel(content, Padding::from([14.0, 18.0]))).center(Fill).into())
}

#[must_use]
pub fn primaries(colorimetry: Option<&str>, hdr: Hdr) -> String {
    let lower = colorimetry.unwrap_or_default().to_ascii_lowercase();
    let named = if lower.starts_with("bt2020") || lower.starts_with("bt2100") {
        Some("BT.2020")
    } else if lower.starts_with("bt709") {
        Some("BT.709")
    } else if lower.starts_with("bt601") || lower.starts_with("smpte170m") {
        Some("BT.601")
    } else if lower.starts_with("srgb") || lower.starts_with("sycc") {
        Some("sRGB")
    } else {
        None
    };
    match (named, colorimetry) {
        (Some(name), _) => name.to_string(),
        (None, Some(raw)) if !raw.is_empty() => raw.to_string(),
        _ => if hdr == Hdr::Sdr { "BT.709" } else { "BT.2020" }.to_string(),
    }
}

#[must_use]
pub fn inspector_rows(app: &App) -> Vec<(String, String)> {
    let Some(media) = &app.media else { return Vec::new() };
    let lang = app.lang;
    let mut rows = vec![(app.t(Key::File).to_string(), media.title.clone())];
    let Some(info) = &media.info else { return rows };
    let mut size = Vec::new();
    if let Some(bytes) = info.bytes {
        size.push(i18n::bytes(lang, bytes));
    }
    if let Some(duration) = info.duration {
        size.push(clock(duration, true));
    }
    if !size.is_empty() {
        rows.push((app.t(Key::Size).into(), size.join(" · ")));
    }
    if let Some(video) = &info.video {
        let fps = i18n::decimal(lang, video.fps, 2);
        let depth = if video.depth >= 10 {
            format!(
                " {}",
                if video.codec.contains("10") {
                    String::new()
                } else {
                    format!("· {} {}", video.depth, lang.pick("біт", "bit"))
                }
            )
        } else {
            String::new()
        };
        rows.push((
            app.t(Key::Video).into(),
            format!("{} · {} × {} · {fps} {}{depth}", video.codec, video.width, video.height, lang.pick("к/с", "fps")),
        ));
        let primaries = primaries(video.colorimetry.as_deref(), video.hdr);
        let colour = match video.hdr {
            Hdr::Hdr10 => format!("{primaries} · PQ (HDR10) → SDR"),
            Hdr::Hlg => format!("{primaries} · HLG → SDR"),
            Hdr::Sdr => primaries,
        };
        rows.push((app.t(Key::Colour).into(), colour));
    }
    if let Some(bitrate) = info.bitrate() {
        rows.push((
            app.t(Key::Bitrate).into(),
            format!("{} {}", i18n::decimal(lang, f64::from(bitrate) / 1_000_000.0, 1), lang.pick("Мбіт/с", "Mbit/s")),
        ));
    }
    let chosen = media.selected_of(TrackKind::Audio).and_then(|id| media.tracks.get(id));
    if let Some(audio) = info.audio.first() {
        let language = chosen
            .and_then(|track| crate::menus::language(track.language.as_deref(), lang))
            .or_else(|| crate::menus::language(audio.language.as_deref(), lang));
        let channels = match audio.channels {
            6 => "5.1".to_string(),
            8 => "7.1".to_string(),
            2 => lang.pick("стерео", "stereo").to_string(),
            1 => lang.pick("моно", "mono").to_string(),
            other => other.to_string(),
        };
        let mut line = format!("{} {channels}", audio.codec);
        if audio.bitrate > 0 {
            line = format!("{line} · {} {}", audio.bitrate / 1000, lang.pick("кбіт/с", "kbit/s"));
        }
        if let Some(language) = language {
            line = format!("{line}, {}", language.to_lowercase());
        }
        rows.push((app.t(Key::Sound).into(), line));
    }
    let decoder = media.player.as_ref().and_then(glide_video::player::Player::decoder);
    if let Some((name, hardware)) = decoder {
        rows.push((
            app.t(Key::Decoder).into(),
            format!("{name} · {}", app.t(if hardware { Key::Hardware } else { Key::Software })),
        ));
    }
    if let Some(dropped) = media.player.as_ref().and_then(glide_video::player::Player::dropped) {
        rows.push((app.t(Key::Dropped).into(), app.t(Key::Frames).replace("{n}", &dropped.to_string())));
    }
    rows
}

#[must_use]
pub fn inspector(app: &App) -> Element<'_, Message> {
    let rows = inspector_rows(app);
    let grid = rows.into_iter().fold(column![].spacing(3), |grid, (key, value)| {
        let hardware = value.ends_with(app.t(Key::Hardware));
        grid.push(
            row![
                ui::text(key, TextStyle { size_px: 11.5, line_height_px: 16.0, ..TextStyle::CAP })
                    .color(Color { a: 0.62, ..ink() })
                    .width(84),
                ui::text(value, TextStyle { size_px: 11.5, line_height_px: 16.0, ..TextStyle::CAP })
                    .color(if hardware { Color::from_rgb8(0x62, 0xd6, 0x9b) } else { ink() })
                    .wrapping(ui::Wrapping::WordOrGlyph)
                    .width(Fill),
            ]
            .spacing(10),
        )
    });
    let head = row![
        ui::text(app.t(Key::InspectorTitle), TextStyle::HEAD).color(ink()).width(Fill),
        glass_disc(Icon::Close, 24.0, false, Some(Message::Inspector(false))),
    ]
    .align_y(Alignment::Center);
    let copy = button(
        row![ui::icon(Icon::Copy, 14.0, ink()), ui::text(app.t(Key::CopyAll), TextStyle::CAP).color(ink())]
            .spacing(6)
            .align_y(Alignment::Center),
    )
    .padding([4.0, 10.0])
    .on_press(Message::CopyInspector)
    .style(|_, _| button::Style {
        background: Some(Background::Color(Color { a: 0.12, ..Color::WHITE })),
        border: Border { radius: 8.0.into(), ..Border::default() },
        ..button::Style::default()
    });
    let panel = glass_panel(column![head, grid, copy].spacing(8).width(300), Padding::from([12.0, 14.0]));
    container(panel).width(Fill).height(Fill).align_x(Alignment::End).padding(Padding::from([60.0, 14.0])).into()
}

#[must_use]
pub fn ocr<'a>(app: &'a App, media: &'a Media, state: &'a Ocr) -> Element<'a, Message> {
    let size = app.window.size;
    let video = media.video_size().map_or(size, |(width, height)| Size::new(width as f32, height as f32));
    let frame = fit(size, video);
    let mut layers: Vec<Element<'a, Message>> = vec![
        mouse_area(container(Space::new().width(Fill).height(Fill)).style(|_| container::Style {
            background: Some(Background::Color(Color { a: 0.12, ..Color::BLACK })),
            ..container::Style::default()
        }))
        .on_press(Message::OcrClose)
        .into(),
    ];
    let footer = match state {
        Ocr::Busy => {
            row![ui::spinner(&app.dark, false), ui::text(app.t(Key::Recognising), TextStyle::CAP).color(ink())]
                .spacing(8)
                .align_y(Alignment::Center)
        }
        Ocr::Missing => row![ui::text(app.t(Key::NoTesseract), TextStyle::CAP).color(ink())],
        Ocr::Failed => row![ui::text(app.t(Key::NothingRecognised), TextStyle::CAP).color(ink())],
        Ocr::Lines(lines) if lines.is_empty() => {
            row![ui::text(app.t(Key::NothingRecognised), TextStyle::CAP).color(ink())]
        }
        Ocr::Lines(lines) => {
            row![ui::text(app.t(Key::Recognised).replace("{n}", &lines.len().to_string()), TextStyle::CAP).color(ink())]
        }
    };
    if let Ocr::Lines(lines) = state {
        let all = lines.iter().map(|line| line.text.clone()).collect::<Vec<_>>().join("\n");
        for line in lines {
            let (x, y) = (frame.x + line.x * frame.width, frame.y + line.y * frame.height);
            let (width, height) = ((line.width * frame.width).max(4.0), (line.height * frame.height).max(4.0));
            let accent = Color::from_rgb8(91, 155, 236);
            let mark =
                mouse_area(container(Space::new().width(width).height(height)).style(move |_| container::Style {
                    background: Some(Background::Color(Color { a: 0.28, ..accent })),
                    border: Border { radius: 3.0.into(), color: accent, width: 1.5 },
                    ..container::Style::default()
                }))
                .on_press(Message::CopyText(line.text.clone()))
                .interaction(iced::mouse::Interaction::Pointer);
            layers.push(container(mark).padding(Padding::from([0.0, 0.0]).left(x).top(y)).into());
        }
        if let Some(first) = lines.first() {
            let (x, y) = (frame.x + first.x * frame.width, (frame.y + first.y * frame.height - 40.0).max(8.0));
            let bubble = glass_panel(
                row![
                    glass_button(app.t(Key::Copy), false, Message::CopyText(all.clone())),
                    glass_button(app.t(Key::SearchWeb), false, Message::SearchText(all)),
                ]
                .spacing(2),
                Padding::from(3.0),
            );
            layers.push(container(bubble).padding(Padding::from([0.0, 0.0]).left(x).top(y)).into());
        }
    }
    let bar = glass_panel(
        row![footer, Space::new().width(Fill), glass_button(app.t(Key::Done), false, Message::OcrClose)]
            .align_y(Alignment::Center),
        Padding::from([6.0, 10.0]),
    );
    layers.push(container(bar).width(Fill).height(Fill).align_y(Alignment::End).padding(24).into());
    stack(layers).width(Fill).height(Fill).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_decoders_name_the_codec_and_the_package() {
        assert_eq!(codec_name("H.265 (Main 10 Profile) decoder").as_deref(), Some("HEVC"));
        assert_eq!(package_of("gstreamer|1.0|glide-clips|H.265 decoder|decoder-video/x-h265"), Some("gst-libav"));
        assert_eq!(package_of("gstreamer|1.0|x|AC-3 decoder|decoder-audio/x-ac3"), Some("gst-plugins-ugly"));
        assert_eq!(codec_name("something odd"), None);
    }

    #[test]
    fn colour_primaries_read_as_the_standard_not_the_gstreamer_token() {
        assert_eq!(primaries(Some("bt2100-pq"), Hdr::Hdr10), "BT.2020");
        assert_eq!(primaries(Some("bt709"), Hdr::Sdr), "BT.709");
        assert_eq!(primaries(Some("2:4:5:4"), Hdr::Sdr), "2:4:5:4");
        assert_eq!(primaries(None, Hdr::Hlg), "BT.2020");
        assert_eq!(primaries(None, Hdr::Sdr), "BT.709");
    }
}

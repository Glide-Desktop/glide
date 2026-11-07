use glide_sdk::Item;
use glide_sdk::Lang;
use glide_video::media::{Track, TrackKind};
use iced::Point;

use crate::app::{App, Aspect, Message};
use crate::i18n::{self, Key};
use crate::icons::Icon;
use crate::keys::SPEEDS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    Speed,
    Tracks,
    Chapters,
    More,
    Aspect,
}

#[must_use]
pub fn language(code: Option<&str>, lang: Lang) -> Option<&'static str> {
    let code = code?.to_ascii_lowercase();
    let short = code.split(['-', '_']).next().unwrap_or("").to_string();
    let names: &[(&[&str], &str, &str)] = &[
        (&["uk", "ukr"], "Ukrainian", "Українська"),
        (&["en", "eng"], "English", "English"),
        (&["de", "deu", "ger"], "German", "Німецька"),
        (&["fr", "fra", "fre"], "French", "Французька"),
        (&["es", "spa"], "Spanish", "Іспанська"),
        (&["it", "ita"], "Italian", "Італійська"),
        (&["pl", "pol"], "Polish", "Польська"),
        (&["pt", "por"], "Portuguese", "Португальська"),
        (&["cs", "ces", "cze"], "Czech", "Чеська"),
        (&["ja", "jpn"], "Japanese", "Японська"),
        (&["zh", "zho", "chi"], "Chinese", "Китайська"),
        (&["ko", "kor"], "Korean", "Корейська"),
        (&["ru", "rus"], "Russian", "Російська"),
        (&["tr", "tur"], "Turkish", "Турецька"),
        (&["nl", "nld", "dut"], "Dutch", "Нідерландська"),
        (&["sv", "swe"], "Swedish", "Шведська"),
    ];
    names.iter().find(|(codes, _, _)| codes.contains(&short.as_str())).map(|(_, en, uk)| lang.pick(uk, en))
}

fn channels(count: Option<i32>, lang: Lang) -> Option<String> {
    let count = count?;
    Some(match count {
        1 => lang.pick("моно", "mono").to_string(),
        2 => lang.pick("стерео", "stereo").to_string(),
        6 => "5.1".to_string(),
        8 => "7.1".to_string(),
        other => format!("{other} {}", lang.pick("кан.", "ch")),
    })
}

fn short_codec(track: &Track) -> Option<String> {
    let codec = track.codec.clone().or_else(|| track.format.clone())?;
    let lower = codec.to_ascii_lowercase();
    let named = [
        ("subrip", "SubRip"),
        ("x-subrip", "SubRip"),
        ("ass", "ASS"),
        ("ssa", "SSA"),
        ("webvtt", "WebVTT"),
        ("pgs", "PGS"),
        ("dvd", "VobSub"),
        ("ac-3", "AC-3"),
        ("ac3", "AC-3"),
        ("e-ac-3", "E-AC-3"),
        ("eac3", "E-AC-3"),
        ("aac", "AAC"),
        ("mpeg-4 aac", "AAC"),
        ("opus", "Opus"),
        ("vorbis", "Vorbis"),
        ("flac", "FLAC"),
        ("dts", "DTS"),
        ("mp3", "MP3"),
        ("mpeg-1 layer 3", "MP3"),
        ("truehd", "TrueHD"),
    ];
    Some(named.iter().find(|(key, _)| lower.contains(key)).map_or(codec, |(_, name)| (*name).to_string()))
}

fn track_title(track: &Track, index: usize, lang: Lang) -> String {
    track
        .title
        .clone()
        .filter(|title| !title.trim().is_empty())
        .or_else(|| language(track.language.as_deref(), lang).map(str::to_string))
        .unwrap_or_else(|| Key::Track.text(lang).replace("{n}", &(index + 1).to_string()))
}

fn icon(item: Item<Message>, glyph: Icon) -> Item<Message> {
    item.glyph(glyph.handle())
}

pub const POPOVER_LEAST: f32 = 212.0;
const POPOVER_GAP: f32 = 8.0;
const POPOVER_OVERHANG: f32 = 20.0;

fn above_hud(app: &App, button_right: f32, items: &[Item<Message>]) -> Point {
    let hud = crate::view::hud_rect(app.window.size);
    let width = glide_sdk::menu::width(items, POPOVER_LEAST);
    let height = glide_sdk::menu::height(items);
    let right = (hud.x + hud.width - button_right + POPOVER_OVERHANG).min(app.window.size.width - 8.0);
    Point::new((right - width).max(8.0), (hud.y - POPOVER_GAP - height).max(8.0))
}

#[must_use]
pub fn build(app: &App, anchor: Anchor) -> (Point, Vec<Item<Message>>) {
    let items = match anchor {
        Anchor::Speed => speed(app),
        Anchor::Tracks => tracks(app),
        Anchor::More => more(app),
        Anchor::Aspect => aspect(app),
        Anchor::Chapters => chapters(app),
    };
    let at = match anchor {
        Anchor::Speed => above_hud(app, 116.0, &items),
        Anchor::Tracks => above_hud(app, 82.0, &items),
        Anchor::More | Anchor::Aspect => above_hud(app, 48.0, &items),
        Anchor::Chapters => Point::new(54.0, 56.0),
    };
    (at, items)
}

#[must_use]
pub fn speed(app: &App) -> Vec<Item<Message>> {
    let (rate, keep) = app.media.as_ref().map_or((1.0, true), |media| (media.rate, media.keep_pitch));
    let mut items = vec![Item::heading(app.t(Key::SpeedLabel))];
    for speed in SPEEDS {
        items.push(
            icon(Item::action(i18n::speed(app.lang, speed), Message::SetSpeed(speed)), Icon::Speed)
                .checked((speed - rate).abs() < 0.01),
        );
    }
    items.push(Item::separator());
    items.push(icon(Item::action(app.t(Key::KeepPitch), Message::KeepPitch), Icon::Pitch).checked(keep));
    items
}

#[must_use]
pub fn tracks(app: &App) -> Vec<Item<Message>> {
    let lang = app.lang;
    let Some(media) = &app.media else { return Vec::new() };
    let chosen_text = media.selected_of(TrackKind::Text);
    let mut items = vec![Item::heading(app.t(Key::Subtitles))];
    items.push(
        icon(Item::action(app.t(Key::Off), Message::PickText(None)), Icon::SubtitlesOff).checked(chosen_text.is_none()),
    );
    for (index, track) in media.tracks.of(TrackKind::Text).enumerate() {
        let (glyph, detail) = if track.external {
            let name =
                media.suburi.as_deref().and_then(|uri| uri.rsplit('/').next()).map(|name| name.replace("%20", " "));
            (
                Icon::File,
                name.map_or_else(
                    || app.t(Key::NextToFile).to_string(),
                    |name| format!("{name} · {}", app.t(Key::NextToFile)),
                ),
            )
        } else {
            let codec = short_codec(track).map(|codec| format!(" · {codec}")).unwrap_or_default();
            (Icon::Subtitles, format!("{}{codec}", app.t(Key::Embedded)))
        };
        items.push(
            icon(Item::action(track_title(track, index, lang), Message::PickText(Some(track.id.clone()))), glyph)
                .detail(detail)
                .checked(chosen_text == Some(track.id.as_str())),
        );
    }
    items.push(icon(Item::action(app.t(Key::AddFromFile), Message::AddSubtitle), Icon::Plus));
    let offset = media.offset_ms as f64 / 1000.0;
    let shown = app.t(Key::Seconds).replace("{n}", &i18n::decimal(lang, offset, 1).replace('-', "−"));
    items.push(
        Item::stepper(
            app.t(Key::Shift),
            shown,
            Some(Message::SubtitleOffset(-100)),
            Some(Message::SubtitleOffset(100)),
        )
        .glyph(Icon::Clock.handle()),
    );
    let audio: Vec<&Track> = media.tracks.of(TrackKind::Audio).collect();
    if !audio.is_empty() {
        items.push(Item::separator());
        items.push(Item::heading(app.t(Key::Sound)));
        let chosen = media.selected_of(TrackKind::Audio);
        for (index, track) in audio.into_iter().enumerate() {
            let commentary = track.title.as_deref().is_some_and(|title| {
                let lower = title.to_lowercase();
                lower.contains("comment") || lower.contains("коментар")
            });
            let detail = [short_codec(track), channels(track.channels, lang)]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" · ");
            let mut item = icon(
                Item::action(track_title(track, index, lang), Message::PickAudio(track.id.clone())),
                if commentary { Icon::Mic } else { Icon::Speaker },
            )
            .checked(chosen == Some(track.id.as_str()));
            if !detail.is_empty() {
                item = item.detail(detail);
            }
            items.push(item);
        }
    }
    items
}

#[must_use]
pub fn chapters(app: &App) -> Vec<Item<Message>> {
    let Some(media) = &app.media else { return Vec::new() };
    let current = media.chapter();
    let mut items = vec![Item::heading(i18n::chapters(app.lang, media.chapters.len()))];
    for (index, chapter) in media.chapters.iter().enumerate() {
        let title = if chapter.title.is_empty() {
            app.t(Key::ChapterNumber).replace("{n}", &(index + 1).to_string())
        } else {
            chapter.title.clone()
        };
        let mut detail = glide_sdk::widget::player::clock(chapter.start, true);
        if current == Some(index) {
            detail = format!("{detail} · {}", app.t(Key::Now));
        }
        let mut item = Item::action(title, Message::GoChapter(index)).detail(detail).checked(current == Some(index));
        item = match media.chapter_frames.get(&index) {
            Some(frame) => item.picture(frame.clone()),
            None => item.glyph(Icon::Mark.handle()),
        };
        items.push(item);
    }
    items
}

#[must_use]
pub fn aspect(app: &App) -> Vec<Item<Message>> {
    let current = app.media.as_ref().map(|media| media.aspect).unwrap_or_default();
    let ratio = app
        .media
        .as_ref()
        .and_then(crate::app::Media::video_size)
        .map(|(width, height)| ratio_label(width, height, app.lang))
        .unwrap_or_default();
    vec![
        icon(Item::action(app.t(Key::AsInFile), Message::SetAspect(Aspect::File)), Icon::Aspect)
            .value(ratio)
            .checked(current == Aspect::File),
        icon(Item::action(app.t(Key::FillWindow), Message::SetAspect(Aspect::Fill)), Icon::Fill)
            .value(app.t(Key::CropsEdges))
            .checked(current == Aspect::Fill),
        icon(Item::action(app.t(Key::Cinema), Message::SetAspect(Aspect::Cinema)), Icon::Wide)
            .checked(current == Aspect::Cinema),
    ]
}

fn aspect_now(app: &App) -> String {
    let current = app.media.as_ref().map(|media| media.aspect).unwrap_or_default();
    let name = app.t(match current {
        Aspect::File => Key::AsInFile,
        Aspect::Fill => Key::FillWindow,
        Aspect::Cinema => Key::Cinema,
    });
    let mut letters = name.chars();
    letters.next().map_or_else(String::new, |first| first.to_lowercase().chain(letters).collect())
}

#[must_use]
pub fn ratio_label(width: u32, height: u32, lang: Lang) -> String {
    let known = [
        (16.0 / 9.0, "16:9"),
        (4.0 / 3.0, "4:3"),
        (21.0 / 9.0, "21:9"),
        (1.0, "1:1"),
        (9.0 / 16.0, "9:16"),
        (2.39, "2,39:1"),
    ];
    let ratio = width as f64 / height.max(1) as f64;
    known.iter().find(|(value, _)| (value - ratio).abs() < 0.03).map_or_else(
        || format!("{}:1", i18n::decimal(lang, ratio, 2)),
        |(_, label)| {
            if lang == Lang::En { label.replace(',', ".") } else { (*label).to_string() }
        },
    )
}

#[must_use]
pub fn more(app: &App) -> Vec<Item<Message>> {
    let Some(media) = &app.media else { return Vec::new() };
    let editable = media.path.is_some() && !media.live;
    vec![
        Item::heading(app.t(Key::View)),
        icon(Item::action(app.t(Key::PictureInPicture), Message::Pip), Icon::Pip).shortcut("Ctrl+Shift+P"),
        icon(Item::action(app.t(Key::OnTop), Message::ToggleOnTop), Icon::Pin).checked(media.on_top),
        icon(Item::action(app.t(Key::Repeat), Message::ToggleRepeat), Icon::Repeat).checked(media.repeat),
        Item::submenu(app.t(Key::AspectMenu), aspect(app)).glyph(Icon::Aspect.handle()).value(aspect_now(app)),
        Item::separator(),
        Item::heading(app.t(Key::Editing)),
        icon(Item::action(app.t(Key::TrimEllipsis), Message::EnterTrim), Icon::Scissors)
            .shortcut("Ctrl+T")
            .enabled(editable),
        icon(Item::action(app.t(Key::SplitClip), Message::SplitClip), Icon::Split).shortcut("Ctrl+Y").enabled(editable),
        icon(Item::action(app.t(Key::RotateLeft), Message::TurnClip(false)), Icon::RotateLeft)
            .shortcut("Ctrl+Shift+L")
            .enabled(editable),
        icon(Item::action(app.t(Key::FlipHorizontal), Message::FlipClip(true)), Icon::FlipH).enabled(editable),
        icon(Item::action(app.t(Key::ExportCopy), Message::ExportOpen), Icon::Export)
            .shortcut("Ctrl+E")
            .enabled(editable),
        Item::separator(),
        icon(Item::action(app.t(Key::Inspector), Message::Inspector(!app.inspector)), Icon::Info).shortcut("Ctrl+I"),
        icon(Item::action(app.t(Key::ShowInFiles), Message::ShowInFiles), Icon::Folder).enabled(media.path.is_some()),
        icon(Item::action(app.t(Key::Shortcuts), Message::Shortcuts), Icon::Keyboard).shortcut("F1"),
    ]
}

#[must_use]
pub fn context(app: &App) -> Vec<Item<Message>> {
    let Some(media) = &app.media else { return Vec::new() };
    let playing = media.playing();
    let video = media.showing_video();
    let time = glide_sdk::widget::player::clock(media.at(), true);
    vec![
        icon(
            Item::action(app.t(if playing { Key::Pause } else { Key::Play }), Message::TogglePlay),
            if playing { Icon::Pause } else { Icon::Play },
        )
        .shortcut(app.t(Key::Space)),
        icon(Item::action(app.t(Key::FrameBack), Message::Step(false)), Icon::FrameBack).shortcut("←"),
        Item::separator(),
        icon(Item::action(app.t(Key::CopyFrame), Message::CopyFrame), Icon::Copy)
            .shortcut("Ctrl+Shift+C")
            .enabled(video),
        icon(Item::action(app.t(Key::SaveFrame), Message::SaveFrame), Icon::Image).enabled(video),
        icon(Item::action(app.t(Key::TextOnFrame), Message::TextOnFrame), Icon::Text)
            .shortcut("Ctrl+Shift+T")
            .enabled(video),
        Item::separator(),
        icon(Item::action(app.t(Key::CopyTime).replace("{time}", &time), Message::CopyTime), Icon::Clock),
        icon(Item::action(app.t(Key::ShowInFiles), Message::ShowInFiles), Icon::Folder).enabled(media.path.is_some()),
        icon(Item::action(app.t(Key::OpenWith), Message::OpenWith), Icon::External),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_codes_get_names_in_both_languages() {
        assert_eq!(language(Some("ukr"), Lang::Uk), Some("Українська"));
        assert_eq!(language(Some("en-US"), Lang::En), Some("English"));
        assert_eq!(language(Some("xx"), Lang::Uk), None);
        assert_eq!(language(None, Lang::Uk), None);
    }

    #[test]
    fn ratios_are_named_when_they_are_common() {
        assert_eq!(ratio_label(3840, 2160, Lang::Uk), "16:9");
        assert_eq!(ratio_label(1440, 1080, Lang::En), "4:3");
        assert_eq!(ratio_label(1000, 300, Lang::En), "3.33:1");
    }

    #[test]
    fn channel_counts_read_like_people_say_them() {
        assert_eq!(channels(Some(6), Lang::Uk).as_deref(), Some("5.1"));
        assert_eq!(channels(Some(2), Lang::Uk).as_deref(), Some("стерео"));
        assert_eq!(channels(None, Lang::Uk), None);
    }
}

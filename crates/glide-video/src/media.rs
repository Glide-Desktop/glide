use std::time::Duration;

use gstreamer as gst;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Video,
    Audio,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    Missing { what: String, detail: Option<String> },
    NotFound,
    Broken(String),
    Network(String),
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chapter {
    pub start: Duration,
    pub title: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackKind {
    Video,
    Audio,
    Text,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Track {
    pub id: String,
    pub kind: TrackKind,
    pub language: Option<String>,
    pub title: Option<String>,
    pub codec: Option<String>,
    pub channels: Option<i32>,
    pub format: Option<String>,
    pub external: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Tracks(Vec<Track>);

impl Tracks {
    pub fn push(&mut self, track: Track) {
        self.0.push(track);
    }

    pub fn retain(&mut self, keep: impl FnMut(&Track) -> bool) {
        self.0.retain(keep);
    }

    pub fn all(&self) -> impl Iterator<Item = &Track> {
        self.0.iter()
    }

    pub fn of(&self, kind: TrackKind) -> impl Iterator<Item = &Track> {
        self.0.iter().filter(move |track| track.kind == kind)
    }

    pub fn video(&self) -> impl Iterator<Item = &Track> {
        self.of(TrackKind::Video)
    }

    #[must_use]
    pub fn get(&self, id: &str) -> Option<&Track> {
        self.0.iter().find(|track| track.id == id)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    #[must_use]
    pub fn selection(&self, current: &[String], kind: TrackKind, pick: Option<&str>) -> Vec<String> {
        let mut ids: Vec<String> =
            current.iter().filter(|id| self.get(id).is_some_and(|track| track.kind != kind)).cloned().collect();
        if let Some(pick) = pick.filter(|pick| self.get(pick).is_some_and(|track| track.kind == kind)) {
            ids.push(pick.to_string());
        }
        ids
    }
}

#[must_use]
pub fn classify(error: &gst::glib::Error, debug: Option<&str>) -> Failure {
    let message = error.message().to_string();
    if error.matches(gst::ResourceError::NotFound) {
        return Failure::NotFound;
    }
    if error.matches(gst::CoreError::MissingPlugin)
        || error.matches(gst::StreamError::CodecNotFound)
        || error.matches(gst::StreamError::WrongType)
    {
        return Failure::Missing { what: message, detail: None };
    }
    if error.matches(gst::ResourceError::OpenRead)
        && debug
            .is_some_and(|debug| debug.contains("souphttpsrc") || debug.contains("http") || debug.contains("network"))
    {
        return Failure::Network(message);
    }
    if error.matches(gst::ResourceError::Read)
        && debug.is_some_and(|debug| debug.contains("http") || debug.contains("soup"))
    {
        return Failure::Network(message);
    }
    if error.matches(gst::StreamError::Decode)
        || error.matches(gst::StreamError::Demux)
        || error.matches(gst::StreamError::Format)
        || error.matches(gst::StreamError::TypeNotFound)
        || error.matches(gst::StreamError::Failed)
    {
        return Failure::Broken(message);
    }
    Failure::Other(message)
}

fn unescape(text: &str) -> String {
    text.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

#[must_use]
pub fn plain_text(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut tag = false;
    for character in raw.chars() {
        match character {
            '<' => tag = true,
            '>' if tag => tag = false,
            _ if tag => {}
            '\r' => {}
            _ => out.push(character),
        }
    }
    unescape(out.trim())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub start: Duration,
    pub end: Duration,
    pub text: String,
}

fn ass_time(raw: &str) -> Option<Duration> {
    let mut parts = raw.trim().split(':');
    let hours: u64 = parts.next()?.trim().parse().ok()?;
    let minutes: u64 = parts.next()?.trim().parse().ok()?;
    let seconds = parts.next()?.trim();
    if parts.next().is_some() {
        return None;
    }
    let (whole, fraction) = seconds.split_once('.').unwrap_or((seconds, "0"));
    let whole: u64 = whole.parse().ok()?;
    let digits: String = fraction.chars().take(3).collect();
    let millis: u64 = format!("{digits:0<3}").parse().ok()?;
    Some(Duration::from_millis(((hours * 60 + minutes) * 60 + whole) * 1000 + millis))
}

fn cue_time(raw: &str) -> Option<Duration> {
    let stamp = raw.trim().replace(',', ".");
    if stamp.matches(':').count() == 1 { ass_time(&format!("0:{stamp}")) } else { ass_time(&stamp) }
}

#[must_use]
pub fn cue_script(script: &str) -> Vec<Line> {
    let mut lines = Vec::new();
    let mut rows = script.lines().map(|row| row.trim_start_matches('\u{feff}').trim_end_matches('\r'));
    while let Some(row) = rows.next() {
        let Some((from, to)) = row.split_once("-->") else { continue };
        let to = to.split_whitespace().next().unwrap_or_default();
        let (Some(start), Some(end)) = (cue_time(from), cue_time(to)) else { continue };
        let body: Vec<&str> = rows.by_ref().take_while(|row| !row.trim().is_empty()).collect();
        let text = plain_text(&body.join("\n"));
        if end > start && !text.is_empty() {
            lines.push(Line { start, end, text });
        }
    }
    lines.sort_by_key(|line| line.start);
    lines
}

#[must_use]
pub fn ass_script(script: &str) -> Vec<Line> {
    let mut events = false;
    let mut format: Vec<String> = Vec::new();
    let mut lines = Vec::new();
    for raw in script.lines() {
        let line = raw.trim_start_matches('\u{feff}').trim();
        if line.starts_with('[') {
            events = line.eq_ignore_ascii_case("[events]");
            continue;
        }
        if !events {
            continue;
        }
        if let Some(fields) = line.strip_prefix("Format:") {
            format = fields.split(',').map(|field| field.trim().to_ascii_lowercase()).collect();
            continue;
        }
        let Some(fields) = line.strip_prefix("Dialogue:") else { continue };
        let count = if format.is_empty() { 10 } else { format.len() };
        let values: Vec<&str> = fields.splitn(count, ',').collect();
        let index = |name: &str, fallback: usize| format.iter().position(|field| field == name).unwrap_or(fallback);
        let (Some(start), Some(end), Some(text)) = (
            values.get(index("start", 1)).and_then(|value| ass_time(value)),
            values.get(index("end", 2)).and_then(|value| ass_time(value)),
            values.get(index("text", count - 1)),
        ) else {
            continue;
        };
        let text = ass_text(text);
        if end > start && !text.is_empty() {
            lines.push(Line { start, end, text });
        }
    }
    lines.sort_by_key(|line| line.start);
    lines
}

#[must_use]
pub fn text_at(lines: &[Line], at: Duration) -> Option<String> {
    let started = lines.partition_point(|line| line.start <= at);
    let first = started.saturating_sub(64);
    let showing: Vec<&str> =
        lines[first..started].iter().filter(|line| at < line.end).map(|line| line.text.as_str()).collect();
    (!showing.is_empty()).then(|| showing.join("\n"))
}

#[must_use]
pub fn ass_text(raw: &str) -> String {
    let line = raw.trim();
    let body = if let Some(rest) = line.strip_prefix("Dialogue:") {
        rest.splitn(10, ',').nth(9).unwrap_or("")
    } else if line.split(',').next().is_some_and(|first| first.trim().parse::<u32>().is_ok())
        && line.matches(',').count() >= 8
    {
        line.splitn(9, ',').nth(8).unwrap_or("")
    } else {
        line
    };
    let mut out = String::with_capacity(body.len());
    let mut block = false;
    for character in body.chars() {
        match character {
            '{' => block = true,
            '}' if block => block = false,
            _ if block => {}
            _ => out.push(character),
        }
    }
    out.replace("\\N", "\n").replace("\\n", "\n").replace("\\h", " ").trim().to_string()
}

fn walk(entries: &[gst::TocEntry], into: &mut Vec<Chapter>) {
    for entry in entries {
        if entry.entry_type() == gst::TocEntryType::Chapter
            && let Some((start, _)) = entry.start_stop_times()
        {
            let title = entry
                .tags()
                .and_then(|tags| tags.get::<gst::tags::Title>().map(|title| title.get().to_string()))
                .unwrap_or_default();
            into.push(Chapter { start: Duration::from_nanos(start.max(0) as u64), title });
        }
        walk(&entry.sub_entries(), into);
    }
}

#[must_use]
pub fn chapters(toc: &gst::Toc) -> Vec<Chapter> {
    let mut found = Vec::new();
    walk(&toc.entries(), &mut found);
    found.sort_by_key(|chapter| chapter.start);
    found.dedup_by_key(|chapter| chapter.start);
    found
}

#[must_use]
pub fn chapter_at(chapters: &[Chapter], at: Duration) -> Option<usize> {
    chapters.iter().rposition(|chapter| chapter.start <= at)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCRIPT: &str = "\u{feff}[Script Info]\nTitle: x\n\n[V4+ Styles]\nFormat: Name, Fontname\nStyle: Default,Sans\n\n\
        [Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n\
        Comment: 0,0:00:00.00,0:00:09.00,Default,,0,0,0,,not shown\n\
        Dialogue: 0,0:00:03.50,0:00:05.00,Default,,0,0,0,,{\\an8}Угорі, з комою\n\
        Dialogue: 0,0:00:01.20,0:00:04.00,Default,,0,0,0,,{\\i1}Дивись{\\i0},\\Nтуман\n\
        Dialogue: 0,0:00:07.00,0:00:06.00,Default,,0,0,0,,backwards\n";

    #[test]
    fn an_ass_script_becomes_timed_plain_lines() {
        let lines = ass_script(SCRIPT);
        assert_eq!(lines.len(), 2, "comments and lines that end before they start are dropped");
        assert_eq!(lines[0].start, Duration::from_millis(1200));
        assert_eq!(lines[0].text, "Дивись,\nтуман");
        assert_eq!(lines[1].text, "Угорі, з комою", "commas inside the text stay");
        assert_eq!(text_at(&lines, Duration::from_millis(500)), None);
        assert_eq!(text_at(&lines, Duration::from_secs(2)).as_deref(), Some("Дивись,\nтуман"));
        assert_eq!(text_at(&lines, Duration::from_millis(3700)).as_deref(), Some("Дивись,\nтуман\nУгорі, з комою"));
        assert_eq!(text_at(&lines, Duration::from_millis(4500)).as_deref(), Some("Угорі, з комою"));
        assert_eq!(text_at(&lines, Duration::from_secs(6)), None);
    }

    #[test]
    fn ass_times_read_centiseconds_and_hours() {
        assert_eq!(ass_time("1:02:03.45"), Some(Duration::from_millis(3_723_450)));
        assert_eq!(ass_time("0:00:00.5"), Some(Duration::from_millis(500)));
        assert_eq!(ass_time("nonsense"), None);
    }

    #[test]
    fn subrip_and_webvtt_files_become_timed_lines() {
        let srt = "\u{feff}1\r\n00:00:00,200 --> 00:00:01,500\r\n<i>Дивись, туман</i>\r\n\r\n\
                   2\r\n00:01:02,000 --> 00:01:03,250\r\nсходить\r\nпросто під нами\r\n";
        assert_eq!(
            cue_script(srt),
            vec![
                Line {
                    start: Duration::from_millis(200),
                    end: Duration::from_millis(1500),
                    text: "Дивись, туман".into()
                },
                Line {
                    start: Duration::from_secs(62),
                    end: Duration::from_millis(63_250),
                    text: "сходить\nпросто під нами".into()
                },
            ]
        );
        let vtt = "WEBVTT\n\nNOTE опис\n\n00:01.000 --> 00:02.500 align:start\n<c.yellow>Угорі</c>\n";
        assert_eq!(
            cue_script(vtt),
            vec![Line { start: Duration::from_secs(1), end: Duration::from_millis(2500), text: "Угорі".into() }]
        );
        assert!(cue_script("00:00:05,000 --> 00:00:04,000\nназад\n").is_empty(), "a cue that ends first is dropped");
    }

    #[test]
    fn srt_markup_loses_tags_and_entities() {
        assert_eq!(plain_text("<i>Дивись, туман</i> &amp; сонце\r\n"), "Дивись, туман & сонце");
    }

    #[test]
    fn matroska_ass_block_keeps_only_the_text() {
        let block = "12,0,Default,,0,0,0,,{\\i1}Туман{\\i0} сходить\\Nпросто під нами";
        assert_eq!(ass_text(block), "Туман сходить\nпросто під нами");
    }

    #[test]
    fn ass_dialogue_line_keeps_commas_inside_the_text() {
        let line = "Dialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,Ні, ще ні, зачекай";
        assert_eq!(ass_text(line), "Ні, ще ні, зачекай");
    }

    #[test]
    fn plain_text_without_commas_passes_through_ass() {
        assert_eq!(ass_text("Просто текст"), "Просто текст");
    }

    #[test]
    fn chapter_lookup_takes_the_last_started_one() {
        let chapters = vec![
            Chapter { start: Duration::ZERO, title: "a".into() },
            Chapter { start: Duration::from_mins(15), title: "b".into() },
            Chapter { start: Duration::from_mins(35), title: "c".into() },
        ];
        assert_eq!(chapter_at(&chapters, Duration::from_secs(2534)), Some(2));
        assert_eq!(chapter_at(&chapters, Duration::from_secs(899)), Some(0));
        assert_eq!(chapter_at(&[], Duration::from_secs(1)), None);
    }

    #[test]
    fn selection_swaps_only_the_chosen_kind() {
        let mut tracks = Tracks::default();
        for (id, kind) in
            [("v", TrackKind::Video), ("a1", TrackKind::Audio), ("a2", TrackKind::Audio), ("t", TrackKind::Text)]
        {
            tracks.push(Track {
                id: id.into(),
                kind,
                language: None,
                title: None,
                codec: None,
                channels: None,
                format: None,
                external: false,
            });
        }
        let current = vec!["v".to_string(), "a1".to_string(), "t".to_string()];
        assert_eq!(tracks.selection(&current, TrackKind::Audio, Some("a2")), vec!["v", "t", "a2"]);
        assert_eq!(tracks.selection(&current, TrackKind::Text, None), vec!["v", "a1"]);
        assert_eq!(tracks.selection(&current, TrackKind::Text, Some("a2")), vec!["v", "a1"]);
    }
}

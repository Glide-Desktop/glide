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

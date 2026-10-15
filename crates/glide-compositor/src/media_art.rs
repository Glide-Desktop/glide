use std::sync::mpsc;

const MEDIA_ART_MAX_BYTES: u64 = glide_media::ART_MAX_BYTES;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DecodedArt {
    pub art: Option<Vec<u8>>,
    pub art_large: Option<Vec<u8>>,
    pub dominant: Option<glide_theme::Color>,
}

impl DecodedArt {
    #[must_use]
    pub const fn is_hit(&self) -> bool {
        self.art.is_some()
    }
}

#[derive(Debug, Default)]
pub struct MediaArt {
    cache: Option<(String, DecodedArt)>,
    in_flight: Option<String>,
    shown: Option<(String, crate::topbar::MediaWidget)>,
    worker: Option<mpsc::Sender<String>>,
}

impl MediaArt {
    pub fn attach(&mut self, worker: mpsc::Sender<String>) {
        self.worker = Some(worker);
    }

    pub fn show(
        &mut self, track: &glide_media::Track, palette: (glide_theme::Appearance, glide_theme::Accent),
    ) -> crate::topbar::MediaWidget {
        let url = track.art_url.trim().to_string();
        let cached = self.cache.as_ref().filter(|(cached, _)| *cached == url).map(|(_, art)| art.clone());
        let bare = bare_widget(track, palette);
        let widget = if let Some(art) = cached {
            with_art(bare.clone(), art)
        } else {
            self.request(&url);
            bare.clone()
        };
        self.shown = Some((url, bare));
        widget
    }

    pub fn clear(&mut self) {
        self.shown = None;
    }

    pub fn set_progress(&mut self, progress: Option<f32>) {
        if let Some((_, widget)) = self.shown.as_mut() {
            widget.progress = progress;
        }
    }

    pub fn accept(&mut self, url: &str, art: DecodedArt) -> Option<crate::topbar::MediaWidget> {
        if self.in_flight.as_deref() == Some(url) {
            self.in_flight = None;
        }
        if !art.is_hit() {
            return None;
        }
        self.cache = Some((url.to_string(), art.clone()));
        let (shown_url, bare) = self.shown.as_ref()?;
        (shown_url == url).then(|| with_art(bare.clone(), art))
    }

    fn request(&mut self, url: &str) {
        if url.is_empty() || self.in_flight.as_deref() == Some(url) {
            return;
        }
        let Some(worker) = self.worker.as_ref() else {
            return;
        };
        if worker.send(url.to_string()).is_ok() {
            self.in_flight = Some(url.to_string());
        } else {
            tracing::warn!("the cover art decoder is gone; covers will not be shown");
            self.worker = None;
        }
    }
}

fn bare_widget(
    track: &glide_media::Track, palette: (glide_theme::Appearance, glide_theme::Accent),
) -> crate::topbar::MediaWidget {
    crate::topbar::MediaWidget {
        title: track.display_title().to_string(),
        artist: track.artist.clone(),
        album: track.album.clone(),
        length: track.length.filter(|length| !length.is_zero()),
        source: track.identity.clone(),
        art: None,
        art_large: None,
        app_icons: glide_media::player_icon_candidates(&track.bus, &track.desktop_entry),
        accent: glide_theme::Tokens::new(palette.0, palette.1).acc_text,
        playing: track.status == glide_media::Playback::Playing,
        can_next: track.can_go_next,
        can_previous: track.can_go_previous,
        progress: track.progress(),
    }
}

fn with_art(mut widget: crate::topbar::MediaWidget, art: DecodedArt) -> crate::topbar::MediaWidget {
    if let Some(colour) = art.dominant {
        widget.accent = crate::paint::fit_for_contrast(colour, 0.38, 0.82);
    }
    widget.art = art.art;
    widget.art_large = art.art_large;
    widget
}

fn read_art(url: &str) -> Option<Vec<u8>> {
    let path = glide_media::local_art_path(url)?;
    let size = std::fs::metadata(&path).ok()?.len();
    if size == 0 || size > MEDIA_ART_MAX_BYTES {
        return None;
    }
    std::fs::read(path).ok()
}

#[must_use]
pub fn decode(url: &str) -> DecodedArt {
    let bytes = read_art(url);
    let art = bytes.as_deref().and_then(|bytes| crate::paint::decode_cover(bytes, crate::topbar::MEDIA_ART));
    DecodedArt {
        dominant: art.as_deref().and_then(crate::paint::dominant_colour),
        art_large: bytes.as_deref().and_then(|bytes| crate::paint::decode_cover(bytes, crate::topbar::MEDIA_ART_LARGE)),
        art,
    }
}

fn latest(receiver: &mpsc::Receiver<String>, first: String) -> String {
    receiver.try_iter().last().unwrap_or(first)
}

pub fn start(sink: impl Fn(String, DecodedArt) + Send + 'static) -> Option<mpsc::Sender<String>> {
    let (sender, receiver) = mpsc::channel::<String>();
    let spawned = std::thread::Builder::new().name("glide-art-decode".into()).spawn(move || {
        while let Ok(first) = receiver.recv() {
            let url = latest(&receiver, first);
            let art = decode(&url);
            sink(url, art);
        }
    });
    spawned.inspect_err(|err| tracing::warn!(?err, "failed to start the cover art decoder")).ok().map(|_| sender)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(art_url: &str) -> glide_media::Track {
        glide_media::Track {
            bus: "org.mpris.MediaPlayer2.test".into(),
            title: "Song".into(),
            art_url: art_url.into(),
            ..glide_media::Track::default()
        }
    }

    fn hit() -> DecodedArt {
        DecodedArt { art: Some(vec![1, 2, 3, 4]), art_large: Some(vec![5, 6, 7, 8]), dominant: None }
    }

    const PALETTE: (glide_theme::Appearance, glide_theme::Accent) =
        (glide_theme::Appearance::Dark, glide_theme::Accent::Blue);

    fn wired() -> (MediaArt, mpsc::Receiver<String>) {
        let (sender, receiver) = mpsc::channel();
        let mut art = MediaArt::default();
        art.attach(sender);
        (art, receiver)
    }

    #[test]
    fn an_uncached_cover_is_shown_without_art_and_decoded_off_thread() {
        let (mut art, requests) = wired();
        let widget = art.show(&track("file:///a.png"), PALETTE);
        assert!(widget.art.is_none());
        assert_eq!(requests.try_recv().as_deref(), Ok("file:///a.png"));
    }

    #[test]
    fn a_repeated_track_event_does_not_queue_a_second_decode() {
        let (mut art, requests) = wired();
        art.show(&track("file:///a.png"), PALETTE);
        art.show(&track("file:///a.png"), PALETTE);
        assert!(requests.try_recv().is_ok());
        assert!(requests.try_recv().is_err());
    }

    #[test]
    fn decoded_art_for_the_current_track_updates_the_widget() {
        let (mut art, _requests) = wired();
        art.show(&track("file:///a.png"), PALETTE);
        let widget = art.accept("file:///a.png", hit()).expect("the shown track should pick up its art");
        assert_eq!(widget.art, hit().art);
        assert_eq!(widget.art_large, hit().art_large);
    }

    #[test]
    fn a_late_result_for_a_previous_track_is_dropped() {
        let (mut art, _requests) = wired();
        art.show(&track("file:///old.png"), PALETTE);
        art.show(&track("file:///new.png"), PALETTE);
        assert_eq!(art.accept("file:///old.png", hit()), None);
    }

    #[test]
    fn a_result_after_the_player_went_away_is_dropped() {
        let (mut art, _requests) = wired();
        art.show(&track("file:///a.png"), PALETTE);
        art.clear();
        assert_eq!(art.accept("file:///a.png", hit()), None);
    }

    #[test]
    fn a_decoded_cover_is_served_from_the_cache_next_time() {
        let (mut art, requests) = wired();
        art.show(&track("file:///a.png"), PALETTE);
        let _ = requests.try_recv();
        art.accept("file:///a.png", hit());
        let widget = art.show(&track("file:///a.png"), PALETTE);
        assert_eq!(widget.art, hit().art);
        assert!(requests.try_recv().is_err(), "a cache hit must not re-decode");
    }

    #[test]
    fn a_miss_is_not_cached_so_a_lazily_written_file_is_retried() {
        let (mut art, requests) = wired();
        art.show(&track("file:///a.png"), PALETTE);
        let _ = requests.try_recv();
        assert_eq!(art.accept("file:///a.png", DecodedArt::default()), None);
        art.show(&track("file:///a.png"), PALETTE);
        assert_eq!(requests.try_recv().as_deref(), Ok("file:///a.png"));
    }

    #[test]
    fn a_track_without_art_never_queues_a_decode() {
        let (mut art, requests) = wired();
        art.show(&track(""), PALETTE);
        assert!(requests.try_recv().is_err());
    }

    #[test]
    fn progress_changes_survive_the_art_arriving() {
        let (mut art, _requests) = wired();
        art.show(&track("file:///a.png"), PALETTE);
        art.set_progress(Some(0.5));
        let widget = art.accept("file:///a.png", hit()).expect("current track");
        assert_eq!(widget.progress, Some(0.5));
    }

    #[test]
    fn the_worker_only_decodes_the_newest_queued_request() {
        let (sender, receiver) = mpsc::channel();
        sender.send("b".to_string()).unwrap();
        sender.send("c".to_string()).unwrap();
        assert_eq!(latest(&receiver, "a".to_string()), "c");
    }

    #[test]
    fn an_oversized_or_missing_file_decodes_to_a_miss() {
        assert!(!decode("file:///definitely/not/here.png").is_hit());
        assert!(!decode("https://example.invalid/cover.png").is_hit());
    }
}

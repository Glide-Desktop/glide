use std::time::Duration;

pub const BUS_PREFIX: &str = "org.mpris.MediaPlayer2.";
pub const OBJECT_PATH: &str = "/org/mpris/MediaPlayer2";
pub const PLAYER_INTERFACE: &str = "org.mpris.MediaPlayer2.Player";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Playback {
    Playing,
    Paused,
    #[default]
    Stopped,
}

impl Playback {
    #[must_use]
    pub fn from_wire(value: &str) -> Self {
        match value {
            "Playing" => Self::Playing,
            "Paused" => Self::Paused,
            _ => Self::Stopped,
        }
    }

    #[must_use]
    pub const fn is_active(self) -> bool {
        matches!(self, Self::Playing | Self::Paused)
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Track {
    pub bus: String,
    pub identity: String,
    pub desktop_entry: String,
    pub track_id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub art_url: String,
    pub length: Option<Duration>,
    pub position: Option<Duration>,
    pub status: Playback,
    pub can_go_next: bool,
    pub can_go_previous: bool,
    pub can_pause: bool,
}

impl Track {
    #[must_use]
    pub fn display_title(&self) -> &str {
        Some(self.title.trim()).filter(|title| !title.is_empty()).unwrap_or("Unknown")
    }

    #[must_use]
    pub fn progress(&self) -> Option<f32> {
        let length = self.length?;
        let position = self.position?;
        if length.is_zero() {
            return None;
        }
        let fraction = (position.as_secs_f64() / length.as_secs_f64()) as f32;
        Some(fraction.clamp(0.0, 1.0))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum MediaEvent {
    Changed(Box<Track>),
    Progress(Option<f32>),
    Gone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    PlayPause,
    Play,
    Pause,
    Stop,
    Next,
    Previous,
}

#[must_use]
pub fn seek_target(length: Option<Duration>, fraction: f32) -> Option<Duration> {
    let length = length?;
    if length.is_zero() {
        return None;
    }
    let clamped = f64::from(fraction.clamp(0.0, 1.0));
    Some(Duration::from_secs_f64(length.as_secs_f64() * clamped))
}

#[must_use]
pub fn is_player_bus(name: &str) -> bool {
    name.starts_with(BUS_PREFIX) && name.len() > BUS_PREFIX.len()
}

fn bus_head(name: &str) -> &str {
    let tail = name.strip_prefix(BUS_PREFIX).unwrap_or(name);
    let tail = tail.split('.').next().unwrap_or(tail);
    tail.split_once("instance").map_or(tail, |(head, _)| head)
}

#[must_use]
pub fn identity_from_bus(name: &str) -> String {
    let tail = bus_head(name).trim_matches('-');
    if tail.is_empty() {
        return name.to_string();
    }
    let mut chars = tail.chars();
    match chars.next() {
        Some(first) if first.is_lowercase() => first.to_uppercase().collect::<String>() + chars.as_str(),
        _ => tail.to_string(),
    }
}

#[must_use]
pub fn player_icon_candidates(bus: &str, desktop_entry: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |candidate: &str| {
        let candidate = candidate.trim();
        if !candidate.is_empty() && !out.iter().any(|seen| seen == candidate) {
            out.push(candidate.to_string());
        }
    };
    push(desktop_entry.trim_end_matches(".desktop"));
    let head = bus_head(bus).trim_matches(['-', '_']);
    push(&head.to_lowercase());
    push(head);
    out
}

#[must_use]
pub fn artists_to_line(artists: &[String]) -> String {
    artists.iter().map(|artist| artist.trim()).filter(|artist| !artist.is_empty()).collect::<Vec<_>>().join(", ")
}

#[must_use]
pub fn better_candidate(current: Option<&Track>, candidate: &Track) -> bool {
    let Some(current) = current else {
        return candidate.status.is_active();
    };
    if current.bus == candidate.bus {
        return true;
    }
    match (current.status, candidate.status) {
        (Playback::Playing, _) => false,
        (_, Playback::Playing) => true,
        (_, _) => !current.status.is_active() && candidate.status.is_active(),
    }
}

#[must_use]
pub fn local_art_path(art_url: &str) -> Option<std::path::PathBuf> {
    let trimmed = art_url.trim();
    let path = trimmed.strip_prefix("file://")?;
    let path = path.strip_prefix("localhost").unwrap_or(path);
    let decoded = percent_decode(path);
    let path = std::path::PathBuf::from(decoded);
    path.is_file().then_some(path)
}

fn percent_decode(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 3 <= bytes.len() {
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).ok();
            if let Some(value) = hex.and_then(|hex| u8::from_str_radix(hex, 16).ok()) {
                out.push(value);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_player_bus_name_needs_more_than_the_bare_prefix() {
        assert!(is_player_bus("org.mpris.MediaPlayer2.rhythmbox"));
        assert!(!is_player_bus("org.mpris.MediaPlayer2."));
        assert!(!is_player_bus("org.freedesktop.Notifications"));
    }

    #[test]
    fn the_identity_is_derived_from_the_bus_tail() {
        assert_eq!(identity_from_bus("org.mpris.MediaPlayer2.rhythmbox"), "Rhythmbox");
        assert_eq!(identity_from_bus("org.mpris.MediaPlayer2.VLC"), "VLC");
    }

    #[test]
    fn an_instance_suffix_is_stripped_from_the_identity() {
        assert_eq!(identity_from_bus("org.mpris.MediaPlayer2.chromium.instance1234"), "Chromium");
    }

    #[test]
    fn artists_join_into_one_line_and_blanks_fall_away() {
        let artists = vec!["Hor".to_string(), String::new(), " Duet ".to_string()];
        assert_eq!(artists_to_line(&artists), "Hor, Duet");
        assert_eq!(artists_to_line(&[]), "");
    }

    #[test]
    fn progress_needs_both_a_length_and_a_position() {
        let mut track = Track { length: Some(Duration::from_secs(200)), ..Track::default() };
        assert_eq!(track.progress(), None);

        track.position = Some(Duration::from_secs(50));
        assert_eq!(track.progress(), Some(0.25));
    }

    #[test]
    fn a_zero_length_track_reports_no_progress() {
        let track = Track { length: Some(Duration::ZERO), position: Some(Duration::from_secs(1)), ..Track::default() };
        assert_eq!(track.progress(), None);
    }

    #[test]
    fn progress_past_the_end_is_clamped() {
        let track = Track {
            length: Some(Duration::from_secs(10)),
            position: Some(Duration::from_secs(40)),
            ..Track::default()
        };
        assert_eq!(track.progress(), Some(1.0));
    }

    #[test]
    fn a_missing_title_falls_back_rather_than_rendering_empty() {
        let track = Track::default();
        assert_eq!(track.display_title(), "Unknown");
    }

    fn player(bus: &str, status: Playback) -> Track {
        Track { bus: bus.to_string(), status, ..Track::default() }
    }

    #[test]
    fn the_first_active_player_is_adopted() {
        assert!(!better_candidate(None, &player("a", Playback::Stopped)));
        assert!(better_candidate(None, &player("a", Playback::Playing)));
    }

    #[test]
    fn a_playing_player_wins_over_a_paused_one() {
        let paused = player("a", Playback::Paused);
        let playing = player("b", Playback::Playing);
        assert!(better_candidate(Some(&paused), &playing));
        assert!(!better_candidate(Some(&playing), &paused));
    }

    #[test]
    fn the_current_player_always_updates_itself() {
        let current = player("a", Playback::Playing);
        assert!(better_candidate(Some(&current), &player("a", Playback::Paused)));
    }

    #[test]
    fn only_local_file_art_is_accepted() {
        assert!(local_art_path("https://example.com/cover.jpg").is_none());
        assert!(local_art_path("").is_none());
        assert!(local_art_path("file:///definitely/not/here.png").is_none());
    }

    #[test]
    fn a_percent_escaped_art_path_is_decoded() {
        assert_eq!(percent_decode("/tmp/my%20cover.png"), "/tmp/my cover.png");
        assert_eq!(percent_decode("/plain/path.png"), "/plain/path.png");
        assert_eq!(percent_decode("/trailing%"), "/trailing%");
        assert_eq!(percent_decode("/bad%zz"), "/bad%zz");
        assert_eq!(percent_decode("/end%20"), "/end ");
    }

    #[test]
    fn player_icons_prefer_the_desktop_entry_then_the_bus_name() {
        assert_eq!(
            player_icon_candidates("org.mpris.MediaPlayer2.firefox.instance_1_23", "firefox"),
            vec!["firefox".to_string()]
        );
        assert_eq!(
            player_icon_candidates("org.mpris.MediaPlayer2.chromium.instance4521", ""),
            vec!["chromium".to_string()]
        );
        assert_eq!(
            player_icon_candidates("org.mpris.MediaPlayer2.Spotify", "com.spotify.Client"),
            vec!["com.spotify.Client".to_string(), "spotify".to_string(), "Spotify".to_string()]
        );
    }

    #[test]
    fn seeking_maps_a_fraction_onto_the_track_length() {
        let length = Some(Duration::from_secs(200));
        assert_eq!(seek_target(length, 0.0), Some(Duration::ZERO));
        assert_eq!(seek_target(length, 0.5), Some(Duration::from_secs(100)));
        assert_eq!(seek_target(length, 1.0), Some(Duration::from_secs(200)));
    }

    #[test]
    fn seeking_outside_the_track_is_clamped_to_its_ends() {
        let length = Some(Duration::from_mins(1));
        assert_eq!(seek_target(length, -3.0), Some(Duration::ZERO));
        assert_eq!(seek_target(length, 9.0), Some(Duration::from_mins(1)));
    }

    #[test]
    fn a_track_of_unknown_or_zero_length_cannot_be_sought() {
        assert_eq!(seek_target(None, 0.5), None);
        assert_eq!(seek_target(Some(Duration::ZERO), 0.5), None);
    }

    #[test]
    fn commands_are_named_like_their_mpris_methods() {
        assert_eq!(format!("{:?}", Command::PlayPause), "PlayPause");
    }
}

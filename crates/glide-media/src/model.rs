use std::time::Duration;

pub(crate) const BUS_PREFIX: &str = "org.mpris.MediaPlayer2.";
pub(crate) const OBJECT_PATH: &str = "/org/mpris/MediaPlayer2";
pub(crate) const PLAYER_INTERFACE: &str = "org.mpris.MediaPlayer2.Player";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Playback {
    Playing,
    Paused,
    #[default]
    Stopped,
}

impl Playback {
    #[must_use]
    pub(crate) fn from_wire(value: &str) -> Self {
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

#[allow(clippy::struct_excessive_bools)] // Independent MPRIS capability flags (CanGoNext, CanGoPrevious, CanSeek, CanControl)
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
    pub can_seek: bool,
    pub can_control: bool,
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
pub(crate) fn seek_target(length: Option<Duration>, fraction: f32) -> Option<Duration> {
    let length = length?;
    if length.is_zero() || fraction.is_nan() {
        return None;
    }
    let clamped = f64::from(fraction.clamp(0.0, 1.0));
    Some(Duration::from_secs_f64(length.as_secs_f64() * clamped))
}

#[must_use]
pub(crate) fn is_player_bus(name: &str) -> bool {
    name.starts_with(BUS_PREFIX) && name.len() > BUS_PREFIX.len()
}

fn bus_tail(name: &str) -> &str {
    name.strip_prefix(BUS_PREFIX).unwrap_or(name)
}

fn numbers_an_instance(segment: &str) -> bool {
    segment
        .strip_prefix("instance")
        .is_some_and(|number| number.chars().all(|c| c.is_ascii_digit() || matches!(c, '_' | '-')))
}

fn bus_head(name: &str) -> String {
    bus_tail(name)
        .split('.')
        .filter(|segment| !numbers_an_instance(segment))
        .filter(|segment| !segment.trim_matches(['-', '_']).is_empty())
        .collect::<Vec<_>>()
        .join(".")
}

fn bus_is_one_of(name: &str, heads: &[&str]) -> bool {
    let tail = bus_tail(name);
    heads.iter().any(|head| tail == *head || tail.strip_prefix(head).is_some_and(|rest| rest.starts_with('.')))
}

#[must_use]
pub(crate) fn is_mirror_bus(name: &str) -> bool {
    bus_is_one_of(name, &["playerctld", "plasma-browser-integration"])
}

#[must_use]
pub(crate) fn is_relay_bus(name: &str) -> bool {
    is_mirror_bus(name) || bus_is_one_of(name, &["kdeconnect"])
}

#[must_use]
pub(crate) fn identity_or_bus(identity: &str, bus: &str) -> String {
    let identity = identity.trim();
    if identity.is_empty() { identity_from_bus(bus) } else { identity.to_string() }
}

#[must_use]
pub(crate) fn identity_from_bus(name: &str) -> String {
    let head = bus_head(name);
    let tail = head.rsplit('.').next().unwrap_or_default().trim_matches('-');
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
    let head = bus_head(bus);
    let head = head.trim_matches(['-', '_']);
    push(&head.to_lowercase());
    push(head);
    out
}

#[must_use]
pub(crate) fn artists_to_line(artists: &[String]) -> String {
    artists.iter().map(|artist| artist.trim()).filter(|artist| !artist.is_empty()).collect::<Vec<_>>().join(", ")
}

#[must_use]
pub(crate) fn better_candidate(current: Option<&Track>, candidate: &Track) -> bool {
    let Some(current) = current else {
        return candidate.status.is_active();
    };
    if current.bus == candidate.bus {
        return true;
    }
    match (current.status, candidate.status) {
        (current_status, candidate_status) if current_status == candidate_status => {
            candidate.can_control && !current.can_control
        }
        (Playback::Playing, _) => false,
        (_, Playback::Playing) => true,
        (_, _) => !current.status.is_active() && candidate.status.is_active(),
    }
}

#[must_use]
pub fn local_art_path(art_url: &str) -> Option<std::path::PathBuf> {
    glide_config::uri::local_path(art_url).filter(|path| path.is_file())
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
    fn only_a_numbered_instance_part_is_dropped_from_a_bus_name() {
        assert_eq!(identity_from_bus("org.mpris.MediaPlayer2.instancer"), "Instancer");
        assert_eq!(identity_from_bus("org.mpris.MediaPlayer2.org.example.reinstance"), "Reinstance");
        assert_eq!(identity_from_bus("org.mpris.MediaPlayer2.mpv.instance"), "Mpv");
        assert_eq!(
            player_icon_candidates("org.mpris.MediaPlayer2.org.example.Instances.instance-7", ""),
            vec!["org.example.instances".to_string(), "org.example.Instances".to_string()]
        );
    }

    #[test]
    fn a_seek_to_no_place_at_all_is_not_a_seek() {
        assert_eq!(seek_target(Some(Duration::from_secs(200)), f32::NAN), None);
        assert_eq!(seek_target(Some(Duration::from_secs(200)), f32::INFINITY), Some(Duration::from_secs(200)));
    }

    #[test]
    fn a_reverse_dns_bus_name_is_named_after_its_last_part_unless_the_player_names_itself() {
        assert_eq!(identity_from_bus("org.mpris.MediaPlayer2.io.bassi.Amberol"), "Amberol");
        assert_eq!(identity_from_bus("org.mpris.MediaPlayer2.firefox.instance_1_23"), "Firefox");
        assert_eq!(
            identity_or_bus("  Mozilla Firefox ", "org.mpris.MediaPlayer2.firefox.instance_1_23"),
            "Mozilla Firefox"
        );
        assert_eq!(identity_or_bus(" ", "org.mpris.MediaPlayer2.io.bassi.Amberol"), "Amberol");
        assert_eq!(
            player_icon_candidates("org.mpris.MediaPlayer2.io.bassi.Amberol", ""),
            vec!["io.bassi.amberol".to_string(), "io.bassi.Amberol".to_string()]
        );
    }

    #[test]
    fn relays_and_remote_players_are_told_apart_from_local_ones() {
        for relay in [
            "org.mpris.MediaPlayer2.playerctld",
            "org.mpris.MediaPlayer2.plasma-browser-integration",
            "org.mpris.MediaPlayer2.kdeconnect.mpris_000001",
            "org.mpris.MediaPlayer2.kdeconnect",
        ] {
            assert!(is_relay_bus(relay), "{relay}");
        }
        for local in [
            "org.mpris.MediaPlayer2.spotify",
            "org.mpris.MediaPlayer2.firefox.instance_1_23",
            "org.mpris.MediaPlayer2.kdeconnectish",
            "org.mpris.MediaPlayer2.playerctldaemon",
        ] {
            assert!(!is_relay_bus(local), "{local}");
        }
        assert!(is_mirror_bus("org.mpris.MediaPlayer2.playerctld"));
        assert!(!is_mirror_bus("org.mpris.MediaPlayer2.kdeconnect.mpris_000001"), "phone music is still shown");
    }

    #[test]
    fn a_controllable_player_wins_over_one_in_the_same_state_that_is_not() {
        let blind = Track { can_control: false, ..player("a", Playback::Playing) };
        let able = Track { can_control: true, ..player("b", Playback::Playing) };
        assert!(better_candidate(Some(&blind), &able));
        assert!(!better_candidate(Some(&able), &blind));
        assert!(!better_candidate(Some(&able), &Track { bus: "c".into(), ..able.clone() }));
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

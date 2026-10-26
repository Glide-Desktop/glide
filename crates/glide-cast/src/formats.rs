#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mode {
    pub bit: u8,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub interlaced: bool,
}

const fn mode(bit: u8, width: u32, height: u32, fps: u32, interlaced: bool) -> Mode {
    Mode { bit, width, height, fps, interlaced }
}

pub(crate) const CEA: [Mode; 17] = [
    mode(0, 640, 480, 60, false),
    mode(1, 720, 480, 60, false),
    mode(2, 720, 480, 60, true),
    mode(3, 720, 576, 50, false),
    mode(4, 720, 576, 50, true),
    mode(5, 1280, 720, 30, false),
    mode(6, 1280, 720, 60, false),
    mode(7, 1920, 1080, 30, false),
    mode(8, 1920, 1080, 60, false),
    mode(9, 1920, 1080, 60, true),
    mode(10, 1280, 720, 25, false),
    mode(11, 1280, 720, 50, false),
    mode(12, 1920, 1080, 25, false),
    mode(13, 1920, 1080, 50, false),
    mode(14, 1920, 1080, 50, true),
    mode(15, 1280, 720, 24, false),
    mode(16, 1920, 1080, 24, false),
];

pub(crate) const SOFTWARE_PREFERENCE: [u8; 6] = [7, 6, 5, 12, 10, 0];
pub(crate) const HARDWARE_PREFERENCE: [u8; 7] = [8, 7, 6, 13, 12, 5, 0];
pub(crate) const SOFTWARE_PREFERENCE_720: [u8; 4] = [6, 5, 10, 0];
pub(crate) const HARDWARE_PREFERENCE_720: [u8; 4] = [6, 5, 10, 0];

pub(crate) const PROFILE_CBP: u8 = 0x01;
pub(crate) const PROFILE_CHP: u8 = 0x02;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct H264 {
    pub profile: u8,
    pub level: u8,
    pub cea: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VideoFormats {
    pub native: u8,
    pub codecs: Vec<H264>,
}

fn hex<T: TryFrom<u32>>(token: &str) -> Option<T> {
    T::try_from(u32::from_str_radix(token, 16).ok()?).ok()
}

#[must_use]
pub(crate) fn parse_video_formats(value: &str) -> Option<VideoFormats> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("none") {
        return None;
    }
    let mut head = value.splitn(3, char::is_whitespace);
    let native = hex(head.next()?)?;
    let _preferred: u8 = hex(head.next()?)?;
    let codecs = head
        .next()?
        .split(',')
        .filter_map(|codec| {
            let tokens: Vec<&str> = codec.split_whitespace().collect();
            (tokens.len() >= 5).then_some(())?;
            Some(H264 { profile: hex(tokens[0])?, level: hex(tokens[1])?, cea: hex(tokens[2])? })
        })
        .collect::<Vec<_>>();
    (!codecs.is_empty()).then_some(VideoFormats { native, codecs })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Video {
    pub mode: Mode,
    pub profile: u8,
    pub level: u8,
}

#[must_use]
pub(crate) fn choose_video(formats: &VideoFormats, preference: &[u8]) -> Option<Video> {
    let codec =
        formats.codecs.iter().find(|codec| codec.profile & PROFILE_CBP != 0).or_else(|| formats.codecs.first())?;
    let bit = preference.iter().copied().find(|bit| codec.cea & (1 << bit) != 0)?;
    let profile = if codec.profile & PROFILE_CBP != 0 { PROFILE_CBP } else { PROFILE_CHP };
    let level = highest_bit(codec.level).unwrap_or(0x01);
    Some(Video { mode: CEA[usize::from(bit)], profile, level })
}

fn highest_bit(byte: u8) -> Option<u8> {
    (byte != 0).then(|| 1 << byte.ilog2())
}

#[must_use]
pub(crate) fn video_formats_line(video: Video) -> String {
    format!(
        "{:02x} 00 {:02x} {:02x} {:08x} 00000000 00000000 00 0000 0000 00 none none",
        video.mode.bit << 3,
        video.profile,
        video.level,
        1_u32 << video.mode.bit
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Audio {
    Lpcm48,
    Aac48,
}

impl Audio {
    #[must_use]
    pub const fn line(self) -> &'static str {
        match self {
            Self::Lpcm48 => "LPCM 00000002 00",
            Self::Aac48 => "AAC 00000001 00",
        }
    }
}

#[must_use]
pub(crate) fn choose_audio(value: &str) -> Option<Audio> {
    let codecs: Vec<(String, u32)> = value
        .split(',')
        .filter_map(|codec| {
            let mut tokens = codec.split_whitespace();
            Some((tokens.next()?.to_ascii_uppercase(), hex(tokens.next()?)?))
        })
        .collect();
    let has = |name: &str, bit: u32| codecs.iter().any(|(codec, modes)| codec == name && modes & (1 << bit) != 0);
    if has("LPCM", 1) {
        Some(Audio::Lpcm48)
    } else if has("AAC", 0) {
        Some(Audio::Aac48)
    } else {
        None
    }
}

#[must_use]
pub(crate) fn client_rtp_port(value: &str) -> Option<u16> {
    let mut tokens = value.split_whitespace();
    let profile = tokens.next()?;
    profile.to_ascii_uppercase().starts_with("RTP/AVP/UDP").then_some(())?;
    tokens.next()?.parse().ok().filter(|port| *port != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMSUNG: &str = "00 00 02 10 0001ffff 1fffffff 00000fff 00 0000 0000 00 none none";

    #[test]
    fn a_sink_list_parses_into_its_codecs() {
        let formats = parse_video_formats(SAMSUNG).unwrap();
        assert_eq!(formats.codecs, [H264 { profile: 0x02, level: 0x10, cea: 0x0001_ffff }]);
        let two = parse_video_formats("00 00 01 01 00000021 00000000 00000000 00 0000 0000 00 none none, 02 04 000001ff 00000000 00000000 00 0000 0000 00 none none").unwrap();
        assert_eq!(two.codecs.len(), 2);
        assert_eq!(parse_video_formats("none"), None);
        assert_eq!(parse_video_formats("00 00"), None);
    }

    #[test]
    fn the_best_mode_both_sides_support_is_chosen() {
        let formats = parse_video_formats(SAMSUNG).unwrap();
        let video = choose_video(&formats, &SOFTWARE_PREFERENCE).unwrap();
        assert_eq!((video.mode.width, video.mode.height, video.mode.fps), (1920, 1080, 30));
        assert_eq!(video.profile, PROFILE_CHP, "the sink offers only High");
        assert_eq!(video.level, 0x10);
        let hd = choose_video(&formats, &HARDWARE_PREFERENCE).unwrap();
        assert_eq!(hd.mode.fps, 60);
    }

    #[test]
    fn a_small_sink_falls_back_to_the_mandatory_mode() {
        let formats = parse_video_formats("00 00 01 01 00000001 00000000 00000000 00 0000 0000 00 none none").unwrap();
        let video = choose_video(&formats, &SOFTWARE_PREFERENCE).unwrap();
        assert_eq!((video.mode.width, video.mode.height), (640, 480));
        assert_eq!(video.profile, PROFILE_CBP);
    }

    #[test]
    fn the_chosen_mode_is_announced_with_exactly_one_bit() {
        let video = Video { mode: CEA[7], profile: PROFILE_CBP, level: 0x04 };
        assert_eq!(video_formats_line(video), "38 00 01 04 00000080 00000000 00000000 00 0000 0000 00 none none");
    }

    #[test]
    fn lpcm_at_48_khz_wins_and_aac_is_the_fallback() {
        assert_eq!(choose_audio("LPCM 00000003 00, AAC 00000001 00"), Some(Audio::Lpcm48));
        assert_eq!(choose_audio("LPCM 00000001 00, AAC 00000001 00"), Some(Audio::Aac48));
        assert_eq!(choose_audio("none"), None);
    }

    #[test]
    fn the_sink_rtp_port_is_read_from_its_transport() {
        assert_eq!(client_rtp_port("RTP/AVP/UDP;unicast 1028 0 mode=play"), Some(1028));
        assert_eq!(client_rtp_port("RTP/AVP/TCP;unicast 1028 0 mode=play"), None);
        assert_eq!(client_rtp_port("RTP/AVP/UDP;unicast 0 0 mode=play"), None);
    }
}

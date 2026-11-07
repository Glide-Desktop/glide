use std::time::Duration;

use gstreamer as gst;
use gstreamer_pbutils::prelude::*;

use crate::media::{Chapter, Failure};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Hdr {
    #[default]
    Sdr,
    Hdr10,
    Hlg,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Video {
    pub codec: String,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub bitrate: u32,
    pub depth: u32,
    pub hdr: Hdr,
    pub colorimetry: Option<String>,
    pub interlaced: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Audio {
    pub codec: String,
    pub channels: u32,
    pub rate: u32,
    pub bitrate: u32,
    pub language: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Info {
    pub duration: Option<Duration>,
    pub seekable: bool,
    pub live: bool,
    pub title: Option<String>,
    pub container: Option<String>,
    pub video: Option<Video>,
    pub audio: Vec<Audio>,
    pub subtitles: Vec<Option<String>>,
    pub chapters: Vec<Chapter>,
    pub bytes: Option<u64>,
    pub artist: Option<String>,
    pub album: Option<String>,
}

impl Info {
    #[must_use]
    pub fn bitrate(&self) -> Option<u32> {
        let video = self.video.as_ref().map_or(0, |video| video.bitrate);
        let audio: u32 = self.audio.iter().map(|audio| audio.bitrate).sum();
        if video + audio > 0 {
            return Some(video + audio);
        }
        let (bytes, duration) = (self.bytes?, self.duration?);
        let seconds = duration.as_secs_f64();
        (seconds > 0.5).then(|| (bytes as f64 * 8.0 / seconds) as u32)
    }

    #[must_use]
    pub fn resolution_label(&self) -> Option<String> {
        let video = self.video.as_ref()?;
        Some(label_for(video.width, video.height))
    }
}

#[must_use]
pub fn label_for(width: u32, height: u32) -> String {
    let short = width.min(height);
    match short {
        2000.. => "2160p".into(),
        1300.. => "1440p".into(),
        1000.. => "1080p".into(),
        700.. => "720p".into(),
        _ => format!("{short}p"),
    }
}

#[must_use]
pub fn hdr_of(colorimetry: Option<&str>) -> Hdr {
    match colorimetry {
        Some(value) if value.contains("pq") || value.contains("smpte2084") || value.ends_with(":14:9") => Hdr::Hdr10,
        Some(value) if value.contains("hlg") || value.contains("arib-std-b67") || value.ends_with(":16:9") => Hdr::Hlg,
        _ => Hdr::Sdr,
    }
}

fn codec(caps: Option<gst::Caps>) -> String {
    caps.map(|caps| gstreamer_pbutils::pb_utils_get_codec_description(&caps).to_string()).unwrap_or_default()
}

pub fn discover(uri: &str, timeout: Duration) -> Result<Info, Failure> {
    crate::init().map_err(Failure::Other)?;
    let discoverer = gstreamer_pbutils::Discoverer::new(gst::ClockTime::from_nseconds(timeout.as_nanos() as u64))
        .map_err(|err| Failure::Other(err.to_string()))?;
    let found = discoverer.discover_uri(uri).map_err(|err| crate::media::classify(&err, None))?;
    match found.result() {
        gstreamer_pbutils::DiscovererResult::Ok => {}
        gstreamer_pbutils::DiscovererResult::MissingPlugins => {
            let detail = found.missing_elements_installer_details().first().map(ToString::to_string);
            return Err(Failure::Missing { what: String::new(), detail });
        }
        gstreamer_pbutils::DiscovererResult::UriInvalid => return Err(Failure::NotFound),
        gstreamer_pbutils::DiscovererResult::Timeout => return Err(Failure::Other("timeout".into())),
        _ => return Err(Failure::Broken(String::new())),
    }
    let tags = found.tags();
    let tag = |pick: fn(&gst::TagListRef) -> Option<String>| tags.as_ref().and_then(|tags| pick(tags));
    let video = found.video_streams().into_iter().find(|video| !video.is_image()).map(|video| {
        let caps = video.caps();
        let colorimetry = caps
            .as_ref()
            .and_then(|caps| caps.structure(0).and_then(|structure| structure.get::<String>("colorimetry").ok()));
        let rate = video.framerate();
        let fps = if rate.denom() > 0 { f64::from(rate.numer()) / f64::from(rate.denom()) } else { 0.0 };
        Video {
            codec: codec(caps),
            width: video.width(),
            height: video.height(),
            fps,
            bitrate: video.bitrate().max(video.max_bitrate().min(video.bitrate().saturating_mul(2))),
            depth: video.depth(),
            hdr: hdr_of(colorimetry.as_deref()),
            colorimetry,
            interlaced: video.is_interlaced(),
        }
    });
    let audio = found
        .audio_streams()
        .into_iter()
        .map(|audio| Audio {
            codec: codec(audio.caps()),
            channels: audio.channels(),
            rate: audio.sample_rate(),
            bitrate: audio.bitrate(),
            language: audio.language().map(|language| language.to_string()),
        })
        .collect();
    let path = gst::glib::filename_from_uri(uri).ok().map(|(path, _)| path);
    Ok(Info {
        duration: found.duration().map(|duration| Duration::from_nanos(duration.nseconds())),
        seekable: found.is_seekable(),
        live: found.is_live(),
        title: tag(|tags| tags.get::<gst::tags::Title>().map(|tag| tag.get().to_string())),
        artist: tag(|tags| tags.get::<gst::tags::Artist>().map(|tag| tag.get().to_string())),
        album: tag(|tags| tags.get::<gst::tags::Album>().map(|tag| tag.get().to_string())),
        container: tag(|tags| tags.get::<gst::tags::ContainerFormat>().map(|tag| tag.get().to_string())),
        video,
        audio,
        subtitles: found
            .subtitle_streams()
            .into_iter()
            .map(|text| text.language().map(|lang| lang.to_string()))
            .collect(),
        chapters: found.toc().map(|toc| crate::media::chapters(&toc)).unwrap_or_default(),
        bytes: path.and_then(|path| std::fs::metadata(path).ok()).map(|meta| meta.len()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolution_labels_follow_the_short_side() {
        assert_eq!(label_for(3840, 2160), "2160p");
        assert_eq!(label_for(1920, 1080), "1080p");
        assert_eq!(label_for(1080, 1920), "1080p");
        assert_eq!(label_for(1280, 720), "720p");
        assert_eq!(label_for(640, 480), "480p");
    }

    #[test]
    fn transfer_functions_name_the_hdr_kind() {
        assert_eq!(hdr_of(Some("bt2100-pq")), Hdr::Hdr10);
        assert_eq!(hdr_of(Some("bt2100-hlg")), Hdr::Hlg);
        assert_eq!(hdr_of(Some("2:9:14:9")), Hdr::Hdr10);
        assert_eq!(hdr_of(Some("bt709")), Hdr::Sdr);
        assert_eq!(hdr_of(None), Hdr::Sdr);
    }

    #[test]
    fn bitrate_falls_back_to_size_over_duration() {
        let info = Info { bytes: Some(1_000_000), duration: Some(Duration::from_secs(8)), ..Info::default() };
        assert_eq!(info.bitrate(), Some(1_000_000));
    }
}

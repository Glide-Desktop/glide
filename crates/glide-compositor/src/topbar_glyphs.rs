use super::LocalRect;
use crate::config::Color;
use crate::raster::Canvas;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Glyph {
    Bell,
    Quiet,
    Play,
    Pause,
    Previous,
    Next,
    Collapse,
    Close,
    Moon,
    Back,
    Forward,
    Cup,
    Search,
    SpeakerSilent,
    SpeakerLow,
    SpeakerHigh,
    SpeakerMuted,
    Bluetooth,
    Shield,
}

const SPEAKER: &str = "<path d=\"M3 6.6h2.6L9 3.6v10.8L5.6 11.4H3z\"/>";
const NEAR_WAVE: &str = "<path d=\"M11.6 6.8a3.3 3.3 0 010 4.6\"/>";
const FAR_WAVE: &str = "<path d=\"M13.8 4.6a6.4 6.4 0 010 9\"/>";

impl Glyph {
    #[cfg(test)]
    pub(super) const ALL: [Self; 19] = [
        Self::Bell,
        Self::Quiet,
        Self::Play,
        Self::Pause,
        Self::Previous,
        Self::Next,
        Self::Collapse,
        Self::Close,
        Self::Moon,
        Self::Back,
        Self::Forward,
        Self::Cup,
        Self::Search,
        Self::SpeakerSilent,
        Self::SpeakerLow,
        Self::SpeakerHigh,
        Self::SpeakerMuted,
        Self::Bluetooth,
        Self::Shield,
    ];

    fn body(self) -> String {
        let fixed = match self {
            Self::Bell => {
                "<path d=\"M8 2.5a4 4 0 00-4 4v3l-1.5 2h11L12 9.5v-3a4 4 0 00-4-4zM6.5 13.5a1.5 1.5 0 003 0\"/>"
            }
            Self::Quiet => {
                "<path d=\"M8 3a3.5 3.5 0 00-3.5 3.5v2.5L3 11h10l-1.5-2V6.5A3.5 3.5 0 008 3z\"/><path d=\"M6.5 13a1.5 1.5 0 003 0\"/>"
            }
            Self::Play => "<path d=\"M4.5 3v10l8-5z\"/>",
            Self::Pause => "<path d=\"M5 3h2v10H5zM9 3h2v10H9z\"/>",
            Self::Previous => "<path d=\"M5 3h1.5v10H5zM12 3v10l-6-5z\"/>",
            Self::Next => "<path d=\"M11 3h-1.5v10H11zM4 3v10l6-5z\"/>",
            Self::Collapse => "<path d=\"M4 10l4-4 4 4\"/>",
            Self::Close => "<path d=\"M4 4l8 8M12 4l-8 8\"/>",
            Self::Moon => "<path d=\"M14.2 10.4A5.8 5.8 0 017.6 3.8a5.8 5.8 0 106.6 6.6z\"/>",
            Self::Back => "<path d=\"M10 4l-4 4 4 4\"/>",
            Self::Forward => "<path d=\"M6 4l4 4-4 4\"/>",
            Self::Cup => {
                "<path d=\"M3 6.2h8.4v3.6a3 3 0 01-3 3H6a3 3 0 01-3-3z\"/><path d=\"M11.6 7.2h1.2a1.6 1.6 0 010 3.2h-1.2\"/><path d=\"M5.4 3.4v1.2M8 2.8v1.8M10.6 3.4v1.2\"/>"
            }
            Self::Search => "<circle cx=\"7\" cy=\"7\" r=\"4.3\"/><path d=\"M10.3 10.3l2.9 2.9\"/>",
            Self::SpeakerSilent => SPEAKER,
            Self::SpeakerLow => return format!("{SPEAKER}{NEAR_WAVE}"),
            Self::SpeakerHigh => return format!("{SPEAKER}{NEAR_WAVE}{FAR_WAVE}"),
            Self::SpeakerMuted => return format!("{SPEAKER}<path d=\"M12.2 6.2l3.4 5.6M15.6 6.2l-3.4 5.6\"/>"),
            Self::Bluetooth => "<path d=\"M5 5.4l6 5.2-3 2.6V2.8l3 2.6-6 5.2\"/>",
            Self::Shield => "<path d=\"M8 2.2l4.8 1.8v3.6c0 3-2 5.2-4.8 6.2-2.8-1-4.8-3.2-4.8-6.2V4z\"/>",
        };
        fixed.to_string()
    }

    const fn view(self) -> u8 {
        match self {
            Self::Moon | Self::SpeakerSilent | Self::SpeakerLow | Self::SpeakerHigh | Self::SpeakerMuted => 18,
            _ => 16,
        }
    }

    const fn stroke(self) -> Option<&'static str> {
        match self {
            Self::Play | Self::Pause | Self::Previous | Self::Next => None,
            Self::Bell => Some("1.6"),
            Self::Quiet => Some("1.3"),
            Self::Collapse | Self::Close => Some("2.2"),
            Self::Back | Self::Forward => Some("1.8"),
            Self::SpeakerSilent | Self::SpeakerLow | Self::SpeakerHigh | Self::SpeakerMuted | Self::Bluetooth => {
                Some("1.4")
            }
            Self::Moon | Self::Cup | Self::Search | Self::Shield => Some("1.5"),
        }
    }

    pub(super) fn svg(self) -> String {
        let view = self.view();
        let paint = self.stroke().map_or_else(
            || String::from("fill=\"#000\""),
            |width| {
                format!(
                    "fill=\"none\" stroke=\"#000\" stroke-width=\"{width}\" stroke-linecap=\"round\" stroke-linejoin=\"round\""
                )
            },
        );
        format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{view}\" height=\"{view}\" viewBox=\"0 0 {view} {view}\" {paint}>{}</svg>",
            self.body()
        )
    }
}

impl Glyph {
    const fn theme_name(self) -> &'static str {
        match self {
            Self::Bell | Self::Quiet => "notifications",
            Self::Play => "media-playback-start",
            Self::Pause => "media-playback-pause",
            Self::Previous => "media-skip-backward",
            Self::Next => "media-skip-forward",
            Self::Collapse => "pan-up",
            Self::Close => "window-close",
            Self::Moon => "notifications-disabled",
            Self::Back => "pan-start",
            Self::Forward => "pan-end",
            Self::Cup => "caffeine-cup-empty",
            Self::Search => "system-search",
            Self::SpeakerSilent | Self::SpeakerMuted => "audio-volume-muted",
            Self::SpeakerLow => "audio-volume-low",
            Self::SpeakerHigh => "audio-volume-high",
            Self::Bluetooth => "bluetooth",
            Self::Shield => "network-vpn",
        }
    }
}

const fn centred(rect: LocalRect, side: i32) -> (i32, i32) {
    (rect.loc.x + (rect.size.w - side) / 2, rect.loc.y + (rect.size.h - side) / 2)
}

pub(super) fn glyph(c: &mut Canvas<'_>, glyph: Glyph, side: i32, rect: LocalRect, colour: Color) {
    if !themed(c, glyph.theme_name(), side, rect, colour) {
        svg_glyph(c, &glyph.svg(), side, rect, colour);
    }
}

pub(super) fn themed(c: &mut Canvas<'_>, name: &str, side: i32, rect: LocalRect, colour: Color) -> bool {
    c.icon(name, side, centred(rect, side), colour)
}

pub(super) fn svg_glyph(c: &mut Canvas<'_>, svg: &str, side: i32, rect: LocalRect, colour: Color) {
    let at = centred(rect, side);
    if let Some(pixels) = crate::paint::render_svg_tinted(svg.as_bytes(), side * c.scale, colour) {
        crate::paint::blit_square_over(&mut c.buf, c.w, c.h, &pixels, side, at.0, at.1, c.scale);
    }
}

#[must_use]
pub(super) fn wifi_bars(level: u8) -> String {
    use std::fmt::Write;
    let mut bars = String::new();
    for (index, height) in [3.0_f32, 5.5, 8.0, 11.0].into_iter().enumerate() {
        let x = 0.75 + index as f32 * 4.0;
        let opacity = if usize::from(level) > index { 1.0 } else { 0.26 };
        let _ = write!(
            bars,
            "<rect x=\"{x}\" y=\"{y}\" width=\"2.5\" height=\"{height}\" rx=\"1\" fill-opacity=\"{opacity}\"/>",
            y = 13.5 - height
        );
    }
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\" fill=\"#000\">{bars}</svg>"
    )
}

#[must_use]
pub(super) fn battery_cell(pct: u8) -> String {
    let level = f32::from(pct.min(100)) / 100.0 * 9.0;
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\" fill=\"#000\"><path d=\"M6 1.5h4V3h1.5a1 1 0 011 1v10a1 1 0 01-1 1h-7a1 1 0 01-1-1V4a1 1 0 011-1H6z\" fill-opacity=\"0.35\"/><rect x=\"5\" y=\"{y}\" width=\"6\" height=\"{level}\" rx=\"0.5\"/></svg>",
        y = 13.5 - level
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_glyph_is_valid_svg() {
        for glyph in Glyph::ALL {
            assert!(crate::paint::render_svg(glyph.svg().as_bytes(), 24).is_some(), "{glyph:?}");
        }
        for level in 0..=4 {
            assert!(crate::paint::render_svg(wifi_bars(level).as_bytes(), 24).is_some(), "{level}");
        }
        for pct in [0, 37, 100, 255] {
            assert!(crate::paint::render_svg(battery_cell(pct).as_bytes(), 24).is_some(), "{pct}");
        }
    }
}

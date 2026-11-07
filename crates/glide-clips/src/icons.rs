use iced::widget::svg;
use iced::{Color, Element};

macro_rules! glyph {
    ($($body:literal),+) => {
        concat!(
            "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' ",
            "stroke='currentColor' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'>",
            $($body,)+
            "</svg>"
        )
        .as_bytes()
    };
}

macro_rules! solid {
    ($($body:literal),+) => {
        concat!(
            "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='currentColor' stroke='none'>",
            $($body,)+
            "</svg>"
        )
        .as_bytes()
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Icon {
    Play,
    Pause,
    Back10,
    Forward10,
    Back,
    Volume,
    VolumeLow,
    Muted,
    Subtitles,
    SubtitlesOff,
    More,
    Fullscreen,
    LeaveFullscreen,
    Maximize,
    Close,
    Pip,
    Folder,
    Link,
    Speed,
    Pitch,
    Clock,
    Plus,
    File,
    Mic,
    Pin,
    Repeat,
    Aspect,
    Fill,
    Wide,
    Scissors,
    Split,
    RotateLeft,
    RotateRight,
    FlipH,
    FlipV,
    Trash,
    Export,
    Info,
    Keyboard,
    FrameBack,
    FrameForward,
    Copy,
    Image,
    Text,
    Globe,
    External,
    Warn,
    NoVideo,
    Search,
    Speaker,
    Mark,
    Stop,
    DropIn,
}

impl Icon {
    #[must_use]
    #[allow(clippy::too_many_lines)] // One match arm of inline SVG per glyph
    pub fn handle(self) -> svg::Handle {
        let bytes: &'static [u8] = match self {
            Self::Play => {
                solid!("<path d='M7 4.5v15a1 1 0 0 0 1.5.9l12-7.5a1 1 0 0 0 0-1.8l-12-7.5A1 1 0 0 0 7 4.5z'/>")
            }
            Self::Pause => solid!(
                "<rect x='6' y='5' width='4' height='14' rx='1'/><rect x='14' y='5' width='4' height='14' rx='1'/>"
            ),
            Self::Back10 => glyph!("<path d='M3.5 12a8.5 8.5 0 1 0 2.6-6.1'/><path d='M3 4v5h5'/>"),
            Self::Forward10 => glyph!("<path d='M20.5 12a8.5 8.5 0 1 1-2.6-6.1'/><path d='M21 4v5h-5'/>"),
            Self::Back => glyph!("<path d='m15 18-6-6 6-6'/>"),
            Self::Volume => {
                glyph!("<path d='M11 5 6 9H3v6h3l5 4z'/><path d='M15.5 8.5a5 5 0 0 1 0 7M18.5 5.5a9 9 0 0 1 0 13'/>")
            }
            Self::VolumeLow | Self::Speaker => {
                glyph!("<path d='M11 5 6 9H3v6h3l5 4z'/><path d='M15.5 8.5a5 5 0 0 1 0 7'/>")
            }
            Self::Muted => glyph!("<path d='M11 5 6 9H3v6h3l5 4z'/><path d='M16 9l5 6M21 9l-5 6'/>"),
            Self::Subtitles => glyph!(
                "<rect x='3' y='5' width='18' height='14' rx='3'/><path d='M7 12h4M14 12h3M7 15.5h7M16.5 15.5h.5'/>"
            ),
            Self::SubtitlesOff => {
                glyph!("<rect x='3' y='5' width='18' height='14' rx='3'/><path d='M7 12h4M7 15.5h4M3 3l18 18'/>")
            }
            Self::More => solid!(
                "<circle cx='5' cy='12' r='1.7'/><circle cx='12' cy='12' r='1.7'/><circle cx='19' cy='12' r='1.7'/>"
            ),
            Self::Fullscreen | Self::Fill => glyph!("<path d='M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5'/>"),
            Self::LeaveFullscreen => glyph!("<path d='M9 4v5H4M15 4v5h5M9 20v-5H4M15 20v-5h5'/>"),
            Self::Maximize => glyph!("<rect x='5' y='5' width='14' height='14' rx='3'/>"),
            Self::Close => glyph!("<path d='M7 7l10 10M17 7 7 17'/>"),
            Self::Pip => glyph!(
                "<rect x='3' y='5' width='18' height='14' rx='2'/><rect x='12' y='11' width='6' height='5' rx='1' fill='currentColor'/>"
            ),
            Self::Folder => {
                glyph!("<path d='M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z'/>")
            }
            Self::Link => glyph!(
                "<path d='M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1 1M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1'/>"
            ),
            Self::Speed => glyph!("<path d='M12 14l4-4'/><path d='M3.3 17a9 9 0 1 1 17.4 0'/>"),
            Self::Pitch => glyph!(
                "<path d='M9 18V6l10-2v12'/><circle cx='6.5' cy='18' r='2.5'/><circle cx='16.5' cy='16' r='2.5'/>"
            ),
            Self::Clock => glyph!("<circle cx='12' cy='13' r='8'/><path d='M12 9v4l2 2M9 2h6'/>"),
            Self::Plus => glyph!("<path d='M12 5v14M5 12h14'/>"),
            Self::File => glyph!("<path d='M6 3h9l4 4v14H6zM14 3v5h5'/><path d='M9 14h6M9 17h4'/>"),
            Self::Mic => {
                glyph!("<rect x='9' y='3' width='6' height='11' rx='3'/><path d='M5 11a7 7 0 0 0 14 0M12 18v3'/>")
            }
            Self::Pin => glyph!("<path d='M12 17v5M9 3h6l-1 6 4 4H6l4-4z'/>"),
            Self::Repeat => {
                glyph!("<path d='M17 2l4 4-4 4M3 11V9a3 3 0 0 1 3-3h15M7 22l-4-4 4-4M21 13v2a3 3 0 0 1-3 3H3'/>")
            }
            Self::Aspect => glyph!("<rect x='3' y='6' width='18' height='12' rx='2'/><path d='M8 12h8'/>"),
            Self::Wide => glyph!("<rect x='3' y='7' width='18' height='10' rx='2'/>"),
            Self::Scissors => glyph!(
                "<circle cx='6' cy='6' r='3'/><circle cx='6' cy='18' r='3'/><path d='M20 4 8.1 15.9M14.5 14.5 20 20M8.1 8.1 12 12'/>"
            ),
            Self::Split => glyph!(
                "<rect x='2' y='6' width='8' height='12' rx='1.5'/><rect x='14' y='6' width='8' height='12' rx='1.5'/><path d='M12 3v18'/>"
            ),
            Self::RotateLeft => glyph!("<path d='M4 12a8 8 0 1 0 3-6.2M4 4v5h5'/>"),
            Self::RotateRight => glyph!("<path d='M20 12a8 8 0 1 1-3-6.2M20 4v5h-5'/>"),
            Self::FlipH => glyph!("<path d='M12 3v18M8 7l-5 5 5 5M16 7l5 5-5 5'/>"),
            Self::FlipV => glyph!("<path d='M3 12h18M7 8l5-5 5 5M7 16l5 5 5-5'/>"),
            Self::Trash => glyph!("<path d='M4 7h16M10 11v6M14 11v6M5 7l1 13h12l1-13M9 7V4h6v3'/>"),
            Self::Export => glyph!("<path d='M12 15V3M7 8l5-5 5 5M5 14v5a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2v-5'/>"),
            Self::Info => glyph!("<circle cx='12' cy='12' r='9'/><path d='M12 11v5M12 8h.01'/>"),
            Self::Keyboard => glyph!(
                "<rect x='3' y='6' width='18' height='12' rx='2'/><path d='M7 10h.01M11 10h.01M15 10h.01M7 14h10'/>"
            ),
            Self::FrameBack => glyph!("<path d='M5 5v14M9 12l10-7v14z'/>"),
            Self::FrameForward => glyph!("<path d='M19 5v14M15 12 5 5v14z'/>"),
            Self::Copy => glyph!(
                "<rect x='8' y='8' width='13' height='13' rx='2'/><path d='M16 8V5a2 2 0 0 0-2-2H5a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h3'/>"
            ),
            Self::Image => glyph!(
                "<rect x='3' y='5' width='18' height='14' rx='2'/><circle cx='9' cy='10' r='2'/><path d='M21 16l-5-5-9 8'/>"
            ),
            Self::Text => glyph!("<path d='M4 7V4h3M17 4h3v3M20 17v3h-3M7 20H4v-3M8 9h8M8 13h8M8 17h5'/>"),
            Self::Globe => glyph!(
                "<circle cx='12' cy='12' r='9'/><path d='M3 12h18M12 3a14 14 0 0 1 0 18M12 3a14 14 0 0 0 0 18'/>"
            ),
            Self::External => {
                glyph!("<path d='M14 4h6v6M20 4l-9 9M18 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h5'/>")
            }
            Self::Warn => glyph!("<path d='M12 3l9 16H3z'/><path d='M12 10v4M12 17h.01'/>"),
            Self::NoVideo => glyph!("<rect x='2' y='4' width='20' height='16' rx='2'/><path d='M2 4l20 16'/>"),
            Self::Search => glyph!("<circle cx='11' cy='11' r='7'/><path d='m20 20-3.5-3.5'/>"),
            Self::Mark => {
                solid!("<path d='M8 5.5v13a1 1 0 0 0 1.5.86l10.5-6.5a1 1 0 0 0 0-1.72L9.5 4.64A1 1 0 0 0 8 5.5z'/>")
            }
            Self::Stop => solid!("<rect x='6' y='6' width='12' height='12' rx='2'/>"),
            Self::DropIn => glyph!("<path d='M12 4v12M6 10l6 6 6-6M5 20h14'/>"),
        };
        svg::Handle::from_memory(bytes)
    }
}

impl glide_sdk::widget::Glyph for Icon {
    fn draw<'a, M: 'a>(self, side: f32, tint: Color) -> Element<'a, M> {
        glide_sdk::widget::svg_glyph(self.handle(), side, tint)
    }
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Icon {
    Grid,
    Heart,
    HeartFilled,
    Albums,
    Folder,
    Plus,
    Card,
    Eject,
    Trash,
    Search,
    Back,
    Next,
    Share,
    Panel,
    Sliders,
    Minus,
    Fullscreen,
    RotateLeft,
    RotateRight,
    FlipH,
    FlipV,
    Level,
    Sparkle,
    Undo,
    Redo,
    Close,
    Clock,
    Aperture,
    Play,
    Pause,
    Step,
    Pen,
    Screen,
    Sun,
    Wide,
    Pages,
    Image,
    Broken,
    Restore,
    Export,
    Slideshow,
    Info,
}

impl Icon {
    #[must_use]
    #[allow(clippy::too_many_lines)] // One match arm of inline SVG per glyph
    pub fn handle(self) -> svg::Handle {
        let bytes: &'static [u8] = match self {
            Self::Grid => glyph!(
                "<rect x='3' y='3' width='7' height='7' rx='1'/><rect x='14' y='3' width='7' height='7' rx='1'/>",
                "<rect x='3' y='14' width='7' height='7' rx='1'/><rect x='14' y='14' width='7' height='7' rx='1'/>"
            ),
            Self::Heart => glyph!("<path d='M12 20s-7-4.4-7-10a4 4 0 0 1 7-2.6A4 4 0 0 1 19 10c0 5.6-7 10-7 10z'/>"),
            Self::HeartFilled => glyph!(
                "<path fill='currentColor' d='M12 20s-7-4.4-7-10a4 4 0 0 1 7-2.6A4 4 0 0 1 19 10c0 5.6-7 10-7 10z'/>"
            ),
            Self::Albums => {
                glyph!("<rect x='3' y='7' width='14' height='14' rx='2'/><path d='M7 3h12a2 2 0 0 1 2 2v12'/>")
            }
            Self::Folder => {
                glyph!("<path d='M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z'/>")
            }
            Self::Plus => glyph!("<path d='M12 5v14M5 12h14'/>"),
            Self::Card => glyph!(
                "<path d='M7 3h8l4 4v13a1 1 0 0 1-1 1H7a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1z'/><path d='M10 7v3M13 7v3'/>"
            ),
            Self::Eject => glyph!("<path d='M12 5 5 13h14z'/><path d='M5 18h14'/>"),
            Self::Trash => glyph!("<path d='M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13'/>"),
            Self::Search => glyph!("<circle cx='11' cy='11' r='7'/><path d='m20 20-3.5-3.5'/>"),
            Self::Back => glyph!("<path d='m15 18-6-6 6-6'/>"),
            Self::Next => glyph!("<path d='m9 18 6-6-6-6'/>"),
            Self::Share => glyph!("<path d='M12 3v12M7 8l5-5 5 5M5 14v5a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2v-5'/>"),
            Self::Panel => glyph!("<rect x='3' y='4' width='18' height='16' rx='2'/><path d='M15 4v16'/>"),
            Self::Sliders => {
                glyph!("<path d='M4 21v-7M4 10V3M12 21v-9M12 8V3M20 21v-5M20 12V3M1 14h6M9 8h6M17 16h6'/>")
            }
            Self::Minus => glyph!("<path d='M6 12h12'/>"),
            Self::Fullscreen => glyph!("<path d='M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5'/>"),
            Self::RotateLeft => glyph!("<path d='M3 12a9 9 0 1 0 3-6.7L3 8'/><path d='M3 3v5h5'/>"),
            Self::RotateRight => glyph!("<path d='M21 12a9 9 0 1 1-3-6.7L21 8'/><path d='M21 3v5h-5'/>"),
            Self::FlipH => glyph!("<path d='M12 3v18M8 7 3 12l5 5zM16 7l5 5-5 5z'/>"),
            Self::FlipV => glyph!("<path d='M3 12h18M7 8l5-5 5 5zM7 16l5 5 5-5z'/>"),
            Self::Level => glyph!("<path d='M3 17h18M6 13l4-4 3 3 5-5'/>"),
            Self::Sparkle => glyph!(
                "<path d='m12 3 1.8 4.7L18.5 9l-4.7 1.8L12 15.5l-1.8-4.7L5.5 9l4.7-1.3z'/>",
                "<path d='M19 15l.8 2.2L22 18l-2.2.8L19 21l-.8-2.2L16 18l2.2-.8z'/>"
            ),
            Self::Undo => glyph!("<path d='M9 14 4 9l5-5'/><path d='M4 9h11a5 5 0 0 1 0 10h-3'/>"),
            Self::Redo => glyph!("<path d='m15 14 5-5-5-5'/><path d='M20 9H9a5 5 0 0 0 0 10h3'/>"),
            Self::Close => glyph!("<path d='M7 7l10 10M17 7 7 17'/>"),
            Self::Clock => glyph!("<circle cx='12' cy='12' r='9'/><path d='M12 7v5l3 2'/>"),
            Self::Aperture => {
                glyph!("<circle cx='12' cy='12' r='9'/><path d='m14.3 3.3-4.6 8M21 12h-9M14.3 20.7l-4.6-8M3 12h4.6'/>")
            }
            Self::Play => glyph!("<circle cx='12' cy='12' r='9'/><path d='m10 8 6 4-6 4z'/>"),
            Self::Pause => glyph!("<path d='M9 5v14M15 5v14'/>"),
            Self::Step => glyph!("<path d='m6 6 8 6-8 6zM18 6v12'/>"),
            Self::Pen => glyph!("<path d='M3 21l5-5M12 4l8 8-6 2-4 4-2-6z'/>"),
            Self::Screen => glyph!("<rect x='3' y='4' width='18' height='13' rx='2'/><path d='M8 21h8M12 17v4'/>"),
            Self::Sun => glyph!("<circle cx='12' cy='12' r='4'/><path d='M12 3v2M12 19v2M5 12H3M21 12h-2'/>"),
            Self::Wide => glyph!("<path d='M2 8c6-3 14-3 20 0v8c-6 3-14 3-20 0z'/>"),
            Self::Pages => {
                glyph!("<path d='M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z'/><path d='M14 3v5h5'/>")
            }
            Self::Image => glyph!(
                "<rect x='3' y='3' width='18' height='18' rx='2'/><circle cx='9' cy='9' r='2'/><path d='m21 15-5-5L5 21'/>"
            ),
            Self::Broken => glyph!(
                "<path d='M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z'/>",
                "<path d='M14 3v5h5M9.5 13.5l5 5M14.5 13.5l-5 5'/>"
            ),
            Self::Restore => glyph!("<path d='M3 12a9 9 0 1 0 3-6.7L3 8M3 3v5h5'/>"),
            Self::Export => glyph!("<path d='M12 3v12M7 8l5-5 5 5M4 17v2a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-2'/>"),
            Self::Slideshow => glyph!("<rect x='3' y='5' width='18' height='14' rx='2'/><path d='m10 9 5 3-5 3z'/>"),
            Self::Info => glyph!("<circle cx='12' cy='12' r='9'/><path d='M12 11v5M12 8h.01'/>"),
        };
        svg::Handle::from_memory(bytes)
    }
}

impl glide_sdk::widget::Glyph for Icon {
    fn draw<'a, M: 'a>(self, side: f32, tint: Color) -> Element<'a, M> {
        glide_sdk::widget::svg_glyph(self.handle(), side, tint)
    }
}

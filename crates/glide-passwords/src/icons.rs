use iced::widget::svg;
use iced::{Color, Element};

macro_rules! glyph {
    ($($body:literal),+) => {
        concat!(
            "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' ",
            "stroke='currentColor' stroke-width='1.8' stroke-linecap='round' stroke-linejoin='round'>",
            $($body,)+
            "</svg>"
        )
        .as_bytes()
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Icon {
    Grid,
    Key,
    Passkey,
    Timer,
    Card,
    Note,
    Send,
    Shield,
    Trash,
    Lock,
    Sync,
    Plus,
    Search,
    Sort,
    Edit,
    More,
    Copy,
    Eye,
    EyeOff,
    Globe,
    App,
    External,
    Wand,
    Qr,
    Folder,
    History,
    Link,
    Mail,
    Drive,
    Cloud,
    Server,
    Check,
    Warning,
    Close,
    Import,
    Export,
    Insert,
    Restore,
}

impl Icon {
    #[must_use]
    #[allow(clippy::too_many_lines)] // One match arm of inline SVG per glyph
    pub fn handle(self) -> svg::Handle {
        let bytes: &'static [u8] = match self {
            Self::Grid => glyph!(
                "<rect x='4' y='4' width='7' height='7' rx='1.5'/><rect x='13' y='4' width='7' height='7' rx='1.5'/>",
                "<rect x='4' y='13' width='7' height='7' rx='1.5'/><rect x='13' y='13' width='7' height='7' rx='1.5'/>"
            ),
            Self::Key => glyph!("<circle cx='8' cy='15' r='4'/><path d='M10.8 12.2 20 3M16 7l3 3M14 9l2 2'/>"),
            Self::Passkey => glyph!(
                "<circle cx='9' cy='8' r='4'/><path d='M2 21a7 7 0 0 1 11-5.7'/><circle cx='17' cy='15' r='2.5'/>",
                "<path d='M17 17.5V22M17 20h2'/>"
            ),
            Self::Timer => glyph!("<circle cx='12' cy='13' r='8'/><path d='M12 9v4l2.5 2.5M9 2h6'/>"),
            Self::Card => glyph!("<rect x='2' y='5' width='20' height='14' rx='2'/><path d='M2 10h20M6 15h4'/>"),
            Self::Note => glyph!(
                "<path d='M14 3H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9z'/><path d='M14 3v6h6M8 13h8M8 17h5'/>"
            ),
            Self::Send => glyph!("<path d='M22 2 11 13M22 2l-7 20-4-9-9-4z'/>"),
            Self::Shield => {
                glyph!("<path d='M12 3 4 6v6c0 5 3.4 8.4 8 9 4.6-.6 8-4 8-9V6z'/><path d='M12 9v4M12 16h.01'/>")
            }
            Self::Trash => {
                glyph!("<path d='M4 7h16M10 11v6M14 11v6M5 7l1 13a1 1 0 0 0 1 1h10a1 1 0 0 0 1-1l1-13M9 7V4h6v3'/>")
            }
            Self::Lock => {
                glyph!("<rect x='5' y='11' width='14' height='10' rx='2'/><path d='M8 11V7a4 4 0 0 1 8 0v4'/>")
            }
            Self::Sync => {
                glyph!("<path d='M20 11a8 8 0 0 0-14.3-4.9L4 8M4 4v4h4M4 13a8 8 0 0 0 14.3 4.9L20 16M20 20v-4h-4'/>")
            }
            Self::Plus => glyph!("<path d='M12 5v14M5 12h14'/>"),
            Self::Search => glyph!("<circle cx='11' cy='11' r='7'/><path d='m20 20-3.5-3.5'/>"),
            Self::Sort => glyph!("<path d='M7 4v16M4 7l3-3 3 3M17 20V4M14 17l3 3 3-3'/>"),
            Self::Edit => glyph!("<path d='M4 20h4L19 9l-4-4L4 16z'/>"),
            Self::More => glyph!(
                "<circle cx='5' cy='12' r='1' fill='currentColor'/><circle cx='12' cy='12' r='1' fill='currentColor'/>",
                "<circle cx='19' cy='12' r='1' fill='currentColor'/>"
            ),
            Self::Copy => {
                glyph!("<rect x='9' y='9' width='11' height='11' rx='2'/><path d='M5 15V6a2 2 0 0 1 2-2h8'/>")
            }
            Self::Eye => {
                glyph!("<path d='M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7S2 12 2 12z'/><circle cx='12' cy='12' r='3'/>")
            }
            Self::EyeOff => glyph!(
                "<path d='M3 3l18 18M10.6 5.1A10 10 0 0 1 12 5c6.4 0 10 7 10 7a17 17 0 0 1-3.2 4.1'/>",
                "<path d='M6.6 6.6C3.9 8.3 2 12 2 12s3.6 7 10 7a9.6 9.6 0 0 0 5.4-1.6M9.9 9.9a3 3 0 0 0 4.2 4.2'/>"
            ),
            Self::Globe => glyph!(
                "<circle cx='12' cy='12' r='9'/><path d='M3 12h18M12 3a14 14 0 0 1 0 18M12 3a14 14 0 0 0 0 18'/>"
            ),
            Self::App => glyph!("<rect x='3' y='4' width='18' height='16' rx='2'/><path d='M3 9h18'/>"),
            Self::External => {
                glyph!("<path d='M14 4h6v6M20 4l-9 9M18 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h5'/>")
            }
            Self::Wand => glyph!("<path d='M4 20 15 9M14 4v3M12.5 5.5h3M19 9v3M17.5 10.5h3M8 3v2M7 4h2'/>"),
            Self::Qr => glyph!(
                "<rect x='3' y='3' width='7' height='7' rx='1'/><rect x='14' y='3' width='7' height='7' rx='1'/>",
                "<rect x='3' y='14' width='7' height='7' rx='1'/><path d='M14 14h3v3M21 14v.01M17 21h4v-4M14 21v-3'/>"
            ),
            Self::Folder => {
                glyph!("<path d='M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z'/>")
            }
            Self::History => glyph!("<circle cx='12' cy='12' r='9'/><path d='M12 7v5l3 2'/>"),
            Self::Link => glyph!(
                "<path d='M10 14a5 5 0 0 0 7 0l3-3a5 5 0 0 0-7-7l-1 1M14 10a5 5 0 0 0-7 0l-3 3a5 5 0 0 0 7 7l1-1'/>"
            ),
            Self::Mail => glyph!("<rect x='3' y='5' width='18' height='14' rx='2'/><path d='m3 7 9 6 9-6'/>"),
            Self::Drive => {
                glyph!("<rect x='3' y='13' width='18' height='7' rx='2'/><path d='M5 13 7.5 5h9L19 13M7 16.5h.01'/>")
            }
            Self::Cloud => glyph!("<path d='M7 18a5 5 0 0 1-.5-10A6 6 0 0 1 18 8.5 4.5 4.5 0 0 1 17.5 18z'/>"),
            Self::Server => glyph!(
                "<rect x='3' y='4' width='18' height='7' rx='2'/><rect x='3' y='13' width='18' height='7' rx='2'/>",
                "<path d='M7 7.5h.01M7 16.5h.01'/>"
            ),
            Self::Check => glyph!("<path d='m5 12 5 5 9-10'/>"),
            Self::Warning => glyph!("<path d='M12 3 2 20h20z'/><path d='M12 10v4M12 17h.01'/>"),
            Self::Close => glyph!("<path d='M6 6l12 12M18 6 6 18'/>"),
            Self::Import => glyph!("<path d='M12 15V3M7 10l5 5 5-5M4 17v2a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-2'/>"),
            Self::Export => glyph!("<path d='M12 3v12M7 8l5-5 5 5M4 17v2a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-2'/>"),
            Self::Insert => glyph!("<path d='M10 17l5-5-5-5M15 12H3M15 4h4a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2h-4'/>"),
            Self::Restore => glyph!("<path d='M3 12a9 9 0 1 0 3-6.7L3 8M3 3v5h5'/>"),
        };
        svg::Handle::from_memory(bytes)
    }
}

impl glide_sdk::widget::Glyph for Icon {
    fn draw<'a, M: 'a>(self, side: f32, tint: Color) -> Element<'a, M> {
        glide_sdk::widget::svg_glyph(self.handle(), side, tint)
    }
}

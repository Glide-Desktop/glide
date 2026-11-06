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
    Recent,
    Book,
    Pen,
    Heart,
    Sign,
    Folder,
    Download,
    Plus,
    Back,
    Forward,
    Sidebar,
    Search,
    More,
    Thumbs,
    Outline,
    Bookmark,
    Single,
    Continuous,
    Spread,
    Minus,
    Letters,
    Select,
    Highlight,
    Underline,
    Strike,
    Comment,
    Ink,
    TextBox,
    Undo,
    Copy,
    Globe,
    Link,
    RotateLeft,
    RotateRight,
    Screen,
    Fullscreen,
    Print,
    Save,
    Pages,
    Scan,
    Info,
    External,
    Keyboard,
    Paper,
    FitWidth,
    FitPage,
    FitHeight,
    Standard,
    Actual,
    Trash,
    Close,
    Lock,
    Eye,
    Warning,
    Broken,
    Shield,
    Check,
    Pause,
    Laser,
    Presenter,
}

impl Icon {
    #[must_use]
    #[allow(clippy::too_many_lines)] // One match arm of inline SVG per glyph
    pub fn handle(self) -> svg::Handle {
        let bytes: &'static [u8] = match self {
            Self::Recent => glyph!("<circle cx='12' cy='12' r='9'/><path d='M12 7v5l3 2'/>"),
            Self::Book => {
                glyph!("<path d='M4 5a2 2 0 0 1 2-2h5v18H6a2 2 0 0 1-2-2zM13 3h5a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2h-5z'/>")
            }
            Self::Pen => glyph!("<path d='M4 20h4L19 9l-4-4L4 16z'/><path d='M13 7l4 4'/>"),
            Self::Heart => glyph!("<path d='M12 19l-7-6.5a4.5 4.5 0 0 1 7-5.6 4.5 4.5 0 0 1 7 5.6z'/>"),
            Self::Sign => glyph!("<path d='M3 21c3-1 5-3 6-6l9-9 3 3-9 9c-3 1-5 3-6 6zM15 6l3 3'/>"),
            Self::Folder => {
                glyph!("<path d='M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z'/>")
            }
            Self::Download => glyph!("<path d='M12 4v11m-5-5 5 5 5-5M5 20h14'/>"),
            Self::Plus => glyph!("<path d='M12 5v14M5 12h14'/>"),
            Self::Back => glyph!("<path d='m15 18-6-6 6-6'/>"),
            Self::Forward => glyph!("<path d='m9 18 6-6-6-6'/>"),
            Self::Sidebar => glyph!("<rect x='3' y='4' width='18' height='16' rx='2'/><path d='M9 4v16'/>"),
            Self::Search => glyph!("<circle cx='11' cy='11' r='7'/><path d='m20 20-3.5-3.5'/>"),
            Self::More => glyph!(
                "<circle cx='5' cy='12' r='1' fill='currentColor'/><circle cx='12' cy='12' r='1' fill='currentColor'/>",
                "<circle cx='19' cy='12' r='1' fill='currentColor'/>"
            ),
            Self::Thumbs => glyph!(
                "<rect x='4' y='3' width='7' height='9' rx='1'/><rect x='13' y='3' width='7' height='9' rx='1'/>",
                "<rect x='4' y='14' width='7' height='7' rx='1'/><rect x='13' y='14' width='7' height='7' rx='1'/>"
            ),
            Self::Outline => glyph!("<path d='M4 6h16M8 12h12M8 18h12M4 12h.01M4 18h.01'/>"),
            Self::Bookmark => glyph!("<path d='M6 3h12v18l-6-4-6 4z'/>"),
            Self::Single | Self::FitPage => glyph!("<rect x='6' y='3' width='12' height='18' rx='1'/>"),
            Self::Continuous => glyph!(
                "<rect x='6' y='2' width='12' height='9' rx='1'/><rect x='6' y='13' width='12' height='9' rx='1'/>"
            ),
            Self::Spread => glyph!(
                "<rect x='2' y='5' width='9' height='14' rx='1'/><rect x='13' y='5' width='9' height='14' rx='1'/>"
            ),
            Self::Minus => glyph!("<path d='M6 12h12'/>"),
            Self::Letters => glyph!("<path d='M4 19 9 5l5 14M6 14h6M15 19l3-8 3 8M16 17h4'/>"),
            Self::Select => glyph!("<path d='M5 3l14 8-6 2-3 6z'/>"),
            Self::Highlight => glyph!("<path d='M9 11l-5 5v4h4l5-5M14 6l4 4-5 5-4-4zM3 21h7'/>"),
            Self::Underline => glyph!("<path d='M7 4v7a5 5 0 0 0 10 0V4M5 21h14'/>"),
            Self::Strike => glyph!("<path d='M5 12h14M16 6a4 3 0 0 0-8 1c0 4 8 3 8 7a4 3 0 0 1-8 1'/>"),
            Self::Comment => glyph!("<path d='M5 5h14v10H9l-4 4z'/>"),
            Self::Ink => glyph!("<path d='M3 17c3-4 5-8 8-6s-1 6 2 7 4-5 8-6'/>"),
            Self::TextBox => glyph!("<path d='M5 6V4h14v2M12 4v16M9 20h6'/>"),
            Self::Undo => glyph!("<path d='M9 14 4 9l5-5'/><path d='M4 9h11a5 5 0 0 1 0 10h-3'/>"),
            Self::Copy => glyph!(
                "<rect x='8' y='8' width='13' height='13' rx='2'/><path d='M16 8V5a2 2 0 0 0-2-2H5a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h3'/>"
            ),
            Self::Globe => glyph!(
                "<circle cx='12' cy='12' r='9'/><path d='M3 12h18M12 3a14 14 0 0 1 0 18M12 3a14 14 0 0 0 0 18'/>"
            ),
            Self::Link => glyph!(
                "<path d='M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1 1M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1'/>"
            ),
            Self::RotateLeft => glyph!("<path d='M4 12a8 8 0 1 0 3-6.2M4 4v5h5'/>"),
            Self::RotateRight => glyph!("<path d='M20 12a8 8 0 1 1-3-6.2M20 4v5h-5'/>"),
            Self::Screen => glyph!("<rect x='3' y='4' width='18' height='12' rx='2'/><path d='M8 20h8M12 16v4'/>"),
            Self::Fullscreen => glyph!("<path d='M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5'/>"),
            Self::Print => glyph!(
                "<path d='M7 9V3h10v6M7 17H5a2 2 0 0 1-2-2v-4a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2v4a2 2 0 0 1-2 2h-2M7 14h10v7H7z'/>"
            ),
            Self::Save => glyph!("<path d='M5 3h11l3 3v15H5zM8 3v5h7M8 21v-7h8v7'/>"),
            Self::Pages => glyph!(
                "<rect x='3' y='3' width='7' height='9' rx='1'/><rect x='14' y='3' width='7' height='9' rx='1'/><rect x='3' y='15' width='18' height='6' rx='1'/>"
            ),
            Self::Scan => glyph!("<path d='M4 7V4h3M17 4h3v3M20 17v3h-3M7 20H4v-3M8 9h8M8 13h8M8 17h5'/>"),
            Self::Info => glyph!("<circle cx='12' cy='12' r='9'/><path d='M12 11v5M12 8h.01'/>"),
            Self::External => {
                glyph!("<path d='M14 4h6v6M20 4l-9 9M18 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h5'/>")
            }
            Self::Keyboard => glyph!(
                "<rect x='3' y='6' width='18' height='12' rx='2'/><path d='M7 10h.01M11 10h.01M15 10h.01M7 14h10'/>"
            ),
            Self::Paper => glyph!("<path d='M6 3h9l4 4v14H6zM14 3v5h5'/>"),
            Self::FitWidth => glyph!("<path d='M3 12h18M7 8l-4 4 4 4M17 8l4 4-4 4'/>"),
            Self::FitHeight => glyph!("<path d='M12 3v18M8 7l4-4 4 4M8 17l4 4 4-4'/>"),
            Self::Standard => glyph!("<path d='M6 8l2-1.5V17M16 8l2-1.5V17M12 10.5h.01M12 14.5h.01'/>"),
            Self::Actual => glyph!("<circle cx='11' cy='11' r='7'/><path d='m20 20-3.5-3.5M8 11h6M11 8v6'/>"),
            Self::Trash => glyph!("<path d='M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13'/>"),
            Self::Close => glyph!("<path d='M7 7l10 10M17 7 7 17'/>"),
            Self::Lock => {
                glyph!("<rect x='5' y='11' width='14' height='10' rx='2'/><path d='M8 11V7a4 4 0 0 1 8 0v4'/>")
            }
            Self::Eye => {
                glyph!("<path d='M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12z'/><circle cx='12' cy='12' r='3'/>")
            }
            Self::Warning => glyph!(
                "<path d='M12 9v4M12 17h.01M10.3 3.9 2.4 18a2 2 0 0 0 1.7 3h15.8a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0z'/>"
            ),
            Self::Broken => glyph!(
                "<path d='M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z'/><path d='m10 12 4 4M14 12l-4 4'/>"
            ),
            Self::Shield => glyph!("<path d='M12 3 4 6v6c0 5 3.5 8 8 9 4.5-1 8-4 8-9V6z'/><path d='m9 12 2 2 4-4'/>"),
            Self::Check => glyph!("<path d='m5 12 5 5 9-10'/>"),
            Self::Pause => glyph!("<path d='M9 5v14M15 5v14'/>"),
            Self::Laser => glyph!("<path d='M12 19V5M5 12l7-7 7 7'/>"),
            Self::Presenter => glyph!(
                "<rect x='2' y='4' width='13' height='10' rx='1.5'/><rect x='17' y='8' width='5' height='10' rx='1'/>"
            ),
        };
        svg::Handle::from_memory(bytes)
    }
}

impl glide_sdk::widget::Glyph for Icon {
    fn draw<'a, M: 'a>(self, side: f32, tint: Color) -> Element<'a, M> {
        glide_sdk::widget::svg_glyph(self.handle(), side, tint)
    }
}

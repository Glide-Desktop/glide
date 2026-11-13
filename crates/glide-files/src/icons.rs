use iced::widget::svg;

macro_rules! glyph {
    ($($body:literal),+) => {
        concat!(
            "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' ",
            "stroke='currentColor' stroke-width='1.75' stroke-linecap='round' stroke-linejoin='round'>",
            $($body,)+
            "</svg>"
        )
        .as_bytes()
    };
}

macro_rules! page {
    ($extra:literal) => {
        glyph!(
            "<path d='M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z' fill='currentColor' fill-opacity='.12'/><path d='M14 3v5h5'/>",
            $extra
        )
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Icon {
    Folder,
    Home,
    Documents,
    Downloads,
    Pictures,
    Music,
    Videos,
    Desktop,
    Trash,
    Drive,
    Usb,
    Eject,
    File,
    FileText,
    FileCode,
    FileImage,
    FileAudio,
    FileVideo,
    FileArchive,
    FilePdf,
    FileBinary,
    ChevronLeft,
    ChevronRight,
    Search,
    Grid,
    List,
    More,
    Maximize,
    Close,
    ArrowUp,
    ArrowDown,
    Link,
    Pause,
    Play,
    Copy,
    Move,
    Phone,
    Camera,
    Check,
    FolderPlus,
    Cut,
    Terminal,
    Open,
    Window,
    Tab,
    Paste,
    Duplicate,
    Pencil,
    Tag,
    Info,
    Refresh,
    Star,
    Undo,
    Redo,
    SelectAll,
    Invert,
    Filter,
    Eye,
    Sort,
    Columns,
    Keyboard,
    Compress,
    Extract,
    Restore,
    Clock,
    Path,
    Wrench,
    Lock,
    Queue,
    Panel,
    Apps,
    FilePlus,
    Hash,
    Shield,
    Zoom,
    Network,
}

impl Icon {
    #[must_use]
    #[allow(clippy::too_many_lines)] // One match arm of inline SVG per glyph
    pub fn handle(self) -> svg::Handle {
        let bytes: &'static [u8] = match self {
            Self::Folder => glyph!(
                "<path d='M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z' fill='currentColor' fill-opacity='.22'/><path d='M3 10h18'/>"
            ),
            Self::Home => {
                glyph!("<path d='m3 10 9-7 9 7v10a1 1 0 0 1-1 1h-5v-6H9v6H4a1 1 0 0 1-1-1z'/>")
            }
            Self::Documents => page!("<path d='M9 13h6M9 17h6'/>"),
            Self::Downloads => {
                glyph!("<path d='M12 4v11'/><path d='m7 10 5 5 5-5'/><path d='M5 20h14'/>")
            }
            Self::Pictures => glyph!(
                "<rect x='3' y='4' width='18' height='16' rx='2'/><circle cx='9' cy='10' r='2'/><path d='m21 16-5-5-9 9'/>"
            ),
            Self::Music => {
                glyph!("<path d='M9 18V5l11-2v13'/><circle cx='6' cy='18' r='3'/><circle cx='17' cy='16' r='3'/>")
            }
            Self::Videos => glyph!("<rect x='3' y='5' width='14' height='14' rx='2'/><path d='m17 10 4-2.5v9L17 14'/>"),
            Self::Desktop => glyph!("<rect x='2' y='4' width='20' height='13' rx='2'/><path d='M8 21h8M12 17v4'/>"),
            Self::Trash => glyph!(
                "<path d='M4 7h16'/><path d='M10 11v6M14 11v6'/><path d='M6 7l1 12a2 2 0 0 0 2 2h6a2 2 0 0 0 2-2l1-12'/><path d='M9 7V4h6v3'/>"
            ),
            Self::Drive => glyph!(
                "<rect x='3' y='13' width='18' height='7' rx='2'/><path d='M5 13 7.5 5h9L19 13'/><path d='M7 16.5h.01M11 16.5h.01'/>"
            ),
            Self::Usb => glyph!(
                "<rect x='6' y='9' width='12' height='12' rx='2' fill='currentColor' fill-opacity='.12'/><path d='M8 9V3h8v6'/><path d='M10.5 5.5h.01M13.5 5.5h.01'/>"
            ),
            Self::Eject => glyph!("<path d='M12 5 5 14h14z'/><path d='M5 19h14'/>"),
            Self::File => page!(""),
            Self::FileText => page!("<path d='M9 13h6M9 17h4'/>"),
            Self::FileCode => page!("<path d='m10 12-2 2.5 2 2.5M14 12l2 2.5-2 2.5'/>"),
            Self::FileImage => {
                page!("<circle cx='10' cy='12.5' r='1.5'/><path d='m17 18-3-3-5 5'/>")
            }
            Self::FileAudio => page!("<circle cx='10' cy='17' r='2'/><path d='M12 17v-6l3 1'/>"),
            Self::FileVideo => page!("<path d='m10 12 5 3-5 3z'/>"),
            Self::FileArchive => page!("<path d='M10 7h1M10 10h1M10 13h1M9.5 16h2v3h-2z'/>"),
            Self::FilePdf => page!("<path d='M9 18v-5h2a1.5 1.5 0 0 1 0 3H9'/>"),
            Self::FileBinary => page!("<path d='m9 13 2 2-2 2M13 17h3'/>"),
            Self::ChevronLeft => glyph!("<path d='m15 18-6-6 6-6'/>"),
            Self::ChevronRight => glyph!("<path d='m9 18 6-6-6-6'/>"),
            Self::Search => glyph!("<circle cx='11' cy='11' r='7'/><path d='m20 20-3.5-3.5'/>"),
            Self::Grid => glyph!(
                "<rect x='4' y='4' width='6.5' height='6.5' rx='1.5'/><rect x='13.5' y='4' width='6.5' height='6.5' rx='1.5'/><rect x='4' y='13.5' width='6.5' height='6.5' rx='1.5'/><rect x='13.5' y='13.5' width='6.5' height='6.5' rx='1.5'/>"
            ),
            Self::List => glyph!("<path d='M9 6h11M9 12h11M9 18h11M4 6h.01M4 12h.01M4 18h.01'/>"),
            Self::More => glyph!(
                "<circle cx='5' cy='12' r='1' fill='currentColor'/><circle cx='12' cy='12' r='1' fill='currentColor'/><circle cx='19' cy='12' r='1' fill='currentColor'/>"
            ),
            Self::Maximize => glyph!("<rect x='6' y='6' width='12' height='12' rx='2'/>"),
            Self::Close => glyph!("<path d='M18 6 6 18M6 6l12 12'/>"),
            Self::ArrowUp => glyph!("<path d='M12 19V5M6 11l6-6 6 6'/>"),
            Self::ArrowDown => glyph!("<path d='M12 5v14M6 13l6 6 6-6'/>"),
            Self::Link => glyph!("<path d='M7 17 17 7M9 7h8v8'/>"),
            Self::Pause => glyph!("<path d='M9 5v14M15 5v14'/>"),
            Self::Play => glyph!("<path d='M8 5v14l11-7z'/>"),
            Self::Copy => glyph!(
                "<rect x='8' y='8' width='12' height='12' rx='2'/><path d='M16 8V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h2'/>"
            ),
            Self::Move => glyph!("<path d='M5 12h14M13 6l6 6-6 6'/>"),
            Self::FolderPlus => glyph!(
                "<path d='M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z'/><path d='M12 10.5v5M9.5 13h5'/>"
            ),
            Self::Phone => glyph!("<rect x='7' y='2.5' width='10' height='19' rx='2'/><path d='M11 18.5h2'/>"),
            Self::Camera => glyph!(
                "<path d='M4 8h3l2-3h6l2 3h3a1 1 0 0 1 1 1v9a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V9a1 1 0 0 1 1-1z'/><circle cx='12' cy='13' r='3.5'/>"
            ),
            Self::Check => glyph!("<path d='m5 12.5 4.5 4.5L19 7.5'/>"),
            Self::Cut => glyph!(
                "<circle cx='6' cy='6' r='3'/><circle cx='6' cy='18' r='3'/><path d='M20 4 8.1 15.9M14.5 14.5 20 20M8.1 8.1 12 12'/>"
            ),
            Self::Terminal => glyph!("<path d='m4 17 6-5-6-5M12 19h8'/>"),
            Self::Open => glyph!(
                "<path d='M11 5H6a2 2 0 0 0-2 2v11a2 2 0 0 0 2 2h11a2 2 0 0 0 2-2v-5'/><path d='M14 4h6v6M20 4l-9 9'/>"
            ),
            Self::Window => glyph!("<rect x='3' y='4' width='18' height='16' rx='2'/><path d='M3 9h18'/>"),
            Self::Tab => glyph!(
                "<path d='M3 19V7a2 2 0 0 1 2-2h5a2 2 0 0 1 2 2v2h7a2 2 0 0 1 2 2v8z'/><path d='M16.5 12.5v4M14.5 14.5h4'/>"
            ),
            Self::Paste => glyph!(
                "<rect x='5' y='5' width='14' height='16' rx='2'/><path d='M9 5V4a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v1'/><path d='M9 12h6M9 16h4'/>"
            ),
            Self::Duplicate => glyph!(
                "<rect x='8' y='8' width='12' height='12' rx='2'/><path d='M16 8V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h2'/><path d='M14 11.5v5M11.5 14h5'/>"
            ),
            Self::Pencil => glyph!("<path d='M4 20h4L19 9l-4-4L4 16z'/><path d='m13 7 4 4'/>"),
            Self::Tag => glyph!("<path d='M3 12V4h8l10 10-8 8z'/><path d='M7.5 8h.01'/>"),
            Self::Info => glyph!("<circle cx='12' cy='12' r='9'/><path d='M12 11v6M12 7.5h.01'/>"),
            Self::Refresh => glyph!(
                "<path d='M20 11a8 8 0 0 0-14.5-4L4 9'/><path d='M4 4v5h5'/><path d='M4 13a8 8 0 0 0 14.5 4L20 15'/><path d='M20 20v-5h-5'/>"
            ),
            Self::Star => {
                glyph!("<path d='m12 3 2.7 5.6 6.1.8-4.5 4.3 1.1 6.1L12 16.9l-5.4 2.9 1.1-6.1-4.5-4.3 6.1-.8z'/>")
            }
            Self::Undo => glyph!("<path d='M9 14 4 9l5-5'/><path d='M4 9h10a6 6 0 0 1 0 12h-3'/>"),
            Self::Redo => glyph!("<path d='m15 14 5-5-5-5'/><path d='M20 9H10a6 6 0 0 0 0 12h3'/>"),
            Self::SelectAll => glyph!(
                "<rect x='4' y='4' width='16' height='16' rx='2' stroke-dasharray='3 3'/><path d='m8.5 12.5 2.5 2.5 4.5-5'/>"
            ),
            Self::Invert => glyph!(
                "<rect x='4' y='4' width='16' height='16' rx='2'/><path d='M4 20 20 4'/><path d='M12 4h6a2 2 0 0 1 2 2v6z' fill='currentColor' fill-opacity='.3'/>"
            ),
            Self::Filter => glyph!("<path d='M4 5h16l-6 8v6l-4-2v-4z'/>"),
            Self::Eye => glyph!(
                "<path d='M2.5 12S6 5.5 12 5.5 21.5 12 21.5 12 18 18.5 12 18.5 2.5 12 2.5 12z'/><circle cx='12' cy='12' r='2.5'/>"
            ),
            Self::Sort => glyph!("<path d='M7 4v16M3.5 16.5 7 20l3.5-3.5'/><path d='M17 20V4M13.5 7.5 17 4l3.5 3.5'/>"),
            Self::Columns => glyph!("<rect x='3' y='4' width='18' height='16' rx='2'/><path d='M9 4v16M15 4v16'/>"),
            Self::Keyboard => glyph!(
                "<rect x='2' y='6' width='20' height='12' rx='2'/><path d='M6 10h.01M10 10h.01M14 10h.01M18 10h.01M7 14h10'/>"
            ),
            Self::Compress => glyph!(
                "<path d='M4 8h16v11a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1z'/><path d='M3 4h18v4H3z'/><path d='M12 11v6M9.5 14.5 12 17l2.5-2.5'/>"
            ),
            Self::Extract => glyph!(
                "<path d='M4 8h16v11a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1z'/><path d='M3 4h18v4H3z'/><path d='M12 17v-6M9.5 13.5 12 11l2.5 2.5'/>"
            ),
            Self::Restore => glyph!("<path d='M4 12a8 8 0 1 0 2.5-5.8L4 8.5'/><path d='M4 4v4.5h4.5'/>"),
            Self::Clock => glyph!("<circle cx='12' cy='12' r='9'/><path d='M12 7v5l3.5 2'/>"),
            Self::Path => glyph!("<path d='M4 6h3M4 18h3M5.5 6v12'/><path d='M11 12h9M16.5 8.5 20 12l-3.5 3.5'/>"),
            Self::Wrench => glyph!(
                "<path d='M14.5 6.5a4.5 4.5 0 0 0-6 5.6L3 17.6V21h3.4l5.5-5.5a4.5 4.5 0 0 0 5.6-6L14.5 12.5 11.5 9.5z'/>"
            ),
            Self::Lock => {
                glyph!("<rect x='5' y='11' width='14' height='10' rx='2'/><path d='M8 11V8a4 4 0 0 1 8 0v3'/>")
            }
            Self::Queue => glyph!("<path d='M9 6h11M9 12h11M9 18h11'/><path d='M4 5v2M4 11v2M4 17v2'/>"),
            Self::Panel => glyph!("<rect x='3' y='4' width='18' height='16' rx='2'/><path d='M15 4v16'/>"),
            Self::Apps => glyph!(
                "<rect x='4' y='4' width='6.5' height='6.5' rx='1.5'/><rect x='13.5' y='4' width='6.5' height='6.5' rx='1.5'/><rect x='4' y='13.5' width='6.5' height='6.5' rx='1.5'/><path d='M16.75 14v6M13.75 17h6'/>"
            ),
            Self::FilePlus => page!("<path d='M12 12v6M9 15h6'/>"),
            Self::Hash => glyph!("<path d='M9 4 7 20M17 4l-2 16M4 9h17M3 15h17'/>"),
            Self::Shield => glyph!("<path d='M12 3 4.5 6v6c0 4.5 3.2 7.6 7.5 9 4.3-1.4 7.5-4.5 7.5-9V6z'/>"),
            Self::Zoom => glyph!("<circle cx='11' cy='11' r='7'/><path d='m20 20-3.5-3.5M11 8v6M8 11h6'/>"),
            Self::Network => glyph!(
                "<circle cx='12' cy='12' r='9'/><path d='M3 12h18'/><path d='M12 3c2.5 2.6 3.8 5.6 3.8 9s-1.3 6.4-3.8 9c-2.5-2.6-3.8-5.6-3.8-9S9.5 5.6 12 3z'/>"
            ),
        };
        svg::Handle::from_memory(bytes)
    }
}

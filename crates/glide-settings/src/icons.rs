use iced::widget::svg;

macro_rules! glyph {
    ($body:literal) => {
        concat!(
            "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' ",
            "stroke='black' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'>",
            $body,
            "</svg>"
        )
        .as_bytes()
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Search,
    Wifi,
    WifiOff,
    Wifi1,
    Wifi2,
    Wifi3,
    Bluetooth,
    Appearance,
    Display,
    Volume,
    VolumeMute,
    Mic,
    MicMute,
    Keyboard,
    Mouse,
    Info,
    Check,
    Lock,
    Refresh,
    ChevronRight,
    ChevronUp,
    ChevronDown,
    Close,
    Plus,
    Trash,
    Moon,
    Ethernet,
    Headphones,
    Phone,
    Logo,
    User,
    Warning,
    Sparkle,
    Dock,
    Window,
    Battery,
    Apps,
    Cpu,
    Memory,
    Shield,
    Bell,
    Image,
    TopBar,
    Grid,
    Command,
    Terminal,
    Accessibility,
    Hand,
    Users,
    Clock,
    Globe,
    Rocket,
    Drive,
    Printer,
    Airplane,
    Eye,
    Key,
    Speaker,
    Gamepad,
    Camera,
    TextSize,
    Pointer,
    Contrast,
    History,
    Download,
    Hotspot,
    Tablet,
    Link,
    Folder,
    Document,
    Music,
    Film,
    Copy,
    Pencil,
    Archive,
    External,
    Maximize,
    Gesture,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    ArrowDown,
    Pinch,
}

impl Icon {
    #[must_use]
    #[allow(clippy::too_many_lines)] // One match arm of inline SVG per glyph
    pub const fn bytes(self) -> &'static [u8] {
        match self {
            Self::Search => glyph!("<circle cx='11' cy='11' r='7'/><path d='M16.5 16.5 21 21'/>"),
            Self::Wifi => glyph!(
                "<path d='M2 8.8a15 15 0 0 1 20 0'/><path d='M5 12.3a10 10 0 0 1 14 0'/><path d='M8.5 15.8a5 5 0 0 1 7 0'/><circle cx='12' cy='19.5' r='1' fill='black'/>"
            ),
            Self::Wifi3 => glyph!(
                "<path d='M2 8.8a15 15 0 0 1 20 0' stroke-opacity='0.3'/><path d='M5 12.3a10 10 0 0 1 14 0'/><path d='M8.5 15.8a5 5 0 0 1 7 0'/><circle cx='12' cy='19.5' r='1' fill='black'/>"
            ),
            Self::Wifi2 => glyph!(
                "<path d='M2 8.8a15 15 0 0 1 20 0' stroke-opacity='0.3'/><path d='M5 12.3a10 10 0 0 1 14 0' stroke-opacity='0.3'/><path d='M8.5 15.8a5 5 0 0 1 7 0'/><circle cx='12' cy='19.5' r='1' fill='black'/>"
            ),
            Self::Wifi1 => glyph!(
                "<path d='M2 8.8a15 15 0 0 1 20 0' stroke-opacity='0.3'/><path d='M5 12.3a10 10 0 0 1 14 0' stroke-opacity='0.3'/><path d='M8.5 15.8a5 5 0 0 1 7 0' stroke-opacity='0.3'/><circle cx='12' cy='19.5' r='1' fill='black'/>"
            ),
            Self::WifiOff => glyph!(
                "<path d='M2 8.8a15 15 0 0 1 4.2-2.9M10.7 4.6A15 15 0 0 1 22 8.8'/><path d='M5 12.3a10 10 0 0 1 5-2.6'/><path d='M8.5 15.8a5 5 0 0 1 7 0'/><path d='M3 3l18 18'/>"
            ),
            Self::Bluetooth => glyph!("<path d='M7 7l10 10-5 5V2l5 5L7 17'/>"),
            Self::Appearance => glyph!("<circle cx='12' cy='12' r='9'/><path d='M12 3a9 9 0 0 1 0 18z' fill='black'/>"),
            Self::Display => glyph!("<rect x='2' y='3.5' width='20' height='13.5' rx='2'/><path d='M8 21h8M12 17v4'/>"),
            Self::Volume => glyph!(
                "<path d='M11 5 6 9H2v6h4l5 4z' fill='black'/><path d='M15.5 8.5a5 5 0 0 1 0 7'/><path d='M19 5a10 10 0 0 1 0 14'/>"
            ),
            Self::VolumeMute => glyph!("<path d='M11 5 6 9H2v6h4l5 4z' fill='black'/><path d='M16 9l6 6M22 9l-6 6'/>"),
            Self::Mic => {
                glyph!("<rect x='9' y='2' width='6' height='12' rx='3'/><path d='M5 10v1a7 7 0 0 0 14 0v-1M12 18v4'/>")
            }
            Self::MicMute => glyph!(
                "<path d='M15 9.3V5a3 3 0 0 0-5.7-1.3M9 9v2a3 3 0 0 0 5 2.2'/><path d='M19 10v1a7 7 0 0 1-.9 3.4M5 10v1a7 7 0 0 0 11.5 5.4M12 18v4M3 3l18 18'/>"
            ),
            Self::Keyboard => glyph!(
                "<rect x='2' y='5' width='20' height='14' rx='2'/><path d='M6 9h.01M10 9h.01M14 9h.01M18 9h.01M6 12.5h.01M10 12.5h.01M14 12.5h.01M18 12.5h.01M7.5 16h9'/>"
            ),
            Self::Mouse => {
                glyph!("<rect x='6' y='2.5' width='12' height='19' rx='6'/><path d='M12 6.5v4'/>")
            }
            Self::Info => {
                glyph!("<circle cx='12' cy='12' r='9'/><path d='M12 11v5.5M12 7.6h.01'/>")
            }
            Self::Check => glyph!("<path d='M5 12.5l4.5 4.5L19 7.5'/>"),
            Self::Lock => {
                glyph!("<rect x='5' y='11' width='14' height='10' rx='2'/><path d='M8 11V7.5a4 4 0 0 1 8 0V11'/>")
            }
            Self::Refresh => {
                glyph!("<path d='M20.5 12a8.5 8.5 0 1 1-2.5-6l2.5 2.5'/><path d='M20.5 3.5v5h-5'/>")
            }
            Self::ChevronRight => glyph!("<path d='M9 6l6 6-6 6'/>"),
            Self::ChevronUp => glyph!("<path d='M6 15l6-6 6 6'/>"),
            Self::ChevronDown => glyph!("<path d='M6 9l6 6 6-6'/>"),
            Self::Close => glyph!("<path d='M6 6l12 12M18 6 6 18'/>"),
            Self::Plus => glyph!("<path d='M12 5v14M5 12h14'/>"),
            Self::Trash => glyph!("<path d='M4 7h16M10 11v6M14 11v6M6 7l1 13h10l1-13M9 7V4h6v3'/>"),
            Self::Moon => glyph!("<path d='M20 14.5A8 8 0 0 1 9.5 4a8 8 0 1 0 10.5 10.5z'/>"),
            Self::Ethernet => glyph!(
                "<rect x='3' y='8' width='18' height='9' rx='1.5'/><path d='M7.5 17v3M12 17v3M16.5 17v3M9 8V5h6v3'/>"
            ),
            Self::Headphones => glyph!(
                "<path d='M3 18v-6a9 9 0 0 1 18 0v6'/><rect x='3' y='14' width='4.5' height='7' rx='1.5'/><rect x='16.5' y='14' width='4.5' height='7' rx='1.5'/>"
            ),
            Self::Phone => {
                glyph!("<rect x='6.5' y='2' width='11' height='20' rx='2.5'/><path d='M11 18h2'/>")
            }
            Self::Logo => glyph!(
                "<path d='M2.5 11.2 21 3l-6.4 18-3.3-7.2z' fill='black' fill-opacity='0.18'/><path d='M11.3 13.8 21 3'/>"
            ),
            Self::User => glyph!("<circle cx='12' cy='8' r='4'/><path d='M4 21a8 8 0 0 1 16 0'/>"),
            Self::Warning => glyph!(
                "<path d='M10.3 3.9 2.4 17.5A2 2 0 0 0 4.1 20.5h15.8a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0z'/><path d='M12 9v4M12 16.8h.01'/>"
            ),
            Self::Dock => glyph!(
                "<rect x='3' y='3.5' width='18' height='12' rx='2'/><rect x='5' y='17.5' width='14' height='3.5' rx='1.75'/><path d='M8.5 19.25h.01M12 19.25h.01M15.5 19.25h.01'/>"
            ),
            Self::Window => glyph!(
                "<rect x='3' y='4' width='18' height='16' rx='2.5'/><path d='M3 9h18'/><path d='M6.5 6.5h.01M9 6.5h.01'/>"
            ),
            Self::Battery => glyph!(
                "<rect x='2.5' y='7' width='17' height='10' rx='2'/><path d='M22 11v2'/><rect x='5' y='9.5' width='8' height='5' rx='0.8' fill='black'/>"
            ),
            Self::Apps => glyph!(
                "<rect x='4' y='4' width='6.5' height='6.5' rx='1.5'/><rect x='13.5' y='4' width='6.5' height='6.5' rx='1.5'/><rect x='4' y='13.5' width='6.5' height='6.5' rx='1.5'/><rect x='13.5' y='13.5' width='6.5' height='6.5' rx='1.5'/>"
            ),
            Self::Cpu => glyph!(
                "<rect x='6' y='6' width='12' height='12' rx='2'/><path d='M9.5 2.5v3.5M14.5 2.5v3.5M9.5 18v3.5M14.5 18v3.5M2.5 9.5H6M2.5 14.5H6M18 9.5h3.5M18 14.5h3.5'/>"
            ),
            Self::Memory => glyph!(
                "<rect x='2.5' y='7' width='19' height='10' rx='1.5'/><path d='M6.5 17v3M10.5 17v3M13.5 17v3M17.5 17v3M7 10.5v3M12 10.5v3M17 10.5v3'/>"
            ),
            Self::Shield => glyph!(
                "<path d='M12 2.5 4.5 5.5v6c0 4.6 3.1 8.4 7.5 10 4.4-1.6 7.5-5.4 7.5-10v-6z'/><path d='M9 12l2 2 4-4'/>"
            ),
            Self::Bell => glyph!("<path d='M6 16V11a6 6 0 0 1 12 0v5l1.5 2h-15z'/><path d='M10 20.5a2 2 0 0 0 4 0'/>"),
            Self::Image => glyph!(
                "<rect x='3' y='4' width='18' height='16' rx='2.5'/><circle cx='9' cy='9.5' r='1.8'/><path d='M3.5 17.5l5-5 4 4 2.5-2.5 5.5 5'/>"
            ),
            Self::TopBar => {
                glyph!("<rect x='3' y='4' width='18' height='16' rx='2.5'/><path d='M3 8.5h18' stroke-width='3'/>")
            }
            Self::Grid => glyph!(
                "<rect x='3' y='5' width='8' height='6' rx='1.5'/><rect x='13' y='5' width='8' height='6' rx='1.5'/><rect x='3' y='13' width='8' height='6' rx='1.5'/><rect x='13' y='13' width='8' height='6' rx='1.5'/>"
            ),
            Self::Terminal => {
                glyph!("<rect x='3' y='4' width='18' height='16' rx='2.5'/><path d='m7.5 9.5 3 2.5-3 2.5M12.5 15h4'/>")
            }
            Self::Command => glyph!(
                "<path d='M9 9V6.5A2.5 2.5 0 1 0 6.5 9H17.5A2.5 2.5 0 1 0 15 6.5V17.5A2.5 2.5 0 1 0 17.5 15H6.5A2.5 2.5 0 1 0 9 17.5z'/>"
            ),
            Self::Accessibility => glyph!(
                "<circle cx='12' cy='4.5' r='1.8'/><path d='M4.5 8.5 12 10l7.5-1.5M12 10v4.5M12 14.5l-3.5 6.5M12 14.5l3.5 6.5'/>"
            ),
            Self::Hand => glyph!(
                "<path d='M8 12V5.5a1.5 1.5 0 0 1 3 0V11M11 10V4a1.5 1.5 0 0 1 3 0v7M14 10.5V5.5a1.5 1.5 0 0 1 3 0V14a7 7 0 0 1-7 7h-.5a6 6 0 0 1-4.6-2.2L3 16.2a1.6 1.6 0 0 1 2.4-2.1L8 16.5V9a1.5 1.5 0 0 1 3 0'/>"
            ),
            Self::Users => glyph!(
                "<circle cx='9' cy='8' r='3.5'/><path d='M2.5 20a6.5 6.5 0 0 1 13 0'/><path d='M15.5 4.8a3.5 3.5 0 0 1 0 6.4M18 14.2a6.5 6.5 0 0 1 3.5 5.8'/>"
            ),
            Self::Clock => glyph!("<circle cx='12' cy='12' r='9'/><path d='M12 7v5l3.5 2'/>"),
            Self::Globe => glyph!(
                "<circle cx='12' cy='12' r='9'/><path d='M3 12h18M12 3a14 14 0 0 1 0 18M12 3a14 14 0 0 0 0 18'/>"
            ),
            Self::Rocket => glyph!(
                "<path d='M12 15l-3-3c1.5-4.5 4.5-8 10-9-1 5.5-4.5 8.5-9 10z'/><path d='M9 12H5l2.5-3.5H11M12 15v4l3.5-2.5V13'/><path d='M6.5 16.5c-1.5 1-2 3.5-2 3.5s2.5-.5 3.5-2'/>"
            ),
            Self::Drive => glyph!(
                "<rect x='3' y='13' width='18' height='7' rx='2'/><path d='M5.5 13 8 5h8l2.5 8'/><path d='M7 16.5h.01M10.5 16.5h.01'/>"
            ),
            Self::Printer => glyph!(
                "<path d='M7 9V3.5h10V9'/><rect x='3' y='9' width='18' height='8' rx='2'/><rect x='7' y='14' width='10' height='6.5' rx='1'/>"
            ),
            Self::Sparkle => glyph!(
                "<path d='M12 3l1.9 5.1L19 10l-5.1 1.9L12 17l-1.9-5.1L5 10l5.1-1.9z'/><path d='M19 16l.7 1.8 1.8.7-1.8.7L19 21l-.7-1.8-1.8-.7 1.8-.7z'/>"
            ),
            Self::Airplane => glyph!(
                "<path d='M10.5 3.5a1.5 1.5 0 0 1 3 0V9l7.5 4.5v2l-7.5-2.3V18l2.5 2v1.5L12 20.5l-4 1V20l2.5-2v-4.8L3 15.5v-2L10.5 9z'/>"
            ),
            Self::Eye => glyph!(
                "<path d='M2 12s3.6-6.5 10-6.5S22 12 22 12s-3.6 6.5-10 6.5S2 12 2 12z'/><circle cx='12' cy='12' r='3'/>"
            ),
            Self::Key => {
                glyph!("<circle cx='7.5' cy='15.5' r='4'/><path d='M10.5 12.5 20 3M16.5 6.5l2.5 2.5M14 9l2 2'/>")
            }
            Self::Speaker => glyph!(
                "<rect x='5' y='2.5' width='14' height='19' rx='2.5'/><circle cx='12' cy='14.5' r='3.5'/><path d='M12 7h.01'/>"
            ),
            Self::Gamepad => glyph!(
                "<path d='M7 7h10a5 5 0 0 1 4.8 6.3l-1 3.7a2.7 2.7 0 0 1-4.6 1.1L14 16h-4l-2.2 2.1a2.7 2.7 0 0 1-4.6-1.1l-1-3.7A5 5 0 0 1 7 7z'/><path d='M8 10v3M6.5 11.5h3M15.5 11h.01M17.5 12.5h.01'/>"
            ),
            Self::Camera => {
                glyph!("<rect x='2.5' y='6.5' width='13.5' height='11' rx='2'/><path d='M16 10.5 21.5 7v10L16 13.5'/>")
            }
            Self::TextSize => {
                glyph!("<path d='M3 19 8 6l5 13M4.8 14.5h6.4'/><path d='M14.5 19l3.5-8.5 3.5 8.5M15.8 16h4.4'/>")
            }
            Self::Pointer => glyph!(
                "<path d='M5 3l14 7.5-6.2 1.8L10 18.5z' fill='black' fill-opacity='0.15'/><path d='M12.8 12.3 18 17.5'/>"
            ),
            Self::Contrast => {
                glyph!("<circle cx='12' cy='12' r='9'/><path d='M12 3v18M12 7.5h4.5M12 12h6M12 16.5h4.5'/>")
            }
            Self::History => glyph!(
                "<path d='M3.5 12a8.5 8.5 0 1 0 2.5-6L3.5 8.5'/><path d='M3.5 3.5v5h5'/><path d='M12 7.5V12l3 2'/>"
            ),
            Self::Download => glyph!(
                "<path d='M12 3.5v11M7.5 10l4.5 4.5 4.5-4.5'/><path d='M4 16.5v2A2 2 0 0 0 6 20.5h12a2 2 0 0 0 2-2v-2'/>"
            ),
            Self::Hotspot => glyph!(
                "<circle cx='12' cy='12' r='1.8' fill='black'/><path d='M8.5 15.5a5 5 0 0 1 0-7M15.5 8.5a5 5 0 0 1 0 7M5.5 18.5a9.2 9.2 0 0 1 0-13M18.5 5.5a9.2 9.2 0 0 1 0 13'/>"
            ),
            Self::Tablet => glyph!("<rect x='4' y='2.5' width='16' height='19' rx='2.5'/><path d='M10.5 18.5h3'/>"),
            Self::Link => glyph!(
                "<path d='M10 14a4.5 4.5 0 0 0 6.4 0l3-3a4.5 4.5 0 0 0-6.4-6.4l-1.2 1.2'/><path d='M14 10a4.5 4.5 0 0 0-6.4 0l-3 3a4.5 4.5 0 0 0 6.4 6.4l1.2-1.2'/>"
            ),
            Self::Folder => {
                glyph!("<path d='M3 7.5A2 2 0 0 1 5 5.5h4l2 2.5h8a2 2 0 0 1 2 2V17a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z'/>")
            }
            Self::Document => glyph!(
                "<path d='M6 2.5h8l4.5 4.5v12.5a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2v-15a2 2 0 0 1 2-2z'/><path d='M13.5 2.5V7.5h5M8 12.5h8M8 16.5h5'/>"
            ),
            Self::Music => glyph!(
                "<path d='M9 18V5.5l11-2V16'/><circle cx='6.5' cy='18' r='2.5'/><circle cx='17.5' cy='16' r='2.5'/>"
            ),
            Self::Film => glyph!(
                "<rect x='3' y='3.5' width='18' height='17' rx='2'/><path d='M7.5 3.5v17M16.5 3.5v17M3 8h4.5M3 12h18M3 16h4.5M16.5 8H21M16.5 16H21'/>"
            ),
            Self::Copy => glyph!(
                "<rect x='8.5' y='8.5' width='12' height='12' rx='2'/><path d='M15.5 8.5V5.5a2 2 0 0 0-2-2h-8a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h3'/>"
            ),
            Self::Pencil => {
                glyph!("<path d='M4 20h4L19 9a2.8 2.8 0 0 0-4-4L4 16z'/><path d='M13.5 6.5l4 4'/>")
            }
            Self::Archive => glyph!(
                "<rect x='3' y='4' width='18' height='4.5' rx='1'/><path d='M5 8.5V18a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8.5M10 12.5h4'/>"
            ),
            Self::External => glyph!(
                "<path d='M14 4h6v6M20 4l-9 9'/><path d='M18 14v4a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h4'/>"
            ),
            Self::Maximize => glyph!("<rect x='5' y='5' width='14' height='14' rx='2'/>"),
            Self::Gesture => glyph!(
                "<path d='M9 13V5.5a1.5 1.5 0 0 1 3 0V12M12 11.5V4a1.5 1.5 0 0 1 3 0v8M15 11V6.5a1.5 1.5 0 0 1 3 0V15a6 6 0 0 1-6 6h-1a5 5 0 0 1-4.2-2.3L5 16a1.5 1.5 0 0 1 2.5-1.7L9 16'/><path d='M2.5 6h3M3.5 4.5 2 6l1.5 1.5'/>"
            ),
            Self::ArrowLeft => glyph!("<path d='M19 12H5M11 6l-6 6 6 6'/>"),
            Self::ArrowRight => glyph!("<path d='M5 12h14M13 6l6 6-6 6'/>"),
            Self::ArrowUp => glyph!("<path d='M12 19V5M6 11l6-6 6 6'/>"),
            Self::ArrowDown => glyph!("<path d='M12 5v14M6 13l6 6 6-6'/>"),
            Self::Pinch => {
                glyph!("<path d='M4 4l5 5M4 4h4M4 4v4M20 20l-5-5M20 20h-4M20 20v-4'/><circle cx='12' cy='12' r='1.5'/>")
            }
        }
    }

    #[must_use]
    pub fn handle(self) -> svg::Handle {
        svg::Handle::from_memory(self.bytes())
    }

    #[must_use]
    pub fn for_signal(signal: u8) -> Self {
        match signal {
            75.. => Self::Wifi,
            50..=74 => Self::Wifi3,
            25..=49 => Self::Wifi2,
            _ => Self::Wifi1,
        }
    }

    #[must_use]
    pub fn for_bluetooth(kind: &str) -> Self {
        match kind {
            "audio-headset" | "audio-headphones" => Self::Headphones,
            "audio-card" | "audio-speakers" => Self::Speaker,
            "input-keyboard" => Self::Keyboard,
            "input-mouse" => Self::Mouse,
            "input-tablet" => Self::Tablet,
            "input-gaming" => Self::Gamepad,
            "phone" => Self::Phone,
            "computer" => Self::Display,
            "camera-video" | "camera-photo" => Self::Camera,
            _ => Self::Bluetooth,
        }
    }
}

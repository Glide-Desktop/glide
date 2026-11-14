use std::sync::OnceLock;

use iced::widget::svg;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    SelectAll,
    Comment,
    Duplicate,
    Upper,
    Lower,
    Find,
    Code,
    Open,
    Folder,
    File,
    Path,
    Refresh,
    Close,
    CloseOthers,
    CloseRight,
    CloseSaved,
    CloseAll,
    Indent,
    Ending,
    Check,
    Clear,
}

const COUNT: usize = Icon::Clear as usize + 1;
static HANDLES: [OnceLock<svg::Handle>; COUNT] = [const { OnceLock::new() }; COUNT];

impl Icon {
    const fn bytes(self) -> &'static [u8] {
        match self {
            Self::Undo => glyph!("<path d='M9 14 4 9l5-5'/><path d='M4 9h11a5 5 0 0 1 0 10h-3'/>"),
            Self::Redo => glyph!("<path d='m15 14 5-5-5-5'/><path d='M20 9H9a5 5 0 0 0 0 10h3'/>"),
            Self::Cut => glyph!(
                "<circle cx='6' cy='6' r='2.5'/><circle cx='6' cy='18' r='2.5'/>",
                "<path d='M8 7.5 20 18M8 16.5 20 6'/>"
            ),
            Self::Copy => {
                glyph!("<rect x='9' y='9' width='11' height='11' rx='2'/>", "<path d='M5 15V6a2 2 0 0 1 2-2h9'/>")
            }
            Self::Paste => glyph!(
                "<rect x='5' y='5' width='14' height='16' rx='2'/>",
                "<path d='M9 5V4a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v1M9 12h6M9 16h4'/>"
            ),
            Self::SelectAll => glyph!(
                "<rect x='4' y='4' width='16' height='16' rx='2' stroke-dasharray='3 3'/>",
                "<path d='M9 12l2 2 4-4'/>"
            ),
            Self::Comment => glyph!("<path d='M10 5 6 19M18 5l-4 14'/>"),
            Self::Duplicate => glyph!("<path d='M5 8h14M5 12h14M5 16h8M17 14v6M14 17h6'/>"),
            Self::Upper => glyph!("<path d='M3 18 8 6l5 12M5 14h6'/>", "<path d='M18 18V8M15 11l3-3 3 3'/>"),
            Self::Lower => {
                glyph!("<circle cx='7.5' cy='14' r='4'/><path d='M11.5 10v8'/>", "<path d='M18 8v10M15 15l3 3 3-3'/>")
            }
            Self::Find => glyph!("<circle cx='11' cy='11' r='6'/><path d='m20 20-4.5-4.5'/>"),
            Self::Code => glyph!("<path d='m8 7-5 5 5 5M16 7l5 5-5 5M13.5 5l-3 14'/>"),
            Self::Open => {
                glyph!("<path d='M14 4h6v6M20 4l-9 9M18 14v4a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h4'/>")
            }
            Self::Folder => {
                glyph!("<path d='M3.5 7.5a2 2 0 0 1 2-2h4l2 2h7a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2z'/>")
            }
            Self::File => glyph!(
                "<path d='M7 3.5h7l4.5 4.5v11a1.5 1.5 0 0 1-1.5 1.5H7A1.5 1.5 0 0 1 5.5 19V5A1.5 1.5 0 0 1 7 3.5z'/>",
                "<path d='M14 3.5V8h4.5'/>"
            ),
            Self::Path => glyph!(
                "<path d='M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1 1'/>",
                "<path d='M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1'/>"
            ),
            Self::Refresh => glyph!("<path d='M20 12a8 8 0 1 1-2.6-5.9L20 8M20 3v5h-5'/>"),
            Self::Close => glyph!("<path d='M7 7l10 10M17 7 7 17'/>"),
            Self::CloseOthers => glyph!(
                "<rect x='8' y='5' width='8' height='14' rx='2'/>",
                "<path d='M2.5 9.5l3 3M5.5 9.5l-3 3M18.5 9.5l3 3M21.5 9.5l-3 3'/>"
            ),
            Self::CloseRight => {
                glyph!("<rect x='3' y='5' width='8' height='14' rx='2'/>", "<path d='M15 9l6 6M21 9l-6 6'/>")
            }
            Self::CloseSaved => glyph!("<path d='M4 12.5l4 4 8-9'/>", "<path d='M16 15l5 5M21 15l-5 5'/>"),
            Self::CloseAll => {
                glyph!("<rect x='3' y='3' width='18' height='18' rx='3'/>", "<path d='M9 9l6 6M15 9l-6 6'/>")
            }
            Self::Indent => glyph!("<path d='M4 6h16M11 12h9M4 18h16M4 9.5l3 2.5-3 2.5'/>"),
            Self::Ending => glyph!("<path d='M20 5v6a3 3 0 0 1-3 3H5M9 10l-4 4 4 4'/>"),
            Self::Check => glyph!("<rect x='4' y='4' width='16' height='16' rx='3'/><path d='M8.5 12l2.5 2.5 4.5-5'/>"),
            Self::Clear => glyph!("<path d='M5 7h14M10 7V4h4v3M7 7l1 13h8l1-13'/>"),
        }
    }

    #[must_use]
    pub fn handle(self) -> svg::Handle {
        HANDLES[self as usize].get_or_init(|| svg::Handle::from_memory(self.bytes())).clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Icon; COUNT] = [
        Icon::Undo,
        Icon::Redo,
        Icon::Cut,
        Icon::Copy,
        Icon::Paste,
        Icon::SelectAll,
        Icon::Comment,
        Icon::Duplicate,
        Icon::Upper,
        Icon::Lower,
        Icon::Find,
        Icon::Code,
        Icon::Open,
        Icon::Folder,
        Icon::File,
        Icon::Path,
        Icon::Refresh,
        Icon::Close,
        Icon::CloseOthers,
        Icon::CloseRight,
        Icon::CloseSaved,
        Icon::CloseAll,
        Icon::Indent,
        Icon::Ending,
        Icon::Check,
        Icon::Clear,
    ];

    #[test]
    fn every_icon_has_its_own_drawing_in_table_order() {
        let mut seen: Vec<&[u8]> = Vec::new();
        for (index, icon) in ALL.into_iter().enumerate() {
            assert_eq!(icon as usize, index);
            let bytes = icon.bytes();
            assert!(std::str::from_utf8(bytes).is_ok_and(|text| text.ends_with("</svg>")));
            assert!(!seen.contains(&bytes), "{icon:?} repeats another icon");
            seen.push(bytes);
        }
    }
}

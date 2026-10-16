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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Plus,
    More,
    Close,
    Search,
    Up,
    Down,
    Bell,
}

impl Icon {
    #[must_use]
    pub fn handle(self) -> svg::Handle {
        let bytes: &'static [u8] = match self {
            Self::Plus => glyph!("<path d='M12 5v14M5 12h14'/>"),
            Self::More => glyph!(
                "<circle cx='5' cy='12' r='1' fill='currentColor'/><circle cx='12' cy='12' r='1' fill='currentColor'/><circle cx='19' cy='12' r='1' fill='currentColor'/>"
            ),
            Self::Close => glyph!("<path d='M18 6 6 18M6 6l12 12'/>"),
            Self::Search => glyph!("<circle cx='11' cy='11' r='7'/><path d='m20 20-3.5-3.5'/>"),
            Self::Up => glyph!("<path d='m6 15 6-6 6 6'/>"),
            Self::Down => glyph!("<path d='m6 9 6 6 6-6'/>"),
            Self::Bell => glyph!("<path d='M6 16V11a6 6 0 0 1 12 0v5l1.5 2h-15z'/><path d='M10 20.5a2 2 0 0 0 4 0'/>"),
        };
        svg::Handle::from_memory(bytes)
    }
}

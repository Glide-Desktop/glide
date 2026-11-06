pub mod comic;
pub mod convert;
pub mod epub;
pub mod pdf;
pub mod text;
pub mod write;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    #[must_use]
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    #[must_use]
    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    #[must_use]
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }

    #[must_use]
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x <= self.right() && y >= self.y && y <= self.bottom()
    }

    #[must_use]
    pub fn union(&self, other: &Self) -> Self {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        Self { x, y, w: self.right().max(other.right()) - x, h: self.bottom().max(other.bottom()) - y }
    }

    #[must_use]
    pub fn intersects(&self, other: &Self) -> bool {
        self.x < other.right() && other.x < self.right() && self.y < other.bottom() && other.y < self.bottom()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Size {
    pub w: f32,
    pub h: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Word {
    pub text: String,
    pub rect: Rect,
    pub line: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PageText {
    pub words: Vec<Word>,
    pub images: Vec<Rect>,
}

impl PageText {
    #[must_use]
    pub fn plain(&self) -> String {
        let mut out = String::new();
        let mut line = None;
        for word in &self.words {
            match line {
                Some(current) if current == word.line => out.push(' '),
                Some(_) => out.push('\n'),
                None => {}
            }
            out.push_str(&word.text);
            line = Some(word.line);
        }
        out
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Heading {
    pub title: String,
    pub level: u8,
    pub page: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Target {
    Page(usize),
    Uri(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Link {
    pub rect: Rect,
    pub target: Target,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Mark {
    Highlight,
    Underline,
    Strike,
    Comment,
    Ink,
    Text,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Hue {
    Yellow,
    Green,
    Blue,
    Red,
    Purple,
}

impl Hue {
    pub const ALL: [Self; 5] = [Self::Yellow, Self::Green, Self::Blue, Self::Red, Self::Purple];

    #[must_use]
    pub const fn rgb(self) -> [u8; 3] {
        match self {
            Self::Yellow => [0xe0, 0xa8, 0x3f],
            Self::Green => [0x6c, 0xbb, 0x62],
            Self::Blue => [0x5b, 0x9b, 0xec],
            Self::Red => [0xe3, 0x73, 0x6a],
            Self::Purple => [0xa0, 0x77, 0xd8],
        }
    }

    #[must_use]
    pub fn nearest(rgb: [f32; 3]) -> Self {
        let distance = |hue: Self| {
            let [r, g, b] = hue.rgb();
            let d = |a: f32, b: u8| (a - f32::from(b) / 255.0).powi(2);
            d(rgb[0], r) + d(rgb[1], g) + d(rgb[2], b)
        };
        Self::ALL.into_iter().min_by(|a, b| distance(*a).total_cmp(&distance(*b))).unwrap_or(Self::Yellow)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Note {
    pub id: u64,
    pub page: usize,
    pub mark: Mark,
    pub hue: Hue,
    pub rects: Vec<Rect>,
    pub quote: String,
    pub comment: String,
    pub author: String,
    pub modified: String,
    pub stroke: Vec<(f32, f32)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FieldKind {
    Text,
    Check(bool),
    Choice(Vec<String>),
    Signature { signed: bool, signer: String, when: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Field {
    pub name: String,
    pub label: String,
    pub kind: FieldKind,
    pub value: String,
    pub page: usize,
    pub rect: Rect,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FontInfo {
    pub name: String,
    pub embedded: bool,
    pub kind: String,
}

#[allow(clippy::struct_excessive_bools)] // Independent facts read from the file: encryption, two permissions, scripts
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Info {
    pub title: String,
    pub author: String,
    pub subject: String,
    pub creator: String,
    pub producer: String,
    pub created: String,
    pub modified: String,
    pub version: String,
    pub encrypted: bool,
    pub print: bool,
    pub copy: bool,
    pub javascript: bool,
    pub attachments: usize,
    pub fonts: Vec<FontInfo>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Format {
    Pdf,
    PostScript,
    Djvu,
    Xps,
    Tiff,
    Comic,
    Epub,
    Office,
}

impl Format {
    #[must_use]
    pub fn of(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        Some(match ext.as_str() {
            "pdf" => Self::Pdf,
            "ps" | "eps" => Self::PostScript,
            "djvu" | "djv" => Self::Djvu,
            "xps" | "oxps" => Self::Xps,
            "tif" | "tiff" => Self::Tiff,
            "cbz" | "cbr" | "cb7" | "cbt" => Self::Comic,
            "epub" | "fb2" => Self::Epub,
            "doc" | "docx" | "odt" | "rtf" | "ppt" | "pptx" | "odp" | "xls" | "xlsx" | "ods" => Self::Office,
            _ => return None,
        })
    }

    #[must_use]
    pub const fn badge(self) -> &'static str {
        match self {
            Self::Pdf => "PDF",
            Self::PostScript => "PS",
            Self::Djvu => "DjVu",
            Self::Xps => "XPS",
            Self::Tiff => "TIFF",
            Self::Comic => "CBZ",
            Self::Epub => "EPUB",
            Self::Office => "DOC",
        }
    }

    #[must_use]
    pub const fn annotates_in_place(self) -> bool {
        matches!(self, Self::Pdf)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OpenError {
    Locked { wrong: bool },
    Damaged,
    Missing { tool: String, package: String },
    Unsupported,
    Io(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Opened {
    pub format: Format,
    pub pages: Vec<Size>,
    pub labels: Vec<String>,
    pub info: Info,
    pub outline: Vec<Heading>,
    pub fields: Vec<Field>,
    pub notes: Vec<Note>,
    pub bookmarks: Vec<(String, usize)>,
    pub broken: Vec<usize>,
    pub reflow: Option<Vec<epub::Block>>,
    pub view_only: Option<PathBuf>,
    pub right_to_left: bool,
}

pub trait Engine: Send {
    fn opened(&self) -> &Opened;
    fn render(&mut self, page: usize, scale: f32) -> Option<image::RgbaImage>;
    fn text(&mut self, page: usize) -> PageText;
    fn links(&mut self, page: usize) -> Vec<Link>;
}

pub fn open(path: &Path, password: Option<&str>) -> Result<Box<dyn Engine>, OpenError> {
    let format = Format::of(path).ok_or(OpenError::Unsupported)?;
    match format {
        Format::Pdf => {
            pdf::PdfEngine::open(path, password, format, None).map(|engine| Box::new(engine) as Box<dyn Engine>)
        }
        Format::Comic | Format::Tiff => {
            comic::ImageEngine::open(path, format).map(|engine| Box::new(engine) as Box<dyn Engine>)
        }
        Format::Epub => epub::EpubEngine::open(path).map(|engine| Box::new(engine) as Box<dyn Engine>),
        Format::PostScript | Format::Djvu | Format::Xps | Format::Office => {
            let converted = convert::to_pdf(path, format)?;
            let view_only = (format == Format::Office).then(|| path.to_path_buf());
            pdf::PdfEngine::open(&converted, password, format, view_only)
                .map(|engine| Box::new(engine) as Box<dyn Engine>)
        }
    }
}

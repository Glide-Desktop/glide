use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::i18n::{self, Key, Lang};
use crate::icons::Icon;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FileCategory {
    Folder,
    Document,
    Text,
    Code,
    Image,
    Audio,
    Video,
    Archive,
    Pdf,
    Binary,
    Unknown,
}

impl FileCategory {
    pub const ALL: [Self; 11] = [
        Self::Folder,
        Self::Document,
        Self::Text,
        Self::Code,
        Self::Image,
        Self::Audio,
        Self::Video,
        Self::Archive,
        Self::Pdf,
        Self::Binary,
        Self::Unknown,
    ];

    #[must_use]
    pub fn icon(self) -> Icon {
        match self {
            Self::Folder => Icon::Folder,
            Self::Document => Icon::Documents,
            Self::Text => Icon::FileText,
            Self::Code => Icon::FileCode,
            Self::Image => Icon::FileImage,
            Self::Audio => Icon::FileAudio,
            Self::Video => Icon::FileVideo,
            Self::Archive => Icon::FileArchive,
            Self::Pdf => Icon::FilePdf,
            Self::Binary => Icon::FileBinary,
            Self::Unknown => Icon::File,
        }
    }

    #[must_use]
    pub fn name(self, lang: Lang) -> &'static str {
        let key = match self {
            Self::Folder => Key::CatFolder,
            Self::Document => Key::CatDocument,
            Self::Text => Key::CatText,
            Self::Code => Key::CatCode,
            Self::Image => Key::CatImage,
            Self::Audio => Key::CatAudio,
            Self::Video => Key::CatVideo,
            Self::Archive => Key::CatArchive,
            Self::Pdf => Key::CatPdf,
            Self::Binary => Key::CatBinary,
            Self::Unknown => Key::CatUnknown,
        };
        key.text(lang)
    }

    #[must_use]
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "pdf" => Self::Pdf,
            "" | "txt" | "md" | "markdown" | "rst" | "log" | "csv" | "tsv" | "json" | "xml" | "yaml" | "yml"
            | "toml" | "ini" | "cfg" | "conf" => Self::Text,
            "rs" | "c" | "cpp" | "h" | "hpp" | "cc" | "py" | "js" | "ts" | "jsx" | "tsx" | "html" | "css" | "scss"
            | "go" | "java" | "kt" | "swift" | "rb" | "php" | "sh" | "bash" | "zsh" | "fish" | "lua" | "sql" => {
                Self::Code
            }
            "png" | "jpg" | "jpeg" | "webp" | "svg" | "gif" | "bmp" | "ico" | "tiff" | "avif" | "heic" => Self::Image,
            "mp3" | "flac" | "wav" | "ogg" | "m4a" | "aac" | "opus" | "wma" => Self::Audio,
            "mp4" | "mkv" | "webm" | "avi" | "mov" | "wmv" | "flv" | "m4v" => Self::Video,
            "zip" | "tar" | "gz" | "bz2" | "xz" | "7z" | "rar" | "zst" | "tgz" => Self::Archive,
            "doc" | "docx" | "odt" | "rtf" | "xls" | "xlsx" | "ods" | "ppt" | "pptx" | "odp" => Self::Document,
            "so" | "a" | "o" | "bin" | "exe" | "dll" | "dylib" | "iso" | "img" | "appimage" => Self::Binary,
            _ => Self::Unknown,
        }
    }

    #[must_use]
    pub fn classify(path: &Path, is_dir: bool, mode: u32) -> Self {
        if is_dir {
            return Self::Folder;
        }
        match path.extension() {
            None if mode & 0o111 != 0 => Self::Binary,
            ext => Self::from_extension(&ext.unwrap_or_default().to_string_lossy()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub size: u64,
    pub modified: Option<SystemTime>,
    pub mode: u32,
    pub category: FileCategory,
    pub created: Option<SystemTime>,
    pub tags: Vec<String>,
}

impl FileEntry {
    #[must_use]
    pub fn from_path(path: PathBuf) -> Option<Self> {
        let link = std::fs::symlink_metadata(&path).ok()?;
        let is_symlink = link.file_type().is_symlink();
        let meta = if is_symlink { std::fs::metadata(&path).unwrap_or(link) } else { link };
        let is_dir = meta.is_dir();
        let mode = meta.permissions().mode();
        let name = path.file_name().map_or_else(|| path.to_string_lossy(), |name| name.to_string_lossy()).into_owned();
        Some(Self {
            category: FileCategory::classify(&path, is_dir, mode),
            created: meta.created().ok(),
            tags: super::tags::read(&path),
            name,
            is_dir,
            is_symlink,
            size: if is_dir { 0 } else { meta.len() },
            modified: meta.modified().ok(),
            mode,
            path,
        })
    }

    #[must_use]
    pub fn is_hidden(&self) -> bool {
        self.name.starts_with('.')
    }
}

#[must_use]
pub fn permissions_string(mode: u32) -> String {
    let flags = ['r', 'w', 'x'];
    let symbolic: String = (0..9).map(|bit| if mode & (0o400 >> bit) == 0 { '-' } else { flags[bit % 3] }).collect();
    format!("{symbolic} ({:03o})", mode & 0o777)
}

#[must_use]
pub fn format_bytes(bytes: u64, lang: Lang) -> String {
    const UNITS: [Key; 4] = [Key::UnitKb, Key::UnitMb, Key::UnitGb, Key::UnitTb];
    if bytes < 1000 {
        return format!("{bytes} {}", Key::UnitB.text(lang));
    }
    let mut value = bytes as f64 / 1000.0;
    let mut unit = 0;
    while value >= 999.95 && unit + 1 < UNITS.len() {
        value /= 1000.0;
        unit += 1;
    }
    let number = format!("{value:.1}");
    let number = match lang {
        Lang::Uk => number.replace('.', ","),
        Lang::En => number,
    };
    format!("{number} {}", UNITS[unit].text(lang))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalTime {
    pub year: i32,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

impl LocalTime {
    #[must_use]
    pub fn of(time: SystemTime) -> Option<Self> {
        let secs = match time.duration_since(SystemTime::UNIX_EPOCH) {
            Ok(after) => after.as_secs() as i64,
            Err(before) => -(before.duration().as_secs() as i64),
        };
        let stamp = secs as libc::time_t;
        let mut tm = std::mem::MaybeUninit::<libc::tm>::uninit();
        // SAFETY: `stamp` outlives the call and `tm` is writable storage for one `libc::tm`.
        let filled = unsafe { libc::localtime_r(&raw const stamp, tm.as_mut_ptr()) };
        if filled.is_null() {
            return None;
        }
        // SAFETY: a non-null return means `localtime_r` initialised `tm`.
        let tm = unsafe { tm.assume_init() };
        Some(Self {
            year: tm.tm_year + 1900,
            month: (tm.tm_mon + 1) as u8,
            day: tm.tm_mday as u8,
            hour: tm.tm_hour as u8,
            minute: tm.tm_min as u8,
            second: tm.tm_sec as u8,
        })
    }

    #[must_use]
    pub fn day_number(self) -> i64 {
        days_from_civil(self.year, self.month, self.day)
    }

    #[must_use]
    pub fn iso(self) -> String {
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }
}

#[must_use]
pub fn days_from_civil(year: i32, month: u8, day: u8) -> i64 {
    let year = i64::from(year) - i64::from(month <= 2);
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month = i64::from(month);
    let shifted = if month > 2 { month - 3 } else { month + 9 };
    let day_of_year = (153 * shifted + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[must_use]
pub fn format_time(time: SystemTime, now: SystemTime, lang: Lang) -> String {
    let (Some(at), Some(today)) = (LocalTime::of(time), LocalTime::of(now)) else {
        return String::from("—");
    };
    let clock = format!("{:02}:{:02}", at.hour, at.minute);
    match today.day_number() - at.day_number() {
        0 => format!("{}, {clock}", Key::Today.text(lang)),
        1 => format!("{}, {clock}", Key::Yesterday.text(lang)),
        _ => dated(at.day, at.month, at.year, &clock, lang),
    }
}

fn dated(day: impl std::fmt::Display, month: u8, year: impl std::fmt::Display, clock: &str, lang: Lang) -> String {
    format!("{day} {} {year}, {clock}", i18n::month(month, lang))
}

#[must_use]
pub fn widest_times(lang: Lang) -> Vec<String> {
    let mut samples: Vec<String> = (1..=12).map(|month| dated(28, month, 2026, "23:59", lang)).collect();
    samples.extend([Key::Today, Key::Yesterday].map(|key| format!("{}, 23:59", key.text(lang))));
    samples
}

#[must_use]
pub fn widest_sizes(lang: Lang) -> Vec<String> {
    [999, 999_949, 999_949_000, 999_949_000_000, 999_949_000_000_000].map(|bytes| format_bytes(bytes, lang)).to_vec()
}

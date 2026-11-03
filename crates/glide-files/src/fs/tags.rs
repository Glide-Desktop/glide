use std::ffi::CString;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

pub(crate) const ATTR: &str = "user.xdg.tags";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Color {
    Red,
    Orange,
    Yellow,
    Green,
    Blue,
    Purple,
    Gray,
}

impl Color {
    pub const ALL: [Self; 7] =
        [Self::Red, Self::Orange, Self::Yellow, Self::Green, Self::Blue, Self::Purple, Self::Gray];

    #[must_use]
    pub const fn tag(self) -> &'static str {
        match self {
            Self::Red => "Red",
            Self::Orange => "Orange",
            Self::Yellow => "Yellow",
            Self::Green => "Green",
            Self::Blue => "Blue",
            Self::Purple => "Purple",
            Self::Gray => "Gray",
        }
    }

    #[must_use]
    pub fn of(tag: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|color| color.tag().eq_ignore_ascii_case(tag.trim()))
    }

    #[must_use]
    pub const fn rgb(self) -> [u8; 3] {
        match self {
            Self::Red => [0xe3, 0x73, 0x6a],
            Self::Orange => [0xec, 0x8b, 0x4f],
            Self::Yellow => [0xe0, 0xa8, 0x3f],
            Self::Green => [0x6c, 0xbb, 0x62],
            Self::Blue => [0x5b, 0x9b, 0xec],
            Self::Purple => [0xa0, 0x77, 0xd8],
            Self::Gray => [0x91, 0x8a, 0x7d],
        }
    }
}

#[must_use]
pub fn parse(text: &str) -> Vec<String> {
    let mut tags: Vec<String> = Vec::new();
    for tag in text.split(',').map(str::trim).filter(|tag| !tag.is_empty()) {
        if !tags.iter().any(|seen| seen.eq_ignore_ascii_case(tag)) {
            tags.push(tag.to_string());
        }
    }
    tags
}

#[must_use]
pub fn render(tags: &[String]) -> String {
    tags.join(",")
}

#[must_use]
pub fn colors(tags: &[String]) -> Vec<Color> {
    Color::ALL.into_iter().filter(|color| tags.iter().any(|tag| Color::of(tag) == Some(*color))).collect()
}

#[must_use]
pub fn words(tags: &[String]) -> Vec<&str> {
    tags.iter().map(String::as_str).filter(|tag| Color::of(tag).is_none()).collect()
}

pub fn set(tags: &mut Vec<String>, tag: &str, on: bool) {
    tags.retain(|existing| !existing.eq_ignore_ascii_case(tag));
    if on {
        tags.push(tag.to_string());
    }
}

fn c_path(path: &Path) -> io::Result<CString> {
    CString::new(path.as_os_str().as_bytes()).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))
}

fn attr_name() -> CString {
    CString::new(ATTR).unwrap_or_default()
}

#[must_use]
pub fn read(path: &Path) -> Vec<String> {
    let Ok(file) = c_path(path) else {
        return Vec::new();
    };
    let name = attr_name();
    let mut buffer = vec![0_u8; 256];
    loop {
        // SAFETY: both strings are NUL-terminated and `buffer` is writable for `buffer.len()` bytes.
        let read = unsafe { libc::getxattr(file.as_ptr(), name.as_ptr(), buffer.as_mut_ptr().cast(), buffer.len()) };
        if read >= 0 {
            buffer.truncate(read.unsigned_abs());
            return parse(&String::from_utf8_lossy(&buffer));
        }
        if io::Error::last_os_error().raw_os_error() != Some(libc::ERANGE) || buffer.len() >= 1 << 16 {
            return Vec::new();
        }
        buffer.resize(buffer.len() * 4, 0);
    }
}

pub fn write(path: &Path, tags: &[String]) -> io::Result<()> {
    let file = c_path(path)?;
    let name = attr_name();
    if tags.is_empty() {
        // SAFETY: both arguments are NUL-terminated C strings that outlive the call.
        let removed = unsafe { libc::removexattr(file.as_ptr(), name.as_ptr()) };
        let err = io::Error::last_os_error();
        return if removed == 0 || err.raw_os_error() == Some(libc::ENODATA) { Ok(()) } else { Err(err) };
    }
    let value = render(tags);
    // SAFETY: the strings are NUL-terminated and `value` is readable for its full length.
    let written = unsafe { libc::setxattr(file.as_ptr(), name.as_ptr(), value.as_ptr().cast(), value.len(), 0) };
    if written == 0 { Ok(()) } else { Err(io::Error::last_os_error()) }
}

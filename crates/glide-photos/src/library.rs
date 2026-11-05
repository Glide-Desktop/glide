use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::formats::{self, Family, Format, Traits};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Date {
    pub year: i32,
    pub month: u32,
    pub day: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Photo {
    pub path: PathBuf,
    pub name: String,
    pub folder: PathBuf,
    pub format: Format,
    pub traits: Traits,
    pub bytes: u64,
    pub modified: u64,
    pub date: Date,
    pub minute: u32,
    pub screenshot: bool,
    pub dims: Option<(u32, u32)>,
}

impl Photo {
    #[must_use]
    pub fn key(&self) -> (Date, u32) {
        (self.date, self.minute)
    }

    #[must_use]
    pub fn animated(&self) -> bool {
        self.traits.frames > 1
    }

    #[must_use]
    pub fn paged(&self) -> bool {
        self.traits.pages > 1
    }

    #[must_use]
    pub fn panorama(&self) -> bool {
        self.dims.is_some_and(|(w, h)| h > 0 && (w as f32 / h as f32 >= 2.0 || h as f32 / w.max(1) as f32 >= 2.0))
    }
}

#[must_use]
pub fn utc_offset() -> i64 {
    let Some(minute) = glide_config::local_minute() else {
        return 0;
    };
    let now = std::time::SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let utc = (now % 86_400) as i64 / 60 * 60;
    let mut offset = i64::from(minute) * 60 - utc;
    if offset > 14 * 3600 {
        offset -= 86_400;
    } else if offset < -12 * 3600 {
        offset += 86_400;
    }
    (offset + 450).div_euclid(900) * 900
}

#[must_use]
pub const fn civil(seconds: i64) -> (Date, u32) {
    let days = seconds.div_euclid(86_400) + 719_468;
    let minute = (seconds.rem_euclid(86_400) / 60) as u32;
    let era = days.div_euclid(146_097);
    let doe = days - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = (yoe + era * 400 + if month <= 2 { 1 } else { 0 }) as i32;
    (Date { year, month, day }, minute)
}

#[must_use]
pub fn today() -> Date {
    glide_config::local_time().map_or_else(
        || {
            let now = std::time::SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
            civil(now as i64).0
        },
        |time| Date { year: time.year, month: u32::from(time.month), day: u32::from(time.day) },
    )
}

#[must_use]
pub fn days_between(a: Date, b: Date) -> i64 {
    day_number(b) - day_number(a)
}

const fn day_number(date: Date) -> i64 {
    let year = if date.month <= 2 { date.year - 1 } else { date.year } as i64;
    let era = year.div_euclid(400);
    let yoe = year - era * 400;
    let month = date.month as i64;
    let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + date.day as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn screenshot_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    ["screenshot", "знімок екрана", "glide-screenshot", "scrot", "снимок экрана"]
        .iter()
        .any(|mark| lower.starts_with(mark))
}

#[must_use]
pub fn read_photo(path: &Path, offset: i64) -> Option<Photo> {
    let format = formats::of(path)?;
    let meta = std::fs::metadata(path).ok().filter(std::fs::Metadata::is_file)?;
    let modified = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_secs();
    let name = path.file_name()?.to_string_lossy().into_owned();
    let exif = matches!(format.family, Family::Raster | Family::Raw).then(|| formats::exif(path)).flatten();
    let taken = exif.as_ref().and_then(|exif| formats::camera(exif).taken);
    let (date, minute) = taken.map_or_else(
        || civil(modified as i64 + offset),
        |(year, month, day, hour, minute)| (Date { year, month, day }, hour * 60 + minute),
    );
    let dims = match format.family {
        Family::Raster | Family::Hdr => image::image_dimensions(path).ok(),
        _ => None,
    };
    Some(Photo {
        folder: path.parent().map(Path::to_path_buf).unwrap_or_default(),
        traits: formats::traits(path, format),
        screenshot: screenshot_name(&name),
        path: path.to_path_buf(),
        name,
        format,
        bytes: meta.len(),
        modified,
        date,
        minute,
        dims,
    })
}

const MAX_DEPTH: usize = 8;

#[must_use]
pub fn scan(roots: &[PathBuf]) -> Vec<Photo> {
    let offset = utc_offset();
    let mut seen = HashSet::new();
    let mut photos = Vec::new();
    for root in roots {
        walk(root, 0, offset, &mut seen, &mut photos);
    }
    sort(&mut photos);
    photos
}

pub fn sort(photos: &mut [Photo]) {
    photos.sort_by(|a, b| b.key().cmp(&a.key()).then_with(|| a.name.cmp(&b.name)));
}

fn walk(dir: &Path, depth: usize, offset: i64, seen: &mut HashSet<PathBuf>, out: &mut Vec<Photo>) {
    let Ok(canonical) = dir.canonicalize() else {
        return;
    };
    if depth > MAX_DEPTH || !seen.insert(canonical) {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() || (kind.is_symlink() && path.is_dir()) {
            walk(&path, depth + 1, offset, seen, out);
        } else if let Some(photo) = read_photo(&path, offset) {
            out.push(photo);
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub date: Date,
    pub items: Vec<usize>,
}

#[must_use]
pub fn by_day(photos: &[Photo], shown: &[usize]) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    for &index in shown {
        let date = photos[index].date;
        match groups.last_mut() {
            Some(group) if group.date == date => group.items.push(index),
            _ => groups.push(Group { date, items: vec![index] }),
        }
    }
    groups
}

#[derive(Debug, Clone, PartialEq)]
pub struct Period {
    pub year: i32,
    pub month: Option<u32>,
    pub count: usize,
    pub cover: usize,
    pub folders: Vec<String>,
}

#[must_use]
pub fn periods(photos: &[Photo], shown: &[usize], monthly: bool) -> Vec<Period> {
    let mut map: BTreeMap<(i32, u32), Period> = BTreeMap::new();
    for &index in shown {
        let photo = &photos[index];
        let key = (photo.date.year, if monthly { photo.date.month } else { 0 });
        let entry = map.entry(key).or_insert_with(|| Period {
            year: photo.date.year,
            month: monthly.then_some(photo.date.month),
            count: 0,
            cover: index,
            folders: Vec::new(),
        });
        entry.count += 1;
        if let Some(name) = photo.folder.file_name().map(|name| name.to_string_lossy().into_owned())
            && entry.folders.len() < 2
            && !entry.folders.contains(&name)
        {
            entry.folders.push(name);
        }
    }
    map.into_values().rev().collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Smart {
    Recent,
    Raw,
    Animated,
    Vector,
    Screenshots,
    Hdr,
    Panoramas,
    Paged,
}

impl Smart {
    pub const ALL: [Self; 8] = [
        Self::Recent,
        Self::Raw,
        Self::Animated,
        Self::Vector,
        Self::Screenshots,
        Self::Hdr,
        Self::Panoramas,
        Self::Paged,
    ];

    #[must_use]
    pub fn holds(self, photo: &Photo, now: u64) -> bool {
        match self {
            Self::Recent => now.saturating_sub(photo.modified) <= 30 * 86_400,
            Self::Raw => photo.format.family == Family::Raw,
            Self::Animated => photo.animated(),
            Self::Vector => photo.format.family == Family::Vector,
            Self::Screenshots => photo.screenshot,
            Self::Hdr => photo.format.family == Family::Hdr,
            Self::Panoramas => photo.panorama(),
            Self::Paged => photo.paged(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Filter {
    Everything,
    Favourites,
    Kind(Smart),
    ThisYear,
}

#[must_use]
pub fn search(
    photos: &[Photo], within: &[usize], query: &str, captions: &dyn Fn(&Path) -> Option<String>,
) -> Vec<usize> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    if words.is_empty() {
        return within.to_vec();
    }
    within
        .iter()
        .copied()
        .filter(|&index| {
            let photo = &photos[index];
            let mut hay = photo.name.to_lowercase();
            hay.push(' ');
            hay.push_str(&photo.folder.to_string_lossy().to_lowercase());
            hay.push(' ');
            hay.push_str(&photo.format.name.to_lowercase());
            if let Some(caption) = captions(&photo.path) {
                hay.push(' ');
                hay.push_str(&caption.to_lowercase());
            }
            words.iter().all(|word| hay.contains(word.as_str()))
        })
        .collect()
}

#[must_use]
pub fn user_dir(key: &str) -> Option<PathBuf> {
    let home = glide_config::home()?;
    let text = std::fs::read_to_string(glide_config::config_home()?.join("user-dirs.dirs")).ok();
    let found = text.as_deref().and_then(|text| parse_user_dir(text, &home, key));
    found.or_else(|| {
        let fallback = match key {
            "XDG_PICTURES_DIR" => "Pictures",
            _ => return None,
        };
        Some(home.join(fallback))
    })
}

#[must_use]
pub fn parse_user_dir(text: &str, home: &Path, key: &str) -> Option<PathBuf> {
    text.lines().map(str::trim).filter(|line| !line.starts_with('#')).find_map(|line| {
        let (name, value) = line.split_once('=')?;
        if name.trim() != key {
            return None;
        }
        let value = value.trim().trim_matches('"');
        if let Some(rest) = value.strip_prefix("$HOME") {
            Some(home.join(rest.trim_start_matches('/')))
        } else {
            value.starts_with('/').then(|| PathBuf::from(value))
        }
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Card {
    pub mount: PathBuf,
    pub label: String,
    pub dcim: PathBuf,
}

#[must_use]
pub fn camera_cards() -> Vec<Card> {
    let Ok(mounts) = std::fs::read_to_string("/proc/self/mounts") else {
        return Vec::new();
    };
    mounts
        .lines()
        .filter_map(|line| {
            let point = line.split_whitespace().nth(1)?.replace("\\040", " ");
            let mount = PathBuf::from(point);
            let removable = mount.starts_with("/run/media") || mount.starts_with("/media") || mount.starts_with("/mnt");
            let dcim = mount.join("DCIM");
            (removable && dcim.is_dir()).then(|| Card {
                label: mount.file_name().map_or_else(String::new, |name| name.to_string_lossy().into_owned()),
                mount,
                dcim,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn photo(name: &str, date: Date, minute: u32) -> Photo {
        Photo {
            path: PathBuf::from(format!("/p/{name}")),
            name: name.into(),
            folder: PathBuf::from("/p"),
            format: formats::of(Path::new(name)).unwrap(),
            traits: Traits { frames: 1, pages: 1 },
            bytes: 1,
            modified: 0,
            date,
            minute,
            screenshot: screenshot_name(name),
            dims: Some((4000, 3000)),
        }
    }

    #[test]
    fn civil_dates_round_trip_through_day_numbers() {
        let (date, minute) = civil(1_793_865_555);
        assert_eq!(date, Date { year: 2026, month: 11, day: 5 });
        assert!(minute < 1440);
        assert_eq!(days_between(Date { year: 2026, month: 2, day: 27 }, Date { year: 2026, month: 3, day: 1 }), 2);
        assert_eq!(days_between(Date { year: 2024, month: 12, day: 31 }, Date { year: 2025, month: 1, day: 1 }), 1);
    }

    #[test]
    fn photos_group_newest_day_first_and_periods_count_their_photos() {
        let d1 = Date { year: 2026, month: 11, day: 5 };
        let d2 = Date { year: 2026, month: 11, day: 2 };
        let d3 = Date { year: 2025, month: 7, day: 9 };
        let mut photos =
            vec![photo("b.jpg", d2, 10), photo("a.jpg", d1, 5), photo("c.png", d3, 0), photo("d.jpg", d1, 50)];
        sort(&mut photos);
        let all: Vec<usize> = (0..photos.len()).collect();
        let days = by_day(&photos, &all);
        assert_eq!(days.iter().map(|g| (g.date, g.items.len())).collect::<Vec<_>>(), [(d1, 2), (d2, 1), (d3, 1)]);
        assert_eq!(photos[days[0].items[0]].name, "d.jpg");
        let months = periods(&photos, &all, true);
        assert_eq!(
            months.iter().map(|p| (p.year, p.month, p.count)).collect::<Vec<_>>(),
            [(2026, Some(11), 3), (2025, Some(7), 1)]
        );
        let years = periods(&photos, &all, false);
        assert_eq!(years.iter().map(|p| (p.year, p.count)).collect::<Vec<_>>(), [(2026, 3), (2025, 1)]);
    }

    #[test]
    fn search_needs_every_word_in_the_name_folder_format_or_caption() {
        let date = Date { year: 2026, month: 1, day: 1 };
        let photos = vec![photo("lake.jpg", date, 0), photo("Screenshot 1.png", date, 0)];
        let all = [0, 1];
        let caption = |path: &Path| path.ends_with("lake.jpg").then(|| "Озеро Синевир".to_string());
        assert_eq!(search(&photos, &all, "озеро", &caption), [0]);
        assert_eq!(search(&photos, &all, "png screenshot", &caption), [1]);
        assert_eq!(search(&photos, &all, "  ", &caption), [0, 1]);
        assert!(Smart::Screenshots.holds(&photos[1], 0));
        assert!(!Smart::Screenshots.holds(&photos[0], 0));
    }

    #[test]
    fn user_dirs_expand_home_and_ignore_comments() {
        let text = "# c\nXDG_PICTURES_DIR=\"$HOME/Зображення\"\nXDG_MUSIC_DIR=\"/srv/m\"\n";
        let home = Path::new("/home/me");
        assert_eq!(parse_user_dir(text, home, "XDG_PICTURES_DIR"), Some(PathBuf::from("/home/me/Зображення")));
        assert_eq!(parse_user_dir(text, home, "XDG_MUSIC_DIR"), Some(PathBuf::from("/srv/m")));
        assert_eq!(parse_user_dir(text, home, "XDG_VIDEOS_DIR"), None);
    }
}

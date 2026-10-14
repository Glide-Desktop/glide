use std::cmp::Ordering;
use std::collections::VecDeque;
use std::iter::Peekable;
use std::path::Path;
use std::str::Chars;

use super::entry::FileEntry;
use crate::i18n::{Key, Lang};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortBy {
    #[default]
    Name,
    Size,
    Modified,
    Type,
}

impl SortBy {
    pub const ALL: [Self; 4] = [Self::Name, Self::Size, Self::Modified, Self::Type];

    #[must_use]
    pub fn label(self, lang: Lang) -> &'static str {
        match self {
            Self::Name => Key::ColName,
            Self::Size => Key::ColSize,
            Self::Modified => Key::ColModified,
            Self::Type => Key::ColType,
        }
        .text(lang)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SortConfig {
    pub by: SortBy,
    pub ascending: bool,
}

impl Default for SortConfig {
    fn default() -> Self {
        Self { by: SortBy::Name, ascending: true }
    }
}

const PSEUDO_FS: [&str; 4] = ["/proc", "/sys", "/dev", "/run"];

pub fn read_directory(dir: &Path, show_hidden: bool, sort: SortConfig) -> std::io::Result<Vec<FileEntry>> {
    let mut entries: Vec<FileEntry> = std::fs::read_dir(dir)?
        .flatten()
        .filter_map(|entry| FileEntry::from_path(entry.path()))
        .filter(|entry| show_hidden || !entry.is_hidden())
        .collect();
    sort_entries(&mut entries, sort);
    Ok(entries)
}

#[must_use]
pub fn search_recursive(
    root: &Path, query: &str, show_hidden: bool, max_results: usize, cancelled: &dyn Fn() -> bool,
) -> Vec<FileEntry> {
    let needle = query.trim().to_lowercase();
    let mut results = Vec::new();
    if needle.is_empty() {
        return results;
    }
    let mut queue = VecDeque::from([root.to_path_buf()]);
    while let Some(dir) = queue.pop_front() {
        if cancelled() {
            break;
        }
        let Ok(read_dir) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in read_dir.flatten() {
            let Some(found) = FileEntry::from_path(entry.path()) else {
                continue;
            };
            if !show_hidden && found.is_hidden() {
                continue;
            }
            if found.is_dir && !found.is_symlink && !PSEUDO_FS.iter().any(|skip| found.path == Path::new(skip)) {
                queue.push_back(found.path.clone());
            }
            if found.name.to_lowercase().contains(&needle) || found.tags.iter().any(|tag| tag.to_lowercase() == needle)
            {
                results.push(found);
                if results.len() >= max_results {
                    return results;
                }
            }
        }
    }
    results
}

pub fn sort_entries(entries: &mut [FileEntry], config: SortConfig) {
    entries.sort_by(|a, b| {
        if a.is_dir != b.is_dir {
            return b.is_dir.cmp(&a.is_dir);
        }
        let by_name = || natural_cmp(&a.name, &b.name).then_with(|| a.name.cmp(&b.name));
        let primary = match config.by {
            SortBy::Name => by_name(),
            SortBy::Size => a.size.cmp(&b.size),
            SortBy::Modified => a.modified.cmp(&b.modified),
            SortBy::Type => a.category.cmp(&b.category),
        };
        let primary = if config.ascending { primary } else { primary.reverse() };
        primary.then_with(by_name)
    });
}

#[must_use]
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut a, mut b) = (a.chars().peekable(), b.chars().peekable());
    loop {
        let ordering = match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let (left, right) = (take_number(&mut a), take_number(&mut b));
                let (left, right) = (left.trim_start_matches('0'), right.trim_start_matches('0'));
                left.len().cmp(&right.len()).then_with(|| left.cmp(right))
            }
            (Some(x), Some(y)) => {
                a.next();
                b.next();
                x.to_lowercase().cmp(y.to_lowercase())
            }
        };
        if ordering != Ordering::Equal {
            return ordering;
        }
    }
}

fn take_number(chars: &mut Peekable<Chars<'_>>) -> String {
    let mut digits = String::new();
    while let Some(digit) = chars.next_if(char::is_ascii_digit) {
        digits.push(digit);
    }
    digits
}

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trashed {
    pub file: PathBuf,
    pub info: PathBuf,
    pub original: PathBuf,
}

#[must_use]
pub fn home() -> Option<PathBuf> {
    Some(glide_config::data_home()?.join("Trash"))
}

fn encode(path: &Path) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    for byte in path.as_os_str().as_encoded_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => out.push(char::from(*byte)),
            _ => {
                let _ = write!(out, "%{byte:02X}");
            }
        }
    }
    out
}

fn decode(text: &str) -> PathBuf {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let Ok(value) = u8::from_str_radix(&text[index + 1..index + 3], 16)
        {
            out.push(value);
            index += 3;
            continue;
        }
        out.push(bytes[index]);
        index += 1;
    }
    PathBuf::from(String::from_utf8_lossy(&out).into_owned())
}

fn stamp() -> String {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let (date, minute) = crate::library::civil(now as i64 + crate::library::utc_offset());
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:00", date.year, date.month, date.day, minute / 60, minute % 60)
}

pub fn put(path: &Path) -> std::io::Result<Trashed> {
    put_in(&home().ok_or_else(|| std::io::Error::other("no data home"))?, path)
}

pub fn put_in(trash: &Path, path: &Path) -> std::io::Result<Trashed> {
    let files = trash.join("files");
    let infos = trash.join("info");
    std::fs::create_dir_all(&files)?;
    std::fs::create_dir_all(&infos)?;
    let absolute = std::path::absolute(path)?;
    let name = path.file_name().ok_or_else(|| std::io::Error::other("no name"))?.to_string_lossy().into_owned();
    let (stem, ext) = match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem.to_string(), format!(".{ext}")),
        _ => (name.clone(), String::new()),
    };
    let mut candidate = name;
    let mut n = 2;
    loop {
        let info = infos.join(format!("{candidate}.trashinfo"));
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&info) {
            Ok(mut file) => {
                use std::io::Write;
                write!(file, "[Trash Info]\nPath={}\nDeletionDate={}\n", encode(&absolute), stamp())?;
                let target = files.join(&candidate);
                if let Err(error) = std::fs::rename(&absolute, &target) {
                    let _ = std::fs::remove_file(&info);
                    return Err(error);
                }
                return Ok(Trashed { file: target, info, original: absolute });
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                candidate = format!("{stem} {n}{ext}");
                n += 1;
            }
            Err(error) => return Err(error),
        }
    }
}

#[must_use]
pub fn list() -> Vec<Trashed> {
    home().map(|trash| list_in(&trash)).unwrap_or_default()
}

#[must_use]
pub fn list_in(trash: &Path) -> Vec<Trashed> {
    let Ok(entries) = std::fs::read_dir(trash.join("info")) else {
        return Vec::new();
    };
    let mut found: Vec<Trashed> = entries
        .flatten()
        .filter_map(|entry| {
            let info = entry.path();
            let name = info.file_name()?.to_str()?.strip_suffix(".trashinfo")?.to_string();
            let text = std::fs::read_to_string(&info).ok()?;
            let original = text.lines().find_map(|line| line.strip_prefix("Path=")).map(decode)?;
            let file = trash.join("files").join(name);
            (crate::formats::of(&file).is_some() && file.is_file()).then_some(Trashed { file, info, original })
        })
        .collect();
    found.sort_by(|a, b| a.original.cmp(&b.original));
    found
}

pub fn restore(item: &Trashed) -> std::io::Result<PathBuf> {
    if item.original.exists() {
        return Err(std::io::Error::new(std::io::ErrorKind::AlreadyExists, "a file is already there"));
    }
    if let Some(dir) = item.original.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::rename(&item.file, &item.original)?;
    let _ = std::fs::remove_file(&item.info);
    Ok(item.original.clone())
}

pub fn purge(item: &Trashed) -> std::io::Result<()> {
    std::fs::remove_file(&item.file)?;
    let _ = std::fs::remove_file(&item.info);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn photos_go_to_the_trash_with_their_origin_and_come_back() {
        let root = std::env::temp_dir().join(format!("glide-photos-trash-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let pictures = root.join("Зображення");
        std::fs::create_dir_all(&pictures).unwrap();
        let trash = root.join("Trash");
        let first = pictures.join("a b.jpg");
        std::fs::write(&first, b"1").unwrap();
        let one = put_in(&trash, &first).unwrap();
        std::fs::write(&first, b"2").unwrap();
        let two = put_in(&trash, &first).unwrap();
        assert_ne!(one.file, two.file);
        assert!(two.file.ends_with("a b 2.jpg"));
        let listed = list_in(&trash);
        assert_eq!(listed.len(), 2);
        assert!(listed.iter().all(|item| item.original == first));
        assert!(restore(&listed[0]).is_ok());
        assert!(first.is_file());
        assert!(restore(&listed[1]).is_err());
        purge(&listed[1]).unwrap();
        assert!(list_in(&trash).is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn paths_survive_percent_encoding() {
        let path = Path::new("/home/me/Зображення/a b%.jpg");
        assert_eq!(decode(&encode(path)), path);
    }
}

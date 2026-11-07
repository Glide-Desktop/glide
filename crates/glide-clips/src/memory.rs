use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

pub const RESUME_TAIL: Duration = Duration::from_mins(2);
pub const RESUME_HEAD: Duration = Duration::from_secs(5);
pub const RECENT: usize = 4;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "kebab-case")]
pub struct Entry {
    pub position: f64,
    pub duration: f64,
    pub opened: u64,
    pub volume: Option<f64>,
    pub rate: Option<f64>,
    pub audio: Option<String>,
    pub text: Option<String>,
    pub subtitles: Option<bool>,
    pub offset: i64,
}

impl Entry {
    #[must_use]
    pub fn resume(&self) -> Option<Duration> {
        let (at, length) =
            (Duration::from_secs_f64(self.position.max(0.0)), Duration::from_secs_f64(self.duration.max(0.0)));
        (at > RESUME_HEAD && length > at + RESUME_TAIL).then_some(at)
    }

    #[must_use]
    pub fn progress(&self) -> f32 {
        if self.duration <= 0.0 { 0.0 } else { (self.position / self.duration).clamp(0.0, 1.0) as f32 }
    }

    #[must_use]
    pub fn left(&self) -> Duration {
        Duration::from_secs_f64((self.duration - self.position).max(0.0))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Memory {
    pub files: BTreeMap<String, Entry>,
}

#[must_use]
pub fn path() -> Option<PathBuf> {
    Some(glide_config::state_home()?.join("glide/clips/files.toml"))
}

#[must_use]
pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_secs())
}

impl Memory {
    #[must_use]
    pub fn load() -> Self {
        path().map_or_else(Self::default, |path| Self::read(&path))
    }

    #[must_use]
    pub fn read(path: &Path) -> Self {
        std::fs::read_to_string(path).ok().and_then(|text| toml::from_str(&text).ok()).unwrap_or_default()
    }

    #[must_use]
    pub fn get(&self, file: &Path) -> Option<&Entry> {
        self.files.get(&file.to_string_lossy().into_owned())
    }

    pub fn entry(&mut self, file: &Path) -> &mut Entry {
        self.files.entry(file.to_string_lossy().into_owned()).or_default()
    }

    #[must_use]
    pub fn recent(&self, count: usize, exists: impl Fn(&Path) -> bool) -> Vec<(PathBuf, Entry)> {
        let mut all: Vec<(PathBuf, Entry)> = self
            .files
            .iter()
            .filter(|(_, entry)| entry.resume().is_some())
            .map(|(path, entry)| (PathBuf::from(path), entry.clone()))
            .filter(|(path, _)| exists(path))
            .collect();
        all.sort_by_key(|(_, entry)| std::cmp::Reverse(entry.opened));
        all.truncate(count);
        all
    }

    pub fn forget(&mut self) {
        self.files.clear();
    }
}

pub struct Saver {
    sender: mpsc::Sender<Memory>,
}

impl Saver {
    #[must_use]
    pub fn spawn(path: Option<PathBuf>) -> Self {
        let (sender, receiver) = mpsc::channel::<Memory>();
        let spawned = std::thread::Builder::new().name("glide-clips-memory".into()).spawn(move || {
            while let Ok(mut latest) = receiver.recv() {
                while let Ok(newer) = receiver.try_recv() {
                    latest = newer;
                }
                if let Some(path) = &path
                    && let Err(err) = write(path, &latest)
                {
                    tracing::warn!("could not remember playback positions: {err}");
                }
            }
        });
        if let Err(err) = spawned {
            tracing::warn!("could not start the memory writer: {err}");
        }
        Self { sender }
    }

    pub fn save(&self, memory: &Memory) {
        let _ = self.sender.send(memory.clone());
    }
}

pub fn write(path: &Path, memory: &Memory) -> std::io::Result<()> {
    let text = toml::to_string(memory).map_err(std::io::Error::other)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let partial = path.with_extension(format!("toml.{}.part", std::process::id()));
    std::fs::write(&partial, text)?;
    std::fs::rename(&partial, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(position: f64, duration: f64, opened: u64) -> Entry {
        Entry { position, duration, opened, ..Entry::default() }
    }

    #[test]
    fn resume_needs_a_start_and_more_than_two_minutes_left() {
        assert_eq!(entry(2534.0, 5400.0, 0).resume(), Some(Duration::from_secs(2534)));
        assert_eq!(entry(3.0, 5400.0, 0).resume(), None);
        assert_eq!(entry(5300.0, 5400.0, 0).resume(), None);
        assert_eq!(entry(30.0, 0.0, 0).resume(), None);
    }

    #[test]
    fn recent_lists_resumable_files_newest_first() {
        let mut memory = Memory::default();
        memory.files.insert("/a.mkv".into(), entry(600.0, 3600.0, 10));
        memory.files.insert("/b.mkv".into(), entry(600.0, 3600.0, 30));
        memory.files.insert("/c.mkv".into(), entry(3590.0, 3600.0, 50));
        memory.files.insert("/gone.mkv".into(), entry(600.0, 3600.0, 40));
        let recent = memory.recent(4, |path| path != Path::new("/gone.mkv"));
        let names: Vec<_> = recent.iter().map(|(path, _)| path.to_string_lossy().into_owned()).collect();
        assert_eq!(names, vec!["/b.mkv", "/a.mkv"]);
    }

    #[test]
    fn memory_round_trips_through_toml() {
        let dir = std::env::temp_dir().join(format!("glide-clips-memory-{}", std::process::id()));
        let file = dir.join("files.toml");
        let mut memory = Memory::default();
        *memory.entry(Path::new("/відео/Карпати.mkv")) = Entry {
            position: 12.5,
            duration: 90.0,
            opened: 7,
            volume: Some(0.7),
            text: Some("uk".into()),
            ..Entry::default()
        };
        write(&file, &memory).unwrap();
        assert_eq!(Memory::read(&file), memory);
        let _ = std::fs::remove_dir_all(dir);
    }
}

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

const HALF_LIFE_DAYS: f64 = 14.0;
const SECONDS_PER_DAY: f64 = 86_400.0;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Launches {
    pub count: u32,
    pub last_unix: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LaunchHistory {
    entries: HashMap<String, Launches>,
}

#[must_use]
pub fn now_unix() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |elapsed| elapsed.as_secs())
}

#[must_use]
pub fn default_path() -> Option<PathBuf> {
    Some(glide_config::state_home()?.join("glide/launches.json"))
}

impl LaunchHistory {
    #[must_use]
    pub fn parse(json: &str) -> Self {
        let entries = serde_json::from_str(json).unwrap_or_default();
        Self { entries }
    }

    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(&self.entries).unwrap_or_else(|_| "{}".to_string())
    }

    #[must_use]
    pub fn load_from(path: &Path) -> Self {
        std::fs::read_to_string(path).map(|json| Self::parse(&json)).unwrap_or_default()
    }

    #[must_use]
    pub fn load() -> Self {
        default_path().map_or_else(Self::default, |path| Self::load_from(&path))
    }

    #[must_use]
    pub fn get(&self, id: &str) -> Option<Launches> {
        self.entries.get(id).copied()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn record(&mut self, id: &str, now: u64) {
        if id.is_empty() {
            return;
        }
        let entry = self.entries.entry(id.to_string()).or_default();
        entry.count = entry.count.saturating_add(1);
        entry.last_unix = entry.last_unix.max(now);
    }

    #[must_use]
    pub fn frecency(&self, id: &str, now: u64) -> f64 {
        let Some(entry) = self.entries.get(id) else {
            return 0.0;
        };
        let age_days = now.saturating_sub(entry.last_unix) as f64 / SECONDS_PER_DAY;
        f64::from(entry.count) * 0.5_f64.powf(age_days / HALF_LIFE_DAYS)
    }

    pub fn save_to(&self, path: &Path) -> std::io::Result<()> {
        glide_config::write_atomic(path, self.to_json().as_bytes(), 0o644)
    }

    pub fn save_in_background(self) {
        let Some(path) = default_path() else {
            return;
        };
        std::thread::spawn(move || {
            if let Err(err) = self.save_to(&path) {
                tracing::warn!(?err, path = %path.display(), "failed to save launch history");
            }
        });
    }
}

const RUN_KEEP: usize = 20;

#[must_use]
pub fn run_path() -> Option<PathBuf> {
    Some(glide_config::state_home()?.join("glide/run-history"))
}

#[must_use]
pub fn load_runs() -> Vec<String> {
    run_path().and_then(|path| std::fs::read_to_string(path).ok()).map(|text| parse_runs(&text)).unwrap_or_default()
}

#[must_use]
pub fn parse_runs(text: &str) -> Vec<String> {
    text.lines().map(str::trim).filter(|line| !line.is_empty()).take(RUN_KEEP).map(str::to_string).collect()
}

#[must_use]
pub fn with_run(mut runs: Vec<String>, command: &str) -> Vec<String> {
    let command = command.trim();
    runs.retain(|run| run != command);
    runs.insert(0, command.to_string());
    runs.truncate(RUN_KEEP);
    runs
}

pub fn remember_run(command: &str) {
    let (Some(path), command) = (run_path(), command.trim().to_string()) else {
        return;
    };
    if command.is_empty() || command.contains('\n') {
        return;
    }
    let spawned = std::thread::Builder::new().name("glide-run-history".into()).spawn(move || {
        let runs = with_run(std::fs::read_to_string(&path).map(|text| parse_runs(&text)).unwrap_or_default(), &command);
        if let Err(err) = glide_config::write_private(&path, (runs.join("\n") + "\n").as_bytes()) {
            tracing::warn!(?err, path = %path.display(), "failed to save the run history");
        }
    });
    if let Err(err) = spawned {
        tracing::warn!(%err, "could not start the run history writer");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: u64 = 86_400;

    #[test]
    fn the_run_history_keeps_the_newest_command_first_without_repeats() {
        let runs = with_run(parse_runs("htop\n\nfirefox --private-window\n"), " htop ");
        assert_eq!(runs, ["htop", "firefox --private-window"]);
        let runs = with_run(runs, "nvim notes.md");
        assert_eq!(runs[0], "nvim notes.md");
        let many = (0..30).fold(Vec::new(), |runs, n| with_run(runs, &format!("cmd {n}")));
        assert_eq!(many.len(), RUN_KEEP);
        assert_eq!(many[0], "cmd 29");
    }

    #[test]
    fn recording_counts_launches_and_keeps_the_latest_time() {
        let mut history = LaunchHistory::default();
        history.record("firefox", 100);
        history.record("firefox", 50);
        assert_eq!(history.get("firefox"), Some(Launches { count: 2, last_unix: 100 }));
        history.record("", 100);
        assert_eq!(history.get(""), None, "an unnamed app is not tracked");
    }

    #[test]
    fn frecency_halves_every_half_life() {
        let mut history = LaunchHistory::default();
        for _ in 0..4 {
            history.record("a", 0);
        }
        assert!((history.frecency("a", 0) - 4.0).abs() < 1e-9);
        assert!((history.frecency("a", 14 * DAY) - 2.0).abs() < 1e-9);
        assert!(history.frecency("missing", 0).abs() < f64::EPSILON);
    }

    #[test]
    fn json_round_trips_and_garbage_is_an_empty_history() {
        let mut history = LaunchHistory::default();
        history.record("org.gnome.Nautilus", 1_700_000_000);
        let parsed = LaunchHistory::parse(&history.to_json());
        assert_eq!(parsed, history);
        assert!(LaunchHistory::parse("not json").is_empty());
        assert!(LaunchHistory::parse("{\"a\": 3}").is_empty());
    }

    #[test]
    fn saving_is_atomic_and_leaves_no_temporary_files() {
        let dir = std::env::temp_dir().join(format!("glide-history-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("nested").join("launches.json");
        let mut history = LaunchHistory::default();
        history.record("x", 7);
        history.save_to(&path).expect("save");
        assert_eq!(LaunchHistory::load_from(&path), history);
        let leftovers = std::fs::read_dir(path.parent().expect("parent")).expect("list").count();
        assert_eq!(leftovers, 1, "only launches.json remains");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

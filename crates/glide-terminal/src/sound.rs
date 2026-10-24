use std::path::PathBuf;
use std::process::{Command, Stdio};

const BUNDLED: &str = "glide/sounds/terminal-bell.mp3";
const PLAYERS: [&str; 2] = ["pw-play", "paplay"];

fn expand(path: &str) -> PathBuf {
    path.strip_prefix("~/")
        .and_then(|rest| glide_config::home().map(|home| home.join(rest)))
        .unwrap_or_else(|| PathBuf::from(path))
}

#[must_use]
pub(crate) fn bell_file(configured: Option<&str>) -> Option<PathBuf> {
    if let Some(path) = configured.map(str::trim).filter(|path| !path.is_empty()) {
        return Some(expand(path)).filter(|path| path.is_file());
    }
    let source = cfg!(debug_assertions)
        .then(|| PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/sounds/terminal-bell.mp3")));
    glide_config::data_dirs().into_iter().map(|dir| dir.join(BUNDLED)).chain(source).find(|path| path.is_file())
}

pub fn play(file: PathBuf) {
    let _ = std::thread::Builder::new().name("glide-terminal-bell".into()).spawn(move || {
        for player in PLAYERS {
            let started = Command::new(player)
                .arg(&file)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn();
            if let Ok(mut child) = started
                && child.wait().is_ok_and(|status| status.success())
            {
                return;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bundled_bell_is_found_and_a_missing_custom_one_is_not_replaced() {
        assert!(bell_file(None).is_some_and(|path| path.ends_with("terminal-bell.mp3")));
        assert_eq!(bell_file(Some("/nonexistent/bell.ogg")), None);
        assert_eq!(bell_file(Some("  ")), bell_file(None));
    }
}

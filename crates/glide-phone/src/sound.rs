use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::process::Command;

const PLAYERS: [(&str, &[&str]); 3] = [("pw-play", &[]), ("paplay", &[]), ("canberra-gtk-play", &["-f"])];

const ALARM_SOUNDS: [&str; 4] = ["alarm-clock-elapsed", "phone-incoming-call", "complete", "bell"];

const EXTENSIONS: [&str; 3] = ["oga", "ogg", "wav"];

fn find_alarm_sound() -> Option<PathBuf> {
    let data_dirs = [
        PathBuf::from("/usr/share/sounds/freedesktop/stereo"),
        PathBuf::from("/usr/local/share/sounds/freedesktop/stereo"),
    ];

    for name in ALARM_SOUNDS {
        for dir in &data_dirs {
            for ext in EXTENSIONS {
                let candidate = dir.join(format!("{name}.{ext}"));
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

fn find_player() -> Option<(&'static str, &'static [&'static str])> {
    PLAYERS.iter().copied().find(|(player, _)| glide_config::on_path(player))
}

#[derive(Debug, Clone)]
pub struct AlarmHandle {
    active: Arc<AtomicBool>,
}

impl AlarmHandle {
    pub fn stop(&self) {
        self.active.store(false, Ordering::SeqCst);
    }

    #[must_use]
    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }
}

#[must_use]
pub fn start_alarm_loop() -> AlarmHandle {
    let active = Arc::new(AtomicBool::new(true));
    let handle = AlarmHandle { active: active.clone() };

    tokio::spawn(async move {
        let Some((player, args)) = find_player() else {
            tracing::warn!("no audio player found (pw-play, paplay, canberra-gtk-play)");
            return;
        };

        let Some(sound_file) = find_alarm_sound() else {
            tracing::warn!("no alarm sound file found in freedesktop stereo theme");
            return;
        };

        while active.load(Ordering::SeqCst) {
            let mut cmd = Command::new(player);
            cmd.args(args);
            cmd.arg(&sound_file);

            match cmd.spawn() {
                Ok(mut child) => {
                    tokio::select! {
                        _ = child.wait() => {}
                        () = async {
                            while active.load(Ordering::SeqCst) {
                                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                            }
                        } => {
                            let _ = child.kill().await;
                            break;
                        }
                    }
                }
                Err(err) => {
                    tracing::warn!(player, %err, "failed to spawn audio player");
                    break;
                }
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }
    });

    handle
}

pub fn play_sound_once(path: &Path) {
    let Some((player, args)) = find_player() else {
        return;
    };
    let mut cmd = std::process::Command::new(player);
    cmd.args(args).arg(path);
    let _ = cmd.spawn();
}

use std::path::{Path, PathBuf};

use glide_notify::Sound;

const PLAYERS: [(&str, &[&str]); 3] = [("pw-play", &[]), ("paplay", &[]), ("canberra-gtk-play", &["-f"])];
const THEMES: [&str; 1] = ["freedesktop"];
const EXTENSIONS: [&str; 3] = ["oga", "ogg", "wav"];

fn valid_name(name: &str) -> bool {
    !name.is_empty() && !name.contains('/') && !name.starts_with('.')
}

#[must_use]
pub fn theme_file(name: &str, data_dirs: &[PathBuf], exists: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    if !valid_name(name) {
        return None;
    }
    let mut candidate = name;
    loop {
        for dir in data_dirs {
            for theme in THEMES {
                for extension in EXTENSIONS {
                    let path = dir.join("sounds").join(theme).join("stereo").join(format!("{candidate}.{extension}"));
                    if exists(&path) {
                        return Some(path);
                    }
                }
            }
        }
        candidate = &candidate[..candidate.rfind('-')?];
    }
}

fn resolve(sound: &Sound) -> Option<PathBuf> {
    match sound {
        Sound::None => None,
        Sound::File(path) => Some(PathBuf::from(path)).filter(|path| path.is_absolute() && path.is_file()),
        Sound::Named(name) => {
            let dirs: Vec<PathBuf> = glide_config::data_home().into_iter().chain(glide_config::data_dirs()).collect();
            theme_file(name, &dirs, Path::is_file)
        }
    }
}

use glide_config::on_path;

pub fn play(sound: Sound) {
    let spawned = std::thread::Builder::new().name("glide-sound".into()).spawn(move || {
        let Some(file) = resolve(&sound) else {
            tracing::debug!(?sound, "no file for a notification sound");
            return;
        };
        let Some((player, args)) = PLAYERS.iter().find(|(player, _)| on_path(player)) else {
            tracing::debug!("no sound player (pw-play, paplay, canberra-gtk-play) is installed");
            return;
        };
        let mut command = std::process::Command::new(player);
        command
            .args(*args)
            .arg(&file)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        if let Err(err) = crate::spawn_detached(command) {
            tracing::debug!(player, %err, "could not play a notification sound");
        }
    });
    if let Err(err) = spawned {
        tracing::debug!(%err, "could not start the sound thread");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sound_name_falls_back_along_its_dashes_and_through_the_data_dirs() {
        let dirs = [PathBuf::from("/home/u/.local/share"), PathBuf::from("/usr/share")];
        let present = PathBuf::from("/usr/share/sounds/freedesktop/stereo/message.oga");
        let exists = |path: &Path| path == present;
        assert_eq!(theme_file("message-new-instant", &dirs, exists), Some(present.clone()));
        assert_eq!(theme_file("message", &dirs, exists), Some(present.clone()));
        assert_eq!(theme_file("bell", &dirs, exists), None);
    }

    #[test]
    fn a_sound_name_cannot_climb_out_of_the_theme() {
        let anything = |_: &Path| true;
        for name in ["../../etc/passwd", "a/b", ".hidden", ""] {
            assert_eq!(theme_file(name, &[PathBuf::from("/usr/share")], anything), None, "{name}");
        }
    }

    #[test]
    fn only_an_existing_absolute_file_is_played_as_is() {
        assert_eq!(resolve(&Sound::None), None);
        assert_eq!(resolve(&Sound::File("relative.oga".into())), None);
        assert_eq!(resolve(&Sound::File("/glide/test/missing.oga".into())), None);
    }
}

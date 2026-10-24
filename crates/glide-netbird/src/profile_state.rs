use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 255
        && !id.starts_with('.')
        && id.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

#[must_use]
pub fn directory() -> Option<PathBuf> {
    glide_config::config_home().map(|home| home.join("netbird"))
}

fn state_file(dir: &Path, id: &str) -> Option<PathBuf> {
    valid_id(id).then(|| dir.join(format!("{id}.state.json")))
}

fn write_private(path: &Path, contents: &[u8]) -> io::Result<()> {
    let dir = path.parent().ok_or_else(|| io::Error::other("no parent directory"))?;
    fs::create_dir_all(dir)?;
    let temp = dir.join(format!(".{}.tmp", path.file_name().and_then(|name| name.to_str()).unwrap_or("state")));
    match fs::remove_file(&temp) {
        Err(err) if err.kind() != io::ErrorKind::NotFound => return Err(err),
        _ => {}
    }
    let mut file = fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&temp)?;
    file.write_all(contents)?;
    file.sync_all()?;
    fs::rename(&temp, path)
}

#[must_use]
pub(crate) fn email_in(dir: &Path, id: &str) -> Option<String> {
    let text = fs::read_to_string(state_file(dir, id)?).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    value.get("email")?.as_str().filter(|email| !email.is_empty()).map(str::to_string)
}

pub(crate) fn store_email_in(dir: &Path, id: &str, email: &str) -> io::Result<()> {
    let path = state_file(dir, id).ok_or_else(|| io::Error::other("invalid profile id"))?;
    let text = serde_json::to_vec(&serde_json::json!({ "email": email })).map_err(io::Error::other)?;
    write_private(&path, &text)
}

pub(crate) fn remove_in(dir: &Path, id: &str) -> io::Result<()> {
    let Some(path) = state_file(dir, id) else {
        return Ok(());
    };
    match fs::remove_file(path) {
        Err(err) if err.kind() != io::ErrorKind::NotFound => Err(err),
        _ => Ok(()),
    }
}

pub(crate) fn mark_active_in(dir: &Path, id: &str) -> io::Result<()> {
    if !valid_id(id) {
        return Err(io::Error::other("invalid profile id"));
    }
    write_private(&dir.join("active_profile.txt"), id.as_bytes())
}

#[must_use]
pub fn email(id: &str) -> Option<String> {
    email_in(&directory()?, id)
}

pub(crate) fn store_email(id: &str, email: &str) -> io::Result<()> {
    store_email_in(&directory().ok_or_else(|| io::Error::other("no config directory"))?, id, email)
}

pub fn remove(id: &str) -> io::Result<()> {
    directory().map_or(Ok(()), |dir| remove_in(&dir, id))
}

pub(crate) fn mark_active(id: &str) -> io::Result<()> {
    mark_active_in(&directory().ok_or_else(|| io::Error::other("no config directory"))?, id)
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("glide-netbird-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn the_account_email_round_trips_through_a_private_state_file() {
        let dir = scratch("email");
        assert_eq!(email_in(&dir, "default"), None);
        store_email_in(&dir, "default", "me@example.com").expect("store");
        assert_eq!(email_in(&dir, "default").as_deref(), Some("me@example.com"));
        let mode = fs::metadata(dir.join("default.state.json")).expect("meta").permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        remove_in(&dir, "default").expect("remove");
        remove_in(&dir, "default").expect("removing twice is fine");
        assert_eq!(email_in(&dir, "default"), None);
        fs::remove_dir_all(&dir).expect("cleanup");
    }

    #[test]
    fn profile_ids_that_could_leave_the_directory_are_refused() {
        let dir = scratch("ids");
        for id in ["", "../escape", ".hidden", "a/b", "with space"] {
            assert!(store_email_in(&dir, id, "x@y").is_err(), "{id}");
            assert!(mark_active_in(&dir, id).is_err(), "{id}");
            assert_eq!(email_in(&dir, id), None);
        }
        mark_active_in(&dir, "work-2").expect("valid id");
        assert_eq!(fs::read_to_string(dir.join("active_profile.txt")).expect("read"), "work-2");
        fs::remove_dir_all(&dir).expect("cleanup");
    }

    #[test]
    fn a_leftover_world_readable_temp_file_does_not_leak_its_mode_into_the_state_file() {
        let dir = scratch("leftover");
        fs::create_dir_all(&dir).expect("mkdir");
        let temp = dir.join(".default.state.json.tmp");
        fs::write(&temp, b"stale").expect("leftover");
        fs::set_permissions(&temp, fs::Permissions::from_mode(0o644)).expect("chmod");
        store_email_in(&dir, "default", "me@example.com").expect("store");
        let mode = fs::metadata(dir.join("default.state.json")).expect("meta").permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        assert!(!temp.exists());
        assert_eq!(email_in(&dir, "default").as_deref(), Some("me@example.com"));
        fs::remove_dir_all(&dir).expect("cleanup");
    }
}

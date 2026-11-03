use std::{
    collections::HashMap,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Mutex, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant, SystemTime},
};

const TIMEOUT: Duration = Duration::from_secs(3);
const MARKER: &[u8] = b"\0__GLIDE_LOGIN_ENV__\0";
const READ_PROFILE: &str = r"printf '\0__GLIDE_LOGIN_ENV__\0'; env -0";
const LOGIN_CAPABLE: &[&str] = &["bash", "zsh", "ksh", "mksh", "dash", "sh", "yash", "fish"];
const SKIPPED: &[&str] = &[
    "_",
    "PWD",
    "OLDPWD",
    "SHLVL",
    "COLORTERM",
    "TERM",
    "TERM_PROGRAM",
    "TERM_PROGRAM_VERSION",
    "DESKTOP_STARTUP_ID",
    "XDG_ACTIVATION_TOKEN",
];

static ENABLED: AtomicBool = AtomicBool::new(true);

type Stamp = Vec<(PathBuf, Option<SystemTime>)>;
static CACHE: Mutex<Option<(Stamp, HashMap<String, String>)>> = Mutex::new(None);

pub fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
}

#[must_use]
pub fn shell_for_profile(shell: Option<&str>) -> PathBuf {
    shell
        .filter(|shell| {
            Path::new(shell)
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| LOGIN_CAPABLE.contains(&name))
        })
        .map_or_else(|| PathBuf::from("/bin/sh"), PathBuf::from)
}

fn profile_files(home: Option<&Path>) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = [
        "/etc/profile",
        "/etc/profile.d",
        "/etc/environment",
        "/etc/bash.bashrc",
        "/etc/zsh/zprofile",
        "/etc/zsh/zshenv",
        "/etc/zprofile",
        "/etc/zshenv",
        "/etc/fish/config.fish",
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect();
    if let Ok(entries) = std::fs::read_dir("/etc/profile.d") {
        files.extend(entries.filter_map(Result::ok).map(|entry| entry.path()));
    }
    if let Some(home) = home {
        for name in [
            ".profile",
            ".bash_profile",
            ".bash_login",
            ".bashrc",
            ".zprofile",
            ".zshenv",
            ".zlogin",
            ".zshrc",
            ".config/fish/config.fish",
        ] {
            files.push(home.join(name));
        }
    }
    files.sort();
    files
}

fn stamp(files: Vec<PathBuf>) -> Stamp {
    files
        .into_iter()
        .map(|file| {
            let modified = std::fs::metadata(&file).and_then(|meta| meta.modified()).ok();
            (file, modified)
        })
        .collect()
}

#[must_use]
pub fn parse(output: &[u8]) -> Option<HashMap<String, String>> {
    let start = output.windows(MARKER.len()).rposition(|window| window == MARKER)? + MARKER.len();
    Some(
        output[start..]
            .split(|byte| *byte == 0)
            .filter_map(|entry| {
                let entry = std::str::from_utf8(entry).ok()?;
                let (name, value) = entry.split_once('=')?;
                valid_name(name).then(|| (name.to_string(), value.to_string()))
            })
            .collect(),
    )
}

fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|first| first == '_' || first.is_ascii_alphabetic())
        && chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

#[must_use]
pub fn overlay<S: std::hash::BuildHasher>(
    login: &HashMap<String, String, S>, current: &HashMap<String, String, S>,
) -> HashMap<String, String> {
    login
        .iter()
        .filter(|(name, value)| !SKIPPED.contains(&name.as_str()) && current.get(*name) != Some(*value))
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect()
}

fn capture(shell: &Path, home: Option<&Path>) -> Option<HashMap<String, String>> {
    let mut command = Command::new(shell);
    command.args(["-l", "-c", READ_PROFILE]).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    if let Some(home) = home {
        command.current_dir(home);
    }
    let mut child = command.spawn().ok()?;
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::Builder::new()
        .name("glide-terminal-login-env".into())
        .spawn(move || {
            let mut output = Vec::new();
            let _ = stdout.read_to_end(&mut output);
            output
        })
        .ok()?;
    let deadline = Instant::now() + TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    parse(&reader.join().ok()?)
}

#[must_use]
pub fn fresh() -> HashMap<String, String> {
    if !ENABLED.load(Ordering::Relaxed) {
        return HashMap::new();
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let now = stamp(profile_files(home.as_deref()));
    let mut cache = CACHE.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some((stamped, env)) = cache.as_ref()
        && *stamped == now
    {
        return env.clone();
    }
    let shell = shell_for_profile(std::env::var("SHELL").ok().as_deref());
    let current: HashMap<String, String> = std::env::vars().collect();
    let env = capture(&shell, home.as_deref()).map(|login| overlay(&login, &current)).unwrap_or_default();
    *cache = Some((now, env.clone()));
    env
}

pub fn warm() {
    let _ = std::thread::Builder::new().name("glide-terminal-login-env".into()).spawn(|| {
        let _ = fresh();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_printed_by_profile_scripts_before_the_marker_is_ignored() {
        let mut output = b"Welcome!\n\0junk=1\0".to_vec();
        output.extend_from_slice(MARKER);
        output.extend_from_slice(b"PATH=/home/me/.cargo/bin:/usr/bin\0EDITOR=nvim\0bad name=x\0MULTI=a\nb\0");
        let env = parse(&output).expect("the marker is there");
        assert_eq!(env.get("PATH").map(String::as_str), Some("/home/me/.cargo/bin:/usr/bin"));
        assert_eq!(env.get("MULTI").map(String::as_str), Some("a\nb"), "values may span lines");
        assert!(!env.contains_key("junk"), "only what follows the marker");
        assert!(!env.contains_key("bad name"));
        assert_eq!(parse(b"no marker at all"), None);
    }

    #[test]
    fn only_what_the_login_profile_changed_is_passed_on() {
        let current = HashMap::from([
            ("PATH".to_string(), "/usr/bin".to_string()),
            ("WAYLAND_DISPLAY".to_string(), "wayland-1".to_string()),
        ]);
        let login = HashMap::from([
            ("PATH".to_string(), "/home/me/.local/bin:/usr/bin".to_string()),
            ("WAYLAND_DISPLAY".to_string(), "wayland-1".to_string()),
            ("RUSTUP_HOME".to_string(), "/opt/rustup".to_string()),
            ("SHLVL".to_string(), "1".to_string()),
            ("PWD".to_string(), "/home/me".to_string()),
        ]);
        let changed = overlay(&login, &current);
        assert_eq!(changed.len(), 2, "{changed:?}");
        assert_eq!(changed["PATH"], "/home/me/.local/bin:/usr/bin");
        assert_eq!(changed["RUSTUP_HOME"], "/opt/rustup");
    }

    #[test]
    fn shells_that_cannot_run_the_probe_fall_back_to_sh() {
        assert_eq!(shell_for_profile(Some("/usr/bin/zsh")), PathBuf::from("/usr/bin/zsh"));
        assert_eq!(shell_for_profile(Some("/usr/bin/fish")), PathBuf::from("/usr/bin/fish"));
        assert_eq!(shell_for_profile(Some("/usr/bin/nu")), PathBuf::from("/bin/sh"));
        assert_eq!(shell_for_profile(None), PathBuf::from("/bin/sh"));
    }

    #[test]
    fn a_real_login_shell_reports_its_environment() {
        let Some(env) = capture(Path::new("/bin/sh"), None) else {
            return;
        };
        assert!(env.contains_key("PATH"), "a login shell always has a PATH");
    }

    #[test]
    fn the_profile_stamp_changes_when_a_profile_file_does() {
        let dir = std::env::temp_dir().join(format!("glide-terminal-login-env-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("scratch dir");
        let profile = dir.join(".profile");
        let before = stamp(profile_files(Some(&dir)));
        std::fs::write(&profile, "export FOO=1\n").expect("write profile");
        let after = stamp(profile_files(Some(&dir)));
        assert_ne!(before, after, "a new ~/.profile must invalidate the cache");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

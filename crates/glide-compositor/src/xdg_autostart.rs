use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Phase {
    Initialization,
    WindowManager,
    Panel,
    Desktop,
    #[default]
    Applications,
}

impl Phase {
    fn parse(value: &str) -> Self {
        match value {
            "EarlyInitialization" | "PreDisplayServer" | "DisplayServer" | "Initialization" => Self::Initialization,
            "WindowManager" => Self::WindowManager,
            "Panel" => Self::Panel,
            "Desktop" => Self::Desktop,
            _ => Self::Applications,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub command: String,
    pub phase: Phase,
    pub delay: u64,
}

impl Entry {
    #[must_use]
    pub fn shell_command(&self) -> String {
        if self.delay == 0 { self.command.clone() } else { format!("sleep {}; {}", self.delay, self.command) }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    pub id: String,
    pub entry: Entry,
}

pub struct Probe<'a> {
    pub program: &'a dyn Fn(&str) -> bool,
    pub gsetting: &'a dyn Fn(&str, &str) -> Option<bool>,
    pub config_file: &'a dyn Fn(&str) -> bool,
}

fn condition_met(condition: &str, desktops: &[String], probe: &Probe<'_>) -> bool {
    let mut words = condition.split_whitespace();
    match (words.next(), words.next(), words.next()) {
        (Some("GSettings"), Some(schema), Some(key)) => (probe.gsetting)(schema, key).unwrap_or(false),
        (Some("if-exists"), Some(file), None) => (probe.config_file)(file),
        (Some("unless-exists"), Some(file), None) => !(probe.config_file)(file),
        (Some("GNOME3"), Some("if-session"), Some(session)) => desktops.contains(&session.to_lowercase()),
        (Some("GNOME3"), Some("unless-session"), Some(session)) => !desktops.contains(&session.to_lowercase()),
        _ => false,
    }
}

fn list(value: &str) -> Vec<String> {
    value.split(';').map(str::trim).filter(|item| !item.is_empty()).map(str::to_lowercase).collect()
}

fn exec_command(exec: &str) -> String {
    exec.split_whitespace().filter(|word| !(word.len() == 2 && word.starts_with('%'))).collect::<Vec<_>>().join(" ")
}

fn on_path(program: &str) -> bool {
    let path = Path::new(program);
    if path.is_absolute() {
        return path.is_file();
    }
    std::env::var_os("PATH").is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(program).is_file()))
}

#[must_use]
pub fn parse_entry(text: &str, desktops: &[String], probe: &Probe<'_>) -> Option<Entry> {
    let mut exec = None;
    let mut is_app = false;
    let mut skipped = false;
    let mut condition = None;
    let mut only: Option<Vec<String>> = None;
    let mut not: Vec<String> = Vec::new();
    let mut try_exec = None;
    let mut phase = Phase::default();
    let mut delay = 0;
    for (key, value) in glide_config::desktop::entry(text) {
        match key {
            "Type" => is_app = value == "Application",
            "Exec" => exec = Some(value.to_string()),
            "Hidden" | "Terminal" => skipped |= value == "true",
            "X-GNOME-Autostart-enabled" => skipped |= value == "false",
            "AutostartCondition" => condition = Some(value.to_string()),
            "OnlyShowIn" => only = Some(list(value)),
            "NotShowIn" => not = list(value),
            "TryExec" => try_exec = Some(value.to_string()),
            "X-GNOME-Autostart-Phase" => phase = Phase::parse(value),
            "X-GNOME-Autostart-Delay" => delay = value.parse().unwrap_or(0),
            _ => {}
        }
    }
    if !is_app || skipped {
        return None;
    }
    if let Some(only) = &only
        && !desktops.iter().any(|desktop| only.contains(desktop))
    {
        return None;
    }
    if desktops.iter().any(|desktop| not.contains(desktop)) {
        return None;
    }
    if let Some(program) = try_exec.filter(|program| !program.is_empty())
        && !(probe.program)(&program)
    {
        return None;
    }
    if let Some(condition) = condition.filter(|condition| !condition.is_empty())
        && !condition_met(&condition, desktops, probe)
    {
        return None;
    }
    let command = exec_command(&exec?);
    (!command.is_empty() && !glide_config::autostart::provided_by_shell(&command)).then_some(Entry {
        command,
        phase,
        delay,
    })
}

fn gsetting(schema: &str, key: &str) -> Option<bool> {
    let output = std::process::Command::new("gsettings").args(["get", schema, key]).output().ok()?;
    match String::from_utf8_lossy(&output.stdout).trim() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn config_file(name: &str) -> bool {
    glide_config::config_home().is_some_and(|home| home.join(name).exists())
}

#[must_use]
pub fn autostart_dirs() -> Vec<PathBuf> {
    glide_config::config_dirs().into_iter().map(|dir| dir.join("autostart")).collect()
}

#[must_use]
pub fn current_desktops() -> Vec<String> {
    let names: Vec<String> = std::env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .split(':')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_lowercase)
        .collect();
    if names.is_empty() { vec![String::from("glide")] } else { names }
}

#[must_use]
pub fn launches(dirs: &[PathBuf], desktops: &[String], probe: &Probe<'_>) -> Vec<Launch> {
    let mut seen = HashSet::new();
    let mut found = Vec::new();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        let mut paths: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "desktop"))
            .collect();
        paths.sort();
        for path in paths {
            let Some(id) = path.file_name().map(|name| name.to_string_lossy().into_owned()) else {
                continue;
            };
            if !seen.insert(id.clone()) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            if let Some(entry) = parse_entry(&text, desktops, probe) {
                found.push(Launch { id, entry });
            }
        }
    }
    found.sort_by_key(|launch| launch.entry.phase);
    found
}

pub fn spawn_all() {
    let spawned = std::thread::Builder::new().name(String::from("glide-xdg-autostart")).spawn(|| {
        let probe = Probe { program: &on_path, gsetting: &gsetting, config_file: &config_file };
        for launch in launches(&autostart_dirs(), &current_desktops(), &probe) {
            match crate::spawn_shell_command(&launch.entry.shell_command()) {
                Ok(pid) => tracing::info!(id = launch.id, pid, "started autostart entry"),
                Err(err) => {
                    tracing::warn!(id = launch.id, "failed to start autostart entry: {err}");
                }
            }
        }
    });
    if let Err(err) = spawned {
        tracing::warn!("could not start the XDG autostart thread: {err}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn glide() -> Vec<String> {
        vec![String::from("glide")]
    }

    const INSTALLED: Probe<'static> = Probe { program: &|_| true, gsetting: &|_, _| None, config_file: &|_| false };

    fn command(text: &str, probe: &Probe<'_>) -> Option<String> {
        parse_entry(text, &glide(), probe).map(|entry| entry.shell_command())
    }

    fn conditional(condition: &str) -> String {
        format!("[Desktop Entry]\nType=Application\nExec=app\nAutostartCondition={condition}\n")
    }

    #[test]
    fn plain_entries_run_with_field_codes_stripped() {
        let text = "[Desktop Entry]\nType=Application\nExec=syncthing serve --no-browser %U\n";
        assert_eq!(command(text, &INSTALLED).as_deref(), Some("syncthing serve --no-browser"));
    }

    #[test]
    fn applets_the_shell_replaces_are_not_started() {
        for exec in ["nm-applet --indicator %U", "/usr/bin/nm-applet", "blueman-applet", "/usr/bin/netbird-ui"] {
            let text = format!("[Desktop Entry]\nType=Application\nExec={exec}\nNotShowIn=KDE;GNOME;\n");
            assert_eq!(command(&text, &INSTALLED), None, "{exec}");
        }
    }

    #[test]
    fn hidden_disabled_terminal_and_unknown_conditional_entries_are_skipped() {
        for extra in [
            "Hidden=true",
            "Terminal=true",
            "X-GNOME-Autostart-enabled=false",
            "AutostartCondition=GNOME /desktop/gnome/applications/at/screen_reader_enabled",
        ] {
            let text = format!("[Desktop Entry]\nType=Application\nExec=app\n{extra}\n");
            assert_eq!(command(&text, &INSTALLED), None, "{extra}");
        }
    }

    #[test]
    fn a_gsettings_condition_follows_the_key_and_fails_closed() {
        let text = conditional("GSettings org.gnome.desktop.a11y.applications screen-reader-enabled");
        let with = |value: Option<bool>| {
            let gsetting = move |schema: &str, key: &str| {
                assert_eq!((schema, key), ("org.gnome.desktop.a11y.applications", "screen-reader-enabled"));
                value
            };
            command(&text, &Probe { gsetting: &gsetting, ..INSTALLED })
        };
        assert_eq!(with(Some(true)).as_deref(), Some("app"));
        assert_eq!(with(Some(false)), None);
        assert_eq!(with(None), None, "no gsettings, no schema or a non-boolean key");
    }

    #[test]
    fn file_and_session_conditions_are_evaluated() {
        let has_file = Probe { config_file: &|name| name == "marker", ..INSTALLED };
        assert!(command(&conditional("if-exists marker"), &has_file).is_some());
        assert!(command(&conditional("if-exists other"), &has_file).is_none());
        assert!(command(&conditional("unless-exists marker"), &has_file).is_none());
        assert!(command(&conditional("unless-exists other"), &has_file).is_some());
        assert!(command(&conditional("GNOME3 unless-session gnome"), &INSTALLED).is_some());
        assert!(command(&conditional("GNOME3 if-session gnome"), &INSTALLED).is_none());
        assert!(command(&conditional("GNOME3 if-session Glide"), &INSTALLED).is_some());
    }

    #[test]
    fn a_delay_is_slept_before_the_command_and_phases_order_the_launches() {
        let delayed = "[Desktop Entry]\nType=Application\nExec=tray --quiet\nX-GNOME-Autostart-Delay=5\n";
        assert_eq!(command(delayed, &INSTALLED).as_deref(), Some("sleep 5; tray --quiet"));
        let bogus = "[Desktop Entry]\nType=Application\nExec=tray\nX-GNOME-Autostart-Delay=soon\n";
        assert_eq!(command(bogus, &INSTALLED).as_deref(), Some("tray"));
        assert!(Phase::parse("Initialization") < Phase::parse("Panel"));
        assert!(Phase::parse("Panel") < Phase::parse("Applications"));
        assert_eq!(Phase::parse("Nonsense"), Phase::default());
    }

    #[test]
    fn show_in_lists_are_matched_against_the_desktop() {
        let only = "[Desktop Entry]\nType=Application\nExec=a\nOnlyShowIn=GNOME;Unity;\n";
        assert_eq!(command(only, &INSTALLED), None);
        let only_glide = "[Desktop Entry]\nType=Application\nExec=a\nOnlyShowIn=Glide;\n";
        assert!(command(only_glide, &INSTALLED).is_some());
        let not = "[Desktop Entry]\nType=Application\nExec=a\nNotShowIn=Glide;KDE;\n";
        assert_eq!(command(not, &INSTALLED), None);
    }

    #[test]
    fn a_missing_try_exec_program_skips_the_entry() {
        let text = "[Desktop Entry]\nType=Application\nExec=b\nTryExec=missing-binary\n";
        assert_eq!(command(text, &Probe { program: &|_| false, ..INSTALLED }), None);
        assert!(command(text, &INSTALLED).is_some());
    }

    #[test]
    fn a_user_entry_overrides_the_system_one_with_the_same_name() {
        let root = std::env::temp_dir().join(format!("glide-xdg-autostart-{}", std::process::id()));
        let user = root.join("user");
        let system = root.join("system");
        std::fs::create_dir_all(&user).expect("user dir");
        std::fs::create_dir_all(&system).expect("system dir");
        std::fs::write(system.join("a.desktop"), "[Desktop Entry]\nType=Application\nExec=sh\n").expect("system a");
        std::fs::write(system.join("b.desktop"), "[Desktop Entry]\nType=Application\nExec=sh -c true\n")
            .expect("system b");
        std::fs::write(
            system.join("c.desktop"),
            "[Desktop Entry]\nType=Application\nExec=sh -c early\nX-GNOME-Autostart-Phase=Initialization\n",
        )
        .expect("system c");
        std::fs::write(user.join("a.desktop"), "[Desktop Entry]\nType=Application\nExec=sh\nHidden=true\n")
            .expect("user a");
        let found = launches(&[user, system], &glide(), &INSTALLED);
        let started: Vec<(&str, &str)> =
            found.iter().map(|launch| (launch.id.as_str(), launch.entry.command.as_str())).collect();
        assert_eq!(started, [("c.desktop", "sh -c early"), ("b.desktop", "sh -c true")], "earlier phases go first");
        std::fs::remove_dir_all(&root).expect("cleanup");
    }
}

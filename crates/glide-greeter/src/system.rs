use std::{fs, path::Path};

use glide_config::desktop::entry_value as desktop_value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserEntry {
    pub username: String,
    pub display_name: String,
    pub uid: u32,
    pub initial: char,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionEntry {
    pub id: String,
    pub name: String,
    pub exec: Vec<String>,
    pub desktop_names: String,
}

impl SessionEntry {
    #[must_use]
    pub fn env(&self) -> Vec<String> {
        let mut env = vec!["XDG_SESSION_TYPE=wayland".to_string(), format!("XDG_SESSION_DESKTOP={}", self.id)];
        if !self.desktop_names.is_empty() {
            env.push(format!("XDG_CURRENT_DESKTOP={}", self.desktop_names));
        }
        if self.is_glide() {
            env.push(format!("{}=1", crate::handoff::ENV));
        }
        env
    }

    #[must_use]
    pub fn is_glide(&self) -> bool {
        self.id.starts_with("glide")
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct GreeterStatus {
    pub battery: Option<(u8, bool)>,
    pub wifi: Option<u8>,
}

const UID_MIN: u32 = 1000;
const UID_MAX: u32 = 60000;

#[must_use]
pub fn list_users() -> Vec<UserEntry> {
    let users = fs::read("/etc/passwd").map(|content| parse_passwd_bytes(&content)).unwrap_or_default();
    if users.is_empty() { default_users() } else { users }
}

#[must_use]
pub fn parse_passwd_bytes(content: &[u8]) -> Vec<UserEntry> {
    parse_passwd(&String::from_utf8_lossy(content))
}

#[must_use]
pub fn parse_passwd(content: &str) -> Vec<UserEntry> {
    let mut users = Vec::new();
    for line in content.lines() {
        let fields: Vec<&str> = line.split(':').collect();
        if fields.len() < 7 {
            continue;
        }
        let username = fields[0].trim();
        let Ok(uid) = fields[2].trim().parse::<u32>() else {
            continue;
        };
        let shell = fields[6].trim();
        if !(UID_MIN..=UID_MAX).contains(&uid)
            || shell.ends_with("/false")
            || shell.ends_with("/nologin")
            || username.is_empty()
        {
            continue;
        }
        let gecos = fields[4].split(',').next().unwrap_or_default().trim();
        let display_name = if gecos.is_empty() { username } else { gecos };
        users.push(UserEntry {
            username: username.to_string(),
            display_name: display_name.to_string(),
            uid,
            initial: initial_of(display_name),
        });
    }
    users
}

fn initial_of(name: &str) -> char {
    name.chars().find(|c| c.is_alphanumeric()).and_then(|c| c.to_uppercase().next()).unwrap_or('?')
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LastLogin {
    pub user: String,
    pub session: String,
}

impl LastLogin {
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let value = |key: &str| {
            text.lines()
                .filter_map(|line| line.split_once('='))
                .find(|(name, _)| name.trim() == key)
                .map(|(_, value)| value.trim().to_string())
                .unwrap_or_default()
        };
        Self { user: value("user"), session: value("session") }
    }

    #[must_use]
    pub fn serialize(&self) -> String {
        format!("user={}\nsession={}\n", self.user, self.session)
    }

    fn path() -> Option<std::path::PathBuf> {
        Some(glide_config::state_home()?.join("glide").join("greeter-last"))
    }

    #[must_use]
    pub fn load() -> Self {
        Self::path().and_then(|path| fs::read_to_string(path).ok()).map(|text| Self::parse(&text)).unwrap_or_default()
    }

    pub fn save(&self) {
        let Some(path) = Self::path() else {
            return;
        };
        let written =
            path.parent().map_or(Ok(()), fs::create_dir_all).and_then(|()| fs::write(&path, self.serialize()));
        if let Err(err) = written {
            tracing::debug!(?err, path = %path.display(), "the greeter cannot remember the last login");
        }
    }
}

#[must_use]
pub fn current_user(display_name: &str) -> UserEntry {
    let (uid, login) = login_account();
    let username = login.or_else(|| std::env::var("USER").ok()).unwrap_or_else(|| "user".to_string());
    let display_name = match display_name.trim() {
        "" => username.clone(),
        name => name.to_string(),
    };
    UserEntry { initial: initial_of(&display_name), username, display_name, uid }
}

fn login_account() -> (u32, Option<String>) {
    // SAFETY: `getuid` cannot fail. `getpwuid` returns a pointer into
    // libc-owned static storage (or null); it is dereferenced only after the
    // null checks and the name is copied out before any other libc call.
    unsafe {
        let uid = libc::getuid();
        let entry = libc::getpwuid(uid);
        if entry.is_null() || (*entry).pw_name.is_null() {
            return (uid, None);
        }
        let name = std::ffi::CStr::from_ptr((*entry).pw_name)
            .to_str()
            .ok()
            .filter(|name| !name.is_empty())
            .map(str::to_string);
        (uid, name)
    }
}

fn default_users() -> Vec<UserEntry> {
    let current_user = std::env::var("USER").unwrap_or_else(|_| "user".to_string());
    vec![UserEntry {
        initial: initial_of(&current_user),
        username: current_user.clone(),
        display_name: current_user,
        uid: UID_MIN,
    }]
}

#[must_use]
pub fn list_sessions() -> Vec<SessionEntry> {
    let dirs = ["/usr/local/share/wayland-sessions", "/usr/share/wayland-sessions"];

    let mut sessions: Vec<SessionEntry> = Vec::new();
    for dir in dirs {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "desktop")
                && let Ok(text) = fs::read_to_string(&path)
                && let Some(session) = parse_session(&path, &text)
                && session_is_installed(&text)
                && !sessions.iter().any(|s| s.id == session.id)
            {
                sessions.push(session);
            }
        }
    }

    if sessions.is_empty() {
        return default_sessions();
    }
    sort_sessions(&mut sessions);
    sessions
}

pub fn sort_sessions(sessions: &mut [SessionEntry]) {
    sessions.sort_by_cached_key(|s| (!s.id.starts_with("glide"), s.name.to_lowercase()));
}

fn session_is_installed(text: &str) -> bool {
    desktop_value(text, "TryExec").is_none_or(glide_config::on_path)
}

fn default_sessions() -> Vec<SessionEntry> {
    let glide_exec = ["/usr/local/bin/glide-session", "/usr/bin/glide-session"]
        .into_iter()
        .find(|path| Path::new(path).exists())
        .unwrap_or("glide-session");
    vec![SessionEntry {
        id: "glide".to_string(),
        name: "Glide".to_string(),
        exec: vec![glide_exec.to_string()],
        desktop_names: "Glide".to_string(),
    }]
}

#[must_use]
pub fn parse_session(path: &Path, text: &str) -> Option<SessionEntry> {
    let id = path.file_stem()?.to_string_lossy().to_string();
    let truthy = |key| desktop_value(text, key).is_some_and(|v| v.eq_ignore_ascii_case("true"));
    if truthy("Hidden") || truthy("NoDisplay") {
        return None;
    }
    let exec = split_exec(desktop_value(text, "Exec")?);
    if exec.is_empty() {
        return None;
    }
    let name = desktop_value(text, "Name").filter(|name| !name.is_empty()).map_or_else(|| id.clone(), str::to_string);
    let desktop_names = desktop_value(text, "DesktopNames").unwrap_or_default().trim_end_matches(';').replace(';', ":");
    Some(SessionEntry { id, name, exec, desktop_names })
}

#[must_use]
pub fn split_exec(exec: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_arg = false;
    let mut quoted = false;
    let mut chars = exec.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '"' => {
                quoted = !quoted;
                in_arg = true;
            }
            '\\' if quoted => {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            c if c.is_whitespace() && !quoted => {
                if in_arg {
                    args.push(std::mem::take(&mut current));
                    in_arg = false;
                }
            }
            c => {
                current.push(c);
                in_arg = true;
            }
        }
    }
    if in_arg {
        args.push(current);
    }
    args.into_iter()
        .filter(|arg| arg == "%%" || !(arg.len() == 2 && arg.starts_with('%')))
        .map(|arg| arg.replace("%%", "%"))
        .collect()
}

#[must_use]
pub fn parse_wireless(text: &str) -> Option<u8> {
    text.lines().skip(2).find_map(|line| {
        let (_, rest) = line.split_once(':')?;
        let link: f32 = rest.split_whitespace().nth(1)?.trim_end_matches('.').parse().ok()?;
        Some((link / 70.0 * 100.0).clamp(0.0, 100.0).round() as u8)
    })
}

#[must_use]
pub fn read_status() -> GreeterStatus {
    let battery = glide_shell::battery::read_battery().map(|info| {
        let charging = matches!(
            info.state,
            glide_shell::battery::BatteryState::Charging | glide_shell::battery::BatteryState::Full
        );
        (info.pct, charging)
    });
    let wifi = fs::read_to_string("/proc/net/wireless").ok().as_deref().and_then(parse_wireless);
    GreeterStatus { battery, wifi }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(id: &str, name: &str) -> SessionEntry {
        SessionEntry { id: id.into(), name: name.into(), exec: vec![id.into()], desktop_names: String::new() }
    }

    #[test]
    fn default_sessions_contain_glide() {
        assert!(default_sessions().iter().any(|s| s.id == "glide"));
    }

    #[test]
    fn passwd_keeps_only_login_users_in_the_regular_uid_range() {
        let passwd = "root:x:0:0:root:/root:/bin/bash\n\
            greeter:x:964:964::/var/lib/greetd:/bin/false\n\
            olena:x:1000:1000:Олена Коваль,,,:/home/olena:/bin/zsh\n\
            svc:x:1001:1001::/:/usr/bin/nologin\n\
            nobody:x:65534:65534:Nobody:/:/usr/bin/nologin\n\
            dyn:x:61184:61184::/:/bin/sh\n\
            bob:x:1002:1002::/home/bob:/bin/bash\n";
        let users = parse_passwd(passwd);
        let names: Vec<&str> = users.iter().map(|u| u.username.as_str()).collect();
        assert_eq!(names, ["olena", "bob"]);
        assert_eq!(users[0].display_name, "Олена Коваль");
        assert_eq!(users[0].initial, 'О');
        assert_eq!(users[1].display_name, "bob");
        assert_eq!(users[1].initial, 'B');
    }

    #[test]
    fn one_latin1_gecos_byte_does_not_hide_every_other_account() {
        let mut passwd = b"root:x:0:0:root:/root:/bin/bash\n".to_vec();
        passwd.extend_from_slice(b"jose:x:1000:1000:Jos\xe9,,,:/home/jose:/bin/zsh\n");
        passwd.extend_from_slice(b"bob:x:1002:1002::/home/bob:/bin/bash\n");
        assert!(String::from_utf8(passwd.clone()).is_err(), "the fixture is not valid UTF-8");
        let users = parse_passwd_bytes(&passwd);
        let names: Vec<&str> = users.iter().map(|u| u.username.as_str()).collect();
        assert_eq!(names, ["jose", "bob"]);
        assert!(users[0].display_name.starts_with("Jos"));
        assert_eq!(users[0].uid, 1000);
        assert_eq!(users[1].display_name, "bob");
    }

    #[test]
    fn glide_sorts_first_and_the_rest_alphabetically_without_breaking_total_order() {
        let mut sessions = vec![
            session("sway", "Sway"),
            session("glide", "Glide"),
            session("gnome", "GNOME"),
            session("glide-debug", "Glide (debug)"),
            session("hyprland", "Hyprland"),
        ];
        sort_sessions(&mut sessions);
        let ids: Vec<&str> = sessions.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["glide", "glide-debug", "gnome", "hyprland", "sway"]);
    }

    #[test]
    fn session_files_honour_quotes_field_codes_and_desktop_names() {
        let text = "[Desktop Entry]\nName=Glide\nName[uk]=Глайд\n\
            Exec=\"/opt/my glide/glide-session\" --flag %U\nDesktopNames=Glide;GNOME;\n\
            [Desktop Action x]\nName=Other\n";
        let parsed = parse_session(Path::new("/x/glide.desktop"), text).unwrap();
        assert_eq!(parsed.name, "Glide");
        assert_eq!(parsed.exec, ["/opt/my glide/glide-session", "--flag"]);
        assert_eq!(parsed.desktop_names, "Glide:GNOME");
        assert!(parsed.env().contains(&"XDG_CURRENT_DESKTOP=Glide:GNOME".to_string()));
        assert!(parsed.env().contains(&"XDG_SESSION_TYPE=wayland".to_string()));
        assert_eq!(split_exec("run %% --keep=50%% %f"), ["run", "%", "--keep=50%"], "%% is a literal percent sign");
    }

    #[test]
    fn only_glide_sessions_are_asked_to_hold_the_handoff_cover() {
        let flag = format!("{}=1", crate::handoff::ENV);
        assert!(session("glide", "Glide").env().contains(&flag));
        assert!(!session("sway", "Sway").env().contains(&flag));
    }

    #[test]
    fn hidden_or_exec_less_sessions_are_skipped() {
        let hidden = "[Desktop Entry]\nName=X\nExec=x\nHidden=true\n";
        assert!(parse_session(Path::new("/x/x.desktop"), hidden).is_none());
        let no_exec = "[Desktop Entry]\nName=X\n";
        assert!(parse_session(Path::new("/x/x.desktop"), no_exec).is_none());
    }

    #[test]
    fn wireless_link_quality_maps_to_percent() {
        let text = "Inter-| sta-|   Quality        |   Discarded packets\n \
            face | tus | link level noise |  nwid  crypt   frag  retry   misc | beacon | 22\n \
            wlan0: 0000   54.  -56.  -256        0      0      0      0     29        0\n";
        assert_eq!(parse_wireless(text), Some(77));
        assert_eq!(parse_wireless("h1\nh2\n"), None);
    }
}

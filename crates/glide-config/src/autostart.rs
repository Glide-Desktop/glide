use std::path::Path;

const ALWAYS_PROVIDED: [&str; 4] = ["nm-applet", "nm-tray", "blueman-applet", "netbird-ui"];

const POLKIT_AGENTS: [&str; 8] = [
    "polkit-gnome-authentication-agent-1",
    "polkit-kde-authentication-agent-1",
    "polkit-mate-authentication-agent-1",
    "lxqt-policykit-agent",
    "lxpolkit",
    "xfce-polkit",
    "soteria",
    "hyprpolkitagent",
];

const PHONE_DAEMONS: [&str; 2] = ["kdeconnectd", "kdeconnect-indicator"];

const PHONE_DAEMON: &str = "glide-phone";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Provided {
    pub polkit_agent: bool,
    pub phone: bool,
}

impl Provided {
    pub const ALL: Self = Self { polkit_agent: true, phone: true };

    #[must_use]
    pub fn detect(polkit_agent: bool) -> Self {
        Self { polkit_agent, phone: crate::on_path(PHONE_DAEMON) }
    }
}

#[must_use]
pub fn program(exec: &str) -> &str {
    let mut words = exec.split_whitespace().map(|word| word.trim_matches(['"', '\''])).peekable();
    if words.next_if(|word| Path::new(word).file_name().is_some_and(|name| name == "env")).is_some() {
        while words.next_if(|word| word.split_once('=').is_some_and(|(key, _)| !key.is_empty())).is_some() {}
    }
    let word = words.next().unwrap_or_default();
    Path::new(word).file_name().and_then(|name| name.to_str()).unwrap_or(word)
}

#[must_use]
pub fn is_phone_daemon(exec: &str) -> bool {
    program(exec) == PHONE_DAEMON
}

#[must_use]
pub fn provided_by_shell(exec: &str, provided: Provided) -> bool {
    let name = program(exec);
    let listed = |programs: &[&str]| programs.iter().any(|program| program.eq_ignore_ascii_case(name));
    listed(&ALWAYS_PROVIDED)
        || (provided.polkit_agent && listed(&POLKIT_AGENTS))
        || (provided.phone && listed(&PHONE_DAEMONS))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn network_bluetooth_and_netbird_applets_are_provided_by_the_shell_however_they_are_spelled() {
        for exec in [
            "nm-applet",
            "nm-applet --indicator %U",
            "/usr/bin/nm-applet",
            "nm-tray",
            "blueman-applet",
            "Blueman-Applet",
            "/usr/bin/netbird-ui",
            "netbird-ui --daemon-addr unix:///var/run/netbird.sock",
            "\"/usr/bin/nm-applet\"",
            "'nm-applet' --indicator",
            "env FOO=1 nm-applet",
            "/usr/bin/env FOO=1 \"BAR=a\" blueman-applet",
            "/usr/lib/polkit-gnome/polkit-gnome-authentication-agent-1",
            "/usr/lib/polkit-kde-authentication-agent-1",
            "lxqt-policykit-agent",
            "lxpolkit",
            "kdeconnectd",
        ] {
            assert!(provided_by_shell(exec, Provided::ALL), "{exec}");
        }
    }

    #[test]
    fn other_programs_and_arguments_naming_an_applet_are_not() {
        for exec in [
            "",
            "netbird",
            "nm-connection-editor",
            "pkill nm-applet",
            "nm-applet2",
            "env FOO=1 pkill nm-applet",
            "env",
            "\"nm-applet2\"",
        ] {
            assert!(!provided_by_shell(exec, Provided::ALL), "{exec}");
        }
    }

    #[test]
    fn with_the_shell_polkit_agent_off_system_agents_start_so_the_session_keeps_one() {
        let without_agent = Provided { polkit_agent: false, phone: true };
        for exec in ["/usr/lib/polkit-gnome/polkit-gnome-authentication-agent-1", "lxpolkit", "soteria"] {
            assert!(!provided_by_shell(exec, without_agent), "{exec}");
        }
        assert!(provided_by_shell("nm-applet", without_agent), "the applets stay replaced");
    }

    #[test]
    fn without_glide_phone_kde_connect_keeps_running() {
        let without_phone = Provided { polkit_agent: true, phone: false };
        assert!(!provided_by_shell("/usr/bin/kdeconnectd", without_phone));
        assert!(!provided_by_shell("kdeconnect-indicator", without_phone));
        assert!(provided_by_shell("lxpolkit", without_phone));
    }

    #[test]
    fn the_phone_daemon_is_recognised_by_its_program() {
        assert!(is_phone_daemon("glide-phone --daemon"));
        assert!(is_phone_daemon("/usr/bin/glide-phone --daemon"));
        assert!(!is_phone_daemon("glide-phone-helper"));
        assert_eq!(program("env A=1 /usr/bin/foot -e htop"), "foot");
    }
}

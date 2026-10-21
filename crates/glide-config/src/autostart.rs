use std::path::Path;

const PROVIDED_BY_SHELL: [&str; 3] = ["nm-applet", "blueman-applet", "netbird-ui"];

#[must_use]
pub fn provided_by_shell(exec: &str) -> bool {
    let mut words = exec.split_whitespace().map(|word| word.trim_matches(['"', '\''])).peekable();
    if words.next_if(|word| Path::new(word).file_name().is_some_and(|name| name == "env")).is_some() {
        while words.next_if(|word| word.split_once('=').is_some_and(|(key, _)| !key.is_empty())).is_some() {}
    }
    let word = words.next().unwrap_or_default();
    let name = Path::new(word).file_name().and_then(|name| name.to_str()).unwrap_or(word);
    PROVIDED_BY_SHELL.iter().any(|program| program.eq_ignore_ascii_case(name))
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
            "blueman-applet",
            "Blueman-Applet",
            "/usr/bin/netbird-ui",
            "netbird-ui --daemon-addr unix:///var/run/netbird.sock",
            "\"/usr/bin/nm-applet\"",
            "'nm-applet' --indicator",
            "env FOO=1 nm-applet",
            "/usr/bin/env FOO=1 \"BAR=a\" blueman-applet",
        ] {
            assert!(provided_by_shell(exec), "{exec}");
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
            assert!(!provided_by_shell(exec), "{exec}");
        }
    }
}

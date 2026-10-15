use std::path::Path;

const PROVIDED_BY_SHELL: [&str; 2] = ["nm-applet", "blueman-applet"];

#[must_use]
pub fn provided_by_shell(exec: &str) -> bool {
    let word = exec.split_whitespace().next().unwrap_or_default();
    let name = Path::new(word).file_name().and_then(|name| name.to_str()).unwrap_or(word);
    PROVIDED_BY_SHELL.iter().any(|program| program.eq_ignore_ascii_case(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn network_and_bluetooth_applets_are_provided_by_the_shell_however_they_are_spelled() {
        for exec in ["nm-applet", "nm-applet --indicator %U", "/usr/bin/nm-applet", "blueman-applet", "Blueman-Applet"]
        {
            assert!(provided_by_shell(exec), "{exec}");
        }
    }

    #[test]
    fn other_programs_and_arguments_naming_an_applet_are_not() {
        for exec in ["", "netbird", "nm-connection-editor", "pkill nm-applet", "nm-applet2"] {
            assert!(!provided_by_shell(exec), "{exec}");
        }
    }
}

use std::{
    path::Path,
    sync::{PoisonError, RwLock},
};

use crate::on_path;

const KNOWN: [(&str, &[&str]); 23] = [
    ("glide-terminal", &["-e"]),
    ("xdg-terminal-exec", &[]),
    ("ptyxis", &["--"]),
    ("kgx", &["-e"]),
    ("gnome-terminal", &["--"]),
    ("konsole", &["-e"]),
    ("alacritty", &["-e"]),
    ("foot", &[]),
    ("kitty", &[]),
    ("wezterm", &["start", "--"]),
    ("ghostty", &["-e"]),
    ("xfce4-terminal", &["-x"]),
    ("mate-terminal", &["-x"]),
    ("terminator", &["-x"]),
    ("tilix", &["-x"]),
    ("sakura", &["-x"]),
    ("deepin-terminal", &["-x"]),
    ("lxterminal", &["-e"]),
    ("qterminal", &["-e"]),
    ("urxvt", &["-e"]),
    ("st", &["-e"]),
    ("x-terminal-emulator", &["-e"]),
    ("xterm", &["-e"]),
];
const DISPATCHERS: [&str; 2] = ["xdg-terminal-exec", "x-terminal-emulator"];
const FALLBACK: &str = "glide-terminal";
const FALLBACK_ARGS: &[&str] = &["-e"];

static CONFIGURED: RwLock<Option<String>> = RwLock::new(None);

fn usable(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

pub fn set_configured(terminal: Option<&str>) {
    *CONFIGURED.write().unwrap_or_else(PoisonError::into_inner) = usable(terminal).map(str::to_string);
}

#[must_use]
pub fn configured() -> Option<String> {
    CONFIGURED.read().unwrap_or_else(PoisonError::into_inner).clone()
}

fn command_word(terminal: &str) -> &str {
    terminal.split_whitespace().next().unwrap_or_default()
}

fn program(terminal: &str) -> &str {
    let word = command_word(terminal);
    Path::new(word).file_name().and_then(|name| name.to_str()).unwrap_or(word)
}

#[must_use]
pub fn program_name(terminal: &str) -> &str {
    program(terminal)
}

#[must_use]
pub fn known_installed(installed: impl Fn(&str) -> bool) -> Vec<&'static str> {
    KNOWN.iter().map(|(name, _)| *name).filter(|name| !DISPATCHERS.contains(name) && installed(name)).collect()
}

#[must_use]
pub fn installed() -> Vec<&'static str> {
    known_installed(on_path)
}

#[must_use]
pub fn exec_args(terminal: &str) -> &'static [&'static str] {
    let name = program(terminal);
    KNOWN.iter().find(|(known, _)| *known == name).map_or(FALLBACK_ARGS, |(_, args)| args)
}

#[must_use]
pub fn find(configured: Option<&str>, environment: Option<&str>, installed: impl Fn(&str) -> bool) -> Option<String> {
    let named = usable(configured).or_else(|| usable(environment).filter(|terminal| installed(command_word(terminal))));
    named.or_else(|| KNOWN.iter().map(|(name, _)| *name).find(|name| installed(name))).map(str::to_string)
}

#[must_use]
pub fn pick(configured: Option<&str>, environment: Option<&str>, installed: impl Fn(&str) -> bool) -> String {
    find(configured, environment, installed).unwrap_or_else(|| FALLBACK.to_string())
}

#[must_use]
pub fn current() -> String {
    let environment = std::env::var("TERMINAL").ok();
    pick(configured().as_deref(), environment.as_deref(), on_path)
}

#[must_use]
pub fn wrap(terminal: &str, command: &str) -> String {
    std::iter::once(terminal).chain(exec_args(terminal).iter().copied()).chain([command]).collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_configured_terminal_wins_then_the_environment_then_the_first_installed() {
        let installed = |name: &str| ["foot", "kitty", "xterm", "/usr/bin/kitty"].contains(&name);
        assert_eq!(pick(Some("wezterm"), Some("foot"), installed), "wezterm");
        assert_eq!(pick(Some("  "), Some("kitty"), installed), "kitty");
        assert_eq!(pick(None, Some("missing-term"), installed), "foot", "a stale TERMINAL is ignored");
        assert_eq!(pick(None, Some("/usr/bin/kitty --single-instance"), installed), "/usr/bin/kitty --single-instance");
        assert_eq!(pick(None, None, installed), "foot");
        assert_eq!(pick(None, None, |name| name == "xdg-terminal-exec"), "xdg-terminal-exec");
    }

    #[test]
    fn xterm_is_picked_only_when_installed_and_glides_own_terminal_is_the_last_resort() {
        assert_eq!(pick(None, None, |name| name == "xterm"), "xterm");
        assert_eq!(find(None, None, |_| false), None);
        assert_eq!(pick(None, None, |_| false), "glide-terminal", "Glide ships its own terminal");
    }

    #[test]
    fn debians_alternative_comes_after_every_named_terminal_but_before_xterm() {
        let installed = |name: &str| ["x-terminal-emulator", "xterm"].contains(&name);
        assert_eq!(pick(None, None, installed), "x-terminal-emulator");
        assert_eq!(pick(None, None, |name| ["x-terminal-emulator", "tilix"].contains(&name)), "tilix");
        assert_eq!(known_installed(installed), ["xterm"], "a dispatcher is not offered as a choice");
    }

    #[test]
    fn a_terminal_named_by_path_is_checked_at_that_path() {
        let installed = |name: &str| ["/opt/x/kitty", "foot"].contains(&name);
        assert_eq!(pick(None, Some("/opt/x/kitty"), installed), "/opt/x/kitty");
        assert_eq!(pick(None, Some("/opt/y/kitty"), installed), "foot", "same name elsewhere is not enough");
        assert!(!on_path("/nonexistent/glide-terminal-test"));
        assert!(!on_path("./nonexistent-glide-terminal-test"));
        assert!(on_path("/bin/sh") || on_path("/usr/bin/sh"));
    }

    #[test]
    fn installed_terminals_keep_the_preference_order_and_skip_the_dispatcher() {
        let installed = |name: &str| ["foot", "xdg-terminal-exec", "glide-terminal"].contains(&name);
        assert_eq!(known_installed(installed), ["glide-terminal", "foot"]);
        assert_eq!(program_name("/usr/bin/kitty --single-instance"), "kitty");
    }

    #[test]
    fn each_terminal_gets_its_own_way_of_running_a_command() {
        assert_eq!(wrap("alacritty", "htop"), "alacritty -e htop");
        assert_eq!(wrap("gnome-terminal", "htop"), "gnome-terminal -- htop");
        assert_eq!(wrap("foot", "vim notes.txt"), "foot vim notes.txt");
        assert_eq!(wrap("wezterm", "htop"), "wezterm start -- htop");
        assert_eq!(wrap("xdg-terminal-exec", "htop"), "xdg-terminal-exec htop");
        assert_eq!(wrap("/usr/bin/kitty --single-instance", "htop"), "/usr/bin/kitty --single-instance htop");
        assert_eq!(wrap("some-new-terminal", "htop"), "some-new-terminal -e htop", "-e is the common convention");
        for terminal in ["mate-terminal", "terminator", "tilix", "sakura", "deepin-terminal"] {
            assert_eq!(wrap(terminal, "htop"), format!("{terminal} -x htop"));
        }
        for terminal in ["lxterminal", "qterminal", "urxvt", "st", "x-terminal-emulator"] {
            assert_eq!(wrap(terminal, "htop"), format!("{terminal} -e htop"));
        }
    }
}

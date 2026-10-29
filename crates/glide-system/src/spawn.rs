use std::process::{Command, Output};
use std::time::Duration;

use crate::{cursor_config, theme_export};

static DETACHED_CHILDREN: std::sync::Mutex<Vec<std::process::Child>> = std::sync::Mutex::new(Vec::new());

pub fn spawn_detached(mut command: Command) -> std::io::Result<u32> {
    reap_child_processes();
    if let Some(terminal) = glide_config::terminal::configured() {
        command.env("TERMINAL", terminal);
    }
    if let Some(theme) = cursor_config::configured_theme() {
        command.env("XCURSOR_THEME", theme);
    }
    if let Some(size) = cursor_config::configured_size() {
        command.env("XCURSOR_SIZE", size.to_string());
    }
    if std::env::var_os("QT_QPA_PLATFORMTHEME").is_none()
        && let Some(theme) = theme_export::qt_platform_theme()
    {
        command.env("QT_QPA_PLATFORMTHEME", theme);
    }
    if let Some(style) = theme_export::qt_style_override() {
        command.env("QT_STYLE_OVERRIDE", style);
    }
    let child = command.spawn()?;
    let pid = child.id();
    adopt(child);
    Ok(pid)
}

pub fn adopt(child: std::process::Child) {
    if let Ok(mut children) = DETACHED_CHILDREN.lock() {
        children.push(child);
    }
}

pub fn spawn_shell_command(command: &str) -> std::io::Result<u32> {
    spawn_shell_command_with_env(command, &[])
}

pub fn spawn_shell_command_with_env(command: &str, env: &[(&str, &str)]) -> std::io::Result<u32> {
    let mut shell = Command::new("sh");
    shell.arg("-c").arg(command);
    for (key, value) in env {
        shell.env(key, value);
    }
    spawn_detached(shell)
}

pub fn reap_child_processes() {
    if let Ok(mut children) = DETACHED_CHILDREN.lock() {
        children.retain_mut(|child| matches!(child.try_wait(), Ok(None)));
    }
}

#[must_use]
pub fn run_with_timeout(program: &str, args: &[&str], timeout: Duration) -> Option<Output> {
    run_with_input(program, args, None, timeout)
}

#[must_use]
pub fn run_with_input(program: &str, args: &[&str], input: Option<&str>, timeout: Duration) -> Option<Output> {
    let line = input.map(|text| zeroize::Zeroizing::new(format!("{text}\n")));
    let mut command = Command::new(program);
    command.args(args).env("LC_ALL", "C");
    glide_config::process::run(&mut command, line.as_deref().map(String::as_bytes), timeout).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    static CHILD_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn detached_child_count() -> usize {
        DETACHED_CHILDREN.lock().map_or(0, |children| children.len())
    }

    #[test]
    fn the_reaper_never_steals_a_child_another_thread_is_waiting_for() {
        let _guard = CHILD_TEST_LOCK.lock().unwrap();
        let output = std::thread::spawn(|| Command::new("true").output());
        for _ in 0..200 {
            reap_child_processes();
        }
        let status =
            output.join().expect("thread must not panic").expect("output() must see its own exit status").status;
        assert!(status.success());
    }

    #[test]
    fn finished_detached_children_are_reaped() {
        let _guard = CHILD_TEST_LOCK.lock().unwrap();
        reap_child_processes();
        let before = detached_child_count();
        spawn_shell_command("true").expect("sh must exist");
        assert_eq!(detached_child_count(), before + 1);
        std::thread::sleep(Duration::from_millis(100));
        reap_child_processes();
        assert_eq!(detached_child_count(), before);
    }

    #[test]
    fn spawn_shell_command_runs_through_a_shell() {
        let _guard = CHILD_TEST_LOCK.lock().unwrap();
        let marker = std::env::temp_dir().join(format!("glide-sh-test-{}", std::process::id()));
        let _ = std::fs::remove_file(&marker);
        spawn_shell_command(&format!("true; touch {}", marker.display())).expect("sh must exist");
        std::thread::sleep(Duration::from_millis(150));
        reap_child_processes();
        assert!(marker.exists(), "the shell did not run both commands");
        let _ = std::fs::remove_file(&marker);
    }

    #[test]
    fn a_launch_hands_the_activation_token_to_the_child() {
        let _guard = CHILD_TEST_LOCK.lock().unwrap();
        let marker = std::env::temp_dir().join(format!("glide-env-test-{}", std::process::id()));
        let _ = std::fs::remove_file(&marker);
        spawn_shell_command_with_env(
            &format!("printf '%s %s' \"$XDG_ACTIVATION_TOKEN\" \"$DESKTOP_STARTUP_ID\" > {}", marker.display()),
            &[("XDG_ACTIVATION_TOKEN", "tok"), ("DESKTOP_STARTUP_ID", "tok")],
        )
        .expect("sh must exist");
        std::thread::sleep(Duration::from_millis(150));
        reap_child_processes();
        assert_eq!(std::fs::read_to_string(&marker).unwrap_or_default(), "tok tok");
        let _ = std::fs::remove_file(&marker);
    }
}

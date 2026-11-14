use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use glide_files::fs::run::{Kind, allow, argv, classify, held, probe, run, shebang};

fn scratch(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let dir = std::env::temp_dir().join(format!("glide-run-{tag}-{nanos}"));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn put(path: &Path, text: &str, mode: u32) {
    std::fs::write(path, text).expect("write");
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).expect("chmod");
}

fn words(list: &[OsString]) -> Vec<String> {
    list.iter().map(|word| word.to_string_lossy().into_owned()).collect()
}

fn appears(path: &Path) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if path.exists() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

#[test]
fn programs_scripts_and_launchers_are_told_apart_by_content_then_name() {
    let elf = classify("tool", b"\x7fELF\x02\x01", 0o755).expect("program");
    assert_eq!((elf.kind, elf.allowed, elf.needs_permission()), (Kind::Program, true, false));
    let download = classify("Game.AppImage", b"\x7fELF\x02\x01", 0o644).expect("program");
    assert!(download.needs_permission(), "a downloaded binary must be allowed first");

    let script = classify("backup", b"#!/usr/bin/env python3\nprint()", 0o644).expect("script");
    assert_eq!((script.kind, script.needs_permission()), (Kind::Script, false));
    assert_eq!(shebang(b"#! /bin/sh -e\nexit"), Some(vec![String::from("/bin/sh"), String::from("-e")]));
    assert_eq!(shebang(b"echo hi"), None);

    let bare = classify("deploy.sh", b"echo hi\n", 0o644).expect("script by its extension");
    assert_eq!(words(&argv(Path::new("/x/deploy.sh"), &bare)), ["sh", "/x/deploy.sh"]);
    let launcher = classify("App.desktop", b"[Desktop Entry]\n", 0o644).expect("launcher");
    assert_eq!((launcher.kind, launcher.runs_in_terminal()), (Kind::Launcher, false));

    assert_eq!(classify("photo.jpg", b"\xff\xd8\xff", 0o755), None, "an exec bit alone runs nothing");
    assert_eq!(classify("notes.txt", b"hello", 0o644), None);
}

#[test]
fn a_script_runs_through_its_interpreter_until_it_is_executable() {
    let root = scratch("argv");
    let script = root.join("job");
    put(&script, "#!/bin/sh\nexit 0\n", 0o644);
    let found = probe(&script).expect("script");
    assert_eq!(words(&argv(&script, &found)), ["/bin/sh", script.to_string_lossy().as_ref()]);
    allow(&script).expect("chmod");
    let mode = std::fs::metadata(&script).expect("meta").permissions().mode() & 0o777;
    assert_eq!(mode, 0o755, "everyone who can read it may run it");
    let found = probe(&script).expect("script");
    assert_eq!(words(&argv(&script, &found)), [script.to_string_lossy().as_ref()]);
    assert_eq!(probe(&root), None, "a folder is not a program");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn running_starts_the_file_in_its_own_folder() {
    let root = scratch("spawn");
    let script = root.join("make mark.sh");
    put(&script, "touch \"ran here\"\n", 0o644);
    let found = probe(&script).expect("script");
    run(&script, &found, false, "").expect("started");
    assert!(appears(&root.join("ran here")), "a script without a shebang or an exec bit still runs");

    let tool = root.join("tool");
    put(&tool, "#!/bin/sh\ntouch direct\n", 0o755);
    run(&tool, &probe(&tool).expect("script"), false, "").expect("started");
    assert!(appears(&root.join("direct")));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_launcher_runs_its_exec_line() {
    let root = scratch("launcher");
    let mark = root.join("launched");
    let entry = root.join("Mark.desktop");
    let text = format!("[Desktop Entry]\nType=Application\nName=Mark\nExec=touch \"{}\"\n", mark.display());
    put(&entry, &text, 0o755);
    let found = probe(&entry).expect("launcher");
    assert_eq!((found.kind, found.needs_permission()), (Kind::Launcher, false));
    run(&entry, &found, false, "").expect("started");
    assert!(appears(&mark));
    put(&entry, &text, 0o644);
    assert!(probe(&entry).expect("launcher").needs_permission(), "an unmarked launcher asks first");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_terminal_run_waits_for_enter_and_reports_the_exit_code() {
    let root = scratch("hold");
    let out = root.join("out");
    let script = root.join("fail.sh");
    put(&script, "echo working\nexit 3\n", 0o644);
    let found = probe(&script).expect("script");
    let wrapped = held(&argv(&script, &found), "ended with");
    assert_eq!(&words(&wrapped)[..2], ["sh", "-c"]);
    let (program, args) = wrapped.split_first().expect("argv");
    let output =
        std::process::Command::new(program).args(args).stdin(std::process::Stdio::null()).output().expect("ran");
    std::fs::write(&out, &output.stdout).expect("out");
    let shown = String::from_utf8_lossy(&output.stdout);
    assert!(shown.contains("working") && shown.contains("ended with 3"), "{shown}");
    let _ = std::fs::remove_dir_all(root);
}

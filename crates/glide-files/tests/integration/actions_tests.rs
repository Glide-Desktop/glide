use std::ffi::OsString;
use std::path::{Path, PathBuf};

use glide_files::fs::mime::{MimeDb, glob_match};
use glide_files::fs::{Codes, Extensions, Offer, Roots, Run, Target};
use glide_files::i18n::{Key, Lang};

struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("glide-files-actions-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        Self(dir)
    }

    fn write(&self, name: &str, content: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().expect("file has a parent")).expect("mkdir");
        std::fs::write(&path, content).expect("write file");
        path
    }

    fn roots(&self) -> Roots {
        Roots { data: vec![self.0.join("share")], taps: vec![self.0.join("taps")] }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const GLOBS: &str = "\
# comment
50:application/zip:*.zip
50:application/x-compressed-tar:*.tar.gz
50:application/gzip:*.gz
50:image/png:*.png
50:text/x-csrc:*.c
50:text/x-makefile:makefile
50:text/x-readme:README*
50:text/x-csrc-upper:*.C:cs
";

fn scratch_with_mime(name: &str) -> Scratch {
    let scratch = Scratch::new(name);
    scratch.write("share/mime/globs2", GLOBS);
    scratch.write("share/mime/subclasses", "application/x-compressed-tar application/gzip\ntext/x-csrc text/plain\n");
    scratch.write("share/mime/aliases", "application/x-zip application/zip\n");
    scratch
}

fn target(extensions: &Extensions, path: &str, is_dir: bool) -> Target {
    extensions.target(Path::new(path), is_dir)
}

fn labels(offers: &[Offer]) -> Vec<&str> {
    offers.iter().map(|offer| offer.label.as_str()).collect()
}

#[test]
fn mime_guess_prefers_literals_then_longest_suffix() {
    let mut db = MimeDb::default();
    db.add_globs(GLOBS);
    assert_eq!(db.guess("Makefile"), Some("text/x-makefile"));
    assert_eq!(db.guess("a.tar.gz"), Some("application/x-compressed-tar"));
    assert_eq!(db.guess("a.GZ"), Some("application/gzip"));
    assert_eq!(db.guess("x.C"), Some("text/x-csrc-upper"));
    assert_eq!(db.guess("x.c"), Some("text/x-csrc"));
    assert_eq!(db.guess("README.old"), Some("text/x-readme"));
    assert_eq!(db.guess("notes"), None);
}

#[test]
fn mime_lineage_covers_subclasses_aliases_and_wildcards() {
    let scratch = scratch_with_mime("lineage");
    let db = MimeDb::load(&scratch.roots().data);
    assert!(db.is_a("application/x-compressed-tar", "application/gzip"));
    assert!(db.is_a("application/x-compressed-tar", "application/octet-stream"));
    assert!(db.is_a("application/zip", "application/x-zip"));
    assert!(db.is_a("text/x-csrc", "text/plain"));
    assert!(db.is_a("image/png", "image/*"));
    assert!(db.is_a("inode/directory", "all/all"));
    assert!(!db.is_a("inode/directory", "all/allfiles"));
    assert!(!db.is_a("inode/directory", "application/octet-stream"));
    assert!(!db.is_a("image/png", "application/zip"));
}

#[test]
fn glob_match_handles_stars_and_question_marks() {
    assert!(glob_match("*.tar.*", "a.tar.gz"));
    assert!(glob_match("photo-??.png", "photo-01.png"));
    assert!(!glob_match("photo-??.png", "photo-1.png"));
    assert!(glob_match("*", ""));
    assert!(!glob_match("*.zip", "zip"));
}

#[test]
fn service_menu_offers_match_mime_count_and_submenu() {
    let scratch = scratch_with_mime("servicemenu");
    scratch.write(
        "share/kio/servicemenus/images.desktop",
        "[Desktop Entry]\n\
         Type=Service\n\
         MimeType=image/*;\n\
         X-KDE-MaxNumberOfUrls=2\n\
         X-KDE-Submenu=Convert\n\
         X-KDE-Submenu[uk]=Перетворити\n\
         Actions=jpg;_SEPARATOR_;missing\n\
         \n\
         [Desktop Action jpg]\n\
         Name=To JPEG\n\
         Name[uk]=У JPEG\n\
         Exec=convert %f %F\n",
    );
    scratch.write(
        "share/kservices5/ServiceMenus/remote.desktop",
        "[Desktop Entry]\nMimeType=all/all;\nX-KDE-Protocols=sftp\nActions=a\n\
         [Desktop Action a]\nName=Remote\nExec=true\n",
    );
    let extensions = Extensions::load(&scratch.roots(), Lang::Uk);
    let png = target(&extensions, "/p/a.png", false);
    let offers = extensions.offers(std::slice::from_ref(&png));
    assert_eq!(labels(&offers), ["У JPEG"]);
    assert_eq!(offers[0].group.as_deref(), Some("Перетворити"));

    let three = vec![png.clone(), png.clone(), png];
    assert!(extensions.offers(&three).is_empty());
    let zip = target(&extensions, "/p/a.zip", false);
    assert!(extensions.offers(&[zip]).is_empty());
}

#[test]
fn file_manager_actions_use_profiles_and_selection_count() {
    let scratch = scratch_with_mime("fma");
    scratch.write(
        "share/file-manager/actions/sum.desktop",
        "[Desktop Entry]\n\
         Type=Action\n\
         Name=Checksum\n\
         Profiles=files;\n\
         \n\
         [X-Action-Profile files]\n\
         MimeTypes=all/allfiles;!image/*;\n\
         Basenames=*.zip;*.C\n\
         Matchcase=false\n\
         SelectionCount==1\n\
         Exec=sha256sum %f\n",
    );
    scratch.write(
        "share/file-manager/actions/off.desktop",
        "[Desktop Entry]\nName=Off\nEnabled=false\nProfiles=p\n\
         [X-Action-Profile p]\nExec=true\n",
    );
    let extensions = Extensions::load(&scratch.roots(), Lang::En);
    let zip = target(&extensions, "/p/a.ZIP", false);
    assert_eq!(labels(&extensions.offers(std::slice::from_ref(&zip))), ["Checksum"]);
    assert!(extensions.offers(&[target(&extensions, "/p/a.png", false)]).is_empty());
    assert!(extensions.offers(&[target(&extensions, "/p/dir.zip", true)]).is_empty());
    assert!(extensions.offers(&[zip.clone(), zip]).is_empty());
}

#[test]
fn thunar_archive_plugin_scripts_become_compress_and_extract() {
    let scratch = scratch_with_mime("tap");
    let script = scratch.write("taps/peazip.tap", "#!/bin/sh\n");
    scratch.write(
        "share/applications/peazip.desktop",
        "[Desktop Entry]\nName=PeaZip\nExec=peazip %F\nMimeType=application/zip;application/gzip;\n",
    );
    scratch.write("taps/orphan.tap", "#!/bin/sh\n");
    let extensions = Extensions::load(&scratch.roots(), Lang::En);

    let archive = target(&extensions, "/p/a.tar.gz", false);
    let offers = extensions.offers(std::slice::from_ref(&archive));
    assert_eq!(labels(&offers), [Key::Compress.en(), Key::ExtractHere.en(), Key::ExtractTo.en()]);
    assert_eq!(offers[0].group, None);

    let folder = target(&extensions, "/p/docs", true);
    assert_eq!(labels(&extensions.offers(&[folder, archive.clone()])), [Key::Compress.en()]);

    let commands = offers[1].run.commands(&[archive], Path::new("/p")).expect("tap command");
    let expected: Vec<OsString> =
        vec!["sh".into(), script.into(), "extract-here".into(), "/p".into(), "/p/a.tar.gz".into()];
    assert_eq!(commands, [expected]);
}

fn exec_with(line: &str, targets: &[Target], codes: Codes) -> Vec<Vec<String>> {
    Run::Exec { line: line.to_string(), desktop: PathBuf::from("/d/menu.desktop"), codes }
        .commands(targets, Path::new("/"))
        .expect("valid Exec")
        .into_iter()
        .map(|argv| argv.into_iter().map(|arg| arg.to_string_lossy().into_owned()).collect())
        .collect()
}

fn exec(line: &str, targets: &[Target]) -> Vec<Vec<String>> {
    exec_with(line, targets, Codes::Action)
}

#[test]
fn a_terminal_app_opens_its_files_inside_the_default_terminal() {
    use glide_files::fs::actions::exec_line;
    let extensions = Extensions::default();
    let files = [target(&extensions, "/a/notes.txt", false)];
    let line = exec_line("vim %F", true, &|| String::from("gnome-terminal"));
    assert_eq!(exec(&line, &files), [["gnome-terminal", "--", "vim", "/a/notes.txt"]]);
    let line = exec_line("vim %F", true, &|| String::from("foot"));
    assert_eq!(exec(&line, &files), [["foot", "vim", "/a/notes.txt"]]);
    let never = || -> String { panic!("no terminal is looked up for a graphical app") };
    assert_eq!(exec_line("gedit %F", false, &never), "gedit %F");
}

#[test]
fn exec_field_codes_expand_per_desktop_entry_rules() {
    let extensions = Extensions::default();
    let files = [target(&extensions, "/a/one two.txt", false), target(&extensions, "/b/three.tar", false)];
    assert_eq!(exec("tool --all %F", &files), [["tool", "--all", "/a/one two.txt", "/b/three.tar"]]);
    assert_eq!(exec("tool %f", &files), [["tool", "/a/one two.txt"], ["tool", "/b/three.tar"]]);
    assert_eq!(exec("tool --name=%w%x %% %c", &files[..1]), [["tool", "--name=one two.txt", "%", "1"]]);
    assert_eq!(exec(r#""my tool" "arg \"q\"" %U"#, &files[..1]), [["my tool", "arg \"q\"", "file:///a/one%20two.txt"]]);
    assert_eq!(exec("tool %k %i", &files[..1]), [["tool", "/d/menu.desktop"]]);
    assert!(
        Run::Exec { line: "tool \"open".to_string(), desktop: PathBuf::new(), codes: Codes::Action }
            .commands(&files, Path::new("/"))
            .is_err()
    );
}

#[test]
fn an_application_exec_line_follows_the_desktop_entry_codes() {
    let extensions = Extensions::default();
    let files = [target(&extensions, "/a/one.txt", false), target(&extensions, "/b/two.txt", false)];
    let viewer = || Codes::Entry { name: String::from("Image Viewer"), icon: Some(String::from("viewer")) };
    assert_eq!(
        exec_with("viewer --title %c %i %F", &files, viewer()),
        [["viewer", "--title", "Image Viewer", "--icon", "viewer", "/a/one.txt", "/b/two.txt"]],
        "%c is the app's name and %i its icon, not a count"
    );
    assert_eq!(
        exec_with("viewer %d %D %n %N %v %m %f", &files, viewer()),
        [["viewer", "/a/one.txt"], ["viewer", "/b/two.txt"]],
        "deprecated codes are dropped and do not split the launch"
    );
    let plain = Codes::Entry { name: String::from("Viewer"), icon: None };
    assert_eq!(exec_with("viewer %i %U", &files[..1], plain), [["viewer", "file:///a/one.txt"]], "no icon, no --icon");
}

fn app(name: &str, mimes: &str) -> String {
    format!("[Desktop Entry]\nType=Application\nName={name}\nExec={name} %F\nMimeType={mimes}\n")
}

#[test]
fn open_with_lists_apps_that_handle_every_selected_type() {
    let scratch = scratch_with_mime("open-with");
    scratch.write("share/applications/viewer.desktop", &app("viewer", "image/png;"));
    scratch.write("share/applications/editor.desktop", &app("editor", "text/plain;"));
    scratch.write("share/applications/gnome/code.desktop", &app("code", "text/x-csrc;image/png;"));
    scratch.write(
        "share/applications/hidden.desktop",
        "[Desktop Entry]\nName=hidden\nExec=hidden\nMimeType=image/png;\nHidden=true\n",
    );
    scratch.write("share/applications/mimeapps.list", "[Default Applications]\nimage/png=gnome-code.desktop;\n");
    let extensions = Extensions::load(&scratch.roots(), Lang::En);
    let png = extensions.target(Path::new("/x/a.png"), false);
    let c = extensions.target(Path::new("/x/a.c"), false);
    let names = |targets: &[Target]| -> Vec<String> {
        extensions.apps_for(targets).iter().map(|app| app.name.clone()).collect()
    };
    assert_eq!(names(std::slice::from_ref(&png)), ["code", "viewer"]);
    assert_eq!(names(std::slice::from_ref(&c)), ["code", "editor"]);
    assert_eq!(names(&[png, c]), ["code"]);
    assert_eq!(extensions.default_app("image/png"), Some("gnome-code.desktop"));
}

#[test]
fn open_with_hides_no_display_entries_and_repeated_launchers() {
    let scratch = scratch_with_mime("open-with-listed");
    scratch.write("share/applications/browser.desktop", &app("browser", "image/png;"));
    scratch.write(
        "share/applications/com.vendor.Browser.desktop",
        "[Desktop Entry]\nName=browser\nExec=browser %F\nMimeType=image/png;\nNoDisplay=true\n",
    );
    scratch
        .write("share/applications/copy.desktop", "[Desktop Entry]\nName=Copy\nExec=browser %F\nMimeType=image/png;\n");
    scratch.write("share/applications/viewer.desktop", &app("viewer", "image/png;"));
    let extensions = Extensions::load(&scratch.roots(), Lang::En);
    let png = extensions.target(Path::new("/x/a.png"), false);
    let all = extensions.apps_for(std::slice::from_ref(&png)).len();
    let listed: Vec<String> =
        extensions.listed_apps_for(std::slice::from_ref(&png)).iter().map(|app| app.name.clone()).collect();
    assert_eq!(all, 4, "every entry can still open the file");
    assert_eq!(listed.len(), 2, "{listed:?}");
    assert!(listed.contains(&String::from("viewer")));
}

#[test]
fn choosing_a_default_app_rewrites_only_that_mime_line() {
    use glide_files::fs::actions::with_default;
    let before = "[Added Associations]\nimage/png=a.desktop;\n[Default Applications]\nimage/png=old.desktop;\ntext/plain=ed.desktop;\n";
    let after = with_default(before, "image/png", "new.desktop");
    assert!(after.contains("[Added Associations]\nimage/png=a.desktop;"));
    assert!(after.contains("image/png=new.desktop;"));
    assert!(!after.contains("old.desktop"));
    assert!(after.contains("text/plain=ed.desktop;"));
    let fresh = with_default("", "image/png", "new.desktop");
    assert_eq!(fresh, "[Default Applications]\nimage/png=new.desktop;\n");
}

#[test]
fn setting_default_app_associates_arbitrary_apps_and_updates_in_memory() {
    let scratch = scratch_with_mime("set-default");
    scratch.write("share/applications/generic.desktop", "[Desktop Entry]\nName=Generic\nExec=generic %F\n");
    let mut extensions = Extensions::load(&scratch.roots(), Lang::En);
    let custom = extensions.target(Path::new("/x/file.xyz"), false);
    assert!(extensions.apps_for(std::slice::from_ref(&custom)).is_empty());
    extensions.set_default(&custom.mime, "generic.desktop");
    assert_eq!(extensions.default_app(&custom.mime), Some("generic.desktop"));
    let apps = extensions.apps_for(std::slice::from_ref(&custom));
    assert_eq!(apps.len(), 1);
    assert_eq!(apps[0].id, "generic.desktop");
}

#[test]
fn all_usable_apps_are_discoverable_for_other_applications() {
    let scratch = scratch_with_mime("all-apps");
    scratch.write("share/applications/generic.desktop", "[Desktop Entry]\nName=Generic\nExec=generic\n");
    scratch.write("share/applications/hidden.desktop", "[Desktop Entry]\nName=Hidden\nExec=hidden\nNoDisplay=true\n");
    let extensions = Extensions::load(&scratch.roots(), Lang::En);
    let all = extensions.all_listed_apps();
    assert!(all.iter().any(|app| app.id == "generic.desktop"));
    assert!(!all.iter().any(|app| app.id == "hidden.desktop"));
}

#[test]
fn expand_without_field_codes_appends_file_targets() {
    let extensions = Extensions::default();
    let files = [target(&extensions, "/a/one.txt", false), target(&extensions, "/b/two.txt", false)];
    let plain = Codes::Entry { name: String::from("Tool"), icon: None };
    let args = exec_with("my-tool --option", &files, plain);
    assert_eq!(args, [["my-tool", "--option", "/a/one.txt", "/b/two.txt"]]);
}

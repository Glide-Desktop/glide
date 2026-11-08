use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;

struct Staging(PathBuf);

impl Drop for Staging {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn installer_stages_the_welcome_binary_desktop_entry_and_icon() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let staging = Staging(std::env::temp_dir().join(format!("glide-welcome-install-{}", std::process::id())));
    let binaries = staging.0.join("binaries");
    let destination = staging.0.join("destination");
    std::fs::create_dir_all(&binaries).expect("create binary fixtures");
    for name in [
        "glide",
        "glidectl",
        "glide-settings",
        "glide-files",
        "glide-terminal",
        "glide-text",
        "glide-passwords",
        "glide-photos",
        "glide-folio",
        "glide-clips",
        "glide-catalog",
        "glide-portal",
        "glide-keychain",
        "glide-phone",
        "glide-xembed",
        "libpam_glide_keychain.so",
    ] {
        std::fs::write(binaries.join(name), b"fixture").expect("create binary fixture");
    }
    let welcome = PathBuf::from(env!("CARGO_BIN_EXE_glide-welcome"));
    std::fs::copy(&welcome, binaries.join("glide-welcome")).expect("copy the built welcome binary");

    let result = Command::new(root.join("scripts/install.sh"))
        .args(["--prefix", "/opt/glide-test", "--destdir"])
        .arg(&destination)
        .arg("--from")
        .arg(&binaries)
        .output()
        .expect("run installer");
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));

    let installed = destination.join("opt/glide-test/bin/glide-welcome");
    let metadata = std::fs::metadata(&installed).expect("welcome binary is installed");
    assert_eq!(metadata.len(), std::fs::metadata(binaries.join("glide-welcome")).unwrap().len());
    assert_ne!(metadata.permissions().mode() & 0o111, 0);
    let desktop = destination.join("opt/glide-test/share/applications/glide-welcome.desktop");
    assert_eq!(std::fs::read(desktop).unwrap(), std::fs::read(root.join("data/glide-welcome.desktop")).unwrap());
    let icon = destination.join("usr/share/icons/hicolor/scalable/apps/glide-welcome.svg");
    assert_eq!(std::fs::read(icon).unwrap(), std::fs::read(root.join("data/glide.svg")).unwrap());
    let folio_desktop = destination.join("opt/glide-test/share/applications/glide-folio.desktop");
    let folio_desktop = std::fs::read_to_string(folio_desktop).unwrap();
    assert!(folio_desktop.lines().any(|line| line == "Icon=glide-folio"));
    let folio_icon = destination.join("usr/share/icons/hicolor/scalable/apps/glide-folio.svg");
    assert_eq!(std::fs::read(folio_icon).unwrap(), std::fs::read(root.join("data/glide-folio.svg")).unwrap());
}

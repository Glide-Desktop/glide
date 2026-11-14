use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;

use glide_config::facekey::{PAM_SERVICES, PamService};

pub const MARKER: &str = "# glide-facekey";
pub const ETC_DIR: &str = "/etc/pam.d";
pub const VENDOR_DIR: &str = "/usr/lib/pam.d";

#[must_use]
pub fn line(binary: &Path, service: PamService) -> String {
    format!(
        "auth       sufficient   pam_exec.so quiet {} -verify -source {}",
        binary.display(),
        service.source().name()
    )
}

#[must_use]
pub fn enabled(text: &str) -> bool {
    text.lines().any(|line| line.trim() == MARKER)
}

#[must_use]
pub fn without_facekey(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut skip_next = false;
    for line in text.split_inclusive('\n') {
        if skip_next {
            skip_next = false;
            continue;
        }
        if line.trim() == MARKER {
            skip_next = true;
            continue;
        }
        out.push_str(line);
    }
    out
}

#[must_use]
pub fn with_facekey(text: &str, rule: &str) -> String {
    let clean = without_facekey(text);
    let mut out = String::with_capacity(clean.len() + rule.len() + MARKER.len() + 2);
    let mut placed = false;
    for line in clean.split_inclusive('\n') {
        let trimmed = line.trim();
        if !placed && !trimmed.is_empty() && !trimmed.starts_with('#') {
            out.push_str(MARKER);
            out.push('\n');
            out.push_str(rule);
            out.push('\n');
            placed = true;
        }
        out.push_str(line);
    }
    if !placed {
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(MARKER);
        out.push('\n');
        out.push_str(rule);
        out.push('\n');
    }
    out
}

pub fn apply(service: PamService, on: bool, binary: &Path, etc: &Path, vendor: &Path) -> io::Result<()> {
    let target = etc.join(service.file());
    let source = if target.exists() { target.clone() } else { vendor.join(service.file()) };
    let meta = fs::symlink_metadata(&source)?;
    if !meta.is_file() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, format!("{} is not a regular file", source.display())));
    }
    let text = fs::read_to_string(&source)?;
    let next = if on { with_facekey(&text, &line(binary, service)) } else { without_facekey(&text) };
    if next == text && source == target {
        return Ok(());
    }
    let temp = etc.join(format!(".{}.facekey", service.file()));
    let _ = fs::remove_file(&temp);
    let mut file =
        OpenOptions::new().write(true).create_new(true).mode(0o600).custom_flags(libc::O_NOFOLLOW).open(&temp)?;
    file.write_all(next.as_bytes())?;
    file.sync_all()?;
    drop(file);
    fs::set_permissions(&temp, fs::Permissions::from_mode(meta.permissions().mode() & 0o7777))?;
    fs::rename(&temp, &target)
}

#[must_use]
pub fn status(etc: &Path, vendor: &Path) -> BTreeMap<String, Option<bool>> {
    PAM_SERVICES
        .iter()
        .map(|service| {
            let file = service.file();
            let text = fs::read_to_string(etc.join(file)).or_else(|_| fs::read_to_string(vendor.join(file)));
            (service.source().name().to_string(), text.ok().map(|text| enabled(&text)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SUDO: &str = "#%PAM-1.0\nauth\t\tinclude\t\tsystem-auth\naccount\t\tinclude\t\tsystem-auth\n";

    #[test]
    fn the_rule_goes_before_the_first_auth_line_and_comes_out_exactly() {
        let rule = line(Path::new("/usr/bin/facekey"), PamService::Sudo);
        let on = with_facekey(SUDO, &rule);
        assert!(on.starts_with(&format!("#%PAM-1.0\n{MARKER}\n{rule}\nauth")));
        assert!(enabled(&on));
        assert_eq!(with_facekey(&on, &rule), on);
        assert_eq!(without_facekey(&on), SUDO);
        assert!(!enabled(SUDO));
    }

    #[test]
    fn a_file_of_only_comments_gets_the_rule_appended() {
        let on = with_facekey("# empty", "auth sufficient x");
        assert_eq!(on, format!("# empty\n{MARKER}\nauth sufficient x\n"));
    }

    #[test]
    fn apply_copies_a_vendor_file_into_etc_and_keeps_its_mode() {
        let dir = tempfile::tempdir().unwrap();
        let (etc, vendor) = (dir.path().join("etc"), dir.path().join("vendor"));
        fs::create_dir_all(&etc).unwrap();
        fs::create_dir_all(&vendor).unwrap();
        fs::write(vendor.join("polkit-1"), SUDO).unwrap();
        fs::set_permissions(vendor.join("polkit-1"), fs::Permissions::from_mode(0o644)).unwrap();
        let binary = Path::new("/usr/bin/facekey");
        apply(PamService::Polkit, true, binary, &etc, &vendor).unwrap();
        let written = fs::read_to_string(etc.join("polkit-1")).unwrap();
        assert!(enabled(&written));
        assert_eq!(fs::metadata(etc.join("polkit-1")).unwrap().permissions().mode() & 0o777, 0o644);
        assert_eq!(status(&etc, &vendor).get("polkit"), Some(&Some(true)));
        apply(PamService::Polkit, false, binary, &etc, &vendor).unwrap();
        assert_eq!(fs::read_to_string(etc.join("polkit-1")).unwrap(), SUDO);
        assert_eq!(status(&etc, &vendor).get("sudo"), Some(&None));
    }
}

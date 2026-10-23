use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Access {
    Clipboard,
    Input,
    Screen,
}

impl Access {
    pub const ALL: [Self; 3] = [Self::Clipboard, Self::Input, Self::Screen];
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Rule {
    pub allow: Vec<String>,
    pub deny: Vec<String>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Permissions {
    pub clipboard: Rule,
    pub input: Rule,
    pub screen: Rule,
}

impl Permissions {
    #[must_use]
    pub fn path() -> Option<PathBuf> {
        crate::state_home().map(|dir| dir.join("glide").join("permissions.toml"))
    }

    pub fn load_from(path: &Path) -> io::Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err)),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(err) => Err(err),
        }
    }

    pub fn save_to(&self, path: &Path) -> io::Result<()> {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let text = toml::to_string(self).map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        let dir = path.parent().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no parent folder"))?;
        std::fs::create_dir_all(dir)?;
        let temp = dir.join(format!(".permissions.{}.tmp", std::process::id()));
        let mut file = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&temp)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        std::fs::rename(&temp, path)
    }

    #[must_use]
    pub const fn rule(&self, access: Access) -> &Rule {
        match access {
            Access::Clipboard => &self.clipboard,
            Access::Input => &self.input,
            Access::Screen => &self.screen,
        }
    }

    const fn rule_mut(&mut self, access: Access) -> &mut Rule {
        match access {
            Access::Clipboard => &mut self.clipboard,
            Access::Input => &mut self.input,
            Access::Screen => &mut self.screen,
        }
    }

    #[must_use]
    pub fn remembered(&self, access: Access, program: &str) -> Option<bool> {
        let rule = self.rule(access);
        if rule.deny.iter().any(|entry| entry == program) {
            Some(false)
        } else if rule.allow.iter().any(|entry| entry == program) {
            Some(true)
        } else {
            None
        }
    }

    pub fn remember(&mut self, access: Access, program: &str, allowed: bool) {
        self.forget(access, program);
        let rule = self.rule_mut(access);
        let list = if allowed { &mut rule.allow } else { &mut rule.deny };
        list.push(program.to_string());
        list.sort();
    }

    pub fn forget(&mut self, access: Access, program: &str) {
        let rule = self.rule_mut(access);
        rule.allow.retain(|entry| entry != program);
        rule.deny.retain(|entry| entry != program);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_program_is_unknown_until_remembered_and_a_new_answer_replaces_the_old_one() {
        let mut permissions = Permissions::default();
        assert_eq!(permissions.remembered(Access::Clipboard, "/usr/bin/wl-paste"), None);
        permissions.remember(Access::Clipboard, "/usr/bin/wl-paste", false);
        assert_eq!(permissions.remembered(Access::Clipboard, "/usr/bin/wl-paste"), Some(false));
        permissions.remember(Access::Clipboard, "/usr/bin/wl-paste", true);
        assert_eq!(permissions.remembered(Access::Clipboard, "/usr/bin/wl-paste"), Some(true));
        assert!(permissions.clipboard.deny.is_empty());
        assert_eq!(permissions.remembered(Access::Screen, "/usr/bin/wl-paste"), None, "kinds are kept apart");
    }

    #[test]
    fn a_hand_edited_file_listing_a_program_twice_is_read_as_a_refusal() {
        let permissions = Permissions {
            input: Rule { allow: vec!["/usr/bin/wtype".into()], deny: vec!["/usr/bin/wtype".into()] },
            ..Permissions::default()
        };
        assert_eq!(permissions.remembered(Access::Input, "/usr/bin/wtype"), Some(false));
    }

    #[test]
    fn the_file_round_trips_and_a_missing_file_reads_as_empty() {
        let dir = std::env::temp_dir().join(format!("glide-permissions-test-{}", std::process::id()));
        let path = dir.join("permissions.toml");
        assert_eq!(Permissions::load_from(&path).unwrap(), Permissions::default());
        let mut permissions = Permissions::default();
        permissions.remember(Access::Screen, "/usr/bin/grim", true);
        permissions.save_to(&path).unwrap();
        assert_eq!(Permissions::load_from(&path).unwrap(), permissions);
        let mode = std::fs::metadata(&path).unwrap().permissions();
        assert_eq!(std::os::unix::fs::PermissionsExt::mode(&mode) & 0o777, 0o600);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn an_unknown_key_is_an_error_not_a_silent_grant() {
        let dir = std::env::temp_dir().join(format!("glide-permissions-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("permissions.toml");
        std::fs::write(&path, "[clipboard]\nallowed = [\"/usr/bin/x\"]\n").unwrap();
        assert!(Permissions::load_from(&path).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
}

use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub const NAMESPACE: &str = "permissions";

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
    pub fn legacy_path() -> Option<PathBuf> {
        crate::state_home().map(|dir| dir.join("glide").join("permissions.toml"))
    }

    #[must_use]
    pub fn load() -> Self {
        crate::service::load(NAMESPACE)
    }

    #[must_use]
    pub fn from_values(values: &crate::service::Values) -> Self {
        crate::service::read(values).unwrap_or_default()
    }

    pub fn update(change: impl FnOnce(&mut Self)) -> io::Result<Self> {
        crate::service::update(NAMESPACE, change).map_err(io::Error::other)
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
    fn stored_answers_read_back_and_an_unreadable_set_grants_nothing() {
        let mut permissions = Permissions::default();
        permissions.remember(Access::Screen, "/usr/bin/grim", true);
        let values = serde_json::to_value(&permissions).map(crate::service::flatten).expect("serializes");
        assert_eq!(Permissions::from_values(&values), permissions);
        let broken = crate::service::Values::from([("screen.allow".to_owned(), serde_json::json!("/usr/bin/x"))]);
        assert_eq!(Permissions::from_values(&broken), Permissions::default());
    }
}

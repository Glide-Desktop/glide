use std::time::Instant;

use glide_shell::i18n::{Key, Lang};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stage {
    Ask,
    Checking,
    Wrong,
    Locked,
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Control {
    Close,
    Field,
    Reveal,
    Submit,
    Shield,
    Details,
    Cancel,
    Understood,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Hint {
    Keys,
    Checking,
    Wrong { left: u8 },
    Broken,
    CapsLock,
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct View {
    pub stage: Stage,
    pub app: Option<String>,
    pub icon: Option<String>,
    pub message: String,
    pub user: String,
    pub display: String,
    pub action: String,
    pub program: Option<String>,
    pub bullets: usize,
    pub shown: Option<String>,
    pub details: bool,
    pub shield: bool,
    pub hint: Hint,
    pub note: Option<String>,
    pub ready: bool,
}

impl std::fmt::Debug for View {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("View")
            .field("stage", &self.stage)
            .field("app", &self.app)
            .field("action", &self.action)
            .field("details", &self.details)
            .field("shield", &self.shield)
            .field("hint", &self.hint)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Motion {
    pub shaken: Option<Instant>,
    pub checking: Option<Instant>,
}

#[must_use]
pub fn attempts_left(lang: Lang, left: u8) -> String {
    let key = match lang.plural(u64::from(left)) {
        glide_app::i18n::Plural::One => Key::PkLeftOne,
        glide_app::i18n::Plural::Few => Key::PkLeftFew,
        glide_app::i18n::Plural::Many => Key::PkLeftMany,
    };
    let text = match lang {
        Lang::Uk => key.uk(),
        Lang::En => key.en(),
    };
    text.replace("{n}", &left.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ukrainian_attempt_counts_agree_with_the_number() {
        assert_eq!(attempts_left(Lang::En, 1), "1 attempt left.");
        assert_eq!(attempts_left(Lang::Uk, 1), "Залишилася 1 спроба.");
        assert_eq!(attempts_left(Lang::Uk, 2), "Залишилось 2 спроби.");
        assert_eq!(attempts_left(Lang::Uk, 5), "Залишилось 5 спроб.");
    }
}

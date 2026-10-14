use std::collections::HashMap;

pub use crate::config::{LayoutScope, NewWindowLayout};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ScopeKey {
    Desktop(u64),
    App(String),
    Window(u64),
}

#[derive(Debug, Default)]
pub struct LayoutMemory {
    scope: LayoutScope,
    new_window: NewWindowLayout,
    current: Option<ScopeKey>,
    remembered: HashMap<ScopeKey, u32>,
}

impl LayoutMemory {
    #[must_use]
    pub const fn scope(&self) -> LayoutScope {
        self.scope
    }

    pub fn set_policy(&mut self, scope: LayoutScope, new_window: NewWindowLayout) {
        self.new_window = new_window;
        if self.scope != scope {
            self.scope = scope;
            self.current = None;
            self.remembered.clear();
        }
    }

    pub fn enter(&mut self, key: Option<ScopeKey>, active: u32) -> Option<u32> {
        if self.current == key {
            return None;
        }
        if let Some(previous) = std::mem::replace(&mut self.current, key.clone()) {
            self.remembered.insert(previous, active);
        }
        let fresh = (self.new_window == NewWindowLayout::Default).then_some(0);
        self.remembered.get(&key?).copied().or(fresh).filter(|target| *target != active)
    }

    pub fn retain(&mut self, keep: impl Fn(&ScopeKey) -> bool) {
        self.remembered.retain(|key, _| keep(key));
        if self.current.as_ref().is_some_and(|key| !keep(key)) {
            self.current = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(id: u64) -> Option<ScopeKey> {
        (id > 0).then_some(ScopeKey::Window(id))
    }

    #[test]
    fn a_key_seen_for_the_first_time_starts_with_the_default_layout() {
        let mut memory = LayoutMemory::default();
        assert_eq!(memory.enter(window(1), 0), None);
        assert_eq!(memory.enter(window(2), 1), Some(0));
    }

    #[test]
    fn a_key_seen_for_the_first_time_can_keep_the_current_layout_instead() {
        let mut memory = LayoutMemory::default();
        memory.set_policy(LayoutScope::Window, NewWindowLayout::Current);
        assert_eq!(memory.enter(window(1), 0), None);
        assert_eq!(memory.enter(window(2), 1), None);
    }

    #[test]
    fn returning_to_a_key_restores_the_layout_it_was_left_with() {
        let mut memory = LayoutMemory::default();
        memory.enter(window(1), 0);
        memory.enter(window(2), 1);
        assert_eq!(memory.enter(window(1), 0), Some(1));
        assert_eq!(memory.enter(window(2), 1), Some(0));
    }

    #[test]
    fn leaving_every_key_remembers_the_layout_and_reentering_is_a_no_op_when_it_matches() {
        let mut memory = LayoutMemory::default();
        memory.enter(window(1), 0);
        assert_eq!(memory.enter(window(0), 2), None, "no key means leave");
        assert_eq!(memory.enter(window(1), 2), None);
        assert_eq!(memory.enter(window(1), 0), None, "same key twice never switches");
    }

    #[test]
    fn a_forgotten_window_no_longer_restores_anything() {
        let mut memory = LayoutMemory::default();
        memory.enter(window(1), 0);
        memory.enter(window(2), 1);
        memory.retain(|key| *key != ScopeKey::Window(1));
        assert_eq!(memory.enter(window(1), 0), None);
    }

    #[test]
    fn changing_the_scope_drops_everything_remembered() {
        let mut memory = LayoutMemory::default();
        memory.enter(Some(ScopeKey::App("a".into())), 0);
        memory.enter(Some(ScopeKey::App("b".into())), 1);
        memory.set_policy(LayoutScope::Window, NewWindowLayout::Current);
        assert_eq!(memory.scope(), LayoutScope::Window);
        assert_eq!(memory.enter(Some(ScopeKey::App("a".into())), 0), None);
    }
}

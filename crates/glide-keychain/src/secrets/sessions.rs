use std::collections::{BTreeSet, HashMap};

use super::session::SessionKey;

pub const MAX_SESSIONS: usize = 1024;

struct Slot {
    key: Option<SessionKey>,
    owner: Option<String>,
}

#[derive(Default)]
pub struct Sessions {
    slots: HashMap<String, Slot>,
}

#[must_use]
pub fn serial(path: &str) -> u64 {
    path.rsplit('/').next().and_then(|tail| tail.parse().ok()).unwrap_or(0)
}

#[must_use]
pub fn victim<'a>(
    sessions: impl IntoIterator<Item = (u64, Option<&'a str>)>, alive: impl Fn(&str) -> bool,
) -> Option<u64> {
    let mut oldest: Option<u64> = None;
    let mut orphan: Option<u64> = None;
    for (serial, owner) in sessions {
        oldest = Some(oldest.map_or(serial, |at| at.min(serial)));
        if owner.is_some_and(|owner| !alive(owner)) {
            orphan = Some(orphan.map_or(serial, |at| at.min(serial)));
        }
    }
    orphan.or(oldest)
}

impl Sessions {
    pub fn insert(&mut self, path: String, key: Option<SessionKey>, owner: Option<String>) {
        self.slots.insert(path, Slot { key, owner });
    }

    #[must_use]
    pub fn key(&self, path: &str) -> Option<Option<SessionKey>> {
        self.slots.get(path).map(|slot| slot.key.clone())
    }

    pub fn remove(&mut self, path: &str) -> bool {
        self.slots.remove(path).is_some()
    }

    pub fn drop_owner(&mut self, owner: &str) -> Vec<String> {
        let gone: Vec<String> = self
            .slots
            .iter()
            .filter(|(_, slot)| slot.owner.as_deref() == Some(owner))
            .map(|(path, _)| path.clone())
            .collect();
        for path in &gone {
            self.slots.remove(path);
        }
        gone
    }

    #[must_use]
    pub fn over_limit(&self) -> bool {
        self.slots.len() > MAX_SESSIONS
    }

    #[must_use]
    pub fn owners(&self) -> BTreeSet<String> {
        self.slots.values().filter_map(|slot| slot.owner.clone()).collect()
    }

    pub fn evict(&mut self, alive: impl Fn(&str) -> bool) -> Option<String> {
        let chosen = victim(self.slots.iter().map(|(path, slot)| (serial(path), slot.owner.as_deref())), alive)?;
        let path = self.slots.keys().find(|path| serial(path) == chosen).cloned()?;
        self.slots.remove(&path);
        Some(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(serial: u64) -> String {
        format!("/org/freedesktop/secrets/session/{serial}")
    }

    #[test]
    fn the_oldest_session_of_a_gone_owner_goes_first() {
        let sessions = [(3, Some(":1.30")), (1, Some(":1.10")), (2, Some(":1.20")), (4, Some(":1.20"))];
        assert_eq!(victim(sessions, |owner| owner != ":1.20"), Some(2));
        assert_eq!(victim(sessions, |owner| owner == ":1.10"), Some(2), "the lowest serial among the orphans");
        assert_eq!(victim(sessions, |_| false), Some(1));
    }

    #[test]
    fn without_an_orphan_the_oldest_session_is_evicted_and_unknown_owners_count_as_alive() {
        let sessions = [(7, Some(":1.7")), (5, Some(":1.5")), (9, None)];
        assert_eq!(victim(sessions, |_| true), Some(5));
        assert_eq!(
            victim([(4, None), (2, None)], |_| false),
            Some(2),
            "a session with no known owner is never an orphan"
        );
        assert_eq!(victim(std::iter::empty(), |_| true), None);
        assert_eq!(victim([(4, Some(":1.4")), (2, None)], |owner| owner != ":1.4"), Some(4));
    }

    #[test]
    fn serials_are_read_from_the_path_and_compared_as_numbers() {
        assert_eq!(serial(&path(1024)), 1024);
        assert!(serial(&path(9)) < serial(&path(10)), "9 sorts before 10 numerically, not as text");
        assert_eq!(serial("/org/freedesktop/secrets/session/x"), 0);
    }

    #[test]
    fn a_departed_clients_sessions_are_dropped_and_the_others_stay() {
        let mut sessions = Sessions::default();
        sessions.insert(path(1), None, Some(":1.10".into()));
        sessions.insert(path(2), None, Some(":1.20".into()));
        sessions.insert(path(3), None, Some(":1.10".into()));
        sessions.insert(path(4), None, None);
        let mut dropped = sessions.drop_owner(":1.10");
        dropped.sort();
        assert_eq!(dropped, [path(1), path(3)]);
        assert!(sessions.key(&path(1)).is_none() && sessions.key(&path(3)).is_none());
        assert!(sessions.key(&path(2)).is_some() && sessions.key(&path(4)).is_some());
        assert!(sessions.drop_owner(":1.10").is_empty());
        assert_eq!(sessions.owners().into_iter().collect::<Vec<_>>(), [":1.20"]);
    }

    #[test]
    fn a_crowded_table_evicts_an_orphan_instead_of_the_long_running_clients_session() {
        let mut sessions = Sessions::default();
        sessions.insert(path(1), None, Some(":1.1".into()));
        for serial in 2..=MAX_SESSIONS as u64 {
            sessions.insert(path(serial), None, Some(":1.2".into()));
        }
        assert!(!sessions.over_limit());
        sessions.insert(path(MAX_SESSIONS as u64 + 1), None, Some(":1.3".into()));
        assert!(sessions.over_limit());
        let evicted = sessions.evict(|owner| owner != ":1.2");
        assert_eq!(evicted, Some(path(2)), "the oldest session whose client left, not the oldest session");
        assert!(sessions.key(&path(1)).is_some(), "the long-running client keeps its session");
        assert!(sessions.key(&path(2)).is_none());
        assert_eq!(sessions.evict(|_| true), Some(path(1)), "with every owner alive the oldest goes");
    }

    #[test]
    fn closing_a_session_forgets_it() {
        let mut sessions = Sessions::default();
        sessions.insert(path(1), None, Some(":1.1".into()));
        assert!(sessions.remove(&path(1)));
        assert!(!sessions.remove(&path(1)));
        assert!(sessions.key(&path(1)).is_none());
    }
}

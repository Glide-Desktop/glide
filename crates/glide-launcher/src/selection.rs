use std::path::PathBuf;

use super::{
    Launcher,
    list::{list_rect, max_scroll, rows},
    search::Entry,
};

#[derive(Debug, Clone, PartialEq, Eq)]
enum EntryKey {
    Whole(Entry),
    App(String),
    Window(u64),
    File(PathBuf),
    Action(String, String),
    Provided(String, String),
}

#[derive(Debug, Clone)]
pub(super) struct Kept {
    key: EntryKey,
    scroll: usize,
    pointer_selected: bool,
}

impl Launcher {
    fn entry_key(&self, entry: &Entry) -> Option<EntryKey> {
        Some(match entry {
            Entry::App(index) => EntryKey::App(self.apps.get(*index)?.id.clone()),
            Entry::Window { index, .. } => EntryKey::Window(self.windows.get(*index)?.id),
            Entry::File(index) => EntryKey::File(self.recent.get(*index)?.path.clone()),
            Entry::Action { app, action } => {
                let owner = self.apps.get(*app)?;
                EntryKey::Action(owner.id.clone(), owner.actions.get(*action)?.id.clone())
            }
            Entry::Provided { index, .. } => {
                let found = self.provided.get(*index)?;
                EntryKey::Provided(found.provider.bus.clone(), found.id.clone())
            }
            Entry::Run(_) | Entry::Calculator { .. } | Entry::Command(_) | Entry::Settings(_) => {
                EntryKey::Whole(entry.clone())
            }
        })
    }

    pub(super) fn keep_selection(&self) -> Option<Kept> {
        if !self.is_listing() {
            return None;
        }
        let entry = self.entries.get(self.selected?)?;
        Some(Kept { key: self.entry_key(entry)?, scroll: self.list_scroll, pointer_selected: self.pointer_selected })
    }

    pub(super) fn restore_selection(&mut self, kept: &Kept) {
        let Some(position) = self.entries.iter().position(|entry| self.entry_key(entry).as_ref() == Some(&kept.key))
        else {
            return;
        };
        self.selected = Some(position);
        self.pointer_selected = kept.pointer_selected;
        let limit = max_scroll(&rows(&self.entries), list_rect(self.area).size.h);
        self.list_scroll = kept.scroll.min(limit);
        self.ensure_selection_visible();
    }
}

#[cfg(test)]
mod tests {
    use smithay::utils::{Logical, Point, Rectangle, Size};

    use super::*;
    use crate::{icon::DesktopApp, providers, recent::RecentFile, search::OpenWindow};

    fn area() -> Rectangle<i32, Logical> {
        Rectangle::new(Point::from((0, 0)), Size::from((1920, 1000)))
    }

    fn apps(names: &[&str]) -> Vec<DesktopApp> {
        names
            .iter()
            .map(|name| DesktopApp {
                id: format!("{}.desktop", name.to_lowercase()),
                name: (*name).to_string(),
                command: "true".to_string(),
                ..DesktopApp::default()
            })
            .collect()
    }

    fn typed(launcher: &mut Launcher, text: &str) {
        for ch in text.chars() {
            launcher.push_query(ch);
        }
    }

    fn found(id: &str, name: &str) -> providers::Found {
        let provider = providers::Provider {
            desktop_id: "test.desktop".into(),
            bus: "org.test.Provider".into(),
            path: "/".into(),
        };
        providers::Found {
            desktop_id: provider.desktop_id.clone(),
            provider,
            id: id.into(),
            name: name.into(),
            description: String::new(),
        }
    }

    fn walked_to(launcher: &mut Launcher, row: usize) {
        for _ in 0..row {
            assert!(launcher.move_selection(0, 1));
        }
        assert_eq!(launcher.selected(), Some(row));
    }

    fn selected_app_id(launcher: &Launcher) -> String {
        match launcher.selected_entry() {
            Some(Entry::App(index)) => launcher.apps[index].id.clone(),
            other => panic!("an app row was expected, got {other:?}"),
        }
    }

    #[test]
    fn a_provider_answer_leaves_the_keyboard_selection_where_it_was() {
        let mut launcher = Launcher::new(apps(&["Fire", "Firefox", "Firebird", "Firewall"]), area());
        typed(&mut launcher, "fire");
        walked_to(&mut launcher, 2);
        let chosen = launcher.selected_entry();
        let generation = launcher.provider_generation;

        launcher.set_provided(generation, vec![found("a", "Alpha"), found("b", "Beta")]);

        assert_eq!(launcher.entries.len(), 6, "the answer adds its rows");
        assert_eq!(launcher.selected(), Some(2));
        assert_eq!(launcher.selected_entry(), chosen);
    }

    #[test]
    fn a_selected_provider_row_is_followed_when_a_newer_answer_reorders_the_rows() {
        let mut launcher = Launcher::new(apps(&["Fire", "Firefox"]), area());
        typed(&mut launcher, "fire");
        let generation = launcher.provider_generation;
        launcher.set_provided(generation, vec![found("a", "Alpha"), found("b", "Beta"), found("c", "Gamma")]);
        walked_to(&mut launcher, 3);
        let provided_id = |launcher: &Launcher| match launcher.selected_entry() {
            Some(Entry::Provided { index, .. }) => launcher.provided[index].id.clone(),
            other => panic!("a provider row was expected, got {other:?}"),
        };
        assert_eq!(provided_id(&launcher), "b");

        launcher.set_provided(generation, vec![found("c", "Gamma"), found("d", "Delta"), found("b", "Beta")]);

        assert_eq!(provided_id(&launcher), "b");
        assert_eq!(launcher.selected(), Some(4));
    }

    #[test]
    fn a_row_that_vanished_from_the_answer_falls_back_to_the_first_row() {
        let mut launcher = Launcher::new(apps(&["Fire", "Firefox"]), area());
        typed(&mut launcher, "fire");
        let generation = launcher.provider_generation;
        launcher.set_provided(generation, vec![found("a", "Alpha")]);
        walked_to(&mut launcher, 2);

        launcher.set_provided(generation, vec![found("z", "Zeta")]);

        assert_eq!(launcher.selected(), Some(0));
    }

    #[test]
    fn a_list_scrolled_for_the_selection_stays_scrolled_when_an_answer_arrives() {
        let names: Vec<String> = (0..40).map(|i| format!("Item {i:02}")).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let mut launcher = Launcher::new(apps(&refs), area());
        typed(&mut launcher, "item");
        launcher.set_area(Rectangle::new(Point::from((0, 0)), Size::from((1920, 420))));
        for _ in 0..launcher.entries.len() {
            launcher.move_selection(0, 1);
        }
        let (scroll, chosen) = (launcher.list_scroll, launcher.selected_entry());
        assert!(scroll > 0, "the list had to scroll");

        let generation = launcher.provider_generation;
        launcher.set_provided(generation, vec![found("a", "Alpha")]);

        assert_eq!(launcher.list_scroll, scroll);
        assert_eq!(launcher.selected_entry(), chosen);
    }

    #[test]
    fn a_pointer_selection_stays_a_pointer_selection_when_an_answer_arrives() {
        let mut launcher = Launcher::new(apps(&["Fire", "Firefox", "Firebird"]), area());
        typed(&mut launcher, "fire");
        launcher.set_hover(Some(1));
        assert!(launcher.pointer_selected);
        let generation = launcher.provider_generation;

        launcher.set_provided(generation, vec![found("a", "Alpha")]);

        assert_eq!(launcher.selected(), Some(1));
        assert!(launcher.pointer_selected);
    }

    #[test]
    fn typing_still_resets_the_selection_to_the_first_row() {
        let mut launcher = Launcher::new(apps(&["Fire", "Firefox", "Firebird"]), area());
        typed(&mut launcher, "fire");
        walked_to(&mut launcher, 2);
        launcher.push_query('b');
        assert_eq!(launcher.selected(), Some(0));
    }

    #[test]
    fn loading_the_app_list_keeps_the_selected_app_by_identity_not_by_index() {
        let mut launcher = Launcher::new(apps(&["Fire", "Firefox", "Firebird", "Firewall"]), area());
        typed(&mut launcher, "fire");
        walked_to(&mut launcher, 2);
        let chosen = selected_app_id(&launcher);

        let reordered = apps(&["Firewall", "Firebird", "Firefox", "Fire", "Fireplace"]);
        let pixels = vec![None; reordered.len()];
        launcher.set_apps(reordered, &pixels, 1);

        assert_eq!(selected_app_id(&launcher), chosen);
    }

    #[test]
    fn new_windows_and_recent_files_do_not_move_the_selected_row_to_the_top() {
        let mut launcher = Launcher::new(apps(&["Fire", "Firefox", "Firebird"]), area());
        typed(&mut launcher, "fire");
        walked_to(&mut launcher, 2);
        let chosen = launcher.selected_entry();

        launcher.set_windows(vec![OpenWindow { id: 9, title: "Fire drill".into(), app_id: "fire".into() }]);
        assert_eq!(launcher.selected_entry(), chosen);

        launcher.set_recent(vec![RecentFile {
            path: "/home/u/fire.txt".into(),
            name: "fire.txt".into(),
            folder: "~".into(),
        }]);
        assert_eq!(launcher.selected_entry(), chosen);
    }

    #[test]
    fn a_selected_window_row_is_followed_by_window_id() {
        let mut launcher = Launcher::new(apps(&["Terminal"]), area());
        let window = |id, title: &str| OpenWindow { id, title: title.into(), app_id: String::new() };
        launcher.set_windows(vec![window(1, "build log"), window(2, "build notes")]);
        typed(&mut launcher, "build");
        let selected_window = |launcher: &Launcher| match launcher.selected_entry() {
            Some(Entry::Window { index, .. }) => launcher.windows[index].id,
            other => panic!("a window row was expected, got {other:?}"),
        };
        launcher.move_selection(0, 1);
        let chosen = selected_window(&launcher);

        launcher.set_windows(vec![window(3, "build tools"), window(2, "build notes"), window(1, "build log")]);

        assert_eq!(selected_window(&launcher), chosen);
    }
}

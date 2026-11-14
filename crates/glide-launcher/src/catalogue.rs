use std::{collections::HashMap, sync::Arc};

use glide_shell::icon::{self, DesktopApp, IconLoader};

use crate::{history::LaunchHistory, recent::RecentFile};

pub type IconPixels = Arc<[u8]>;

#[derive(Debug, Clone, Default)]
pub struct Catalogue {
    pub apps: Vec<DesktopApp>,
    pub icons: Vec<Option<IconPixels>>,
    pub scale: i32,
    pub history: LaunchHistory,
    pub recent: Vec<RecentFile>,
    keys: Vec<String>,
}

impl Catalogue {
    #[must_use]
    pub fn load(theme: Option<String>, scale: i32, previous: Option<&Self>) -> Self {
        let scale = scale.max(1);
        let side = crate::icon_side() * scale;
        let mut loader = IconLoader::new(theme);
        let mut catalogue = Self::assemble(icon::list_applications(), scale, previous, |name| {
            loader.load_named(name, side).map(<[u8]>::to_vec)
        });
        catalogue.history = LaunchHistory::load();
        catalogue.recent = crate::recent::load();
        catalogue
    }

    #[must_use]
    pub fn assemble(
        apps: Vec<DesktopApp>, scale: i32, previous: Option<&Self>, mut render: impl FnMut(&str) -> Option<Vec<u8>>,
    ) -> Self {
        let scale = scale.max(1);
        let kept: HashMap<&str, &IconPixels> = previous
            .filter(|previous| previous.scale == scale)
            .map(|previous| {
                previous
                    .keys
                    .iter()
                    .zip(&previous.icons)
                    .filter_map(|(key, pixels)| Some((key.as_str(), pixels.as_ref()?)))
                    .collect()
            })
            .unwrap_or_default();
        let keys: Vec<String> = apps.iter().map(|app| icon::cache_name(&app.icon)).collect();
        let icons = apps
            .iter()
            .zip(&keys)
            .map(|(app, key)| {
                kept.get(key.as_str()).map(|pixels| Arc::clone(pixels)).or_else(|| render(&app.icon).map(Arc::from))
            })
            .collect();
        Self { apps, icons, scale, history: LaunchHistory::default(), recent: Vec::new(), keys }
    }

    #[must_use]
    pub fn shows_the_same_grid(&self, other: &Self) -> bool {
        self.scale == other.scale
            && self.history == other.history
            && self.apps == other.apps
            && self.icons.len() == other.icons.len()
            && self.icons.iter().zip(&other.icons).all(|pair| match pair {
                (None, None) => true,
                (Some(ours), Some(theirs)) => Arc::ptr_eq(ours, theirs) || ours == theirs,
                _ => false,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(id: &str, icon: &str) -> DesktopApp {
        DesktopApp { id: id.to_string(), name: id.to_string(), icon: icon.to_string(), ..DesktopApp::default() }
    }

    fn rendered(name: &str) -> Option<Vec<u8>> {
        (name != "absent").then(|| name.as_bytes().to_vec())
    }

    #[test]
    fn a_reload_renders_only_the_icons_the_previous_catalogue_lacks() {
        let first = Catalogue::assemble(vec![app("a", "alpha"), app("b", "absent")], 2, None, rendered);
        assert_eq!(first.icons.iter().map(Option::is_some).collect::<Vec<_>>(), [true, false]);

        let mut asked = Vec::new();
        let apps = vec![app("c", "gamma"), app("a", "alpha"), app("b", "absent")];
        let second = Catalogue::assemble(apps, 2, Some(&first), |name| {
            asked.push(name.to_string());
            rendered(name)
        });

        assert_eq!(asked, ["gamma", "absent"], "a missing icon is looked for again, a rendered one is not");
        let (kept, again) = (first.icons[0].as_ref().unwrap(), second.icons[1].as_ref().unwrap());
        assert!(Arc::ptr_eq(kept, again), "the pixels are shared, not copied");
    }

    #[test]
    fn another_scale_renders_every_icon_again() {
        let first = Catalogue::assemble(vec![app("a", "alpha")], 1, None, rendered);
        let mut asked = 0;
        let second = Catalogue::assemble(vec![app("a", "alpha")], 2, Some(&first), |name| {
            asked += 1;
            rendered(name)
        });
        assert_eq!(asked, 1);
        assert!(!first.shows_the_same_grid(&second));
    }

    #[test]
    fn two_scans_of_an_unchanged_disk_show_the_same_grid() {
        let apps = || vec![app("a", "alpha"), app("b", "absent")];
        let first = Catalogue::assemble(apps(), 1, None, rendered);
        let reused = Catalogue::assemble(apps(), 1, Some(&first), rendered);
        let separate = Catalogue::assemble(apps(), 1, None, rendered);
        assert!(first.shows_the_same_grid(&reused));
        assert!(first.shows_the_same_grid(&separate), "equal pixels count even when they are not shared");
    }

    #[test]
    fn a_new_app_a_new_picture_or_a_new_launch_changes_the_grid() {
        let first = Catalogue::assemble(vec![app("a", "alpha")], 1, None, rendered);

        let grown = Catalogue::assemble(vec![app("a", "alpha"), app("b", "beta")], 1, Some(&first), rendered);
        assert!(!first.shows_the_same_grid(&grown));

        let repainted = Catalogue::assemble(vec![app("a", "alpha")], 1, None, |_| Some(vec![7]));
        assert!(!first.shows_the_same_grid(&repainted));

        let mut launched = first.clone();
        launched.history.record("a", 1);
        assert!(!first.shows_the_same_grid(&launched));
    }
}

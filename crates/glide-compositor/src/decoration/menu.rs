use crate::{GlideState, window::GlideWindow};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuItem {
    Minimize,
    ToggleMaximize,
    Close,
    ForceQuit,
}

impl MenuItem {
    pub const ALL: &'static [Self] = &[Self::Minimize, Self::ToggleMaximize, Self::Close, Self::ForceQuit];

    #[must_use]
    pub fn label(self, maximized: bool) -> &'static str {
        use crate::i18n::{Key, t};
        match self {
            Self::Minimize => t(Key::Minimize),
            Self::ToggleMaximize => {
                if maximized {
                    t(Key::Restore)
                } else {
                    t(Key::Maximize)
                }
            }
            Self::Close => t(Key::Close),
            Self::ForceQuit => t(Key::ForceQuit),
        }
    }

    #[must_use]
    pub const fn accel(self) -> Option<&'static str> {
        match self {
            Self::Minimize => Some("Super+M"),
            Self::ToggleMaximize => Some("Super+F"),
            Self::Close => Some("Alt+F4"),
            Self::ForceQuit => None,
        }
    }

    #[must_use]
    pub const fn is_danger(self) -> bool {
        matches!(self, Self::Close | Self::ForceQuit)
    }
}

pub(crate) fn run_menu_item(state: &mut GlideState, window: &GlideWindow, item: MenuItem) {
    let Some(id) = state.window_id(window) else {
        return;
    };
    match item {
        MenuItem::Minimize => {
            tracing::info!(?id, "minimizing via titlebar menu");
            state.minimize_with_flight(id);
        }
        MenuItem::ToggleMaximize => {
            tracing::info!(?id, "toggling maximize via titlebar menu");
            state.morph_geometry(id, |state| {
                state.layout.toggle_maximized(id);
            });
        }
        MenuItem::Close => {
            tracing::info!(?id, "closing via titlebar menu");
            if let Some(toplevel) = window.toplevel() {
                toplevel.send_close();
            }
            state.reconcile_space();
        }
        MenuItem::ForceQuit => {
            tracing::info!(?id, "force quitting via titlebar menu");
            state.force_quit_window(window);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_maximize_label_reflects_the_current_state() {
        assert_ne!(MenuItem::ToggleMaximize.label(false), MenuItem::ToggleMaximize.label(true));
        assert_eq!(MenuItem::ToggleMaximize.label(true), crate::i18n::t(crate::i18n::Key::Restore));
    }

    #[test]
    fn only_closing_and_force_quitting_are_destructive() {
        assert!(!MenuItem::Minimize.is_danger());
        assert!(!MenuItem::ToggleMaximize.is_danger());
        assert!(MenuItem::Close.is_danger());
        assert!(MenuItem::ForceQuit.is_danger());
        assert_eq!(MenuItem::ForceQuit.accel(), None, "nothing is bound to it by default");
    }

    #[test]
    fn every_accelerator_matches_a_real_default_keybind() {
        assert_eq!(MenuItem::Minimize.accel(), Some("Super+M"));
        assert_eq!(MenuItem::ToggleMaximize.accel(), Some("Super+F"));
        assert_eq!(MenuItem::Close.accel(), Some("Alt+F4"));
    }

    #[test]
    fn all_lists_every_variant_exactly_once() {
        assert_eq!(MenuItem::ALL.len(), 4);
        assert_eq!(MenuItem::ALL.last(), Some(&MenuItem::ForceQuit), "the harshest action sits last");
        assert!(MenuItem::ALL.contains(&MenuItem::Minimize));
        assert!(MenuItem::ALL.contains(&MenuItem::ToggleMaximize));
        assert!(MenuItem::ALL.contains(&MenuItem::Close));
    }
}

use crate::{GlideState, window::GlideWindow};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuItem {
    Minimize,
    ToggleMaximize,
    Close,
}

impl MenuItem {
    pub const ALL: &'static [Self] = &[Self::Minimize, Self::ToggleMaximize, Self::Close];

    #[must_use]
    pub fn label(self, maximized: bool) -> &'static str {
        match self {
            Self::Minimize => "Minimize",
            Self::ToggleMaximize => {
                if maximized {
                    "Restore"
                } else {
                    "Maximize"
                }
            }
            Self::Close => "Close",
        }
    }

    /// `Option` is deliberate here, not a mistake the current three
    /// variants all happening to return `Some` — a future item without a
    /// bound key (a snap/always-on-top toggle, say) needs to render with
    /// no accelerator column at all, not an empty one.
    #[must_use]
    #[allow(clippy::unnecessary_wraps)]
    pub const fn accel(self) -> Option<&'static str> {
        match self {
            Self::Minimize => Some("Super+M"),
            Self::ToggleMaximize => Some("Super+F"),
            Self::Close => Some("Alt+F4"),
        }
    }

    #[must_use]
    pub const fn is_danger(self) -> bool {
        matches!(self, Self::Close)
    }
}

pub(crate) fn run_menu_item(state: &mut GlideState, window: &GlideWindow, item: MenuItem) {
    let Some(id) = state.window_id(window) else {
        return;
    };
    match item {
        MenuItem::Minimize => {
            tracing::info!(?id, "minimizing via titlebar menu");
            state.layout.set_minimized(id, true);
        }
        MenuItem::ToggleMaximize => {
            tracing::info!(?id, "toggling maximize via titlebar menu");
            state.layout.toggle_maximized(id);
        }
        MenuItem::Close => {
            tracing::info!(?id, "closing via titlebar menu");
            if let Some(toplevel) = window.toplevel() {
                toplevel.send_close();
            }
        }
    }
    state.reconcile_space();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_maximize_label_reflects_the_current_state() {
        assert_eq!(MenuItem::ToggleMaximize.label(false), "Maximize");
        assert_eq!(MenuItem::ToggleMaximize.label(true), "Restore");
    }

    #[test]
    fn every_item_but_close_is_non_destructive() {
        assert!(!MenuItem::Minimize.is_danger());
        assert!(!MenuItem::ToggleMaximize.is_danger());
        assert!(MenuItem::Close.is_danger());
    }

    #[test]
    fn every_accelerator_matches_a_real_default_keybind() {
        // These are asserted against literal strings, not resolved through
        // config.rs, deliberately: a menu hint that quietly drifts from the
        // keybind it claims to mirror is worse than no hint at all.
        assert_eq!(MenuItem::Minimize.accel(), Some("Super+M"));
        assert_eq!(MenuItem::ToggleMaximize.accel(), Some("Super+F"));
        assert_eq!(MenuItem::Close.accel(), Some("Alt+F4"));
    }

    #[test]
    fn all_lists_every_variant_exactly_once() {
        assert_eq!(MenuItem::ALL.len(), 3);
        assert!(MenuItem::ALL.contains(&MenuItem::Minimize));
        assert!(MenuItem::ALL.contains(&MenuItem::ToggleMaximize));
        assert!(MenuItem::ALL.contains(&MenuItem::Close));
    }
}

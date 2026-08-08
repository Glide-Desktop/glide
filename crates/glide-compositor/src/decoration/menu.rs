use crate::{GlideState, window::GlideWindow};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuItem {
    Minimize,
}

impl MenuItem {
    pub const ALL: &'static [Self] = &[Self::Minimize];

    pub fn label(self) -> &'static str {
        match self {
            Self::Minimize => "Minimize",
        }
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
    }
    state.reconcile_space();
}

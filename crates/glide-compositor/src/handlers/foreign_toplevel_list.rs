use smithay::wayland::foreign_toplevel_list::{
    ForeignToplevelListHandler, ForeignToplevelListState,
};

use crate::GlideState;

impl ForeignToplevelListHandler for GlideState {
    fn foreign_toplevel_list_state(&mut self) -> &mut ForeignToplevelListState {
        &mut self.protocols.foreign_toplevel_list_state
    }
}

#[cfg(test)]
mod tests {
    use crate::GlideState;
    use smithay::reexports::wayland_server::Display;
    use smithay::wayland::foreign_toplevel_list::ForeignToplevelListState;

    #[test]
    fn test_foreign_toplevel_list_state_lifecycle() {
        let display = Display::<GlideState>::new().unwrap();
        let dh = display.handle();

        let mut state = ForeignToplevelListState::new::<GlideState>(&dh);
        let global = state.global();
        assert!(!format!("{global:?}").is_empty());

        let handle = state.new_toplevel::<GlideState>("Initial Title", "com.example.app");
        assert_eq!(handle.title(), "Initial Title");
        assert_eq!(handle.app_id(), "com.example.app");
        assert!(!handle.is_closed());

        handle.send_title("Updated Title");
        assert_eq!(handle.title(), "Updated Title");

        handle.send_app_id("com.example.updated");
        assert_eq!(handle.app_id(), "com.example.updated");

        handle.send_done();

        state.remove_toplevel(&handle);
        assert!(handle.is_closed());
    }
}

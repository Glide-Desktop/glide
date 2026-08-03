use smithay::{
    backend::renderer::utils::on_commit_buffer_handler,
    reexports::wayland_server::{
        Client,
        protocol::{wl_buffer, wl_surface::WlSurface},
    },
    wayland::{
        buffer::BufferHandler,
        compositor::{
            CompositorClientState, CompositorHandler, CompositorState, get_parent,
            is_sync_subsurface,
        },
        shm::{ShmHandler, ShmState},
    },
};

use crate::{
    GlideState,
    grabs::resize_grab,
    state::{ClientState, window_surface},
};

use super::xdg_shell;

impl CompositorHandler for GlideState {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor_state
    }

    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        &client
            .get_data::<ClientState>()
            .expect("every client is inserted with a ClientState in state.rs")
            .compositor_state
    }

    fn commit(&mut self, surface: &WlSurface) {
        on_commit_buffer_handler::<Self>(surface);
        if !is_sync_subsurface(surface) {
            let mut root = surface.clone();
            while let Some(parent) = get_parent(&root) {
                root = parent;
            }
            if let Some(window) = self.space.elements().find(|w| window_surface(w) == &root) {
                window.on_commit();
            }
        }

        // Layer surfaces are not windows: when the commit belongs to one,
        // none of the window-commit handling below applies to it.
        if super::layer_shell::handle_commit(self, surface) {
            return;
        }

        xdg_shell::handle_commit(self, surface);

        // A resize changes both size and (for top/left edges) position in
        // `Space` directly, so the layout has to be re-synced from the result
        // or the next reconcile would undo the resize.
        resize_grab::handle_commit(&mut self.space, surface);
        let resized = self
            .space
            .elements()
            .find(|w| window_surface(w) == surface)
            .cloned();
        if let Some(window) = resized {
            self.record_window_resize(&window);
        }
    }
}

impl BufferHandler for GlideState {
    fn buffer_destroyed(&mut self, _buffer: &wl_buffer::WlBuffer) {}
}

impl ShmHandler for GlideState {
    fn shm_state(&self) -> &ShmState {
        &self.shm_state
    }
}

use smithay::{
    backend::renderer::utils::on_commit_buffer_handler,
    reexports::wayland_server::{
        Client,
        protocol::{wl_buffer, wl_surface::WlSurface},
    },
    utils::{Logical, Point},
    wayland::{
        buffer::BufferHandler,
        compositor::{CompositorClientState, CompositorHandler, CompositorState, get_parent, is_sync_subsurface},
        shm::{ShmHandler, ShmState},
    },
};

use crate::{GlideState, grabs::resize_grab, state::ClientState};

use super::xdg_shell;

impl CompositorHandler for GlideState {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.protocols.compositor_state
    }

    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        &client
            .get_data::<ClientState>()
            .expect("every client is inserted with a ClientState in state.rs")
            .compositor_state
    }

    fn commit(&mut self, surface: &WlSurface) {
        on_commit_buffer_handler::<Self>(surface);
        self.queue_redraw();
        if !is_sync_subsurface(surface) {
            let mut root = surface.clone();
            while let Some(parent) = get_parent(&root) {
                root = parent;
            }
            if let Some(window) = self.mapped_window(&root) {
                window.on_commit();
            }
        }

        if super::layer_shell::handle_commit(self, surface) {
            return;
        }

        xdg_shell::handle_commit(self, surface);

        let location_before = self.mapped_location(surface);
        resize_grab::handle_commit(&mut self.space, surface);
        let location_after = self.mapped_location(surface);
        if let Some(window) = self.mapped_window(surface) {
            let moved_to = location_after.filter(|_| location_after != location_before);
            self.record_window_resize(&window, moved_to);
        }
    }
}

impl GlideState {
    fn mapped_location(&self, surface: &WlSurface) -> Option<Point<i32, Logical>> {
        self.mapped_window(surface).and_then(|window| self.space.element_location(&window))
    }
}

impl BufferHandler for GlideState {
    fn buffer_destroyed(&mut self, _buffer: &wl_buffer::WlBuffer) {}
}

impl ShmHandler for GlideState {
    fn shm_state(&self) -> &ShmState {
        &self.protocols.shm_state
    }
}

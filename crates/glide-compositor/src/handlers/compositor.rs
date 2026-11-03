use smithay::{
    backend::renderer::utils::on_commit_buffer_handler,
    reexports::{
        calloop::{EventSource, Interest},
        wayland_server::{
            Client, Resource,
            protocol::{wl_buffer, wl_surface::WlSurface},
        },
    },
    utils::{Logical, Point},
    wayland::{
        buffer::BufferHandler,
        compositor::{
            BufferAssignment, CompositorClientState, CompositorHandler, CompositorState, SurfaceAttributes,
            add_blocker, add_destruction_hook, add_pre_commit_hook, get_parent, is_sync_subsurface, with_states,
        },
        dmabuf::get_dmabuf,
        drm_syncobj::DrmSyncobjCachedState,
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

    fn new_surface(&mut self, surface: &WlSurface) {
        add_destruction_hook::<Self, _>(surface, |state: &mut Self, surface: &WlSurface| {
            if state.window_id_for_surface(surface).is_some() {
                state.untrack_window(surface);
            }
        });
        add_pre_commit_hook::<Self, _>(surface, |state: &mut Self, _display, surface: &WlSurface| {
            state.wait_for_buffer(surface);
        });
    }

    fn destroyed(&mut self, surface: &WlSurface) {
        if self.window_id_for_surface(surface).is_some() {
            self.untrack_window(surface);
        }
    }

    fn commit(&mut self, surface: &WlSurface) {
        let unmapping = self.window_id_for_surface(surface).filter(|_| {
            with_states(surface, |states| {
                matches!(
                    states.cached_state.get::<SurfaceAttributes>().current().buffer,
                    Some(BufferAssignment::Removed)
                )
            })
        });
        if let Some(id) = unmapping
            && let Some(window) = self.window_map.get(&id).cloned()
        {
            self.capture_closing_window(id, &window);
        }

        on_commit_buffer_handler::<Self>(surface);
        if let Some(id) = unmapping {
            self.on_window_unmapped(id);
        }
        self.queue_redraw();
        if self.drag.icon.as_ref() == Some(surface)
            && let Some(delta) = with_states(surface, |states| {
                states.cached_state.get::<SurfaceAttributes>().current().buffer_delta.take()
            })
        {
            self.drag.move_icon(delta);
        }
        if !is_sync_subsurface(surface) {
            let mut root = surface.clone();
            while let Some(parent) = get_parent(&root) {
                root = parent;
            }
            if let Some(window) = self.window_id_for_surface(&root).and_then(|id| self.window_map.get(&id)) {
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
    fn wait_for_buffer(&mut self, surface: &WlSurface) {
        let mut acquire = None;
        let dmabuf = with_states(surface, |states| {
            acquire.clone_from(&states.cached_state.get::<DrmSyncobjCachedState>().pending().acquire_point);
            match states.cached_state.get::<SurfaceAttributes>().pending().buffer.as_ref() {
                Some(BufferAssignment::NewBuffer(buffer)) => get_dmabuf(buffer).cloned().ok(),
                _ => None,
            }
        });
        let Some(dmabuf) = dmabuf else {
            return;
        };
        let Some(client) = surface.client() else {
            return;
        };
        if let Some(acquire) = acquire {
            match acquire.generate_blocker() {
                Ok((blocker, source)) => {
                    if self.unblock_when(source, client).is_ok() {
                        add_blocker(surface, blocker);
                    }
                    return;
                }
                Err(err) => tracing::debug!(target: "glide::gpu", ?err, "cannot wait on an acquire point"),
            }
        }
        match dmabuf.generate_blocker(Interest::READ) {
            Ok((blocker, source)) => {
                if self.unblock_when(source, client).is_ok() {
                    add_blocker(surface, blocker);
                }
            }
            Err(err) => tracing::trace!(target: "glide::gpu", ?err, "no implicit fence to wait on"),
        }
    }

    fn unblock_when<S>(&self, source: S, client: Client) -> Result<(), ()>
    where
        S: EventSource<Event = (), Ret = Result<(), std::io::Error>> + 'static,
    {
        self.loop_handle
            .insert_source(source, move |(), _, state| {
                let display = state.display_handle.clone();
                state.client_compositor_state(&client).blocker_cleared(state, &display);
                Ok(())
            })
            .map(drop)
            .map_err(|err| tracing::warn!(target: "glide::gpu", ?err, "cannot wait for a client buffer"))
    }

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

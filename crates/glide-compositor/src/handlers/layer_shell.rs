use smithay::{
    desktop::{LayerSurface, layer_map_for_output},
    output::Output,
    reexports::wayland_server::protocol::{wl_output::WlOutput, wl_surface::WlSurface},
    wayland::shell::wlr_layer::{
        Layer, LayerSurface as WlrLayerSurface, LayerSurfaceData, WlrLayerShellHandler,
        WlrLayerShellState,
    },
};

use crate::GlideState;

impl WlrLayerShellHandler for GlideState {
    fn shell_state(&mut self) -> &mut WlrLayerShellState {
        &mut self.protocols.layer_shell_state
    }

    fn new_layer_surface(
        &mut self,
        surface: WlrLayerSurface,
        wl_output: Option<WlOutput>,
        _layer: Layer,
        namespace: String,
    ) {
        let output = wl_output
            .as_ref()
            .and_then(Output::from_resource)
            .or_else(|| self.space.outputs().next().cloned());
        let Some(output) = output else {
            tracing::warn!(
                namespace,
                "layer surface arrived with no output to map it on"
            );
            return;
        };

        tracing::info!(namespace, "new layer surface");
        let layer = LayerSurface::new(surface, namespace);
        if let Err(err) = layer_map_for_output(&output).map_layer(&layer) {
            tracing::warn!(?err, "failed to map layer surface");
            return;
        }

        if layer.can_receive_keyboard_focus() {
            self.layer_focus = Some(layer);
            self.apply_keyboard_focus_public();
        }
        self.queue_redraw();
    }

    fn layer_destroyed(&mut self, surface: WlrLayerSurface) {
        let layer_and_output = self.space.outputs().find_map(|output| {
            let map = layer_map_for_output(output);
            map.layers()
                .find(|&layer| layer.layer_surface() == &surface)
                .cloned()
                .map(|layer| (layer, output.clone()))
        });

        if let Some((layer, output)) = layer_and_output {
            layer_map_for_output(&output).unmap_layer(&layer);

            if self.layer_focus.as_ref() == Some(&layer) {
                self.layer_focus = None;
                self.apply_keyboard_focus_public();
            }

            self.sync_layout_outputs();
            self.reconcile_space();
        }
    }
}

pub fn handle_commit(state: &mut GlideState, surface: &WlSurface) -> bool {
    let output = state
        .space
        .outputs()
        .find(|o| {
            let map = layer_map_for_output(o);
            map.layer_for_surface(surface, smithay::desktop::WindowSurfaceType::TOPLEVEL)
                .is_some()
        })
        .cloned();
    let Some(output) = output else {
        return false;
    };

    let initial_configure_sent = smithay::wayland::compositor::with_states(surface, |states| {
        states
            .data_map
            .get::<LayerSurfaceData>()
            .is_none_or(|data| {
                data.lock()
                    .expect("single-threaded event loop, this mutex is never poisoned")
                    .initial_configure_sent
            })
    });

    let changed = {
        let mut map = layer_map_for_output(&output);
        let changed = map.arrange();

        if !initial_configure_sent
            && let Some(layer) =
                map.layer_for_surface(surface, smithay::desktop::WindowSurfaceType::TOPLEVEL)
        {
            layer.layer_surface().send_configure();
        }

        changed
    };

    if changed {
        state.sync_layout_outputs();
        state.reconcile_space();
    }

    true
}

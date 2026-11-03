use smithay::{
    desktop::{LayerSurface, WindowSurfaceType, layer_map_for_output},
    output::Output,
    reexports::wayland_server::protocol::{wl_output::WlOutput, wl_surface::WlSurface},
    wayland::shell::wlr_layer::{
        Layer, LayerSurface as WlrLayerSurface, LayerSurfaceData, WlrLayerShellHandler, WlrLayerShellState,
    },
};

use crate::GlideState;

impl WlrLayerShellHandler for GlideState {
    fn shell_state(&mut self) -> &mut WlrLayerShellState {
        &mut self.protocols.layer_shell_state
    }

    fn new_layer_surface(
        &mut self, surface: WlrLayerSurface, wl_output: Option<WlOutput>, _layer: Layer, namespace: String,
    ) {
        let output =
            wl_output.as_ref().and_then(Output::from_resource).or_else(|| self.space.outputs().next().cloned());
        let Some(output) = output else {
            tracing::warn!(namespace, "layer surface arrived with no output to map it on");
            return;
        };

        tracing::info!(namespace, "new layer surface");
        let layer = LayerSurface::new(surface, namespace);
        if let Err(err) = layer_map_for_output(&output).map_layer(&layer) {
            tracing::warn!(?err, "failed to map layer surface");
            return;
        }
        self.queue_redraw();
    }

    fn layer_destroyed(&mut self, surface: WlrLayerSurface) {
        let layer_and_output = self.space.outputs().find_map(|output| {
            let map = layer_map_for_output(output);
            map.layers().find(|&layer| layer.layer_surface() == &surface).cloned().map(|layer| (layer, output.clone()))
        });

        if let Some((layer, output)) = layer_and_output {
            layer_map_for_output(&output).unmap_layer(&layer);
            self.session.mapped_layers.remove(layer.wl_surface());
            if self.layer_focus.as_ref() == Some(&layer) {
                self.layer_focus = None;
            }
            self.sync_layout_outputs();
            self.reconcile_space();
        }
        self.session.mapped_layers.retain(smithay::utils::IsAlive::alive);
    }
}

impl GlideState {
    fn sync_layer_focus(&mut self, layer: &LayerSurface) {
        let surface = layer.wl_surface();
        let mapped =
            smithay::backend::renderer::utils::with_renderer_surface_state(surface, |state| state.buffer().is_some())
                .unwrap_or(false);
        let was_mapped = self.session.mapped_layers.contains(surface);
        if mapped == was_mapped {
            self.apply_keyboard_focus();
            return;
        }
        if mapped {
            self.session.mapped_layers.insert(surface.clone());
            if !self.screen_locked()
                && matches!(layer.layer(), Layer::Top | Layer::Overlay)
                && layer.can_receive_keyboard_focus()
            {
                self.layer_focus = Some(layer.clone());
            }
        } else {
            self.session.mapped_layers.remove(surface);
            if self.layer_focus.as_ref() == Some(layer) {
                self.layer_focus = None;
            }
        }
        self.reconcile_space();
    }
}

pub fn handle_commit(state: &mut GlideState, surface: &WlSurface) -> bool {
    let Some(output) = state
        .space
        .outputs()
        .find(|o| layer_map_for_output(o).layer_for_surface(surface, WindowSurfaceType::TOPLEVEL).is_some())
        .cloned()
    else {
        return false;
    };

    let initial_configure_sent = smithay::wayland::compositor::with_states(surface, |states| {
        states.data_map.get::<LayerSurfaceData>().is_none_or(|data| {
            data.lock().expect("single-threaded event loop, this mutex is never poisoned").initial_configure_sent
        })
    });

    let mut map = layer_map_for_output(&output);
    let changed = map.arrange();
    let layer = map.layer_for_surface(surface, WindowSurfaceType::TOPLEVEL).cloned();
    if !initial_configure_sent && let Some(layer) = layer.as_ref() {
        layer.layer_surface().send_configure();
    }
    drop(map);

    if changed {
        state.sync_layout_outputs();
        state.reconcile_space();
    }
    if let Some(layer) = layer {
        state.sync_layer_focus(&layer);
    }

    true
}

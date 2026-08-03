//! `wlr-layer-shell`: panels, launchers, wallpapers, notifications.
//!
//! Layer surfaces are deliberately *not* windows: they never enter
//! `glide-layout`, have no workspace, and no decoration. Their entire
//! layout contract is Smithay's per-output [`LayerMap`] — `arrange()`
//! positions them from their own anchors/margins, and the space they claim
//! via exclusive zones reaches the layout engine only as a shrunken
//! `work_area` through `sync_layout_outputs`.
//!
//! [`LayerMap`]: smithay::desktop::LayerMap

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
        &mut self.layer_shell_state
    }

    fn new_layer_surface(
        &mut self,
        surface: WlrLayerSurface,
        wl_output: Option<WlOutput>,
        _layer: Layer,
        namespace: String,
    ) {
        // A client may name an output; without one, ours is the primary
        // (Glide is single-output for now, so "the first" is "the one").
        let output = wl_output
            .as_ref()
            .and_then(Output::from_resource)
            .or_else(|| self.space.outputs().next().cloned());
        let Some(output) = output else {
            // No output mapped at all (startup race, headless). The surface
            // is dropped; the client sees `closed` and can retry on the
            // next output announcement.
            tracing::warn!(
                namespace,
                "layer surface arrived with no output to map it on"
            );
            return;
        };

        tracing::info!(namespace, "new layer surface");
        if let Err(err) =
            layer_map_for_output(&output).map_layer(&LayerSurface::new(surface, namespace))
        {
            tracing::warn!(?err, "failed to map layer surface");
        }
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

            // The zone the surface reserved is free again; without this a
            // dead panel would keep its strip of the work area forever.
            self.sync_layout_outputs();
            self.reconcile_space();
        }
    }
}

/// Layer-surface part of the commit hook: arranges the map and sends the
/// initial configure. Returns `true` if `surface` was a layer surface (so
/// the caller can skip its window-commit handling).
///
/// Arranging on *every* commit, not only the first, is what keeps exclusive
/// zones honest: a panel resizing itself changes the zone it claims, and
/// the work area has to follow.
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
        // Arrange before the initial configure so the size we offer the
        // client already respects its anchors and exclusive zone.
        let changed = map.arrange();

        if !initial_configure_sent
            && let Some(layer) =
                map.layer_for_surface(surface, smithay::desktop::WindowSurfaceType::TOPLEVEL)
        {
            layer.layer_surface().send_configure();
        }

        changed
        // The map guard drops here — before sync/reconcile below, which
        // take it again. Holding it across those calls would deadlock the
        // single-threaded loop on its own mutex.
    };

    if changed {
        state.sync_layout_outputs();
        state.reconcile_space();
    }

    true
}

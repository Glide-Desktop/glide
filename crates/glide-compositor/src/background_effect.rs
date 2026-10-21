use smithay::desktop::Space;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Rectangle};
use smithay::wayland::background_effect::{BackgroundEffectSurfaceCachedState, Capability, ExtBackgroundEffectHandler};
use smithay::wayland::compositor::{self, RectangleKind, RegionAttributes};

use crate::state::GlideState;
use crate::window::GlideWindow;

impl ExtBackgroundEffectHandler for GlideState {
    fn capabilities(&self) -> Capability {
        Capability::Blur
    }

    fn set_blur_region(&mut self, _wl_surface: WlSurface, _region: RegionAttributes) {
        self.queue_redraw();
    }

    fn unset_blur_region(&mut self, _wl_surface: WlSurface) {
        self.queue_redraw();
    }
}

#[must_use]
pub fn region_rects(region: &RegionAttributes, bounds: Rectangle<i32, Logical>) -> Vec<Rectangle<i32, Logical>> {
    let mut rects: Vec<Rectangle<i32, Logical>> = Vec::new();
    for (kind, rect) in &region.rects {
        match kind {
            RectangleKind::Add => rects.extend(rect.intersection(bounds)),
            RectangleKind::Subtract => {
                rects = rects.into_iter().flat_map(|kept| kept.subtract_rect(*rect)).collect();
            }
        }
    }
    rects.retain(|rect| !rect.is_empty());
    rects
}

#[must_use]
pub fn blur_rects(space: &Space<GlideWindow>, area: Rectangle<i32, Logical>) -> Vec<Rectangle<i32, Logical>> {
    let mut out = Vec::new();
    for window in space.elements() {
        if window.is_animating_geometry() {
            continue;
        }
        let Some(toplevel) = window.toplevel() else {
            continue;
        };
        let Some(location) = space.element_location(window) else {
            continue;
        };
        let geometry = window.geometry();
        let region = compositor::with_states(toplevel.wl_surface(), |states| {
            states.cached_state.get::<BackgroundEffectSurfaceCachedState>().current().blur_region.clone()
        });
        let Some(region) = region else {
            continue;
        };
        let origin = location - geometry.loc;
        let bounds = Rectangle::new(geometry.loc, geometry.size);
        out.extend(
            region_rects(&region, bounds)
                .into_iter()
                .map(|rect| Rectangle::new(rect.loc + origin, rect.size))
                .filter_map(|rect| rect.intersection(area)),
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blur_region_is_its_added_rects_minus_the_subtracted_ones_inside_the_window() {
        let region = RegionAttributes {
            rects: vec![
                (RectangleKind::Add, Rectangle::new((0, 0).into(), (100, 100).into())),
                (RectangleKind::Subtract, Rectangle::new((0, 0).into(), (100, 20).into())),
            ],
        };
        let bounds = Rectangle::new((0, 0).into(), (80, 200).into());
        let rects = region_rects(&region, bounds);
        let area: i32 = rects.iter().map(|rect| rect.size.w * rect.size.h).sum();
        assert_eq!(area, 80 * 80);
        assert!(rects.iter().all(|rect| rect.loc.y >= 20 && rect.loc.x + rect.size.w <= 80));
    }
}

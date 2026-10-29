pub use glide_overlays::overview::*;

use smithay::{
    backend::renderer::{
        ImportAll, ImportMem, Renderer,
        element::utils::{Relocate, RelocateRenderElement, RescaleRenderElement},
    },
    utils::{Logical, Point, Scale},
};

use crate::{
    clip::ClipCapableRenderer,
    raster::ShellRenderer,
    window::{GlideWindow, GlideWindowRenderElement},
};

smithay::render_elements! {
    pub OverviewRenderElement<R> where R: ImportAll + ImportMem + ClipCapableRenderer;
    Preview = RelocateRenderElement<RescaleRenderElement<GlideWindowRenderElement<R>>>,
    Chrome = OverviewElement<R>,
}

impl<R: Renderer> std::fmt::Debug for OverviewRenderElement<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Preview(e) => f.debug_tuple("Preview").field(e).finish(),
            Self::Chrome(e) => f.debug_tuple("Chrome").field(e).finish(),
            Self::_GenericCatcher(e) => f.debug_tuple("_GenericCatcher").field(e).finish(),
        }
    }
}

fn window_elements<R>(
    renderer: &mut R, window: &GlideWindow, draw: &WindowDraw, scale: Scale<f64>,
) -> impl Iterator<Item = OverviewRenderElement<R>>
where
    R: ShellRenderer + ClipCapableRenderer,
    R::Error: std::fmt::Debug,
{
    let frame = draw.frame;
    let factor = draw.rect.size.w / f64::from(frame.size.w.max(1));
    let buffer_offset = window.geometry().loc;
    let content_origin = Point::<i32, Logical>::from((
        frame.loc.x - buffer_offset.x,
        frame.loc.y + window.header_height() - buffer_offset.y,
    ));
    let real = content_origin.to_f64().to_physical(scale).to_i32_round();
    let anchor = frame.loc.to_f64().to_physical(scale).to_i32_round();
    let target = draw.rect.loc.to_physical(scale).to_i32_round();
    let shape = crate::window::PreviewShape { radius: draw.radius, scale: factor };
    window.preview_render_elements(renderer, real, scale, draw.alpha, shape).into_iter().map(move |element| {
        let scaled = RescaleRenderElement::from_element(element, anchor, factor);
        OverviewRenderElement::Preview(RelocateRenderElement::from_element(scaled, target - anchor, Relocate::Relative))
    })
}

pub fn render<'a, R, F>(
    ui: &mut OverviewUi, renderer: &mut R, frame: Frame<'_, F>,
    window: impl Fn(glide_layout::WindowId) -> Option<&'a GlideWindow>,
) -> Vec<OverviewRenderElement<R>>
where
    R: ShellRenderer + ClipCapableRenderer,
    R::Error: std::fmt::Debug,
    F: FnMut(glide_layout::WindowId) -> Option<TileInfo>,
{
    let scale = frame.scale;
    let layers = ui.render(renderer, frame);
    let mut elements = Vec::with_capacity(layers.len());
    for layer in layers {
        match layer {
            Layer::Chrome(element) => elements.push(OverviewRenderElement::Chrome(*element)),
            Layer::Window(draw) => {
                if let Some(glide) = window(draw.id) {
                    elements.extend(window_elements(renderer, glide, &draw, scale));
                }
            }
        }
    }
    elements
}

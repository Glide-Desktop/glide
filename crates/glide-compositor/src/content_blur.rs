use std::cell::RefCell;

use smithay::backend::renderer::element::surface::{WaylandSurfaceRenderElement, WaylandSurfaceTexture};
use smithay::backend::renderer::element::{Element, Id, Kind, RenderElement, UnderlyingStorage};
use smithay::backend::renderer::utils::{CommitCounter, DamageSet, OpaqueRegions};
use smithay::backend::renderer::{ImportAll, Renderer, Texture};
use smithay::utils::{Buffer as BufferCoord, Logical, Physical, Point, Rectangle, Scale, Size, Transform};

use crate::clip::{ClipCapableRenderer, RoundedClip};
use crate::window_traits::BlurRegion;

pub const BLUR_RADIUS: f64 = 24.0;
const BLUR_NAMESPACE: usize = 0xB10B_0000;

pub const BLUR_SHADER: &str = r"#version 100

//_DEFINES_

#if defined(EXTERNAL)
#extension GL_OES_EGL_image_external : require
#endif

precision highp float;
#if defined(EXTERNAL)
uniform samplerExternalOES tex;
#else
uniform sampler2D tex;
#endif

uniform float alpha;
varying vec2 v_coords;

uniform vec2 unit_scale;
uniform vec2 unit_offset;
uniform vec2 dst_size;
uniform vec4 clip_rect;
uniform vec4 clip_radii;
uniform vec2 blur_step;

#if defined(DEBUG_FLAGS)
uniform float tint;
#endif

void main() {
    vec4 sum = vec4(0.0);
    float weight = 0.0;
    for (int i = 0; i < 48; i++) {
        float fi = float(i);
        float r = sqrt((fi + 0.5) / 48.0);
        float a = fi * 2.39996323;
        vec2 offset = vec2(cos(a), sin(a)) * r;
        float w = exp(-2.0 * r * r);
        sum += texture2D(tex, v_coords + offset * blur_step) * w;
        weight += w;
    }
    vec4 color = sum / weight;
    float luma = dot(color.rgb, vec3(0.2126, 0.7152, 0.0722));
    color.rgb = clamp(mix(vec3(luma), color.rgb, 1.4), 0.0, color.a);

#if defined(NO_ALPHA)
    color = vec4(color.rgb, 1.0) * alpha;
#else
    color = color * alpha;
#endif

#if defined(DEBUG_FLAGS)
    if (tint == 1.0)
        color = vec4(0.0, 0.2, 0.0, 0.2) + color * 0.8;
#endif

    vec2 unit = v_coords * unit_scale + unit_offset;
    vec2 p = unit * dst_size - clip_rect.xy;
    vec2 half_size = clip_rect.zw * 0.5;
    vec2 d = p - half_size;
    float rr = (d.x < 0.0)
        ? ((d.y < 0.0) ? clip_radii.x : clip_radii.w)
        : ((d.y < 0.0) ? clip_radii.y : clip_radii.z);
    rr = min(rr, min(half_size.x, half_size.y));
    vec2 q = abs(d) - (half_size - vec2(rr));
    float dist = length(max(q, vec2(0.0))) - rr;

    gl_FragColor = color * clamp(0.5 - dist, 0.0, 1.0);
}
";

#[derive(Debug, Default)]
pub struct ContentBlur(RefCell<Vec<BlurRegion>>);

pub fn set_regions(window: &crate::window::GlideWindow, regions: &[BlurRegion]) {
    window.user_data().insert_if_missing(ContentBlur::default);
    if let Some(blur) = window.user_data().get::<ContentBlur>() {
        regions.clone_into(&mut blur.0.borrow_mut());
    }
}

#[must_use]
pub fn regions(window: &crate::window::GlideWindow) -> Vec<BlurRegion> {
    window.user_data().get::<ContentBlur>().map(|blur| blur.0.borrow().clone()).unwrap_or_default()
}

#[must_use]
pub fn region_rect(origin: Point<i32, Physical>, region: &BlurRegion, scale: Scale<f64>) -> Rectangle<i32, Physical> {
    let logical: Rectangle<i32, Logical> =
        Rectangle::new(Point::from((region.x, region.y)), Size::from((region.width, region.height)));
    let physical = logical.to_physical_precise_round(scale);
    Rectangle::new(origin + physical.loc, physical.size)
}

#[must_use]
pub fn crop(
    geometry: Rectangle<i32, Physical>, src: Rectangle<f64, BufferCoord>, inside: Rectangle<i32, Physical>,
) -> Option<Rectangle<f64, BufferCoord>> {
    let area = geometry.intersection(inside)?;
    if geometry.size.w <= 0 || geometry.size.h <= 0 || area.is_empty() {
        return None;
    }
    let (kx, ky) = (src.size.w / f64::from(geometry.size.w), src.size.h / f64::from(geometry.size.h));
    let x = src.loc.x + f64::from(area.loc.x - geometry.loc.x) * kx;
    let y = src.loc.y + f64::from(area.loc.y - geometry.loc.y) * ky;
    Some(Rectangle::new(Point::from((x, y)), Size::from((f64::from(area.size.w) * kx, f64::from(area.size.h) * ky))))
}

#[derive(Debug)]
pub struct Blurred<R: Renderer> {
    inner: WaylandSurfaceRenderElement<R>,
    src: Rectangle<f64, BufferCoord>,
    dst: Rectangle<i32, Physical>,
    clip: RoundedClip,
    step: (f64, f64),
    id: Id,
}

impl<R: Renderer + ImportAll> Blurred<R>
where
    R::TextureId: Texture + 'static,
{
    #[must_use]
    pub fn new(
        inner: WaylandSurfaceRenderElement<R>, region: Rectangle<i32, Physical>, radius: f32, scale: Scale<f64>,
        index: usize,
    ) -> Option<Self> {
        if inner.transform() != Transform::Normal {
            return None;
        }
        let geometry = inner.geometry(scale);
        let src = crop(geometry, inner.src(), region)?;
        let dst = geometry.intersection(region)?;
        let clip = RoundedClip::from_frame(dst, region, radius.max(0.01))?;
        let radius_px = BLUR_RADIUS * scale.x;
        let full = inner.src();
        let step = (
            radius_px * full.size.w / f64::from(geometry.size.w.max(1)),
            radius_px * full.size.h / f64::from(geometry.size.h.max(1)),
        );
        let id = inner.id().namespaced(BLUR_NAMESPACE + index);
        Some(Self { inner, src, dst, clip, step, id })
    }
}

impl<R: Renderer + ImportAll> Element for Blurred<R>
where
    R::TextureId: Texture + 'static,
{
    fn id(&self) -> &Id {
        &self.id
    }

    fn current_commit(&self) -> CommitCounter {
        self.inner.current_commit()
    }

    fn geometry(&self, _scale: Scale<f64>) -> Rectangle<i32, Physical> {
        self.dst
    }

    fn src(&self) -> Rectangle<f64, BufferCoord> {
        self.src
    }

    fn transform(&self) -> Transform {
        Transform::Normal
    }

    fn damage_since(&self, _scale: Scale<f64>, commit: Option<CommitCounter>) -> DamageSet<i32, Physical> {
        if commit == Some(self.current_commit()) {
            DamageSet::default()
        } else {
            DamageSet::from_slice(&[Rectangle::from_size(self.dst.size)])
        }
    }

    fn opaque_regions(&self, _scale: Scale<f64>) -> OpaqueRegions<i32, Physical> {
        OpaqueRegions::default()
    }

    fn alpha(&self) -> f32 {
        self.inner.alpha()
    }

    fn kind(&self) -> Kind {
        Kind::Unspecified
    }
}

impl<R> RenderElement<R> for Blurred<R>
where
    R: Renderer + ImportAll + ClipCapableRenderer,
    R::TextureId: Texture + 'static,
{
    fn underlying_storage(&self, _renderer: &mut R) -> Option<UnderlyingStorage<'_>> {
        None
    }

    fn draw(
        &self, frame: &mut R::Frame<'_, '_>, src: Rectangle<f64, BufferCoord>, dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>], _opaque_regions: &[Rectangle<i32, Physical>],
        _cache: Option<&smithay::utils::user_data::UserDataMap>,
    ) -> Result<(), R::Error> {
        match self.inner.texture() {
            WaylandSurfaceTexture::Texture(texture) => {
                R::draw_blurred(frame, texture, src, dst, damage, self.alpha(), &self.clip, self.step)
            }
            WaylandSurfaceTexture::SolidColor(_) => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: i32, y: i32, width: i32, height: i32) -> Rectangle<i32, Physical> {
        Rectangle::new(Point::from((x, y)), Size::from((width, height)))
    }

    #[test]
    fn the_crop_follows_the_part_of_the_surface_under_the_region() {
        let geometry = rect(100, 50, 1280, 720);
        let src = Rectangle::new(Point::from((0.0, 0.0)), Size::from((1920.0, 1080.0)));
        let cropped = crop(geometry, src, rect(420, 664, 640, 82)).unwrap();
        assert!((cropped.loc.x - 480.0).abs() < 1e-6, "{cropped:?}");
        assert!((cropped.loc.y - 921.0).abs() < 1e-6);
        assert!((cropped.size.w - 960.0).abs() < 1e-6);
        assert!((cropped.size.h - 123.0).abs() < 1e-6);
        assert_eq!(crop(geometry, src, rect(0, 0, 50, 40)), None, "a region outside the surface blurs nothing");
    }

    #[test]
    fn regions_are_placed_from_the_surface_origin_at_the_output_scale() {
        let region = BlurRegion { x: 320, y: 614, width: 640, height: 82, radius: 12 };
        assert_eq!(region_rect(Point::from((10, 20)), &region, Scale::from(2.0)), rect(650, 1248, 1280, 164));
    }
}

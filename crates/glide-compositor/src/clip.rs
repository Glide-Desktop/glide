//! Rounded-corner clipping of client surface content.
//!
//! Glide draws a window's titlebar and its shadow/border ring itself, and
//! both are rounded. The client's own content, however, is a plain
//! rectangle, and nothing used to cut its corners — so on a decorated
//! window the square content corners poked out past the rounded border
//! curve by roughly `0.29 × radius`. The titlebar hid the top two; the
//! bottom two were plainly visible.
//!
//! Covering them is impossible with ordinary blending, because the pixels
//! that belong there are whatever sits *behind* the window. The corners
//! have to be cut at the moment the surface texture is sampled, which
//! means a fragment shader — this module.
//!
//! Backends opt in through [`ClipCapableRenderer`]. Only the GLES renderer
//! implements it; everything degrades to an ordinary unclipped draw when
//! the shader is unavailable, so a compile failure costs the corners and
//! nothing else.

use smithay::{
    backend::renderer::{
        Renderer, Texture,
        element::{
            Element, Id, Kind, RenderElement, UnderlyingStorage,
            surface::WaylandSurfaceRenderElement,
        },
        gles::{
            GlesError, GlesFrame, GlesRenderer, GlesTexProgram, Uniform, UniformName, UniformType,
        },
        utils::{CommitCounter, DamageSet, OpaqueRegions},
    },
    utils::{Buffer as BufferCoord, Physical, Point, Rectangle, Scale, Transform},
};

/// Clips the sampled texture to a rounded rectangle.
///
/// `v_coords` is the only varying the fixed vertex shader hands us, and it
/// spans the element's `src` region in normalised *texture* space — not the
/// destination quad. `unit_scale`/`unit_offset` convert it back into a
/// `0..1` span across the destination, from which `dst_size` recovers
/// destination-local pixels. Doing the arithmetic this way keeps the clip
/// correct for surfaces that draw a cropped sub-region of their buffer
/// (viewporter), instead of assuming `src` always covers the whole thing.
const ROUNDED_CLIP_SHADER: &str = r"#version 100

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

#if defined(DEBUG_FLAGS)
uniform float tint;
#endif

void main() {
    vec4 color = texture2D(tex, v_coords);

#if defined(NO_ALPHA)
    color = vec4(color.rgb, 1.0) * alpha;
#else
    color = color * alpha;
#endif

#if defined(DEBUG_FLAGS)
    if (tint == 1.0)
        color = vec4(0.0, 0.2, 0.0, 0.2) + color * 0.8;
#endif

    // Destination-local pixel position of this fragment.
    vec2 unit = v_coords * unit_scale + unit_offset;
    vec2 p = unit * dst_size - clip_rect.xy;

    vec2 half_size = clip_rect.zw * 0.5;
    vec2 d = p - half_size;

    // Per-corner radius: pick by quadrant (TL, TR, BR, BL).
    float r = (d.x < 0.0)
        ? ((d.y < 0.0) ? clip_radii.x : clip_radii.w)
        : ((d.y < 0.0) ? clip_radii.y : clip_radii.z);
    r = min(r, min(half_size.x, half_size.y));

    // Signed distance to a rounded rectangle; negative inside.
    vec2 q = abs(d) - (half_size - vec2(r));
    float dist = length(max(q, vec2(0.0))) - r;

    // One-pixel anti-aliased edge.
    gl_FragColor = color * clamp(0.5 - dist, 0.0, 1.0);
}
";

const UNIFORM_UNIT_SCALE: &str = "unit_scale";
const UNIFORM_UNIT_OFFSET: &str = "unit_offset";
const UNIFORM_DST_SIZE: &str = "dst_size";
const UNIFORM_CLIP_RECT: &str = "clip_rect";
const UNIFORM_CLIP_RADII: &str = "clip_radii";

/// The rounded rectangle a surface's content is cut against, stored
/// **relative to the element's own destination rect**.
///
/// Absolute pixels would not survive the overview: previews are wrapped in
/// `RescaleRenderElement`/`RelocateRenderElement`, so the `dst` handed to
/// `draw` is the scaled-and-moved rect while the window's frame rect was
/// measured before any of that. Keeping the clip proportional means the
/// same value is correct at 1:1 on the desktop and at any preview scale,
/// and the corner radius shrinks with the preview exactly as a CSS
/// `transform: scale()` would shrink it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoundedClip {
    /// Clip-rect origin as a fraction of the element's destination size,
    /// measured from that destination's own origin. Negative when the
    /// window frame starts above/left of this particular surface (which is
    /// the normal case: the frame includes the titlebar, the surface does
    /// not).
    offset: [f32; 2],
    /// Clip-rect size as a fraction of the element's destination size.
    size: [f32; 2],
    /// Corner radii as a fraction of the clip rect's height, ordered
    /// top-left, top-right, bottom-right, bottom-left.
    radii: [f32; 4],
}

impl RoundedClip {
    /// Builds a clip for an element whose destination is `dst`, cutting to
    /// `frame` (both in the same physical-pixel space) with a uniform
    /// `radius_physical` on all four corners.
    #[must_use]
    pub fn from_frame(
        dst: Rectangle<i32, Physical>,
        frame: Rectangle<i32, Physical>,
        radius_physical: f32,
    ) -> Option<Self> {
        if dst.size.is_empty() || frame.size.is_empty() || radius_physical <= 0.0 {
            return None;
        }

        #[allow(clippy::cast_precision_loss)]
        let (dst_w, dst_h) = (dst.size.w as f32, dst.size.h as f32);
        #[allow(clippy::cast_precision_loss)]
        let (frame_w, frame_h) = (frame.size.w as f32, frame.size.h as f32);
        #[allow(clippy::cast_precision_loss)]
        let offset = [
            (frame.loc.x - dst.loc.x) as f32 / dst_w,
            (frame.loc.y - dst.loc.y) as f32 / dst_h,
        ];

        Some(Self {
            offset,
            size: [frame_w / dst_w, frame_h / dst_h],
            radii: [radius_physical / frame_h; 4],
        })
    }

    /// Resolves the clip against a concrete destination rect, yielding
    /// `(rect_x, rect_y, rect_w, rect_h)` in destination-local px plus the
    /// four radii in px.
    fn resolve(&self, dst: Rectangle<i32, Physical>) -> ([f32; 4], [f32; 4]) {
        #[allow(clippy::cast_precision_loss)]
        let (dst_w, dst_h) = (dst.size.w as f32, dst.size.h as f32);
        let rect = [
            self.offset[0] * dst_w,
            self.offset[1] * dst_h,
            self.size[0] * dst_w,
            self.size[1] * dst_h,
        ];
        let radii = self.radii.map(|frac| frac * rect[3]);
        (rect, radii)
    }

    /// Whether this clip would actually remove anything.
    #[must_use]
    pub fn is_visible(&self) -> bool {
        self.size[0] > 0.0 && self.size[1] > 0.0 && self.radii.iter().any(|r| *r > 0.0)
    }
}

/// A renderer that can cut a surface's corners while sampling it.
pub trait ClipCapableRenderer: Renderer {
    /// Compiles and caches whatever the clip needs, returning whether it
    /// is usable. Called while building elements, when a `&mut` renderer
    /// is still available — never mid-frame.
    fn prepare_rounded_clip(&mut self) -> bool;

    /// Draws `texture` clipped to `clip`. Implementations must fall back
    /// to an ordinary draw rather than failing if the clip is unavailable.
    #[allow(clippy::too_many_arguments)]
    fn draw_rounded_clipped(
        frame: &mut Self::Frame<'_, '_>,
        texture: &Self::TextureId,
        src: Rectangle<f64, BufferCoord>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        opaque_regions: &[Rectangle<i32, Physical>],
        transform: Transform,
        alpha: f32,
        clip: &RoundedClip,
    ) -> Result<(), Self::Error>;
}

/// Newtype so the compiled program can live in the EGL context's user data
/// without colliding with anything else stored there.
struct CachedClipProgram(GlesTexProgram);

fn compile_clip_program(renderer: &mut GlesRenderer) -> Option<GlesTexProgram> {
    let uniforms = [
        UniformName::new(UNIFORM_UNIT_SCALE, UniformType::_2f),
        UniformName::new(UNIFORM_UNIT_OFFSET, UniformType::_2f),
        UniformName::new(UNIFORM_DST_SIZE, UniformType::_2f),
        UniformName::new(UNIFORM_CLIP_RECT, UniformType::_4f),
        UniformName::new(UNIFORM_CLIP_RADII, UniformType::_4f),
    ];
    match renderer.compile_custom_texture_shader(ROUNDED_CLIP_SHADER, &uniforms) {
        Ok(program) => Some(program),
        Err(err) => {
            tracing::warn!(
                ?err,
                "rounded-corner clip shader failed to compile; window content corners \
                 will stay square"
            );
            None
        }
    }
}

impl ClipCapableRenderer for GlesRenderer {
    fn prepare_rounded_clip(&mut self) -> bool {
        if self
            .egl_context()
            .user_data()
            .get::<Option<CachedClipProgram>>()
            .is_none()
        {
            // Compile before touching user data: `compile_*` needs `&mut
            // self`, and the borrow of `user_data()` would still be live.
            let compiled = compile_clip_program(self);
            self.egl_context()
                .user_data()
                .insert_if_missing(|| compiled.map(CachedClipProgram));
        }

        self.egl_context()
            .user_data()
            .get::<Option<CachedClipProgram>>()
            .is_some_and(Option::is_some)
    }

    fn draw_rounded_clipped(
        frame: &mut GlesFrame<'_, '_>,
        texture: &Self::TextureId,
        src: Rectangle<f64, BufferCoord>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        opaque_regions: &[Rectangle<i32, Physical>],
        transform: Transform,
        alpha: f32,
        clip: &RoundedClip,
    ) -> Result<(), GlesError> {
        let program = frame
            .egl_context()
            .user_data()
            .get::<Option<CachedClipProgram>>()
            .and_then(Option::as_ref)
            .map(|cached| cached.0.clone());

        let uniforms = program
            .as_ref()
            .map(|_| clip_uniforms(texture, src, dst, clip));

        frame.render_texture_from_to(
            texture,
            src,
            dst,
            damage,
            opaque_regions,
            transform,
            alpha,
            program.as_ref(),
            uniforms.as_deref().unwrap_or(&[]),
        )
    }
}

/// Builds the uniform set that maps this draw's texture coordinates onto
/// the clip rectangle. See [`ROUNDED_CLIP_SHADER`] for why the conversion
/// is needed at all.
fn clip_uniforms(
    texture: &impl Texture,
    src: Rectangle<f64, BufferCoord>,
    dst: Rectangle<i32, Physical>,
    clip: &RoundedClip,
) -> Vec<Uniform<'static>> {
    let tex_size = texture.size();
    #[allow(clippy::cast_precision_loss)]
    let (tex_w, tex_h) = (tex_size.w as f32, tex_size.h as f32);
    #[allow(clippy::cast_possible_truncation)]
    let (src_w, src_h) = (src.size.w as f32, src.size.h as f32);
    #[allow(clippy::cast_possible_truncation)]
    let (src_x, src_y) = (src.loc.x as f32, src.loc.y as f32);

    // Degenerate src would divide by zero; a scale of 1 with no offset
    // makes the shader clip against the raw texture coords, which is the
    // least surprising fallback.
    let (unit_scale, unit_offset) = if src_w > 0.0 && src_h > 0.0 {
        (
            [tex_w / src_w, tex_h / src_h],
            [-src_x / src_w, -src_y / src_h],
        )
    } else {
        ([1.0, 1.0], [0.0, 0.0])
    };

    #[allow(clippy::cast_precision_loss)]
    let dst_size = [dst.size.w as f32, dst.size.h as f32];
    let (clip_rect, clip_radii) = clip.resolve(dst);

    vec![
        Uniform::new(UNIFORM_UNIT_SCALE, unit_scale),
        Uniform::new(UNIFORM_UNIT_OFFSET, unit_offset),
        Uniform::new(UNIFORM_DST_SIZE, dst_size),
        Uniform::new(UNIFORM_CLIP_RECT, clip_rect),
        Uniform::new(UNIFORM_CLIP_RADII, clip_radii),
    ]
}

/// A client surface element drawn with its corners cut to the window's
/// rounded outline.
///
/// Every [`Element`] query is delegated straight to the wrapped surface
/// element, so damage tracking, opaque regions, geometry and commit
/// counters all keep behaving exactly as they did before — only the final
/// texture sample changes. Opaque regions are the one exception: a clipped
/// corner is no longer opaque, so they are dropped.
#[derive(Debug)]
pub struct ClippedSurfaceElement<R: Renderer> {
    inner: WaylandSurfaceRenderElement<R>,
    clip: RoundedClip,
    id: Id,
}

impl<R> ClippedSurfaceElement<R>
where
    R: Renderer + smithay::backend::renderer::ImportAll,
    R::TextureId: Texture + 'static,
{
    pub fn new(inner: WaylandSurfaceRenderElement<R>, clip: RoundedClip) -> Self {
        Self {
            // Reuse the wrapped surface's id rather than minting a fresh
            // one. These elements are rebuilt every frame, so `Id::new()`
            // here handed the damage tracker a brand-new element each time:
            // it saw the old one vanish and a new one appear, damaged both
            // rects in full, and could never track anything incrementally.
            // The bare and clipped variants are never emitted together for
            // the same surface, so sharing the id is unambiguous.
            id: inner.id().clone(),
            inner,
            clip,
        }
    }
}

impl<R> Element for ClippedSurfaceElement<R>
where
    R: Renderer + smithay::backend::renderer::ImportAll,
    R::TextureId: Texture + 'static,
{
    fn id(&self) -> &Id {
        &self.id
    }

    fn current_commit(&self) -> CommitCounter {
        self.inner.current_commit()
    }

    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> {
        self.inner.geometry(scale)
    }

    fn location(&self, scale: Scale<f64>) -> Point<i32, Physical> {
        self.inner.location(scale)
    }

    fn src(&self) -> Rectangle<f64, BufferCoord> {
        self.inner.src()
    }

    fn transform(&self) -> Transform {
        self.inner.transform()
    }

    fn damage_since(
        &self,
        scale: Scale<f64>,
        commit: Option<CommitCounter>,
    ) -> DamageSet<i32, Physical> {
        self.inner.damage_since(scale, commit)
    }

    /// Deliberately empty: the cut corners are translucent, so promising
    /// the compositor that this element covers its rect opaquely would
    /// let it skip painting what shows through them.
    fn opaque_regions(&self, _scale: Scale<f64>) -> OpaqueRegions<i32, Physical> {
        OpaqueRegions::default()
    }

    fn alpha(&self) -> f32 {
        self.inner.alpha()
    }

    fn kind(&self) -> Kind {
        self.inner.kind()
    }
}

impl<R> RenderElement<R> for ClippedSurfaceElement<R>
where
    R: Renderer + smithay::backend::renderer::ImportAll + ClipCapableRenderer,
    R::TextureId: Texture + 'static,
{
    fn underlying_storage(&self, renderer: &mut R) -> Option<UnderlyingStorage<'_>> {
        self.inner.underlying_storage(renderer)
    }

    fn draw(
        &self,
        frame: &mut R::Frame<'_, '_>,
        src: Rectangle<f64, BufferCoord>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        opaque_regions: &[Rectangle<i32, Physical>],
        cache: Option<&smithay::utils::user_data::UserDataMap>,
    ) -> Result<(), R::Error> {
        use smithay::backend::renderer::element::surface::WaylandSurfaceTexture;

        match self.inner.texture() {
            WaylandSurfaceTexture::Texture(texture) => R::draw_rounded_clipped(
                frame,
                texture,
                src,
                dst,
                damage,
                opaque_regions,
                self.inner.transform(),
                self.inner.alpha(),
                &self.clip,
            ),
            // Single-pixel buffers have no texture to sample; they are
            // vanishingly rare as toplevel content, so they simply go
            // through unclipped rather than growing a second code path.
            WaylandSurfaceTexture::SolidColor(_) => {
                self.inner
                    .draw(frame, src, dst, damage, opaque_regions, cache)
            }
        }
    }
}

/// Whether a clip is worth applying to an element covering `dst`.
///
/// The mapping from texture coordinates back to destination pixels that
/// [`ROUNDED_CLIP_SHADER`] performs assumes an untransformed surface.
/// Rotated or flipped outputs/buffers would land the rounded corners on
/// the wrong edges, so those are left alone.
#[must_use]
pub fn clip_applies(transform: Transform, dst: Rectangle<i32, Physical>) -> bool {
    transform == Transform::Normal && !dst.size.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use smithay::utils::Size;

    fn rect(x: i32, y: i32, w: i32, h: i32) -> Rectangle<i32, Physical> {
        Rectangle::new(Point::from((x, y)), Size::from((w, h)))
    }

    #[test]
    fn the_shader_keeps_the_define_marker_smithay_substitutes() {
        assert!(
            ROUNDED_CLIP_SHADER.contains("//_DEFINES_"),
            "without the marker smithay cannot build the NO_ALPHA/EXTERNAL variants"
        );
    }

    #[test]
    fn the_shader_handles_every_variant_smithay_compiles() {
        // smithay builds three variants: plain, NO_ALPHA and EXTERNAL.
        for define in ["NO_ALPHA", "EXTERNAL", "DEBUG_FLAGS"] {
            assert!(
                ROUNDED_CLIP_SHADER.contains(define),
                "shader ignores the {define} variant"
            );
        }
        assert!(
            ROUNDED_CLIP_SHADER.contains("samplerExternalOES"),
            "the EXTERNAL variant needs an external sampler declaration"
        );
    }

    #[test]
    fn every_declared_uniform_appears_in_the_shader() {
        for name in [
            UNIFORM_UNIT_SCALE,
            UNIFORM_UNIT_OFFSET,
            UNIFORM_DST_SIZE,
            UNIFORM_CLIP_RECT,
            UNIFORM_CLIP_RADII,
        ] {
            assert!(
                ROUNDED_CLIP_SHADER.contains(name),
                "uniform {name} is declared to smithay but unused by the shader"
            );
        }
    }

    #[test]
    fn a_zero_radius_frame_produces_no_clip() {
        assert!(RoundedClip::from_frame(rect(0, 0, 100, 80), rect(0, 0, 100, 80), 0.0).is_none());
    }

    #[test]
    fn a_degenerate_rect_produces_no_clip() {
        assert!(RoundedClip::from_frame(rect(0, 0, 0, 0), rect(0, 0, 100, 80), 12.0).is_none());
        assert!(RoundedClip::from_frame(rect(0, 0, 100, 80), rect(0, 0, 0, 0), 12.0).is_none());
    }

    #[test]
    fn a_clip_matching_its_element_resolves_back_to_the_same_pixels() {
        let dst = rect(0, 0, 200, 100);
        let clip = RoundedClip::from_frame(dst, dst, 12.0).expect("must build");
        let (r, radii) = clip.resolve(dst);
        assert!((r[0] - 0.0).abs() < 0.01 && (r[1] - 0.0).abs() < 0.01);
        assert!((r[2] - 200.0).abs() < 0.01 && (r[3] - 100.0).abs() < 0.01);
        assert!(radii.iter().all(|v| (v - 12.0).abs() < 0.01));
    }

    /// The whole reason the clip is stored proportionally: an overview
    /// preview is rescaled after the fact, so resolving against a smaller
    /// destination has to scale the rect *and* the radius with it.
    #[test]
    fn resolving_against_a_scaled_destination_scales_the_whole_clip() {
        let dst = rect(0, 0, 200, 100);
        let clip = RoundedClip::from_frame(dst, dst, 12.0).expect("must build");

        let (r, radii) = clip.resolve(rect(500, 400, 100, 50));
        assert!((r[2] - 100.0).abs() < 0.01, "width did not halve: {r:?}");
        assert!((r[3] - 50.0).abs() < 0.01, "height did not halve: {r:?}");
        assert!(
            (radii[0] - 6.0).abs() < 0.01,
            "radius must halve with the preview, got {radii:?}"
        );
    }

    /// A surface sits *below* the titlebar, so the frame it clips against
    /// starts above its own destination — the offset must come out
    /// negative, and the clip rect taller than the surface.
    #[test]
    fn a_frame_taller_than_the_surface_yields_a_negative_offset() {
        let titlebar = 48;
        let content = rect(10, 10 + titlebar, 200, 100);
        let frame = rect(10, 10, 200, 100 + titlebar);
        let clip = RoundedClip::from_frame(content, frame, 12.0).expect("must build");

        let (r, _) = clip.resolve(content);
        assert!(
            r[1] < 0.0,
            "the frame starts above the surface, so the offset must be negative: {r:?}"
        );
        #[allow(clippy::cast_precision_loss)]
        let expected_h = (100 + titlebar) as f32;
        assert!(
            (r[3] - expected_h).abs() < 0.01,
            "clip must span the whole frame height: {r:?}"
        );
    }

    #[test]
    fn a_built_clip_is_visible() {
        let dst = rect(0, 0, 100, 80);
        let clip = RoundedClip::from_frame(dst, dst, 12.0).expect("must build");
        assert!(clip.is_visible());
    }

    #[test]
    fn a_clip_applies_only_to_untransformed_surfaces() {
        let dst = rect(0, 0, 100, 80);
        assert!(clip_applies(Transform::Normal, dst));
        for transform in [
            Transform::_90,
            Transform::_180,
            Transform::_270,
            Transform::Flipped,
        ] {
            assert!(
                !clip_applies(transform, dst),
                "{transform:?} must fall back to an unclipped draw"
            );
        }
    }

    #[test]
    fn an_empty_destination_never_clips() {
        assert!(!clip_applies(Transform::Normal, rect(0, 0, 0, 0)));
    }
}

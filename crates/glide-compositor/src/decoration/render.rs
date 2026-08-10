use smithay::{
    backend::{allocator::Fourcc, renderer::element::memory::MemoryRenderBuffer},
    utils::{Point, Rectangle, Transform},
};

use crate::{config::Color, paint::Painter, window::GlideWindow};

use super::{BUTTON_PAD_X, BUTTON_PAD_Y, BUTTON_SIZE, BoxDecoCache, BoxDecoKey, DecorationBuffer};

pub(crate) fn redraw_button(
    buf: &mut Option<MemoryRenderBuffer>,
    scale: i32,
    bg: Color,
    fg: Color,
    icon: fn(&mut [u8], i32, i32, i32, Color, Color),
) {
    let (pw, ph) = (BUTTON_SIZE * scale, BUTTON_SIZE * scale);
    let buf = buf.get_or_insert_with(|| {
        MemoryRenderBuffer::new(Fourcc::Argb8888, (pw, ph), scale, Transform::Normal, None)
    });
    let mut ctx = buf.render();
    ctx.resize((pw, ph));
    let _ = ctx.draw(|pixels| {
        icon(pixels, BUTTON_SIZE, BUTTON_SIZE, scale, fg, bg);
        Ok::<_, ()>(vec![Rectangle::from_size((pw, ph).into())])
    });
}

#[profiling::function]
pub fn frame_buffers(window: &GlideWindow, painter: &Painter) -> Vec<DecorationBuffer> {
    let Some(mut state) = window.try_decoration_state_mut() else {
        return Vec::new();
    };
    let header = &mut state.header;

    let content = window.geometry().size;
    if header.width != content.w {
        header.width = content.w;
    }
    if header.content_height != content.h {
        header.content_height = content.h;
    }

    if !header.is_decorated() {
        return Vec::new();
    }

    header.redraw(painter);

    let mut out: Vec<DecorationBuffer> = Vec::new();

    if let Some(buf) = &header.menu_buf {
        out.push(DecorationBuffer {
            buf: buf.clone(),
            offset: Point::from((header.menu_x, header.height)),
        });
    }

    if let Some(buf) = &header.maximize_buf {
        out.push(DecorationBuffer {
            buf: buf.clone(),
            offset: Point::from((BUTTON_PAD_X, BUTTON_PAD_Y)),
        });
    }

    if let Some(buf) = &header.close_buf {
        out.push(DecorationBuffer {
            buf: buf.clone(),
            offset: Point::from((header.width - BUTTON_PAD_X - BUTTON_SIZE, BUTTON_PAD_Y)),
        });
    }

    if let Some(buf) = &header.titlebar_buf {
        out.push(DecorationBuffer {
            buf: buf.clone(),
            offset: Point::from((0, 0)),
        });
    }

    out
}

/// `DecorationTheme` only exposes one shadow size and one shadow color per
/// focus state (a deliberate choice — see Stage 1's commit message on why
/// the config schema wasn't broken open to a full elevation layer list).
/// The design spec's actual `--el-win`/`--el-win-idle` tokens (see
/// `glide_theme::elevation`) are each a blurred pair: a tighter, slightly
/// less opaque layer close to the window, and a softer, fuller-opacity
/// layer further out — `blur == shadow_size` for the outer layer in every
/// one of the spec's four size/appearance combinations, with `dy` running
/// close to `0.43 * blur` in each. This reconstructs that two-layer shape
/// from the one configured size/color instead of the flat single-band
/// halo the compositor drew before.
fn derive_shadow_layers(
    shadow_size: f32,
    shadow_color: Color,
) -> [crate::paint::ShadowLayerSpec; 2] {
    use crate::paint::SHADOW_DY_RATIO;
    const INNER_SIZE_RATIO: f32 = 0.24;
    const INNER_ALPHA_RATIO: f32 = 0.65;

    let outer = crate::paint::ShadowLayerSpec {
        dy: shadow_size * SHADOW_DY_RATIO,
        blur: shadow_size,
        color: shadow_color,
    };
    let inner_size = shadow_size * INNER_SIZE_RATIO;
    let inner = crate::paint::ShadowLayerSpec {
        dy: inner_size * SHADOW_DY_RATIO,
        blur: inner_size,
        color: shadow_color.with_alpha(shadow_color.0[3] * INNER_ALPHA_RATIO),
    };
    [inner, outer]
}

pub fn shadow_and_border_buffer(window: &GlideWindow, scale: i32) -> Option<DecorationBuffer> {
    let mut state = window.try_decoration_state_mut()?;
    if !state.header.is_decorated() {
        return None;
    }

    let width = state.header.width;
    let frame_height = state.header.height + state.header.content_height;
    if width <= 0 || frame_height <= 0 {
        return None;
    }

    let is_focused = state.header.focused;
    let theme = &state.header.theme;

    let border_width = i32::try_from(theme.border_width)
        .unwrap_or(super::RESIZE_BORDER)
        .clamp(0, super::RESIZE_BORDER);

    let border_color = if is_focused {
        theme.border_focused
    } else {
        theme.border
    };

    let (shadow_size, shadow_color) = if theme.shadow {
        if is_focused {
            (
                i32::try_from(theme.shadow_size_focused).unwrap_or(0),
                theme.shadow_color_focused,
            )
        } else {
            (
                i32::try_from(theme.shadow_size).unwrap_or(0),
                theme.shadow_color,
            )
        }
    } else {
        (0, crate::config::Color([0.0, 0.0, 0.0, 0.0]))
    };

    // `shadow_fade_margin`, not `shadow_size`, on purpose — the shadow is
    // drawn offset downward (see `derive_shadow_layers`), so the buffer
    // needs extra room on every side or the blur gets hard-clipped right
    // as it approaches zero alpha, which reads as a dense, sharp-edged
    // halo instead of a soft fade (and can visibly square off a corner
    // right where the clip lands).
    let margin = crate::paint::shadow_fade_margin(shadow_size) + border_width;
    if margin <= 0 {
        return None;
    }

    let is_maximized = state.header.maximized;
    let key = BoxDecoKey {
        width,
        height: frame_height,
        scale,
        focused: is_focused,
        maximized: is_maximized,
        theme_rev: state.header.theme_rev,
    };

    let needs_redraw = state.box_deco.as_ref().is_none_or(|cache| cache.key != key);

    if needs_redraw {
        let physical_w = width * scale;
        let physical_h = frame_height * scale;
        let physical_margin = margin * scale;
        let buf_w = physical_w + physical_margin * 2;
        let buf_h = physical_h + physical_margin * 2;

        if let Ok(pixels) = usize::try_from(buf_w * buf_h * 4) {
            let mut buf = vec![0u8; pixels];

            // Pixel dimensions comfortably fit an f32 mantissa.
            #[allow(clippy::cast_precision_loss)]
            let shadow_layers = derive_shadow_layers((shadow_size * scale) as f32, shadow_color);
            let window_radius = if is_maximized {
                0.0
            } else {
                theme.corner_radius
            };
            #[allow(clippy::cast_precision_loss)]
            crate::paint::draw_outset_shadow_and_border(
                &mut buf,
                buf_w,
                buf_h,
                physical_w as f32,
                physical_h as f32,
                &shadow_layers,
                (border_width * scale) as f32,
                border_color,
                window_radius * scale as f32,
                // Nothing behind the window's own rounded footprint — the
                // shadow fill would otherwise bleed through any pixels the
                // client renders transparent (e.g. translucent terminals).
                Some(window_radius * scale as f32),
            );

            let memory = MemoryRenderBuffer::from_slice(
                &buf,
                Fourcc::Argb8888,
                (buf_w, buf_h),
                scale,
                Transform::Normal,
                None,
            );
            state.box_deco = Some(BoxDecoCache { key, buf: memory });
        }
    }

    state.box_deco.as_ref().map(|cache| DecorationBuffer {
        buf: cache.buf.clone(),
        offset: Point::from((-margin, -margin)),
    })
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn outer_layer_blur_matches_the_configured_shadow_size_exactly() {
        let color = Color([0.0, 0.0, 0.0, 0.5]);
        let [_, outer] = derive_shadow_layers(68.0, color);
        assert!((outer.blur - 68.0).abs() < f32::EPSILON);
        assert_eq!(
            outer.color, color,
            "the outer layer keeps the configured color as-is"
        );
    }

    #[test]
    fn inner_layer_is_tighter_and_less_opaque_than_the_outer_one() {
        let color = Color([0.0, 0.0, 0.0, 0.5]);
        let [inner, outer] = derive_shadow_layers(68.0, color);
        assert!(inner.blur < outer.blur, "inner must be a tighter blur");
        assert!(inner.dy < outer.dy, "inner must sit closer to the window");
        assert!(
            inner.color.0[3] < outer.color.0[3],
            "inner must be less opaque than the configured color"
        );
    }

    #[test]
    fn vertical_offset_is_proportional_to_blur_in_both_layers() {
        let color = Color([0.0, 0.0, 0.0, 0.5]);
        let [inner, outer] = derive_shadow_layers(32.0, color);
        let ratio_inner = inner.dy / inner.blur;
        let ratio_outer = outer.dy / outer.blur;
        assert!(
            (ratio_inner - ratio_outer).abs() < 0.001,
            "both layers must use the same dy/blur ratio: inner={ratio_inner} outer={ratio_outer}"
        );
    }

    #[test]
    fn zero_shadow_size_produces_zero_sized_invisible_layers() {
        let color = Color([0.0, 0.0, 0.0, 0.5]);
        let [inner, outer] = derive_shadow_layers(0.0, color);
        assert_eq!(inner.blur, 0.0);
        assert_eq!(outer.blur, 0.0);
        assert_eq!(inner.dy, 0.0);
        assert_eq!(outer.dy, 0.0);
    }

    #[test]
    fn scales_linearly_with_the_configured_size() {
        let color = Color([0.0, 0.0, 0.0, 0.5]);
        let [inner_1x, outer_1x] = derive_shadow_layers(32.0, color);
        let [inner_2x, outer_2x] = derive_shadow_layers(64.0, color);
        assert!((outer_2x.blur - outer_1x.blur * 2.0).abs() < 0.01);
        assert!((inner_2x.blur - inner_1x.blur * 2.0).abs() < 0.01);
        assert!((outer_2x.dy - outer_1x.dy * 2.0).abs() < 0.01);
    }
}

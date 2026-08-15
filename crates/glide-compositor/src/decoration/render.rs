use smithay::{
    backend::{allocator::Fourcc, renderer::element::memory::MemoryRenderBuffer},
    utils::{Point, Rectangle, Transform},
};

use crate::{config::Color, paint::Painter, window::GlideWindow};

use super::{BUTTON_PAD_X, BUTTON_PAD_Y, BUTTON_SIZE, BoxDecoCache, BoxDecoKey, DecorationBuffer};

const PRESS_SCALE: f64 = 0.9;

pub(crate) fn redraw_button(
    buf: &mut Option<MemoryRenderBuffer>,
    scale: i32,
    bg: Color,
    fg: Color,
    pressed: bool,
    icon: fn(&mut [u8], i32, i32, i32, Color, Color),
) {
    let (pw, ph) = (BUTTON_SIZE * scale, BUTTON_SIZE * scale);
    let buf = buf.get_or_insert_with(|| {
        MemoryRenderBuffer::new(Fourcc::Argb8888, (pw, ph), scale, Transform::Normal, None)
    });
    let mut ctx = buf.render();
    ctx.resize((pw, ph));
    let _ = ctx.draw(|pixels| {
        if pressed {
            #[allow(clippy::cast_possible_truncation)]
            let side = ((f64::from(BUTTON_SIZE) * PRESS_SCALE).round() as i32).max(1);
            let (spw, sph) = (side * scale, side * scale);
            let mut small = vec![0u8; usize::try_from(spw * sph * 4).unwrap_or_default()];
            icon(&mut small, side, side, scale, fg, bg);

            pixels.fill(0);
            let (ox, oy) = ((pw - spw) / 2, (ph - sph) / 2);
            let src_stride = usize::try_from(spw * 4).unwrap_or_default();
            let dst_stride = usize::try_from(pw * 4).unwrap_or_default();
            for row in 0..usize::try_from(sph).unwrap_or_default() {
                let src_start = row * src_stride;
                let dst_start = (row + usize::try_from(oy).unwrap_or_default()) * dst_stride
                    + usize::try_from(ox * 4).unwrap_or_default();
                pixels[dst_start..dst_start + src_stride]
                    .copy_from_slice(&small[src_start..src_start + src_stride]);
            }
        } else {
            icon(pixels, BUTTON_SIZE, BUTTON_SIZE, scale, fg, bg);
        }
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
        let t = header.menu_open_progress();
        #[allow(clippy::cast_possible_truncation)]
        let rise = (super::MENU_OPEN_RISE * (1.0 - t)).round() as i32;
        let scale = super::MENU_OPEN_SCALE + (1.0 - super::MENU_OPEN_SCALE) * t;
        #[allow(clippy::cast_possible_truncation)]
        out.push(DecorationBuffer {
            buf: buf.clone(),
            offset: Point::from((header.menu_x, header.height + rise)),
            alpha: t as f32,
            scale: (t < 1.0).then_some(scale),
        });
    }

    if let Some(buf) = &header.maximize_buf {
        out.push(DecorationBuffer::plain(
            buf.clone(),
            Point::from((BUTTON_PAD_X, BUTTON_PAD_Y)),
        ));
    }

    if let Some(buf) = &header.close_buf {
        out.push(DecorationBuffer::plain(
            buf.clone(),
            Point::from((header.width - BUTTON_PAD_X - BUTTON_SIZE, BUTTON_PAD_Y)),
        ));
    }

    if let Some(buf) = &header.titlebar_buf {
        out.push(DecorationBuffer::plain(buf.clone(), Point::from((0, 0))));
    }

    out
}

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
    let theme = state.header.theme.clone();

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
        state.box_deco_fade = outgoing_fade(state.box_deco.as_ref(), key);
    }

    if needs_redraw {
        let physical_w = width * scale;
        let physical_h = frame_height * scale;
        let physical_margin = margin * scale;
        let buf_w = physical_w + physical_margin * 2;
        let buf_h = physical_h + physical_margin * 2;

        if let Ok(pixels) = usize::try_from(buf_w * buf_h * 4) {
            let mut buf = vec![0u8; pixels];

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

    state
        .box_deco
        .as_ref()
        .map(|cache| DecorationBuffer::plain(cache.buf.clone(), Point::from((-margin, -margin))))
}

fn outgoing_fade(current: Option<&BoxDecoCache>, next: BoxDecoKey) -> Option<super::BoxDecoFade> {
    let cache = current?;
    let focus_only = cache.key.focused != next.focused
        && cache.key.width == next.width
        && cache.key.height == next.height
        && cache.key.scale == next.scale
        && cache.key.maximized == next.maximized
        && cache.key.theme_rev == next.theme_rev;

    focus_only.then(|| super::BoxDecoFade {
        buf: cache.buf.clone(),
        started: std::time::Instant::now(),
    })
}

pub fn shadow_and_border_fade(window: &GlideWindow) -> Option<(DecorationBuffer, f32)> {
    let mut state = window.try_decoration_state_mut()?;
    let alpha = state
        .box_deco_fade
        .as_ref()
        .and_then(super::BoxDecoFade::alpha);
    let Some(alpha) = alpha else {
        state.box_deco_fade = None;
        return None;
    };

    let fade = state.box_deco_fade.as_ref()?;
    let margin = shadow_margin_for(&state.header);
    Some((
        DecorationBuffer::plain(fade.buf.clone(), Point::from((-margin, -margin))),
        alpha,
    ))
}

fn shadow_margin_for(header: &super::HeaderBar) -> i32 {
    header.shadow_margin()
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

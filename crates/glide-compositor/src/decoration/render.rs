use smithay::{
    backend::{allocator::Fourcc, renderer::element::memory::MemoryRenderBuffer},
    utils::{Point, Rectangle, Transform},
};

use crate::{config::Color, paint::Painter, window::GlideWindow};

use super::{BUTTON_PAD_X, BUTTON_PAD_Y, BUTTON_SIZE, BoxDecoCache, BoxDecoFade, BoxDecoKey, DecorationBuffer};

const PRESS_SCALE: f64 = 0.96;

fn pressed_side() -> i32 {
    let inset = (f64::from(BUTTON_SIZE) * (1.0 - PRESS_SCALE) / 2.0).round().max(1.0) as i32;
    (BUTTON_SIZE - inset * 2).max(1)
}

pub(crate) fn redraw_button(
    buf: &mut Option<MemoryRenderBuffer>, scale: i32, bg: Color, fg: Color, pressed: bool,
    icon: fn(&mut [u8], i32, i32, i32, Color, Color),
) {
    let (pw, ph) = (BUTTON_SIZE * scale, BUTTON_SIZE * scale);
    let buf =
        buf.get_or_insert_with(|| MemoryRenderBuffer::new(Fourcc::Argb8888, (pw, ph), scale, Transform::Normal, None));
    let mut ctx = buf.render();
    ctx.resize((pw, ph));
    let _ = ctx.draw(|pixels| {
        if pressed {
            let side = pressed_side();
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
                pixels[dst_start..dst_start + src_stride].copy_from_slice(&small[src_start..src_start + src_stride]);
            }
        } else {
            icon(pixels, BUTTON_SIZE, BUTTON_SIZE, scale, fg, bg);
        }
        Ok::<_, ()>(vec![Rectangle::from_size((pw, ph).into())])
    });
}

#[profiling::function]
pub fn frame_buffers(window: &GlideWindow, painter: &Painter, scale: i32) -> Vec<DecorationBuffer> {
    let Some(mut state) = window.try_decoration_state_mut() else {
        return Vec::new();
    };
    let header = &mut state.header;

    let content = window.geometry().size;
    header.width = content.w;
    header.content_height = content.h;
    if !header.is_decorated() {
        return Vec::new();
    }

    header.redraw(painter, scale);

    let mut out: Vec<DecorationBuffer> = Vec::new();
    let Some(buffers) = header.buffers_at(scale) else {
        return out;
    };

    if let Some(buf) = &buffers.menu {
        let t = header.menu_open_progress();
        let rise = (super::MENU_OPEN_RISE * (1.0 - t)).round() as i32;
        let scale = super::MENU_OPEN_SCALE + (1.0 - super::MENU_OPEN_SCALE) * t;
        out.push(DecorationBuffer {
            buf: buf.clone(),
            offset: Point::from((header.menu_x, header.height + rise)),
            alpha: t as f32,
            scale: (t < 1.0).then_some(scale),
            overlay: true,
        });
    }

    let left = Point::from((BUTTON_PAD_X, BUTTON_PAD_Y));
    let right = Point::from((header.width - BUTTON_PAD_X - BUTTON_SIZE, BUTTON_PAD_Y));
    let (maximize_at, close_at) = if header.close_on_left() { (right, left) } else { (left, right) };

    for (buf, at) in
        [(&buffers.maximize, maximize_at), (&buffers.close, close_at), (&buffers.titlebar, Point::from((0, 0)))]
    {
        if let Some(buf) = buf {
            out.push(DecorationBuffer::plain(buf.clone(), at));
        }
    }
    out
}

fn derive_shadow_layers(shadow_size: f32, shadow_color: Color) -> [crate::paint::ShadowLayerSpec; 2] {
    use crate::paint::SHADOW_DY_RATIO;
    const INNER_SIZE_RATIO: f32 = 0.24;
    const INNER_ALPHA_RATIO: f32 = 0.65;

    let outer =
        crate::paint::ShadowLayerSpec { dy: shadow_size * SHADOW_DY_RATIO, blur: shadow_size, color: shadow_color };
    let inner_size = shadow_size * INNER_SIZE_RATIO;
    let inner = crate::paint::ShadowLayerSpec {
        dy: inner_size * SHADOW_DY_RATIO,
        blur: inner_size,
        color: shadow_color.with_alpha(shadow_color.0[3] * INNER_ALPHA_RATIO),
    };
    [inner, outer]
}

fn window_shadow_layers(
    theme: &glide_theme::DecorationTheme, focused: bool, scale: f32,
) -> Vec<crate::paint::ShadowLayerSpec> {
    if !theme.shadow {
        return Vec::new();
    }
    let stock = glide_theme::DecorationTheme::default();
    let customised = theme.shadow_size != stock.shadow_size
        || theme.shadow_size_focused != stock.shadow_size_focused
        || theme.shadow_color != stock.shadow_color
        || theme.shadow_color_focused != stock.shadow_color_focused;
    if customised {
        let (size, color) = if focused {
            (theme.shadow_size_focused, theme.shadow_color_focused)
        } else {
            (theme.shadow_size, theme.shadow_color)
        };
        return derive_shadow_layers(size as f32 * scale, color).to_vec();
    }
    let level = if focused { glide_theme::elevation::Level::ElWin } else { glide_theme::elevation::Level::ElWinIdle };
    glide_theme::elevation::layers(level, theme.appearance)
        .into_iter()
        .filter(|layer| layer.blur > 0.0)
        .map(|layer| crate::paint::ShadowLayerSpec {
            dy: layer.dy * scale,
            blur: layer.blur * scale,
            color: layer.color,
        })
        .collect()
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

    let border_width = state.header.border_width();
    let border_color = if is_focused { theme.border_focused } else { theme.border };

    let margin = state.header.shadow_margin();
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

    let needs_redraw = state.box_deco.get(&scale).is_none_or(|cache| cache.key != key);

    if needs_redraw {
        let fade = state.box_deco_fade.take();
        state.box_deco_fade = next_fade(fade, state.box_deco.get(&scale), key);
        let physical_w = width * scale;
        let physical_h = frame_height * scale;
        let physical_margin = margin * scale;
        let buf_w = physical_w + physical_margin * 2;
        let buf_h = physical_h + physical_margin * 2;

        if let Ok(pixels) = usize::try_from(buf_w * buf_h * 4) {
            let mut buf = vec![0u8; pixels];

            let shadow_layers = window_shadow_layers(&theme, is_focused, scale as f32);
            let window_radius = if is_maximized { 0.0 } else { theme.corner_radius };
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

            let memory =
                MemoryRenderBuffer::from_slice(&buf, Fourcc::Argb8888, (buf_w, buf_h), scale, Transform::Normal, None);
            state.box_deco.insert(scale, BoxDecoCache { key, buf: memory });
        }
    }

    state.box_deco.get(&scale).map(|cache| DecorationBuffer::plain(cache.buf.clone(), Point::from((-margin, -margin))))
}

pub(super) fn next_fade(
    fade: Option<BoxDecoFade>, current: Option<&BoxDecoCache>, next: BoxDecoKey,
) -> Option<BoxDecoFade> {
    let Some(cache) = current else {
        return fade;
    };
    let focus_only = cache.key.focused != next.focused && BoxDecoKey { focused: next.focused, ..cache.key } == next;
    if !focus_only {
        return None;
    }

    match fade.and_then(|fade| fade.alpha().map(|alpha| (fade, alpha))) {
        Some((mut fade, _)) if fade.focused == next.focused => {
            fade.bufs.entry(next.scale).or_insert_with(|| cache.buf.clone());
            Some(fade)
        }
        running => Some(BoxDecoFade::start(
            running.map_or(1.0, |(_, alpha)| 1.0 - f64::from(alpha)),
            next.focused,
            next.scale,
            cache.buf.clone(),
        )),
    }
}

pub fn shadow_and_border_fade(window: &GlideWindow, scale: i32) -> Option<(DecorationBuffer, f32)> {
    let mut state = window.try_decoration_state_mut()?;
    let alpha = state.box_deco_fade.as_ref().and_then(BoxDecoFade::alpha);
    let Some(alpha) = alpha else {
        state.box_deco_fade = None;
        return None;
    };

    let buf = state.box_deco_fade.as_ref()?.bufs.get(&scale)?.clone();
    let margin = state.header.shadow_margin();
    Some((DecorationBuffer::plain(buf, Point::from((-margin, -margin))), alpha))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stock_windows_use_the_el_win_tokens_in_either_appearance() {
        for appearance in [glide_theme::Appearance::Light, glide_theme::Appearance::Dark] {
            let theme = glide_theme::DecorationTheme::from_tokens(&glide_theme::Tokens::new(
                appearance,
                glide_theme::Accent::Blue,
            ));
            for (focused, level) in
                [(true, glide_theme::elevation::Level::ElWin), (false, glide_theme::elevation::Level::ElWinIdle)]
            {
                let layers = window_shadow_layers(&theme, focused, 2.0);
                let tokens = glide_theme::elevation::layers(level, appearance);
                assert_eq!(layers.len(), tokens.len() - 1, "the ring is the border");
                for (layer, token) in layers.iter().zip(&tokens[1..]) {
                    assert_eq!((layer.dy, layer.blur), (token.dy * 2.0, token.blur * 2.0));
                    assert_eq!(layer.color, token.color);
                }
            }
        }
    }

    #[test]
    fn a_customised_shadow_is_still_honoured() {
        let theme = glide_theme::DecorationTheme { shadow_size_focused: 40, ..glide_theme::DecorationTheme::default() };
        let layers = window_shadow_layers(&theme, true, 1.0);
        assert_eq!(layers.last().map(|l| l.blur), Some(40.0));
    }

    #[test]
    fn outer_layer_blur_matches_the_configured_shadow_size_exactly() {
        let color = Color([0.0, 0.0, 0.0, 0.5]);
        let [_, outer] = derive_shadow_layers(68.0, color);
        assert!((outer.blur - 68.0).abs() < f32::EPSILON);
        assert_eq!(outer.color, color, "the outer layer keeps the configured color as-is");
    }

    #[test]
    fn inner_layer_is_tighter_and_less_opaque_than_the_outer_one() {
        let color = Color([0.0, 0.0, 0.0, 0.5]);
        let [inner, outer] = derive_shadow_layers(68.0, color);
        assert!(inner.blur < outer.blur, "inner must be a tighter blur");
        assert!(inner.dy < outer.dy, "inner must sit closer to the window");
        assert!(inner.color.0[3] < outer.color.0[3], "inner must be less opaque than the configured color");
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

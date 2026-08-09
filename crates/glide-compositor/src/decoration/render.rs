use smithay::{
    backend::{allocator::Fourcc, renderer::element::memory::MemoryRenderBuffer},
    utils::{Logical, Point, Rectangle, Size, Transform},
};

use crate::{config::Color, paint::Painter, window::GlideWindow};

use super::{BUTTON_PAD_X, BUTTON_PAD_Y, BUTTON_SIZE, DecorationBuffer};

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

    if let Some(wl_surface) = window.toplevel().map(|t| t.wl_surface().clone()) {
        use smithay::wayland::shell::xdg::XdgToplevelSurfaceData;
        let title = smithay::wayland::compositor::with_states(&wl_surface, |states| {
            states
                .data_map
                .get::<XdgToplevelSurfaceData>()
                .and_then(|data| data.lock().ok())
                .and_then(|guard| guard.title.clone())
        });
        if let Some(title) = title {
            header.set_title(title);
        }
    }

    header.redraw(painter);

    let mut out: Vec<DecorationBuffer> = Vec::new();

    for (i, buf) in header.menu_row_bufs.iter().enumerate() {
        let y = header.height * (1 + i32::try_from(i).unwrap_or(0));
        out.push(DecorationBuffer {
            buf: buf.clone(),
            offset: Point::from((header.menu_x, y)),
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

pub(crate) fn border_rects(
    width: i32,
    frame_height: i32,
    border_width: i32,
) -> [(Point<i32, Logical>, Size<i32, Logical>); 4] {
    [
        (
            Point::from((-border_width, -border_width)),
            Size::from((width + border_width * 2, border_width)),
        ),
        (
            Point::from((-border_width, frame_height)),
            Size::from((width + border_width * 2, border_width)),
        ),
        (
            Point::from((-border_width, 0)),
            Size::from((border_width, frame_height)),
        ),
        (
            Point::from((width, 0)),
            Size::from((border_width, frame_height)),
        ),
    ]
}

pub(crate) type ShadowRect = (Point<i32, Logical>, Size<i32, Logical>, f32);

pub(crate) fn shadow_rects(
    width: i32,
    frame_height: i32,
    shadow_size: i32,
    bands: usize,
) -> Vec<ShadowRect> {
    if shadow_size <= 0 || bands == 0 || width <= 0 || frame_height <= 0 {
        return Vec::new();
    }

    let bands_i32 = i32::try_from(bands).unwrap_or(1).max(1);
    let step = (shadow_size / bands_i32).max(1);

    let mut out = Vec::with_capacity(bands * 4);
    let mut outset = 0;
    for i in 0..bands {
        #[allow(clippy::cast_precision_loss)]
        let t = (i as f64 + 0.5) / bands as f64;
        let falloff = 1.0 - t;
        #[allow(clippy::cast_possible_truncation)]
        let alpha = (falloff * falloff) as f32;

        let cur_w = width + outset * 2;
        let cur_h = frame_height + outset * 2;
        for (offset, size) in border_rects(cur_w, cur_h, step) {
            let adjusted = Point::from((offset.x - outset, offset.y - outset));
            out.push((adjusted, size, alpha));
        }
        outset += step;
    }
    out
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

    let margin = shadow_size + border_width;
    if margin <= 0 {
        return None;
    }

    let w = width;
    let h = frame_height;

    let needs_redraw = state
        .box_deco
        .as_ref()
        .is_none_or(|(cw, ch, cscale, _)| *cw != w || *ch != h || *cscale != scale);

    if needs_redraw {
        let physical_w = w * scale;
        let physical_h = h * scale;
        let physical_margin = margin * scale;
        let buf_w = physical_w + physical_margin * 2;
        let buf_h = physical_h + physical_margin * 2;

        if let Ok(pixels) = usize::try_from(buf_w * buf_h * 4) {
            let mut buf = vec![0u8; pixels];

            crate::paint::draw_outset_shadow_and_border(
                &mut buf,
                buf_w,
                buf_h,
                physical_w as f32,
                physical_h as f32,
                (shadow_size * scale) as f32,
                super::SHADOW_BANDS,
                shadow_color,
                (border_width * scale) as f32,
                border_color,
                super::MENU_CORNER_RADIUS * scale as f32, // Window radius is same as menu
            );

            let memory = MemoryRenderBuffer::from_slice(
                &buf,
                Fourcc::Argb8888,
                (buf_w, buf_h),
                scale,
                Transform::Normal,
                None,
            );
            state.box_deco = Some((w, h, scale, memory));
        }
    }

    if let Some((_, _, _, memory)) = state.box_deco.as_ref() {
        Some(DecorationBuffer {
            buf: memory.clone(),
            offset: Point::from((-margin, -margin)),
        })
    } else {
        None
    }
}

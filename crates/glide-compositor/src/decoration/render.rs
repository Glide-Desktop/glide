use smithay::{
    backend::{
        allocator::Fourcc,
        renderer::element::{memory::MemoryRenderBuffer, solid::SolidColorBuffer},
    },
    utils::{Logical, Point, Rectangle, Size, Transform},
};

use crate::{config::Color, paint::Painter, window::GlideWindow};

use super::{BUTTON_WIDTH, BorderStrip, DecorationBuffer, RESIZE_BORDER};

pub(crate) fn redraw_button(
    buf: &mut Option<MemoryRenderBuffer>,
    height: i32,
    scale: i32,
    bg: Color,
    fg: Color,
    icon: fn(&mut [u8], i32, i32, i32, Color, Color),
) {
    let w = BUTTON_WIDTH;
    let (pw, ph) = (w * scale, height * scale);
    let buf = buf.get_or_insert_with(|| {
        MemoryRenderBuffer::new(Fourcc::Argb8888, (pw, ph), scale, Transform::Normal, None)
    });
    let mut ctx = buf.render();
    ctx.resize((pw, ph));
    let _ = ctx.draw(|pixels| {
        icon(pixels, w, height, scale, fg, bg);
        Ok::<_, ()>(vec![Rectangle::from_size((pw, ph).into())])
    });
}

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
            offset: Point::from((0, 0)),
        });
    }

    if let Some(buf) = &header.close_buf {
        out.push(DecorationBuffer {
            buf: buf.clone(),
            offset: Point::from((header.width - BUTTON_WIDTH, 0)),
        });
    }

    if let Some(buf) = &header.chevron_buf {
        out.push(DecorationBuffer {
            buf: buf.clone(),
            offset: Point::from((header.width - BUTTON_WIDTH * 2, 0)),
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

pub fn border_strips(window: &GlideWindow) -> Vec<BorderStrip> {
    let Some(mut state) = window.try_decoration_state_mut() else {
        return Vec::new();
    };
    if !state.header.is_decorated() {
        return Vec::new();
    }

    let width = state.header.width;
    let frame_height = state.header.height + state.header.content_height;
    let border_width = i32::try_from(state.header.theme.border_width)
        .unwrap_or(RESIZE_BORDER)
        .clamp(0, RESIZE_BORDER);
    if border_width == 0 || width <= 0 || frame_height <= 0 {
        return Vec::new();
    }

    let color = if state.header.focused {
        state.header.theme.border_focused
    } else {
        state.header.theme.border
    }
    .0;

    let [top, bottom, left, right] = border_rects(width, frame_height, border_width);
    let border = &mut state.border;
    let mut out = Vec::with_capacity(4);

    border.top.update(top.1, color);
    out.push(BorderStrip {
        buf: border.top.clone(),
        offset: top.0,
    });

    border.bottom.update(bottom.1, color);
    out.push(BorderStrip {
        buf: border.bottom.clone(),
        offset: bottom.0,
    });

    border.left.update(left.1, color);
    out.push(BorderStrip {
        buf: border.left.clone(),
        offset: left.0,
    });

    border.right.update(right.1, color);
    out.push(BorderStrip {
        buf: border.right.clone(),
        offset: right.0,
    });

    out
}

pub fn shadow_strips(window: &GlideWindow) -> Vec<BorderStrip> {
    let Some(mut state) = window.try_decoration_state_mut() else {
        return Vec::new();
    };
    if !state.header.is_decorated() || !state.header.theme.shadow {
        return Vec::new();
    }

    let width = state.header.width;
    let frame_height = state.header.height + state.header.content_height;
    let shadow_size = i32::try_from(state.header.theme.shadow_size).unwrap_or(0);
    let rects = shadow_rects(width, frame_height, shadow_size, super::SHADOW_BANDS);
    if rects.is_empty() {
        return Vec::new();
    }

    let base_color = state.header.theme.shadow_color.0;
    state
        .shadow
        .rings
        .resize_with(rects.len(), SolidColorBuffer::default);

    let mut out = Vec::with_capacity(rects.len());
    for ((offset, size, ring_alpha), buf) in rects.into_iter().zip(state.shadow.rings.iter_mut()) {
        let mut color = base_color;
        color[3] *= ring_alpha;
        buf.update(size, color);
        out.push(BorderStrip {
            buf: buf.clone(),
            offset,
        });
    }

    out
}

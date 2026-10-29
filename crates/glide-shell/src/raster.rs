use smithay::backend::renderer::{ImportAll, ImportMem, Renderer, element::memory::MemoryRenderBuffer};
use smithay::utils::{Logical, Rectangle, Transform};

use glide_paint::{Painter, TextAlign};
use glide_theme::{Color, TextStyle};

pub trait ShellRenderer: Renderer<TextureId: Send + Clone + 'static> + ImportAll + ImportMem {}

impl<R: Renderer<TextureId: Send + Clone + 'static> + ImportAll + ImportMem> ShellRenderer for R {}

pub type Rect = Rectangle<i32, Logical>;

#[must_use]
pub fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rectangle::new((x, y).into(), (w, h).into())
}

#[must_use]
pub fn canvas(width: i32, height: i32, scale: i32) -> Option<Vec<u8>> {
    let count = (width * scale).checked_mul(height * scale)?.checked_mul(4)?;
    Some(vec![0u8; usize::try_from(count).ok()?])
}

#[must_use]
pub fn buffer_from(buf: &[u8], width: i32, height: i32, scale: i32) -> MemoryRenderBuffer {
    MemoryRenderBuffer::from_slice(
        buf,
        smithay::backend::allocator::Fourcc::Argb8888,
        (width * scale, height * scale),
        scale,
        Transform::Normal,
        None,
    )
}

#[must_use]
pub fn shadow(
    (width, height): (i32, i32), margin: i32, radius: f32, layers: &[glide_theme::elevation::ShadowLayer], scale: i32,
) -> Option<Canvas<'static>> {
    let mut canvas = Canvas::new(width + margin * 2, height + margin * 2, scale)?;
    let factor = scale as f32;
    let specs: Vec<_> = layers
        .iter()
        .map(|layer| glide_paint::ShadowLayerSpec {
            dy: layer.dy * factor,
            blur: layer.blur * factor,
            color: layer.color,
        })
        .collect();
    glide_paint::draw_outset_shadow_and_border(
        &mut canvas.buf,
        canvas.w * scale,
        canvas.h * scale,
        width as f32 * factor,
        height as f32 * factor,
        &specs,
        0.0,
        Color([0.0; 4]),
        radius * factor,
        Some(radius * factor),
    );
    Some(canvas)
}

pub struct Backdrop<'a> {
    pub pixels: &'a [u8],
    pub width: i32,
    pub height: i32,
    pub fractional_scale: f64,
    pub generation: u64,
}

impl Backdrop<'_> {
    #[must_use]
    pub fn blit(&self, dst: &mut [u8], width: i32, height: i32, radius: f32, rect: Rect, scale: i32) -> bool {
        let to_src = |v: i32| (f64::from(v) * self.fractional_scale).round() as i32;
        glide_paint::blit_rounded_backdrop(
            dst,
            width * scale,
            height * scale,
            radius * scale as f32,
            self.pixels,
            self.width,
            self.height,
            (to_src(rect.loc.x), to_src(rect.loc.y), to_src(rect.size.w).max(1), to_src(rect.size.h).max(1)),
        )
    }
}

pub struct Canvas<'a, B = Vec<u8>> {
    pub buf: B,
    pub w: i32,
    pub h: i32,
    pub scale: i32,
    pub icons: Option<&'a mut crate::icon::IconLoader>,
}

impl<'a> Canvas<'a> {
    #[must_use]
    pub fn new(w: i32, h: i32, scale: i32) -> Option<Self> {
        Some(Self { buf: canvas(w, h, scale)?, w, h, scale, icons: None })
    }

    #[must_use]
    pub fn with_icons(mut self, icons: Option<&'a mut crate::icon::IconLoader>) -> Self {
        self.icons = icons;
        self
    }

    #[must_use]
    pub fn popover((w, h): (i32, i32), scale: i32, radius: f32, bg: Color, border: Color) -> Option<Self> {
        let mut canvas = Self::new(w, h, scale)?;
        canvas.panel(bg, radius);
        canvas.stroke(canvas.full(), 1.0, radius, border);
        Some(canvas)
    }

    #[must_use]
    pub fn finish(&self) -> MemoryRenderBuffer {
        buffer_from(&self.buf, self.w, self.h, self.scale)
    }
}

impl<'b> Canvas<'static, &'b mut [u8]> {
    pub fn over(buf: &'b mut [u8], w: i32, h: i32, scale: i32) -> Self {
        Self { buf, w, h, scale, icons: None }
    }
}

impl<B: AsMut<[u8]>> Canvas<'_, B> {
    pub fn full(&self) -> Rect {
        Rectangle::new((0, 0).into(), (self.w, self.h).into())
    }

    pub fn panel(&mut self, colour: Color, radius: f32) {
        Painter::fill_panel_background(self.buf.as_mut(), self.w, self.h, self.scale, colour, radius, [true; 4]);
    }

    pub fn fill(&mut self, rect: Rect, colour: Color, radius: f32) {
        let (Rectangle { loc, size }, (w, h, scale)) = (rect, (self.w, self.h, self.scale));
        Painter::fill_rounded_rect_over(self.buf.as_mut(), w, h, loc.x, loc.y, size.w, size.h, scale, colour, radius);
    }

    pub fn gradient(&mut self, rect: Rect, radius: f32, from: Color, to: Color) {
        let (Rectangle { loc, size }, (w, h, scale)) = (rect, (self.w, self.h, self.scale));
        glide_paint::fill_rounded_rect_gradient_over(
            self.buf.as_mut(),
            w,
            h,
            loc.x,
            loc.y,
            size.w,
            size.h,
            scale,
            from,
            to,
            radius,
        );
    }

    pub fn stroke(&mut self, rect: Rect, width: f32, radius: f32, colour: Color) {
        let (Rectangle { loc, size }, (w, h, scale)) = (rect, (self.w, self.h, self.scale));
        glide_paint::stroke_rounded_rect_over(
            self.buf.as_mut(),
            w,
            h,
            loc.x,
            loc.y,
            size.w,
            size.h,
            scale,
            width,
            radius,
            colour,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub fn text(
        &mut self, painter: &Painter, (left, right, top, bottom): (i32, i32, i32, i32), align: TextAlign, text: &str,
        colour: Color, style: &TextStyle,
    ) {
        painter.draw_label_over_styled_in_band(
            self.buf.as_mut(),
            self.w,
            self.h,
            left,
            right,
            top,
            bottom,
            self.scale,
            align,
            text,
            colour.to_buffer_bytes(),
            style,
        );
    }

    pub fn text_in(
        &mut self, painter: &Painter, rect: Rect, align: TextAlign, text: &str, colour: Color, style: &TextStyle,
    ) {
        let band = (rect.loc.x, rect.loc.x + rect.size.w, rect.loc.y, rect.loc.y + rect.size.h);
        self.text(painter, band, align, text, colour, style);
    }

    pub fn label(
        &mut self, painter: &Painter, (left, right): (i32, i32), align: TextAlign, text: &str, colour: Color,
        style: &TextStyle,
    ) {
        self.text(painter, (left, right, 0, self.h), align, text, colour, style);
    }

    pub fn blit(&mut self, src: &[u8], rect: Rect) {
        let (Rectangle { loc, size }, (w, h, scale)) = (rect, (self.w, self.h, self.scale));
        glide_paint::blit_rect_over(self.buf.as_mut(), w, h, src, size.w, size.h, loc.x, loc.y, scale);
    }

    pub fn blit_square(&mut self, src: &[u8], side: i32, (x, y): (i32, i32)) {
        glide_paint::blit_square_over(self.buf.as_mut(), self.w, self.h, src, side, x, y, self.scale);
    }

    pub fn icon(&mut self, name: &str, side: i32, at: (i32, i32), colour: Color) -> bool {
        let Some(loader) = self.icons.as_deref_mut() else {
            return false;
        };
        let Some(pix) = loader.load_symbolic(name, side * self.scale, colour) else {
            return false;
        };
        glide_paint::blit_square_over(self.buf.as_mut(), self.w, self.h, pix, side, at.0, at.1, self.scale);
        true
    }

    pub fn glyph(&mut self, rect: Rect, draw: impl FnOnce(&mut [u8], i32, i32, i32)) {
        let Some(mut glyph) = canvas(rect.size.w, rect.size.h, self.scale) else {
            return;
        };
        draw(&mut glyph, rect.size.w, rect.size.h, self.scale);
        self.blit(&glyph, rect);
    }
}

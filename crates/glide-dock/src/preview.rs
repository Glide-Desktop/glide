use std::collections::HashMap;
use std::time::Duration;

use glide_theme::DockTheme;
use glide_theme::motion::{ease_io, ease_out, t1, t2};
use smithay::backend::renderer::element::{
    Kind,
    memory::{MemoryRenderBuffer, MemoryRenderBufferRenderElement},
};
use smithay::utils::{Logical, Point, Rectangle, Scale, Size};

use glide_paint::{Painter, TextAlign};
use glide_shell::raster::{Canvas, ShellRenderer, rect};

pub type Rect = Rectangle<i32, Logical>;

const MAX_THUMB_W: i32 = 200;
const MIN_THUMB_W: i32 = 96;
const THUMB_H: i32 = 120;
const PAD: i32 = glide_theme::spacing::SP3;
const GAP: i32 = glide_theme::spacing::SP2;
const LABEL_GAP: i32 = 6;
const LABEL_H: i32 = 16;
const ANCHOR_GAP: i32 = glide_theme::spacing::SP2;
const EDGE_MARGIN: i32 = glide_theme::spacing::SP2;
const RING: f32 = 3.0;
const CLOSE: i32 = 22;
const CLOSE_INSET: i32 = 6;
const CLOSE_GLYPH: f32 = 8.0;
const CLOSE_STROKE: f32 = 1.6;
const RISE: f64 = 6.0;
const MAX_THUMBS: usize = 6;
const MINIMIZED_ALPHA: f32 = 0.55;
const RADIUS: f32 = glide_theme::shape::R3;
pub const THUMB_RADIUS: f32 = glide_theme::shape::R2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewWindow {
    pub id: u64,
    pub title: String,
    pub minimized: bool,
    pub size: (i32, i32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PreviewHit {
    Window(u64),
    Close(u64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thumb {
    pub id: u64,
    pub frame: Rect,
    pub content: Rect,
    pub label: Rect,
    pub close: Rect,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewLayout {
    pub panel: Rect,
    pub bridge: Rect,
    pub thumbs: Vec<Thumb>,
}

impl PreviewLayout {
    #[must_use]
    pub fn keeps(&self, pos: Point<f64, Logical>) -> bool {
        self.panel.to_f64().contains(pos) || self.bridge.to_f64().contains(pos)
    }

    #[must_use]
    pub fn hit(&self, pos: Point<f64, Logical>) -> Option<PreviewHit> {
        self.thumbs.iter().find_map(|thumb| {
            if thumb.close.to_f64().contains(pos) {
                Some(PreviewHit::Close(thumb.id))
            } else if thumb.frame.to_f64().contains(pos) || thumb.label.to_f64().contains(pos) {
                Some(PreviewHit::Window(thumb.id))
            } else {
                None
            }
        })
    }
}

fn fitted((width, height): (i32, i32)) -> (i32, i32) {
    let (width, height) = (f64::from(width.max(1)), f64::from(height.max(1)));
    let factor = (f64::from(MAX_THUMB_W) / width).min(f64::from(THUMB_H) / height).min(1.0);
    ((width * factor).round() as i32, (height * factor).round() as i32)
}

#[must_use]
pub fn layout(windows: &[PreviewWindow], tile: Rect, lift: i32, area: Rect) -> PreviewLayout {
    let room = area.size.w - EDGE_MARGIN * 2 - PAD * 2;
    let mut boxes: Vec<(u64, (i32, i32), i32)> = Vec::new();
    let mut used = 0;
    for window in windows.iter().take(MAX_THUMBS) {
        let content = fitted(window.size);
        let width = content.0.max(MIN_THUMB_W);
        let next = if boxes.is_empty() { width } else { used + GAP + width };
        if next > room && !boxes.is_empty() {
            break;
        }
        used = next;
        boxes.push((window.id, content, width));
    }
    let width = PAD * 2 + used;
    let height = PAD + THUMB_H + LABEL_GAP + LABEL_H + PAD;
    let min_x = area.loc.x + EDGE_MARGIN;
    let max_x = (area.loc.x + area.size.w - EDGE_MARGIN - width).max(min_x);
    let x = (tile.loc.x + (tile.size.w - width) / 2).clamp(min_x, max_x);
    let y = tile.loc.y - lift - ANCHOR_GAP - height;
    let panel = rect(x, y, width, height);
    let bridge_top = y + height;
    let bridge = rect(tile.loc.x, bridge_top, tile.size.w, (tile.loc.y + tile.size.h - bridge_top).max(0));
    let mut cursor = x + PAD;
    let thumbs = boxes
        .into_iter()
        .map(|(id, (content_w, content_h), box_w)| {
            let frame = rect(cursor, y + PAD, box_w, THUMB_H);
            let content =
                rect(cursor + (box_w - content_w) / 2, y + PAD + (THUMB_H - content_h) / 2, content_w, content_h);
            let label = rect(cursor, y + PAD + THUMB_H + LABEL_GAP, box_w, LABEL_H);
            let close = rect(cursor + box_w - CLOSE_INSET - CLOSE, y + PAD + CLOSE_INSET, CLOSE, CLOSE);
            cursor += box_w + GAP;
            Thumb { id, frame, content, label, close }
        })
        .collect();
    PreviewLayout { panel, bridge, thumbs }
}

type PanelKey = (Vec<(u64, String, bool, i32)>, u64, i32);

#[derive(Debug)]
pub struct Preview {
    pub index: usize,
    pub app_id: String,
    windows: Vec<PreviewWindow>,
    reveal: glide_ui::Reveal,
    hover: glide_ui::HoverAnimation<u64>,
    hovered: Option<PreviewHit>,
    close_hover: glide_ui::Tween,
    swap: glide_ui::Tween,
    panel: Option<(PanelKey, MemoryRenderBuffer)>,
    rings: HashMap<(i32, i32, i32), MemoryRenderBuffer>,
    closes: HashMap<(bool, i32), MemoryRenderBuffer>,
    shadows: HashMap<(i32, i32, i32), MemoryRenderBuffer>,
}

impl Preview {
    #[must_use]
    pub fn new(index: usize, app_id: String, windows: Vec<PreviewWindow>) -> Self {
        let mut reveal = glide_ui::Reveal::closed().with_ease(ease_out);
        reveal.open(t2());
        Self {
            index,
            app_id,
            windows,
            reveal,
            hover: glide_ui::HoverAnimation::default(),
            hovered: None,
            close_hover: glide_ui::Tween::at(0.0),
            swap: glide_ui::Tween::at(1.0),
            panel: None,
            rings: HashMap::new(),
            closes: HashMap::new(),
            shadows: HashMap::new(),
        }
    }

    #[must_use]
    pub fn windows(&self) -> &[PreviewWindow] {
        &self.windows
    }

    pub fn set_windows(&mut self, windows: Vec<PreviewWindow>) -> bool {
        if self.windows == windows {
            return false;
        }
        self.windows = windows;
        self.swap = glide_ui::Tween::at(0.35);
        self.swap.retarget(1.0, t2(), ease_out);
        true
    }

    #[must_use]
    pub fn is_open(&self) -> bool {
        self.reveal.is_open()
    }

    pub fn reopen(&mut self) {
        self.reveal.open(t2());
    }

    pub fn close(&mut self) {
        self.reveal.close(t2());
        self.set_hover(None);
    }

    #[must_use]
    pub fn is_visible(&self) -> bool {
        self.reveal.is_open() || self.reveal.is_visible(t2())
    }

    pub fn settle(&mut self) {
        self.reveal.settle(t2());
    }

    #[must_use]
    pub fn is_animating(&self) -> bool {
        let reveal = self.reveal.is_animating(t2());
        let hover = self.hover.is_animating(t1());
        let close = self.close_hover.is_animating(t1());
        reveal || hover || close || self.swap.is_animating(t2())
    }

    #[must_use]
    pub const fn hovered(&self) -> Option<PreviewHit> {
        self.hovered
    }

    pub fn set_hover(&mut self, hit: Option<PreviewHit>) -> bool {
        if self.hovered == hit {
            return false;
        }
        self.hovered = hit;
        let window = hit.map(|hit| match hit {
            PreviewHit::Window(id) | PreviewHit::Close(id) => id,
        });
        self.hover.set_with(window, t1());
        let over_close = f64::from(u8::from(matches!(hit, Some(PreviewHit::Close(_)))));
        self.close_hover.retarget(over_close, t1(), ease_io);
        true
    }

    fn progress(&self) -> f64 {
        self.reveal.value(t2()).clamp(0.0, 1.0)
    }

    pub fn clear_rasters(&mut self) {
        self.panel = None;
        self.rings.clear();
        self.closes.clear();
        self.shadows.clear();
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Live {
    pub id: u64,
    pub content: Rect,
    pub alpha: f32,
}

pub struct Drawn<R: smithay::backend::renderer::Renderer> {
    pub top: Vec<MemoryRenderBufferRenderElement<R>>,
    pub live: Vec<Live>,
    pub base: Vec<MemoryRenderBufferRenderElement<R>>,
}

fn panel_key(preview: &Preview, layout: &PreviewLayout, revision: u64, scale: i32) -> PanelKey {
    let labels = layout
        .thumbs
        .iter()
        .filter_map(|thumb| {
            let window = preview.windows.iter().find(|window| window.id == thumb.id)?;
            Some((thumb.id, window.title.clone(), window.minimized, thumb.frame.size.w))
        })
        .collect();
    (labels, revision, scale)
}

fn panel_canvas(
    painter: &Painter, theme: &DockTheme, preview: &Preview, layout: &PreviewLayout, scale: i32,
) -> Option<Canvas<'static>> {
    let panel = layout.panel;
    let mut c = Canvas::popover((panel.size.w, panel.size.h), scale, RADIUS, theme.menu_background, theme.menu_border)?;
    let local = |area: Rect| rect(area.loc.x - panel.loc.x, area.loc.y - panel.loc.y, area.size.w, area.size.h);
    for thumb in &layout.thumbs {
        let Some(window) = preview.windows.iter().find(|window| window.id == thumb.id) else {
            continue;
        };
        c.fill(local(thumb.frame), theme.fallback_background, THUMB_RADIUS);
        let colour = if window.minimized { theme.menu_text_muted } else { theme.menu_text };
        c.text_in(painter, local(thumb.label), TextAlign::Center, &window.title, colour, &glide_theme::TextStyle::CAP);
    }
    Some(c)
}

fn ring_canvas(size: Size<i32, Logical>, theme: &DockTheme, scale: i32) -> Option<Canvas<'static>> {
    let mut c = Canvas::new(size.w, size.h, scale)?;
    let factor = scale as f32;
    let (width, height) = (size.w * scale, size.h * scale);
    glide_paint::draw_inset_border(
        &mut c.buf,
        width,
        height,
        RING * factor,
        THUMB_RADIUS * factor,
        theme.focused_color,
    );
    Some(c)
}

fn close_canvas(theme: &DockTheme, hovered: bool, scale: i32) -> Option<Canvas<'static>> {
    let mut c = Canvas::new(CLOSE, CLOSE, scale)?;
    let (fill, glyph) =
        if hovered { (theme.menu_text_danger, theme.badge_text) } else { (theme.menu_background, theme.menu_text) };
    c.fill(c.full(), fill, glide_theme::shape::R_ROUND);
    glide_paint::draw_close_glyph_over(&mut c.buf, CLOSE, CLOSE, CLOSE_GLYPH, CLOSE_STROKE, scale, glyph);
    Some(c)
}

fn element<R: ShellRenderer>(
    renderer: &mut R, buffer: &MemoryRenderBuffer, at: Point<i32, Logical>, alpha: f32, scale: Scale<f64>,
) -> Option<MemoryRenderBufferRenderElement<R>> {
    MemoryRenderBufferRenderElement::from_buffer(
        renderer,
        at.to_f64().to_physical(scale),
        buffer,
        Some(alpha),
        None,
        None,
        Kind::Unspecified,
    )
    .ok()
}

pub fn render<R: ShellRenderer>(
    preview: &mut Preview, renderer: &mut R, painter: &Painter, theme: &DockTheme, layout: &PreviewLayout,
    (scale, output_scale, revision): (Scale<f64>, i32, u64),
) -> Drawn<R> {
    let mut drawn = Drawn { top: Vec::new(), live: Vec::new(), base: Vec::new() };
    let progress = preview.progress();
    if progress <= 0.0 {
        return drawn;
    }
    let alpha = glide_theme::motion::assist(progress) as f32;
    let rise = (RISE * (1.0 - progress)).round() as i32;
    let shift = |at: Point<i32, Logical>| Point::from((at.x, at.y + rise));

    let key = panel_key(preview, layout, revision, output_scale);
    if preview.panel.as_ref().is_none_or(|(cached, _)| *cached != key)
        && let Some(canvas) = panel_canvas(painter, theme, preview, layout, output_scale)
    {
        preview.panel = Some((key, canvas.finish()));
    }

    for thumb in &layout.thumbs {
        let level = preview.hover.level(thumb.id, t1()) as f32;
        if level > 0.0 {
            let size = thumb.frame.size;
            let ring_key = (size.w, size.h, output_scale);
            if !preview.rings.contains_key(&ring_key)
                && let Some(canvas) = ring_canvas(size, theme, output_scale)
            {
                preview.rings.insert(ring_key, canvas.finish());
            }
            for hovered in [false, true] {
                if !preview.closes.contains_key(&(hovered, output_scale))
                    && let Some(canvas) = close_canvas(theme, hovered, output_scale)
                {
                    preview.closes.insert((hovered, output_scale), canvas.finish());
                }
            }
            let over = preview.close_hover.value(t1(), ease_io) as f32;
            let on_close = matches!(preview.hovered, Some(PreviewHit::Close(id)) if id == thumb.id);
            let danger = if on_close { over } else { 0.0 };
            for (hovered, weight) in [(true, danger), (false, 1.0 - danger)] {
                if weight > 0.0
                    && let Some(buffer) = preview.closes.get(&(hovered, output_scale))
                    && let Some(element) =
                        element(renderer, buffer, shift(thumb.close.loc), alpha * level * weight, scale)
                {
                    drawn.top.push(element);
                }
            }
            if let Some(buffer) = preview.rings.get(&ring_key)
                && let Some(element) = element(renderer, buffer, shift(thumb.frame.loc), alpha * level, scale)
            {
                drawn.top.push(element);
            }
        }
        let minimized = preview.windows.iter().any(|window| window.id == thumb.id && window.minimized);
        let content = Rectangle::new(shift(thumb.content.loc), thumb.content.size);
        let settled = preview.swap.value(t2(), ease_out) as f32;
        drawn.live.push(Live {
            id: thumb.id,
            content,
            alpha: alpha * settled * if minimized { MINIMIZED_ALPHA } else { 1.0 },
        });
    }

    if let Some((_, buffer)) = preview.panel.as_ref()
        && let Some(element) = element(renderer, buffer, shift(layout.panel.loc), alpha, scale)
    {
        drawn.base.push(element);
    }
    let size = layout.panel.size;
    let shadow_key = (size.w, size.h, output_scale);
    if !preview.shadows.contains_key(&shadow_key)
        && let Some(buffer) = glide_shell::popup_menu::draw_elevation(
            size,
            RADIUS,
            output_scale.max(1),
            glide_theme::elevation::Level::El3,
            theme.appearance,
        )
    {
        preview.shadows.insert(shadow_key, buffer);
    }
    let margin = glide_shell::popup_menu::shadow_margin();
    if let Some(buffer) = preview.shadows.get(&shadow_key) {
        let at = shift(Point::from((layout.panel.loc.x - margin, layout.panel.loc.y - margin)));
        if let Some(element) = element(renderer, buffer, at, alpha, scale) {
            drawn.base.push(element);
        }
    }
    drawn
}

#[must_use]
pub fn dwell() -> Duration {
    crate::TOOLTIP_DELAY
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area() -> Rect {
        rect(0, 0, 1920, 1080)
    }

    fn tile() -> Rect {
        rect(900, 1000, 56, 56)
    }

    fn window(id: u64, size: (i32, i32)) -> PreviewWindow {
        PreviewWindow { id, title: format!("Window {id}"), minimized: false, size }
    }

    #[test]
    fn the_panel_sits_above_its_tile_and_every_thumb_keeps_its_windows_shape() {
        let windows = [window(1, (1600, 900)), window(2, (400, 800))];
        let layout = layout(&windows, tile(), 4, area());
        let panel = layout.panel;
        assert_eq!(panel.loc.y + panel.size.h, tile().loc.y - 4 - ANCHOR_GAP);
        assert_eq!(panel.loc.x + panel.size.w / 2, tile().loc.x + tile().size.w / 2, "centred on the tile");
        let [wide, tall] = &layout.thumbs[..] else { panic!("two thumbs") };
        assert_eq!(wide.content.size, Size::from((200, 113)));
        assert_eq!(tall.content.size, Size::from((60, 120)));
        assert_eq!(tall.frame.size.w, MIN_THUMB_W, "a narrow window still gets a usable target");
        for thumb in &layout.thumbs {
            assert!(thumb.frame.contains_rect(thumb.content));
            assert!(thumb.frame.contains_rect(thumb.close));
            assert!(panel.contains_rect(thumb.frame) && panel.contains_rect(thumb.label));
        }
        assert_eq!(fitted((100, 50)), (100, 50), "a small window is never blown up");
    }

    #[test]
    fn the_panel_stays_on_screen_and_drops_what_does_not_fit() {
        let edge = rect(4, 1000, 56, 56);
        let layout = layout(&[window(1, (800, 600))], edge, 0, area());
        assert_eq!(layout.panel.loc.x, EDGE_MARGIN);
        let many: Vec<PreviewWindow> = (0..10).map(|id| window(id, (1600, 900))).collect();
        assert_eq!(super::layout(&many, tile(), 0, area()).thumbs.len(), MAX_THUMBS);
        let narrow = rect(0, 0, 500, 1080);
        let squeezed = super::layout(&many, rect(200, 1000, 56, 56), 0, narrow);
        assert!(squeezed.panel.size.w <= narrow.size.w - EDGE_MARGIN * 2);
        assert!(!squeezed.thumbs.is_empty());
    }

    #[test]
    fn the_pointer_can_travel_from_tile_to_panel_and_hits_resolve_to_windows_or_close() {
        let layout = layout(&[window(7, (1280, 720))], tile(), 4, area());
        let gap = Point::from((f64::from(tile().loc.x + 10), f64::from(tile().loc.y - 2)));
        assert!(layout.keeps(gap), "the gap between tile and panel is bridged");
        let thumb = &layout.thumbs[0];
        let centre = |area: Rect| {
            Point::from((f64::from(area.loc.x + area.size.w / 2), f64::from(area.loc.y + area.size.h / 2)))
        };
        assert_eq!(layout.hit(centre(thumb.close)), Some(PreviewHit::Close(7)));
        assert_eq!(layout.hit(centre(thumb.content)), Some(PreviewHit::Window(7)));
        assert_eq!(layout.hit(centre(thumb.label)), Some(PreviewHit::Window(7)));
        assert_eq!(layout.hit(Point::from((0.0, 0.0))), None);
        assert!(!layout.keeps(Point::from((0.0, 0.0))));
    }

    #[test]
    fn hovering_moves_the_ring_and_closing_fades_the_panel() {
        let mut preview = Preview::new(0, "mail".into(), vec![window(1, (800, 600)), window(2, (800, 600))]);
        assert!(preview.is_open() && preview.is_visible());
        assert!(preview.set_hover(Some(PreviewHit::Window(1))) && !preview.set_hover(Some(PreviewHit::Window(1))));
        assert!(preview.set_hover(Some(PreviewHit::Close(1))));
        assert_eq!(preview.hovered(), Some(PreviewHit::Close(1)));
        assert!(!preview.set_windows(preview.windows().to_vec()));
        assert!(preview.set_windows(vec![window(1, (800, 600))]));
        preview.close();
        assert!(!preview.is_open());
        assert_eq!(preview.hovered(), None);
    }

    #[test]
    #[ignore = "writes preview PNGs to $GLIDE_DOCK_PREVIEW"]
    fn chrome_preview() {
        use glide_shell::image::Image;
        let Some(dir) = std::env::var_os("GLIDE_DOCK_PREVIEW") else {
            return;
        };
        let painter = Painter::new(14.0, None);
        let scale = 2;
        for appearance in [glide_theme::Appearance::Dark, glide_theme::Appearance::Light] {
            let theme = DockTheme::from_tokens(&glide_theme::Tokens::new(appearance, glide_theme::Accent::Blue));
            let windows = vec![
                PreviewWindow { id: 1, title: "Inbox — Mail".into(), minimized: false, size: (1600, 1000) },
                PreviewWindow {
                    id: 2,
                    title: "Draft: Quarterly report for the team".into(),
                    minimized: true,
                    size: (900, 1100),
                },
            ];
            let preview = Preview::new(0, "mail".into(), windows);
            let layout = layout(preview.windows(), rect(300, 400, 56, 56), 4, rect(0, 0, 800, 480));
            let origin = layout.panel.loc;
            let (w, h) = ((layout.panel.size.w * scale) as u32, (layout.panel.size.h * scale) as u32);
            let mut image = Image::new(w + 64, h + 64).expect("image");
            let shade = if appearance == glide_theme::Appearance::Dark { 30 } else { 230 };
            let backdrop = vec![shade; ((w + 64) * (h + 64) * 4) as usize];
            image.draw(&Image::from_bgra(w + 64, h + 64, backdrop).expect("backdrop"), 0, 0);
            let mut place = |canvas: &Canvas<'static>, at: Point<i32, Logical>| {
                let (cw, ch) = ((canvas.w * scale) as u32, (canvas.h * scale) as u32);
                let sprite = Image::from_bgra(cw, ch, canvas.buf.clone()).expect("sprite");
                image.draw(&sprite, 32 + (at.x - origin.x) * scale, 32 + (at.y - origin.y) * scale);
            };
            place(&panel_canvas(&painter, &theme, &preview, &layout, scale).expect("panel"), origin);
            for (index, thumb) in layout.thumbs.iter().enumerate() {
                let mut window = Canvas::new(thumb.content.size.w, thumb.content.size.h, scale).expect("window");
                window.panel(theme.background, 0.0);
                place(&window, thumb.content.loc);
                if index == 0 {
                    place(&ring_canvas(thumb.frame.size, &theme, scale).expect("ring"), thumb.frame.loc);
                    place(&close_canvas(&theme, true, scale).expect("close"), thumb.close.loc);
                }
            }
            let png = image.png().expect("png");
            let file = format!("dock-preview-{appearance:?}.png").to_lowercase();
            std::fs::write(std::path::Path::new(&dir).join(file), png).expect("write");
        }
    }
}

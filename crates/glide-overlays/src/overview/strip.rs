use std::collections::HashMap;

use glide_layout::WorkspaceId;
use glide_theme::{OverviewTheme, WallpaperFit};
use smithay::{
    backend::renderer::element::memory::MemoryRenderBuffer,
    utils::{Logical, Point, Rectangle, Scale, Size},
};

use super::{OverviewElement, PerScale, RasterKey, hover_anim, place_in};
use crate::raster::ShellRenderer;

const TOP_GAP: i32 = 16;
const MIN_HEIGHT: i32 = 72;
const MAX_HEIGHT: i32 = 120;
const HEIGHT_SHARE: f64 = 0.1;
const GAP: i32 = 14;
const ADD_WIDTH: i32 = 56;
const CAPTION_GAP: i32 = 6;
const CAPTION_HEIGHT: i32 = 16;
const BOTTOM_GAP: i32 = glide_theme::spacing::SP5;
pub(super) const RISE: f64 = 24.0;
const LIFT: f64 = 2.0;
const DROP_GROW: f64 = 0.08;
const BORDER: f32 = 1.5;
const DROP_BORDER: f32 = 2.0;
const DROP_TINT: f32 = 0.18;
const RING: f32 = 2.0;
const RING_GAP: i32 = 3;
const SHADOW_DY: f32 = 2.0;
const SHADOW_BLUR: f32 = 8.0;
const SHADOW_ALPHA: f32 = 0.28;
const BUCKETS: f64 = 12.0;
const PLUS: i32 = 18;
const PLUS_STROKE: i32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Slot {
    Workspace(WorkspaceId),
    Add,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WallKey {
    size: (i32, i32),
    generation: u64,
    fit: WallpaperFit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CaptionKey {
    text: String,
    active: bool,
    raster: RasterKey,
}

type Sized = (RasterKey, (i32, i32), MemoryRenderBuffer);

#[derive(Debug)]
struct Opening(glide_ui::Tween);

impl Default for Opening {
    fn default() -> Self {
        Self(glide_ui::Tween::at(0.0))
    }
}

impl std::ops::Deref for Opening {
    type Target = glide_ui::Tween;

    fn deref(&self) -> &glide_ui::Tween {
        &self.0
    }
}

impl std::ops::DerefMut for Opening {
    fn deref_mut(&mut self) -> &mut glide_ui::Tween {
        &mut self.0
    }
}

#[derive(Debug, Default)]
pub(super) struct Strip {
    workspaces: Vec<WorkspaceId>,
    active: Option<WorkspaceId>,
    can_add: bool,
    hover: glide_ui::HoverAnimation<Slot>,
    drop: glide_ui::HoverAnimation<Slot>,
    ring: Option<glide_ui::Tween>,
    add_open: Opening,
    wallpaper: PerScale<(WallKey, MemoryRenderBuffer)>,
    frames: PerScale<HashMap<(u8, u8), Sized>>,
    ring_buffer: PerScale<Sized>,
    captions: PerScale<HashMap<usize, (CaptionKey, i32, MemoryRenderBuffer)>>,
    add_tile: PerScale<HashMap<u8, Sized>>,
    plus: PerScale<(RasterKey, MemoryRenderBuffer)>,
}

fn slide() -> std::time::Duration {
    glide_theme::motion::t3()
}

fn bucket(level: f64) -> u8 {
    (level.clamp(0.0, 1.0) * BUCKETS).round() as u8
}

#[must_use]
pub(super) fn thumb_size(area: Rectangle<i32, Logical>, output: Rectangle<i32, Logical>) -> Size<i32, Logical> {
    let height = (f64::from(area.size.h) * HEIGHT_SHARE).round() as i32;
    let height = height.clamp(MIN_HEIGHT, MAX_HEIGHT);
    let aspect = f64::from(output.size.w.max(1)) / f64::from(output.size.h.max(1));
    Size::from(((f64::from(height) * aspect).round() as i32, height))
}

#[must_use]
pub(super) fn band(area: Rectangle<i32, Logical>, output: Rectangle<i32, Logical>) -> i32 {
    TOP_GAP + thumb_size(area, output).h + CAPTION_GAP + CAPTION_HEIGHT + BOTTOM_GAP
}

fn margin() -> i32 {
    glide_paint::shadow_fade_margin(SHADOW_BLUR.ceil() as i32)
}

fn grown(rect: Rectangle<f64, Logical>, factor: f64) -> Rectangle<f64, Logical> {
    let size = Size::from((rect.size.w * factor, rect.size.h * factor));
    let loc = Point::from((rect.loc.x - (size.w - rect.size.w) / 2.0, rect.loc.y - (size.h - rect.size.h) / 2.0));
    Rectangle::new(loc, size)
}

fn shifted(rect: Rectangle<f64, Logical>, dy: f64) -> Rectangle<f64, Logical> {
    Rectangle::new(Point::from((rect.loc.x, rect.loc.y + dy)), rect.size)
}

impl Strip {
    pub(super) fn set(&mut self, workspaces: Vec<WorkspaceId>, active: WorkspaceId, can_add: bool) {
        self.workspaces = workspaces;
        self.active = Some(active);
        self.can_add = can_add;
        let index = self.index_of(active).unwrap_or(0) as f64;
        match self.ring.as_mut() {
            Some(ring) => {
                ring.retarget(index, slide(), glide_theme::motion::ease_out);
            }
            None => self.ring = Some(glide_ui::Tween::at(index)),
        }
        let (workspaces, can_add) = (&self.workspaces, self.can_add);
        let gone = |slot: Slot| match slot {
            Slot::Workspace(id) => !workspaces.contains(&id),
            Slot::Add => !can_add,
        };
        for animation in [&mut self.hover, &mut self.drop] {
            if animation.target().is_some_and(gone) {
                animation.retarget_without_restarting(None);
            }
        }
    }

    fn index_of(&self, id: WorkspaceId) -> Option<usize> {
        self.workspaces.iter().position(|ws| *ws == id)
    }

    pub(super) fn set_dragging(&mut self, dragging: bool) {
        self.add_open.retarget(if dragging { 1.0 } else { 0.0 }, slide(), glide_theme::motion::ease_out);
    }

    pub(super) fn set_hover(&mut self, hover: Option<Slot>) -> bool {
        self.hover.set_with(hover, hover_anim())
    }

    pub(super) fn set_drop(&mut self, slot: Option<Slot>) -> bool {
        self.drop.set_with(slot, hover_anim())
    }

    pub(super) fn is_animating(&self) -> bool {
        self.hover.is_animating(hover_anim())
            || self.drop.is_animating(hover_anim())
            || self.ring.as_ref().is_some_and(|ring| ring.is_animating(slide()))
            || self.add_open.is_animating(slide())
    }

    fn add_open(&self) -> f64 {
        self.add_open.value(slide(), glide_theme::motion::ease_out)
    }

    pub(super) fn slots(
        &self, area: Rectangle<i32, Logical>, output: Rectangle<i32, Logical>,
    ) -> Vec<(Slot, Rectangle<f64, Logical>)> {
        let thumb = thumb_size(area, output).to_f64();
        let add = f64::from(ADD_WIDTH) + (thumb.w - f64::from(ADD_WIDTH)) * self.add_open();
        let count = self.workspaces.len() as f64;
        let gaps = (count - 1.0).max(0.0) * f64::from(GAP);
        let total = count * thumb.w + gaps + if self.can_add { f64::from(GAP) + add } else { 0.0 };
        let mut x = f64::from(area.loc.x) + (f64::from(area.size.w) - total) / 2.0;
        let y = f64::from(area.loc.y + TOP_GAP);
        let mut slots = Vec::with_capacity(self.workspaces.len() + 1);
        for id in &self.workspaces {
            slots.push((Slot::Workspace(*id), Rectangle::new(Point::from((x, y)), thumb)));
            x += thumb.w + f64::from(GAP);
        }
        if self.can_add {
            slots.push((Slot::Add, Rectangle::new(Point::from((x, y)), Size::from((add, thumb.h)))));
        }
        slots
    }

    pub(super) fn slot_at(
        &self, area: Rectangle<i32, Logical>, output: Rectangle<i32, Logical>, pos: Point<f64, Logical>,
    ) -> Option<Slot> {
        self.slots(area, output).into_iter().find(|(_, rect)| rect.contains(pos)).map(|(slot, _)| slot)
    }

    pub(super) fn drawn(&self, slot: Slot, base: Rectangle<f64, Logical>, rise: f64) -> Rectangle<f64, Logical> {
        let hover = glide_theme::motion::E_OVER.ease_f64(self.hover.level(slot, hover_anim()));
        let drop = glide_theme::motion::E_OVER.ease_f64(self.drop.level(slot, hover_anim()));
        let lifted = shifted(base, -rise - hover * LIFT);
        if matches!(slot, Slot::Add) { lifted } else { grown(lifted, DROP_GROW.mul_add(drop, 1.0)) }
    }

    pub(super) fn thumb_rect(
        &self, area: Rectangle<i32, Logical>, output: Rectangle<i32, Logical>, workspace: WorkspaceId, rise: f64,
    ) -> Option<Rectangle<f64, Logical>> {
        self.slots(area, output)
            .into_iter()
            .find(|(slot, _)| *slot == Slot::Workspace(workspace))
            .map(|(slot, base)| self.drawn(slot, base, rise))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_chrome<R: ShellRenderer>(
        &mut self, renderer: &mut R, theme: &OverviewTheme, painter: &crate::paint::Painter,
        (area, output): (Rectangle<i32, Logical>, Rectangle<i32, Logical>), (alpha, rise): (f32, f64),
        (scale, output_scale): (Scale<f64>, i32), elements: &mut Vec<OverviewElement<R>>,
    ) {
        if alpha <= 0.0 {
            return;
        }
        let key = RasterKey::new(theme, output_scale);
        let thumb = thumb_size(area, output);
        let slots = self.slots(area, output);

        if let (Some(ring), Some((_, first))) = (self.ring.as_ref(), slots.first()) {
            let index = ring.value(slide(), glide_theme::motion::ease_out);
            let outset = f64::from(RING_GAP) + f64::from(RING);
            let x = first.loc.x + index * f64::from(thumb.w + GAP) - outset;
            let size = (thumb.w + 2 * (RING_GAP + RING as i32), thumb.h + 2 * (RING_GAP + RING as i32));
            let outdated =
                self.ring_buffer.get(&output_scale).is_none_or(|(cached, at, _)| *cached != key || *at != size);
            if outdated && let Some(memory) = draw_ring(theme, size, output_scale) {
                self.ring_buffer.insert(output_scale, (key, size, memory));
            }
            if let Some((_, size, memory)) = self.ring_buffer.get(&output_scale) {
                let rect = Rectangle::new(Point::from((x, first.loc.y - outset - rise)), Size::from(*size).to_f64());
                elements.extend(place_in(renderer, rect, *size, memory, alpha, scale).map(OverviewElement::Buffer));
            }
        }

        let margin = margin();
        let mut captions = Vec::new();
        for (index, (slot, base)) in slots.iter().enumerate() {
            let drawn = self.drawn(*slot, *base, rise);
            let hover = self.hover.level(*slot, hover_anim());
            let drop = self.drop.level(*slot, hover_anim());
            match slot {
                Slot::Workspace(id) => {
                    let buckets = (bucket(hover), bucket(drop));
                    let size = (thumb.w, thumb.h);
                    let frames = self.frames.entry(output_scale).or_default();
                    let outdated = frames.get(&buckets).is_none_or(|(cached, at, _)| *cached != key || *at != size);
                    if outdated && let Some(memory) = draw_frame(theme, size, buckets, output_scale) {
                        frames.insert(buckets, (key, size, memory));
                    }
                    if let Some((_, _, memory)) = frames.get(&buckets) {
                        let factor = drawn.size.w / f64::from(thumb.w);
                        let outer = Rectangle::new(
                            Point::from((
                                drawn.loc.x - f64::from(margin) * factor,
                                drawn.loc.y - f64::from(margin) * factor,
                            )),
                            Size::from((
                                f64::from(thumb.w + 2 * margin) * factor,
                                f64::from(thumb.h + 2 * margin) * factor,
                            )),
                        );
                        let logical = (thumb.w + 2 * margin, thumb.h + 2 * margin);
                        elements.extend(
                            place_in(renderer, outer, logical, memory, alpha, scale).map(OverviewElement::Buffer),
                        );
                    }
                    let active = self.active == Some(*id);
                    let text =
                        crate::i18n::t(crate::i18n::Key::OverviewDesktop).replace("{}", &(index + 1).to_string());
                    captions.push((index, text, active, *base, 1.0));
                }
                Slot::Add => {
                    let open = self.add_open();
                    self.render_add(renderer, theme, drawn, drop, (alpha, key), (scale, output_scale), elements);
                    if open > 0.0 {
                        let text = crate::i18n::t(crate::i18n::Key::OverviewNewDesktop).to_string();
                        captions.push((usize::MAX, text, false, *base, glide_theme::motion::assist(open)));
                    }
                }
            }
        }

        for (index, text, active, base, share) in captions {
            let width = painter.text_width_styled(&text, &glide_theme::TextStyle::CAP).ceil() as i32 + 2;
            let caption = CaptionKey { text, active, raster: key };
            let cache = self.captions.entry(output_scale).or_default();
            let outdated = cache.get(&index).is_none_or(|(cached, _, _)| *cached != caption);
            if outdated {
                let colour = if active { theme.strip_caption_active } else { theme.strip_caption };
                let style = if active {
                    glide_theme::TextStyle { weight: 600, ..glide_theme::TextStyle::CAP }
                } else {
                    glide_theme::TextStyle::CAP
                };
                if let Some(mut canvas) = crate::raster::Canvas::new(width, CAPTION_HEIGHT, output_scale) {
                    canvas.label(painter, (0, width), crate::paint::TextAlign::Center, &caption.text, colour, &style);
                    cache.insert(index, (caption.clone(), width, canvas.finish()));
                }
            }
            if let Some((_, width, memory)) = cache.get(&index) {
                let rect = Rectangle::new(
                    Point::from((
                        base.loc.x + (base.size.w - f64::from(*width)) / 2.0,
                        base.loc.y + base.size.h + f64::from(CAPTION_GAP) - rise,
                    )),
                    Size::from((f64::from(*width), f64::from(CAPTION_HEIGHT))),
                );
                let shown = alpha * share as f32;
                elements.extend(
                    place_in(renderer, rect, (*width, CAPTION_HEIGHT), memory, shown, scale)
                        .map(OverviewElement::Buffer),
                );
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_add<R: ShellRenderer>(
        &mut self, renderer: &mut R, theme: &OverviewTheme, drawn: Rectangle<f64, Logical>, drop: f64,
        (alpha, key): (f32, RasterKey), (scale, output_scale): (Scale<f64>, i32),
        elements: &mut Vec<OverviewElement<R>>,
    ) {
        let plus_stale = self.plus.get(&output_scale).is_none_or(|(cached, _)| *cached != key);
        if plus_stale && let Some(memory) = draw_plus(theme, output_scale) {
            self.plus.insert(output_scale, (key, memory));
        }
        if let Some((_, memory)) = self.plus.get(&output_scale) {
            let side = f64::from(PLUS);
            let rect = Rectangle::new(
                Point::from((drawn.loc.x + (drawn.size.w - side) / 2.0, drawn.loc.y + (drawn.size.h - side) / 2.0)),
                Size::from((side, side)),
            );
            elements.extend(place_in(renderer, rect, (PLUS, PLUS), memory, alpha, scale).map(OverviewElement::Buffer));
        }
        let level = bucket(drop);
        let size = (ADD_WIDTH, drawn.size.h.round() as i32);
        let tiles = self.add_tile.entry(output_scale).or_default();
        let outdated = tiles.get(&level).is_none_or(|(cached, at, _)| *cached != key || *at != size);
        if outdated && let Some(memory) = draw_add(theme, size, f64::from(level) / BUCKETS, output_scale) {
            tiles.insert(level, (key, size, memory));
        }
        if let Some((_, size, memory)) = tiles.get(&level) {
            elements.extend(place_in(renderer, drawn, *size, memory, alpha, scale).map(OverviewElement::Buffer));
        }
    }

    pub(super) fn render_wallpapers<R: ShellRenderer>(
        &mut self, renderer: &mut R, (area, output): (Rectangle<i32, Logical>, Rectangle<i32, Logical>),
        (alpha, rise, fit): (f32, f64, WallpaperFit), (scale, output_scale): (Scale<f64>, i32),
        elements: &mut Vec<OverviewElement<R>>,
    ) {
        if alpha <= 0.0 {
            return;
        }
        let thumb = thumb_size(area, output);
        let wall = WallKey { size: (thumb.w, thumb.h), generation: glide_paint::wallpaper_generation(), fit };
        if self.wallpaper.get(&output_scale).is_none_or(|(cached, _)| *cached != wall)
            && let Some(memory) = draw_wallpaper(thumb, fit, output_scale)
        {
            self.wallpaper.insert(output_scale, (wall, memory));
        }
        let Some((_, memory)) = self.wallpaper.get(&output_scale) else {
            return;
        };
        for (slot, base) in self.slots(area, output) {
            if matches!(slot, Slot::Add) {
                continue;
            }
            let drawn = self.drawn(slot, base, rise);
            elements.extend(
                place_in(renderer, drawn, (thumb.w, thumb.h), memory, alpha, scale).map(OverviewElement::Buffer),
            );
        }
    }
}

fn shadow_layers() -> [glide_theme::elevation::ShadowLayer; 1] {
    let color = glide_theme::Color([0.0, 0.0, 0.0, SHADOW_ALPHA]);
    [glide_theme::elevation::ShadowLayer { dx: 0.0, dy: SHADOW_DY, blur: SHADOW_BLUR, spread: 0.0, color }]
}

fn draw_frame(
    theme: &OverviewTheme, (w, h): (i32, i32), (hover, drop): (u8, u8), output_scale: i32,
) -> Option<MemoryRenderBuffer> {
    let (hover, drop) = (f32::from(hover) / BUCKETS as f32, f32::from(drop) / BUCKETS as f32);
    let (margin, radius) = (margin(), glide_theme::shape::R2);
    let mut c = crate::raster::shadow((w, h), margin, radius, &shadow_layers(), output_scale)?;
    let thumb = crate::raster::rect(margin, margin, w, h);
    if drop > 0.0 {
        c.fill(thumb, theme.highlight.with_alpha(DROP_TINT * drop), radius);
    }
    let pick = theme.strip_pick.with_alpha(theme.strip_pick.0[3] * hover);
    let border = pick.lerp(theme.highlight, drop);
    if border.0[3] > 0.0 {
        c.stroke(thumb, (DROP_BORDER - BORDER).mul_add(drop, BORDER), radius, border);
    }
    Some(c.finish())
}

fn draw_ring(theme: &OverviewTheme, (w, h): (i32, i32), output_scale: i32) -> Option<MemoryRenderBuffer> {
    let mut c = crate::raster::Canvas::new(w, h, output_scale)?;
    let radius = glide_theme::shape::R2 + (RING_GAP as f32) + RING;
    c.stroke(c.full(), RING, radius, theme.strip_ring);
    Some(c.finish())
}

fn draw_add(theme: &OverviewTheme, (w, h): (i32, i32), drop: f64, output_scale: i32) -> Option<MemoryRenderBuffer> {
    let mut c = crate::raster::Canvas::new(w, h, output_scale)?;
    let radius = glide_theme::shape::R2;
    let drop = drop as f32;
    c.fill(c.full(), theme.strip_add.lerp(theme.strip_drop, drop), radius);
    c.stroke(
        c.full(),
        (DROP_BORDER - 1.0).mul_add(drop, 1.0),
        radius,
        theme.strip_add_border.lerp(theme.highlight, drop),
    );
    Some(c.finish())
}

fn draw_plus(theme: &OverviewTheme, output_scale: i32) -> Option<MemoryRenderBuffer> {
    let mut c = crate::raster::Canvas::new(PLUS, PLUS, output_scale)?;
    let middle = (PLUS - PLUS_STROKE) / 2;
    let radius = PLUS_STROKE as f32 / 2.0;
    c.fill(crate::raster::rect(2, middle, PLUS - 4, PLUS_STROKE), theme.strip_caption_active, radius);
    c.fill(crate::raster::rect(middle, 2, PLUS_STROKE, PLUS - 4), theme.strip_caption_active, radius);
    Some(c.finish())
}

fn draw_wallpaper(thumb: Size<i32, Logical>, fit: WallpaperFit, output_scale: i32) -> Option<MemoryRenderBuffer> {
    let (w, h) = (thumb.w * output_scale, thumb.h * output_scale);
    let len = usize::try_from(w * h * 4).ok()?;
    let mut source = vec![0u8; len];
    glide_paint::draw_wallpaper_into(&mut source, w, h, fit);
    let mut canvas = crate::raster::Canvas::new(thumb.w, thumb.h, output_scale)?;
    let radius = glide_theme::shape::R2 * output_scale as f32;
    glide_paint::blit_rounded_backdrop(&mut canvas.buf, w, h, radius, &source, w, h, (0, 0, w, h)).then_some(())?;
    Some(canvas.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area() -> Rectangle<i32, Logical> {
        Rectangle::new(Point::from((0, 34)), Size::from((1440, 866)))
    }

    fn output() -> Rectangle<i32, Logical> {
        Rectangle::new(Point::from((0, 0)), Size::from((1440, 900)))
    }

    #[test]
    fn thumbnails_keep_the_output_shape_and_a_tenth_of_the_work_area() {
        let thumb = thumb_size(area(), output());
        assert_eq!(thumb.h, 87);
        assert_eq!(thumb.w, (87.0_f64 * 1440.0 / 900.0).round() as i32);
        let tall = Rectangle::new(Point::from((0, 0)), Size::from((1080, 1920)));
        assert!(thumb_size(tall, tall).w < thumb_size(tall, tall).h, "a portrait output has portrait thumbnails");
        let tiny = Rectangle::new(Point::from((0, 0)), Size::from((400, 300)));
        assert_eq!(thumb_size(tiny, tiny).h, MIN_HEIGHT);
        assert!(band(area(), output()) > thumb.h + TOP_GAP);
    }

    #[test]
    fn one_workspace_still_gets_a_thumbnail_and_the_add_tile_follows_it() {
        let mut strip = Strip::default();
        strip.set(vec![WorkspaceId(1)], WorkspaceId(1), true);
        let slots = strip.slots(area(), output());
        assert_eq!(slots.len(), 2);
        let (thumb, add) = (slots[0].1, slots[1].1);
        assert_eq!(slots[1].0, Slot::Add);
        assert!((add.loc.x - (thumb.loc.x + thumb.size.w + f64::from(GAP))).abs() < 1e-9);
        assert!((add.size.w - f64::from(ADD_WIDTH)).abs() < 1e-9);
        let total = add.loc.x + add.size.w - thumb.loc.x;
        assert!((thumb.loc.x - (1440.0 - total) / 2.0).abs() < 1e-9, "the row is centred");
        let centre = Point::from((add.loc.x + 4.0, add.loc.y + 4.0));
        assert_eq!(strip.slot_at(area(), output(), centre), Some(Slot::Add));
    }

    #[test]
    fn fixed_workspaces_offer_no_add_tile() {
        let mut strip = Strip::default();
        strip.set(vec![WorkspaceId(1), WorkspaceId(2)], WorkspaceId(2), false);
        let slots = strip.slots(area(), output());
        assert!(slots.iter().all(|(slot, _)| *slot != Slot::Add));
        let centre = |rect: Rectangle<f64, Logical>| {
            Point::from((rect.loc.x + rect.size.w / 2.0, rect.loc.y + rect.size.h / 2.0))
        };
        assert_eq!(strip.slot_at(area(), output(), centre(slots[1].1)), Some(Slot::Workspace(WorkspaceId(2))));
        assert_eq!(strip.slot_at(area(), output(), Point::from((4.0, 40.0))), None);
    }

    #[test]
    fn the_add_tile_opens_to_a_full_thumbnail_while_a_window_is_carried() {
        let mut strip = Strip::default();
        strip.set(vec![WorkspaceId(1)], WorkspaceId(1), true);
        strip.set_dragging(true);
        if glide_theme::motion::is_disabled() {
            let add = strip.slots(area(), output())[1].1;
            assert_eq!(add.size.w.round() as i32, thumb_size(area(), output()).w);
        }
        assert!(glide_theme::motion::is_disabled() || strip.is_animating());
    }

    #[test]
    fn a_hover_or_drop_on_a_vanished_workspace_is_dropped_on_the_next_set() {
        let mut strip = Strip::default();
        strip.set(vec![WorkspaceId(1), WorkspaceId(2)], WorkspaceId(1), true);
        strip.set_hover(Some(Slot::Workspace(WorkspaceId(2))));
        strip.set_drop(Some(Slot::Workspace(WorkspaceId(2))));
        strip.set(vec![WorkspaceId(1), WorkspaceId(3)], WorkspaceId(1), true);
        assert_eq!(strip.hover.target(), None);
        assert_eq!(strip.drop.target(), None);
    }
}

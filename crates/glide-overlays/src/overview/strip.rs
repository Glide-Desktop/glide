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
const ADD_STEP: i32 = 4;
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
    places: HashMap<WorkspaceId, glide_ui::Tween>,
    reorder: Option<Reorder>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Reorder {
    id: WorkspaceId,
    start: f64,
    grab: f64,
    pointer: f64,
    lifted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Reordered {
    Click(WorkspaceId),
    Moved { id: WorkspaceId, index: usize },
    Settled,
}

fn slide() -> std::time::Duration {
    glide_theme::motion::t3()
}

fn shuffle() -> std::time::Duration {
    glide_theme::motion::t2()
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

pub(super) fn margin() -> i32 {
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
        let same_active = self.active == Some(active);
        self.workspaces = workspaces;
        self.active = Some(active);
        self.can_add = can_add;
        if self.reorder.is_some_and(|reorder| !self.workspaces.contains(&reorder.id)) {
            self.reorder = None;
        }
        self.retarget_places();
        let index = self.index_of(active).unwrap_or(0) as f64;
        match self.ring.as_mut() {
            Some(ring) if same_active => ring.jump(index),
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

    fn row(&self, area: Rectangle<i32, Logical>, output: Rectangle<i32, Logical>) -> (f64, f64) {
        let thumb = thumb_size(area, output).to_f64();
        let add = f64::from(ADD_WIDTH) + (thumb.w - f64::from(ADD_WIDTH)) * self.add_open();
        let count = self.workspaces.len() as f64;
        let gaps = (count - 1.0).max(0.0) * f64::from(GAP);
        let total = count * thumb.w + gaps + if self.can_add { f64::from(GAP) + add } else { 0.0 };
        (f64::from(area.loc.x) + (f64::from(area.size.w) - total) / 2.0, thumb.w + f64::from(GAP))
    }

    fn reorder_target(&self, reorder: &Reorder, (left, step): (f64, f64)) -> usize {
        let last = self.workspaces.len().saturating_sub(1) as f64;
        ((reorder.pointer - reorder.grab - left) / step).round().clamp(0.0, last) as usize
    }

    fn visual_order(&self, row: (f64, f64)) -> Vec<WorkspaceId> {
        let mut order = self.workspaces.clone();
        if let Some(reorder) = self.reorder.filter(|reorder| reorder.lifted) {
            order.retain(|id| *id != reorder.id);
            order.insert(self.reorder_target(&reorder, row).min(order.len()), reorder.id);
        }
        order
    }

    fn retarget_places_in(&mut self, row: (f64, f64)) {
        let order = self.visual_order(row);
        self.places.retain(|id, _| order.contains(id));
        for (index, id) in order.into_iter().enumerate() {
            let index = index as f64;
            match self.places.get_mut(&id) {
                Some(place) => {
                    place.retarget(index, shuffle(), glide_theme::motion::ease_out);
                }
                None => {
                    self.places.insert(id, glide_ui::Tween::at(index));
                }
            }
        }
    }

    fn retarget_places(&mut self) {
        let row = (0.0, 1.0);
        let lifted = self.reorder.is_some_and(|reorder| reorder.lifted);
        if !lifted {
            self.retarget_places_in(row);
        }
    }

    pub(super) fn press_reorder(
        &mut self, id: WorkspaceId, x: f64, area: Rectangle<i32, Logical>, output: Rectangle<i32, Logical>,
    ) {
        let slot_x = self
            .slots(area, output)
            .into_iter()
            .find(|(slot, _)| *slot == Slot::Workspace(id))
            .map_or(x, |(_, rect)| rect.loc.x);
        self.reorder = Some(Reorder { id, start: x, grab: x - slot_x, pointer: x, lifted: false });
    }

    #[must_use]
    pub(super) fn reorder_pressed(&self) -> bool {
        self.reorder.is_some()
    }

    #[must_use]
    pub(super) fn lifted(&self) -> Option<WorkspaceId> {
        self.reorder.filter(|reorder| reorder.lifted).map(|reorder| reorder.id)
    }

    pub(super) fn reorder_motion(
        &mut self, x: f64, area: Rectangle<i32, Logical>, output: Rectangle<i32, Logical>,
    ) -> bool {
        let Some(reorder) = self.reorder.as_mut() else {
            return false;
        };
        reorder.pointer = x;
        if !reorder.lifted && (x - reorder.start).abs() < super::drag::SLOP {
            return false;
        }
        reorder.lifted = true;
        let row = self.row(area, output);
        self.retarget_places_in(row);
        true
    }

    pub(super) fn release_reorder(
        &mut self, area: Rectangle<i32, Logical>, output: Rectangle<i32, Logical>,
    ) -> Option<Reordered> {
        let reorder = self.reorder?;
        let row = self.row(area, output);
        let target = self.reorder_target(&reorder, row);
        self.reorder = None;
        if !reorder.lifted {
            return Some(Reordered::Click(reorder.id));
        }
        let (left, step) = row;
        let last = self.workspaces.len().saturating_sub(1) as f64;
        let dropped = ((reorder.pointer - reorder.grab - left) / step).clamp(0.0, last);
        let mut place = glide_ui::Tween::at(dropped);
        place.retarget(target as f64, shuffle(), glide_theme::motion::ease_out);
        self.places.insert(reorder.id, place);
        if self.index_of(reorder.id) == Some(target) {
            self.retarget_places_in(row);
            return Some(Reordered::Settled);
        }
        Some(Reordered::Moved { id: reorder.id, index: target })
    }

    pub(super) fn cancel_reorder(&mut self) -> bool {
        let was = self.reorder.take().is_some();
        self.retarget_places();
        was
    }

    #[must_use]
    pub(super) fn paint_order(&self) -> Vec<WorkspaceId> {
        let lifted = self.lifted();
        let mut order: Vec<WorkspaceId> = lifted.into_iter().collect();
        order.extend(self.workspaces.iter().copied().filter(|id| Some(*id) != lifted));
        order
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

    #[cfg(test)]
    pub(super) fn dropping(&self) -> Option<Slot> {
        self.drop.target()
    }

    #[cfg(test)]
    pub(super) fn open_add(&mut self, level: f64) {
        self.add_open.jump(level);
    }

    pub(super) fn is_animating(&self) -> bool {
        self.hover.is_animating(hover_anim())
            || self.drop.is_animating(hover_anim())
            || self.ring.as_ref().is_some_and(|ring| ring.is_animating(slide()))
            || self.add_open.is_animating(slide())
            || self.places.values().any(|place| place.is_animating(shuffle()))
    }

    fn add_open(&self) -> f64 {
        self.add_open.value(slide(), glide_theme::motion::ease_out)
    }

    pub(super) fn slots(
        &self, area: Rectangle<i32, Logical>, output: Rectangle<i32, Logical>,
    ) -> Vec<(Slot, Rectangle<f64, Logical>)> {
        let thumb = thumb_size(area, output).to_f64();
        let add = f64::from(ADD_WIDTH) + (thumb.w - f64::from(ADD_WIDTH)) * self.add_open();
        let (left, step) = self.row(area, output);
        let y = f64::from(area.loc.y + TOP_GAP);
        let last = left + self.workspaces.len().saturating_sub(1) as f64 * step;
        let lifted = self.reorder.filter(|reorder| reorder.lifted);
        let mut slots = Vec::with_capacity(self.workspaces.len() + 1);
        for id in self.paint_order() {
            let index = self.index_of(id).unwrap_or(0) as f64;
            let place =
                self.places.get(&id).map_or(index, |place| place.value(shuffle(), glide_theme::motion::ease_out));
            let x = match lifted {
                Some(reorder) if reorder.id == id => (reorder.pointer - reorder.grab).clamp(left, last.max(left)),
                _ => place.mul_add(step, left),
            };
            slots.push((Slot::Workspace(id), Rectangle::new(Point::from((x, y)), thumb)));
        }
        if self.can_add {
            let x = self.workspaces.len() as f64 * step + left;
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

    fn render_ring<R: ShellRenderer>(
        &mut self, renderer: &mut R, theme: &OverviewTheme,
        (active, thumb, key): (Rectangle<f64, Logical>, Size<i32, Logical>, RasterKey), (alpha, rise): (f32, f64),
        scale: Scale<f64>, elements: &mut Vec<OverviewElement<R>>,
    ) {
        let Some(ring) = self.ring.as_ref() else {
            return;
        };
        let output_scale = key.output_scale;
        let behind = ring.value(slide(), glide_theme::motion::ease_out) - ring.target();
        let outset = f64::from(RING_GAP) + f64::from(RING);
        let x = behind.mul_add(f64::from(thumb.w + GAP), active.loc.x) - outset;
        let size = (thumb.w + 2 * (RING_GAP + RING as i32), thumb.h + 2 * (RING_GAP + RING as i32));
        let outdated = self.ring_buffer.get(&output_scale).is_none_or(|(cached, at, _)| *cached != key || *at != size);
        if outdated && let Some(memory) = draw_ring(theme, size, output_scale).map(|c| c.finish()) {
            self.ring_buffer.insert(output_scale, (key, size, memory));
        }
        if let Some((_, size, memory)) = self.ring_buffer.get(&output_scale) {
            let rect = Rectangle::new(Point::from((x, active.loc.y - outset - rise)), Size::from(*size).to_f64());
            elements.extend(place_in(renderer, rect, *size, memory, alpha, scale).map(OverviewElement::Buffer));
        }
    }

    pub(super) fn render_chrome<R: ShellRenderer>(
        &mut self, renderer: &mut R, (theme, painter): (&OverviewTheme, &crate::paint::Painter),
        (area, output): (Rectangle<i32, Logical>, Rectangle<i32, Logical>), (alpha, rise): (f32, f64),
        (scale, output_scale): (Scale<f64>, i32),
    ) -> Chrome<R> {
        let mut chrome = Chrome { ring: Vec::new(), slots: Vec::new() };
        if alpha <= 0.0 {
            return chrome;
        }
        let key = RasterKey::new(theme, output_scale);
        let thumb = thumb_size(area, output);
        let slots = self.slots(area, output);

        let active_slot = self.active.map(Slot::Workspace);
        let active_rect = slots.iter().find(|(slot, _)| Some(*slot) == active_slot).map(|(_, rect)| *rect);
        if let Some(active) = active_rect {
            self.render_ring(renderer, theme, (active, thumb, key), (alpha, rise), scale, &mut chrome.ring);
        }

        let mut captions = Vec::new();
        for (position, (slot, base)) in slots.iter().enumerate() {
            let drawn = self.drawn(*slot, *base, rise);
            let hover = self.hover.level(*slot, hover_anim());
            let drop = self.drop.level(*slot, hover_anim());
            let mut pieces = Vec::new();
            match slot {
                Slot::Workspace(id) => {
                    let frame = self.render_frame(renderer, theme, (drawn, thumb, key), (hover, drop), (alpha, scale));
                    pieces.extend(frame);
                    let active = self.active == Some(*id);
                    let index = self.index_of(*id).unwrap_or(0);
                    let text =
                        crate::i18n::t(crate::i18n::Key::OverviewDesktop).replace("{}", &(index + 1).to_string());
                    captions.push(Caption { slot: position, index, text, active, base: *base, share: 1.0 });
                }
                Slot::Add => {
                    let open = self.add_open();
                    self.render_add(
                        renderer,
                        theme,
                        (drawn, drop, thumb.w),
                        (alpha, key),
                        (scale, output_scale),
                        &mut pieces,
                    );
                    if open > 0.0 {
                        let text = crate::i18n::t(crate::i18n::Key::OverviewNewDesktop).to_string();
                        let share = glide_theme::motion::assist(open);
                        captions.push(Caption {
                            slot: position,
                            index: usize::MAX,
                            text,
                            active: false,
                            base: *base,
                            share,
                        });
                    }
                }
            }
            chrome.slots.push((*slot, pieces));
        }

        for caption in &captions {
            let element =
                self.render_caption(renderer, (theme, painter), (key, alpha, rise), (scale, output_scale), caption);
            chrome.slots[caption.slot].1.extend(element);
        }
        chrome
    }

    fn render_frame<R: ShellRenderer>(
        &mut self, renderer: &mut R, theme: &OverviewTheme,
        (drawn, thumb, key): (Rectangle<f64, Logical>, Size<i32, Logical>, RasterKey), (hover, drop): (f64, f64),
        (alpha, scale): (f32, Scale<f64>),
    ) -> Option<OverviewElement<R>> {
        let margin = margin();
        let buckets = (bucket(hover), bucket(drop));
        let size = (thumb.w, thumb.h);
        let frames = self.frames.entry(key.output_scale).or_default();
        let outdated = frames.get(&buckets).is_none_or(|(cached, at, _)| *cached != key || *at != size);
        if outdated && let Some(memory) = draw_frame(theme, size, buckets, key.output_scale).map(|c| c.finish()) {
            frames.insert(buckets, (key, size, memory));
        }
        let (_, _, memory) = frames.get(&buckets)?;
        let factor = drawn.size.w / f64::from(thumb.w);
        let outer = Rectangle::new(
            Point::from((drawn.loc.x - f64::from(margin) * factor, drawn.loc.y - f64::from(margin) * factor)),
            Size::from((f64::from(thumb.w + 2 * margin) * factor, f64::from(thumb.h + 2 * margin) * factor)),
        );
        let logical = (thumb.w + 2 * margin, thumb.h + 2 * margin);
        place_in(renderer, outer, logical, memory, alpha, scale).map(OverviewElement::Buffer)
    }

    fn render_caption<R: ShellRenderer>(
        &mut self, renderer: &mut R, (theme, painter): (&OverviewTheme, &crate::paint::Painter),
        (key, alpha, rise): (RasterKey, f32, f64), (scale, output_scale): (Scale<f64>, i32), caption: &Caption,
    ) -> Option<OverviewElement<R>> {
        let width = painter.text_width_styled(&caption.text, &glide_theme::TextStyle::CAP).ceil() as i32 + 2;
        let wanted = CaptionKey { text: caption.text.clone(), active: caption.active, raster: key };
        let cache = self.captions.entry(output_scale).or_default();
        let outdated = cache.get(&caption.index).is_none_or(|(cached, _, _)| *cached != wanted);
        if outdated {
            let colour = if caption.active { theme.strip_caption_active } else { theme.strip_caption };
            let style = if caption.active {
                glide_theme::TextStyle { weight: 600, ..glide_theme::TextStyle::CAP }
            } else {
                glide_theme::TextStyle::CAP
            };
            if let Some(mut canvas) = crate::raster::Canvas::new(width, CAPTION_HEIGHT, output_scale) {
                canvas.label(painter, (0, width), crate::paint::TextAlign::Center, &wanted.text, colour, &style);
                cache.insert(caption.index, (wanted, width, canvas.finish()));
            }
        }
        let (_, width, memory) = cache.get(&caption.index)?;
        let base = caption.base;
        let rect = Rectangle::new(
            Point::from((
                base.loc.x + (base.size.w - f64::from(*width)) / 2.0,
                base.loc.y + base.size.h + f64::from(CAPTION_GAP) - rise,
            )),
            Size::from((f64::from(*width), f64::from(CAPTION_HEIGHT))),
        );
        let shown = alpha * caption.share as f32;
        place_in(renderer, rect, (*width, CAPTION_HEIGHT), memory, shown, scale).map(OverviewElement::Buffer)
    }

    fn render_add<R: ShellRenderer>(
        &mut self, renderer: &mut R, theme: &OverviewTheme, (drawn, drop, full): (Rectangle<f64, Logical>, f64, i32),
        (alpha, key): (f32, RasterKey), (scale, output_scale): (Scale<f64>, i32),
        elements: &mut Vec<OverviewElement<R>>,
    ) {
        let plus_stale = self.plus.get(&output_scale).is_none_or(|(cached, _)| *cached != key);
        if plus_stale && let Some(memory) = draw_plus(theme, output_scale).map(|c| c.finish()) {
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
        let size = (add_raster_width(drawn.size.w, full), drawn.size.h.round() as i32);
        let tiles = self.add_tile.entry(output_scale).or_default();
        let outdated = tiles.get(&level).is_none_or(|(cached, at, _)| *cached != key || *at != size);
        let painted = outdated.then(|| draw_add(theme, size, f64::from(level) / BUCKETS, output_scale)).flatten();
        if let Some(memory) = painted.map(|c| c.finish()) {
            tiles.insert(level, (key, size, memory));
        }
        if let Some((_, size, memory)) = tiles.get(&level) {
            elements.extend(place_in(renderer, drawn, *size, memory, alpha, scale).map(OverviewElement::Buffer));
        }
    }

    pub(super) fn render_wallpapers<R: ShellRenderer>(
        &mut self, renderer: &mut R, (area, output): (Rectangle<i32, Logical>, Rectangle<i32, Logical>),
        (alpha, rise, fit): (f32, f64, WallpaperFit), (scale, output_scale): (Scale<f64>, i32),
    ) -> Vec<(WorkspaceId, OverviewElement<R>)> {
        let mut walls = Vec::new();
        if alpha <= 0.0 {
            return walls;
        }
        let thumb = thumb_size(area, output);
        let wall = WallKey { size: (thumb.w, thumb.h), generation: glide_paint::wallpaper_generation(), fit };
        if self.wallpaper.get(&output_scale).is_none_or(|(cached, _)| *cached != wall)
            && let Some(memory) = draw_wallpaper(thumb, fit, output_scale).map(|c| c.finish())
        {
            self.wallpaper.insert(output_scale, (wall, memory));
        }
        let Some((_, memory)) = self.wallpaper.get(&output_scale) else {
            return walls;
        };
        for (slot, base) in self.slots(area, output) {
            let Slot::Workspace(id) = slot else {
                continue;
            };
            let drawn = self.drawn(slot, base, rise);
            let element = place_in(renderer, drawn, (thumb.w, thumb.h), memory, alpha, scale);
            walls.extend(element.map(|element| (id, OverviewElement::Buffer(element))));
        }
        walls
    }
}

#[derive(Debug)]
struct Caption {
    slot: usize,
    index: usize,
    text: String,
    active: bool,
    base: Rectangle<f64, Logical>,
    share: f64,
}

pub(super) struct Chrome<R: ShellRenderer> {
    pub(super) ring: Vec<OverviewElement<R>>,
    pub(super) slots: Vec<(Slot, Vec<OverviewElement<R>>)>,
}

fn add_raster_width(drawn: f64, full: i32) -> i32 {
    let span = full - ADD_WIDTH;
    if span == 0 {
        return ADD_WIDTH;
    }
    let open = ((drawn - f64::from(ADD_WIDTH)) / f64::from(span)).clamp(0.0, 1.0);
    let steps = f64::from((span.abs() + ADD_STEP - 1) / ADD_STEP);
    ADD_WIDTH + (f64::from(span) * (open * steps).round() / steps).round() as i32
}

fn shadow_layers() -> [glide_theme::elevation::ShadowLayer; 1] {
    let color = glide_theme::Color([0.0, 0.0, 0.0, SHADOW_ALPHA]);
    [glide_theme::elevation::ShadowLayer { dx: 0.0, dy: SHADOW_DY, blur: SHADOW_BLUR, spread: 0.0, color }]
}

fn draw_frame(theme: &OverviewTheme, (w, h): (i32, i32), (hover, drop): (u8, u8), output_scale: i32) -> Option<Canvas> {
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
    Some(c)
}

fn draw_ring(theme: &OverviewTheme, (w, h): (i32, i32), output_scale: i32) -> Option<Canvas> {
    let mut c = crate::raster::Canvas::new(w, h, output_scale)?;
    let radius = glide_theme::shape::R2 + (RING_GAP as f32) + RING;
    c.stroke(c.full(), RING, radius, theme.strip_ring);
    Some(c)
}

fn draw_add(theme: &OverviewTheme, (w, h): (i32, i32), drop: f64, output_scale: i32) -> Option<Canvas> {
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
    Some(c)
}

fn draw_plus(theme: &OverviewTheme, output_scale: i32) -> Option<Canvas> {
    let mut c = crate::raster::Canvas::new(PLUS, PLUS, output_scale)?;
    let middle = (PLUS - PLUS_STROKE) / 2;
    let radius = PLUS_STROKE as f32 / 2.0;
    c.fill(crate::raster::rect(2, middle, PLUS - 4, PLUS_STROKE), theme.strip_caption_active, radius);
    c.fill(crate::raster::rect(middle, 2, PLUS_STROKE, PLUS - 4), theme.strip_caption_active, radius);
    Some(c)
}

fn draw_wallpaper(thumb: Size<i32, Logical>, fit: WallpaperFit, output_scale: i32) -> Option<Canvas> {
    let (w, h) = (thumb.w * output_scale, thumb.h * output_scale);
    let len = usize::try_from(w * h * 4).ok()?;
    let mut source = vec![0u8; len];
    glide_paint::draw_wallpaper_into(&mut source, w, h, fit);
    let mut canvas = crate::raster::Canvas::new(thumb.w, thumb.h, output_scale)?;
    let radius = glide_theme::shape::R2 * output_scale as f32;
    glide_paint::blit_rounded_backdrop(&mut canvas.buf, w, h, radius, &source, w, h, (0, 0, w, h)).then_some(())?;
    Some(canvas)
}

type Canvas = crate::raster::Canvas<'static>;

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
    #[ignore = "writes PNGs to $GLIDE_OVERVIEW_PREVIEW for a visual check"]
    fn preview() {
        use glide_shell::image::Image;
        let Some(dir) = std::env::var_os("GLIDE_OVERVIEW_PREVIEW") else {
            return;
        };
        let scale = 2;
        let painter = glide_paint::Painter::new(14.0, None);
        for appearance in [glide_theme::Appearance::Dark, glide_theme::Appearance::Light] {
            let tokens = glide_theme::Tokens::new(appearance, glide_theme::Accent::Blue);
            let theme = OverviewTheme::from_tokens(&tokens);
            let thumb = thumb_size(area(), output());
            let (w, h) = (thumb.w, thumb.h);
            let (width, height) = (((w + GAP) * 5 + 120) * scale, (h * 2 + 260) * scale);
            let mut image = Image::new(width as u32, height as u32).expect("image");
            let shade = if appearance == glide_theme::Appearance::Dark { 24 } else { 226 };
            let backdrop: Vec<u8> =
                std::iter::repeat_n([shade, shade, shade, 255], (width * height) as usize).flatten().collect();
            image.draw(&Image::from_bgra(width as u32, height as u32, backdrop).expect("backdrop"), 0, 0);
            let put = |image: &mut Image, canvas: &Canvas, x: i32, y: i32| {
                let (cw, ch) = ((canvas.w * scale) as u32, (canvas.h * scale) as u32);
                image.draw(&Image::from_bgra(cw, ch, canvas.buf.clone()).expect("layer"), x * scale, y * scale);
            };
            let margin = margin();
            let wall = draw_wallpaper(thumb, WallpaperFit::default(), scale).expect("wallpaper");
            let states = [(0, 0), (BUCKETS as u8, 0), (0, BUCKETS as u8)];
            for (index, buckets) in states.into_iter().enumerate() {
                let x = 40 + i32::try_from(index).unwrap_or(0) * (w + GAP);
                let y = 40;
                put(&mut image, &wall, x, y);
                let mut window = crate::raster::Canvas::new(w / 2, h / 2, scale).expect("window");
                window.fill(window.full(), tokens.win_bg, MINI_RADIUS_PREVIEW);
                window.fill(crate::raster::rect(0, 0, w / 2, 4), tokens.bar_bg, MINI_RADIUS_PREVIEW);
                put(&mut image, &window, x + w / 5, y + h / 5);
                let frame = draw_frame(&theme, (w, h), buckets, scale).expect("frame");
                put(&mut image, &frame, x - margin, y - margin);
                let caption = format!("Стіл {}", index + 1);
                let mut label = crate::raster::Canvas::new(w, CAPTION_HEIGHT, scale).expect("caption");
                let colour = if index == 0 { theme.strip_caption_active } else { theme.strip_caption };
                label.label(
                    &painter,
                    (0, w),
                    crate::paint::TextAlign::Center,
                    &caption,
                    colour,
                    &glide_theme::TextStyle::CAP,
                );
                put(&mut image, &label, x, y + h + CAPTION_GAP);
            }
            let outset = RING_GAP + RING as i32;
            let ring = draw_ring(&theme, (w + 2 * outset, h + 2 * outset), scale).expect("ring");
            put(&mut image, &ring, 40 - outset, 40 - outset);
            let add_x = 40 + 3 * (w + GAP);
            let add = draw_add(&theme, (ADD_WIDTH, h), 0.0, scale).expect("add");
            put(&mut image, &add, add_x, 40);
            let plus = draw_plus(&theme, scale).expect("plus");
            put(&mut image, &plus, add_x + (ADD_WIDTH - PLUS) / 2, 40 + (h - PLUS) / 2);
            let open = draw_add(&theme, (w, h), 1.0, scale).expect("open");
            put(&mut image, &open, add_x + ADD_WIDTH + GAP, 40);
            put(&mut image, &plus, add_x + ADD_WIDTH + GAP + (w - PLUS) / 2, 40 + (h - PLUS) / 2);
            let card = super::super::draw_page_card(&theme, (w * 3, 150), scale).expect("card");
            put(&mut image, &card, 40, h + 110);
            let file = format!("overview-strip-{appearance:?}.png").to_lowercase();
            std::fs::write(std::path::Path::new(&dir).join(file), image.png().expect("png")).expect("write");
        }
    }

    const MINI_RADIUS_PREVIEW: f32 = 2.0;

    fn three() -> Strip {
        let mut strip = Strip::default();
        strip.set(vec![WorkspaceId(1), WorkspaceId(2), WorkspaceId(3)], WorkspaceId(1), true);
        strip
    }

    fn x_of(strip: &Strip, id: WorkspaceId) -> f64 {
        strip.slots(area(), output()).into_iter().find(|(slot, _)| *slot == Slot::Workspace(id)).unwrap().1.loc.x
    }

    #[test]
    fn a_thumbnail_pressed_and_let_go_in_place_is_a_click() {
        let mut strip = three();
        let x = x_of(&strip, WorkspaceId(2)) + 10.0;
        strip.press_reorder(WorkspaceId(2), x, area(), output());
        assert!(!strip.reorder_motion(x + 3.0, area(), output()));
        assert_eq!(strip.release_reorder(area(), output()), Some(Reordered::Click(WorkspaceId(2))));
        assert!(!strip.reorder_pressed());
    }

    #[test]
    fn a_dragged_thumbnail_follows_the_pointer_and_lands_where_it_is_let_go() {
        let mut strip = three();
        let step = f64::from(thumb_size(area(), output()).w + GAP);
        let first = x_of(&strip, WorkspaceId(1));
        strip.press_reorder(WorkspaceId(1), first + 10.0, area(), output());
        assert!(strip.reorder_motion(first + 10.0 + step * 1.6, area(), output()));
        assert_eq!(strip.lifted(), Some(WorkspaceId(1)));
        assert!((x_of(&strip, WorkspaceId(1)) - (first + step * 1.6)).abs() < 1e-6, "it sits under the pointer");
        assert_eq!(strip.paint_order()[0], WorkspaceId(1), "the carried thumbnail draws on top");
        assert_eq!(strip.release_reorder(area(), output()), Some(Reordered::Moved { id: WorkspaceId(1), index: 2 }));
    }

    #[test]
    fn a_thumbnail_dropped_back_on_its_own_place_settles() {
        let mut strip = three();
        let x = x_of(&strip, WorkspaceId(3)) + 10.0;
        strip.press_reorder(WorkspaceId(3), x, area(), output());
        strip.reorder_motion(x + 20.0, area(), output());
        assert_eq!(strip.release_reorder(area(), output()), Some(Reordered::Settled));
        strip.press_reorder(WorkspaceId(3), x, area(), output());
        assert!(strip.cancel_reorder());
        assert!(!strip.reorder_pressed());
    }

    #[test]
    fn the_add_tile_raster_follows_the_width_it_is_drawn_at_and_is_exact_at_rest() {
        let full = 139;
        assert_eq!(add_raster_width(f64::from(ADD_WIDTH), full), ADD_WIDTH);
        assert_eq!(add_raster_width(f64::from(full), full), full);
        let drawn = |step: i32| f64::from(ADD_WIDTH) + f64::from(full - ADD_WIDTH) * f64::from(step) / 40.0;
        let widths: Vec<i32> = (0..=40).map(|step| add_raster_width(drawn(step), full)).collect();
        assert!(widths.windows(2).all(|pair| pair[0] <= pair[1]), "it only grows with the tile: {widths:?}");
        let mut distinct = widths.clone();
        distinct.dedup();
        let most = ((full - ADD_WIDTH) / ADD_STEP) as usize + 2;
        assert!(distinct.len() <= most, "rerasters stay bounded: {distinct:?}");
        assert!(distinct.len() < widths.len(), "nearby widths share a raster: {distinct:?}");
        let tolerance = f64::from(ADD_STEP) / 2.0 + 1.0;
        for (step, width) in (0..=40).zip(&widths) {
            assert!((f64::from(*width) - drawn(step)).abs() <= tolerance, "step {step}: raster {width}");
        }
    }

    #[test]
    fn a_thumbnail_narrower_than_the_add_tile_gives_a_raster_that_shrinks_with_it() {
        let full = 40;
        assert_eq!(add_raster_width(f64::from(ADD_WIDTH), full), ADD_WIDTH);
        assert_eq!(add_raster_width(f64::from(full), full), full);
        assert!(add_raster_width(48.0, full) < ADD_WIDTH);
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

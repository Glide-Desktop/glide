use std::time::{Duration, Instant};

use glide_layout::{WindowId, WorkspaceId, compute_overview_layout};
use glide_theme::{OverviewTheme, WallpaperFit};
use smithay::{
    backend::renderer::{
        ImportAll, ImportMem, Renderer,
        element::{
            Kind,
            memory::{MemoryRenderBuffer, MemoryRenderBufferRenderElement},
            solid::{SolidColorBuffer, SolidColorRenderElement},
            utils::RescaleRenderElement,
        },
    },
    utils::{Logical, Physical, Point, Rectangle, Scale, Size},
};

use crate::raster::ShellRenderer;

pub mod close;
pub mod drag;
#[cfg(test)]
mod probe;
mod strip;

pub use drag::{Side, Target};
pub use strip::Slot;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TileInfo {
    pub title: String,
    pub app: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    pub id: WindowId,
    pub frame: Rectangle<i32, Logical>,
    pub workspace: WorkspaceId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Place {
    pub output: Rectangle<i32, Logical>,
    pub area: Rectangle<i32, Logical>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindowDraw {
    pub id: WindowId,
    pub frame: Rectangle<i32, Logical>,
    pub rect: Rectangle<f64, Logical>,
    pub alpha: f32,
    pub radius: f32,
}

pub enum Layer<R: ShellRenderer> {
    Chrome(Box<OverviewElement<R>>),
    Window(WindowDraw),
}

impl<R: ShellRenderer> Layer<R> {
    fn chrome(element: OverviewElement<R>) -> Self {
        Self::Chrome(Box::new(element))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveTo {
    Workspace(WorkspaceId),
    New,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Release {
    Click(WindowId),
    Move { id: WindowId, to: MoveTo },
    SwitchTo(WorkspaceId),
    Reorder { workspace: WorkspaceId, index: usize },
    Settled,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Phase {
    Opening { started: Instant, from: SceneFrom },
    Open,
    Closing { started: Instant, select: Option<WindowId>, from: SceneFrom },
    Interactive { progress: f64 },
}

#[cfg(test)]
impl Phase {
    #[must_use]
    pub const fn opening(started: Instant) -> Self {
        Self::Opening { started, from: SceneFrom::ZERO }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneFrom {
    pub primary: f64,
    pub backdrop: f64,
    pub release: Option<glide_theme::motion::Release>,
}

impl SceneFrom {
    const ZERO: Self = Self { primary: 0.0, backdrop: 0.0, release: None };

    fn ease(self, curve: glide_theme::motion::CubicBezier, fraction: f64) -> f64 {
        self.release.map_or_else(|| curve.ease_f64(fraction), |release| release.at(fraction))
    }

    const fn pick(self, backdrop: bool) -> f64 {
        if backdrop { self.backdrop } else { self.primary }
    }
}

fn hover_anim() -> Duration {
    glide_theme::motion::t2()
}

fn slide() -> Duration {
    glide_theme::motion::t3()
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    (b - a).mul_add(t, a)
}

fn lerp_rect(from: Rectangle<f64, Logical>, to: Rectangle<f64, Logical>, t: f64) -> Rectangle<f64, Logical> {
    Rectangle::new(
        Point::from((lerp(from.loc.x, to.loc.x, t), lerp(from.loc.y, to.loc.y, t))),
        Size::from((lerp(from.size.w, to.size.w, t).max(1.0), lerp(from.size.h, to.size.h, t).max(1.0))),
    )
}

fn round_rect(rect: Rectangle<f64, Logical>) -> Rectangle<i32, Logical> {
    Rectangle::new(
        Point::from((rect.loc.x.round() as i32, rect.loc.y.round() as i32)),
        Size::from((rect.size.w.round().max(1.0) as i32, rect.size.h.round().max(1.0) as i32)),
    )
}

fn moved_by(rect: Rectangle<f64, Logical>, dx: f64) -> Rectangle<f64, Logical> {
    Rectangle::new(Point::from((rect.loc.x + dx, rect.loc.y)), rect.size)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RasterKey {
    output_scale: i32,
    theme_rev: u64,
    painter_gen: u64,
}

impl RasterKey {
    fn new(theme: &OverviewTheme, output_scale: i32) -> Self {
        Self { output_scale, theme_rev: theme.revision(), painter_gen: crate::paint::painter_generation() }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LabelKey {
    title: String,
    app: Option<String>,
    width: i32,
    raster: RasterKey,
    icons: u64,
}

type CachedLabel = (LabelKey, MemoryRenderBuffer);
type CachedHighlight = (i32, i32, RasterKey, MemoryRenderBuffer);
type CachedClose = (u8, RasterKey, MemoryRenderBuffer);
type PerScale<T> = std::collections::HashMap<i32, T>;

const PEEK: i32 = 48;
const EDGE_ZONE: f64 = 64.0;
const NEIGHBOUR_ALPHA: f32 = 0.5;
const RUBBER_BAND: f64 = 0.25;
const MINI_RADIUS: f32 = 2.0;

#[derive(Debug, Clone)]
struct Preview {
    id: WindowId,
    frame: Rectangle<i32, Logical>,
    workspace: WorkspaceId,
    target: Rectangle<i32, Logical>,
    row: usize,
    stack: usize,
    glide: Option<(Rectangle<i32, Logical>, glide_ui::Tween)>,
    label: PerScale<CachedLabel>,
    highlight: PerScale<CachedHighlight>,
    close: PerScale<CachedClose>,
}

impl Preview {
    fn target_now(&self) -> Rectangle<f64, Logical> {
        match &self.glide {
            Some((from, tween)) => {
                lerp_rect(from.to_f64(), self.target.to_f64(), tween.value(slide(), glide_theme::motion::ease_out))
            }
            None => self.target.to_f64(),
        }
    }

    fn is_gliding(&self) -> bool {
        self.glide.as_ref().is_some_and(|(_, tween)| tween.is_animating(slide()))
    }
}

#[derive(Debug, Clone, Copy)]
struct Landing {
    id: WindowId,
    from: Rectangle<f64, Logical>,
    started: Instant,
}

impl Landing {
    fn progress(&self) -> f64 {
        let duration = slide();
        if duration.is_zero() {
            return 1.0;
        }
        (glide_ui::clock::since(self.started).as_secs_f64() / duration.as_secs_f64()).clamp(0.0, 1.0)
    }
}

#[must_use]
pub fn options(theme: &OverviewTheme) -> glide_layout::OverviewOptions {
    glide_layout::OverviewOptions {
        gap: theme.gap,
        padding: theme.padding,
        label_height: if theme.label { theme.label_height } else { 0 },
        label_offset: if theme.label { label_offset(theme.label_height) } else { 0 },
        max_scale: theme.max_scale,
    }
}

#[derive(Debug)]
pub struct OverviewUi {
    previews: Vec<Preview>,
    entries: Vec<Entry>,
    hover: glide_ui::HoverAnimation<WindowId>,
    close_hover: glide_ui::HoverAnimation<WindowId>,
    phase: Phase,
    duration: Duration,
    theme: OverviewTheme,
    place: Place,
    focused: Option<WindowId>,
    workspaces: Vec<WorkspaceId>,
    active: WorkspaceId,
    can_add: bool,
    scroll: glide_ui::Tween,
    scrub: f64,
    scroll_release: Option<glide_theme::motion::Release>,
    release_velocity: Option<f64>,
    strip: strip::Strip,
    close_watch: close::Watch,
    handoff: f64,
    drag: Option<drag::Drag>,
    moved: Option<(WindowId, Rectangle<f64, Logical>)>,
    landings: Vec<Landing>,
    scrim: SolidColorBuffer,
    page_cards: PerScale<(RasterKey, (i32, i32), MemoryRenderBuffer)>,
}

const HANDOFF_LIFT: f64 = 280.0;
const HANDOFF_SHRINK: f64 = 0.08;
const HANDOFF_STRIP: f64 = 110.0;

fn handoff_rect(rect: Rectangle<f64, Logical>, lift: f64) -> Rectangle<f64, Logical> {
    let factor = HANDOFF_SHRINK.mul_add(-lift, 1.0);
    let (w, h) = (rect.size.w * factor, rect.size.h * factor);
    let x = rect.loc.x + (rect.size.w - w) / 2.0;
    let y = (rect.size.h - h).mul_add(0.5, rect.loc.y) - HANDOFF_LIFT * lift;
    Rectangle::new(Point::from((x, y)), Size::from((w.max(1.0), h.max(1.0))))
}

fn fitted(frame: Rectangle<i32, Logical>, output: Rectangle<i32, Logical>) -> Rectangle<f64, Logical> {
    let (frame, output) = (frame.to_f64(), output.to_f64());
    let shrink = (output.size.w / frame.size.w.max(1.0)).min(output.size.h / frame.size.h.max(1.0)).min(1.0);
    let size = Size::from((frame.size.w * shrink, frame.size.h * shrink));
    let x = frame.loc.x.clamp(output.loc.x, (output.loc.x + output.size.w - size.w).max(output.loc.x));
    let y = frame.loc.y.clamp(output.loc.y, (output.loc.y + output.size.h - size.h).max(output.loc.y));
    Rectangle::new(Point::from((x, y)), size)
}

fn mini_rect(
    frame: Rectangle<i32, Logical>, thumb: Rectangle<f64, Logical>, output: Rectangle<i32, Logical>,
) -> Rectangle<f64, Logical> {
    let k = thumb.size.w / f64::from(output.size.w.max(1));
    let inside = fitted(frame, output);
    Rectangle::new(
        Point::from((
            (inside.loc.x - f64::from(output.loc.x)).mul_add(k, thumb.loc.x),
            (inside.loc.y - f64::from(output.loc.y)).mul_add(k, thumb.loc.y),
        )),
        Size::from((inside.size.w * k, inside.size.h * k)),
    )
}

fn layout_previews(
    entries: &[Entry], grid: Rectangle<i32, Logical>, theme: &OverviewTheme, old: &mut [Preview],
) -> Vec<Preview> {
    let layout_area = glide_layout::Rectangle::from_loc_and_size(grid.loc.x, grid.loc.y, grid.size.w, grid.size.h);
    let mut workspaces: Vec<WorkspaceId> = Vec::new();
    for entry in entries {
        if !workspaces.contains(&entry.workspace) {
            workspaces.push(entry.workspace);
        }
    }
    let mut slots = std::collections::HashMap::new();
    for workspace in workspaces {
        let sizes: Vec<(WindowId, glide_layout::Size)> = entries
            .iter()
            .filter(|entry| entry.workspace == workspace)
            .map(|entry| (entry.id, glide_layout::Size::new(entry.frame.size.w, entry.frame.size.h)))
            .collect();
        for slot in compute_overview_layout(&sizes, layout_area, &options(theme)) {
            slots.insert(slot.id, slot);
        }
    }
    entries
        .iter()
        .enumerate()
        .filter_map(|(stack, entry)| {
            let slot = slots.get(&entry.id)?;
            let target = to_smithay_rect(slot.target);
            let (label, highlight, close, glide) = old
                .iter_mut()
                .find(|p| p.id == entry.id)
                .map(|p| {
                    let glide = if p.target == target && p.workspace == entry.workspace {
                        p.glide.take()
                    } else if p.workspace == entry.workspace {
                        let mut tween = glide_ui::Tween::at(0.0);
                        tween.retarget(1.0, slide(), glide_theme::motion::ease_out);
                        Some((round_rect(p.target_now()), tween))
                    } else {
                        None
                    };
                    (
                        std::mem::take(&mut p.label),
                        std::mem::take(&mut p.highlight),
                        std::mem::take(&mut p.close),
                        glide,
                    )
                })
                .unwrap_or_default();
            Some(Preview {
                id: entry.id,
                frame: entry.frame,
                workspace: entry.workspace,
                target,
                row: slot.row,
                stack,
                glide,
                label,
                highlight,
                close,
            })
        })
        .collect()
}

impl OverviewUi {
    #[must_use]
    pub fn new(
        entries: &[Entry], place: Place, theme: &OverviewTheme, focused: Option<WindowId>,
        (workspaces, active, can_add): (Vec<WorkspaceId>, WorkspaceId, bool),
    ) -> Self {
        let mut hover = glide_ui::HoverAnimation::default();
        let mut strip = strip::Strip::default();
        strip.set(workspaces.clone(), active, can_add);
        let index = workspaces.iter().position(|ws| *ws == active).unwrap_or(0) as f64;
        let mut ui = Self {
            previews: Vec::new(),
            entries: entries.to_vec(),
            hover: glide_ui::HoverAnimation::default(),
            close_hover: glide_ui::HoverAnimation::default(),
            phase: Phase::Opening { started: glide_ui::clock::now(), from: SceneFrom::ZERO },
            duration: theme.duration(),
            theme: theme.clone(),
            place,
            focused,
            workspaces,
            active,
            can_add,
            scroll: glide_ui::Tween::at(index),
            scrub: 0.0,
            scroll_release: None,
            release_velocity: None,
            strip,
            close_watch: close::Watch::default(),
            handoff: 0.0,
            drag: None,
            moved: None,
            landings: Vec::new(),
            scrim: SolidColorBuffer::default(),
            page_cards: PerScale::default(),
        };
        ui.previews = layout_previews(entries, ui.grid_area(), theme, &mut []);
        hover.retarget_without_restarting(focused.filter(|id| ui.on_active_page(*id)));
        ui.hover = hover;
        ui
    }

    #[must_use]
    pub const fn place(&self) -> Place {
        self.place
    }

    #[must_use]
    pub fn contains(&self, pos: Point<f64, Logical>) -> bool {
        self.place.output.to_f64().contains(pos)
    }

    pub fn set_handoff(&mut self, progress: f64) -> bool {
        let progress = progress.clamp(0.0, 1.0);
        let changed = (self.handoff - progress).abs() > f64::EPSILON;
        self.handoff = progress;
        changed
    }

    fn handoff_lift(&self) -> f64 {
        glide_theme::motion::ease_io(glide_theme::motion::window(self.handoff, 0.0, 0.6))
    }

    fn handoff_strip(&self) -> f64 {
        glide_theme::motion::ease_io(glide_theme::motion::window(self.handoff, 0.0, 0.4))
    }

    pub fn watch_close(&mut self, id: WindowId, newest: WindowId) {
        self.close_watch.ask(id, newest, glide_ui::clock::now());
    }

    #[must_use]
    pub fn awaits_close(&self) -> bool {
        self.close_watch.is_waiting()
    }

    pub fn refused_close(&mut self, live: &[close::Live]) -> Option<WindowId> {
        if self.is_closing() {
            return None;
        }
        self.close_watch.poll(glide_ui::clock::now(), live)
    }

    fn grid_area(&self) -> Rectangle<i32, Logical> {
        let area = self.place.area;
        let band = strip::band(area, self.place.output);
        Rectangle::new(
            Point::from((area.loc.x, area.loc.y + band)),
            Size::from((area.size.w, (area.size.h - band).max(1))),
        )
    }

    fn stride(&self) -> f64 {
        f64::from((self.place.area.size.w - self.theme.padding - PEEK).max(1))
    }

    fn page_card(&self) -> Rectangle<f64, Logical> {
        let grid = self.grid_area().to_f64();
        let inset = f64::from(self.theme.padding) / 2.0;
        Rectangle::new(
            Point::from((grid.loc.x + inset, grid.loc.y)),
            Size::from(((grid.size.w - 2.0 * inset).max(1.0), (grid.size.h - inset).max(1.0))),
        )
    }

    fn page(&self, workspace: WorkspaceId) -> Option<usize> {
        self.workspaces.iter().position(|ws| *ws == workspace)
    }

    fn active_page(&self) -> usize {
        self.page(self.active).unwrap_or(0)
    }

    fn scroll_duration(&self) -> Duration {
        self.scroll_release.map_or_else(slide, |release| release.duration)
    }

    fn base_scroll(&self) -> f64 {
        let release = self.scroll_release;
        let ease = |t: f64| release.map_or_else(|| glide_theme::motion::ease_out(t), |release| release.at(t));
        self.scroll.value(self.scroll_duration(), ease)
    }

    fn scroll(&self) -> f64 {
        self.base_scroll() + self.scrub
    }

    fn retarget_scroll(&mut self, target: f64) {
        let from = self.base_scroll();
        let release = self
            .release_velocity
            .take()
            .map(|velocity| glide_theme::motion::Release::new(target - from, velocity, slide()));
        self.scroll.jump(from);
        self.scroll_release = release;
        self.scroll.retarget(target, self.scroll_duration(), glide_theme::motion::ease_out);
    }

    pub fn scrub_pages(&mut self, offset: f64) -> bool {
        let offset = offset.clamp(-1.0, 1.0);
        let blocked = (offset > 0.0 && self.neighbour(1).is_none()) || (offset < 0.0 && self.neighbour(-1).is_none());
        let offset = if blocked { offset * RUBBER_BAND } else { offset };
        let changed = (self.scrub - offset).abs() > f64::EPSILON;
        self.scrub = offset;
        changed
    }

    pub fn release_scrub(&mut self, velocity: f64) {
        let from = self.scroll();
        self.scrub = 0.0;
        self.scroll_release = None;
        self.scroll.jump(from);
        self.release_velocity = Some(velocity);
    }

    pub fn settle_scroll(&mut self) {
        if self.release_velocity.is_some() {
            self.retarget_scroll(self.active_page() as f64);
        }
    }

    fn page_dx(&self, page: usize) -> f64 {
        (page as f64 - self.scroll()) * self.stride()
    }

    fn origin(&self, preview: &Preview, page: usize) -> Rectangle<f64, Logical> {
        let delta = page as f64 - self.active_page() as f64;
        moved_by(preview.frame.to_f64(), delta * f64::from(self.place.output.size.w))
    }

    fn tile_rect(&self, preview: &Preview, t: f64) -> Option<Rectangle<f64, Logical>> {
        let page = self.page(preview.workspace)?;
        let target = moved_by(preview.target_now(), self.page_dx(page));
        Some(handoff_rect(lerp_rect(self.origin(preview, page), target, t), self.handoff_lift()))
    }

    fn on_active_page(&self, id: WindowId) -> bool {
        self.previews.iter().any(|p| p.id == id && p.workspace == self.active)
    }

    #[must_use]
    pub fn workspace_of(&self, id: WindowId) -> Option<WorkspaceId> {
        self.previews.iter().find(|p| p.id == id).map(|p| p.workspace)
    }

    pub fn set_workspaces(&mut self, workspaces: Vec<WorkspaceId>, active: WorkspaceId, can_add: bool) {
        let target = workspaces.iter().position(|ws| *ws == active).unwrap_or(0) as f64;
        let before = self.page(self.active);
        let after = workspaces.iter().position(|ws| *ws == self.active);
        if let (Some(before), Some(after)) = (before, after)
            && before != after
        {
            let shifted = self.base_scroll() - before as f64 + after as f64;
            self.scroll.jump(shifted);
        } else if let Some(before) = before
            && after.is_none()
        {
            let side = if target >= before as f64 { 1.0 } else { 0.0 };
            self.scroll.jump(self.base_scroll() - side);
        }
        if (self.scroll.target() - target).abs() > f64::EPSILON || self.release_velocity.is_some() {
            self.retarget_scroll(target);
        }
        self.strip.set(workspaces.clone(), active, can_add);
        self.workspaces = workspaces;
        self.active = active;
        self.can_add = can_add;
        if self.hover.target().is_some_and(|id| !self.on_active_page(id)) {
            self.hover.retarget_without_restarting(None);
        }
    }

    #[must_use]
    pub fn strip_at(&self, pos: Point<f64, Logical>) -> Option<Slot> {
        (self.progress() > 0.0).then(|| self.strip.slot_at(self.place.area, self.place.output, pos)).flatten()
    }

    pub fn set_strip_hover(&mut self, hover: Option<Slot>) -> bool {
        self.strip.set_hover(hover)
    }

    fn front_window(&self) -> Option<WindowId> {
        match self.phase {
            Phase::Closing { select: Some(id), .. } => Some(id),
            _ => self.focused,
        }
    }

    fn paint_order(&self) -> Vec<usize> {
        let front = self.front_window();
        let active = self.active;
        let mut ranked: Vec<usize> = (0..self.previews.len()).collect();
        ranked.sort_by_key(|&index| {
            let p = &self.previews[index];
            std::cmp::Reverse((p.workspace == active, front == Some(p.id), p.stack))
        });
        ranked
    }

    pub fn relayout(&mut self, entries: &[Entry], place: Place) {
        if self.place == place && self.entries == entries {
            return;
        }
        self.place = place;
        self.entries = entries.to_vec();
        let old_hover_index = self.hover.target().and_then(|id| self.active_ids().iter().position(|p| *p == id));
        let mut old = std::mem::take(&mut self.previews);
        self.previews = layout_previews(entries, self.grid_area(), &self.theme, &mut old);

        if let Some((id, rect)) = self.moved.take() {
            self.settle_moved(id, rect);
        }
        if self.drag.as_ref().is_some_and(|drag| !self.previews.iter().any(|p| p.id == drag.id)) {
            self.drag = None;
            self.strip.set_dragging(false);
            self.strip.set_drop(None);
        }
        self.landings.retain(|landing| entries.iter().any(|entry| entry.id == landing.id));

        if self.hover.target().is_some_and(|id| !self.on_active_page(id)) {
            let active = self.active_ids();
            let neighbor = old_hover_index.and_then(|idx| active.get(idx.min(active.len().saturating_sub(1))).copied());
            self.hover.retarget_without_restarting(neighbor);
        }
        if self.close_hover.target().is_some_and(|id| !self.on_active_page(id)) {
            self.close_hover.retarget_without_restarting(None);
        }
    }

    fn settle_moved(&mut self, id: WindowId, rect: Rectangle<f64, Logical>) {
        let Some(workspace) = self.workspace_of(id) else {
            return;
        };
        if workspace == self.active {
            self.glide_from(id, rect);
        } else {
            self.landings.retain(|landing| landing.id != id);
            self.landings.push(Landing { id, from: rect, started: glide_ui::clock::now() });
        }
    }

    fn glide_from(&mut self, id: WindowId, rect: Rectangle<f64, Logical>) {
        let Some(index) = self.previews.iter().position(|p| p.id == id) else {
            return;
        };
        let Some(page) = self.page(self.previews[index].workspace) else {
            return;
        };
        let local = moved_by(rect, -self.page_dx(page));
        let mut tween = glide_ui::Tween::at(0.0);
        tween.retarget(1.0, slide(), glide_theme::motion::ease_out);
        self.previews[index].glide = Some((round_rect(local), tween));
    }

    fn active_ids(&self) -> Vec<WindowId> {
        self.previews.iter().filter(|p| p.workspace == self.active).map(|p| p.id).collect()
    }

    pub fn windows(&self) -> impl Iterator<Item = WindowId> + '_ {
        self.previews.iter().map(|p| p.id)
    }

    fn elapsed_fraction(&self, started: Instant) -> f64 {
        if self.duration.is_zero() {
            return 1.0;
        }
        let elapsed = glide_ui::clock::since(started).as_secs_f64();
        (elapsed / self.duration.as_secs_f64()).clamp(0.0, 1.0)
    }

    fn scene_progress(&self, curve: glide_theme::motion::CubicBezier, backdrop: bool) -> f64 {
        match self.phase {
            Phase::Opening { started, from } => {
                let eased = from.ease(curve, self.elapsed_fraction(started));
                let from = from.pick(backdrop);
                from + (1.0 - from) * eased
            }
            Phase::Open => 1.0,
            Phase::Closing { started, from, .. } => {
                from.pick(backdrop) * (1.0 - from.ease(curve, self.elapsed_fraction(started)))
            }
            Phase::Interactive { progress } => progress.clamp(0.0, 1.0),
        }
    }

    #[must_use]
    pub fn progress(&self) -> f64 {
        self.scene_progress(glide_theme::motion::E_OUT, false)
    }

    #[must_use]
    pub fn backdrop_progress(&self) -> f64 {
        self.scene_progress(glide_theme::motion::E_IO, true)
    }

    #[must_use]
    pub fn is_animating(&self) -> bool {
        let phase_animating = match self.phase {
            Phase::Open | Phase::Interactive { .. } => false,
            Phase::Opening { started, .. } | Phase::Closing { started, .. } => {
                glide_ui::clock::since(started) < self.duration
            }
        };
        phase_animating || self.hovers_animating()
    }

    fn hovers_animating(&self) -> bool {
        self.hover.progress(hover_anim()) < 1.0
            || self.close_hover.progress(hover_anim()) < 1.0
            || self.strip.is_animating()
            || self.scroll.is_animating(self.scroll_duration())
            || self.previews.iter().any(Preview::is_gliding)
            || !self.landings.is_empty()
            || self.drag.as_ref().is_some_and(|drag| drag.is_animating() || drag.dwell_pending())
    }

    pub fn tick(&mut self) -> Tick {
        self.landings.retain(|landing| landing.progress() < 1.0);
        match self.phase {
            Phase::Opening { started, .. } if glide_ui::clock::since(started) >= self.duration => {
                self.phase = Phase::Open;
                Tick::Running
            }
            Phase::Closing { started, select, .. } if glide_ui::clock::since(started) >= self.duration => {
                Tick::Finished { select }
            }
            Phase::Open | Phase::Interactive { .. } if self.hovers_animating() => Tick::Running,
            Phase::Open | Phase::Interactive { .. } => Tick::Idle,
            Phase::Opening { .. } | Phase::Closing { .. } => Tick::Running,
        }
    }

    pub fn set_interactive_progress(&mut self, progress: f64) {
        self.phase = Phase::Interactive { progress: progress.clamp(0.0, 1.0) };
    }

    pub fn commit_to_opening(&mut self, velocity: f64) {
        let current = self.progress();
        if current >= 1.0 {
            self.phase = Phase::Open;
            return;
        }
        let release = glide_theme::motion::Release::new(1.0 - current, velocity, self.theme.duration());
        let from = SceneFrom { primary: current, backdrop: self.backdrop_progress(), release: Some(release) };
        self.duration = release.duration;
        self.phase = Phase::Opening { started: glide_ui::clock::now(), from };
    }

    pub fn catch(&mut self) -> bool {
        if !matches!(self.phase, Phase::Opening { .. } | Phase::Closing { .. }) {
            return false;
        }
        self.set_interactive_progress(self.progress());
        true
    }

    pub fn revert_to_closing(&mut self, velocity: f64) {
        self.close_from(None, Some(velocity));
    }

    #[must_use]
    pub const fn is_interactive(&self) -> bool {
        matches!(self.phase, Phase::Interactive { .. })
    }

    pub fn begin_close(&mut self, select: Option<WindowId>) {
        self.cancel_drag();
        self.close_from(select, None);
    }

    fn close_from(&mut self, select: Option<WindowId>, velocity: Option<f64>) {
        if matches!(self.phase, Phase::Closing { .. }) {
            return;
        }
        let primary = self.progress();
        let release =
            velocity.map(|velocity| glide_theme::motion::Release::new(-primary, velocity, self.theme.duration()));
        let from = SceneFrom { primary, backdrop: self.backdrop_progress(), release };
        self.duration = release.map_or_else(|| self.scaled_duration(primary), |release| release.duration);
        self.phase = Phase::Closing { started: glide_ui::clock::now(), select, from };
    }

    fn scaled_duration(&self, fraction: f64) -> Duration {
        let base_duration = self.theme.duration();
        base_duration.mul_f64(fraction.clamp(0.0, 1.0)).max(glide_theme::motion::t1()).min(base_duration)
    }

    #[must_use]
    pub const fn is_closing(&self) -> bool {
        matches!(self.phase, Phase::Closing { .. })
    }

    #[must_use]
    pub fn hovered(&self) -> Option<WindowId> {
        self.hover.target()
    }

    #[must_use]
    pub fn preview_at(&self, pos: Point<f64, Logical>) -> Option<WindowId> {
        if !self.contains(pos) {
            return None;
        }
        let t = self.progress();
        let dragged = self.dragged();
        self.paint_order()
            .into_iter()
            .map(|index| &self.previews[index])
            .filter(|p| Some(p.id) != dragged)
            .find(|p| self.tile_rect(p, t).is_some_and(|rect| rect.contains(pos)))
            .map(|p| p.id)
    }

    pub fn set_hover(&mut self, hover: Option<WindowId>) -> bool {
        let hover = hover.filter(|id| self.on_active_page(*id));
        self.hover.set(hover)
    }

    fn hover_progress(&self) -> f64 {
        glide_theme::motion::E_OUT.ease_f64(self.hover.progress(hover_anim()))
    }

    fn close_hover_progress(&self) -> f64 {
        glide_theme::motion::E_OVER.ease_f64(self.close_hover.progress(hover_anim()))
    }

    #[must_use]
    pub fn preview_rect(&self, id: WindowId) -> Option<Rectangle<i32, Logical>> {
        let t = self.progress();
        self.previews.iter().find(|p| p.id == id).and_then(|p| self.tile_rect(p, t)).map(round_rect)
    }

    #[must_use]
    pub fn close_button_at(&self, pos: Point<f64, Logical>) -> Option<WindowId> {
        if self.progress() < 1.0 || self.drag.is_some() {
            return None;
        }
        let id = self.hover.target()?;
        let rect = self.preview_rect(id)?;
        close_button_rect(rect).to_f64().contains(pos).then_some(id)
    }

    pub fn set_close_hover(&mut self, hover: Option<WindowId>) -> bool {
        self.close_hover.set(hover)
    }

    pub fn navigate(&mut self, dir: Direction) -> bool {
        let t = self.progress();
        let active: Vec<(WindowId, usize, f64)> = self
            .previews
            .iter()
            .filter(|p| p.workspace == self.active)
            .filter_map(|p| {
                let rect = self.tile_rect(p, t)?;
                Some((p.id, p.row, rect.loc.x + rect.size.w / 2.0))
            })
            .collect();
        if active.is_empty() {
            return false;
        }
        let Some(current) = self.hover.target().and_then(|id| active.iter().position(|(p, ..)| *p == id)) else {
            let default = match dir {
                Direction::Left | Direction::Up => active.last(),
                Direction::Right | Direction::Down => active.first(),
            };
            return self.set_hover(default.map(|(id, ..)| *id));
        };
        let next = match dir {
            Direction::Left => current.checked_sub(1),
            Direction::Right => (current + 1 < active.len()).then_some(current + 1),
            Direction::Up | Direction::Down => {
                let (_, row, centre) = active[current];
                let target_row = if matches!(dir, Direction::Up) {
                    match row.checked_sub(1) {
                        Some(r) => r,
                        None => return false,
                    }
                } else {
                    row + 1
                };
                active
                    .iter()
                    .enumerate()
                    .filter(|(_, (_, row, _))| *row == target_row)
                    .min_by(|(_, a), (_, b)| (a.2 - centre).abs().total_cmp(&(b.2 - centre).abs()))
                    .map(|(i, _)| i)
            }
        };
        next.is_some_and(|index| self.set_hover(Some(active[index].0)))
    }

    #[must_use]
    pub fn neighbour(&self, step: isize) -> Option<WorkspaceId> {
        let index = self.active_page().checked_add_signed(step)?;
        self.workspaces.get(index).copied()
    }

    #[must_use]
    pub fn is_dragging(&self) -> bool {
        self.drag.as_ref().is_some_and(drag::Drag::is_lifted)
    }

    #[must_use]
    pub fn is_pressed(&self) -> bool {
        self.drag.is_some() || self.strip.reorder_pressed()
    }

    pub fn press_strip(&mut self, workspace: WorkspaceId, pos: Point<f64, Logical>) -> bool {
        if self.is_closing() || self.drag.is_some() {
            return false;
        }
        self.strip.press_reorder(workspace, pos.x, self.place.area, self.place.output);
        true
    }

    fn dragged(&self) -> Option<WindowId> {
        self.drag.as_ref().filter(|drag| drag.is_lifted()).map(|drag| drag.id)
    }

    pub fn press(&mut self, id: WindowId, pos: Point<f64, Logical>) -> bool {
        if self.is_closing() || !self.on_active_page(id) {
            return false;
        }
        let Some(rect) = self.preview_rect(id) else {
            return false;
        };
        self.drag = Some(drag::Drag::press(id, pos, rect));
        true
    }

    pub fn drag_motion(&mut self, pos: Point<f64, Logical>) -> bool {
        if self.strip.reorder_pressed() {
            return self.strip.reorder_motion(pos.x, self.place.area, self.place.output);
        }
        let Some(drag) = self.drag.as_mut() else {
            return false;
        };
        let was_lifted = drag.is_lifted();
        if !drag.motion(pos) {
            return false;
        }
        if !was_lifted {
            self.strip.set_dragging(true);
            self.hover.retarget_without_restarting(None);
            self.close_hover.retarget_without_restarting(None);
        }
        let source = self.drag.as_ref().and_then(|drag| self.workspace_of(drag.id));
        let target = self.drop_target(pos, source);
        let slot = match target {
            Some(Target::Workspace(id)) => Some(Slot::Workspace(id)),
            Some(Target::New) => Some(Slot::Add),
            _ => None,
        };
        self.strip.set_drop(slot);
        if let Some(drag) = self.drag.as_mut() {
            drag.set_target(target, glide_ui::clock::now());
        }
        true
    }

    fn drop_target(&self, pos: Point<f64, Logical>, source: Option<WorkspaceId>) -> Option<Target> {
        if let Some(slot) = self.strip.slot_at(self.place.area, self.place.output, pos) {
            return match slot {
                Slot::Workspace(id) if Some(id) == source => None,
                Slot::Workspace(id) => Some(Target::Workspace(id)),
                Slot::Add => Some(Target::New),
            };
        }
        let grid = self.grid_area().to_f64();
        if pos.y < grid.loc.y || pos.y > grid.loc.y + grid.size.h {
            return None;
        }
        let output = self.place.output.to_f64();
        if pos.x < output.loc.x + EDGE_ZONE && self.neighbour(-1).is_some() {
            return Some(Target::Edge(Side::Left));
        }
        if pos.x > output.loc.x + output.size.w - EDGE_ZONE && (self.neighbour(1).is_some() || self.can_add) {
            return Some(Target::Edge(Side::Right));
        }
        grid.contains(pos).then_some(Target::Page)
    }

    pub fn drag_dwell(&mut self) -> Option<WorkspaceId> {
        let side = self.drag.as_mut().filter(|drag| drag.is_lifted())?.dwell_elapsed(glide_ui::clock::now())?;
        self.neighbour(if side == Side::Left { -1 } else { 1 })
    }

    pub fn release(&mut self) -> Option<Release> {
        if let Some(reordered) = self.strip.release_reorder(self.place.area, self.place.output) {
            return Some(match reordered {
                strip::Reordered::Click(workspace) => Release::SwitchTo(workspace),
                strip::Reordered::Moved { id, index } => Release::Reorder { workspace: id, index },
                strip::Reordered::Settled => Release::Settled,
            });
        }
        let drag = self.drag.take()?;
        self.strip.set_dragging(false);
        self.strip.set_drop(None);
        if !drag.is_lifted() {
            return Some(Release::Click(drag.id));
        }
        let rect = drag.rect();
        let source = self.workspace_of(drag.id);
        let to = match drag.target() {
            Some(Target::Workspace(id)) => Some(MoveTo::Workspace(id)),
            Some(Target::New) => Some(MoveTo::New),
            Some(Target::Edge(Side::Left)) => self.neighbour(-1).map(MoveTo::Workspace),
            Some(Target::Edge(Side::Right)) => {
                self.neighbour(1).map(MoveTo::Workspace).or_else(|| self.can_add.then_some(MoveTo::New))
            }
            Some(Target::Page) => (source != Some(self.active)).then_some(MoveTo::Workspace(self.active)),
            None => None,
        };
        if let Some(to) = to.filter(|to| source.is_none_or(|source| *to != MoveTo::Workspace(source))) {
            self.moved = Some((drag.id, rect));
            Some(Release::Move { id: drag.id, to })
        } else {
            self.glide_from(drag.id, rect);
            Some(Release::Settled)
        }
    }

    pub fn cancel_drag(&mut self) -> bool {
        if self.strip.cancel_reorder() {
            return true;
        }
        let Some(drag) = self.drag.take() else {
            return false;
        };
        self.strip.set_dragging(false);
        self.strip.set_drop(None);
        if drag.is_lifted() {
            self.glide_from(drag.id, drag.rect());
        }
        true
    }

    pub fn prepare_move(&mut self, id: WindowId) {
        let t = self.progress();
        self.moved =
            self.previews.iter().find(|p| p.id == id).and_then(|p| self.tile_rect(p, t)).map(|rect| (id, rect));
    }

    pub fn abandon_move(&mut self) {
        if let Some((id, rect)) = self.moved.take() {
            self.glide_from(id, rect);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tick {
    Idle,
    Running,
    Finished { select: Option<WindowId> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

const LABEL_PADDING: i32 = 12;
const LABEL_ICON_INSET: i32 = 5;
const LABEL_ICON_GAP: i32 = 6;
const LABEL_RISE: f64 = 4.0;
const OV_CLOSE_SIZE: i32 = 26;
const OV_CLOSE_OUTSET: i32 = 10;
const OV_CLOSE_HOVER_SCALE: f64 = 1.12;
const OV_CLOSE_GLYPH: f32 = 5.5;
const OV_CLOSE_GLYPH_STROKE: f32 = 1.65;
const OV_CLOSE_BORDER: f32 = 1.0;

fn close_shadow_layers(appearance: glide_theme::Appearance) -> Vec<glide_theme::elevation::ShadowLayer> {
    glide_theme::elevation::layers(glide_theme::elevation::Level::El2, appearance)
}

fn close_shadow_margin() -> i32 {
    static MARGIN: std::sync::OnceLock<i32> = std::sync::OnceLock::new();
    *MARGIN.get_or_init(|| {
        let widest = [glide_theme::Appearance::Light, glide_theme::Appearance::Dark]
            .into_iter()
            .flat_map(close_shadow_layers)
            .map(|layer| layer.blur)
            .fold(0.0_f32, f32::max)
            .ceil() as i32;
        crate::paint::shadow_fade_margin(widest)
    })
}

fn close_button_rect(preview: Rectangle<i32, Logical>) -> Rectangle<i32, Logical> {
    Rectangle::new(
        Point::from((
            preview.loc.x + preview.size.w + OV_CLOSE_OUTSET - OV_CLOSE_SIZE,
            preview.loc.y - OV_CLOSE_OUTSET,
        )),
        Size::from((OV_CLOSE_SIZE, OV_CLOSE_SIZE)),
    )
}

#[must_use]
pub const fn label_offset(label_height: i32) -> i32 {
    -label_height / 2
}

const fn label_icon_side(label_height: i32) -> i32 {
    label_height - LABEL_ICON_INSET * 2
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LabelMetrics {
    width: i32,
    text: (i32, i32),
}

fn label_metrics(text_w: i32, label_height: i32, icon: bool, max_w: i32) -> LabelMetrics {
    let lead = if icon { LABEL_ICON_INSET + label_icon_side(label_height) + LABEL_ICON_GAP } else { LABEL_PADDING };
    let width = (lead + text_w + LABEL_PADDING).clamp(1, max_w.max(1));
    LabelMetrics { width, text: (lead, (width - LABEL_PADDING).max(lead)) }
}

fn to_smithay_rect(rect: glide_layout::Rectangle) -> Rectangle<i32, Logical> {
    Rectangle::new(Point::from((rect.loc.x, rect.loc.y)), Size::from((rect.size.width, rect.size.height)))
}

smithay::render_elements! {
    pub OverviewElement<R> where R: ImportAll + ImportMem;
    Solid = SolidColorRenderElement,
    Buffer = MemoryRenderBufferRenderElement<R>,
    ScaledBuffer = RescaleRenderElement<MemoryRenderBufferRenderElement<R>>,
}

impl<R: Renderer> std::fmt::Debug for OverviewElement<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Solid(e) => f.debug_tuple("Solid").field(e).finish(),
            Self::Buffer(e) => f.debug_tuple("Buffer").field(e).finish(),
            Self::ScaledBuffer(e) => f.debug_tuple("ScaledBuffer").field(e).finish(),
            Self::_GenericCatcher(e) => f.debug_tuple("_GenericCatcher").field(e).finish(),
        }
    }
}

fn hover_bucket(t: f32) -> u8 {
    (t.clamp(0.0, 1.0) * 20.0).round() as u8
}

fn draw_close_button(theme: &OverviewTheme, hover_t: f32, output_scale: i32) -> Option<MemoryRenderBuffer> {
    let margin = close_shadow_margin();
    let bg = theme.close_background.lerp(theme.close_hover_background, hover_t);
    let fg = theme.close_foreground.lerp(theme.close_hover_foreground, hover_t);
    let border = theme.close_border.lerp(theme.close_hover_background, hover_t);
    let radius = OV_CLOSE_SIZE as f32 / 2.0;
    let side = (OV_CLOSE_SIZE, OV_CLOSE_SIZE);
    let mut c = crate::raster::shadow(side, margin, radius, &close_shadow_layers(theme.appearance), output_scale)?;
    let button = crate::raster::rect(margin, margin, OV_CLOSE_SIZE, OV_CLOSE_SIZE);
    c.fill(button, bg, radius);
    c.stroke(button, OV_CLOSE_BORDER, radius, border);
    let (glyph, stroke) = (OV_CLOSE_GLYPH, OV_CLOSE_GLYPH_STROKE);
    crate::paint::draw_close_glyph_over(&mut c.buf, c.w, c.h, glyph, stroke, output_scale, fg);
    Some(c.finish())
}

fn draw_page_card(
    theme: &OverviewTheme, (w, h): (i32, i32), output_scale: i32,
) -> Option<crate::raster::Canvas<'static>> {
    let mut c = crate::raster::Canvas::new(w, h, output_scale)?;
    let radius = glide_theme::shape::R4;
    c.fill(c.full(), theme.strip_add, radius);
    c.stroke(c.full(), 1.0, radius, theme.strip_add_border);
    Some(c)
}

fn paint_label(
    c: &mut crate::raster::Canvas<'_>, theme: &OverviewTheme, painter: &crate::paint::Painter, icon: Option<&[u8]>,
    title: &str, text: (i32, i32),
) {
    let (height, scale) = (c.h, c.scale);
    let (radius, s) = (height as f32 / 2.0, scale as f32);
    c.panel(theme.label_background, radius);
    crate::paint::draw_inset_border(&mut c.buf, c.w * scale, c.h * scale, s, radius * s, theme.label_border);
    if let Some(pixels) = icon {
        let side = label_icon_side(height);
        c.blit_square(pixels, side, (LABEL_ICON_INSET, (height - side) / 2));
    }
    let align = if icon.is_some() { crate::paint::TextAlign::Left } else { crate::paint::TextAlign::Center };
    c.label(painter, text, align, title, theme.label_color, &glide_theme::TextStyle::OV_LABEL);
}

fn place<R: ShellRenderer>(
    renderer: &mut R, pos: Point<f64, Physical>, memory: &MemoryRenderBuffer, alpha: f32,
) -> Option<MemoryRenderBufferRenderElement<R>> {
    MemoryRenderBufferRenderElement::from_buffer(renderer, pos, memory, Some(alpha), None, None, Kind::Unspecified).ok()
}

fn place_in<R: ShellRenderer>(
    renderer: &mut R, rect: Rectangle<f64, Logical>, (w, h): (i32, i32), memory: &MemoryRenderBuffer, alpha: f32,
    scale: Scale<f64>,
) -> Option<MemoryRenderBufferRenderElement<R>> {
    let src = Rectangle::from_size(Size::from((f64::from(w), f64::from(h))));
    let size = Size::from((rect.size.w.round().max(1.0) as i32, rect.size.h.round().max(1.0) as i32));
    MemoryRenderBufferRenderElement::from_buffer(
        renderer,
        rect.loc.to_physical(scale),
        memory,
        Some(alpha),
        Some(src),
        Some(size),
        Kind::Unspecified,
    )
    .ok()
}

pub struct Frame<'a, F> {
    pub scale: Scale<f64>,
    pub output_scale: i32,
    pub painter: &'a crate::paint::Painter,
    pub icons: &'a mut crate::icon::IconLoader,
    pub wallpaper: WallpaperFit,
    pub lookup: F,
}

impl OverviewUi {
    fn render_label<R: ShellRenderer>(
        &mut self, renderer: &mut R, (index, rect): (usize, Rectangle<i32, Logical>), tile: &TileInfo,
        frame: &mut Frame<'_, impl FnMut(WindowId) -> Option<TileInfo>>, (alpha, rise): (f32, i32),
    ) -> Option<OverviewElement<R>> {
        let theme = &self.theme;
        let (height, output_scale) = (theme.label_height, frame.output_scale);
        let side = label_icon_side(height);
        let app = tile.app.as_deref().filter(|app| side > 0 && frame.icons.load(app, side * output_scale).is_some());
        let text_w = frame.painter.text_width_styled(&tile.title, &glide_theme::TextStyle::OV_LABEL).ceil() as i32;
        let metrics = label_metrics(text_w, height, app.is_some(), rect.size.w);
        let key = LabelKey {
            title: tile.title.clone(),
            app: app.map(str::to_string),
            width: metrics.width,
            raster: RasterKey::new(theme, output_scale),
            icons: frame.icons.generation(),
        };
        let preview = &mut self.previews[index];
        if preview.label.get(&output_scale).is_none_or(|(cached, _)| *cached != key) {
            let mut c = crate::raster::Canvas::new(metrics.width, height, output_scale)?;
            let icon = app.and_then(|app| frame.icons.load(app, side * output_scale));
            paint_label(&mut c, theme, frame.painter, icon, &tile.title, metrics.text);
            preview.label.insert(output_scale, (key, c.finish()));
        }
        let (_, memory) = preview.label.get(&output_scale)?;
        let pos = Point::<i32, Logical>::from((
            rect.loc.x + (rect.size.w - metrics.width) / 2,
            rect.loc.y + rect.size.h + label_offset(height) + rise,
        ))
        .to_f64()
        .to_physical(frame.scale);
        place(renderer, pos, memory, alpha).map(OverviewElement::Buffer)
    }

    fn render_close<R: ShellRenderer>(
        &mut self, renderer: &mut R, index: usize, rect: Rectangle<i32, Logical>, alpha: f32, scale: Scale<f64>,
        output_scale: i32,
    ) -> Option<OverviewElement<R>> {
        let id = self.previews[index].id;
        let t = if self.close_hover.target() == Some(id) {
            self.close_hover_progress().clamp(0.0, 1.0) as f32
        } else {
            0.0
        };
        let bucket = hover_bucket(t);
        let key = RasterKey::new(&self.theme, output_scale);
        let theme = self.theme.clone();
        let preview = &mut self.previews[index];
        if preview
            .close
            .get(&output_scale)
            .is_none_or(|(cached, cached_key, _)| *cached != bucket || *cached_key != key)
            && let Some(memory) = draw_close_button(&theme, f32::from(bucket) / 20.0, output_scale)
        {
            preview.close.insert(output_scale, (bucket, key, memory));
        }
        let (_, _, memory) = preview.close.get(&output_scale)?;
        let button = close_button_rect(rect);
        let margin = close_shadow_margin();
        let pos = Point::from((button.loc.x - margin, button.loc.y - margin)).to_f64().to_physical(scale);
        let element = place(renderer, pos, memory, alpha)?;
        let grow = 1.0 + f64::from(t) * (OV_CLOSE_HOVER_SCALE - 1.0);
        if (grow - 1.0).abs() < f64::EPSILON {
            return Some(OverviewElement::Buffer(element));
        }
        let centre = Point::from((button.loc.x + OV_CLOSE_SIZE / 2, button.loc.y + OV_CLOSE_SIZE / 2))
            .to_physical_precise_round(scale);
        Some(OverviewElement::ScaledBuffer(RescaleRenderElement::from_element(element, centre, grow)))
    }

    fn render_ring<R: ShellRenderer>(
        &mut self, renderer: &mut R, index: usize, rect: Rectangle<i32, Logical>, alpha: f32, scale: Scale<f64>,
        output_scale: i32,
    ) -> Option<OverviewElement<R>> {
        let (w, h) = (rect.size.w.max(1), rect.size.h.max(1));
        let key = RasterKey::new(&self.theme, output_scale);
        let (radius, border, colour) = (self.theme.corner_radius, self.theme.border, self.theme.highlight);
        let preview = &mut self.previews[index];
        let outdated = preview
            .highlight
            .get(&output_scale)
            .is_none_or(|(cw, ch, cached, _)| *cw != w || *ch != h || *cached != key);
        let (pw, ph) = (w * output_scale, h * output_scale);
        if outdated && let Ok(pixels) = usize::try_from(pw * ph * 4) {
            let mut buf = vec![0u8; pixels];
            let radius = radius * output_scale as f32;
            crate::paint::draw_inset_border(&mut buf, pw, ph, border as f32 * output_scale as f32, radius, colour);
            preview.highlight.insert(output_scale, (w, h, key, crate::raster::buffer_from(&buf, w, h, output_scale)));
        }
        let (cw, ch, _, memory) = preview.highlight.get(&output_scale)?;
        place_in(renderer, rect.to_f64(), (*cw, *ch), memory, alpha, scale).map(OverviewElement::Buffer)
    }

    fn thumbnail_stack<R: ShellRenderer>(
        &self, slot: Slot, pieces: Vec<OverviewElement<R>>, walls: &mut Vec<(WorkspaceId, OverviewElement<R>)>,
        hidden: &[WindowId], (alpha, rise): (f32, f64),
    ) -> Vec<Layer<R>> {
        let mut stack: Vec<Layer<R>> = pieces.into_iter().map(Layer::chrome).collect();
        let Slot::Workspace(workspace) = slot else {
            return stack;
        };
        let output = self.place.output;
        if alpha > 0.0
            && let Some(thumb) = self.strip.thumb_rect(self.place.area, output, workspace, rise)
        {
            let mut on_page: Vec<&Preview> =
                self.previews.iter().filter(|p| p.workspace == workspace && !hidden.contains(&p.id)).collect();
            on_page.sort_by_key(|p| std::cmp::Reverse(p.stack));
            stack.extend(on_page.into_iter().map(|p| {
                Layer::Window(WindowDraw {
                    id: p.id,
                    frame: p.frame,
                    rect: mini_rect(p.frame, thumb, output),
                    alpha,
                    radius: MINI_RADIUS,
                })
            }));
        }
        if let Some(position) = walls.iter().position(|(id, _)| *id == workspace) {
            stack.push(Layer::chrome(walls.remove(position).1));
        }
        stack
    }

    fn landing_rect(&self, landing: &Landing, rise: f64) -> Option<Rectangle<f64, Logical>> {
        let preview = self.previews.iter().find(|p| p.id == landing.id)?;
        let thumb = self.strip.thumb_rect(self.place.area, self.place.output, preview.workspace, rise)?;
        let to = mini_rect(preview.frame, thumb, self.place.output);
        Some(lerp_rect(landing.from, to, glide_theme::motion::ease_out(landing.progress())))
    }

    #[allow(clippy::too_many_lines)]
    #[must_use]
    pub fn render<R, F>(&mut self, renderer: &mut R, mut frame: Frame<'_, F>) -> Vec<Layer<R>>
    where
        R: ShellRenderer,
        F: FnMut(WindowId) -> Option<TileInfo>,
    {
        let (scale, output_scale) = (frame.scale, frame.output_scale);
        let t = self.progress();
        let output = self.place.output;
        let lift = self.handoff_lift();
        let exit_alpha = glide_theme::motion::assist(1.0 - lift) as f32;
        let mut layers: Vec<Layer<R>> = Vec::new();

        let corner = self.theme.corner_radius;
        if let Some(drag) = self.drag.as_ref().filter(|drag| drag.is_lifted())
            && let Some(preview) = self.previews.iter().find(|p| p.id == drag.id)
        {
            layers.push(Layer::Window(WindowDraw {
                id: drag.id,
                frame: preview.frame,
                rect: drag.rect(),
                alpha: exit_alpha,
                radius: corner,
            }));
        }

        let leaving = self.handoff_strip();
        let strip_alpha =
            (glide_theme::motion::assist(t) * glide_theme::motion::assist(1.0 - leaving)) as f32 * exit_alpha.max(0.0);
        let strip_rise = (1.0 - t).mul_add(strip::RISE, leaving * HANDOFF_STRIP);
        let landings = self.landings.clone();
        for landing in &landings {
            let Some(rect) = self.landing_rect(landing, strip_rise) else {
                continue;
            };
            let Some(preview) = self.previews.iter().find(|p| p.id == landing.id) else {
                continue;
            };
            let eased = glide_theme::motion::ease_out(landing.progress()) as f32;
            layers.push(Layer::Window(WindowDraw {
                id: landing.id,
                frame: preview.frame,
                rect,
                alpha: exit_alpha,
                radius: (MINI_RADIUS - corner).mul_add(eased, corner),
            }));
        }

        let place = (self.place.area, output);
        let theme = self.theme.clone();
        let chrome = self.strip.render_chrome(
            renderer,
            (&theme, frame.painter),
            place,
            (strip_alpha, strip_rise),
            (scale, output_scale),
        );
        let mut walls = self.strip.render_wallpapers(
            renderer,
            place,
            (strip_alpha, strip_rise, frame.wallpaper),
            (scale, output_scale),
        );

        let dragged = self.dragged();
        let hidden: Vec<WindowId> = dragged.into_iter().chain(landings.iter().map(|landing| landing.id)).collect();
        let lifted = self.strip.lifted();
        let (add, thumbs): (Vec<_>, Vec<_>) = chrome.slots.into_iter().partition(|(slot, _)| *slot == Slot::Add);
        let mut thumbs = thumbs.into_iter();
        let mut ring = Some(chrome.ring);
        let mut push_ring = |layers: &mut Vec<Layer<R>>| {
            layers.extend(ring.take().into_iter().flatten().map(Layer::chrome));
        };
        if lifted.is_none_or(|id| id == self.active) {
            push_ring(&mut layers);
        }
        for (slot, pieces) in thumbs.by_ref().take(usize::from(lifted.is_some())) {
            layers.extend(self.thumbnail_stack(slot, pieces, &mut walls, &hidden, (strip_alpha, strip_rise)));
        }
        push_ring(&mut layers);
        layers.extend(add.into_iter().flat_map(|(_, pieces)| pieces).map(Layer::chrome));
        for (slot, pieces) in thumbs {
            layers.extend(self.thumbnail_stack(slot, pieces, &mut walls, &hidden, (strip_alpha, strip_rise)));
        }

        let output_f = output.to_f64();
        let mut tiles: Vec<(usize, Rectangle<f64, Logical>, bool)> = Vec::new();
        for index in self.paint_order() {
            let preview = &self.previews[index];
            if hidden.contains(&preview.id) {
                continue;
            }
            let Some(rect) = self.tile_rect(preview, t) else {
                continue;
            };
            if rect.overlaps(output_f) {
                tiles.push((index, rect, preview.workspace == self.active));
            }
        }

        if t > 0.0 && dragged.is_none() {
            let rise = ((1.0 - t) * LABEL_RISE).round() as i32;
            let label_alpha = t.clamp(0.0, 1.0) as f32 * exit_alpha;
            if theme.label {
                for &(index, rect, active) in &tiles {
                    if !active {
                        continue;
                    }
                    let Some(tile) = (frame.lookup)(self.previews[index].id) else {
                        continue;
                    };
                    if let Some(element) =
                        self.render_label(renderer, (index, round_rect(rect)), &tile, &mut frame, (label_alpha, rise))
                    {
                        layers.push(Layer::chrome(element));
                    }
                }
            }
            let hovered = self.hover.target();
            if let Some(&(index, rect, _)) =
                tiles.iter().find(|(index, _, active)| *active && Some(self.previews[*index].id) == hovered)
            {
                let rect = round_rect(rect);
                if let Some(element) = self.render_close(renderer, index, rect, label_alpha, scale, output_scale) {
                    layers.push(Layer::chrome(element));
                }
                let ring_alpha = (self.hover_progress() * t.clamp(0.0, 1.0)) as f32 * exit_alpha;
                if ring_alpha > 0.0
                    && let Some(element) = self.render_ring(renderer, index, rect, ring_alpha, scale, output_scale)
                {
                    layers.push(Layer::chrome(element));
                }
            }
        }

        for (index, rect, active) in tiles {
            let preview = &self.previews[index];
            if (frame.lookup)(preview.id).is_none() {
                continue;
            }
            let alpha = if active { exit_alpha } else { exit_alpha * NEIGHBOUR_ALPHA };
            layers.push(Layer::Window(WindowDraw {
                id: preview.id,
                frame: preview.frame,
                rect,
                alpha,
                radius: corner,
            }));
        }

        let card_alpha = glide_theme::motion::assist(t) as f32 * exit_alpha;
        if card_alpha > 0.0 {
            let card = self.page_card();
            let size = (card.size.w.round() as i32, card.size.h.round() as i32);
            let key = RasterKey::new(&theme, output_scale);
            let outdated =
                self.page_cards.get(&output_scale).is_none_or(|(cached, at, _)| *cached != key || *at != size);
            if outdated && let Some(memory) = draw_page_card(&theme, size, output_scale).map(|c| c.finish()) {
                self.page_cards.insert(output_scale, (key, size, memory));
            }
            if let Some((_, size, memory)) = self.page_cards.get(&output_scale) {
                let active = self.active_page();
                for page in (0..self.workspaces.len()).filter(|page| *page != active) {
                    let rect = moved_by(card, self.page_dx(page));
                    if rect.overlaps(output_f) {
                        let element = place_in(renderer, rect, *size, memory, card_alpha, scale);
                        layers.extend(element.map(|element| Layer::chrome(OverviewElement::Buffer(element))));
                    }
                }
            }
        }

        let backdrop_t = (self.backdrop_progress() * (1.0 - glide_theme::motion::ease_io(self.handoff))) as f32;
        let scrim_color = theme.dim.premultiplied(backdrop_t);
        self.scrim.update((output.size.w, output.size.h), scrim_color);
        layers.push(Layer::chrome(OverviewElement::Solid(SolidColorRenderElement::from_buffer(
            &self.scrim,
            output.loc.to_f64().to_physical(scale).to_i32_round(),
            scale,
            1.0,
            Kind::Unspecified,
        ))));
        layers
    }
}

#[cfg(test)]
mod tests;

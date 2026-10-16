use std::time::{Duration, Instant};

use glide_layout::{WindowId, compute_overview_layout};
use smithay::{
    backend::renderer::{
        ImportAll, ImportMem, Renderer,
        element::{
            Kind,
            memory::{MemoryRenderBuffer, MemoryRenderBufferRenderElement},
            solid::{SolidColorBuffer, SolidColorRenderElement},
            utils::{Relocate, RelocateRenderElement, RescaleRenderElement},
        },
    },
    utils::{Logical, Point, Rectangle, Scale, Size},
};

use crate::{
    clip::ClipCapableRenderer,
    config::{OverviewTheme, OverviewThemeExt},
    raster::ShellRenderer,
    window::{GlideWindow, GlideWindowRenderElement},
};

#[path = "overview_strip.rs"]
mod strip;

#[path = "overview_close.rs"]
pub mod close;

type OverviewEntry = (WindowId, Rectangle<i32, Logical>, bool, Option<Tile>);

#[derive(Debug, Clone)]
pub struct Tile {
    pub window: GlideWindow,
    pub title: String,
    pub app: Option<String>,
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

fn lerp_rect(from: Rectangle<i32, Logical>, to: Rectangle<i32, Logical>, t: f64) -> Rectangle<i32, Logical> {
    let mix = |a: i32, b: i32| (f64::from(a) + (f64::from(b) - f64::from(a)) * t).round() as i32;

    Rectangle::new(
        Point::from((mix(from.loc.x, to.loc.x), mix(from.loc.y, to.loc.y))),
        Size::from((mix(from.size.w, to.size.w).max(1), mix(from.size.h, to.size.h).max(1))),
    )
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

#[derive(Debug, Clone)]
pub struct Preview {
    pub id: WindowId,
    pub target: Rectangle<i32, Logical>,
    pub origin: Rectangle<i32, Logical>,
    pub row: usize,
    stack: usize,
    label: PerScale<CachedLabel>,
    highlight: PerScale<CachedHighlight>,
    close: PerScale<CachedClose>,
}

impl Preview {
    #[must_use]
    pub fn rect_at(&self, t: f64) -> Rectangle<i32, Logical> {
        lerp_rect(self.origin, self.target, t)
    }

    #[must_use]
    pub fn scale_at(&self, t: f64) -> f64 {
        let from = 1.0;
        let to = f64::from(self.target.size.w) / f64::from(self.origin.size.w.max(1));
        from + (to - from) * t
    }
}

#[derive(Debug)]
pub struct OverviewUi {
    previews: Vec<Preview>,
    hover: glide_ui::HoverAnimation<WindowId>,
    close_hover: glide_ui::HoverAnimation<WindowId>,
    phase: Phase,
    duration: Duration,
    theme: OverviewTheme,
    area: Rectangle<i32, Logical>,
    focused: Option<WindowId>,
    strip: strip::Strip,
    close_watch: close::Watch,

    scrim: SolidColorBuffer,
}

fn paint_rank(preview: &Preview, front: Option<WindowId>, area: Rectangle<i32, Logical>) -> (bool, bool, usize) {
    (front == Some(preview.id), preview.origin.overlaps(area), preview.stack)
}

fn layout_previews(
    windows: &[(WindowId, Rectangle<i32, Logical>)], area: Rectangle<i32, Logical>, theme: &OverviewTheme,
    old: &mut [Preview],
) -> Vec<Preview> {
    let sizes: Vec<(WindowId, glide_layout::Size)> =
        windows.iter().map(|(id, rect)| (*id, glide_layout::Size::new(rect.size.w, rect.size.h))).collect();
    let layout_area = glide_layout::Rectangle::from_loc_and_size(area.loc.x, area.loc.y, area.size.w, area.size.h);
    compute_overview_layout(&sizes, layout_area, &theme.options())
        .iter()
        .filter_map(|slot| {
            let stack = windows.iter().position(|(id, _)| *id == slot.id)?;
            let (label, highlight, close) = old
                .iter_mut()
                .find(|p| p.id == slot.id)
                .map(|p| (std::mem::take(&mut p.label), std::mem::take(&mut p.highlight), std::mem::take(&mut p.close)))
                .unwrap_or_default();
            Some(Preview {
                id: slot.id,
                target: to_smithay_rect(slot.target),
                origin: windows[stack].1,
                row: slot.row,
                stack,
                label,
                highlight,
                close,
            })
        })
        .collect()
}

impl OverviewUi {
    pub fn new(
        windows: &[(WindowId, Rectangle<i32, Logical>)], area: Rectangle<i32, Logical>, theme: &OverviewTheme,
        focused: Option<WindowId>,
    ) -> Self {
        let previews = layout_previews(windows, area, theme, &mut []);
        let mut hover = glide_ui::HoverAnimation::default();
        hover.retarget_without_restarting(focused);

        Self {
            previews,
            hover,
            close_hover: glide_ui::HoverAnimation::default(),
            phase: Phase::Opening { started: Instant::now(), from: SceneFrom::ZERO },
            duration: theme.duration(),
            theme: theme.clone(),
            area,
            focused,
            strip: strip::Strip::default(),
            close_watch: close::Watch::default(),
            scrim: SolidColorBuffer::default(),
        }
    }

    pub fn watch_close(&mut self, id: WindowId, newest: WindowId) {
        self.close_watch.ask(id, newest, Instant::now());
    }

    #[must_use]
    pub fn awaits_close(&self) -> bool {
        self.close_watch.is_waiting()
    }

    pub fn refused_close(&mut self, live: &[close::Live]) -> Option<WindowId> {
        if self.is_closing() {
            return None;
        }
        self.close_watch.poll(Instant::now(), live)
    }

    fn grid_area(&self) -> Rectangle<i32, Logical> {
        let band = self.strip.band();
        Rectangle::new(
            Point::from((self.area.loc.x, self.area.loc.y + band)),
            Size::from((self.area.size.w, (self.area.size.h - band).max(1))),
        )
    }

    pub fn set_workspaces(&mut self, workspaces: Vec<glide_layout::WorkspaceId>, active: glide_layout::WorkspaceId) {
        if self.strip.set(workspaces, active) {
            let windows: Vec<(WindowId, Rectangle<i32, Logical>)> =
                self.previews.iter().map(|p| (p.id, p.origin)).collect();
            let mut old = std::mem::take(&mut self.previews);
            self.previews = layout_previews(&windows, self.grid_area(), &self.theme, &mut old);
        }
    }

    #[must_use]
    pub fn workspace_at(&self, pos: Point<f64, Logical>) -> Option<glide_layout::WorkspaceId> {
        (self.progress() > 0.0).then(|| self.strip.workspace_at(self.area, pos)).flatten()
    }

    pub fn set_strip_hover(&mut self, hover: Option<glide_layout::WorkspaceId>) -> bool {
        self.strip.set_hover(hover)
    }

    fn front_window(&self) -> Option<WindowId> {
        match self.phase {
            Phase::Closing { select: Some(id), .. } => Some(id),
            _ => self.focused,
        }
    }

    fn paint_order(&self) -> Vec<WindowId> {
        let front = self.front_window();
        let mut ranked: Vec<&Preview> = self.previews.iter().collect();
        ranked.sort_by_key(|p| std::cmp::Reverse(paint_rank(p, front, self.area)));
        ranked.into_iter().map(|p| p.id).collect()
    }

    fn matches_current_layout(
        &self, windows: &[(WindowId, Rectangle<i32, Logical>)], area: Rectangle<i32, Logical>,
    ) -> bool {
        self.area == area
            && self.previews.len() == windows.len()
            && self
                .previews
                .iter()
                .zip(windows)
                .all(|(preview, (id, origin))| preview.id == *id && preview.origin == *origin)
    }

    pub fn relayout(&mut self, windows: &[(WindowId, Rectangle<i32, Logical>)], area: Rectangle<i32, Logical>) {
        if self.matches_current_layout(windows, area) {
            return;
        }
        self.area = area;
        let old_hover_index = self.hover.target().and_then(|id| self.previews.iter().position(|p| p.id == id));
        let mut old = std::mem::take(&mut self.previews);
        self.previews = layout_previews(windows, self.grid_area(), &self.theme, &mut old);

        if self.hover.target().is_some_and(|id| !self.contains(id)) {
            let neighbor = old_hover_index.and_then(|idx| {
                let clamped = idx.min(self.previews.len().saturating_sub(1));
                self.previews.get(clamped).map(|p| p.id)
            });
            self.hover.retarget_without_restarting(neighbor);
        }
        if self.close_hover.target().is_some_and(|id| !self.contains(id)) {
            self.close_hover.retarget_without_restarting(None);
        }
    }

    fn contains(&self, id: WindowId) -> bool {
        self.previews.iter().any(|p| p.id == id)
    }

    pub fn windows(&self) -> impl Iterator<Item = WindowId> + '_ {
        self.previews.iter().map(|p| p.id)
    }

    fn elapsed_fraction(&self, started: Instant) -> f64 {
        if self.duration.is_zero() {
            return 1.0;
        }
        let elapsed = started.elapsed().as_secs_f64();
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
            Phase::Opening { started, .. } | Phase::Closing { started, .. } => started.elapsed() < self.duration,
        };
        phase_animating || self.hovers_animating()
    }

    fn hovers_animating(&self) -> bool {
        self.hover.progress(hover_anim()) < 1.0
            || self.close_hover.progress(hover_anim()) < 1.0
            || self.strip.is_animating()
    }

    pub fn tick(&mut self) -> Tick {
        match self.phase {
            Phase::Opening { started, .. } if started.elapsed() >= self.duration => {
                self.phase = Phase::Open;
                Tick::Running
            }
            Phase::Closing { started, select, .. } if started.elapsed() >= self.duration => Tick::Finished { select },
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
        self.phase = Phase::Opening { started: Instant::now(), from };
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
        self.phase = Phase::Closing { started: Instant::now(), select, from };
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
        let t = self.progress();
        self.previews.iter().find(|p| p.rect_at(t).to_f64().contains(pos)).map(|p| p.id)
    }

    pub fn set_hover(&mut self, hover: Option<WindowId>) -> bool {
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
        self.previews.iter().find(|p| p.id == id).map(|p| p.rect_at(t))
    }

    #[must_use]
    pub fn close_button_at(&self, pos: Point<f64, Logical>) -> Option<WindowId> {
        let t = self.progress();
        if t < 1.0 {
            return None;
        }
        self.previews.iter().find(|p| close_button_rect(p.rect_at(t)).to_f64().contains(pos)).map(|p| p.id)
    }

    pub fn set_close_hover(&mut self, hover: Option<WindowId>) -> bool {
        self.close_hover.set(hover)
    }

    pub fn navigate(&mut self, dir: Direction) -> bool {
        if self.previews.is_empty() {
            return false;
        }

        let Some(current) = self.hover.target().and_then(|id| self.previews.iter().position(|p| p.id == id)) else {
            let default = match dir {
                Direction::Left | Direction::Up => self.previews.last(),
                Direction::Right | Direction::Down => self.previews.first(),
            };
            return self.set_hover(default.map(|p| p.id));
        };

        let next = match dir {
            Direction::Left => current.checked_sub(1),
            Direction::Right => (current + 1 < self.previews.len()).then_some(current + 1),
            Direction::Up | Direction::Down => {
                let row = self.previews[current].row;
                let target_row = if matches!(dir, Direction::Up) {
                    match row.checked_sub(1) {
                        Some(r) => r,
                        None => return false,
                    }
                } else {
                    row + 1
                };

                let t = self.progress();
                let centre = centre_x(self.previews[current].rect_at(t));

                self.previews
                    .iter()
                    .enumerate()
                    .filter(|(_, p)| p.row == target_row)
                    .min_by(|(_, a), (_, b)| {
                        let da = (centre_x(a.rect_at(t)) - centre).abs();
                        let db = (centre_x(b.rect_at(t)) - centre).abs();
                        da.total_cmp(&db)
                    })
                    .map(|(i, _)| i)
            }
        };

        match next {
            Some(index) => self.set_hover(Some(self.previews[index].id)),
            None => false,
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

pub(crate) const fn label_offset(label_height: i32) -> i32 {
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

fn centre_x(rect: Rectangle<i32, Logical>) -> f64 {
    f64::from(rect.loc.x) + f64::from(rect.size.w) / 2.0
}

fn to_smithay_rect(rect: glide_layout::Rectangle) -> Rectangle<i32, Logical> {
    Rectangle::new(Point::from((rect.loc.x, rect.loc.y)), Size::from((rect.size.width, rect.size.height)))
}

smithay::render_elements! {
    pub OverviewRenderElement<R> where R: ImportAll + ImportMem + ClipCapableRenderer;
    Preview = RelocateRenderElement<RescaleRenderElement<GlideWindowRenderElement<R>>>,
    Solid = SolidColorRenderElement,
    Buffer = MemoryRenderBufferRenderElement<R>,
    ScaledBuffer = RescaleRenderElement<MemoryRenderBufferRenderElement<R>>,
}

impl<R: Renderer> std::fmt::Debug for OverviewRenderElement<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Preview(e) => f.debug_tuple("Preview").field(e).finish(),
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

fn place<R>(
    renderer: &mut R, pos: Point<f64, smithay::utils::Physical>, memory: &MemoryRenderBuffer, alpha: f32,
) -> Option<MemoryRenderBufferRenderElement<R>>
where
    R: ShellRenderer,
{
    MemoryRenderBufferRenderElement::from_buffer(renderer, pos, memory, Some(alpha), None, None, Kind::Unspecified).ok()
}

#[allow(clippy::too_many_arguments)]
fn push_close_buttons<R>(
    renderer: &mut R, previews: &mut [Preview], entries: &[OverviewEntry], theme: &OverviewTheme,
    hovered: Option<WindowId>, hover_t: f32, alpha: f32, scale: Scale<f64>, output_scale: i32,
    elements: &mut Vec<OverviewRenderElement<R>>,
) where
    R: ShellRenderer + ClipCapableRenderer,
{
    for (id, rect, _, resolved) in entries {
        if resolved.is_none() {
            continue;
        }
        let Some(preview) = previews.iter_mut().find(|p| p.id == *id) else {
            continue;
        };

        let t = if hovered == Some(*id) { hover_t } else { 0.0 };
        let bucket = hover_bucket(t);
        let key = RasterKey::new(theme, output_scale);
        let needs_redraw = preview
            .close
            .get(&output_scale)
            .is_none_or(|(cached_bucket, cached_key, _)| *cached_bucket != bucket || *cached_key != key);

        if needs_redraw {
            let quantized = f32::from(bucket) / 20.0;
            if let Some(memory) = draw_close_button(theme, quantized, output_scale) {
                preview.close.insert(output_scale, (bucket, key, memory));
            }
        }

        let Some((_, _, memory)) = preview.close.get(&output_scale) else {
            continue;
        };

        let button = close_button_rect(*rect);
        let margin = close_shadow_margin();

        let pos = Point::from((button.loc.x - margin, button.loc.y - margin)).to_f64().to_physical(scale);

        let Some(element) = place(renderer, pos, memory, alpha) else {
            continue;
        };

        let grow = 1.0 + f64::from(t) * (OV_CLOSE_HOVER_SCALE - 1.0);
        if (grow - 1.0).abs() < f64::EPSILON {
            elements.push(OverviewRenderElement::Buffer(element));
        } else {
            let centre = Point::from((button.loc.x + OV_CLOSE_SIZE / 2, button.loc.y + OV_CLOSE_SIZE / 2))
                .to_physical_precise_round(scale);
            elements
                .push(OverviewRenderElement::ScaledBuffer(RescaleRenderElement::from_element(element, centre, grow)));
        }
    }
}

impl OverviewUi {
    fn render_labels<R>(
        &mut self, renderer: &mut R, entries: &[OverviewEntry], scale: Scale<f64>, output_scale: i32,
        painter: &crate::paint::Painter, icons: &mut crate::icon::IconLoader,
    ) -> Vec<OverviewRenderElement<R>>
    where
        R: ShellRenderer + ClipCapableRenderer,
        R::Error: std::fmt::Debug,
    {
        let t = self.progress();
        let theme = self.theme.clone();
        if !theme.label || t <= 0.0 {
            return Vec::new();
        }

        let alpha = t.clamp(0.0, 1.0) as f32;
        let rise = ((1.0 - t) * LABEL_RISE).round() as i32;
        let height = theme.label_height;
        let side = label_icon_side(height);
        icons.expire_missing();

        let mut elements = Vec::new();
        for (id, rect, _, resolved) in entries {
            let Some(tile) = resolved else {
                continue;
            };
            let Some(preview) = self.previews.iter_mut().find(|p| p.id == *id) else {
                continue;
            };

            let app = tile.app.as_deref().filter(|app| side > 0 && icons.load(app, side * output_scale).is_some());
            let text_w = painter.text_width_styled(&tile.title, &glide_theme::TextStyle::OV_LABEL).ceil() as i32;
            let metrics = label_metrics(text_w, height, app.is_some(), rect.size.w);

            let key = LabelKey {
                title: tile.title.clone(),
                app: app.map(str::to_string),
                width: metrics.width,
                raster: RasterKey::new(&theme, output_scale),
                icons: icons.generation(),
            };
            if preview.label.get(&output_scale).is_none_or(|(cached, _)| *cached != key) {
                let Some(mut c) = crate::raster::Canvas::new(metrics.width, height, output_scale) else {
                    continue;
                };
                let icon = app.and_then(|app| icons.load(app, side * output_scale));
                paint_label(&mut c, &theme, painter, icon, &tile.title, metrics.text);
                preview.label.insert(output_scale, (key, c.finish()));
            }

            let Some((_, memory)) = preview.label.get(&output_scale) else {
                continue;
            };
            let width = metrics.width;

            let pos = Point::from((
                rect.loc.x + (rect.size.w - width) / 2,
                rect.loc.y + rect.size.h + label_offset(height) + rise,
            ))
            .to_f64()
            .to_physical(scale);
            elements.extend(place(renderer, pos, memory, alpha).map(OverviewRenderElement::Buffer));
        }
        elements
    }

    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    #[profiling::function]
    pub fn render<R, F>(
        &mut self, renderer: &mut R, output_rect: Rectangle<i32, Logical>, scale: Scale<f64>, output_scale: i32,
        painter: &crate::paint::Painter, icons: &mut crate::icon::IconLoader, mut lookup: F,
    ) -> Vec<OverviewRenderElement<R>>
    where
        R: ShellRenderer + ClipCapableRenderer,
        R::Error: std::fmt::Debug,
        F: FnMut(WindowId) -> Option<Tile>,
    {
        let t = self.progress();
        let mut elements = Vec::new();

        let hover = self.hover.target();
        let theme = self.theme.clone();

        let entries: Vec<OverviewEntry> =
            self.previews.iter().map(|p| (p.id, p.rect_at(t), hover == Some(p.id), lookup(p.id))).collect();

        elements.extend(self.render_labels(renderer, &entries, scale, output_scale, painter, icons));
        self.strip.render(renderer, &theme, (self.area, t), (scale, output_scale), &mut elements);

        if t > 0.0 {
            let button_alpha = t.clamp(0.0, 1.0) as f32;
            let close_hovered = self.close_hover.target();
            let close_hover_t = self.close_hover_progress().clamp(0.0, 1.0) as f32;
            push_close_buttons(
                renderer,
                &mut self.previews,
                &entries,
                &theme,
                close_hovered,
                close_hover_t,
                button_alpha,
                scale,
                output_scale,
                &mut elements,
            );
        }

        let hover_t = self.hover_progress() * t.clamp(0.0, 1.0);
        if hover_t > 0.0
            && let Some(hover_id) = hover
            && let Some(preview) = self.previews.iter_mut().find(|p| p.id == hover_id)
        {
            let rect = preview.rect_at(t);
            let (w, h) = (preview.target.size.w.max(1), preview.target.size.h.max(1));
            let key = RasterKey::new(&theme, output_scale);
            let needs_redraw =
                preview.highlight.get(&output_scale).is_none_or(|(cached_w, cached_h, cached_key, _)| {
                    *cached_w != w || *cached_h != h || *cached_key != key
                });

            let (pw, ph) = (w * output_scale, h * output_scale);
            if needs_redraw && let Ok(pixels) = usize::try_from(pw * ph * 4) {
                let mut buf = vec![0u8; pixels];
                let radius = theme.corner_radius * output_scale as f32;
                let border_width = theme.border as f32 * output_scale as f32;
                crate::paint::draw_inset_border(&mut buf, pw, ph, border_width, radius, theme.highlight);
                let memory = crate::raster::buffer_from(&buf, w, h, output_scale);
                preview.highlight.insert(output_scale, (w, h, key, memory));
            }

            if let Some((cached_w, cached_h, _, memory)) = preview.highlight.get(&output_scale) {
                let pos = rect.loc.to_f64().to_physical(scale);
                let alpha = hover_t as f32;
                let src = Rectangle::from_size(Size::from((f64::from(*cached_w), f64::from(*cached_h))));
                if let Ok(element) = MemoryRenderBufferRenderElement::from_buffer(
                    renderer,
                    pos,
                    memory,
                    Some(alpha),
                    Some(src),
                    Some(rect.size),
                    Kind::Unspecified,
                ) {
                    elements.push(OverviewRenderElement::Buffer(element));
                }
            }
        }

        let stacked: Vec<&OverviewEntry> =
            self.paint_order().into_iter().filter_map(|id| entries.iter().find(|(entry, ..)| *entry == id)).collect();
        for (id, rect, _, resolved) in stacked {
            let Some(Tile { window, .. }) = resolved else {
                continue;
            };
            let Some(preview) = self.previews.iter().find(|p| p.id == *id) else {
                continue;
            };

            let window_scale = preview.scale_at(t);

            let buffer_offset = window.geometry().loc;
            let content_origin = Point::from((
                preview.origin.loc.x + GlideWindow::insets_left() - buffer_offset.x,
                preview.origin.loc.y + window.header_height() - buffer_offset.y,
            ));
            let real_physical = content_origin.to_f64().to_physical(scale).to_i32_round();
            let frame_physical = preview.origin.loc.to_f64().to_physical(scale).to_i32_round();
            let target_physical = rect.loc.to_f64().to_physical(scale).to_i32_round();

            let raw: Vec<GlideWindowRenderElement<R>> =
                window.preview_render_elements(renderer, real_physical, scale, 1.0);

            for element in raw {
                let scaled = RescaleRenderElement::from_element(element, frame_physical, window_scale);
                let placed =
                    RelocateRenderElement::from_element(scaled, target_physical - frame_physical, Relocate::Relative);
                elements.push(OverviewRenderElement::Preview(placed));
            }
        }

        let backdrop_t = self.backdrop_progress() as f32;
        let scrim_color = theme.dim.0.map(|c| (c * backdrop_t).clamp(0.0, 1.0));
        self.scrim.update((output_rect.size.w, output_rect.size.h), scrim_color);
        elements.push(OverviewRenderElement::Solid(SolidColorRenderElement::from_buffer(
            &self.scrim,
            output_rect.loc.to_f64().to_physical(scale).to_i32_round(),
            scale,
            1.0,
            Kind::Unspecified,
        )));

        elements
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme_with_duration(ms: u64) -> OverviewTheme {
        OverviewTheme { duration_ms: ms, ..OverviewTheme::default() }
    }

    use crate::raster::rect;

    fn centre(rect: Rectangle<i32, Logical>) -> Point<f64, Logical> {
        Point::from((
            f64::from(rect.loc.x) + f64::from(rect.size.w) / 2.0,
            f64::from(rect.loc.y) + f64::from(rect.size.h) / 2.0,
        ))
    }

    fn ago(ms: u64) -> Instant {
        Instant::now()
            .checked_sub(Duration::from_millis(ms))
            .expect("the process has been up for longer than the test's offset")
    }

    fn closing_since(ms: u64) -> Phase {
        Phase::Closing {
            started: ago(ms),
            select: None,
            from: SceneFrom { primary: 1.0, backdrop: 1.0, release: None },
        }
    }

    #[test]
    fn the_focused_window_paints_above_neighbours_and_other_workspaces_paint_last() {
        let focused = WindowId::next();
        let above = WindowId::next();
        let elsewhere = WindowId::next();
        let windows = [
            (focused, rect(100, 100, 800, 600)),
            (above, rect(300, 200, 800, 600)),
            (elsewhere, rect(2020, 100, 800, 600)),
        ];
        let area = rect(0, 0, 1920, 1080);
        let mut ui = OverviewUi::new(&windows, area, &theme_with_duration(200), Some(focused));
        assert_eq!(ui.paint_order(), vec![focused, above, elsewhere]);

        ui.begin_close(Some(elsewhere));
        assert_eq!(ui.paint_order()[0], elsewhere);
    }

    fn ui_with_one_window(duration_ms: u64) -> (OverviewUi, WindowId) {
        let id = WindowId::next();
        let windows = [(id, rect(0, 0, 800, 600))];
        let area = rect(0, 0, 1920, 1080);
        let ui = OverviewUi::new(&windows, area, &theme_with_duration(duration_ms), Some(id));
        (ui, id)
    }

    mod scene_easing {
        use super::*;

        #[test]
        fn the_exit_is_front_loaded_just_like_the_entry() {
            let (mut ui, _id) = ui_with_one_window(400);
            ui.phase = closing_since(80);
            let remaining = ui.progress();
            assert!(remaining < 0.6, "the exit stalled: still {remaining} open a fifth of the way through");
        }

        #[test]
        fn closing_mirrors_opening_over_the_same_elapsed_time() {
            for millis in [40_u64, 120, 200, 320] {
                let (mut opening, _) = ui_with_one_window(400);
                opening.phase = Phase::opening(ago(millis));
                let (mut closing, _) = ui_with_one_window(400);
                closing.phase = closing_since(millis);
                let sum = opening.progress() + closing.progress();
                assert!(
                    (sum - 1.0).abs() < 0.05,
                    "at {millis}ms opening {} and closing {} should sum to 1",
                    opening.progress(),
                    closing.progress()
                );
            }
        }

        #[test]
        fn a_settled_overview_is_fully_progressed_on_both_curves() {
            let (ui, _id) = ui_with_one_window(0);
            assert!((ui.progress() - 1.0).abs() < f64::EPSILON);
            assert!((ui.backdrop_progress() - 1.0).abs() < f64::EPSILON);
        }

        #[test]
        fn previews_lead_the_backdrop_mid_transition() {
            let (mut ui, _id) = ui_with_one_window(400);
            ui.phase = Phase::opening(ago(80));
            let previews = ui.progress();
            let backdrop = ui.backdrop_progress();
            assert!(previews > backdrop, "previews ({previews}) must lead the backdrop ({backdrop})");
            assert!((0.0..1.0).contains(&backdrop), "backdrop: {backdrop}");
        }

        #[test]
        fn closing_runs_both_curves_back_towards_zero() {
            let (mut ui, _id) = ui_with_one_window(400);
            ui.begin_close(None);
            ui.phase = closing_since(320);
            assert!(ui.progress() < 0.5);
            assert!(ui.backdrop_progress() < 0.5);
        }
    }

    #[test]
    fn the_preview_scale_moves_linearly_from_the_window_to_its_slot() {
        let (ui, _id) = ui_with_one_window(0);
        let preview = &ui.previews[0];
        let settled = f64::from(preview.target.size.w) / f64::from(preview.origin.size.w);
        for step in 0..=200 {
            let t = f64::from(step) / 200.0;
            assert!((preview.scale_at(t) - (1.0 + (settled - 1.0) * t)).abs() < 1e-9);
        }
        assert!((LABEL_RISE - 4.0).abs() < f64::EPSILON, ".ov-label starts 4px low");
        assert_eq!((label_offset(28), label_offset(40)), (-14, -20));
        for height in [0, 24, 28, 40] {
            assert_eq!(height + label_offset(height), height - height / 2);
        }
    }

    #[test]
    fn the_close_button_straddles_the_top_right_corner_and_clicks_only_when_settled() {
        let preview = rect(100, 200, 400, 300);
        let button = close_button_rect(preview);
        assert_eq!(button.size, Size::from((OV_CLOSE_SIZE, OV_CLOSE_SIZE)));
        assert_eq!(button.loc.y, preview.loc.y - OV_CLOSE_OUTSET);
        let right = preview.loc.x + preview.size.w;
        assert_eq!(button.loc.x + button.size.w, right + OV_CLOSE_OUTSET);
        assert!(button.loc.x < right && button.loc.y + button.size.h > preview.loc.y);

        let (ui, id) = ui_with_one_window(0);
        let settled = ui.previews[0].rect_at(1.0);
        assert_eq!(ui.close_button_at(centre(close_button_rect(settled))), Some(id));
        assert_eq!(ui.close_button_at(centre(settled)), None);
        let (mut moving, _) = ui_with_one_window(400);
        moving.phase = Phase::opening(ago(40));
        let button = close_button_rect(moving.previews[0].rect_at(moving.progress()));
        assert_eq!(moving.close_button_at(centre(button)), None);
    }

    #[test]
    fn the_label_pill_makes_room_for_the_app_icon_and_ellipsises_inside_its_padding() {
        assert_eq!(label_icon_side(28), 18);
        assert_eq!(label_metrics(100, 28, false, 1000), LabelMetrics { width: 124, text: (12, 112) });
        assert_eq!(label_metrics(100, 28, true, 1000), LabelMetrics { width: 141, text: (29, 129) });
        assert_eq!(label_metrics(400, 28, true, 200), LabelMetrics { width: 200, text: (29, 188) });
        assert_eq!(label_metrics(400, 28, true, 20).text, (29, 29));
    }

    #[test]
    fn the_close_cross_and_shadow_follow_the_design() {
        assert!((OV_CLOSE_GLYPH - 6.0 * 11.0 / 12.0).abs() < 0.01);
        assert!((OV_CLOSE_GLYPH_STROKE - 1.8 * 11.0 / 12.0).abs() < 0.01);
        assert!((0.15..0.28).contains(&(OV_CLOSE_GLYPH / OV_CLOSE_SIZE as f32)));
        let layers = close_shadow_layers(glide_theme::Appearance::Dark);
        assert_eq!(layers.len(), 2, "--el-2 is a two-layer shadow");
        assert!(layers[0].blur < layers[1].blur);
        assert!(layers.iter().all(|layer| layer.color.0[3] > 0.0));
        let widest = layers.iter().map(|layer| layer.blur).fold(0.0_f32, f32::max);
        assert!(close_shadow_margin() > 0 && close_shadow_margin() as f32 >= widest);
    }

    #[test]
    fn close_hover_animates_and_relayout_keeps_only_what_still_exists() {
        let (mut ui, id) = ui_with_one_window(0);
        assert!(ui.set_close_hover(Some(id)), "a new target must animate");
        assert!(!ui.set_close_hover(Some(id)), "the same target is a no-op");
        assert!(ui.is_animating(), "the pop keeps frames coming");
        let key = RasterKey::new(&ui.theme, 1);
        ui.previews[0].close.insert(1, (0, key, MemoryRenderBuffer::default()));
        let fresh = WindowId::next();
        let area = rect(0, 0, 1920, 1080);
        ui.relayout(&[(id, rect(0, 0, 800, 600)), (fresh, rect(0, 0, 400, 300))], area);
        let preview = |ui: &OverviewUi, id| ui.previews.iter().find(|p| p.id == id).map(|p| p.close.len());
        assert_eq!(preview(&ui, id), Some(1), "a surviving window keeps its button");
        assert_eq!(preview(&ui, fresh), Some(0), "a new preview inherits nothing");
        ui.relayout(&[(fresh, rect(0, 0, 400, 300))], area);
        assert_eq!(ui.close_hover.target(), None, "a gone window's hover is cleared");
        assert!(ui.set_close_hover(Some(fresh)) && ui.set_close_hover(None));
    }

    #[test]
    fn relayout_with_identical_inputs_is_skipped_entirely() {
        let (mut ui, id) = ui_with_one_window(0);
        let windows = [(id, rect(0, 0, 800, 600))];
        let area = rect(0, 0, 1920, 1080);

        ui.previews[0].row = 99;

        ui.relayout(&windows, area);
        assert_eq!(ui.previews[0].row, 99, "identical inputs must leave the previews untouched");

        ui.relayout(&windows, rect(0, 0, 1280, 720));
        assert_eq!(ui.previews[0].row, 0);
    }

    #[test]
    fn relayout_recomputes_when_a_windows_origin_changes() {
        let (mut ui, id) = ui_with_one_window(0);
        ui.previews[0].row = 99;

        let resized = [(id, rect(0, 0, 400, 300))];
        ui.relayout(&resized, rect(0, 0, 1920, 1080));

        assert_eq!(ui.previews[0].row, 0, "a resized origin must relayout");
        assert_eq!(ui.previews[0].origin, rect(0, 0, 400, 300));
    }

    #[test]
    fn relayout_moves_hover_to_the_adjacent_neighbor_not_first() {
        let a = WindowId::next();
        let b = WindowId::next();
        let c = WindowId::next();
        let windows = [(a, rect(0, 0, 800, 600)), (b, rect(0, 0, 800, 600)), (c, rect(0, 0, 800, 600))];
        let area = rect(0, 0, 1920, 1080);
        let theme = theme_with_duration(0);

        let mut ui = OverviewUi::new(&windows, area, &theme, Some(b));

        let remaining = [(a, rect(0, 0, 800, 600)), (c, rect(0, 0, 800, 600))];
        ui.relayout(&remaining, area);

        assert_eq!(ui.hovered(), Some(c), "hover should move to the neighbor at the same index, not to first()");
    }

    #[test]
    fn navigate_without_hover_respects_direction() {
        let a = WindowId::next();
        let b = WindowId::next();
        let windows = [(a, rect(0, 0, 800, 600)), (b, rect(0, 0, 800, 600))];
        let area = rect(0, 0, 1920, 1080);
        let theme = theme_with_duration(0);

        let mut ui = OverviewUi::new(&windows, area, &theme, None);
        ui.set_hover(None);

        ui.navigate(Direction::Right);
        assert_eq!(ui.hovered(), Some(a), "Right should pick the first preview");

        ui.set_hover(None);
        ui.navigate(Direction::Left);
        assert_eq!(ui.hovered(), Some(b), "Left should pick the last preview");
    }

    #[test]
    fn the_hover_ring_grows_in_and_keeps_the_pump_alive_until_it_settles() {
        let (mut ui, id) = ui_with_one_window(0);
        assert!((ui.hover_progress() - 1.0).abs() < f64::EPSILON, "no pop on open");
        assert!(!ui.set_hover(Some(id)), "the same target is a no-op");
        assert_eq!(ui.tick(), Tick::Running, "settling the open phase");
        assert_eq!(ui.tick(), Tick::Idle);
        assert!(!ui.is_animating());
        ui.set_hover(None);
        assert!((ui.hover_progress() - 1.0).abs() < f64::EPSILON);
        assert!(ui.set_hover(Some(id)));
        assert!(ui.hover_progress() < 1.0, "a fresh hover is not settled");
        assert!(ui.is_animating());
        assert_eq!(ui.tick(), Tick::Running);
        std::thread::sleep(hover_anim() + Duration::from_millis(30));
        assert!((ui.hover_progress() - 1.0).abs() < f64::EPSILON);
        assert_eq!(ui.tick(), Tick::Idle);
    }

    #[test]
    fn a_flicked_commit_carries_the_finger_speed_into_the_animation() {
        let (mut ui, _id) = ui_with_one_window(400);
        ui.set_interactive_progress(0.4);
        ui.commit_to_opening(3.0);
        let Phase::Opening { from, .. } = ui.phase else {
            panic!("expected Phase::Opening");
        };
        let release = from.release.expect("a gesture commit uses a release curve");
        let speed = release.slope * 0.6 / release.duration.as_secs_f64();
        assert!((speed - 3.0).abs() < 0.05, "left the finger at {speed}");
    }

    #[test]
    fn closing_after_a_shortened_commit_runs_the_full_duration_again() {
        let (mut ui, _id) = ui_with_one_window(400);
        ui.set_interactive_progress(0.9);
        ui.commit_to_opening(0.0);
        assert!(ui.duration < ui.theme.duration());
        ui.phase = Phase::Open;
        ui.begin_close(None);
        assert_eq!(ui.duration, ui.theme.duration());
    }

    #[test]
    fn interactive_scrubbing_is_linear_on_both_curves_and_clamped() {
        let (mut ui, _id) = ui_with_one_window(400);
        for step in 0..=100 {
            let q = f64::from(step) / 100.0;
            ui.set_interactive_progress(q);
            assert!((ui.progress() - q).abs() < 1e-9);
            assert!((ui.backdrop_progress() - q).abs() < 1e-9);
            assert!(ui.is_interactive());
            assert!(!ui.previews[0].rect_at(q).size.is_empty());
        }

        ui.set_interactive_progress(-100.0);
        assert!(ui.progress().abs() < f64::EPSILON);
        assert!(ui.backdrop_progress().abs() < f64::EPSILON);

        ui.set_interactive_progress(100.0);
        assert!((ui.progress() - 1.0).abs() < f64::EPSILON);
        assert!((ui.backdrop_progress() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn empirical_challenge_commit_cancel_arbitration() {
        let (mut ui, _id) = ui_with_one_window(400);

        ui.set_interactive_progress(0.36);
        assert!((ui.progress() - 0.36).abs() < 1e-9);
        assert!((ui.backdrop_progress() - 0.36).abs() < 1e-9);
        ui.commit_to_opening(0.0);
        assert!(!ui.is_interactive());
        assert!(!ui.is_closing());
        match ui.phase {
            Phase::Opening { from, .. } => {
                assert!((from.primary - 0.36).abs() < 1e-9);
                assert!((from.backdrop - 0.36).abs() < 1e-9);
            }
            _ => panic!("Expected Phase::Opening"),
        }

        let (mut ui_cancel, _id) = ui_with_one_window(400);
        ui_cancel.set_interactive_progress(0.34);
        assert!((ui_cancel.progress() - 0.34).abs() < 1e-9);
        assert!((ui_cancel.backdrop_progress() - 0.34).abs() < 1e-9);
        ui_cancel.revert_to_closing(0.0);
        assert!(!ui_cancel.is_interactive());
        assert!(ui_cancel.is_closing());
        match ui_cancel.phase {
            Phase::Closing { from, select, .. } => {
                assert_eq!(select, None);
                assert!((from.primary - 0.34).abs() < 1e-9);
                assert!((from.backdrop - 0.34).abs() < 1e-9);
            }
            _ => panic!("Expected Phase::Closing"),
        }

        let (mut ui_full, _id) = ui_with_one_window(400);
        ui_full.set_interactive_progress(1.0);
        ui_full.commit_to_opening(0.0);
        assert_eq!(ui_full.phase, Phase::Open);

        let (mut ui_zero, _id) = ui_with_one_window(400);
        ui_zero.set_interactive_progress(0.0);
        ui_zero.revert_to_closing(0.0);
        assert!(ui_zero.is_closing());
    }
}

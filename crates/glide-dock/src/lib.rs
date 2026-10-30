pub mod config;
pub mod preview;

use std::{
    collections::{HashMap, hash_map::Entry},
    hash::Hash,
    time::{Duration, Instant},
};

use glide_theme::motion::{ease_io, ease_out, t1, t2, t3, t4};
use smithay::{
    backend::renderer::{
        Renderer,
        element::{
            Kind,
            memory::{MemoryRenderBuffer, MemoryRenderBufferRenderElement},
        },
    },
    utils::{Logical, Point, Rectangle, Scale, Size},
};

use glide_paint::TextAlign;
pub use glide_shell::raster::Backdrop;
use glide_shell::raster::{Canvas, ShellRenderer, rect};
use glide_theme::{Color, DockTheme};

use crate::config::{DockAutohide, DockConfig, DockItemConfig};

const AUTOHIDE_DELAY: Duration = Duration::from_millis(400);
pub const REVEAL_EDGE: f64 = 3.0;
const REVEAL_DWELL: Duration = Duration::from_millis(120);
pub const ATTENTION_REVEAL: Duration = Duration::from_millis(1500);
pub const LAUNCH_TIMEOUT: Duration = glide_shell::icon::LAUNCH_TIMEOUT;
pub const WINDOW_KEY_PREFIX: &str = "window-";

const LAUNCH_BOUNCE: f64 = 8.0;
const ATTENTION_BOUNCE: f64 = 6.0;
const ATTENTION_BOUNCES: u32 = 2;
const MENU_WINDOW_LIMIT: usize = 10;

const BADGE_HEIGHT: i32 = 16;
const BADGE_MIN_WIDTH: i32 = 16;
const BADGE_PAD_X: i32 = 5;
const BADGE_INSET: i32 = glide_theme::spacing::SP1;
const PROGRESS_HEIGHT: i32 = 4;
const PROGRESS_INSET: i32 = glide_theme::spacing::SP2;

const BADGE_STYLE: glide_theme::TextStyle = glide_theme::TextStyle {
    size_px: glide_theme::TextStyle::MICRO.size_px,
    line_height_px: glide_theme::TextStyle::MICRO.line_height_px,
    weight: glide_theme::TextStyle::MICRO.weight,
    ..glide_theme::TextStyle::NUM
};

fn bounce_height(elapsed: Duration, period: Duration, amplitude: f64, bounces: Option<u32>) -> f64 {
    if period.is_zero() {
        return 0.0;
    }
    let cycles = elapsed.as_secs_f64() / period.as_secs_f64();
    if bounces.is_some_and(|count| cycles >= f64::from(count)) {
        return 0.0;
    }
    let phase = cycles.fract();
    let rise = if phase < 0.5 {
        glide_theme::motion::E_OVER.ease_f64(phase * 2.0)
    } else {
        1.0 - glide_theme::motion::E_IO.ease_f64(phase * 2.0 - 1.0)
    };
    amplitude * rise
}

#[must_use]
pub fn step_index(current: Option<usize>, delta: i32, count: usize) -> usize {
    let count = i64::try_from(count).unwrap_or(i64::MAX).max(1);
    let next = match current.and_then(|index| i64::try_from(index).ok()) {
        Some(index) => (index + i64::from(delta)).rem_euclid(count),
        None if delta < 0 => count - 1,
        None => 0,
    };
    usize::try_from(next).unwrap_or(0)
}

fn dwell_elapsed(since: Instant, now: Instant) -> bool {
    now.saturating_duration_since(since) >= REVEAL_DWELL
}

fn badge_label(count: i64) -> String {
    if count > 99 { "99+".to_string() } else { count.to_string() }
}

fn app_key(app_id: &str) -> String {
    app_id.to_lowercase()
}

#[must_use]
pub fn is_pinnable(app_id: &str) -> bool {
    !app_id.trim().is_empty() && !app_id.starts_with(WINDOW_KEY_PREFIX)
}

const fn linear(t: f64) -> f64 {
    t
}

fn badge_duration(target: f64) -> Duration {
    if target > 0.5 { t2() } else { t1() }
}

const RUN_DOT: i32 = 4;
const RUN_BAR: i32 = 16;
const RUN_GAP: i32 = 3;
const RUN_THICKNESS: i32 = 4;
const RUN_BAND: i32 = RUN_GAP + RUN_THICKNESS;

const LAUNCHER_HOVER_LIFT: i32 = 0;
const LAUNCHER_HOVER_SCALE: f64 = 1.0;

const SEPARATOR_WIDTH: i32 = 1;
const SEPARATOR_MARGIN: i32 = glide_theme::spacing::SP1;
const SEPARATOR_HEIGHT: i32 = 28;

pub const TOOLTIP_DELAY: Duration = glide_theme::component::TOOLTIP_DELAY;
const TOOLTIP_HEIGHT: i32 = 26;
const TOOLTIP_PAD_X: i32 = glide_theme::spacing::SP3;
const TOOLTIP_GAP: i32 = glide_theme::spacing::SP2;
const TOOLTIP_RADIUS: f32 = glide_theme::shape::R2;
const DRAG_THRESHOLD: f64 = 6.0;
const DRAG_LIFT: i32 = 8;

const MENU_RISE: i32 = 6;
const MENU_SCALE_FROM: f64 = 0.94;
const PRESS_RADIUS: f32 = glide_theme::shape::R3;

fn reveal_progress(reveal: &glide_ui::Reveal, duration: Duration) -> f64 {
    if reveal.is_open() { reveal.progress(duration, ease_out) } else { reveal.progress(duration, ease_io) }
}

#[derive(Debug, Clone, Copy)]
struct Fade {
    tween: glide_ui::Tween,
    rising: bool,
}

impl Fade {
    fn hidden() -> Self {
        Self { tween: glide_ui::Tween::at(0.0), rising: false }
    }

    fn ease(self) -> fn(f64) -> f64 {
        if self.rising { ease_out } else { ease_io }
    }

    fn set(&mut self, to: f64, duration: Duration) -> bool {
        let from = self.value(duration);
        let changed = self.tween.retarget(to, duration, self.ease());
        if changed {
            self.rising = to > from;
        }
        changed
    }

    fn value(self, duration: Duration) -> f64 {
        self.tween.value(duration, self.ease())
    }

    fn target(self) -> f64 {
        self.tween.target()
    }

    fn is_animating(self, duration: Duration) -> bool {
        self.tween.is_animating(duration)
    }
}

#[derive(Debug, Clone, Copy)]
struct AutohideFade {
    tween: glide_ui::Tween,
}

impl AutohideFade {
    fn shown() -> Self {
        Self { tween: glide_ui::Tween::at(0.0) }
    }

    fn hiding(self) -> bool {
        self.tween.target() > 0.5
    }

    fn duration(self) -> Duration {
        if self.hiding() { t3() } else { t2() }
    }

    fn ease(self) -> fn(f64) -> f64 {
        if self.hiding() { ease_io } else { ease_out }
    }

    fn value(self) -> f64 {
        self.tween.value(self.duration(), self.ease())
    }

    fn set(&mut self, hidden: bool) -> bool {
        if hidden == self.hiding() {
            return false;
        }
        let from = self.value();
        self.tween.jump(from);
        self.tween.retarget(if hidden { 1.0 } else { 0.0 }, Duration::ZERO, ease_io)
    }

    fn is_animating(self) -> bool {
        self.tween.is_animating(self.duration())
    }
}

type Layers<K = String> = HashMap<K, glide_ui::Tween>;

fn layer_alpha<K: Eq + Hash + std::borrow::Borrow<Q>, Q: Eq + Hash + ?Sized>(
    layers: &Layers<K>, key: &Q, duration: Duration,
) -> f64 {
    layers.get(key).map_or(0.0, |tween| tween.value(duration, ease_io))
}

fn fade_all<K>(layers: &mut Layers<K>, duration: Duration) {
    for tween in layers.values_mut() {
        tween.retarget(0.0, duration, ease_io);
    }
}

fn focus_layer<K: Eq + Hash>(layers: &mut Layers<K>, target: Option<K>, duration: Duration) {
    for (key, tween) in layers.iter_mut() {
        if Some(key) != target.as_ref() {
            tween.retarget(0.0, duration, ease_io);
        }
    }
    if let Some(key) = target {
        layers.entry(key).or_insert_with(|| glide_ui::Tween::at(0.0)).retarget(1.0, duration, ease_io);
    }
}

fn prune_layers<K>(layers: &mut Layers<K>, duration: Duration) {
    layers.retain(|_, tween| tween.target() > 0.0 || tween.is_animating(duration));
}

fn any_animating<K>(layers: &Layers<K>, duration: Duration) -> bool {
    layers.values().any(|tween| tween.is_animating(duration))
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct TileSlide {
    from: f64,
    to: i32,
    started: Instant,
}

impl TileSlide {
    fn settled(x: i32, now: Instant) -> Self {
        Self { from: f64::from(x), to: x, started: now }
    }

    fn position(&self, now: Instant, duration: Duration) -> f64 {
        let to = f64::from(self.to);
        if duration.is_zero() {
            return to;
        }
        let elapsed = now.saturating_duration_since(self.started);
        if elapsed >= duration {
            return to;
        }
        let t = glide_theme::motion::E_OUT.ease_f64(elapsed.as_secs_f64() / duration.as_secs_f64());
        self.from + (to - self.from) * t
    }

    fn retarget(&mut self, to: i32, now: Instant, duration: Duration) {
        if to == self.to {
            return;
        }
        let from = self.position(now, duration);
        *self = Self { from, to, started: now };
    }

    fn glue(&mut self, x: f64, now: Instant) {
        let to = x.round() as i32;
        *self = Self::settled(to, now);
    }

    fn is_moving(&self, now: Instant, duration: Duration) -> bool {
        !duration.is_zero()
            && (self.from - f64::from(self.to)).abs() > f64::EPSILON
            && now.saturating_duration_since(self.started) < duration
    }
}

fn settled_stamp() -> Instant {
    let now = Instant::now();
    now.checked_sub(t3()).unwrap_or(now)
}

fn settle_slides(
    slides: &mut HashMap<String, TileSlide>, targets: &[(&str, i32)], glued: Option<(usize, f64)>, now: Instant,
    duration: Duration,
) -> Vec<f64> {
    slides.retain(|key, _| targets.iter().any(|(target, _)| target == key));
    targets
        .iter()
        .enumerate()
        .map(|(slot, (key, x))| {
            if !slides.contains_key(*key) {
                slides.insert((*key).to_string(), TileSlide::settled(*x, now));
            }
            let tile = slides.get_mut(*key).expect("inserted just above when missing");
            match glued {
                Some((glued_slot, pointer_x)) if glued_slot == slot => {
                    tile.glue(pointer_x, now);
                    pointer_x
                }
                _ => {
                    tile.retarget(*x, now, duration);
                    tile.position(now, duration)
                }
            }
        })
        .collect()
}

fn note_arrivals(
    arrivals: &mut HashMap<String, Instant>, slides: &HashMap<String, TileSlide>, targets: &[(&str, i32)], now: Instant,
) {
    arrivals.retain(|key, born| {
        now.saturating_duration_since(*born) < t2() && targets.iter().any(|(target, _)| target == key)
    });
    if slides.is_empty() {
        return;
    }
    for (key, _) in targets {
        if !slides.contains_key(*key) {
            arrivals.insert((*key).to_string(), now);
        }
    }
}

fn arrival(arrivals: &HashMap<String, Instant>, key: &str, now: Instant) -> f64 {
    let Some(born) = arrivals.get(key) else { return 1.0 };
    let duration = t2();
    if duration.is_zero() {
        return 1.0;
    }
    let t = now.saturating_duration_since(*born).as_secs_f64() / duration.as_secs_f64();
    glide_theme::motion::E_OUT.ease_f64(t.min(1.0))
}

const ARRIVAL_DROP: f64 = 8.0;

#[derive(Debug)]
struct Departure {
    tile: Buffers<i32>,
    x: i32,
    started: Instant,
}

fn departure(started: Instant, now: Instant) -> f64 {
    let duration = t2();
    if duration.is_zero() {
        return 1.0;
    }
    let t = now.saturating_duration_since(started).as_secs_f64() / duration.as_secs_f64();
    glide_theme::motion::E_IO.ease_f64(t.min(1.0))
}

fn push_departures<R: ShellRenderer>(
    dock: &mut Dock, frame: &mut Frame<'_, R>, first: Option<Rectangle<i32, Logical>>, opacity: f32,
) {
    let now = Instant::now();
    dock.departures.retain(|gone| now.saturating_duration_since(gone.started) < t2());
    let Some(first) = first else { return };
    let theme = frame.theme;
    for gone in &dock.departures {
        let progress = departure(gone.started, now);
        let rect = Rectangle::new(Point::from((gone.x, first.loc.y)), first.size);
        let mut at = HoverGeometry::for_tile(theme, rect, false, false, 0.0, 0.0);
        at.position.y += ARRIVAL_DROP * progress;
        frame.push_tile(gone.tile.get(&frame.output_scale), &at, opacity * (1.0 - progress) as f32);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct DockDrag {
    index: usize,
    grab_offset: f64,
    pressed_at: Point<f64, Logical>,
    pointer: Point<f64, Logical>,
    active: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PressOutcome {
    Nothing,
    Click(usize),
    Drop { index: usize, target: usize },
}

const DOCK_EDGE_MARGIN: i32 = glide_theme::spacing::SP2;
const MIN_ITEM_SIZE: i32 = 20;

fn dock_width(theme: &DockTheme, count: i32, separators: i32) -> i32 {
    if count == 0 {
        return 0;
    }
    theme.padding_x * 2
        + count * theme.item_size
        + (count - 1) * theme.item_gap
        + separators * (SEPARATOR_WIDTH + SEPARATOR_MARGIN * 2)
}

fn fitted_theme(base: &DockTheme, count: i32, separators: i32, available: i32) -> DockTheme {
    if count == 0 || dock_width(base, count, separators) <= available {
        return base.clone();
    }
    let fixed = dock_width(&DockTheme { item_size: 0, item_gap: 0, ..base.clone() }, count, separators);
    let scalable = count * base.item_size + (count - 1) * base.item_gap;
    let factor =
        if scalable > 0 { (f64::from((available - fixed).max(0)) / f64::from(scalable)).clamp(0.0, 1.0) } else { 1.0 };
    let scaled = |value: i32| (f64::from(value) * factor).floor() as i32;
    let mut fitted = base.clone();
    fitted.item_size = scaled(base.item_size).clamp(MIN_ITEM_SIZE.min(base.item_size), base.item_size);
    fitted.item_gap = scaled(base.item_gap).clamp(0, base.item_gap);
    while fitted.item_size > MIN_ITEM_SIZE && dock_width(&fitted, count, separators) > available {
        fitted.item_size -= 1;
    }
    if base.item_size > 0 {
        fitted.icon_size = base.icon_size * fitted.item_size / base.item_size;
    }
    fitted
}

fn raster_revision(theme: &DockTheme) -> u64 {
    glide_paint::raster_revision(theme.revision())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockAction {
    Activate,
    OpenLauncher,
}

#[derive(Debug, Clone)]
pub struct DockItem {
    pub name: String,
    pub command: String,
    pub app_id: String,
    pub separator_before: bool,
    pub action: DockAction,
}

impl DockItem {
    fn launcher() -> Self {
        Self {
            separator_before: true,
            action: DockAction::OpenLauncher,
            ..Self::app("", glide_shell::i18n::t(glide_shell::i18n::Key::Applications), "")
        }
    }

    fn app(app_id: &str, name: &str, command: &str) -> Self {
        Self {
            name: name.to_string(),
            command: command.to_string(),
            app_id: app_id.to_string(),
            separator_before: false,
            action: DockAction::Activate,
        }
    }

    fn from_config(config: &DockItemConfig) -> Self {
        Self {
            app_id: config.app_id.clone().unwrap_or_else(|| config.name.to_lowercase()),
            separator_before: config.separator_before,
            ..Self::app("", &config.name, &config.command)
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RunState {
    pub running: bool,
    pub focused: bool,
    pub windows: u8,
    pub signal: Signal,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Signal {
    #[default]
    Quiet,
    Launching,
    Urgent,
}

impl RunState {
    const fn shows_indicator(self) -> bool {
        self.running || matches!(self.signal, Signal::Launching)
    }

    const fn indicator(self) -> RunIndicator {
        if matches!(self.signal, Signal::Urgent) {
            RunIndicator::Attention
        } else if self.focused {
            RunIndicator::Focused
        } else {
            RunIndicator::Running
        }
    }

    fn dots(self) -> u8 {
        self.windows.clamp(1, RUN_DOTS_MAX)
    }
}

const RUN_DOTS_MAX: u8 = 3;

const fn dots_width(dots: u8) -> i32 {
    let dots = dots as i32;
    dots * RUN_DOT + (dots - 1) * RUN_GAP
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum RunIndicator {
    Running,
    Focused,
    Attention,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Badge {
    pub count: Option<i64>,
    pub progress: Option<f64>,
}

impl Badge {
    fn is_empty(self) -> bool {
        self.count.is_none() && self.progress.is_none()
    }
}

#[derive(Debug, Clone)]
struct Launch {
    app_id: String,
    name: String,
    command: String,
    started: Instant,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MenuWindow {
    pub id: u64,
    pub title: String,
    pub minimized: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MenuContext {
    pub windows: Vec<MenuWindow>,
    pub actions: Vec<String>,
    pub quick: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MenuEntry {
    pub item: DockMenuItem,
    pub label: String,
    pub danger: bool,
    pub muted: bool,
}

impl MenuEntry {
    fn fixed(item: DockMenuItem) -> Self {
        Self { item, label: item.label().to_string(), danger: item.is_danger(), muted: false }
    }
}

fn fit_menu_entries(mut entries: Vec<MenuEntry>, max_rows: usize) -> Vec<MenuEntry> {
    let droppable = |entry: &MenuEntry| {
        matches!(entry.item, DockMenuItem::Window(_) | DockMenuItem::Action(_) | DockMenuItem::Quick(_))
    };
    while entries.len() > max_rows {
        let last_window = entries.iter().rposition(|entry| matches!(entry.item, DockMenuItem::Window(_)));
        let Some(drop) = last_window.or_else(|| entries.iter().rposition(droppable)) else {
            break;
        };
        entries.remove(drop);
    }
    entries
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunningApp {
    pub app_id: String,
    pub name: String,
    pub focused: bool,
    pub windows: usize,
}

#[derive(Debug)]
pub struct Dock {
    theme: DockTheme,
    base_theme: DockTheme,
    pinned: Vec<DockItem>,
    items: Vec<DockItem>,
    run: Vec<RunState>,
    run_changed: Vec<Instant>,
    hover: glide_ui::HoverAnimation<usize>,
    keyboard: bool,
    pub icons: glide_shell::icon::IconLoader,
    icon_generation: u64,
    tiles: Vec<Buffers<i32>>,
    rasters: Rasters,
    shell_backdrop_key: (Option<BackdropKey>, u64),
    menu: Option<DockMenu>,
    menu_reveal: glide_ui::Reveal,
    menu_rows: Layers<usize>,
    drag: Option<DockDrag>,
    slides: HashMap<String, TileSlide>,
    arrivals: HashMap<String, Instant>,
    departures: Vec<Departure>,
    hover_since: Option<Instant>,
    hover_layers: Layers,
    press_layers: Layers,
    run_alphas: HashMap<String, Fade>,
    tooltip: Option<TooltipState>,
    tooltip_reveal: glide_ui::Reveal,
    raster_rev: u64,
    autohide: DockAutohide,
    hide: AutohideFade,
    hide_pending: Option<Instant>,
    edge_since: Option<Instant>,
    reveal_until: Option<Instant>,
    launching: HashMap<String, Launch>,
    urgent: HashMap<String, Instant>,
    badges: HashMap<String, Badge>,
    badge_fades: HashMap<String, (glide_ui::Tween, Badge)>,
    focus_wells: Layers,
    scroll: glide_shell::scroll::ScrollAccumulator,
    preview: Option<crate::preview::Preview>,
}

type Buffers<K> = HashMap<K, MemoryRenderBuffer>;

#[derive(Debug, Default)]
struct Rasters {
    shell: Buffers<(i32, i32, i32)>,
    shadow: Buffers<(i32, i32, i32)>,
    tile_layer: Buffers<i32>,
    drag_layer: Buffers<i32>,
    drag_shadow: Buffers<(i32, i32)>,
    runs: Buffers<(i32, RunIndicator, u8, i32)>,
    menu_row_layer: Buffers<i32>,
    press_layer: Buffers<i32>,
    tooltip: Buffers<(String, i32)>,
    tooltip_shadow: Buffers<(i32, i32)>,
    menu: Buffers<(String, Vec<MenuEntry>, i32)>,
    menu_shadow: Buffers<(i32, i32, i32)>,
    badge: Buffers<(String, i32)>,
    progress: Buffers<(u16, i32)>,
    focus_well: Buffers<i32>,
    launcher_hover: Buffers<i32>,
}

fn cached<K: Eq + Hash>(
    cache: &mut Buffers<K>, key: K, draw: impl FnOnce() -> Option<MemoryRenderBuffer>,
) -> Option<&MemoryRenderBuffer> {
    match cache.entry(key) {
        Entry::Occupied(entry) => Some(entry.into_mut()),
        Entry::Vacant(entry) => draw().map(|buffer| &*entry.insert(buffer)),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TooltipState {
    index: usize,
    label: String,
}

impl Dock {
    #[must_use]
    pub fn new(config: &DockConfig) -> Self {
        let pinned: Vec<DockItem> = config.items.iter().map(DockItem::from_config).collect();
        let mut items = pinned.clone();
        items.push(DockItem::launcher());
        let count = items.len();
        Self {
            theme: config.theme.clone(),
            base_theme: config.theme.clone(),
            pinned,
            items,
            run: vec![RunState::default(); count],
            run_changed: vec![settled_stamp(); count],
            hover: glide_ui::HoverAnimation::default(),
            keyboard: false,
            icons: glide_shell::icon::IconLoader::new(config.theme.icon_theme.clone()),
            icon_generation: 0,
            tiles: vec![HashMap::new(); count],
            rasters: Rasters::default(),
            shell_backdrop_key: (None, 0),
            menu: None,
            menu_reveal: glide_ui::Reveal::closed(),
            menu_rows: HashMap::new(),
            drag: None,
            slides: HashMap::new(),
            arrivals: HashMap::new(),
            departures: Vec::new(),
            hover_since: None,
            hover_layers: HashMap::new(),
            press_layers: HashMap::new(),
            run_alphas: HashMap::new(),
            tooltip: None,
            tooltip_reveal: glide_ui::Reveal::closed(),
            raster_rev: raster_revision(&config.theme),
            autohide: config.autohide,
            hide: AutohideFade::shown(),
            hide_pending: None,
            edge_since: None,
            reveal_until: None,
            launching: HashMap::new(),
            urgent: HashMap::new(),
            badges: HashMap::new(),
            badge_fades: HashMap::new(),
            focus_wells: HashMap::new(),
            scroll: glide_shell::scroll::ScrollAccumulator::default(),
            preview: None,
        }
    }

    pub fn reconfigure(&mut self, config: &DockConfig) {
        let launching = std::mem::take(&mut self.launching);
        let urgent = std::mem::take(&mut self.urgent);
        let badges = std::mem::take(&mut self.badges);
        *self = Self::new(config);
        self.launching = launching;
        self.urgent = urgent;
        self.badges = badges;
    }

    pub fn begin_launch(&mut self, app_id: &str, name: &str, command: &str) -> bool {
        if app_id.trim().is_empty() {
            return false;
        }
        self.launching.insert(
            app_key(app_id),
            Launch {
                app_id: app_id.to_string(),
                name: name.to_string(),
                command: command.to_string(),
                started: Instant::now(),
            },
        );
        true
    }

    pub fn set_urgent(&mut self, app_id: &str, urgent: bool) -> bool {
        let key = app_key(app_id);
        if key.trim().is_empty() {
            return false;
        }
        if urgent {
            if self.urgent.contains_key(&key) {
                return false;
            }
            self.urgent.insert(key, Instant::now());
            true
        } else {
            self.urgent.remove(&key).is_some()
        }
    }

    pub fn set_badge(&mut self, app_id: &str, badge: Badge) -> bool {
        let key = app_key(app_id);
        if badge.is_empty() {
            return self.badges.remove(&key).is_some();
        }
        if self.badges.get(&key) == Some(&badge) {
            return false;
        }
        self.badges.insert(key, badge);
        true
    }

    fn badge_for(&self, app_id: &str) -> Badge {
        self.badges.get(&app_key(app_id)).copied().unwrap_or_default()
    }

    pub fn reveal_for(&mut self, duration: Duration) {
        if !self.is_enabled() {
            return;
        }
        let until = Instant::now() + duration;
        if self.reveal_until.is_none_or(|current| current < until) {
            self.reveal_until = Some(until);
        }
    }

    fn bounce_lift(&self, index: usize, now: Instant) -> f64 {
        let Some(item) = self.items.get(index) else {
            return 0.0;
        };
        let key = app_key(&item.app_id);
        if let Some(launch) = self.launching.get(&key) {
            return bounce_height(now.saturating_duration_since(launch.started), t4(), LAUNCH_BOUNCE, None);
        }
        self.urgent.get(&key).map_or(0.0, |since| {
            bounce_height(now.saturating_duration_since(*since), t3(), ATTENTION_BOUNCE, Some(ATTENTION_BOUNCES))
        })
    }

    fn is_bouncing(&self, now: Instant) -> bool {
        let launch = !self.launching.is_empty() && !t4().is_zero();
        let period = t3();
        let attention = !period.is_zero()
            && self.urgent.values().any(|since| now.saturating_duration_since(*since) < period * ATTENTION_BOUNCES);
        launch || attention
    }

    #[must_use]
    pub const fn autohide(&self) -> DockAutohide {
        self.autohide
    }

    fn is_engaged(&self) -> bool {
        self.menu.is_some()
            || self.drag.is_some()
            || self.hover.target().is_some()
            || self.keyboard
            || self.preview_open()
    }

    pub fn begin_keyboard(&mut self) -> bool {
        if !self.is_enabled() || self.items.is_empty() {
            return false;
        }
        self.keyboard = true;
        let start = self.hover.target().filter(|index| *index < self.items.len()).unwrap_or(0);
        self.set_hover(Some(start));
        true
    }

    pub fn end_keyboard(&mut self) -> bool {
        if !self.keyboard {
            return false;
        }
        self.keyboard = false;
        self.close_menu();
        self.set_hover(None);
        true
    }

    #[must_use]
    pub const fn keyboard_active(&self) -> bool {
        self.keyboard
    }

    #[must_use]
    pub fn keyboard_index(&self) -> Option<usize> {
        self.hover.target().filter(|_| self.keyboard)
    }

    pub fn keyboard_step(&mut self, delta: i32) -> bool {
        let count = self.items.len();
        if !self.keyboard || count == 0 {
            return false;
        }
        let next = step_index(self.hover.target(), delta, count);
        self.set_hover(Some(next))
    }

    pub fn menu_step(&mut self, delta: i32) -> bool {
        let Some(menu) = self.open_menu_state() else {
            return false;
        };
        let count = menu.entries.len();
        if count == 0 {
            return false;
        }
        let next = step_index(menu.hovered, delta, count);
        self.set_menu_row(Some(next))
    }

    #[must_use]
    pub fn menu_selected(&self) -> Option<(usize, DockMenuItem)> {
        let menu = self.open_menu_state()?;
        Some((menu.index, menu.entries.get(menu.hovered?)?.item))
    }

    #[must_use]
    pub fn is_hidden(&self) -> bool {
        self.hide.hiding()
    }

    #[must_use]
    pub fn autohide_level(&self) -> f64 {
        self.hide.value().clamp(0.0, 1.0)
    }

    #[must_use]
    pub fn wants_hidden(&self, obstructed: bool, pointer_in_zone: bool) -> bool {
        if pointer_in_zone || self.is_engaged() {
            return false;
        }
        if self.reveal_until.is_some_and(|until| Instant::now() < until) {
            return false;
        }
        match self.autohide {
            DockAutohide::Never => false,
            DockAutohide::Smart => obstructed,
            DockAutohide::Always => true,
        }
    }

    pub fn update_autohide(&mut self, obstructed: bool, pointer_in_zone: bool) -> bool {
        let now = Instant::now();
        if self.reveal_until.is_some_and(|until| now >= until) {
            self.reveal_until = None;
        }
        let summoning = pointer_in_zone && self.is_hidden() && self.wants_hidden(obstructed, false);
        let pointer_in_zone = if summoning {
            let since = *self.edge_since.get_or_insert(now);
            dwell_elapsed(since, now)
        } else {
            self.edge_since = None;
            pointer_in_zone
        };
        let want = self.wants_hidden(obstructed, pointer_in_zone);
        if want == self.is_hidden() {
            self.hide_pending = None;
            return false;
        }
        if want {
            let since = *self.hide_pending.get_or_insert_with(Instant::now);
            if since.elapsed() < AUTOHIDE_DELAY {
                return false;
            }
        }
        self.hide_pending = None;
        self.hide.set(want)
    }

    #[must_use]
    pub fn in_reveal_zone(&self, area: Rectangle<i32, Logical>, pos: Point<f64, Logical>) -> bool {
        if !self.is_enabled() {
            return false;
        }
        let bottom = f64::from(area.loc.y + area.size.h);
        let left = f64::from(area.loc.x);
        let right = left + f64::from(area.size.w);
        let at_edge = pos.y >= bottom - REVEAL_EDGE && pos.x >= left && pos.x < right;
        if self.is_hidden() {
            return at_edge;
        }
        let mut shell = self.rect(area).to_f64();
        shell.loc.y -= f64::from(glide_theme::spacing::SP3);
        shell.size.h += f64::from(glide_theme::spacing::SP3 + self.theme.bottom_margin);
        at_edge || shell.contains(pos) || self.menu_rect().is_some_and(|rect| rect.to_f64().contains(pos))
    }

    fn retry_missing_icons(&mut self) {
        self.icons.expire_missing();
        let generation = self.icons.generation();
        if generation != self.icon_generation {
            self.icon_generation = generation;
            for tile in &mut self.tiles {
                tile.clear();
            }
        }
    }

    fn invalidate_stale_buffers(&mut self) {
        let current = raster_revision(&self.theme);
        if current == self.raster_rev {
            return;
        }
        self.raster_rev = current;
        self.rasters = Rasters::default();
        for tile in &mut self.tiles {
            tile.clear();
        }
        if let Some(preview) = self.preview.as_mut() {
            preview.clear_rasters();
        }
    }

    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.theme.enabled && !self.items.is_empty()
    }

    #[must_use]
    pub fn is_pinned(&self, index: usize) -> bool {
        index < self.pinned.len()
    }

    #[must_use]
    pub fn action(&self, index: usize) -> Option<DockAction> {
        self.items.get(index).map(|item| item.action)
    }

    #[must_use]
    pub fn icon_theme(&self) -> Option<String> {
        self.theme.icon_theme.clone()
    }

    #[must_use]
    pub fn bottom_margin(&self) -> i32 {
        self.theme.bottom_margin
    }

    #[must_use]
    pub fn items(&self) -> &[DockItem] {
        &self.items
    }

    fn expire_signals(&mut self, running: &[RunningApp]) {
        let now = Instant::now();
        self.launching.retain(|key, launch| {
            !running.iter().any(|app| app.app_id.eq_ignore_ascii_case(key))
                && now.saturating_duration_since(launch.started) < LAUNCH_TIMEOUT
        });
        self.urgent.retain(|key, _| running.iter().any(|app| app.app_id.eq_ignore_ascii_case(key) && !app.focused));
    }

    fn layout_items(&self, running: &[RunningApp]) -> Vec<DockItem> {
        let mut items = self.pinned.clone();
        let mut dynamic: Vec<DockItem> = running
            .iter()
            .filter(|app| !self.is_app_pinned(&app.app_id))
            .map(|app| DockItem::app(&app.app_id, &app.name, ""))
            .collect();
        let mut pending: Vec<&Launch> =
            self.launching.values().filter(|launch| !self.is_app_pinned(&launch.app_id)).collect();
        pending.sort_by_key(|launch| launch.started);
        dynamic.extend(pending.into_iter().map(|launch| DockItem::app(&launch.app_id, &launch.name, &launch.command)));
        for (slot, mut item) in dynamic.into_iter().enumerate() {
            item.separator_before = slot == 0 && !self.pinned.is_empty();
            items.push(item);
        }
        items.push(DockItem::launcher());
        items
    }

    fn run_states(&self, items: &[DockItem], running: &[RunningApp]) -> Vec<RunState> {
        items
            .iter()
            .map(|item| {
                let key = app_key(&item.app_id);
                let signal = if self.urgent.contains_key(&key) {
                    Signal::Urgent
                } else if item.action == DockAction::Activate && self.launching.contains_key(&key) {
                    Signal::Launching
                } else {
                    Signal::Quiet
                };
                let mut state = RunState { signal, ..RunState::default() };
                let mut windows = 0;
                for app in running {
                    if app.app_id.eq_ignore_ascii_case(&item.app_id) {
                        state.running = true;
                        state.focused |= app.focused;
                        windows += app.windows;
                    }
                }
                state.windows = u8::try_from(windows.min(usize::from(RUN_DOTS_MAX))).unwrap_or(RUN_DOTS_MAX);
                state
            })
            .collect()
    }

    pub fn set_running(&mut self, running: &[RunningApp]) -> bool {
        self.expire_signals(running);
        let items = self.layout_items(running);
        let run = self.run_states(&items, running);

        let layout_changed = items.len() != self.items.len()
            || items
                .iter()
                .zip(&self.items)
                .any(|(new, old)| new.app_id != old.app_id || new.separator_before != old.separator_before);
        let renamed: Vec<usize> = if layout_changed {
            Vec::new()
        } else {
            items
                .iter()
                .zip(&self.items)
                .enumerate()
                .filter(|(_, (new, old))| new.name != old.name)
                .map(|(index, _)| index)
                .collect()
        };

        if !layout_changed && run == self.run && renamed.is_empty() {
            return false;
        }
        for index in renamed {
            if let Some(tile) = self.tiles.get_mut(index) {
                tile.clear();
            }
            if self.tooltip.as_ref().is_some_and(|tooltip| tooltip.index == index) {
                self.tooltip = None;
            }
        }

        self.run_changed = self.run_change_stamps(&items, &run);

        if layout_changed {
            let now = Instant::now();
            for (old, tile) in self.items.iter().zip(self.tiles.drain(..)) {
                if items.iter().any(|new| new.app_id == old.app_id) {
                    continue;
                }
                if let Some(slide) = self.slides.get(&old.app_id) {
                    let x = slide.position(now, t2()).round() as i32;
                    self.departures.push(Departure { tile, x, started: now });
                }
            }
            self.tiles = vec![HashMap::new(); items.len()];
            self.rasters.shell.clear();
            self.rasters.shadow.clear();
            let moved = |index: usize| match (self.items.get(index), items.get(index)) {
                (Some(old), Some(new)) => old.app_id != new.app_id,
                _ => true,
            };
            let menu_moved = self.menu.as_ref().is_some_and(|menu| moved(menu.index));
            let hover_moved = self.hover.target().is_some_and(moved);
            if menu_moved {
                self.close_menu();
            }
            self.drag = None;
            fade_all(&mut self.press_layers, t1());
            if hover_moved {
                self.hover.retarget_without_restarting(None);
                fade_all(&mut self.hover_layers, t1());
            }
        }

        self.items = items;
        self.run = run;
        self.retarget_run_alphas();
        true
    }

    fn run_change_stamps(&self, items: &[DockItem], run: &[RunState]) -> Vec<Instant> {
        let now = Instant::now();
        let settled = settled_stamp();
        items
            .iter()
            .zip(run)
            .map(|(item, new_run)| {
                let old_index = self.items.iter().position(|old| old.app_id.eq_ignore_ascii_case(&item.app_id));
                match old_index {
                    Some(old_index) if self.run.get(old_index).is_some_and(|old| old.focused == new_run.focused) => {
                        self.run_changed.get(old_index).copied().unwrap_or(settled)
                    }
                    Some(_) => now,
                    None => settled,
                }
            })
            .collect()
    }

    fn retarget_run_alphas(&mut self) {
        let fade = t2();
        for (item, state) in self.items.iter().zip(&self.run) {
            let shown = state.shows_indicator();
            let target = if shown { 1.0 } else { 0.0 };
            match self.run_alphas.get_mut(&item.app_id) {
                Some(alpha) => {
                    alpha.set(target, fade);
                }
                None if shown => {
                    let mut alpha = Fade::hidden();
                    alpha.set(1.0, fade);
                    self.run_alphas.insert(item.app_id.clone(), alpha);
                }
                None => {}
            }
        }
        let focused: Option<String> = self
            .items
            .iter()
            .zip(&self.run)
            .find(|(item, state)| state.focused && item.action == DockAction::Activate)
            .map(|(item, _)| item.app_id.clone());
        focus_layer(&mut self.focus_wells, focused, t2());
    }

    fn badge_presence(&self, app_id: &str) -> Option<(Badge, f64, bool)> {
        let (tween, badge) = self.badge_fades.get(&app_key(app_id))?;
        let rising = tween.target() > 0.5;
        let progress = tween.value(badge_duration(tween.target()), linear).clamp(0.0, 1.0);
        (progress > 0.0).then_some((*badge, progress, rising))
    }

    fn sync_badge_fades(&mut self) {
        let keys: Vec<String> = self
            .items
            .iter()
            .filter(|item| item.action == DockAction::Activate)
            .map(|item| app_key(&item.app_id))
            .collect();
        for key in &keys {
            let badge = self.badges.get(key).copied().unwrap_or_default();
            let entry =
                self.badge_fades.entry(key.clone()).or_insert_with(|| (glide_ui::Tween::at(0.0), Badge::default()));
            let target = if badge.is_empty() { 0.0 } else { 1.0 };
            if !badge.is_empty() {
                entry.1 = badge;
            }
            if (entry.0.target() - target).abs() > f64::EPSILON {
                let current = entry.0.value(badge_duration(entry.0.target()), linear);
                entry.0.jump(current);
                entry.0.retarget(target, badge_duration(target), linear);
            }
        }
        self.badge_fades.retain(|key, (tween, _)| {
            keys.contains(key) && (tween.target() > 0.0 || tween.is_animating(badge_duration(tween.target())))
        });
    }

    fn badge_fades_animating(&self) -> bool {
        self.badge_fades.values().any(|(tween, _)| tween.is_animating(badge_duration(tween.target())))
    }

    #[must_use]
    pub fn pinned_items(&self) -> &[DockItem] {
        &self.pinned
    }

    pub fn toggle_pinned(&mut self, index: usize, identity: Option<(String, String)>) -> bool {
        let Some(item) = self.items.get(index).cloned() else {
            return false;
        };
        if item.action != DockAction::Activate || !is_pinnable(&item.app_id) {
            return false;
        }

        let position = self.pinned.iter().position(|pinned| pinned.app_id.eq_ignore_ascii_case(&item.app_id));

        if let Some(position) = position {
            self.pinned.remove(position);
        } else {
            let mut item = item;
            if let Some((name, command)) = identity {
                if !name.trim().is_empty() {
                    item.name = name;
                }
                if item.command.trim().is_empty() {
                    item.command = command;
                }
            }
            item.separator_before = false;
            self.pinned.push(item);
        }
        true
    }

    #[must_use]
    pub fn is_app_pinned(&self, app_id: &str) -> bool {
        self.pinned.iter().any(|item| item.app_id.eq_ignore_ascii_case(app_id))
    }

    pub fn pin_app(&mut self, app_id: &str, name: &str, command: &str) -> bool {
        if app_id.trim().is_empty() || self.is_app_pinned(app_id) {
            return false;
        }
        self.pinned.push(DockItem::app(app_id, name, command));
        true
    }

    pub fn unpin_app(&mut self, app_id: &str) -> bool {
        let before = self.pinned.len();
        self.pinned.retain(|item| !item.app_id.eq_ignore_ascii_case(app_id));
        self.pinned.len() != before
    }

    #[must_use]
    pub fn menu_entries(&self, index: usize, context: &MenuContext) -> Vec<MenuEntry> {
        let Some(item) = self.items.get(index) else {
            return Vec::new();
        };
        if item.action != DockAction::Activate {
            return Vec::new();
        }
        let running = self.run.get(index).is_some_and(|state| state.running);
        let launchable = !item.command.trim().is_empty();
        let mut entries: Vec<MenuEntry> = context
            .windows
            .iter()
            .take(MENU_WINDOW_LIMIT)
            .map(|window| MenuEntry {
                item: DockMenuItem::Window(window.id),
                label: if window.title.trim().is_empty() { item.name.clone() } else { window.title.clone() },
                danger: false,
                muted: window.minimized,
            })
            .collect();
        if running {
            entries.push(MenuEntry::fixed(DockMenuItem::ShowAllWindows));
        }
        if running && launchable {
            entries.push(MenuEntry::fixed(DockMenuItem::NewWindow));
        } else if !running && launchable {
            entries.push(MenuEntry::fixed(DockMenuItem::Open));
        }
        entries.extend(context.actions.iter().enumerate().filter(|(_, name)| !name.trim().is_empty()).map(
            |(slot, name)| MenuEntry {
                item: DockMenuItem::Action(slot),
                label: name.clone(),
                danger: false,
                muted: false,
            },
        ));
        entries.extend(context.quick.iter().enumerate().map(|(slot, name)| MenuEntry {
            item: DockMenuItem::Quick(slot),
            label: name.clone(),
            danger: false,
            muted: false,
        }));
        if self.is_pinned(index) {
            entries.push(MenuEntry::fixed(DockMenuItem::Unpin));
        } else if is_pinnable(&item.app_id) {
            entries.push(MenuEntry::fixed(DockMenuItem::Pin));
        }
        if running {
            entries.push(MenuEntry::fixed(DockMenuItem::Hide));
            entries.push(MenuEntry::fixed(DockMenuItem::Quit));
        }
        entries
    }

    pub fn open_menu(&mut self, index: usize, area: Rectangle<i32, Logical>, context: &MenuContext) -> bool {
        self.fit_to(area);
        self.hide_preview();
        let entries = self.menu_entries(index, context);
        if entries.is_empty() {
            return false;
        }
        let Some(tile) = self.item_rects(area).into_iter().nth(index) else {
            return false;
        };
        let entries = fit_menu_entries(entries, glide_shell::popup_menu::rows_fitting_above(tile, area));
        let rect = glide_shell::popup_menu::place_above(tile, area, entries.len());
        let heading = self.menu_title(index);
        if self.menu.as_ref().is_some_and(|menu| menu.index != index) {
            self.menu_reveal = glide_ui::Reveal::closed();
            self.menu_rows.clear();
        }
        self.menu = Some(DockMenu { index, entries, rect, hovered: None, title: heading });
        self.menu_reveal.open(t2());
        self.tooltip_reveal.close(t1());
        true
    }

    fn menu_title(&self, index: usize) -> String {
        self.items.get(index).map(|item| item.name.clone()).unwrap_or_default()
    }

    pub fn close_menu(&mut self) -> bool {
        if !self.menu_reveal.is_open() {
            return false;
        }
        self.menu_reveal.close(t2());
        if let Some(menu) = self.menu.as_mut() {
            menu.hovered = None;
        }
        fade_all(&mut self.menu_rows, t1());
        true
    }

    fn open_menu_state(&self) -> Option<&DockMenu> {
        if !self.menu_reveal.is_open() {
            return None;
        }
        self.menu.as_ref()
    }

    #[must_use]
    pub fn menu_index(&self) -> Option<usize> {
        self.open_menu_state().map(|menu| menu.index)
    }

    #[must_use]
    pub fn menu_rect(&self) -> Option<Rectangle<i32, Logical>> {
        self.open_menu_state().map(|menu| menu.rect)
    }

    #[must_use]
    pub fn menu_item_at(&self, pos: Point<f64, Logical>) -> Option<DockMenuItem> {
        let menu = self.open_menu_state()?;
        let row = menu.row_at(pos)?;
        menu.entries.get(row).map(|entry| entry.item)
    }

    pub fn set_menu_hover(&mut self, pos: Option<Point<f64, Logical>>) -> bool {
        let row = self.open_menu_state().and_then(|menu| pos.and_then(|pos| menu.row_at(pos)));
        self.set_menu_row(row)
    }

    fn set_menu_row(&mut self, hovered: Option<usize>) -> bool {
        if !self.menu_reveal.is_open() {
            return false;
        }
        let Some(menu) = self.menu.as_mut() else {
            return false;
        };
        if menu.hovered == hovered {
            return false;
        }
        let previous = std::mem::replace(&mut menu.hovered, hovered);
        let duration = t1();
        if let Some(row) = previous
            && let Some(tween) = self.menu_rows.get_mut(&row)
        {
            tween.retarget(0.0, duration, ease_io);
        }
        if let Some(row) = hovered {
            self.menu_rows.entry(row).or_insert_with(|| glide_ui::Tween::at(0.0)).retarget(1.0, duration, ease_io);
        }
        true
    }

    pub fn scroll_steps(&mut self, delta: glide_shell::scroll::ScrollDelta) -> i32 {
        self.scroll.feed(delta)
    }

    #[must_use]
    pub fn preview_candidate(&self) -> Option<usize> {
        if self.menu.is_some() || self.is_dragging() || self.keyboard || self.is_hidden() {
            return None;
        }
        if self.hover_since?.elapsed() < crate::preview::dwell() {
            return None;
        }
        self.hover
            .target()
            .filter(|index| self.items.get(*index).is_some_and(|item| item.action == DockAction::Activate))
    }

    pub fn show_preview(&mut self, index: usize, windows: Vec<crate::preview::PreviewWindow>) -> bool {
        let Some(app_id) = self.items.get(index).map(|item| item.app_id.clone()) else {
            return false;
        };
        match self.preview.as_mut() {
            Some(preview) if preview.app_id == app_id => {
                let reopened = !preview.is_open();
                preview.index = index;
                preview.reopen();
                let changed = preview.set_windows(windows);
                reopened || changed
            }
            _ => {
                self.preview = Some(crate::preview::Preview::new(index, app_id, windows));
                self.tooltip_reveal.close(t1());
                true
            }
        }
    }

    pub fn hide_preview(&mut self) -> bool {
        match self.preview.as_mut() {
            Some(preview) if preview.is_open() => {
                preview.close();
                true
            }
            _ => false,
        }
    }

    #[must_use]
    pub fn preview_open(&self) -> bool {
        self.preview.as_ref().is_some_and(crate::preview::Preview::is_open)
    }

    #[must_use]
    pub fn preview_layout(&self, area: Rectangle<i32, Logical>) -> Option<crate::preview::PreviewLayout> {
        let preview = self.preview.as_ref().filter(|preview| preview.is_visible())?;
        let tile = *self.item_rects(area).get(preview.index)?;
        Some(crate::preview::layout(preview.windows(), tile, self.theme.hover_lift, area))
    }

    #[must_use]
    pub fn preview_index(&self) -> Option<usize> {
        self.preview.as_ref().filter(|preview| preview.is_open()).map(|preview| preview.index)
    }

    pub fn set_preview_hover(&mut self, hit: Option<crate::preview::PreviewHit>) -> bool {
        self.preview.as_mut().is_some_and(|preview| preview.set_hover(hit))
    }

    pub fn preview_parts(
        &mut self, area: Rectangle<i32, Logical>,
    ) -> Option<(&mut crate::preview::Preview, &DockTheme, crate::preview::PreviewLayout, u64)> {
        let layout = self.preview_layout(area)?;
        let preview = self.preview.as_mut()?;
        Some((preview, &self.theme, layout, self.raster_rev))
    }

    pub fn set_hover(&mut self, index: Option<usize>) -> bool {
        let changed = self.hover.set_with(index, t2());
        if changed {
            self.hover_since = index.map(|_| Instant::now());
            let target = index.and_then(|index| self.items.get(index)).map(|item| item.app_id.clone());
            focus_layer(&mut self.hover_layers, target, t1());
            if self.tooltip.as_ref().is_some_and(|tooltip| Some(tooltip.index) != index) {
                self.tooltip_reveal.close(t1());
            }
        }
        changed
    }

    fn run_alpha(&self, app_id: &str) -> f64 {
        self.run_alphas.get(app_id).map_or(0.0, |fade| fade.value(t2()))
    }

    fn tooltip_label(&self, index: usize) -> String {
        match self.items.get(index) {
            Some(item) if item.action == DockAction::OpenLauncher => {
                glide_shell::i18n::t(glide_shell::i18n::Key::Applications).to_string()
            }
            Some(_) => self.menu_title(index),
            None => String::new(),
        }
    }

    fn sync_tooltip(&mut self) {
        let duration = t1();
        let Some(index) = self.tooltip_index() else {
            self.tooltip_reveal.close(duration);
            return;
        };
        if self.tooltip.as_ref().is_none_or(|tooltip| tooltip.index != index) {
            let label = self.tooltip_label(index);
            if label.trim().is_empty() {
                self.tooltip_reveal.close(duration);
                return;
            }
            self.tooltip = Some(TooltipState { index, label });
        }
        self.tooltip_reveal.open(duration);
    }

    fn settle(&mut self) {
        self.menu_reveal.settle(t2());
        if !self.menu_reveal.is_open() && !self.menu_reveal.is_visible(t2()) {
            self.menu = None;
            self.menu_rows.clear();
        }
        self.tooltip_reveal.settle(t1());
        if !self.tooltip_reveal.is_open() && !self.tooltip_reveal.is_visible(t1()) {
            self.tooltip = None;
        }
        if let Some(preview) = self.preview.as_mut() {
            preview.settle();
        }
        if self.preview.as_ref().is_some_and(|preview| !preview.is_visible()) {
            self.preview = None;
        }
        prune_layers(&mut self.hover_layers, t1());
        prune_layers(&mut self.press_layers, t1());
        prune_layers(&mut self.menu_rows, t1());
        prune_layers(&mut self.focus_wells, t2());
        self.run_alphas.retain(|_, alpha| alpha.target() > 0.0 || alpha.is_animating(t2()));
        self.sync_badge_fades();
        if self.rasters.badge.len() > 32 {
            self.rasters.badge.clear();
        }
        if self.rasters.progress.len() > 32 {
            self.rasters.progress.clear();
        }
    }

    fn tooltip_index(&self) -> Option<usize> {
        if self.menu.is_some() || self.is_dragging() || self.preview_open() {
            return None;
        }
        let since = self.hover_since?;
        if since.elapsed() < TOOLTIP_DELAY {
            return None;
        }
        self.hover.target().filter(|index| self.items.get(*index).is_some())
    }

    #[must_use]
    pub fn is_animating(&self) -> bool {
        if !self.is_enabled() {
            return false;
        }
        let now = Instant::now();
        let run = t3();
        let tooltip_pending = self.menu.is_none()
            && self.drag.is_none()
            && self.hover_since.is_some_and(|since| since.elapsed() < TOOLTIP_DELAY);
        let autohide = self.hide_pending.is_some()
            || self.hide.is_animating()
            || self.reveal_until.is_some_and(|until| now < until)
            || (self.edge_since.is_some() && self.is_hidden());
        let preview = self.preview.as_ref().is_some_and(crate::preview::Preview::is_animating);
        autohide
            || preview
            || any_animating(&self.focus_wells, t2())
            || self.is_bouncing(now)
            || self.badge_fades_animating()
            || self.hover.is_animating(t2())
            || tooltip_pending
            || (!run.is_zero() && self.run_changed.iter().any(|stamp| stamp.elapsed() < run))
            || self.slides.values().any(|tile| tile.is_moving(now, t2()))
            || self.arrivals.values().any(|born| now.saturating_duration_since(*born) < t2())
            || self.departures.iter().any(|gone| now.saturating_duration_since(gone.started) < t2())
            || self.menu_reveal.is_animating(t2())
            || self.tooltip_reveal.is_animating(t1())
            || any_animating(&self.hover_layers, t1())
            || any_animating(&self.press_layers, t1())
            || any_animating(&self.menu_rows, t1())
            || self.run_alphas.values().any(|alpha| alpha.is_animating(t2()))
    }

    fn slid_rects(
        &mut self, order: &[usize], item_rects: &[Rectangle<i32, Logical>],
        dragged: Option<(usize, Point<f64, Logical>)>,
    ) -> Vec<Rectangle<i32, Logical>> {
        let targets: Vec<(&str, i32)> = order
            .iter()
            .zip(item_rects)
            .map(|(index, rect)| (self.items[*index].app_id.as_str(), rect.loc.x))
            .collect();
        let glued = dragged.and_then(|(index, position)| {
            order.iter().position(|candidate| *candidate == index).map(|slot| (slot, position.x))
        });
        let now = Instant::now();
        note_arrivals(&mut self.arrivals, &self.slides, &targets, now);
        settle_slides(&mut self.slides, &targets, glued, now, t2())
            .into_iter()
            .zip(item_rects)
            .map(|(x, rect)| {
                let mut rect = *rect;
                {
                    rect.loc.x = x.round() as i32;
                }
                rect
            })
            .collect()
    }

    #[cfg(test)]
    fn settle_slides(
        &mut self, targets: &[(&str, i32)], glued: Option<(usize, f64)>, now: Instant, duration: Duration,
    ) -> Vec<f64> {
        settle_slides(&mut self.slides, targets, glued, now, duration)
    }

    pub fn begin_press(&mut self, index: usize, pos: Point<f64, Logical>, area: Rectangle<i32, Logical>) {
        let Some(item) = self.items.get(index) else {
            return;
        };
        if item.action != DockAction::Activate {
            return;
        }
        let app_id = item.app_id.clone();
        let Some(tile) = self.item_rects(area).into_iter().nth(index) else {
            return;
        };
        self.drag = Some(DockDrag {
            index,
            grab_offset: pos.x - f64::from(tile.loc.x),
            pressed_at: pos,
            pointer: pos,
            active: false,
        });
        self.press_layers.entry(app_id).or_insert_with(|| glide_ui::Tween::at(0.0)).jump(1.0);
    }

    pub fn drag_motion(&mut self, pos: Point<f64, Logical>) -> bool {
        let Some(drag) = self.drag.as_mut() else {
            return false;
        };
        drag.pointer = pos;
        if !drag.active {
            let dx = pos.x - drag.pressed_at.x;
            let dy = pos.y - drag.pressed_at.y;
            if dx.hypot(dy) < DRAG_THRESHOLD {
                return false;
            }
            drag.active = true;
            self.close_menu();
            self.hover.retarget_without_restarting(None);
            self.hover_since = None;
            fade_all(&mut self.hover_layers, t1());
            fade_all(&mut self.press_layers, t1());
            self.tooltip_reveal.close(t1());
            self.hide_preview();
        }
        true
    }

    #[must_use]
    pub fn is_dragging(&self) -> bool {
        self.drag.is_some_and(|drag| drag.active)
    }

    pub fn end_press(&mut self, area: Rectangle<i32, Logical>, pos: Point<f64, Logical>) -> PressOutcome {
        let Some(drag) = self.drag.take() else {
            return PressOutcome::Nothing;
        };
        fade_all(&mut self.press_layers, t1());
        if drag.active {
            let target = self.drag_target(&drag, area);
            return PressOutcome::Drop { index: drag.index, target };
        }
        if self.item_at(area, pos) == Some(drag.index) {
            PressOutcome::Click(drag.index)
        } else {
            PressOutcome::Nothing
        }
    }

    fn drag_target(&self, drag: &DockDrag, area: Rectangle<i32, Logical>) -> usize {
        let tile_centre = drag.pointer.x - drag.grab_offset + f64::from(self.theme.item_size) / 2.0;
        self.item_rects(area)
            .iter()
            .enumerate()
            .filter(|(index, _)| self.is_pinned(*index) && *index != drag.index)
            .filter(|(_, rect)| f64::from(rect.loc.x) + f64::from(rect.size.w) / 2.0 <= tile_centre)
            .count()
    }

    pub fn move_pinned(&mut self, from: usize, to: usize) -> bool {
        if from >= self.pinned.len() {
            return false;
        }
        let to = to.min(self.pinned.len() - 1);
        if from == to {
            return false;
        }
        let item = self.pinned.remove(from);
        self.pinned.insert(to, item);
        true
    }

    fn display_order(&self, area: Rectangle<i32, Logical>) -> Option<(Vec<DockItem>, Vec<usize>)> {
        let drag = self.drag.filter(|drag| drag.active)?;
        self.items.get(drag.index)?;

        let mut order: Vec<usize> = (0..self.items.len()).filter(|index| *index != drag.index).collect();
        let pinned_slots = order.iter().filter(|index| self.is_pinned(**index)).count();
        let target = self.drag_target(&drag, area).min(pinned_slots);
        order.insert(target, drag.index);

        let mut items: Vec<DockItem> = order.iter().map(|index| self.items[*index].clone()).collect();
        let pinned_count = pinned_slots + 1;
        for (slot, item) in items.iter_mut().enumerate() {
            if slot < pinned_count {
                item.separator_before = false;
            } else if item.action == DockAction::Activate {
                item.separator_before = slot == pinned_count;
            }
        }
        Some((items, order))
    }

    fn dragged_position(&self, area: Rectangle<i32, Logical>) -> Option<(usize, Point<f64, Logical>)> {
        let drag = self.drag.filter(|drag| drag.active)?;
        let shell = self.rect(area);
        let x = (drag.pointer.x - drag.grab_offset)
            .clamp(f64::from(shell.loc.x), f64::from(shell.loc.x + shell.size.w - self.theme.item_size));
        let y = f64::from(shell.loc.y + self.theme.padding_y - DRAG_LIFT);
        Some((drag.index, Point::from((x, y))))
    }

    fn run_width(&self, index: usize) -> i32 {
        let state = self.run.get(index).copied().unwrap_or_default();
        let dots = dots_width(state.dots());
        let (from, to) = if state.focused { (dots, RUN_BAR) } else { (RUN_BAR, dots) };
        let duration = t3();
        let elapsed = self.run_changed.get(index).map_or(duration, Instant::elapsed);
        if duration.is_zero() || elapsed >= duration {
            return to;
        }
        let t = glide_theme::motion::E_OUT.ease_f64(elapsed.as_secs_f64() / duration.as_secs_f64());
        (f64::from(from) + f64::from(to - from) * t).round() as i32
    }

    fn hover_level(&self, index: usize) -> f64 {
        glide_theme::motion::E_OVER.ease_f64(self.hover.level(index, t2()))
    }

    fn counts(&self) -> (i32, i32) {
        let count = i32::try_from(self.items.len()).unwrap_or(0);
        let separators =
            i32::try_from(self.items.iter().skip(1).filter(|item| item.separator_before).count()).unwrap_or(0);
        (count, separators)
    }

    #[must_use]
    pub fn size(&self) -> Size<i32, Logical> {
        let theme = &self.theme;
        let (count, separators) = self.counts();
        if count == 0 {
            return Size::from((0, 0));
        }
        Size::from((dock_width(theme, count, separators), theme.padding_y * 2 + theme.item_size + RUN_BAND))
    }

    pub fn fit_to(&mut self, area: Rectangle<i32, Logical>) {
        let (count, separators) = self.counts();
        let available = area.size.w - DOCK_EDGE_MARGIN * 2;
        self.theme = fitted_theme(&self.base_theme, count, separators, available);
    }

    #[must_use]
    pub fn rect(&self, area: Rectangle<i32, Logical>) -> Rectangle<i32, Logical> {
        let size = self.size();
        let x = area.loc.x + ((area.size.w - size.w) / 2).max(0);
        let y = area.loc.y + area.size.h - size.h - self.theme.bottom_margin;
        Rectangle::new(Point::from((x, y)), size)
    }

    #[must_use]
    pub fn item_rects(&self, area: Rectangle<i32, Logical>) -> Vec<Rectangle<i32, Logical>> {
        self.rects_for(&self.items, area)
    }

    fn rects_for(&self, items: &[DockItem], area: Rectangle<i32, Logical>) -> Vec<Rectangle<i32, Logical>> {
        let shell = self.rect(area);
        let theme = &self.theme;
        let mut x = shell.loc.x + theme.padding_x;
        let y = shell.loc.y + theme.padding_y;

        items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                if index > 0 {
                    x += theme.item_gap;
                    if item.separator_before {
                        x += SEPARATOR_WIDTH + SEPARATOR_MARGIN * 2;
                    }
                }
                let rect = Rectangle::new(Point::from((x, y)), Size::from((theme.item_size, theme.item_size)));
                x += theme.item_size;
                rect
            })
            .collect()
    }

    #[must_use]
    pub fn tile_rect_for_app(&self, area: Rectangle<i32, Logical>, app_id: &str) -> Option<Rectangle<i32, Logical>> {
        let index = self.items.iter().position(|item| item.app_id.eq_ignore_ascii_case(app_id))?;
        self.item_rects(area).into_iter().nth(index)
    }

    #[must_use]
    pub fn item_at(&self, area: Rectangle<i32, Logical>, pos: Point<f64, Logical>) -> Option<usize> {
        self.item_rects(area).into_iter().position(|rect| rect.to_f64().contains(pos))
    }

    #[must_use]
    pub fn contains(&self, area: Rectangle<i32, Logical>, pos: Point<f64, Logical>) -> bool {
        if !self.is_enabled() || self.is_hidden() {
            return false;
        }
        if self.rect(area).to_f64().contains(pos) {
            return true;
        }
        self.menu_rect().is_some_and(|rect| rect.to_f64().contains(pos))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DockMenuItem {
    Window(u64),
    ShowAllWindows,
    Open,
    NewWindow,
    Action(usize),
    Quick(usize),
    Pin,
    Unpin,
    Hide,
    Quit,
}

impl DockMenuItem {
    #[must_use]
    pub fn label(self) -> &'static str {
        use glide_shell::i18n::{Key, t};
        match self {
            Self::Window(_) | Self::Action(_) | Self::Quick(_) => "",
            Self::ShowAllWindows => t(Key::DockShowAllWindows),
            Self::Open => t(Key::Open),
            Self::NewWindow => t(Key::NewWindow),
            Self::Pin => t(Key::PinToDock),
            Self::Unpin => t(Key::UnpinFromDock),
            Self::Hide => t(Key::DockHide),
            Self::Quit => t(Key::DockQuit),
        }
    }

    #[must_use]
    pub const fn is_danger(self) -> bool {
        matches!(self, Self::Quit)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DockMenu {
    index: usize,
    entries: Vec<MenuEntry>,
    rect: Rectangle<i32, Logical>,
    hovered: Option<usize>,
    title: String,
}

impl DockMenu {
    fn row_at(&self, pos: Point<f64, Logical>) -> Option<usize> {
        glide_shell::popup_menu::row_at(self.rect, self.entries.len(), pos)
    }

    fn rows(&self) -> Vec<glide_shell::popup_menu::MenuRow> {
        self.entries
            .iter()
            .map(|entry| glide_shell::popup_menu::MenuRow {
                label: entry.label.clone().into(),
                danger: entry.danger,
                muted: entry.muted,
            })
            .collect()
    }
}

#[allow(clippy::too_many_arguments)]
pub fn render<R>(
    dock: &mut Dock, renderer: &mut R, painter: &glide_paint::Painter, area: Rectangle<i32, Logical>,
    scale: Scale<f64>, output_scale: i32, hidden: f64, backdrop: Option<&Backdrop<'_>>,
) -> Vec<MemoryRenderBufferRenderElement<R>>
where
    R: ShellRenderer,
{
    let hidden = hidden.clamp(0.0, 1.0);
    dock.fit_to(area);
    if !dock.is_enabled() || hidden >= 1.0 {
        return Vec::new();
    }

    dock.invalidate_stale_buffers();
    dock.retry_missing_icons();
    dock.settle();
    dock.sync_tooltip();

    let shell = dock.rect(area);
    let slide = (f64::from(shell.size.h + dock.theme.bottom_margin) * hidden).round() as i32;
    let opacity = (1.0 - hidden) as f32;
    let (display_items, order) =
        dock.display_order(area).unwrap_or_else(|| (dock.items.clone(), (0..dock.items.len()).collect()));
    let item_rects: Vec<Rectangle<i32, Logical>> = dock
        .rects_for(&display_items, area)
        .into_iter()
        .map(|mut rect| {
            rect.loc.y += slide;
            rect
        })
        .collect();
    let dragged = dock.dragged_position(area);
    let drawn_rects = dock.slid_rects(&order, &item_rects, dragged);
    let theme = dock.theme.clone();
    let mut frame = Frame { renderer, painter, theme: &theme, scale, output_scale, elements: Vec::new() };
    if hidden <= 0.0 {
        push_menu(dock, &mut frame);
        push_tooltip(dock, &mut frame, &order, &drawn_rects, area);
    }
    push_tiles(dock, &mut frame, &order, &drawn_rects, dragged, opacity);
    push_departures(dock, &mut frame, drawn_rects.first().copied(), opacity);
    push_shell(dock, &mut frame, &display_items, &item_rects, (shell, slide), opacity, backdrop);
    frame.elements
}

struct Frame<'a, R: Renderer> {
    renderer: &'a mut R,
    painter: &'a glide_paint::Painter,
    theme: &'a DockTheme,
    scale: Scale<f64>,
    output_scale: i32,
    elements: Vec<MemoryRenderBufferRenderElement<R>>,
}

impl<R: ShellRenderer> Frame<'_, R> {
    fn push(
        &mut self, buffer: Option<&MemoryRenderBuffer>, pos: Point<f64, Logical>, alpha: f32,
        scaled: Option<(Size<i32, Logical>, Size<i32, Logical>)>,
    ) {
        let Some(buffer) = buffer else {
            return;
        };
        if let Ok(element) = MemoryRenderBufferRenderElement::from_buffer(
            self.renderer,
            pos.to_physical(self.scale),
            buffer,
            Some(alpha),
            scaled.map(|(src, _)| Rectangle::from_size(src.to_f64())),
            scaled.map(|(_, size)| size),
            Kind::Unspecified,
        ) {
            self.elements.push(element);
        }
    }

    fn push_tile(&mut self, buffer: Option<&MemoryRenderBuffer>, at: &HoverGeometry, alpha: f32) {
        let item = self.theme.item_size;
        let scaled = (Size::from((item, item)), Size::from((at.side, at.side)));
        self.push(buffer, at.position, alpha, Some(scaled));
    }

    fn push_placed(
        &mut self, buffer: Option<&MemoryRenderBuffer>, placement: &MenuPlacement, rect: Rectangle<i32, Logical>,
        alpha: f32,
    ) {
        let (pos, size) = placement.map(rect);
        self.push(buffer, pos, alpha, Some((rect.size, size)));
    }
}

fn push_tiles<R: ShellRenderer>(
    dock: &mut Dock, frame: &mut Frame<'_, R>, order: &[usize], drawn_rects: &[Rectangle<i32, Logical>],
    dragged: Option<(usize, Point<f64, Logical>)>, opacity: f32,
) {
    let theme = frame.theme;
    let now = Instant::now();
    for (&index, rect) in order.iter().zip(drawn_rects) {
        let app_id = &dock.items[index].app_id;
        let arrive = arrival(&dock.arrivals, app_id, now);
        let opacity = opacity * arrive as f32;
        let press = layer_alpha(&dock.press_layers, app_id, t1()).clamp(0.0, 1.0);
        let hover_alpha = layer_alpha(&dock.hover_layers, app_id, t1()) * (1.0 - press);
        let run = dock.run_alpha(app_id);
        let focus = layer_alpha(&dock.focus_wells, app_id, t2());
        let lift = dock.bounce_lift(index, now);
        let hover_t = dock.hover_level(index);
        let is_hovered = hover_t > 0.0;
        let is_launcher = dock.items[index].action == DockAction::OpenLauncher;
        let dragging = dragged.is_some_and(|(drag_index, _)| drag_index == index);
        let (mut hover, mut well) = match dragged {
            Some((drag_index, position)) if drag_index == index => {
                (HoverGeometry::lifted(theme, position), HoverGeometry::lifted(theme, position))
            }
            _ => (
                HoverGeometry::for_tile(theme, *rect, is_hovered, is_launcher, hover_t, hover_alpha),
                HoverGeometry::for_tile(theme, *rect, false, is_launcher, 0.0, hover_alpha),
            ),
        };
        hover.position.y -= lift - ARRIVAL_DROP * (1.0 - arrive);
        well.position.y += ARRIVAL_DROP * (1.0 - arrive);
        push_badge(dock, frame, index, (&hover, &well), opacity);
        push_tile_icon(dock, frame, index, &hover, opacity);
        let scale = frame.output_scale;
        let tile = (theme.item_size, theme.item_size);
        let layer = well.layer_alpha;
        if is_launcher && layer > 0.0 {
            let icons = &mut dock.icons;
            let buffer = cached(&mut dock.rasters.launcher_hover, scale, || {
                let icon = launcher_icon(icons, theme, scale, true);
                draw_launcher_tile(theme, icon.as_deref(), scale, true)
            });
            frame.push_tile(buffer, &hover, opacity * layer.clamp(0.0, 1.0));
        } else if dragging {
            let buffer = cached(&mut dock.rasters.drag_layer, scale, || {
                draw_panel(tile, drag_layer_color(theme), theme.item_radius, scale)
            });
            frame.push_tile(buffer, &well, opacity);
        } else if !is_launcher && layer > 0.0 {
            let buffer = cached(&mut dock.rasters.tile_layer, scale, || {
                draw_panel(tile, theme.hover_layer, theme.item_radius, scale)
            });
            frame.push_tile(buffer, &well, opacity * layer);
        }
        if press > 0.0 {
            let buffer = cached(&mut dock.rasters.press_layer, scale, || {
                draw_panel(tile, press_layer_color(theme), PRESS_RADIUS, scale)
            });
            frame.push_tile(buffer, &well, opacity * unit_alpha(press));
        }
        if focus > 0.0 {
            let buffer = cached(&mut dock.rasters.focus_well, scale, || {
                draw_panel(tile, theme.well_focused, theme.item_radius, scale)
            });
            frame.push_tile(buffer, &well, opacity * unit_alpha(focus));
        }
        push_run_indicator(dock, frame, index, *rect, run, opacity);
        if dragging {
            let side = hover.side;
            let shadow = cached(&mut dock.rasters.drag_shadow, (side, scale), || {
                glide_shell::popup_menu::draw_elevation(
                    Size::from((side, side)),
                    theme.item_radius,
                    scale_or_one(scale),
                    glide_theme::elevation::Level::El2,
                    theme.appearance,
                )
            });
            let margin = f64::from(glide_shell::popup_menu::shadow_margin());
            let at = Point::from((hover.position.x - margin, hover.position.y - margin));
            frame.push(shadow, at, opacity, None);
        }
    }
}

fn drag_layer_color(theme: &DockTheme) -> Color {
    let hover = theme.hover_layer;
    let scale = glide_theme::motion::ST_DRAG / glide_theme::motion::ST_HOVER;
    hover.with_alpha((hover.0[3] * scale).min(1.0))
}

fn unit_alpha(value: f64) -> f32 {
    value.clamp(0.0, 1.0) as f32
}

fn launcher_icon(
    icons: &mut glide_shell::icon::IconLoader, theme: &DockTheme, scale: i32, hovered: bool,
) -> Option<Vec<u8>> {
    let side = (theme.item_size / 2) * scale_or_one(scale);
    icons.load_symbolic("view-app-grid", side, launcher_colors(theme, hovered).1).map(<[u8]>::to_vec)
}

fn tooltip_x(anchor: Rectangle<i32, Logical>, width: i32, area: Rectangle<i32, Logical>) -> i32 {
    let min_x = area.loc.x + DOCK_EDGE_MARGIN;
    let max_x = (area.loc.x + area.size.w - DOCK_EDGE_MARGIN - width).max(min_x);
    (anchor.loc.x + (anchor.size.w - width) / 2).clamp(min_x, max_x)
}

fn push_tooltip<R: ShellRenderer>(
    dock: &mut Dock, frame: &mut Frame<'_, R>, order: &[usize], item_rects: &[Rectangle<i32, Logical>],
    area: Rectangle<i32, Logical>,
) {
    let Some(tooltip) = dock.tooltip.clone() else {
        return;
    };
    let progress = reveal_progress(&dock.tooltip_reveal, t1());
    if progress <= 0.0 {
        return;
    }
    let alpha = unit_alpha(progress);
    let Some(anchor) =
        order.iter().position(|index| *index == tooltip.index).and_then(|slot| item_rects.get(slot).copied())
    else {
        return;
    };
    let label = tooltip.label;
    if label.trim().is_empty() {
        return;
    }

    let (painter, theme, scale) = (frame.painter, frame.theme, frame.output_scale);
    let cache = &mut dock.rasters.tooltip;
    if !cache.contains_key(&(label.clone(), scale)) {
        cache.retain(|(cached, _), _| cached.as_str() == label.as_str());
    }
    let buffer = cached(cache, (label.clone(), scale), || draw_tooltip(painter, theme, &label, scale));
    let width = tooltip_width(painter, &label);
    let position =
        Point::from((tooltip_x(anchor, width, area), anchor.loc.y - theme.hover_lift - TOOLTIP_GAP - TOOLTIP_HEIGHT));
    frame.push(buffer, position.to_f64(), alpha, None);

    let shadow = cached(&mut dock.rasters.tooltip_shadow, (width, scale), || {
        glide_shell::popup_menu::draw_elevation(
            Size::from((width, TOOLTIP_HEIGHT)),
            TOOLTIP_RADIUS,
            scale_or_one(scale),
            glide_theme::elevation::Level::El2,
            theme.appearance,
        )
    });
    let margin = glide_shell::popup_menu::shadow_margin();
    let pos = Point::from((position.x - margin, position.y - margin));
    frame.push(shadow, pos.to_f64(), alpha, None);
}

fn tooltip_width(painter: &glide_paint::Painter, title: &str) -> i32 {
    let text = painter.text_width_styled(title, &glide_theme::TextStyle::CAP).ceil() as i32;
    text + TOOLTIP_PAD_X * 2
}

fn draw_tooltip(
    painter: &glide_paint::Painter, theme: &DockTheme, title: &str, scale: i32,
) -> Option<MemoryRenderBuffer> {
    let width = tooltip_width(painter, title);
    let height = TOOLTIP_HEIGHT;
    let scale = scale_or_one(scale);
    let mut c = Canvas::popover((width, height), scale, TOOLTIP_RADIUS, theme.menu_background, theme.menu_border)?;
    let style = &glide_theme::TextStyle::CAP;
    c.label(painter, (TOOLTIP_PAD_X, width - TOOLTIP_PAD_X), TextAlign::Center, title, theme.menu_text, style);
    Some(c.finish())
}

fn push_shell<R: ShellRenderer>(
    dock: &mut Dock, frame: &mut Frame<'_, R>, items: &[DockItem], item_rects: &[Rectangle<i32, Logical>],
    (shell, slide): (Rectangle<i32, Logical>, i32), opacity: f32, backdrop: Option<&Backdrop<'_>>,
) {
    let (theme, output_scale) = (frame.theme, frame.output_scale);
    let separators: u64 = items
        .iter()
        .enumerate()
        .filter(|(index, item)| *index > 0 && item.separator_before)
        .fold(0, |acc, (index, _)| acc | (1u64 << (index % 64)));
    let shell_key = (shell.size.w, shell.size.h, output_scale);
    let shell_state = (backdrop_key(backdrop, shell), separators);
    if dock.shell_backdrop_key != shell_state {
        dock.shell_backdrop_key = shell_state;
        dock.rasters.shell.clear();
    }
    let buffer = cached(&mut dock.rasters.shell, shell_key, || {
        draw_shell(theme, items, item_rects, shell, output_scale, backdrop)
    });
    let drawn = Point::from((shell.loc.x, shell.loc.y + slide));
    frame.push(buffer, drawn.to_f64(), opacity, None);

    let shadow = cached(&mut dock.rasters.shadow, shell_key, || {
        glide_shell::popup_menu::draw_elevation(
            shell.size,
            theme.radius,
            output_scale,
            glide_theme::elevation::Level::El2,
            theme.appearance,
        )
    });
    let margin = glide_shell::popup_menu::shadow_margin();
    let pos = Point::from((drawn.x - margin, drawn.y - margin));
    frame.push(shadow, pos.to_f64(), opacity, None);
}

type BackdropKey = (u64, u64, i32, i32);

fn backdrop_key(backdrop: Option<&Backdrop<'_>>, rest: Rectangle<i32, Logical>) -> Option<BackdropKey> {
    backdrop.map(|b| (b.generation, b.fractional_scale.to_bits(), rest.loc.x, rest.loc.y))
}

fn push_menu<R: ShellRenderer>(dock: &mut Dock, frame: &mut Frame<'_, R>) {
    let Some(menu) = dock.menu.clone() else {
        return;
    };
    let progress = reveal_progress(&dock.menu_reveal, t2());
    if progress <= 0.0 {
        return;
    }
    let (painter, theme, output_scale) = (frame.painter, frame.theme, frame.output_scale);
    let placement = MenuPlacement::new(menu.rect, progress);

    let row_alphas: Vec<f32> = (0..menu.entries.len())
        .map(|row| unit_alpha(layer_alpha(&dock.menu_rows, &row, t1())) * placement.alpha)
        .collect();
    if row_alphas.iter().any(|alpha| *alpha > 0.0) {
        let buffer = cached(&mut dock.rasters.menu_row_layer, output_scale, || {
            let width = glide_shell::popup_menu::WIDTH - glide_shell::popup_menu::PADDING * 2;
            let size = (width, glide_shell::popup_menu::ROW_HEIGHT);
            draw_panel(size, theme.menu_item_hover, glide_shell::popup_menu::ROW_RADIUS, output_scale)
        });
        for (row, alpha) in row_alphas.iter().enumerate() {
            if *alpha > 0.0 {
                let row_rect = glide_shell::popup_menu::row_rect(menu.rect, row);
                frame.push_placed(buffer, &placement, row_rect, *alpha);
            }
        }
    }

    let key = (menu.title.clone(), menu.entries.clone(), output_scale);
    let cache = &mut dock.rasters.menu;
    if !cache.contains_key(&key) {
        cache.retain(|(title, entries, _), _| (title, entries) == (&key.0, &key.1));
    }
    let buffer = cached(cache, key, || {
        let colors = glide_shell::popup_menu::MenuColors::from_dock(theme);
        glide_shell::popup_menu::draw(painter, &colors, &menu.title, &menu.rows(), None, output_scale)
    });
    frame.push_placed(buffer, &placement, menu.rect, placement.alpha);

    let shadow_key = (menu.rect.size.w, menu.rect.size.h, output_scale);
    let shadow = cached(&mut dock.rasters.menu_shadow, shadow_key, || {
        glide_shell::popup_menu::draw_shadow(
            menu.rect.size,
            glide_shell::popup_menu::RADIUS,
            output_scale,
            theme.appearance,
        )
    });
    let margin = glide_shell::popup_menu::shadow_margin();
    let shadow_rect = Rectangle::new(
        Point::from((menu.rect.loc.x - margin, menu.rect.loc.y - margin)),
        Size::from((menu.rect.size.w + margin * 2, menu.rect.size.h + margin * 2)),
    );
    frame.push_placed(shadow, &placement, shadow_rect, placement.alpha);
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct MenuPlacement {
    anchor: Point<f64, Logical>,
    rise: f64,
    scale: f64,
    alpha: f32,
}

impl MenuPlacement {
    fn new(rect: Rectangle<i32, Logical>, progress: f64) -> Self {
        let progress = progress.clamp(0.0, 1.0);
        Self {
            anchor: Point::from((
                f64::from(rect.loc.x) + f64::from(rect.size.w) / 2.0,
                f64::from(rect.loc.y + rect.size.h),
            )),
            rise: f64::from(MENU_RISE) * (1.0 - progress),
            scale: MENU_SCALE_FROM + (1.0 - MENU_SCALE_FROM) * progress,
            alpha: progress as f32,
        }
    }

    fn map(&self, rect: Rectangle<i32, Logical>) -> (Point<f64, Logical>, Size<i32, Logical>) {
        let rect = rect.to_f64();
        let x = self.anchor.x + (rect.loc.x - self.anchor.x) * self.scale;
        let y = self.anchor.y + self.rise + (rect.loc.y - self.anchor.y) * self.scale;
        let size = Size::from((
            (rect.size.w * self.scale).round().max(1.0) as i32,
            (rect.size.h * self.scale).round().max(1.0) as i32,
        ));
        (Point::from((x, y)), size)
    }
}

fn draw_panel((width, height): (i32, i32), color: Color, radius: f32, scale: i32) -> Option<MemoryRenderBuffer> {
    let mut c = Canvas::new(width, height, scale_or_one(scale))?;
    c.panel(color, radius);
    Some(c.finish())
}

struct HoverGeometry {
    position: Point<f64, Logical>,
    side: i32,
    layer_alpha: f32,
}

impl HoverGeometry {
    fn centred(theme: &DockTheme, origin: Point<f64, Logical>, lift: f64, scale: f64, layer_alpha: f32) -> Self {
        let item = f64::from(theme.item_size);
        let side = (item * scale).round().max(1.0) as i32;
        let half = f64::from(side) / 2.0;
        Self {
            position: Point::from((origin.x + item / 2.0 - half, origin.y + item / 2.0 - lift - half)),
            side,
            layer_alpha,
        }
    }

    fn lifted(theme: &DockTheme, position: Point<f64, Logical>) -> Self {
        Self::centred(theme, position, 0.0, theme.hover_scale, 1.0)
    }

    fn for_tile(
        theme: &DockTheme, rect: Rectangle<i32, Logical>, is_hovered: bool, is_launcher: bool, hover_t: f64,
        layer_alpha: f64,
    ) -> Self {
        let layer_alpha = layer_alpha.clamp(0.0, 1.0) as f32;
        if !is_hovered {
            return Self {
                position: Point::from((f64::from(rect.loc.x), f64::from(rect.loc.y))),
                side: theme.item_size,
                layer_alpha,
            };
        }

        let (target_lift, target_scale) = if is_launcher {
            (LAUNCHER_HOVER_LIFT, LAUNCHER_HOVER_SCALE)
        } else {
            (theme.hover_lift, theme.hover_scale)
        };

        Self::centred(
            theme,
            rect.loc.to_f64(),
            f64::from(target_lift) * hover_t,
            1.0 + (target_scale - 1.0) * hover_t,
            layer_alpha,
        )
    }
}

fn push_tile_icon<R: ShellRenderer>(
    dock: &mut Dock, frame: &mut Frame<'_, R>, index: usize, hover: &HoverGeometry, opacity: f32,
) {
    let (painter, theme, scale) = (frame.painter, frame.theme, frame.output_scale);
    let item = &dock.items[index];
    let icons = &mut dock.icons;
    let buffer = cached(&mut dock.tiles[index], scale, || {
        let icon = if item.action == DockAction::OpenLauncher {
            launcher_icon(icons, theme, scale, false)
        } else {
            icons.load(&item.app_id, theme.icon_size * scale_or_one(scale)).map(<[u8]>::to_vec)
        };
        draw_tile(painter, theme, item, icon.as_deref(), scale)
    });
    frame.push_tile(buffer, hover, opacity);
}

const fn launcher_colors(theme: &DockTheme, hovered: bool) -> (Color, Color) {
    if hovered {
        (theme.launcher_background_hover, theme.launcher_glyph_hover)
    } else {
        (theme.launcher_background, theme.launcher_glyph)
    }
}

fn launcher_tile(theme: &DockTheme, icon: Option<&[u8]>, scale: i32, hovered: bool) -> Option<Canvas<'static>> {
    let side = theme.item_size;
    let mut c = Canvas::new(side, side, scale_or_one(scale))?;
    let (background, glyph) = launcher_colors(theme, hovered);
    c.fill(c.full(), background, theme.item_radius);
    if let Some(icon) = icon {
        let icon_side = side / 2;
        let offset = (side - icon_side) / 2;
        c.blit_square(icon, icon_side, (offset, offset));
    } else {
        glide_paint::draw_dock_glyph(&mut c.buf, side, side, c.scale, glide_paint::DockGlyph::Grid, glyph);
    }
    Some(c)
}

fn draw_launcher_tile(theme: &DockTheme, icon: Option<&[u8]>, scale: i32, hovered: bool) -> Option<MemoryRenderBuffer> {
    Some(launcher_tile(theme, icon, scale, hovered)?.finish())
}

fn badge_width(painter: &glide_paint::Painter, label: &str) -> i32 {
    let text = painter.text_width_styled(label, &BADGE_STYLE).ceil() as i32;
    (text + BADGE_PAD_X * 2).max(BADGE_MIN_WIDTH)
}

fn badge_scale(progress: f64, rising: bool) -> f64 {
    if rising { 0.6 + 0.4 * glide_theme::motion::E_OVER.ease_f64(progress) } else { 1.0 }
}

fn progress_permille(progress: f64) -> u16 {
    (progress.clamp(0.0, 1.0) * 1000.0).round() as u16
}

fn push_badge<R: ShellRenderer>(
    dock: &mut Dock, frame: &mut Frame<'_, R>, index: usize, (hover, well): (&HoverGeometry, &HoverGeometry),
    opacity: f32,
) {
    let Some(item) = dock.items.get(index) else {
        return;
    };
    if item.action != DockAction::Activate {
        return;
    }
    let live = dock.badge_for(&item.app_id);
    let Some((shown, progress, rising)) = dock.badge_presence(&item.app_id) else {
        return;
    };
    let badge = if live.is_empty() { shown } else { live };
    let alpha = unit_alpha(progress) * opacity;
    let (painter, theme, scale) = (frame.painter, frame.theme, frame.output_scale);

    if let Some(count) = badge.count {
        let label = badge_label(count);
        let buffer =
            cached(&mut dock.rasters.badge, (label.clone(), scale), || draw_badge(painter, theme, &label, scale));
        let width = badge_width(painter, &label);
        let grow = badge_scale(progress, rising);
        let right = hover.position.x + f64::from(hover.side) - f64::from(BADGE_INSET);
        let top = hover.position.y + f64::from(BADGE_INSET);
        let centre = (right - f64::from(width) / 2.0, top + f64::from(BADGE_HEIGHT) / 2.0);
        let drawn_w = f64::from(width) * grow;
        let drawn_h = f64::from(BADGE_HEIGHT) * grow;
        let origin = Point::from((centre.0 - drawn_w / 2.0, centre.1 - drawn_h / 2.0));
        let drawn = Size::from((drawn_w.round().max(1.0) as i32, drawn_h.round().max(1.0) as i32));
        let src = Size::from((width, BADGE_HEIGHT));
        frame.push(buffer, origin, alpha, Some((src, drawn)));
    }

    if let Some(fraction) = badge.progress {
        let permille = progress_permille(fraction);
        let buffer = cached(&mut dock.rasters.progress, (permille, scale), || draw_progress(theme, permille, scale));
        let position = Point::from((
            well.position.x + f64::from(PROGRESS_INSET),
            well.position.y + f64::from(theme.item_size - PROGRESS_INSET - PROGRESS_HEIGHT),
        ));
        frame.push(buffer, position, alpha, None);
    }
}

fn draw_badge(
    painter: &glide_paint::Painter, theme: &DockTheme, label: &str, scale: i32,
) -> Option<MemoryRenderBuffer> {
    let width = badge_width(painter, label);
    let mut c = Canvas::new(width, BADGE_HEIGHT, scale_or_one(scale))?;
    c.fill(c.full(), theme.focused_color, glide_theme::shape::R_ROUND);
    c.label(painter, (0, width), TextAlign::Center, label, theme.badge_text, &BADGE_STYLE);
    Some(c.finish())
}

fn progress_track_width(theme: &DockTheme) -> i32 {
    (theme.item_size - PROGRESS_INSET * 2).max(1)
}

fn progress_fill_width(track: i32, permille: u16) -> i32 {
    if permille == 0 {
        return 0;
    }
    let filled = i64::from(track) * i64::from(permille.min(1000)) / 1000;
    i32::try_from(filled).unwrap_or(track).clamp(PROGRESS_HEIGHT.min(track), track)
}

fn draw_progress(theme: &DockTheme, permille: u16, scale: i32) -> Option<MemoryRenderBuffer> {
    let width = progress_track_width(theme);
    let mut c = Canvas::new(width, PROGRESS_HEIGHT, scale_or_one(scale))?;
    let round = glide_theme::shape::R_ROUND;
    c.fill(c.full(), theme.fallback_background, round);
    let filled = progress_fill_width(width, permille);
    if filled > 0 {
        c.fill(rect(0, 0, filled, PROGRESS_HEIGHT), theme.focused_color, round);
    }
    Some(c.finish())
}

fn press_layer_color(theme: &DockTheme) -> Color {
    let [r, g, b, _] = theme.hover_layer.0;
    Color([r, g, b, glide_theme::motion::ST_PRESS])
}

fn push_run_indicator<R: ShellRenderer>(
    dock: &mut Dock, frame: &mut Frame<'_, R>, index: usize, rect: Rectangle<i32, Logical>, run_alpha: f64,
    opacity: f32,
) {
    if run_alpha <= 0.0 {
        return;
    }
    let (theme, scale) = (frame.theme, frame.output_scale);
    let state = dock.run.get(index).copied().unwrap_or_default();
    let kind = state.indicator();
    let width = dock.run_width(index);
    let dots = if state.focused || width != dots_width(state.dots()) { 1 } else { state.dots() };
    let buffer = cached(&mut dock.rasters.runs, (width, kind, dots, scale), || {
        draw_run_indicator(theme, (width, dots), kind, scale)
    });
    let pos = Point::from((rect.loc.x + (theme.item_size - width) / 2, rect.loc.y + theme.item_size + RUN_GAP));
    frame.push(buffer, pos.to_f64(), opacity * unit_alpha(run_alpha), None);
}

fn draw_shell(
    theme: &DockTheme, items: &[DockItem], item_rects: &[Rectangle<i32, Logical>], shell: Rectangle<i32, Logical>,
    scale: i32, backdrop: Option<&Backdrop<'_>>,
) -> Option<MemoryRenderBuffer> {
    let (w, h) = (shell.size.w, shell.size.h);
    let mut canvas = Canvas::new(w, h, scale)?;
    if backdrop.is_some_and(|source| source.blit(&mut canvas.buf, w, h, theme.radius, shell, scale)) {
        canvas.fill(canvas.full(), theme.background, theme.radius);
    } else {
        canvas.panel(theme.background, theme.radius);
    }
    let factor = scale as f32;
    glide_paint::draw_inset_border(&mut canvas.buf, w * scale, h * scale, factor, theme.radius * factor, theme.border);
    draw_edge_highlight(&mut canvas.buf, w, h, scale, theme.radius, theme.edge_highlight);

    for (index, item) in items.iter().enumerate() {
        if index == 0 || !item.separator_before {
            continue;
        }
        let Some(item) = item_rects.get(index) else {
            continue;
        };
        let x = item.loc.x - shell.loc.x - SEPARATOR_MARGIN - SEPARATOR_WIDTH;
        let y = theme.padding_y + (theme.item_size - SEPARATOR_HEIGHT) / 2;
        canvas.fill(rect(x, y, SEPARATOR_WIDTH, SEPARATOR_HEIGHT), theme.separator, SEPARATOR_WIDTH as f32 / 2.0);
    }
    Some(canvas.finish())
}

fn draw_edge_highlight(buf: &mut [u8], w: i32, h: i32, scale: i32, radius: f32, color: Color) {
    let (Ok(pw), Ok(ph), Some(mut edge)) =
        (usize::try_from(w * scale), usize::try_from(h * scale), Canvas::new(w, h, scale))
    else {
        return;
    };
    if pw == 0 || ph == 0 {
        return;
    }
    edge.stroke(rect(1, 1, w - 2, h - 2), 1.0, (radius - 1.0).max(0.0), color);
    let band = ((radius * scale_f(scale)).ceil() as usize).clamp(1, ph);
    blend_top_band(buf, &edge.buf, pw, band);
}

fn blend_top_band(dst: &mut [u8], src: &[u8], row_pixels: usize, band: usize) {
    let stride = row_pixels * 4;
    for row in 0..band {
        let fade = 1.0 - row as f32 / band as f32;
        let span = row * stride..(row + 1) * stride;
        let (Some(dst_row), Some(src_row)) = (dst.get_mut(span.clone()), src.get(span)) else {
            return;
        };
        for (out, over) in dst_row.chunks_exact_mut(4).zip(src_row.chunks_exact(4)) {
            let alpha = f32::from(over[3]) * fade;
            if alpha <= 0.0 {
                continue;
            }
            let keep = 1.0 - alpha / 255.0;
            for (channel, source) in out.iter_mut().zip(over) {
                let value = f32::from(*source).mul_add(fade, f32::from(*channel) * keep);
                {
                    *channel = value.round().clamp(0.0, 255.0) as u8;
                }
            }
        }
    }
}

fn scale_or_one(scale: i32) -> i32 {
    scale.max(1)
}

fn scale_f(scale: i32) -> f32 {
    scale as f32
}

fn draw_tile(
    painter: &glide_paint::Painter, theme: &DockTheme, item: &DockItem, icon: Option<&[u8]>, scale: i32,
) -> Option<MemoryRenderBuffer> {
    let side = theme.item_size;
    let scale = scale_or_one(scale);
    if item.action == DockAction::OpenLauncher {
        return draw_launcher_tile(theme, icon, scale, false);
    }
    let mut c = Canvas::new(side, side, scale)?;
    let offset = (side - theme.icon_size) / 2;
    if let Some(icon) = icon {
        c.blit_square(icon, theme.icon_size, (offset, offset));
        return Some(c.finish());
    }
    let well = rect(offset, offset, theme.icon_size, theme.icon_size);
    c.fill(well, theme.fallback_background, glide_theme::shape::R3);
    let initial = glide_shell::icon::fallback_initial(&item.name);
    let style = &glide_theme::TextStyle::FALLBACK_INITIAL;
    c.text_in(painter, well, TextAlign::Center, &initial, theme.fallback_text, style);
    Some(c.finish())
}

fn run_indicator_color(theme: &DockTheme, kind: RunIndicator) -> Color {
    match kind {
        RunIndicator::Attention => theme.attention_color,
        RunIndicator::Focused => theme.focused_color,
        RunIndicator::Running => {
            let [r, g, b, a] = theme.running_color.0;
            Color([r, g, b, a * 0.8])
        }
    }
}

fn draw_run_indicator(
    theme: &DockTheme, (width, dots): (i32, u8), kind: RunIndicator, scale: i32,
) -> Option<MemoryRenderBuffer> {
    let mut c = Canvas::new(width, RUN_THICKNESS, scale)?;
    let colour = run_indicator_color(theme, kind);
    let radius = scale_f(RUN_THICKNESS) / 2.0;
    if dots <= 1 {
        c.fill(c.full(), colour, radius);
    } else {
        for dot in 0..i32::from(dots) {
            c.fill(rect(dot * (RUN_DOT + RUN_GAP), 0, RUN_DOT, RUN_THICKNESS), colour, radius);
        }
    }
    Some(c.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(count: usize) -> DockConfig {
        DockConfig {
            theme: DockTheme::default(),
            autohide: DockAutohide::default(),
            items: (0..count)
                .map(|i| DockItemConfig {
                    name: format!("App {i}"),
                    command: "true".to_string(),
                    icon: "terminal".to_string(),
                    app_id: None,
                    color_from: None,
                    color_to: None,
                    separator_before: false,
                })
                .collect(),
        }
    }

    fn area() -> Rectangle<i32, Logical> {
        Rectangle::new(Point::from((0, 0)), Size::from((1920, 1080)))
    }

    mod keyboard {
        use super::*;

        #[test]
        fn steps_wrap_both_ways_and_start_from_the_nearest_end() {
            assert_eq!(step_index(Some(0), 1, 3), 1);
            assert_eq!(step_index(Some(2), 1, 3), 0);
            assert_eq!(step_index(Some(0), -1, 3), 2);
            assert_eq!(step_index(None, 1, 3), 0);
            assert_eq!(step_index(None, -1, 3), 2);
        }

        #[test]
        fn the_dock_takes_keyboard_focus_walks_and_lets_go() {
            let mut dock = Dock::new(&config(3));
            assert!(!dock.keyboard_step(1), "nothing moves before the dock has focus");
            assert!(dock.begin_keyboard());
            assert!(dock.keyboard_active());
            assert_eq!(dock.keyboard_index(), Some(0));
            assert!(dock.keyboard_step(-1));
            assert_eq!(dock.keyboard_index(), Some(dock.items().len() - 1));
            assert!(!dock.wants_hidden(true, false), "a focused dock stays on screen");
            assert!(dock.end_keyboard());
            assert_eq!(dock.keyboard_index(), None);
            assert!(!dock.end_keyboard());
        }

        #[test]
        fn the_menu_rows_are_walked_from_the_keyboard() {
            let mut dock = Dock::new(&config(2));
            dock.begin_keyboard();
            assert!(dock.open_menu(0, area(), &MenuContext::default()));
            assert_eq!(dock.menu_selected(), None, "no row until an arrow is pressed");
            assert!(dock.menu_step(1));
            let (index, first) = dock.menu_selected().expect("a row is selected");
            assert_eq!(index, 0);
            assert!(dock.menu_step(-1));
            assert!(dock.menu_step(1));
            assert_eq!(dock.menu_selected(), Some((0, first)));
        }
    }

    fn centre_of(rect: Rectangle<i32, Logical>) -> Point<f64, Logical> {
        Point::from((
            f64::from(rect.loc.x) + f64::from(rect.size.w) / 2.0,
            f64::from(rect.loc.y) + f64::from(rect.size.h) / 2.0,
        ))
    }

    mod fit {
        use super::*;

        fn narrow() -> Rectangle<i32, Logical> {
            Rectangle::new(Point::from((0, 0)), Size::from((1366, 768)))
        }

        #[test]
        fn a_dock_that_fits_keeps_the_configured_size() {
            let mut dock = Dock::new(&config(5));
            dock.fit_to(narrow());
            assert_eq!(dock.theme, dock.base_theme);
        }

        #[test]
        fn a_crowded_dock_shrinks_its_tiles_to_stay_inside_the_area() {
            let mut dock = Dock::new(&config(30));
            assert!(dock.size().w > narrow().size.w);
            dock.fit_to(narrow());
            assert!(dock.theme.item_size < dock.base_theme.item_size);
            assert!(dock.theme.icon_size <= dock.theme.item_size);
            let rect = dock.rect(narrow());
            assert!(rect.loc.x >= narrow().loc.x);
            assert!(rect.loc.x + rect.size.w <= narrow().size.w);
            for tile in dock.item_rects(narrow()) {
                assert!(tile.loc.x >= 0 && tile.loc.x + tile.size.w <= narrow().size.w);
            }
            dock.fit_to(area());
            assert_eq!(dock.theme.item_size, dock.base_theme.item_size);
        }

        #[test]
        fn a_dock_wider_than_any_fit_is_pinned_to_the_left_edge() {
            let tiny = Rectangle::new(Point::from((10, 0)), Size::from((100, 600)));
            let mut dock = Dock::new(&config(40));
            dock.fit_to(tiny);
            assert_eq!(dock.theme.item_size, MIN_ITEM_SIZE);
            assert_eq!(dock.rect(tiny).loc.x, tiny.loc.x);
        }

        #[test]
        fn tooltips_stay_inside_the_area() {
            let anchor = Rectangle::new(Point::from((2, 700)), Size::from((40, 40)));
            assert_eq!(tooltip_x(anchor, 200, narrow()), DOCK_EDGE_MARGIN);
            let right = Rectangle::new(Point::from((1340, 700)), Size::from((40, 40)));
            assert_eq!(tooltip_x(right, 200, narrow()), 1366 - DOCK_EDGE_MARGIN - 200);
            let middle = Rectangle::new(Point::from((600, 700)), Size::from((40, 40)));
            assert_eq!(tooltip_x(middle, 200, narrow()), 600 + (40 - 200) / 2);
        }

        #[test]
        fn the_backdrop_key_ignores_the_autohide_slide_but_not_the_scale() {
            let pixels = [0u8; 4];
            let backdrop =
                |fractional_scale| Backdrop { pixels: &pixels, width: 1, height: 1, fractional_scale, generation: 7 };
            let rest = Rectangle::new(Point::from((100, 600)), Size::from((300, 80)));
            let one = backdrop(1.0);
            let key = backdrop_key(Some(&one), rest);
            assert_eq!(key, backdrop_key(Some(&one), rest));
            let fractional = backdrop(1.25);
            assert_ne!(key, backdrop_key(Some(&fractional), rest));
        }

        fn entry(item: DockMenuItem) -> MenuEntry {
            MenuEntry { item, label: String::new(), danger: false, muted: false }
        }

        #[test]
        fn a_short_menu_drops_the_oldest_windows_before_any_command() {
            let entries = vec![
                entry(DockMenuItem::Window(1)),
                entry(DockMenuItem::Window(2)),
                entry(DockMenuItem::Window(3)),
                entry(DockMenuItem::ShowAllWindows),
                entry(DockMenuItem::Action(0)),
                entry(DockMenuItem::Hide),
                entry(DockMenuItem::Quit),
            ];
            let items =
                |entries: Vec<MenuEntry>| -> Vec<DockMenuItem> { entries.into_iter().map(|e| e.item).collect() };
            assert_eq!(
                items(fit_menu_entries(entries.clone(), 5)),
                vec![
                    DockMenuItem::Window(1),
                    DockMenuItem::ShowAllWindows,
                    DockMenuItem::Action(0),
                    DockMenuItem::Hide,
                    DockMenuItem::Quit,
                ]
            );
            assert_eq!(
                items(fit_menu_entries(entries.clone(), 3)),
                vec![DockMenuItem::ShowAllWindows, DockMenuItem::Hide, DockMenuItem::Quit,]
            );
            assert_eq!(fit_menu_entries(entries.clone(), 1).len(), 3);
            assert_eq!(fit_menu_entries(entries.clone(), 20), entries);
        }
    }

    mod autohide {
        use super::*;

        fn dock(mode: DockAutohide) -> Dock {
            let mut config = config(3);
            config.autohide = mode;
            Dock::new(&config)
        }

        fn settle_hidden(dock: &mut Dock, obstructed: bool, in_zone: bool) {
            dock.update_autohide(obstructed, in_zone);
            if let Some(since) = dock.hide_pending.as_mut() {
                *since -= AUTOHIDE_DELAY;
            }
            dock.update_autohide(obstructed, in_zone);
        }

        #[test]
        fn never_stays_and_smart_hides_only_while_a_window_covers_it() {
            let mut never = dock(DockAutohide::Never);
            settle_hidden(&mut never, true, false);
            assert!(!never.is_hidden());

            let mut smart = dock(DockAutohide::Smart);
            settle_hidden(&mut smart, false, false);
            assert!(!smart.is_hidden());
            settle_hidden(&mut smart, true, false);
            assert!(smart.is_hidden());
            assert!(smart.update_autohide(false, false), "returns without a dwell");
            assert!(!smart.is_hidden());
        }

        #[test]
        fn always_mode_hides_until_the_pointer_dwells_at_the_edge() {
            let mut dock = dock(DockAutohide::Always);
            settle_hidden(&mut dock, false, false);
            assert!(dock.is_hidden());
            assert!(!dock.update_autohide(false, true), "a brush past the edge");
            assert!(dock.is_hidden());
            assert!(dock.is_animating(), "a pending dwell keeps frames coming");
            if let Some(since) = dock.edge_since.as_mut() {
                *since -= REVEAL_DWELL;
            }
            assert!(dock.update_autohide(false, true));
            assert!(!dock.is_hidden(), "revealing must not wait for the hide delay");
        }

        #[test]
        fn leaving_the_edge_restarts_a_fixed_dwell() {
            let start = Instant::now();
            assert!(!dwell_elapsed(start, start));
            assert!(!dwell_elapsed(start, start + REVEAL_DWELL / 2));
            assert!(dwell_elapsed(start, start + REVEAL_DWELL));
            assert_eq!(REVEAL_DWELL, Duration::from_millis(120));

            let mut dock = dock(DockAutohide::Always);
            settle_hidden(&mut dock, false, false);
            dock.update_autohide(false, true);
            if let Some(since) = dock.edge_since.as_mut() {
                *since -= REVEAL_DWELL / 2;
            }
            dock.update_autohide(false, false);
            assert_eq!(dock.edge_since, None);
            dock.update_autohide(false, true);
            assert!(!dock.update_autohide(false, true));
            assert!(dock.is_hidden());
        }

        #[test]
        fn attention_reveals_a_hidden_dock_for_a_while() {
            let mut dock = dock(DockAutohide::Always);
            settle_hidden(&mut dock, false, false);
            dock.reveal_for(ATTENTION_REVEAL);
            assert!(dock.update_autohide(false, false));
            assert!(!dock.is_hidden());
            dock.reveal_until = Instant::now().checked_sub(Duration::from_millis(1));
            settle_hidden(&mut dock, false, false);
            assert!(dock.is_hidden(), "it hides again once the reveal lapses");
        }

        #[test]
        fn a_lapsed_reveal_no_longer_keeps_the_frame_pump_alive() {
            let mut dock = dock(DockAutohide::Never);
            assert!(!dock.is_animating());
            dock.reveal_for(ATTENTION_REVEAL);
            assert!(dock.is_animating(), "a reveal still running wakes the pump to end it");
            dock.reveal_until = Instant::now().checked_sub(Duration::from_millis(1));
            assert!(dock.reveal_until.is_some(), "nothing has cleared the lapsed reveal yet");
            assert!(!dock.is_animating());
        }

        #[test]
        fn a_disabled_dock_never_keeps_the_frame_pump_alive() {
            let mut config = config(3);
            config.theme.enabled = false;
            let mut dock = Dock::new(&config);
            assert!(!dock.is_enabled());
            dock.reveal_for(ATTENTION_REVEAL);
            assert_eq!(dock.reveal_until, None, "a dock that is not drawn has nothing to reveal");
            dock.reveal_until = Some(Instant::now() + ATTENTION_REVEAL);
            assert!(dock.begin_launch("app 0", "App 0", "true"));
            assert!(dock.set_urgent("app 1", true));
            assert!(!dock.is_animating());
        }

        #[test]
        fn hiding_waits_then_runs_t3_ease_in_out_and_showing_runs_t2_ease_out() {
            let mut dock = dock(DockAutohide::Always);
            assert!(!dock.update_autohide(false, false));
            assert!(!dock.is_hidden());
            assert!(dock.is_animating(), "a pending hide keeps the frame pump alive");

            let mut fade = AutohideFade::shown();
            assert!(fade.set(true));
            assert_eq!(fade.duration(), glide_theme::motion::t3());
            assert!(std::ptr::fn_addr_eq(fade.ease(), ease_io as fn(f64) -> f64));
            assert!(!fade.set(true), "re-hiding is a no-op");
            assert!(fade.set(false));
            assert_eq!(fade.duration(), glide_theme::motion::t2());
            assert!(std::ptr::fn_addr_eq(fade.ease(), ease_out as fn(f64) -> f64));
        }

        #[test]
        fn hover_keeps_it_shown_and_a_hidden_dock_only_listens_at_the_edge() {
            let mut hovered = dock(DockAutohide::Always);
            hovered.set_hover(Some(0));
            settle_hidden(&mut hovered, true, false);
            assert!(!hovered.is_hidden());

            let mut dock = dock(DockAutohide::Always);
            let shell = dock.rect(area()).to_f64();
            let inside = shell.loc + Point::from((10.0, 10.0));
            assert!(dock.contains(area(), inside));
            assert!(dock.in_reveal_zone(area(), Point::from((shell.loc.x + 5.0, shell.loc.y - 4.0))));
            assert!(!dock.in_reveal_zone(area(), Point::from((5.0, 500.0))));
            settle_hidden(&mut dock, false, false);
            assert!(!dock.contains(area(), inside), "a hidden dock takes no clicks");
            assert!(!dock.in_reveal_zone(area(), inside));
            assert!(dock.in_reveal_zone(area(), Point::from((5.0, 1079.0))));
            assert!(!dock.in_reveal_zone(area(), Point::from((5.0, 1070.0))));
        }
    }

    mod pinning {
        use super::*;

        fn running(app_id: &str) -> RunningApp {
            RunningApp { name: app_id.to_string(), app_id: app_id.to_string(), focused: false, windows: 1 }
        }

        fn dock_with_one_running() -> Dock {
            let mut dock = Dock::new(&config(1));
            dock.set_running(&[running("firefox")]);
            dock
        }

        fn index_of(dock: &Dock, app_id: &str) -> usize {
            dock.items().iter().position(|item| item.app_id == app_id).expect("the app is on the dock")
        }

        #[test]
        fn a_tile_that_leaves_fades_out_where_it_was_drawn() {
            let mut dock = dock_with_one_running();
            let x = dock.settle_slides(&[("firefox", 240)], None, Instant::now(), Duration::ZERO)[0];
            dock.set_running(&[]);
            assert_eq!(dock.departures.len(), 1, "the closed app's tile lingers for its fade");
            assert!((f64::from(dock.departures[0].x) - x).abs() < f64::EPSILON);
        }

        #[test]
        fn a_running_app_can_be_pinned_and_survives_the_window_closing() {
            let mut dock = dock_with_one_running();
            let index = index_of(&dock, "firefox");

            assert!(dock.toggle_pinned(index, Some(("Firefox".to_string(), "firefox".to_string()))));
            assert_eq!(dock.pinned_items().len(), 2);

            dock.set_running(&[]);
            assert!(
                dock.items().iter().any(|item| item.app_id == "firefox"),
                "a pinned app must stay on the dock with no window open"
            );
        }

        #[test]
        fn pinning_records_a_command_so_the_tile_can_relaunch() {
            let mut dock = dock_with_one_running();
            let index = index_of(&dock, "firefox");
            dock.toggle_pinned(index, Some(("Firefox".to_string(), "firefox --new-window".to_string())));

            let pinned = dock.pinned_items().iter().find(|item| item.app_id == "firefox").expect("just pinned");
            assert_eq!(pinned.command, "firefox --new-window");
            assert_eq!(pinned.name, "Firefox", "the pin takes the desktop entry's name, not the window title");
        }

        fn menu_items(dock: &Dock, index: usize) -> Vec<DockMenuItem> {
            dock.menu_entries(index, &MenuContext::default()).into_iter().map(|entry| entry.item).collect()
        }

        #[test]
        fn the_menu_offers_pin_for_a_running_unpinned_app_and_unpin_for_a_pinned_one() {
            let dock = dock_with_one_running();
            let index = index_of(&dock, "firefox");
            let items = menu_items(&dock, index);
            assert!(items.contains(&DockMenuItem::Pin));
            assert!(items.contains(&DockMenuItem::Quit));
            assert!(!items.contains(&DockMenuItem::Open));

            let pinned_idle = index_of(&dock, "app 0");
            let items = menu_items(&dock, pinned_idle);
            assert_eq!(items, vec![DockMenuItem::Open, DockMenuItem::Unpin]);
        }

        #[test]
        fn a_running_app_lists_its_windows_first_then_show_all_hide_and_quit() {
            let dock = dock_with_one_running();
            let index = index_of(&dock, "firefox");
            let context = MenuContext {
                windows: vec![
                    MenuWindow { id: 7, title: "Inbox".to_string(), minimized: false },
                    MenuWindow { id: 9, title: String::new(), minimized: true },
                ],
                actions: vec!["Private Window".to_string()],
                quick: vec!["Recent Folder".to_string()],
            };
            let entries = dock.menu_entries(index, &context);
            let items: Vec<DockMenuItem> = entries.iter().map(|entry| entry.item).collect();
            assert_eq!(
                items,
                vec![
                    DockMenuItem::Window(7),
                    DockMenuItem::Window(9),
                    DockMenuItem::ShowAllWindows,
                    DockMenuItem::Action(0),
                    DockMenuItem::Quick(0),
                    DockMenuItem::Pin,
                    DockMenuItem::Hide,
                    DockMenuItem::Quit,
                ]
            );
            assert_eq!(entries[4].label, "Recent Folder", "the app's own quicklist follows its actions");
            assert_eq!(entries[0].label, "Inbox");
            assert!(!entries[0].muted);
            assert_eq!(entries[1].label, "firefox", "an untitled window shows the app");
            assert!(entries[1].muted, "a minimized window is marked");
            assert_eq!(entries[3].label, "Private Window");
            assert!(entries[7].danger, "quit is the destructive row");
        }

        #[test]
        fn a_window_without_an_app_identity_cannot_be_pinned() {
            let mut dock = Dock::new(&config(0));
            dock.set_running(&[running("window-12")]);
            let index = index_of(&dock, "window-12");
            assert!(!menu_items(&dock, index).contains(&DockMenuItem::Pin));
            assert!(!dock.toggle_pinned(index, None));
        }

        #[test]
        fn a_long_window_list_is_capped() {
            let dock = dock_with_one_running();
            let index = index_of(&dock, "firefox");
            let context = MenuContext {
                windows: (0..40).map(|id| MenuWindow { id, title: format!("w{id}"), minimized: false }).collect(),
                actions: Vec::new(),
                quick: Vec::new(),
            };
            let windows = dock
                .menu_entries(index, &context)
                .iter()
                .filter(|entry| matches!(entry.item, DockMenuItem::Window(_)))
                .count();
            assert_eq!(windows, MENU_WINDOW_LIMIT);
        }

        #[test]
        fn an_app_pinned_from_the_grid_gets_a_tile_and_can_be_unpinned_by_id() {
            let mut dock = Dock::new(&config(1));
            assert!(dock.pin_app("org.example.Editor", "Editor", "editor"));
            assert!(!dock.pin_app("org.example.editor", "Editor", "editor"), "already pinned");
            assert!(dock.is_app_pinned("ORG.EXAMPLE.EDITOR"));
            dock.set_running(&[]);
            assert!(dock.items().iter().any(|item| item.app_id == "org.example.Editor"));

            assert!(dock.unpin_app("org.example.editor"));
            assert!(!dock.unpin_app("org.example.editor"));
            dock.set_running(&[]);
            assert!(!dock.items().iter().any(|item| item.app_id == "org.example.Editor"));
        }

        #[test]
        fn the_launcher_tile_has_no_menu() {
            let mut dock = Dock::new(&config(1));
            let last = dock.items().len() - 1;
            assert!(menu_items(&dock, last).is_empty());
            assert!(!dock.open_menu(last, area(), &MenuContext::default()));
        }

        #[test]
        fn the_menu_opens_above_its_tile_and_hit_tests_its_rows() {
            let mut dock = dock_with_one_running();
            let index = index_of(&dock, "firefox");
            assert!(dock.open_menu(index, area(), &MenuContext::default()));
            let menu = dock.menu_rect().expect("menu rect");
            let tile = dock.item_rects(area())[index];
            assert!(menu.loc.y + menu.size.h < tile.loc.y);
            let inside = Point::from((f64::from(menu.loc.x) + 1.0, f64::from(menu.loc.y) + 1.0));
            assert!(dock.contains(area(), inside));

            let items = menu_items(&dock, index);
            for (row, item) in items.iter().enumerate() {
                let rect = glide_shell::popup_menu::row_rect(dock.menu_rect().expect("open"), row);
                let centre =
                    Point::from((f64::from(rect.loc.x + rect.size.w / 2), f64::from(rect.loc.y + rect.size.h / 2)));
                assert_eq!(dock.menu_item_at(centre), Some(*item));
                assert!(dock.set_menu_hover(Some(centre)));
                assert!(!dock.set_menu_hover(Some(centre)));
            }
            assert!(dock.close_menu());
            assert!(!dock.close_menu());
        }

        #[test]
        fn menu_hover_transitions_none_to_some_and_between_rows_and_to_none() {
            let mut dock = dock_with_one_running();
            let index = index_of(&dock, "firefox");
            assert!(dock.open_menu(index, area(), &MenuContext::default()));
            let menu_rect = dock.menu_rect().expect("open menu rect");
            let items = menu_items(&dock, index);
            assert!(items.len() >= 2);

            let row = |index| Some(centre_of(glide_shell::popup_menu::row_rect(menu_rect, index)));
            let outside = Some(Point::from((f64::from(menu_rect.loc.x - 100), f64::from(menu_rect.loc.y - 100))));
            assert_eq!(dock.open_menu_state().unwrap().hovered, None);
            assert!(!dock.set_menu_hover(None));
            for (pointer, hovered, lit) in [
                (row(0), Some(0), [1.0, 0.0]),
                (row(1), Some(1), [0.0, 1.0]),
                (outside, None, [0.0, 0.0]),
                (row(0), Some(0), [1.0, 0.0]),
                (None, None, [0.0, 0.0]),
            ] {
                assert!(dock.set_menu_hover(pointer), "{pointer:?}");
                assert!(!dock.set_menu_hover(pointer), "no change on repeat");
                assert_eq!(dock.open_menu_state().unwrap().hovered, hovered);
                for (index, target) in lit.into_iter().enumerate() {
                    let fade = dock.menu_rows.get(&index).map_or(0.0, glide_ui::Tween::target);
                    assert!((fade - target).abs() < f64::EPSILON, "row {index}");
                }
            }
        }

        #[test]
        fn a_short_press_is_a_click_and_a_long_move_is_a_drag() {
            let mut dock = Dock::new(&config(3));
            let rects = dock.item_rects(area());
            let start = centre_of(rects[0]);

            dock.begin_press(0, start, area());
            assert!(!dock.drag_motion(Point::from((start.x + 2.0, start.y))));
            assert!(!dock.is_dragging());
            assert_eq!(dock.end_press(area(), start), PressOutcome::Click(0));

            dock.begin_press(0, start, area());
            let far = centre_of(rects[2]);
            assert!(dock.drag_motion(far));
            assert!(dock.is_dragging());
            assert_eq!(dock.end_press(area(), far), PressOutcome::Drop { index: 0, target: 2 });
            assert!(!dock.is_dragging());
        }

        #[test]
        fn releasing_outside_the_pressed_tile_is_not_a_click() {
            let mut dock = Dock::new(&config(2));
            let rects = dock.item_rects(area());
            dock.begin_press(0, centre_of(rects[0]), area());
            assert_eq!(dock.end_press(area(), Point::from((10.0, 10.0))), PressOutcome::Nothing);
        }

        #[test]
        fn the_launcher_tile_cannot_be_dragged() {
            let mut dock = Dock::new(&config(1));
            let last = dock.items().len() - 1;
            let rects = dock.item_rects(area());
            dock.begin_press(last, centre_of(rects[last]), area());
            assert!(!dock.drag_motion(Point::from((0.0, 0.0))));
            assert_eq!(dock.end_press(area(), Point::from((0.0, 0.0))), PressOutcome::Nothing);
        }

        #[test]
        fn dragging_reorders_the_display_and_move_pinned_makes_it_stick() {
            let mut dock = Dock::new(&config(3));
            let rects = dock.item_rects(area());
            dock.begin_press(0, centre_of(rects[0]), area());
            dock.drag_motion(centre_of(rects[2]));
            let (shown, order) = dock.display_order(area()).expect("dragging");
            assert_eq!(order, vec![1, 2, 0, 3]);
            assert_eq!(shown[2].app_id, "app 0");
            assert!(shown.iter().take(3).all(|item| !item.separator_before));
            assert!(shown[3].separator_before, "the launcher keeps its divider");

            assert!(dock.move_pinned(0, 2));
            let names: Vec<_> = dock.pinned_items().iter().map(|item| item.app_id.clone()).collect();
            assert_eq!(names, vec!["app 1", "app 2", "app 0"]);
            assert!(!dock.move_pinned(2, 2));
            assert!(!dock.move_pinned(9, 0));
        }

        #[test]
        fn a_running_app_dragged_into_the_pinned_group_is_shown_there() {
            let mut dock = dock_with_one_running();
            let firefox = index_of(&dock, "firefox");
            let rects = dock.item_rects(area());
            dock.begin_press(firefox, centre_of(rects[firefox]), area());
            dock.drag_motion(Point::from((f64::from(rects[0].loc.x), centre_of(rects[0]).y)));
            let (shown, order) = dock.display_order(area()).expect("dragging");
            assert_eq!(order[0], firefox);
            assert!(!shown[0].separator_before);
            assert!(!shown[1].separator_before, "the old dynamic group has nothing left to separate");
        }

        #[test]
        fn a_layout_change_cancels_the_drag() {
            let mut dock = dock_with_one_running();
            let firefox = index_of(&dock, "firefox");
            let rects = dock.item_rects(area());
            dock.begin_press(firefox, centre_of(rects[firefox]), area());
            dock.drag_motion(centre_of(rects[0]));
            dock.set_running(&[]);
            assert!(!dock.is_dragging());
        }

        #[test]
        fn a_tooltip_appears_only_after_the_delay_and_never_during_a_drag() {
            let mut dock = Dock::new(&config(2));
            assert_eq!(dock.tooltip_index(), None);
            dock.set_hover(Some(1));
            assert_eq!(dock.tooltip_index(), None, "too early");
            assert!(dock.is_animating(), "the frame pump must stay alive");
            dock.hover_since = Instant::now().checked_sub(TOOLTIP_DELAY);
            assert_eq!(dock.tooltip_index(), Some(1));

            let rects = dock.item_rects(area());
            dock.begin_press(1, centre_of(rects[1]), area());
            dock.drag_motion(centre_of(rects[0]));
            assert_eq!(dock.tooltip_index(), None);
        }

        #[test]
        fn the_menu_closes_when_its_tile_disappears() {
            let mut dock = dock_with_one_running();
            let index = index_of(&dock, "firefox");
            assert!(dock.open_menu(index, area(), &MenuContext::default()));
            dock.set_running(&[]);
            assert!(dock.menu_rect().is_none());
        }

        #[test]
        fn a_launch_shows_a_temporary_tile_that_the_first_window_takes_over() {
            let mut dock = Dock::new(&config(1));
            assert!(dock.begin_launch("org.example.Editor", "Editor", "editor"));
            dock.set_running(&[]);
            let index = index_of(&dock, "org.example.Editor");
            assert_eq!(dock.run[index].signal, Signal::Launching);
            assert!(dock.run[index].shows_indicator(), "a launch shows the dot");
            assert!(!dock.is_pinned(index));

            dock.set_running(&[running("org.example.editor")]);
            assert!(!dock.launching.contains_key(&app_key("org.example.Editor")));
            let index = index_of(&dock, "org.example.editor");
            assert!(dock.run[index].running);
            assert_eq!(dock.run[index].signal, Signal::Quiet);
        }

        #[test]
        fn a_launch_of_a_pinned_app_marks_its_tile_without_adding_one() {
            let mut dock = Dock::new(&config(1));
            let before = dock.items().len();
            assert!(dock.begin_launch("app 0", "App 0", "true"));
            dock.set_running(&[]);
            assert_eq!(dock.items().len(), before);
            assert_eq!(dock.run[0].signal, Signal::Launching);
        }

        #[test]
        fn a_launch_that_never_maps_a_window_times_out() {
            let mut dock = Dock::new(&config(0));
            dock.begin_launch("org.example.Slow", "Slow", "slow");
            if let Some(launch) = dock.launching.get_mut("org.example.slow") {
                launch.started -= LAUNCH_TIMEOUT;
            }
            dock.set_running(&[]);
            assert!(!dock.launching.contains_key(&app_key("org.example.Slow")));
            assert_eq!(dock.items().len(), 1, "only the launcher is left");
        }

        #[test]
        fn urgency_turns_the_indicator_to_attention_and_focus_clears_it() {
            let mut dock = dock_with_one_running();
            assert!(dock.set_urgent("firefox", true));
            assert!(!dock.set_urgent("firefox", true), "already urgent");
            dock.set_running(&[running("firefox")]);
            let index = index_of(&dock, "firefox");
            assert_eq!(dock.run[index].indicator(), RunIndicator::Attention);

            let mut focused = running("firefox");
            focused.focused = true;
            dock.set_running(&[focused]);
            assert!(!dock.urgent.contains_key(&app_key("firefox")));
            assert_eq!(dock.run[index].indicator(), RunIndicator::Focused);
        }

        #[test]
        fn the_focused_app_keeps_its_well_and_loses_it_when_focus_moves() {
            let mut dock = dock_with_one_running();
            let mut focused = running("firefox");
            focused.focused = true;
            dock.set_running(&[focused]);
            assert!((dock.focus_wells["firefox"].target() - 1.0).abs() < f64::EPSILON);
            dock.set_running(&[running("firefox")]);
            assert!(dock.focus_wells["firefox"].target().abs() < f64::EPSILON);
        }

        #[test]
        fn badges_are_keyed_by_app_and_an_empty_badge_clears() {
            let mut dock = dock_with_one_running();
            let badge = Badge { count: Some(3), progress: Some(0.5) };
            assert!(dock.set_badge("Firefox", badge));
            assert!(!dock.set_badge("firefox", badge), "unchanged");
            assert_eq!(dock.badge_for("FIREFOX"), badge);
            dock.settle();
            assert!((dock.badge_fades["firefox"].0.target() - 1.0).abs() < f64::EPSILON);
            assert!(dock.set_badge("firefox", Badge::default()));
            assert_eq!(dock.badge_for("firefox"), Badge::default());
            dock.settle();
            let fade = dock.badge_fades.get("firefox").map(|(tween, _)| tween.target());
            assert!(fade.is_none_or(|target| target.abs() < f64::EPSILON));
        }

        #[test]
        fn pinning_twice_unpins_and_only_real_tiles_toggle() {
            let mut dock = dock_with_one_running();
            let index = index_of(&dock, "firefox");
            dock.toggle_pinned(index, None);
            dock.set_running(&[running("firefox")]);
            let index = index_of(&dock, "firefox");
            assert!(dock.toggle_pinned(index, None));
            assert!(!dock.pinned_items().iter().any(|i| i.app_id == "firefox"));
            dock.set_running(&[]);
            assert!(!dock.items().iter().any(|item| item.app_id == "firefox"));

            let last = dock.items().len() - 1;
            assert_eq!(dock.action(last), Some(DockAction::OpenLauncher));
            assert!(!dock.toggle_pinned(last, None), "the launcher cannot be pinned");
            assert!(!dock.toggle_pinned(999, None));
            let before = dock.pinned_items().len();
            assert!(dock.toggle_pinned(0, None), "configured items unpin too");
            assert_eq!(dock.pinned_items().len(), before - 1);
        }
    }

    mod slides {
        use super::*;

        fn targets<'a>(pairs: &[(&'a str, i32)]) -> Vec<(&'a str, i32)> {
            pairs.to_vec()
        }

        #[test]
        fn only_tiles_added_after_the_first_build_fade_in() {
            let now = Instant::now();
            let mut arrivals = HashMap::new();
            let mut slides = HashMap::new();
            note_arrivals(&mut arrivals, &slides, &targets(&[("a", 100)]), now);
            assert!(arrivals.is_empty(), "the dock's first build fades nothing in");
            slides.insert("a".to_string(), TileSlide::settled(100, now));
            note_arrivals(&mut arrivals, &slides, &targets(&[("a", 100), ("b", 160)]), now);
            assert_eq!(arrivals.keys().map(String::as_str).collect::<Vec<_>>(), vec!["b"]);
            note_arrivals(&mut arrivals, &slides, &targets(&[("a", 100)]), now);
            assert!(arrivals.is_empty(), "a tile that left again is forgotten");
            assert!((arrival(&arrivals, "a", now) - 1.0).abs() < f64::EPSILON);
        }

        #[test]
        fn a_slot_change_eases_from_the_previous_x_and_keeps_the_frame_pump_alive() {
            let mut dock = Dock::new(&config(1));
            let start = Instant::now();
            let duration = Duration::from_millis(200);

            let first = dock.settle_slides(&targets(&[("a", 100)]), None, start, duration);
            assert_eq!(first, vec![100.0], "a tile's first appearance does not slide");
            assert!(!dock.is_animating());

            let moved = dock.settle_slides(&targets(&[("a", 160)]), None, start, duration);
            assert_eq!(moved, vec![100.0], "the slide starts where the tile was drawn");
            assert!(dock.is_animating(), "a slide in progress must keep frames coming");

            let tile = dock.slides["a"];
            let midway = tile.position(start + duration / 2, duration);
            assert!(midway > 100.0 && midway < 160.0, "midway x was {midway}");
            assert!((tile.position(start + duration, duration) - 160.0).abs() < f64::EPSILON);
            assert!(!tile.is_moving(start + duration, duration));
        }

        #[test]
        fn a_zero_duration_slide_lands_instantly() {
            let mut dock = Dock::new(&config(1));
            let now = Instant::now();
            dock.settle_slides(&targets(&[("a", 100)]), None, now, Duration::ZERO);
            let moved = dock.settle_slides(&targets(&[("a", 160)]), None, now, Duration::ZERO);
            assert_eq!(moved, vec![160.0]);
            assert!(!dock.slides["a"].is_moving(now, Duration::ZERO));
        }

        #[test]
        fn slide_state_is_keyed_by_app_and_vanished_apps_are_dropped() {
            let mut dock = Dock::new(&config(1));
            let start = Instant::now();
            let duration = Duration::from_millis(200);
            dock.settle_slides(&targets(&[("app 0", 100), ("firefox", 160)]), None, start, duration);
            dock.settle_slides(&targets(&[("firefox", 100), ("app 0", 160)]), None, start, duration);
            let before = dock.slides["app 0"];

            let rebuilt = dock.settle_slides(&targets(&[("app 0", 160)]), None, start, duration);
            assert_eq!(dock.slides["app 0"], before, "the rebuild kept the slide");
            assert_eq!(rebuilt, vec![100.0]);
            assert!(!dock.slides.contains_key("firefox"));
        }

        #[test]
        fn the_dragged_tile_is_glued_to_the_pointer_and_slides_home_after_the_drop() {
            let mut dock = Dock::new(&config(1));
            let start = Instant::now();
            let duration = Duration::from_millis(200);
            dock.settle_slides(&targets(&[("a", 100)]), None, start, duration);

            let glued = dock.settle_slides(&targets(&[("a", 100)]), Some((0, 300.5)), start, duration);
            assert_eq!(glued, vec![300.5], "no easing while the pointer holds the tile");
            assert!(!dock.is_animating());

            let dropped = dock.settle_slides(&targets(&[("a", 100)]), None, start, duration);
            assert!((dropped[0] - 300.5).abs() < 1.0, "it sets off from the drop point");
            assert!(dock.is_animating());
        }
    }

    #[test]
    fn a_window_preview_replaces_the_tooltip_after_the_dwell_and_keeps_the_dock_up() {
        let mut dock = Dock::new(&config(0));
        dock.set_running(&[RunningApp {
            app_id: "org.example.Mail".to_string(),
            name: "Mail".to_string(),
            focused: false,
            windows: 2,
        }]);
        let index = dock.items.iter().position(|item| item.app_id == "org.example.Mail").expect("tile");
        let launcher = dock.items.iter().position(|item| item.action == DockAction::OpenLauncher).expect("launcher");
        dock.set_hover(Some(index));
        assert_eq!(dock.preview_candidate(), None, "not before the dwell");
        dock.hover_since = Instant::now().checked_sub(crate::preview::dwell());
        assert_eq!(dock.preview_candidate(), Some(index));
        assert_eq!(dock.tooltip_index(), Some(index));

        let window = crate::preview::PreviewWindow { id: 1, title: "Inbox".into(), minimized: false, size: (800, 600) };
        assert!(dock.show_preview(index, vec![window.clone()]));
        assert!(!dock.show_preview(index, vec![window]), "the same windows change nothing");
        assert_eq!(dock.tooltip_index(), None, "the preview takes the tooltip's place");
        assert_eq!(dock.preview_index(), Some(index));
        dock.set_hover(None);
        assert!(dock.is_engaged(), "the dock stays up while the pointer is on the preview");
        let layout = dock.preview_layout(area()).expect("layout");
        assert!(layout.panel.loc.y + layout.panel.size.h < dock.item_rects(area())[index].loc.y);
        assert!(dock.hide_preview() && !dock.preview_open());

        dock.set_hover(Some(launcher));
        dock.hover_since = Instant::now().checked_sub(crate::preview::dwell());
        assert_eq!(dock.preview_candidate(), None, "the launcher keeps its tooltip");
    }

    #[test]
    fn every_window_of_an_unfocused_app_gets_a_dot_up_to_three_and_focus_keeps_the_bar() {
        let mut dock = Dock::new(&config(0));
        let mail = |windows, focused| RunningApp {
            app_id: "org.example.Mail".to_string(),
            name: "Mail".to_string(),
            focused,
            windows,
        };
        for (windows, dots) in [(1, 1), (2, 2), (5, 3)] {
            dock.set_running(&[mail(windows, false)]);
            let index = dock.items.iter().position(|item| item.app_id == "org.example.Mail").expect("tile");
            dock.run_changed[index] = settled_stamp();
            assert_eq!(dock.run[index].dots(), dots);
            assert_eq!(dock.run_width(index), dots_width(dots), "{windows} windows");
        }
        assert_eq!(dots_width(3), 3 * RUN_DOT + 2 * RUN_GAP);
        assert!(dots_width(RUN_DOTS_MAX) <= dock.theme.item_size, "three dots stay under their tile");
        dock.set_running(&[mail(2, true)]);
        let index = dock.items.iter().position(|item| item.app_id == "org.example.Mail").expect("tile");
        dock.run_changed[index] = settled_stamp();
        assert_eq!(dock.run_width(index), RUN_BAR);
        assert_eq!(RunState { signal: Signal::Launching, ..RunState::default() }.dots(), 1, "a launch shows one dot");
    }

    #[test]
    fn the_run_indicator_has_its_own_band_clear_of_icon_and_shell_edge() {
        let dock = Dock::new(&config(2));
        let theme = &dock.theme;
        assert_eq!(dock.size().h, theme.padding_y * 2 + theme.item_size + RUN_BAND);
        let (shell, item) = (dock.rect(area()), dock.item_rects(area())[0]);
        let indicator_bottom = item.loc.y + theme.item_size + RUN_GAP + RUN_THICKNESS;
        assert!(shell.loc.y + shell.size.h - indicator_bottom >= RUN_GAP);
        assert!(RUN_DOT.max(RUN_BAR) <= theme.item_size);
    }

    #[test]
    fn the_launcher_button_is_always_last_even_with_nothing_pinned() {
        let empty = Dock::new(&config(0));
        assert!(empty.is_enabled());
        assert_eq!(empty.items().len(), 1);
        assert_eq!(empty.action(0), Some(DockAction::OpenLauncher));
        let dock = Dock::new(&config(3));
        let last = dock.items().len() - 1;
        assert_eq!(dock.action(last), Some(DockAction::OpenLauncher));
        for index in 0..last {
            assert_eq!(dock.action(index), Some(DockAction::Activate));
        }
    }

    #[test]
    fn the_dock_is_centred_on_the_bottom_edge() {
        let dock = Dock::new(&config(4));
        let rect = dock.rect(area());

        let left = rect.loc.x - area().loc.x;
        let right = (area().loc.x + area().size.w) - (rect.loc.x + rect.size.w);
        assert!((left - right).abs() <= 1, "not centred: {left} vs {right}");

        assert_eq!(
            area().loc.y + area().size.h - (rect.loc.y + rect.size.h),
            DockTheme::default().bottom_margin,
            "must sit its configured margin above the bottom"
        );
    }

    #[test]
    fn tiles_are_evenly_spaced_and_fit_inside_the_shell() {
        let dock = Dock::new(&config(4));
        let shell = dock.rect(area());
        let rects = dock.item_rects(area());
        assert_eq!(rects.len(), 5, "four pinned apps plus the launcher");

        let theme = DockTheme::default();
        for pair in rects[..4].windows(2) {
            assert_eq!(pair[1].loc.x - (pair[0].loc.x + pair[0].size.w), theme.item_gap);
        }
        for rect in &rects {
            assert!(rect.loc.x >= shell.loc.x + theme.padding_x);
            assert!(rect.loc.x + rect.size.w <= shell.loc.x + shell.size.w - theme.padding_x);
            assert_eq!(rect.size, Size::from((theme.item_size, theme.item_size)));
        }
    }

    #[test]
    fn only_an_inner_separator_widens_the_dock_by_its_footprint() {
        let plain = Dock::new(&config(3));
        let mut with_divider = config(3);
        with_divider.items[2].separator_before = true;
        let widened = Dock::new(&with_divider).size().w - plain.size().w;
        assert_eq!(widened, SEPARATOR_WIDTH + SEPARATOR_MARGIN * 2);
        let mut leading = config(3);
        leading.items[0].separator_before = true;
        assert_eq!(Dock::new(&leading).size(), plain.size());
    }

    #[test]
    fn minimized_windows_fly_to_their_app_tile_found_without_case() {
        let mut config = config(3);
        config.items[1].app_id = Some("Alacritty".to_string());
        let mut dock = Dock::new(&config);
        let tile = dock.tile_rect_for_app(area(), "alacritty");
        assert_eq!(tile, Some(dock.item_rects(area())[1]));
        assert_eq!(dock.tile_rect_for_app(area(), "org.example.Absent"), None);
        dock.set_running(&[RunningApp {
            app_id: "org.example.Editor".to_string(),
            name: "Editor".to_string(),
            focused: true,
            windows: 1,
        }]);
        assert!(dock.tile_rect_for_app(area(), "org.example.Editor").is_some());
    }

    #[test]
    fn tiles_hit_test_their_own_index_and_gaps_hit_nothing() {
        let dock = Dock::new(&config(4));
        let rects = dock.item_rects(area());
        for (index, rect) in rects.iter().enumerate() {
            assert_eq!(dock.item_at(area(), centre_of(*rect)), Some(index));
        }
        let gap_x = f64::from(rects[0].loc.x + rects[0].size.w) + 1.0;
        let y = centre_of(rects[0]).y;
        assert_eq!(dock.item_at(area(), Point::from((gap_x, y))), None);
        assert!(!dock.contains(area(), Point::from((10.0, 10.0))));
        assert!(dock.contains(area(), centre_of(dock.rect(area()))));
    }

    mod running_apps {
        use super::*;

        fn app(id: &str, focused: bool) -> RunningApp {
            RunningApp { app_id: id.to_string(), name: id.to_string(), focused, windows: 1 }
        }

        #[test]
        fn a_pinned_app_gains_a_dot_matched_without_case() {
            let mut dock = Dock::new(&config(2));
            let pinned_id = dock.items()[0].app_id.to_uppercase();
            assert!(dock.set_running(&[app(&pinned_id, true)]));
            assert_eq!(dock.items().len(), 3, "pinned pair plus the launcher");
            assert!(dock.run[0].running && dock.run[0].focused);
            assert!(!dock.run[1].running);
        }

        #[test]
        fn an_unpinned_running_app_gets_a_new_group_tile_for_as_long_as_it_runs() {
            let mut dock = Dock::new(&config(1));
            let running = [app("org.example.Editor", true)];
            assert!(dock.set_running(&running));
            assert!(!dock.set_running(&running), "repeating the set is a no-op");
            assert_eq!(dock.items().len(), 3, "pinned, the new app, then the launcher");
            let added = &dock.items()[1];
            assert_eq!(added.app_id, "org.example.Editor");
            assert!(added.separator_before, "it must start a new group");
            assert!(dock.run[1].running && dock.run[1].focused);
            assert!(!dock.is_pinned(1) && dock.is_pinned(0));
            assert!(dock.set_running(&[]));
            assert_eq!(dock.items().len(), 2, "the tile goes with the window");
        }

        #[test]
        fn a_dock_with_no_pins_still_shows_running_apps() {
            let mut dock = Dock::new(&config(0));

            dock.set_running(&[app("org.example.Editor", true)]);
            assert_eq!(dock.items().len(), 2, "the app, then the launcher");
            assert_eq!(dock.items()[0].app_id, "org.example.Editor");
            assert!(!dock.items()[0].separator_before, "nothing to separate it from");
        }

        #[test]
        fn a_hover_on_a_vanished_tile_is_dropped() {
            let mut dock = Dock::new(&config(1));
            dock.set_running(&[app("org.example.Editor", false)]);
            dock.set_hover(Some(dock.items().len() - 2));

            dock.set_running(&[]);
            assert_eq!(dock.hover.target(), None, "hovering a tile that no longer exists would index out of range");
        }
    }

    #[test]
    fn hovering_a_new_tile_animates_and_repeating_it_does_not() {
        let mut dock = Dock::new(&config(3));
        assert!(dock.set_hover(Some(1)));
        assert!(!dock.set_hover(Some(1)));
        assert!(dock.set_hover(None));
    }

    #[test]
    fn the_launcher_tile_paints_its_hover_tokens_and_keys_them_in_the_revision() {
        let launcher = DockItem::launcher();
        assert_eq!(launcher.action, DockAction::OpenLauncher);
        assert!(launcher.separator_before);
        let theme = DockTheme::default();
        let tokens = glide_theme::Tokens::new(glide_theme::Appearance::Dark, glide_theme::Accent::Blue);
        assert_eq!(launcher_colors(&theme, true), (tokens.fill_2, tokens.fg_1));
        assert_eq!(launcher_colors(&theme, false), (tokens.fill_1, tokens.fg_2));
        let idle = launcher_tile(&theme, None, 1, false).expect("idle raster").buf;
        let hover = launcher_tile(&theme, None, 1, true).expect("hover raster").buf;
        assert_eq!(idle.len(), hover.len());
        assert_ne!(idle, hover, "hovering must repaint the launcher tile");

        let mut recoloured = theme.clone();
        recoloured.launcher_background_hover = glide_theme::Color([1.0, 0.0, 0.0, 1.0]);
        assert_ne!(raster_revision(&theme), raster_revision(&recoloured));
        let mut reglyphed = theme.clone();
        reglyphed.launcher_glyph_hover = glide_theme::Color([0.0, 1.0, 0.0, 1.0]);
        assert_ne!(raster_revision(&theme), raster_revision(&reglyphed));
    }

    #[test]
    fn hover_lifts_and_grows_standard_tiles_smoothly_about_their_slot_centre() {
        let theme = DockTheme::default();
        let rect = Rectangle::new(Point::from((100, 200)), Size::from((theme.item_size, theme.item_size)));
        let hover = |t, launcher| HoverGeometry::for_tile(&theme, rect, true, launcher, t, 1.0);
        let lift = |geometry: HoverGeometry| {
            f64::from(rect.loc.y) - (geometry.position.y + f64::from(geometry.side - theme.item_size) / 2.0)
        };
        assert!((lift(hover(1.0, false)) - 4.0).abs() < 0.01);
        assert!(lift(hover(1.0, true)).abs() < 0.01, "the launcher does not lift");
        assert_eq!(hover(1.0, true).side, theme.item_size);
        let sub_pixel = f64::from(theme.hover_lift) * 0.3;
        assert!((lift(hover(0.3, false)) - sub_pixel).abs() < 1e-9);
        assert!(lift(hover(1.3, false)) > lift(hover(1.0, false)));
        assert!(hover(1.3, false).side >= hover(1.0, false).side);
        let slot_centre = f64::from(rect.loc.x) + f64::from(theme.item_size) / 2.0;
        for t in [0.1, 0.37, 0.8, 1.0, 1.12] {
            let geometry = hover(t, false);
            let centre_x = geometry.position.x + f64::from(geometry.side) / 2.0;
            assert!((centre_x - slot_centre).abs() < 1e-9, "t={t}");
        }
    }

    mod feedback {
        use super::*;

        const PERIOD: Duration = Duration::from_millis(400);

        #[test]
        fn bounces_land_each_cycle_and_attention_stops_after_its_count() {
            let launch = |at| bounce_height(at, PERIOD, 8.0, None);
            assert!(launch(Duration::ZERO).abs() < 1e-9);
            assert!((launch(PERIOD / 2) - 8.0).abs() < 1e-6, "the apex is the amplitude");
            assert!(launch(PERIOD / 4) > 0.0);
            assert!(launch(PERIOD * 99 / 100) < 1.0, "nearly down again");
            assert!(launch(PERIOD * 25 + PERIOD / 2) > 0.0, "launches bounce until resolved");
            let attention = |at| bounce_height(at, PERIOD, 6.0, Some(ATTENTION_BOUNCES));
            assert!(attention(PERIOD * ATTENTION_BOUNCES).abs() < 1e-9);
            assert!(attention(PERIOD + PERIOD / 2) > 0.0);
            assert!(bounce_height(PERIOD / 2, Duration::ZERO, 8.0, None).abs() < 1e-9);
        }

        #[test]
        fn a_bouncing_tile_keeps_the_frame_pump_alive() {
            let mut dock = Dock::new(&config(1));
            assert!(!dock.is_bouncing(Instant::now()));
            dock.begin_launch("app 0", "App 0", "true");
            dock.set_running(&[]);
            assert_eq!(dock.is_bouncing(Instant::now()), !t4().is_zero());
        }

        #[test]
        fn badges_cap_their_count_pop_from_sixty_percent_and_fill_progress() {
            assert_eq!([badge_label(7), badge_label(99), badge_label(100)], ["7", "99", "99+"]);
            assert!((badge_scale(0.0, true) - 0.6).abs() < 1e-9);
            assert!((badge_scale(1.0, true) - 1.0).abs() < 1e-9);
            assert!((badge_scale(0.3, false) - 1.0).abs() < 1e-9, "leaving only fades");
            for (permille, width) in [(0, 0), (1, PROGRESS_HEIGHT), (500, 20), (1000, 40)] {
                assert_eq!(progress_fill_width(40, permille), width);
            }
            assert_eq!((progress_permille(0.25), progress_permille(3.0)), (250, 1000));
        }
    }

    mod edge_highlight {
        use super::*;

        #[test]
        fn only_the_top_band_is_touched_and_it_fades_downward() {
            let width = 4;
            let rows = 6;
            let band = 3;
            let mut dst = vec![10u8; width * rows * 4];
            let src = vec![255u8; width * rows * 4];
            blend_top_band(&mut dst, &src, width, band);
            let row = |y: usize| dst[y * width * 4];
            assert_eq!(row(0), 255, "the top row takes the full highlight");
            assert!(row(1) > row(2) && row(2) > 10);
            for y in band..rows {
                assert_eq!(row(y), 10, "row {y} lies below the glass edge");
            }
        }

        #[test]
        fn the_shell_carries_the_bezel_highlight_on_its_top_edge() {
            let theme = DockTheme::default();
            let (w, h) = (120, 80);
            let mut plain = vec![0u8; usize::try_from(w * h * 4).unwrap_or(0)];
            glide_paint::Painter::fill_panel_background(&mut plain, w, h, 1, theme.background, theme.radius, [true; 4]);
            let mut lit = plain.clone();
            draw_edge_highlight(&mut lit, w, h, 1, theme.radius, theme.edge_highlight);
            let at = |buf: &[u8], x: i32, y: i32| {
                let i = usize::try_from((y * w + x) * 4).unwrap_or(0);
                buf[i..i + 4].to_vec()
            };
            assert_ne!(at(&lit, w / 2, 1), at(&plain, w / 2, 1));
            assert_eq!(at(&lit, w / 2, h / 2), at(&plain, w / 2, h / 2));
        }
    }
}

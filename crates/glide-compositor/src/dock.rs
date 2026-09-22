use std::time::{Duration, Instant};

use smithay::{
    backend::renderer::{
        ImportAll, ImportMem, Renderer,
        element::{
            Kind,
            memory::{MemoryRenderBuffer, MemoryRenderBufferRenderElement},
        },
    },
    utils::{Logical, Point, Rectangle, Scale, Size, Transform},
};

use crate::config::{Color, DockConfig, DockItemConfig, DockTheme};

fn hover_anim() -> Duration {
    glide_theme::motion::t2()
}

const RUN_DOT: i32 = 4;
const RUN_BAR: i32 = 16;
fn run_anim() -> Duration {
    glide_theme::motion::t3()
}
const RUN_GAP: i32 = 3;
const RUN_THICKNESS: i32 = 4;
const RUN_BAND: i32 = RUN_GAP + RUN_THICKNESS;

const LAUNCHER_HOVER_LIFT: i32 = 4;
const LAUNCHER_HOVER_SCALE: f64 = 1.04;

const SEPARATOR_WIDTH: i32 = 1;
const SEPARATOR_MARGIN: i32 = 3;
const SEPARATOR_HEIGHT: i32 = 22;

const TOOLTIP_DELAY: Duration = Duration::from_millis(550);
const TOOLTIP_HEIGHT: i32 = 26;
const TOOLTIP_PAD_X: i32 = glide_theme::spacing::SP3;
const TOOLTIP_GAP: i32 = glide_theme::spacing::SP2;
const TOOLTIP_RADIUS: f32 = glide_theme::shape::R2;
const DRAG_THRESHOLD: f64 = 6.0;
const DRAG_LIFT: i32 = 6;

const MENU_RISE: i32 = 6;
const MENU_SCALE_FROM: f64 = 0.94;
const PRESS_RADIUS: f32 = glide_theme::shape::R3;

fn slide_anim() -> Duration {
    glide_theme::motion::t2()
}

fn menu_anim() -> Duration {
    glide_theme::motion::t2()
}

fn tooltip_anim() -> Duration {
    glide_theme::motion::t1()
}

fn layer_anim() -> Duration {
    glide_theme::motion::t1()
}

fn run_fade_anim() -> Duration {
    glide_theme::motion::t2()
}

fn ease_out(t: f64) -> f64 {
    glide_theme::motion::E_OUT.ease_f64(t)
}

fn ease_io(t: f64) -> f64 {
    glide_theme::motion::E_IO.ease_f64(t)
}

fn reveal_progress(reveal: &glide_ui::Reveal, duration: Duration) -> f64 {
    if reveal.is_open() {
        reveal.progress(duration, ease_out)
    } else {
        reveal.progress(duration, ease_io)
    }
}

#[derive(Debug, Clone, Copy)]
struct Fade {
    tween: glide_ui::Tween,
    rising: bool,
}

impl Fade {
    fn hidden() -> Self {
        Self {
            tween: glide_ui::Tween::at(0.0),
            rising: false,
        }
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

fn layer_alpha(layers: &std::collections::HashMap<String, glide_ui::Tween>, key: &str) -> f64 {
    layers
        .get(key)
        .map_or(0.0, |tween| tween.value(layer_anim(), ease_io))
}

fn fade_all(layers: &mut std::collections::HashMap<String, glide_ui::Tween>) {
    let duration = layer_anim();
    for tween in layers.values_mut() {
        tween.retarget(0.0, duration, ease_io);
    }
}

fn prune_layers(layers: &mut std::collections::HashMap<String, glide_ui::Tween>) {
    let duration = layer_anim();
    layers.retain(|_, tween| tween.target() > 0.0 || tween.is_animating(duration));
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct TileSlide {
    from: f64,
    to: i32,
    started: Instant,
}

impl TileSlide {
    fn settled(x: i32, now: Instant) -> Self {
        Self {
            from: f64::from(x),
            to: x,
            started: now,
        }
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
        *self = Self {
            from,
            to,
            started: now,
        };
    }

    fn glue(&mut self, x: f64, now: Instant) {
        #[allow(clippy::cast_possible_truncation)]
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
    now.checked_sub(run_anim().max(glide_theme::motion::T3))
        .unwrap_or(now)
}

fn slide_key(item: &DockItem) -> &str {
    &item.app_id
}

fn settle_slides(
    slides: &mut std::collections::HashMap<String, TileSlide>,
    targets: &[(&str, i32)],
    glued: Option<(usize, f64)>,
    now: Instant,
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
            let tile = slides
                .get_mut(*key)
                .expect("inserted just above when missing");
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

fn raster_revision(theme: &DockTheme) -> u64 {
    glide_theme::Revision::new()
        .u64(theme.revision())
        .u64(crate::paint::painter_generation())
        .finish()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Glyph {
    Terminal,
    Files,
    Settings,
    Music,
    Mail,
    Generic,
    Grid,
}

impl Glyph {
    fn parse(name: &str) -> Self {
        match name.to_ascii_lowercase().as_str() {
            "terminal" => Self::Terminal,
            "files" => Self::Files,
            "settings" => Self::Settings,
            "music" => Self::Music,
            "mail" => Self::Mail,
            _ => Self::Generic,
        }
    }
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
    pub glyph: Glyph,
    pub color_from: Color,
    pub color_to: Color,
    pub separator_before: bool,
    pub action: DockAction,
}

impl DockItem {
    fn launcher() -> Self {
        let tokens =
            glide_theme::Tokens::new(glide_theme::Appearance::Dark, glide_theme::Accent::Blue);
        let base_bg = Color([
            tokens.shell_fg.0[0],
            tokens.shell_fg.0[1],
            tokens.shell_fg.0[2],
            0.08,
        ]);
        Self {
            name: "Applications".to_string(),
            command: String::new(),
            app_id: String::new(),
            glyph: Glyph::Grid,
            color_from: base_bg,
            color_to: base_bg,
            separator_before: true,
            action: DockAction::OpenLauncher,
        }
    }
}

impl DockItem {
    fn dynamic(app: &RunningApp) -> Self {
        let (color_from, color_to) = dynamic_colors(&app.app_id);
        Self {
            name: app.name.clone(),
            command: String::new(),
            app_id: app.app_id.clone(),
            glyph: Glyph::Generic,
            color_from,
            color_to,
            separator_before: false,
            action: DockAction::Activate,
        }
    }

    fn from_config(config: &DockItemConfig) -> Self {
        let fallback_from = Color::from_hex("#4a4640").expect("literal is valid");
        let fallback_to = Color::from_hex("#23201c").expect("literal is valid");
        Self {
            app_id: config
                .app_id
                .clone()
                .unwrap_or_else(|| config.name.to_lowercase()),
            name: config.name.clone(),
            command: config.command.clone(),
            glyph: Glyph::parse(&config.icon),
            color_from: config.color_from.unwrap_or(fallback_from),
            color_to: config.color_to.unwrap_or(fallback_to),
            separator_before: config.separator_before,
            action: DockAction::Activate,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RunState {
    pub running: bool,
    pub focused: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunningApp {
    pub app_id: String,
    pub name: String,
    pub focused: bool,
}

const DYNAMIC_PALETTE: [(&str, &str); 6] = [
    ("#5b7fa8", "#2f4b6b"),
    ("#7fae94", "#2f7a56"),
    ("#c9a06a", "#8a5c2a"),
    ("#a87f9e", "#6b3f63"),
    ("#8a8377", "#4c4740"),
    ("#a86a6a", "#6b2f2f"),
];

fn dynamic_colors(app_id: &str) -> (Color, Color) {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in app_id.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    let bucket = usize::try_from(hash % DYNAMIC_PALETTE.len() as u64)
        .expect("a modulo of the palette length always fits");
    let (from, to) = DYNAMIC_PALETTE[bucket];
    (
        Color::from_hex(from).expect("palette literals are valid"),
        Color::from_hex(to).expect("palette literals are valid"),
    )
}

#[derive(Debug)]
pub struct Dock {
    theme: DockTheme,
    pinned: Vec<DockItem>,
    items: Vec<DockItem>,
    run: Vec<RunState>,
    run_changed: Vec<Instant>,
    hover: glide_ui::HoverAnimation<usize>,
    pub icons: crate::icon::IconLoader,

    shell: std::collections::HashMap<(i32, i32, i32), MemoryRenderBuffer>,
    shadow: std::collections::HashMap<(i32, i32, i32), MemoryRenderBuffer>,
    tiles: Vec<std::collections::HashMap<i32, MemoryRenderBuffer>>,
    tile_layer: std::collections::HashMap<i32, MemoryRenderBuffer>,
    runs: std::collections::HashMap<(i32, bool, i32), MemoryRenderBuffer>,
    shell_backdrop_key: (Option<(u64, i32, i32)>, u64),
    menu: Option<DockMenu>,
    menu_reveal: glide_ui::Reveal,
    menu_rows: std::collections::HashMap<usize, glide_ui::Tween>,
    menu_row_layer: std::collections::HashMap<i32, MemoryRenderBuffer>,
    drag: Option<DockDrag>,
    slides: std::collections::HashMap<String, TileSlide>,
    hover_since: Option<Instant>,
    hover_layers: std::collections::HashMap<String, glide_ui::Tween>,
    press_layers: std::collections::HashMap<String, glide_ui::Tween>,
    press_layer: std::collections::HashMap<i32, MemoryRenderBuffer>,
    run_alphas: std::collections::HashMap<String, Fade>,
    tooltip: Option<TooltipState>,
    tooltip_reveal: glide_ui::Reveal,
    tooltip_cache: std::collections::HashMap<(String, i32), MemoryRenderBuffer>,
    tooltip_shadow: std::collections::HashMap<(i32, i32), MemoryRenderBuffer>,
    app_names: std::collections::HashMap<String, String>,
    menu_cache: std::collections::HashMap<(String, Vec<DockMenuItem>, i32), MemoryRenderBuffer>,
    menu_shadow: std::collections::HashMap<(i32, i32, i32), MemoryRenderBuffer>,
    raster_rev: u64,
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
            pinned,
            items,
            run: vec![RunState::default(); count],
            run_changed: vec![settled_stamp(); count],
            hover: glide_ui::HoverAnimation::default(),
            icons: crate::icon::IconLoader::new(config.theme.icon_theme.clone()),
            shell: std::collections::HashMap::new(),
            shadow: std::collections::HashMap::new(),
            tiles: vec![std::collections::HashMap::new(); count],
            tile_layer: std::collections::HashMap::new(),
            runs: std::collections::HashMap::new(),
            shell_backdrop_key: (None, 0),
            menu: None,
            menu_reveal: glide_ui::Reveal::closed(),
            menu_rows: std::collections::HashMap::new(),
            menu_row_layer: std::collections::HashMap::new(),
            drag: None,
            slides: std::collections::HashMap::new(),
            hover_since: None,
            hover_layers: std::collections::HashMap::new(),
            press_layers: std::collections::HashMap::new(),
            press_layer: std::collections::HashMap::new(),
            run_alphas: std::collections::HashMap::new(),
            tooltip: None,
            tooltip_reveal: glide_ui::Reveal::closed(),
            tooltip_cache: std::collections::HashMap::new(),
            tooltip_shadow: std::collections::HashMap::new(),
            app_names: std::collections::HashMap::new(),
            menu_cache: std::collections::HashMap::new(),
            menu_shadow: std::collections::HashMap::new(),
            raster_rev: raster_revision(&config.theme),
        }
    }

    pub fn reconfigure(&mut self, config: &DockConfig) {
        *self = Self::new(config);
    }

    fn invalidate_stale_buffers(&mut self) {
        let current = raster_revision(&self.theme);
        if current == self.raster_rev {
            return;
        }
        self.raster_rev = current;
        self.shell.clear();
        self.shadow.clear();
        self.runs.clear();
        self.tile_layer.clear();
        self.menu_cache.clear();
        self.menu_shadow.clear();
        self.menu_row_layer.clear();
        self.press_layer.clear();
        self.tooltip_cache.clear();
        self.tooltip_shadow.clear();
        for tile in &mut self.tiles {
            tile.clear();
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

    pub fn set_running(&mut self, running: &[RunningApp]) -> bool {
        let mut items = self.pinned.clone();
        let mut extras = 0;
        for app in running {
            let pinned = self
                .pinned
                .iter()
                .any(|item| item.app_id.eq_ignore_ascii_case(&app.app_id));
            if pinned {
                continue;
            }
            let mut item = DockItem::dynamic(app);
            item.separator_before = extras == 0 && !self.pinned.is_empty();
            items.push(item);
            extras += 1;
        }

        items.push(DockItem::launcher());

        let run: Vec<RunState> = items
            .iter()
            .map(|item| {
                let mut state = RunState::default();
                for app in running {
                    if app.app_id.eq_ignore_ascii_case(&item.app_id) {
                        state.running = true;
                        state.focused |= app.focused;
                    }
                }
                state
            })
            .collect();

        let layout_changed = items.len() != self.items.len()
            || items.iter().zip(&self.items).any(|(new, old)| {
                new.app_id != old.app_id || new.separator_before != old.separator_before
            });

        if !layout_changed && run == self.run {
            return false;
        }

        let now = Instant::now();
        let settled = settled_stamp();
        let changed_at: Vec<Instant> = items
            .iter()
            .zip(&run)
            .map(|(item, new_run)| {
                let old_index = self
                    .items
                    .iter()
                    .position(|old| old.app_id.eq_ignore_ascii_case(&item.app_id));
                match old_index {
                    Some(old_index)
                        if self
                            .run
                            .get(old_index)
                            .is_some_and(|old| old.focused == new_run.focused) =>
                    {
                        self.run_changed.get(old_index).copied().unwrap_or(settled)
                    }
                    Some(_) => now,
                    None => settled,
                }
            })
            .collect();
        self.run_changed = changed_at;

        if layout_changed {
            self.tiles = vec![std::collections::HashMap::new(); items.len()];
            self.shell.clear();
            self.shadow.clear();
            let menu_moved = self.menu.as_ref().is_some_and(|menu| {
                match (self.items.get(menu.index), items.get(menu.index)) {
                    (Some(old), Some(new)) => old.app_id != new.app_id,
                    _ => true,
                }
            });
            if menu_moved {
                self.close_menu();
            }
            self.drag = None;
            fade_all(&mut self.press_layers);
            let hover_moved = self.hover.target().is_some_and(|index| {
                match (self.items.get(index), items.get(index)) {
                    (Some(old), Some(new)) => old.app_id != new.app_id,
                    _ => true,
                }
            });
            if hover_moved {
                self.hover.retarget_without_restarting(None);
                fade_all(&mut self.hover_layers);
            }
        }

        self.items = items;
        self.run = run;

        let fade = run_fade_anim();
        for (item, state) in self.items.iter().zip(&self.run) {
            let target = if state.running { 1.0 } else { 0.0 };
            match self.run_alphas.get_mut(&item.app_id) {
                Some(alpha) => {
                    alpha.set(target, fade);
                }
                None if state.running => {
                    let mut alpha = Fade::hidden();
                    alpha.set(1.0, fade);
                    self.run_alphas.insert(item.app_id.clone(), alpha);
                }
                None => {}
            }
        }
        true
    }

    #[must_use]
    pub fn pinned_items(&self) -> &[DockItem] {
        &self.pinned
    }

    pub fn toggle_pinned(&mut self, index: usize, identity: Option<(String, String)>) -> bool {
        let Some(item) = self.items.get(index).cloned() else {
            return false;
        };
        if item.action != DockAction::Activate {
            return false;
        }

        let position = self
            .pinned
            .iter()
            .position(|pinned| pinned.app_id.eq_ignore_ascii_case(&item.app_id));

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
        self.pinned
            .iter()
            .any(|item| item.app_id.eq_ignore_ascii_case(app_id))
    }

    pub fn pin_app(&mut self, app_id: &str, name: &str, command: &str) -> bool {
        if app_id.trim().is_empty() || self.is_app_pinned(app_id) {
            return false;
        }
        let (color_from, color_to) = dynamic_colors(app_id);
        self.pinned.push(DockItem {
            name: name.to_string(),
            command: command.to_string(),
            app_id: app_id.to_string(),
            glyph: Glyph::Generic,
            color_from,
            color_to,
            separator_before: false,
            action: DockAction::Activate,
        });
        true
    }

    pub fn unpin_app(&mut self, app_id: &str) -> bool {
        let before = self.pinned.len();
        self.pinned
            .retain(|item| !item.app_id.eq_ignore_ascii_case(app_id));
        self.pinned.len() != before
    }

    #[must_use]
    pub fn menu_items(&self, index: usize) -> Vec<DockMenuItem> {
        let Some(item) = self.items.get(index) else {
            return Vec::new();
        };
        if item.action != DockAction::Activate {
            return Vec::new();
        }
        let running = self.run.get(index).is_some_and(|state| state.running);
        let launchable = !item.command.trim().is_empty();
        let mut items = Vec::new();
        if running && launchable {
            items.push(DockMenuItem::NewWindow);
        } else if !running && launchable {
            items.push(DockMenuItem::Open);
        }
        items.push(if self.is_pinned(index) {
            DockMenuItem::Unpin
        } else {
            DockMenuItem::Pin
        });
        if running {
            items.push(DockMenuItem::CloseAll);
        }
        items
    }

    pub fn open_menu(&mut self, index: usize, area: Rectangle<i32, Logical>) -> bool {
        let items = self.menu_items(index);
        if items.is_empty() {
            return false;
        }
        let Some(tile) = self.item_rects(area).into_iter().nth(index) else {
            return false;
        };
        let rect = crate::popup_menu::place_above(tile, area, items.len());
        let title = self.menu_title(index);
        if self.menu.as_ref().is_some_and(|menu| menu.index != index) {
            self.menu_reveal = glide_ui::Reveal::closed();
            self.menu_rows.clear();
        }
        self.menu = Some(DockMenu {
            index,
            items,
            rect,
            hovered: None,
            title,
        });
        self.menu_reveal.open(menu_anim());
        self.tooltip_reveal.close(tooltip_anim());
        true
    }

    fn menu_title(&mut self, index: usize) -> String {
        let Some(item) = self.items.get(index) else {
            return String::new();
        };
        if self.is_pinned(index) {
            return item.name.clone();
        }
        let fallback = item.name.clone();
        self.app_names
            .entry(item.app_id.to_lowercase())
            .or_insert_with(|| {
                crate::icon::find_application(&item.app_id).map_or(fallback, |app| app.name)
            })
            .clone()
    }

    pub fn close_menu(&mut self) -> bool {
        if !self.menu_reveal.is_open() {
            return false;
        }
        self.menu_reveal.close(menu_anim());
        if let Some(menu) = self.menu.as_mut() {
            menu.hovered = None;
        }
        let duration = layer_anim();
        for tween in self.menu_rows.values_mut() {
            tween.retarget(0.0, duration, ease_io);
        }
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
        menu.items.get(row).copied()
    }

    pub fn set_menu_hover(&mut self, pos: Option<Point<f64, Logical>>) -> bool {
        if !self.menu_reveal.is_open() {
            return false;
        }
        let Some(menu) = self.menu.as_mut() else {
            return false;
        };
        let hovered = pos.and_then(|pos| menu.row_at(pos));
        if menu.hovered == hovered {
            return false;
        }
        let previous = menu.hovered.replace(hovered).flatten();
        let duration = layer_anim();
        if let Some(row) = previous
            && let Some(tween) = self.menu_rows.get_mut(&row)
        {
            tween.retarget(0.0, duration, ease_io);
        }
        if let Some(row) = hovered {
            self.menu_rows
                .entry(row)
                .or_insert_with(|| glide_ui::Tween::at(0.0))
                .retarget(1.0, duration, ease_io);
        }
        true
    }

    fn menu_row_alpha(&self, row: usize) -> f64 {
        self.menu_rows
            .get(&row)
            .map_or(0.0, |tween| tween.value(layer_anim(), ease_io))
    }

    pub fn set_hover(&mut self, index: Option<usize>) -> bool {
        let changed = self.hover.set(index);
        if changed {
            self.hover_since = index.map(|_| Instant::now());
            self.retarget_hover_layer(index);
            if self
                .tooltip
                .as_ref()
                .is_some_and(|tooltip| Some(tooltip.index) != index)
            {
                self.tooltip_reveal.close(tooltip_anim());
            }
        }
        changed
    }

    fn retarget_hover_layer(&mut self, index: Option<usize>) {
        let target = index
            .and_then(|index| self.items.get(index))
            .map(|item| item.app_id.clone());
        let duration = layer_anim();
        for (key, tween) in &mut self.hover_layers {
            if Some(key) != target.as_ref() {
                tween.retarget(0.0, duration, ease_io);
            }
        }
        if let Some(key) = target {
            self.hover_layers
                .entry(key)
                .or_insert_with(|| glide_ui::Tween::at(0.0))
                .retarget(1.0, duration, ease_io);
        }
    }

    fn hover_layer_alpha(&self, app_id: &str) -> f64 {
        layer_alpha(&self.hover_layers, app_id)
    }

    fn press_layer_alpha(&self, app_id: &str) -> f64 {
        layer_alpha(&self.press_layers, app_id)
    }

    fn run_alpha(&self, app_id: &str) -> f64 {
        self.run_alphas
            .get(app_id)
            .map_or(0.0, |fade| fade.value(run_fade_anim()))
    }

    fn tooltip_label(&mut self, index: usize) -> String {
        match self.items.get(index) {
            Some(item) if item.action == DockAction::OpenLauncher => {
                crate::i18n::t(crate::i18n::Key::Applications).to_string()
            }
            Some(_) => self.menu_title(index),
            None => String::new(),
        }
    }

    fn sync_tooltip(&mut self) {
        let duration = tooltip_anim();
        let Some(index) = self.tooltip_index() else {
            self.tooltip_reveal.close(duration);
            return;
        };
        if self
            .tooltip
            .as_ref()
            .is_none_or(|tooltip| tooltip.index != index)
        {
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
        let menu = menu_anim();
        self.menu_reveal.settle(menu);
        if !self.menu_reveal.is_open() && !self.menu_reveal.is_visible(menu) {
            self.menu = None;
            self.menu_rows.clear();
        }
        let tooltip = tooltip_anim();
        self.tooltip_reveal.settle(tooltip);
        if !self.tooltip_reveal.is_open() && !self.tooltip_reveal.is_visible(tooltip) {
            self.tooltip = None;
        }
        prune_layers(&mut self.hover_layers);
        prune_layers(&mut self.press_layers);
        let layer = layer_anim();
        self.menu_rows
            .retain(|_, tween| tween.target() > 0.0 || tween.is_animating(layer));
        let fade = run_fade_anim();
        self.run_alphas
            .retain(|_, alpha| alpha.target() > 0.0 || alpha.is_animating(fade));
    }

    fn tooltip_index(&self) -> Option<usize> {
        if self.menu.is_some() || self.is_dragging() {
            return None;
        }
        let since = self.hover_since?;
        if since.elapsed() < TOOLTIP_DELAY {
            return None;
        }
        self.hover
            .target()
            .filter(|index| self.items.get(*index).is_some())
    }

    #[must_use]
    pub fn is_animating(&self) -> bool {
        let run = run_anim();
        let tooltip_pending = self.menu.is_none()
            && self.drag.is_none()
            && self
                .hover_since
                .is_some_and(|since| since.elapsed() < TOOLTIP_DELAY);
        let now = Instant::now();
        let slide = slide_anim();
        let layer = layer_anim();
        let fade = run_fade_anim();
        let hover_geometry = self.hover.is_animating(hover_anim());
        let run_width = !run.is_zero() && self.run_changed.iter().any(|stamp| stamp.elapsed() < run);
        let slides = self.slides.values().any(|tile| tile.is_moving(now, slide));
        let menu = self.menu_reveal.is_animating(menu_anim());
        let tooltip = self.tooltip_reveal.is_animating(tooltip_anim());
        let hover_layers = self
            .hover_layers
            .values()
            .any(|tween| tween.is_animating(layer));
        let press_layers = self
            .press_layers
            .values()
            .any(|tween| tween.is_animating(layer));
        let menu_rows = self.menu_rows.values().any(|tween| tween.is_animating(layer));
        let run_fade = self.run_alphas.values().any(|alpha| alpha.is_animating(fade));
        hover_geometry
            || tooltip_pending
            || run_width
            || slides
            || menu
            || tooltip
            || hover_layers
            || press_layers
            || menu_rows
            || run_fade
    }

    fn slid_rects(
        &mut self,
        order: &[usize],
        item_rects: &[Rectangle<i32, Logical>],
        dragged: Option<(usize, Point<f64, Logical>)>,
    ) -> Vec<Rectangle<i32, Logical>> {
        let targets: Vec<(&str, i32)> = order
            .iter()
            .zip(item_rects)
            .map(|(index, rect)| (slide_key(&self.items[*index]), rect.loc.x))
            .collect();
        let glued = dragged.and_then(|(index, position)| {
            order
                .iter()
                .position(|candidate| *candidate == index)
                .map(|slot| (slot, position.x))
        });
        settle_slides(
            &mut self.slides,
            &targets,
            glued,
            Instant::now(),
            slide_anim(),
        )
        .into_iter()
        .zip(item_rects)
        .map(|(x, rect)| {
            let mut rect = *rect;
            #[allow(clippy::cast_possible_truncation)]
            {
                rect.loc.x = x.round() as i32;
            }
            rect
        })
        .collect()
    }

    #[cfg(test)]
    fn settle_slides(
        &mut self,
        targets: &[(&str, i32)],
        glued: Option<(usize, f64)>,
        now: Instant,
        duration: Duration,
    ) -> Vec<f64> {
        settle_slides(&mut self.slides, targets, glued, now, duration)
    }

    pub fn begin_press(
        &mut self,
        index: usize,
        pos: Point<f64, Logical>,
        area: Rectangle<i32, Logical>,
    ) {
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
        self.press_layers
            .entry(app_id)
            .or_insert_with(|| glide_ui::Tween::at(0.0))
            .jump(1.0);
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
            fade_all(&mut self.hover_layers);
            fade_all(&mut self.press_layers);
            self.tooltip_reveal.close(tooltip_anim());
        }
        true
    }

    #[must_use]
    pub fn is_dragging(&self) -> bool {
        self.drag.is_some_and(|drag| drag.active)
    }

    pub fn end_press(
        &mut self,
        area: Rectangle<i32, Logical>,
        pos: Point<f64, Logical>,
    ) -> PressOutcome {
        let Some(drag) = self.drag.take() else {
            return PressOutcome::Nothing;
        };
        fade_all(&mut self.press_layers);
        if drag.active {
            let target = self.drag_target(&drag, area);
            return PressOutcome::Drop {
                index: drag.index,
                target,
            };
        }
        if self.item_at(area, pos) == Some(drag.index) {
            PressOutcome::Click(drag.index)
        } else {
            PressOutcome::Nothing
        }
    }

    pub fn cancel_press(&mut self) -> bool {
        fade_all(&mut self.press_layers);
        self.drag.take().is_some_and(|drag| drag.active)
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

        let mut order: Vec<usize> = (0..self.items.len())
            .filter(|index| *index != drag.index)
            .collect();
        let pinned_slots = order.iter().filter(|index| self.is_pinned(**index)).count();
        let target = self.drag_target(&drag, area).min(pinned_slots);
        order.insert(target, drag.index);

        let mut items: Vec<DockItem> = order
            .iter()
            .map(|index| self.items[*index].clone())
            .collect();
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

    fn dragged_position(
        &self,
        area: Rectangle<i32, Logical>,
    ) -> Option<(usize, Point<f64, Logical>)> {
        let drag = self.drag.filter(|drag| drag.active)?;
        let shell = self.rect(area);
        let x = (drag.pointer.x - drag.grab_offset).clamp(
            f64::from(shell.loc.x),
            f64::from(shell.loc.x + shell.size.w - self.theme.item_size),
        );
        let y = f64::from(shell.loc.y + self.theme.padding_y - DRAG_LIFT);
        Some((drag.index, Point::from((x, y))))
    }

    fn run_width(&self, index: usize) -> i32 {
        let focused = self.run.get(index).is_some_and(|state| state.focused);
        let (from, to) = if focused {
            (RUN_DOT, RUN_BAR)
        } else {
            (RUN_BAR, RUN_DOT)
        };
        let duration = run_anim();
        let elapsed = self
            .run_changed
            .get(index)
            .map_or(duration, Instant::elapsed);
        if duration.is_zero() || elapsed >= duration {
            return to;
        }
        let t = glide_theme::motion::E_OUT.ease_f64(elapsed.as_secs_f64() / duration.as_secs_f64());
        #[allow(clippy::cast_possible_truncation)]
        {
            (f64::from(from) + f64::from(to - from) * t).round() as i32
        }
    }

    fn hover_progress(&self) -> f64 {
        glide_theme::motion::E_OVER.ease_f64(self.hover.progress(hover_anim()))
    }

    #[must_use]
    pub fn size(&self) -> Size<i32, Logical> {
        let theme = &self.theme;
        let count = i32::try_from(self.items.len()).unwrap_or(0);
        if count == 0 {
            return Size::from((0, 0));
        }

        let separators = i32::try_from(
            self.items
                .iter()
                .skip(1)
                .filter(|item| item.separator_before)
                .count(),
        )
        .unwrap_or(0);

        let width = theme.padding_x * 2
            + count * theme.item_size
            + (count - 1) * theme.item_gap
            + separators * (SEPARATOR_WIDTH + SEPARATOR_MARGIN * 2);
        Size::from((width, theme.padding_y * 2 + theme.item_size + RUN_BAND))
    }

    #[must_use]
    pub fn rect(&self, area: Rectangle<i32, Logical>) -> Rectangle<i32, Logical> {
        let size = self.size();
        let x = area.loc.x + (area.size.w - size.w) / 2;
        let y = area.loc.y + area.size.h - size.h - self.theme.bottom_margin;
        Rectangle::new(Point::from((x, y)), size)
    }

    #[must_use]
    pub fn item_rects(&self, area: Rectangle<i32, Logical>) -> Vec<Rectangle<i32, Logical>> {
        self.rects_for(&self.items, area)
    }

    fn rects_for(
        &self,
        items: &[DockItem],
        area: Rectangle<i32, Logical>,
    ) -> Vec<Rectangle<i32, Logical>> {
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
                let rect = Rectangle::new(
                    Point::from((x, y)),
                    Size::from((theme.item_size, theme.item_size)),
                );
                x += theme.item_size;
                rect
            })
            .collect()
    }

    #[must_use]
    pub fn tile_rect_for_app(
        &self,
        area: Rectangle<i32, Logical>,
        app_id: &str,
    ) -> Option<Rectangle<i32, Logical>> {
        let index = self
            .items
            .iter()
            .position(|item| item.app_id.eq_ignore_ascii_case(app_id))?;
        self.item_rects(area).into_iter().nth(index)
    }

    #[must_use]
    pub fn item_at(
        &self,
        area: Rectangle<i32, Logical>,
        pos: Point<f64, Logical>,
    ) -> Option<usize> {
        self.item_rects(area)
            .into_iter()
            .position(|rect| rect.to_f64().contains(pos))
    }

    #[must_use]
    pub fn contains(&self, area: Rectangle<i32, Logical>, pos: Point<f64, Logical>) -> bool {
        if !self.is_enabled() {
            return false;
        }
        if self.rect(area).to_f64().contains(pos) {
            return true;
        }
        self.menu_rect()
            .is_some_and(|rect| rect.to_f64().contains(pos))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DockMenuItem {
    Open,
    NewWindow,
    Pin,
    Unpin,
    CloseAll,
}

impl DockMenuItem {
    #[must_use]
    pub fn label(self) -> &'static str {
        use crate::i18n::{Key, t};
        match self {
            Self::Open => t(Key::Open),
            Self::NewWindow => t(Key::NewWindow),
            Self::Pin => t(Key::PinToDock),
            Self::Unpin => t(Key::UnpinFromDock),
            Self::CloseAll => t(Key::CloseAllWindows),
        }
    }

    #[must_use]
    pub const fn is_danger(self) -> bool {
        matches!(self, Self::CloseAll)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DockMenu {
    index: usize,
    items: Vec<DockMenuItem>,
    rect: Rectangle<i32, Logical>,
    hovered: Option<usize>,
    title: String,
}

impl DockMenu {
    fn row_at(&self, pos: Point<f64, Logical>) -> Option<usize> {
        crate::popup_menu::row_at(self.rect, self.items.len(), pos)
    }

    fn rows(&self) -> Vec<crate::popup_menu::MenuRow> {
        self.items
            .iter()
            .map(|item| crate::popup_menu::MenuRow {
                label: item.label(),
                danger: item.is_danger(),
            })
            .collect()
    }
}

#[allow(clippy::too_many_arguments)]
pub fn render<R>(
    dock: &mut Dock,
    renderer: &mut R,
    painter: &crate::paint::Painter,
    area: Rectangle<i32, Logical>,
    scale: Scale<f64>,
    output_scale: i32,
    hidden: f64,
    backdrop: Option<&Backdrop<'_>>,
) -> Vec<MemoryRenderBufferRenderElement<R>>
where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Send + Clone + 'static,
{
    let hidden = hidden.clamp(0.0, 1.0);
    if !dock.is_enabled() || hidden >= 1.0 {
        return Vec::new();
    }

    dock.invalidate_stale_buffers();
    dock.settle();
    dock.sync_tooltip();

    let mut shell = dock.rect(area);
    #[allow(clippy::cast_possible_truncation)]
    let slide = (f64::from(shell.size.h + dock.theme.bottom_margin) * hidden).round() as i32;
    shell.loc.y += slide;
    #[allow(clippy::cast_possible_truncation)]
    let opacity = (1.0 - hidden) as f32;
    let (display_items, order) = dock
        .display_order(area)
        .unwrap_or_else(|| (dock.items.clone(), (0..dock.items.len()).collect()));
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
    let hovered = dock.hover.target();
    let hover_t = dock.hover_progress();
    let theme = dock.theme.clone();

    let mut elements = Vec::new();

    if hidden <= 0.0 {
        push_menu(
            dock,
            renderer,
            painter,
            &theme,
            scale,
            output_scale,
            &mut elements,
        );
        push_tooltip(
            dock,
            renderer,
            painter,
            &theme,
            &order,
            &drawn_rects,
            scale,
            output_scale,
            &mut elements,
        );
    }

    let layers: Vec<TileLayers> = order
        .iter()
        .map(|index| {
            let app_id = &dock.items[*index].app_id;
            TileLayers {
                hover: dock.hover_layer_alpha(app_id),
                press: dock.press_layer_alpha(app_id),
                run: dock.run_alpha(app_id),
            }
        })
        .collect();

    for (slot, rect) in drawn_rects.iter().enumerate() {
        let index = order[slot];
        let tile_layers = layers[slot];
        let is_hovered = hovered == Some(index);
        let is_launcher = dock.items[index].action == DockAction::OpenLauncher;
        let hover = match dragged {
            Some((drag_index, position)) if drag_index == index => {
                HoverGeometry::lifted(&theme, position)
            }
            _ => HoverGeometry::for_tile(
                &theme,
                *rect,
                is_hovered,
                is_launcher,
                hover_t,
                tile_layers.hover,
            ),
        };
        push_tile(
            dock,
            renderer,
            index,
            &theme,
            &hover,
            tile_layers.press,
            scale,
            output_scale,
            opacity,
            &mut elements,
        );
        push_run_indicator(
            dock,
            renderer,
            index,
            &theme,
            *rect,
            tile_layers.run,
            scale,
            output_scale,
            opacity,
            &mut elements,
        );
    }

    push_shell(
        dock,
        renderer,
        &theme,
        &display_items,
        &item_rects,
        shell,
        scale,
        output_scale,
        opacity,
        backdrop,
        &mut elements,
    );

    elements
}

#[derive(Debug, Clone, Copy)]
struct TileLayers {
    hover: f64,
    press: f64,
    run: f64,
}

#[allow(clippy::too_many_arguments)]
fn push_tooltip<R>(
    dock: &mut Dock,
    renderer: &mut R,
    painter: &crate::paint::Painter,
    theme: &DockTheme,
    order: &[usize],
    item_rects: &[Rectangle<i32, Logical>],
    scale: Scale<f64>,
    output_scale: i32,
    elements: &mut Vec<MemoryRenderBufferRenderElement<R>>,
) where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Send + Clone + 'static,
{
    let Some(tooltip) = dock.tooltip.clone() else {
        return;
    };
    let progress = reveal_progress(&dock.tooltip_reveal, tooltip_anim());
    if progress <= 0.0 {
        return;
    }
    #[allow(clippy::cast_possible_truncation)]
    let alpha = progress.clamp(0.0, 1.0) as f32;
    let Some(anchor) = order
        .iter()
        .position(|index| *index == tooltip.index)
        .and_then(|slot| item_rects.get(slot).copied())
    else {
        return;
    };
    let label = tooltip.label;
    if label.trim().is_empty() {
        return;
    }

    let key = (label.clone(), output_scale);
    if !dock.tooltip_cache.contains_key(&key) {
        dock.tooltip_cache.clear();
        if let Some(buffer) = draw_tooltip(painter, theme, &label, output_scale) {
            dock.tooltip_cache.insert(key.clone(), buffer);
        }
    }
    let width = tooltip_width(painter, &label);
    let position = Point::from((
        anchor.loc.x + (anchor.size.w - width) / 2,
        anchor.loc.y - theme.hover_lift - TOOLTIP_GAP - TOOLTIP_HEIGHT,
    ));
    if let Some(buffer) = dock.tooltip_cache.get(&key)
        && let Ok(element) = MemoryRenderBufferRenderElement::from_buffer(
            renderer,
            position.to_f64().to_physical(scale),
            buffer,
            Some(alpha),
            None,
            None,
            Kind::Unspecified,
        )
    {
        elements.push(element);
    }

    let shadow_key = (width, output_scale);
    if let std::collections::hash_map::Entry::Vacant(entry) = dock.tooltip_shadow.entry(shadow_key)
        && let Some(buffer) = draw_el2_shadow(
            Size::from((width, TOOLTIP_HEIGHT)),
            TOOLTIP_RADIUS,
            output_scale,
        )
    {
        entry.insert(buffer);
    }
    if let Some(buffer) = dock.tooltip_shadow.get(&shadow_key) {
        let margin = el2_shadow_margin();
        let pos = Point::from((position.x - margin, position.y - margin));
        if let Ok(element) = MemoryRenderBufferRenderElement::from_buffer(
            renderer,
            pos.to_f64().to_physical(scale),
            buffer,
            Some(alpha),
            None,
            None,
            Kind::Unspecified,
        ) {
            elements.push(element);
        }
    }
}

fn el2_layers() -> Vec<glide_theme::elevation::ShadowLayer> {
    glide_theme::elevation::layers(
        glide_theme::elevation::Level::El2,
        glide_theme::Appearance::Dark,
    )
}

fn el2_shadow_margin() -> i32 {
    #[allow(clippy::cast_possible_truncation)]
    let widest = el2_layers()
        .iter()
        .map(|layer| layer.blur)
        .fold(0.0_f32, f32::max)
        .ceil() as i32;
    crate::paint::shadow_fade_margin(widest)
}

fn draw_el2_shadow(
    size: Size<i32, Logical>,
    radius: f32,
    output_scale: i32,
) -> Option<MemoryRenderBuffer> {
    let margin = el2_shadow_margin();
    let output_scale = scale_or_one(output_scale);
    let (w, h) = (size.w + margin * 2, size.h + margin * 2);
    let (pw, ph) = (w * output_scale, h * output_scale);
    let mut buf = vec![0u8; usize::try_from(pw * ph * 4).ok()?];

    let factor = scale_f(output_scale);
    let layers: Vec<_> = el2_layers()
        .into_iter()
        .map(|layer| crate::paint::ShadowLayerSpec {
            dy: layer.dy * factor,
            blur: layer.blur * factor,
            color: layer.color,
        })
        .collect();
    #[allow(clippy::cast_precision_loss)]
    crate::paint::draw_outset_shadow_and_border(
        &mut buf,
        pw,
        ph,
        factor * size.w as f32,
        factor * size.h as f32,
        &layers,
        0.0,
        Color([0.0, 0.0, 0.0, 0.0]),
        radius * factor,
        Some(radius * factor),
    );

    Some(buffer_from(&buf, w, h, output_scale))
}

fn tooltip_width(painter: &crate::paint::Painter, title: &str) -> i32 {
    #[allow(clippy::cast_possible_truncation)]
    let text = painter
        .text_width_styled(title, &glide_theme::TextStyle::CAP)
        .ceil() as i32;
    text + TOOLTIP_PAD_X * 2
}

fn draw_tooltip(
    painter: &crate::paint::Painter,
    theme: &DockTheme,
    title: &str,
    scale: i32,
) -> Option<MemoryRenderBuffer> {
    let width = tooltip_width(painter, title);
    let height = TOOLTIP_HEIGHT;
    let scale = scale_or_one(scale);
    let pixels = usize::try_from(width * scale * height * scale * 4).ok()?;
    let mut buf = vec![0u8; pixels];

    crate::paint::Painter::fill_panel_background(
        &mut buf,
        width,
        height,
        scale,
        theme.menu_background,
        TOOLTIP_RADIUS,
        [true; 4],
    );
    crate::paint::stroke_rounded_rect_over(
        &mut buf,
        width,
        height,
        0,
        0,
        width,
        height,
        scale,
        1.0,
        TOOLTIP_RADIUS,
        theme.menu_border,
    );
    painter.draw_label_over_styled_in_band(
        &mut buf,
        width,
        height,
        TOOLTIP_PAD_X,
        width - TOOLTIP_PAD_X,
        0,
        height,
        scale,
        crate::paint::TextAlign::Center,
        title,
        theme.menu_text.to_buffer_bytes(),
        &glide_theme::TextStyle::CAP,
    );

    Some(buffer_from(&buf, width, height, scale))
}

#[allow(clippy::too_many_arguments)]
fn push_shell<R>(
    dock: &mut Dock,
    renderer: &mut R,
    theme: &DockTheme,
    items: &[DockItem],
    item_rects: &[Rectangle<i32, Logical>],
    shell: Rectangle<i32, Logical>,
    scale: Scale<f64>,
    output_scale: i32,
    opacity: f32,
    backdrop: Option<&Backdrop<'_>>,
    elements: &mut Vec<MemoryRenderBufferRenderElement<R>>,
) where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Send + Clone + 'static,
{
    let separators: u64 = items
        .iter()
        .enumerate()
        .filter(|(index, item)| *index > 0 && item.separator_before)
        .fold(0, |acc, (index, _)| acc | (1u64 << (index % 64)));
    let shell_key = (shell.size.w, shell.size.h, output_scale);
    let backdrop_key = backdrop.map(|b| (b.generation, shell.loc.x, shell.loc.y));
    let shell_state = (backdrop_key, separators);
    if dock.shell_backdrop_key != shell_state {
        dock.shell_backdrop_key = shell_state;
        dock.shell.clear();
    }
    if let std::collections::hash_map::Entry::Vacant(entry) = dock.shell.entry(shell_key)
        && let Some(buffer) = draw_shell(theme, items, item_rects, shell, output_scale, backdrop)
    {
        entry.insert(buffer);
    }
    if let Some(buffer) = dock.shell.get(&shell_key)
        && let Ok(element) = MemoryRenderBufferRenderElement::from_buffer(
            renderer,
            shell.loc.to_f64().to_physical(scale),
            buffer,
            Some(opacity),
            None,
            None,
            Kind::Unspecified,
        )
    {
        elements.push(element);
    }

    if let std::collections::hash_map::Entry::Vacant(entry) = dock.shadow.entry(shell_key)
        && let Some(buffer) = draw_shell_shadow(theme, shell, output_scale)
    {
        entry.insert(buffer);
    }
    if let Some(buffer) = dock.shadow.get(&shell_key) {
        let margin = shell_shadow_margin();
        let pos = Point::from((shell.loc.x - margin, shell.loc.y - margin))
            .to_f64()
            .to_physical(scale);
        if let Ok(element) = MemoryRenderBufferRenderElement::from_buffer(
            renderer,
            pos,
            buffer,
            Some(opacity),
            None,
            None,
            Kind::Unspecified,
        ) {
            elements.push(element);
        }
    }
}

fn push_menu<R>(
    dock: &mut Dock,
    renderer: &mut R,
    painter: &crate::paint::Painter,
    theme: &DockTheme,
    scale: Scale<f64>,
    output_scale: i32,
    elements: &mut Vec<MemoryRenderBufferRenderElement<R>>,
) where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Send + Clone + 'static,
{
    let Some(menu) = dock.menu.clone() else {
        return;
    };
    let progress = reveal_progress(&dock.menu_reveal, menu_anim());
    if progress <= 0.0 {
        return;
    }
    let placement = MenuPlacement::new(menu.rect, progress);
    let title = menu.title.clone();

    let row_alphas: Vec<f32> = (0..menu.items.len())
        .map(|row| {
            #[allow(clippy::cast_possible_truncation)]
            {
                (dock.menu_row_alpha(row).clamp(0.0, 1.0) as f32) * placement.alpha
            }
        })
        .collect();
    if row_alphas.iter().any(|alpha| *alpha > 0.0) {
        if let std::collections::hash_map::Entry::Vacant(entry) =
            dock.menu_row_layer.entry(output_scale)
            && let Some(buffer) = draw_menu_row_layer(theme, output_scale)
        {
            entry.insert(buffer);
        }
        if let Some(buffer) = dock.menu_row_layer.get(&output_scale) {
            for (row, alpha) in row_alphas.iter().enumerate() {
                if *alpha <= 0.0 {
                    continue;
                }
                let row_rect = crate::popup_menu::row_rect(menu.rect, row);
                let (pos, size) = placement.map(row_rect);
                let src = Rectangle::from_size(row_rect.size.to_f64());
                if let Ok(element) = MemoryRenderBufferRenderElement::from_buffer(
                    renderer,
                    pos.to_physical(scale),
                    buffer,
                    Some(*alpha),
                    Some(src),
                    Some(size),
                    Kind::Unspecified,
                ) {
                    elements.push(element);
                }
            }
        }
    }

    let key = (title.clone(), menu.items.clone(), output_scale);
    if !dock.menu_cache.contains_key(&key) {
        dock.menu_cache.clear();
        let colors = crate::popup_menu::MenuColors::from_dock(theme);
        let buffer = crate::popup_menu::draw(
            painter,
            &colors,
            &title,
            &menu.rows(),
            None,
            output_scale,
        );
        if let Some(buffer) = buffer {
            dock.menu_cache.insert(key.clone(), buffer);
        }
    }
    let (menu_pos, menu_size) = placement.map(menu.rect);
    if let Some(buffer) = dock.menu_cache.get(&key)
        && let Ok(element) = MemoryRenderBufferRenderElement::from_buffer(
            renderer,
            menu_pos.to_physical(scale),
            buffer,
            Some(placement.alpha),
            Some(Rectangle::from_size(menu.rect.size.to_f64())),
            Some(menu_size),
            Kind::Unspecified,
        )
    {
        elements.push(element);
    }

    let shadow_key = (menu.rect.size.w, menu.rect.size.h, output_scale);
    if let std::collections::hash_map::Entry::Vacant(entry) = dock.menu_shadow.entry(shadow_key) {
        let buffer =
            crate::popup_menu::draw_shadow(menu.rect.size, crate::popup_menu::RADIUS, output_scale);
        if let Some(buffer) = buffer {
            entry.insert(buffer);
        }
    }
    if let Some(buffer) = dock.menu_shadow.get(&shadow_key) {
        let margin = shell_shadow_margin();
        let shadow_rect = Rectangle::new(
            Point::from((menu.rect.loc.x - margin, menu.rect.loc.y - margin)),
            Size::from((
                menu.rect.size.w + margin * 2,
                menu.rect.size.h + margin * 2,
            )),
        );
        let (pos, size) = placement.map(shadow_rect);
        if let Ok(element) = MemoryRenderBufferRenderElement::from_buffer(
            renderer,
            pos.to_physical(scale),
            buffer,
            Some(placement.alpha),
            Some(Rectangle::from_size(shadow_rect.size.to_f64())),
            Some(size),
            Kind::Unspecified,
        ) {
            elements.push(element);
        }
    }
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
            #[allow(clippy::cast_possible_truncation)]
            alpha: progress as f32,
        }
    }

    fn map(&self, rect: Rectangle<i32, Logical>) -> (Point<f64, Logical>, Size<i32, Logical>) {
        let rect = rect.to_f64();
        let x = self.anchor.x + (rect.loc.x - self.anchor.x) * self.scale;
        let y = self.anchor.y + self.rise + (rect.loc.y - self.anchor.y) * self.scale;
        #[allow(clippy::cast_possible_truncation)]
        let size = Size::from((
            (rect.size.w * self.scale).round().max(1.0) as i32,
            (rect.size.h * self.scale).round().max(1.0) as i32,
        ));
        (Point::from((x, y)), size)
    }
}

fn draw_menu_row_layer(theme: &DockTheme, scale: i32) -> Option<MemoryRenderBuffer> {
    let width = crate::popup_menu::WIDTH - crate::popup_menu::PADDING * 2;
    let height = crate::popup_menu::ROW_HEIGHT;
    let scale = scale_or_one(scale);
    let pixels = usize::try_from(width * scale * height * scale * 4).ok()?;
    let mut buf = vec![0u8; pixels];
    crate::paint::Painter::fill_panel_background(
        &mut buf,
        width,
        height,
        scale,
        theme.menu_item_hover,
        crate::popup_menu::ROW_RADIUS,
        [true; 4],
    );
    Some(buffer_from(&buf, width, height, scale))
}

fn shell_shadow_margin() -> i32 {
    crate::popup_menu::shadow_margin()
}

fn draw_shell_shadow(
    theme: &DockTheme,
    shell: Rectangle<i32, Logical>,
    output_scale: i32,
) -> Option<MemoryRenderBuffer> {
    crate::popup_menu::draw_shadow(shell.size, theme.radius, output_scale)
}

struct HoverGeometry {
    position: Point<f64, Logical>,
    side: i32,
    layer_alpha: f32,
}

impl HoverGeometry {
    fn lifted(theme: &DockTheme, position: Point<f64, Logical>) -> Self {
        #[allow(clippy::cast_possible_truncation)]
        let side = (f64::from(theme.item_size) * theme.hover_scale).round() as i32;
        let inset = f64::from(side - theme.item_size) / 2.0;
        Self {
            position: Point::from((position.x - inset, position.y - inset)),
            side,
            layer_alpha: 1.0,
        }
    }

    fn for_tile(
        theme: &DockTheme,
        rect: Rectangle<i32, Logical>,
        is_hovered: bool,
        is_launcher: bool,
        hover_t: f64,
        layer_alpha: f64,
    ) -> Self {
        #[allow(clippy::cast_possible_truncation)]
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

        #[allow(clippy::cast_possible_truncation)]
        let lift = (f64::from(target_lift) * hover_t).round() as i32;
        #[allow(clippy::cast_possible_truncation)]
        let side =
            (f64::from(theme.item_size) * (1.0 + (target_scale - 1.0) * hover_t)).round() as i32;

        let inset = f64::from(side - theme.item_size) / 2.0;
        Self {
            position: Point::from((
                f64::from(rect.loc.x) - inset,
                f64::from(rect.loc.y) - inset - f64::from(lift),
            )),
            side,
            layer_alpha,
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn push_tile<R>(
    dock: &mut Dock,
    renderer: &mut R,
    index: usize,
    theme: &DockTheme,
    hover: &HoverGeometry,
    press_alpha: f64,
    scale: Scale<f64>,
    output_scale: i32,
    opacity: f32,
    elements: &mut Vec<MemoryRenderBufferRenderElement<R>>,
) where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Send + Clone + 'static,
{
    push_tile_layer(
        dock,
        renderer,
        theme,
        hover,
        scale,
        output_scale,
        opacity,
        elements,
    );
    push_press_layer(
        dock,
        renderer,
        theme,
        hover,
        press_alpha,
        scale,
        output_scale,
        opacity,
        elements,
    );

    if !dock.tiles[index].contains_key(&output_scale) {
        let icon = {
            let item = &dock.items[index];
            let glyph_side = theme.item_size * scale_or_one(output_scale);
            dock.icons
                .load(&item.app_id, glyph_side)
                .map(<[u8]>::to_vec)
        };
        if let Some(buffer) = draw_tile(theme, &dock.items[index], icon.as_deref(), output_scale) {
            dock.tiles[index].insert(output_scale, buffer);
        }
    }
    let Some(buffer) = dock.tiles[index].get(&output_scale) else {
        return;
    };

    let src = Rectangle::from_size(Size::from((
        f64::from(theme.item_size),
        f64::from(theme.item_size),
    )));
    if let Ok(element) = MemoryRenderBufferRenderElement::from_buffer(
        renderer,
        hover.position.to_physical(scale),
        buffer,
        Some(opacity),
        Some(src),
        Some(Size::from((hover.side, hover.side))),
        Kind::Unspecified,
    ) {
        elements.push(element);
    }
}

#[allow(clippy::too_many_arguments)]
fn push_tile_layer<R>(
    dock: &mut Dock,
    renderer: &mut R,
    theme: &DockTheme,
    hover: &HoverGeometry,
    scale: Scale<f64>,
    output_scale: i32,
    opacity: f32,
    elements: &mut Vec<MemoryRenderBufferRenderElement<R>>,
) where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Send + Clone + 'static,
{
    if hover.layer_alpha <= 0.0 {
        return;
    }

    if let std::collections::hash_map::Entry::Vacant(entry) = dock.tile_layer.entry(output_scale)
        && let Some(buffer) = draw_tile_layer(theme, output_scale)
    {
        entry.insert(buffer);
    }
    let Some(buffer) = dock.tile_layer.get(&output_scale) else {
        return;
    };

    let src = Rectangle::from_size(Size::from((
        f64::from(theme.item_size),
        f64::from(theme.item_size),
    )));
    if let Ok(element) = MemoryRenderBufferRenderElement::from_buffer(
        renderer,
        hover.position.to_physical(scale),
        buffer,
        Some(opacity * hover.layer_alpha),
        Some(src),
        Some(Size::from((hover.side, hover.side))),
        Kind::Unspecified,
    ) {
        elements.push(element);
    }
}

fn draw_tile_layer(theme: &DockTheme, scale: i32) -> Option<MemoryRenderBuffer> {
    let side = theme.item_size;
    let scale = scale_or_one(scale);
    let pixels = usize::try_from(side * scale * side * scale * 4).ok()?;
    let mut buf = vec![0u8; pixels];
    crate::paint::Painter::fill_panel_background(
        &mut buf,
        side,
        side,
        scale,
        theme.hover_layer,
        theme.item_radius,
        [true; 4],
    );
    Some(buffer_from(&buf, side, side, scale))
}

#[allow(clippy::too_many_arguments)]
fn push_press_layer<R>(
    dock: &mut Dock,
    renderer: &mut R,
    theme: &DockTheme,
    hover: &HoverGeometry,
    press_alpha: f64,
    scale: Scale<f64>,
    output_scale: i32,
    opacity: f32,
    elements: &mut Vec<MemoryRenderBufferRenderElement<R>>,
) where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Send + Clone + 'static,
{
    if press_alpha <= 0.0 {
        return;
    }
    #[allow(clippy::cast_possible_truncation)]
    let press_alpha = press_alpha.clamp(0.0, 1.0) as f32;

    if let std::collections::hash_map::Entry::Vacant(entry) = dock.press_layer.entry(output_scale)
        && let Some(buffer) = draw_press_layer(theme, output_scale)
    {
        entry.insert(buffer);
    }
    let Some(buffer) = dock.press_layer.get(&output_scale) else {
        return;
    };

    let src = Rectangle::from_size(Size::from((
        f64::from(theme.item_size),
        f64::from(theme.item_size),
    )));
    if let Ok(element) = MemoryRenderBufferRenderElement::from_buffer(
        renderer,
        hover.position.to_physical(scale),
        buffer,
        Some(opacity * press_alpha),
        Some(src),
        Some(Size::from((hover.side, hover.side))),
        Kind::Unspecified,
    ) {
        elements.push(element);
    }
}

fn press_layer_color(theme: &DockTheme) -> Color {
    let [r, g, b, _] = theme.hover_layer.0;
    Color([r, g, b, glide_theme::motion::ST_PRESS])
}

fn draw_press_layer(theme: &DockTheme, scale: i32) -> Option<MemoryRenderBuffer> {
    let side = theme.item_size;
    let scale = scale_or_one(scale);
    let pixels = usize::try_from(side * scale * side * scale * 4).ok()?;
    let mut buf = vec![0u8; pixels];
    crate::paint::Painter::fill_panel_background(
        &mut buf,
        side,
        side,
        scale,
        press_layer_color(theme),
        PRESS_RADIUS,
        [true; 4],
    );
    Some(buffer_from(&buf, side, side, scale))
}

#[allow(clippy::too_many_arguments)]
fn push_run_indicator<R>(
    dock: &mut Dock,
    renderer: &mut R,
    index: usize,
    theme: &DockTheme,
    rect: Rectangle<i32, Logical>,
    run_alpha: f64,
    scale: Scale<f64>,
    output_scale: i32,
    opacity: f32,
    elements: &mut Vec<MemoryRenderBufferRenderElement<R>>,
) where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Send + Clone + 'static,
{
    if run_alpha <= 0.0 {
        return;
    }
    #[allow(clippy::cast_possible_truncation)]
    let run_alpha = run_alpha.clamp(0.0, 1.0) as f32;
    let state = dock.run.get(index).copied().unwrap_or_default();
    let width = dock.run_width(index);

    let key = (width, state.focused, output_scale);
    if let std::collections::hash_map::Entry::Vacant(entry) = dock.runs.entry(key)
        && let Some(buffer) = draw_run_indicator(theme, width, state.focused, output_scale)
    {
        entry.insert(buffer);
    }
    let Some(buffer) = dock.runs.get(&key) else {
        return;
    };

    let pos = Point::from((
        rect.loc.x + (theme.item_size - width) / 2,
        rect.loc.y + theme.item_size + RUN_GAP,
    ));
    if let Ok(element) = MemoryRenderBufferRenderElement::from_buffer(
        renderer,
        pos.to_f64().to_physical(scale),
        buffer,
        Some(opacity * run_alpha),
        None,
        None,
        Kind::Unspecified,
    ) {
        elements.push(element);
    }
}

fn buffer_from(buf: &[u8], width: i32, height: i32, scale: i32) -> MemoryRenderBuffer {
    MemoryRenderBuffer::from_slice(
        buf,
        smithay::backend::allocator::Fourcc::Argb8888,
        (width * scale, height * scale),
        scale,
        Transform::Normal,
        None,
    )
}

fn draw_shell(
    theme: &DockTheme,
    items: &[DockItem],
    item_rects: &[Rectangle<i32, Logical>],
    shell: Rectangle<i32, Logical>,
    scale: i32,
    backdrop: Option<&Backdrop<'_>>,
) -> Option<MemoryRenderBuffer> {
    let (w, h) = (shell.size.w, shell.size.h);
    let pixels = usize::try_from(w * scale * h * scale * 4).ok()?;
    let mut buf = vec![0u8; pixels];

    let has_backdrop = backdrop.is_some_and(|source| {
        source.blit_into(
            &mut buf,
            w * scale,
            h * scale,
            theme.radius * scale_f(scale),
            shell,
        )
    });

    if has_backdrop {
        crate::paint::Painter::fill_rounded_rect_over(
            &mut buf,
            w,
            h,
            0,
            0,
            w,
            h,
            scale,
            theme.background,
            theme.radius,
        );
    } else {
        crate::paint::Painter::fill_panel_background(
            &mut buf,
            w,
            h,
            scale,
            theme.background,
            theme.radius,
            [true; 4],
        );
    }
    #[allow(clippy::cast_precision_loss)]
    crate::paint::draw_inset_border(
        &mut buf,
        w * scale,
        h * scale,
        scale as f32,
        theme.radius * scale as f32,
        theme.border,
    );

    for (index, item) in items.iter().enumerate() {
        if index == 0 || !item.separator_before {
            continue;
        }
        let Some(rect) = item_rects.get(index) else {
            continue;
        };
        let x = rect.loc.x - shell.loc.x - SEPARATOR_MARGIN - SEPARATOR_WIDTH;
        let height = SEPARATOR_HEIGHT;
        let y = theme.padding_y + (theme.item_size - height) / 2;
        crate::paint::Painter::fill_rounded_rect_over(
            &mut buf,
            w,
            h,
            x,
            y,
            SEPARATOR_WIDTH,
            height,
            scale,
            theme.separator,
            #[allow(clippy::cast_precision_loss)]
            {
                SEPARATOR_WIDTH as f32 / 2.0
            },
        );
    }

    Some(buffer_from(&buf, w, h, scale))
}

fn scale_or_one(scale: i32) -> i32 {
    scale.max(1)
}

#[allow(clippy::cast_precision_loss)]
fn scale_f(scale: i32) -> f32 {
    scale as f32
}

pub struct Backdrop<'a> {
    pub pixels: &'a [u8],
    pub width: i32,
    pub height: i32,
    pub fractional_scale: f64,
    pub generation: u64,
}

impl Backdrop<'_> {
    pub fn blit(
        &self,
        dst: &mut [u8],
        width: i32,
        height: i32,
        radius: f32,
        rect: Rectangle<i32, Logical>,
        scale: i32,
    ) -> bool {
        self.blit_into(
            dst,
            width * scale,
            height * scale,
            radius * scale_f(scale),
            rect,
        )
    }

    fn blit_into(
        &self,
        dst: &mut [u8],
        dst_w: i32,
        dst_h: i32,
        radius: f32,
        rect: Rectangle<i32, Logical>,
    ) -> bool {
        #[allow(clippy::cast_possible_truncation)]
        let to_src = |v: i32| (f64::from(v) * self.fractional_scale).round() as i32;
        glide_paint::blit_rounded_backdrop(
            dst,
            dst_w,
            dst_h,
            radius,
            self.pixels,
            self.width,
            self.height,
            (
                to_src(rect.loc.x),
                to_src(rect.loc.y),
                to_src(rect.size.w).max(1),
                to_src(rect.size.h).max(1),
            ),
        )
    }
}

fn draw_tile(
    theme: &DockTheme,
    item: &DockItem,
    icon: Option<&[u8]>,
    scale: i32,
) -> Option<MemoryRenderBuffer> {
    let side = theme.item_size;
    let scale = scale_or_one(scale);
    let pixels = usize::try_from(side * scale * side * scale * 4).ok()?;
    let mut buf = vec![0u8; pixels];

    if let Some(icon) = icon {
        let n = icon.len().min(buf.len());
        buf[..n].copy_from_slice(&icon[..n]);
        return Some(buffer_from(&buf, side, side, scale));
    }

    crate::paint::fill_rounded_rect_gradient_over(
        &mut buf,
        side,
        side,
        0,
        0,
        side,
        side,
        scale,
        item.color_from,
        item.color_to,
        theme.item_radius,
    );
    let icon_color = if item.action == DockAction::OpenLauncher {
        theme.running_color
    } else {
        theme.icon
    };
    crate::paint::draw_dock_glyph(&mut buf, side, side, scale, item.glyph.into(), icon_color);

    Some(buffer_from(&buf, side, side, scale))
}

fn draw_run_indicator(
    theme: &DockTheme,
    width: i32,
    focused: bool,
    scale: i32,
) -> Option<MemoryRenderBuffer> {
    let height = RUN_THICKNESS;
    let pixels = usize::try_from(width * scale * height * scale * 4).ok()?;
    let mut buf = vec![0u8; pixels];

    let color = if focused {
        theme.focused_color
    } else {
        let [r, g, b, a] = theme.running_color.0;
        Color([r, g, b, a * 0.85])
    };
    #[allow(clippy::cast_precision_loss)]
    crate::paint::Painter::fill_rounded_rect_over(
        &mut buf,
        width,
        height,
        0,
        0,
        width,
        height,
        scale,
        color,
        height as f32 / 2.0,
    );

    Some(buffer_from(&buf, width, height, scale))
}

impl From<Glyph> for crate::paint::DockGlyph {
    fn from(glyph: Glyph) -> Self {
        match glyph {
            Glyph::Terminal => Self::Terminal,
            Glyph::Files => Self::Files,
            Glyph::Settings => Self::Settings,
            Glyph::Music => Self::Music,
            Glyph::Mail => Self::Mail,
            Glyph::Generic => Self::Generic,
            Glyph::Grid => Self::Grid,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(count: usize) -> DockConfig {
        DockConfig {
            theme: DockTheme::default(),
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

    mod pinning {
        use super::*;

        fn running(app_id: &str) -> RunningApp {
            RunningApp {
                name: app_id.to_string(),
                app_id: app_id.to_string(),
                focused: false,
            }
        }

        fn dock_with_one_running() -> Dock {
            let mut dock = Dock::new(&config(1));
            dock.set_running(&[running("firefox")]);
            dock
        }

        fn index_of(dock: &Dock, app_id: &str) -> usize {
            dock.items()
                .iter()
                .position(|item| item.app_id == app_id)
                .expect("the app is on the dock")
        }

        #[test]
        fn a_running_app_can_be_pinned_and_survives_the_window_closing() {
            let mut dock = dock_with_one_running();
            let index = index_of(&dock, "firefox");

            assert!(
                dock.toggle_pinned(index, Some(("Firefox".to_string(), "firefox".to_string())))
            );
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
            dock.toggle_pinned(
                index,
                Some(("Firefox".to_string(), "firefox --new-window".to_string())),
            );

            let pinned = dock
                .pinned_items()
                .iter()
                .find(|item| item.app_id == "firefox")
                .expect("just pinned");
            assert_eq!(pinned.command, "firefox --new-window");
            assert_eq!(
                pinned.name, "Firefox",
                "the pin takes the desktop entry's name, not the window title"
            );
        }

        #[test]
        fn the_menu_offers_pin_for_a_running_unpinned_app_and_unpin_for_a_pinned_one() {
            let dock = dock_with_one_running();
            let index = index_of(&dock, "firefox");
            let items = dock.menu_items(index);
            assert!(items.contains(&DockMenuItem::Pin));
            assert!(items.contains(&DockMenuItem::CloseAll));
            assert!(!items.contains(&DockMenuItem::Open));

            let pinned_idle = index_of(&dock, "app 0");
            let items = dock.menu_items(pinned_idle);
            assert_eq!(items, vec![DockMenuItem::Open, DockMenuItem::Unpin]);
        }

        #[test]
        fn an_app_pinned_from_the_grid_gets_a_tile_and_can_be_unpinned_by_id() {
            let mut dock = Dock::new(&config(1));
            assert!(dock.pin_app("org.example.Editor", "Editor", "editor"));
            assert!(
                !dock.pin_app("org.example.editor", "Editor", "editor"),
                "already pinned"
            );
            assert!(dock.is_app_pinned("ORG.EXAMPLE.EDITOR"));
            dock.set_running(&[]);
            assert!(
                dock.items()
                    .iter()
                    .any(|item| item.app_id == "org.example.Editor")
            );

            assert!(dock.unpin_app("org.example.editor"));
            assert!(!dock.unpin_app("org.example.editor"));
            dock.set_running(&[]);
            assert!(
                !dock
                    .items()
                    .iter()
                    .any(|item| item.app_id == "org.example.Editor")
            );
        }

        #[test]
        fn the_launcher_tile_has_no_menu() {
            let mut dock = Dock::new(&config(1));
            let last = dock.items().len() - 1;
            assert!(dock.menu_items(last).is_empty());
            assert!(!dock.open_menu(last, area()));
        }

        #[test]
        fn the_menu_opens_above_its_tile_and_hit_tests_its_rows() {
            let mut dock = dock_with_one_running();
            let index = index_of(&dock, "firefox");
            assert!(dock.open_menu(index, area()));
            let menu = dock.menu_rect().expect("menu rect");
            let tile = dock.item_rects(area())[index];
            assert!(menu.loc.y + menu.size.h < tile.loc.y);
            let inside = Point::from((f64::from(menu.loc.x) + 1.0, f64::from(menu.loc.y) + 1.0));
            assert!(dock.contains(area(), inside));

            let items = dock.menu_items(index);
            for (row, item) in items.iter().enumerate() {
                let rect = crate::popup_menu::row_rect(dock.menu_rect().expect("open"), row);
                let centre = Point::from((
                    f64::from(rect.loc.x + rect.size.w / 2),
                    f64::from(rect.loc.y + rect.size.h / 2),
                ));
                assert_eq!(dock.menu_item_at(centre), Some(*item));
                assert!(dock.set_menu_hover(Some(centre)));
                assert!(!dock.set_menu_hover(Some(centre)));
            }
            assert!(dock.close_menu());
            assert!(!dock.close_menu());
        }

        fn centre_of(rect: Rectangle<i32, Logical>) -> Point<f64, Logical> {
            Point::from((
                f64::from(rect.loc.x) + f64::from(rect.size.w) / 2.0,
                f64::from(rect.loc.y) + f64::from(rect.size.h) / 2.0,
            ))
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
            assert_eq!(
                dock.end_press(area(), far),
                PressOutcome::Drop {
                    index: 0,
                    target: 2
                }
            );
            assert!(!dock.is_dragging());
        }

        #[test]
        fn releasing_outside_the_pressed_tile_is_not_a_click() {
            let mut dock = Dock::new(&config(2));
            let rects = dock.item_rects(area());
            dock.begin_press(0, centre_of(rects[0]), area());
            assert_eq!(
                dock.end_press(area(), Point::from((10.0, 10.0))),
                PressOutcome::Nothing
            );
        }

        #[test]
        fn the_launcher_tile_cannot_be_dragged() {
            let mut dock = Dock::new(&config(1));
            let last = dock.items().len() - 1;
            let rects = dock.item_rects(area());
            dock.begin_press(last, centre_of(rects[last]), area());
            assert!(!dock.drag_motion(Point::from((0.0, 0.0))));
            assert_eq!(
                dock.end_press(area(), Point::from((0.0, 0.0))),
                PressOutcome::Nothing
            );
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
            let names: Vec<_> = dock
                .pinned_items()
                .iter()
                .map(|item| item.app_id.clone())
                .collect();
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
            dock.drag_motion(Point::from((
                f64::from(rects[0].loc.x),
                centre_of(rects[0]).y,
            )));
            let (shown, order) = dock.display_order(area()).expect("dragging");
            assert_eq!(order[0], firefox);
            assert!(!shown[0].separator_before);
            assert!(
                !shown[1].separator_before,
                "the old dynamic group has nothing left to separate"
            );
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
            assert!(dock.open_menu(index, area()));
            dock.set_running(&[]);
            assert!(dock.menu_rect().is_none());
        }

        #[test]
        fn pinning_twice_unpins() {
            let mut dock = dock_with_one_running();
            let index = index_of(&dock, "firefox");
            dock.toggle_pinned(index, None);
            dock.set_running(&[running("firefox")]);

            let index = index_of(&dock, "firefox");
            assert!(dock.toggle_pinned(index, None));
            assert!(
                !dock.pinned_items().iter().any(|i| i.app_id == "firefox"),
                "the second toggle must remove the pin"
            );

            dock.set_running(&[]);
            assert!(!dock.items().iter().any(|item| item.app_id == "firefox"));
        }

        #[test]
        fn the_launcher_button_cannot_be_pinned() {
            let mut dock = Dock::new(&config(1));
            let last = dock.items().len() - 1;
            assert_eq!(dock.action(last), Some(DockAction::OpenLauncher));
            assert!(!dock.toggle_pinned(last, None));
        }

        #[test]
        fn an_out_of_range_index_is_refused() {
            let mut dock = Dock::new(&config(1));
            assert!(!dock.toggle_pinned(999, None));
        }

        #[test]
        fn unpinning_a_configured_item_is_allowed_too() {
            let mut dock = Dock::new(&config(2));
            let before = dock.pinned_items().len();
            assert!(dock.toggle_pinned(0, None));
            assert_eq!(dock.pinned_items().len(), before - 1);
        }
    }

    mod slides {
        use super::*;

        fn targets<'a>(pairs: &[(&'a str, i32)]) -> Vec<(&'a str, i32)> {
            pairs.to_vec()
        }

        #[test]
        fn a_slot_change_eases_from_the_previous_x_and_keeps_the_frame_pump_alive() {
            let mut dock = Dock::new(&config(1));
            let start = Instant::now();
            let duration = Duration::from_millis(200);

            let first = dock.settle_slides(&targets(&[("a", 100)]), None, start, duration);
            assert_eq!(
                first,
                vec![100.0],
                "a tile's first appearance does not slide"
            );
            assert!(!dock.is_animating());

            let moved = dock.settle_slides(&targets(&[("a", 160)]), None, start, duration);
            assert_eq!(
                moved,
                vec![100.0],
                "the slide starts where the tile was drawn"
            );
            assert!(
                dock.is_animating(),
                "a slide in progress must keep frames coming"
            );

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
            dock.settle_slides(
                &targets(&[("app 0", 100), ("firefox", 160)]),
                None,
                start,
                duration,
            );
            dock.settle_slides(
                &targets(&[("firefox", 100), ("app 0", 160)]),
                None,
                start,
                duration,
            );
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

            let glued =
                dock.settle_slides(&targets(&[("a", 100)]), Some((0, 300.5)), start, duration);
            assert_eq!(
                glued,
                vec![300.5],
                "no easing while the pointer holds the tile"
            );
            assert!(!dock.is_animating());

            let dropped = dock.settle_slides(&targets(&[("a", 100)]), None, start, duration);
            assert!(
                (dropped[0] - 300.5).abs() < 1.0,
                "it sets off from the drop point"
            );
            assert!(dock.is_animating());
        }
    }

    mod hiding {
        fn hidden_for(overview: f64, launcher: f64) -> f64 {
            overview.max(launcher).clamp(0.0, 1.0)
        }

        #[test]
        fn a_fully_open_launcher_hides_the_dock_completely() {
            assert!(
                hidden_for(0.0, 1.0) >= 1.0,
                "the dock must not float over the app grid"
            );
        }

        #[test]
        fn a_half_open_launcher_slides_the_dock_halfway_out() {
            let hidden = hidden_for(0.0, 0.5);
            assert!(hidden > 0.0 && hidden < 1.0);
        }

        #[test]
        fn the_overview_still_hides_the_dock_on_its_own() {
            assert!(hidden_for(1.0, 0.0) >= 1.0);
        }

        #[test]
        fn nothing_open_leaves_the_dock_alone() {
            assert!(hidden_for(0.0, 0.0).abs() < f64::EPSILON);
        }
    }

    mod running_indicator_band {
        use super::*;

        #[test]
        fn the_indicator_sits_clear_of_the_icon_and_the_shell_edge() {
            let dock = Dock::new(&config(2));
            let theme = &dock.theme;
            let shell = dock.rect(area());
            let item = dock.item_rects(area())[0];

            let indicator_top = item.loc.y + theme.item_size + RUN_GAP;
            let indicator_bottom = indicator_top + RUN_THICKNESS;
            let shell_bottom = shell.loc.y + shell.size.h;

            assert!(
                indicator_top >= item.loc.y + theme.item_size,
                "the indicator must not overlap the icon"
            );
            assert!(
                shell_bottom - indicator_bottom >= RUN_GAP,
                "only {}px between the indicator and the shell edge; it used to be \
                 clipped by the rounded corner",
                shell_bottom - indicator_bottom
            );
        }

        #[test]
        fn the_shell_reserves_the_band_rather_than_growing_the_icons() {
            let dock = Dock::new(&config(2));
            let theme = &dock.theme;
            assert_eq!(
                dock.size().h,
                theme.padding_y * 2 + theme.item_size + RUN_BAND
            );
        }

        #[test]
        fn every_indicator_width_stays_inside_its_icon() {
            let dock = Dock::new(&config(2));
            for width in [RUN_DOT, RUN_BAR] {
                assert!(
                    width <= dock.theme.item_size,
                    "an indicator wider than the icon would break the centring"
                );
            }
        }
    }

    #[test]
    fn a_dock_with_nothing_pinned_still_offers_the_launcher() {
        let dock = Dock::new(&config(0));
        assert!(dock.is_enabled());
        assert_eq!(dock.items().len(), 1);
        assert_eq!(dock.action(0), Some(DockAction::OpenLauncher));
    }

    #[test]
    fn the_launcher_button_is_always_last() {
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
            assert_eq!(
                pair[1].loc.x - (pair[0].loc.x + pair[0].size.w),
                theme.item_gap
            );
        }
        for rect in &rects {
            assert!(rect.loc.x >= shell.loc.x + theme.padding_x);
            assert!(rect.loc.x + rect.size.w <= shell.loc.x + shell.size.w - theme.padding_x);
            assert_eq!(rect.size, Size::from((theme.item_size, theme.item_size)));
        }
    }

    #[test]
    fn a_separator_widens_the_dock_by_its_own_footprint() {
        let plain = Dock::new(&config(3));
        let mut with_divider = config(3);
        with_divider.items[2].separator_before = true;
        let divided = Dock::new(&with_divider);

        assert_eq!(
            divided.size().w - plain.size().w,
            SEPARATOR_WIDTH + SEPARATOR_MARGIN * 2
        );
    }

    #[test]
    fn a_leading_separator_is_ignored() {
        let mut leading = config(3);
        leading.items[0].separator_before = true;
        assert_eq!(Dock::new(&leading).size(), Dock::new(&config(3)).size());
    }

    mod minimize_target {
        use super::*;

        #[test]
        fn a_pinned_app_resolves_to_its_own_tile() {
            let mut config = config(3);
            config.items[1].app_id = Some("org.example.Editor".to_string());
            let dock = Dock::new(&config);

            let tile = dock
                .tile_rect_for_app(area(), "org.example.Editor")
                .expect("the app is pinned, so it has a tile");
            assert_eq!(tile, dock.item_rects(area())[1]);
        }

        #[test]
        fn the_lookup_ignores_case() {
            let mut config = config(1);
            config.items[0].app_id = Some("Alacritty".to_string());
            let dock = Dock::new(&config);
            assert!(dock.tile_rect_for_app(area(), "alacritty").is_some());
        }

        #[test]
        fn an_app_with_no_tile_resolves_to_nothing() {
            let dock = Dock::new(&config(2));
            assert_eq!(dock.tile_rect_for_app(area(), "org.example.Absent"), None);
        }

        #[test]
        fn a_running_only_app_still_gets_a_tile_to_fly_to() {
            let mut dock = Dock::new(&config(1));
            dock.set_running(&[RunningApp {
                app_id: "org.example.Editor".to_string(),
                name: "Editor".to_string(),
                focused: true,
            }]);
            assert!(
                dock.tile_rect_for_app(area(), "org.example.Editor")
                    .is_some()
            );
        }
    }

    mod hit_testing {
        use super::*;

        fn centre(rect: Rectangle<i32, Logical>) -> Point<f64, Logical> {
            Point::from((
                f64::from(rect.loc.x) + f64::from(rect.size.w) / 2.0,
                f64::from(rect.loc.y) + f64::from(rect.size.h) / 2.0,
            ))
        }

        #[test]
        fn every_tile_reports_its_own_index() {
            let dock = Dock::new(&config(4));
            for (index, rect) in dock.item_rects(area()).into_iter().enumerate() {
                assert_eq!(dock.item_at(area(), centre(rect)), Some(index));
            }
        }

        #[test]
        fn the_gap_between_tiles_hits_nothing() {
            let dock = Dock::new(&config(2));
            let rects = dock.item_rects(area());
            let gap_x = f64::from(rects[0].loc.x + rects[0].size.w) + 1.0;
            let y = centre(rects[0]).y;
            assert_eq!(dock.item_at(area(), Point::from((gap_x, y))), None);
        }

        #[test]
        fn a_point_far_from_the_dock_is_not_over_it() {
            let dock = Dock::new(&config(3));
            assert!(!dock.contains(area(), Point::from((10.0, 10.0))));
            assert!(dock.contains(area(), centre(dock.rect(area()))));
        }
    }

    mod running_apps {
        use super::*;

        fn app(id: &str, focused: bool) -> RunningApp {
            RunningApp {
                app_id: id.to_string(),
                name: id.to_string(),
                focused,
            }
        }

        #[test]
        fn a_pinned_app_gains_a_dot_without_adding_a_tile() {
            let mut dock = Dock::new(&config(2));
            let pinned_id = dock.items()[0].app_id.clone();

            assert!(dock.set_running(&[app(&pinned_id, true)]));
            assert_eq!(dock.items().len(), 3, "pinned pair plus the launcher");
            assert!(dock.run[0].running && dock.run[0].focused);
            assert!(!dock.run[1].running);
        }

        #[test]
        fn an_unpinned_running_app_gets_its_own_tile() {
            let mut dock = Dock::new(&config(1));
            assert!(dock.set_running(&[app("org.example.Editor", true)]));

            assert_eq!(
                dock.items().len(),
                3,
                "pinned, the new app, then the launcher"
            );
            let added = &dock.items()[1];
            assert_eq!(added.app_id, "org.example.Editor");
            assert!(added.separator_before, "it must start a new group");
            assert!(dock.run[1].running && dock.run[1].focused);
            assert!(!dock.is_pinned(1));
            assert!(dock.is_pinned(0));
        }

        #[test]
        fn closing_the_last_window_removes_a_dynamic_tile_again() {
            let mut dock = Dock::new(&config(1));
            dock.set_running(&[app("org.example.Editor", true)]);
            assert_eq!(dock.items().len(), 3);

            assert!(dock.set_running(&[]));
            assert_eq!(dock.items().len(), 2, "the tile must go with the window");
        }

        #[test]
        fn matching_a_pinned_app_ignores_case() {
            let mut config = config(1);
            config.items[0].app_id = Some("Alacritty".to_string());
            let mut dock = Dock::new(&config);

            dock.set_running(&[app("alacritty", false)]);
            assert_eq!(dock.items().len(), 2, "it is the same app");
            assert!(dock.run[0].running);
        }

        #[test]
        fn repeating_the_same_set_is_a_no_op() {
            let mut dock = Dock::new(&config(1));
            let running = [app("org.example.Editor", false)];
            assert!(dock.set_running(&running));
            assert!(!dock.set_running(&running), "nothing changed");
        }

        #[test]
        fn a_dock_with_no_pins_still_shows_running_apps() {
            let mut dock = Dock::new(&config(0));

            dock.set_running(&[app("org.example.Editor", true)]);
            assert_eq!(dock.items().len(), 2, "the app, then the launcher");
            assert_eq!(dock.items()[0].app_id, "org.example.Editor");
            assert!(
                !dock.items()[0].separator_before,
                "nothing to separate it from"
            );
        }

        #[test]
        fn a_dynamic_tile_keeps_the_same_colours_across_rebuilds() {
            let first = dynamic_colors("org.example.Editor");
            let second = dynamic_colors("org.example.Editor");
            assert_eq!(first, second);
            assert_ne!(first, dynamic_colors("org.example.Other"));
        }

        #[test]
        fn a_hover_on_a_vanished_tile_is_dropped() {
            let mut dock = Dock::new(&config(1));
            dock.set_running(&[app("org.example.Editor", false)]);
            dock.set_hover(Some(dock.items().len() - 2));

            dock.set_running(&[]);
            assert_eq!(
                dock.hover.target(),
                None,
                "hovering a tile that no longer exists would index out of range"
            );
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
    fn launcher_item_has_subtle_state_background_and_grid_glyph() {
        let launcher = DockItem::launcher();
        assert_eq!(launcher.glyph, Glyph::Grid);
        assert_eq!(launcher.action, DockAction::OpenLauncher);
        assert!(launcher.separator_before);
        assert!((launcher.color_from.0[3] - 0.08).abs() < 0.01);
        assert_eq!(launcher.color_from, launcher.color_to);
    }

    #[test]
    fn hover_geometry_differs_between_launcher_and_standard_tiles() {
        let theme = DockTheme::default();
        let rect = Rectangle::new(
            Point::from((100, 200)),
            Size::from((theme.item_size, theme.item_size)),
        );

        let standard_hover = HoverGeometry::for_tile(&theme, rect, true, false, 1.0);
        let launcher_hover = HoverGeometry::for_tile(&theme, rect, true, true, 1.0);

        let standard_lift = f64::from(rect.loc.y)
            - (standard_hover.position.y + f64::from(standard_hover.side - theme.item_size) / 2.0);
        let launcher_lift = f64::from(rect.loc.y)
            - (launcher_hover.position.y + f64::from(launcher_hover.side - theme.item_size) / 2.0);

        assert!((standard_lift - 6.0).abs() < 0.01);
        assert!((launcher_lift - 4.0).abs() < 0.01);
    }
}

pub mod calc;
pub mod field;
mod glue;
pub mod history;
mod list;
pub mod providers;
pub mod recent;
pub mod search;

use std::time::{Duration, Instant};

use smithay::{
    backend::renderer::{
        ImportAll, ImportMem,
        element::{
            Kind,
            memory::{MemoryRenderBuffer, MemoryRenderBufferRenderElement},
            solid::{SolidColorBuffer, SolidColorRenderElement},
            utils::RescaleRenderElement,
        },
    },
    utils::{Logical, Physical, Point, Rectangle, Scale, Size},
};

use glide_theme::motion::{ease_io, ease_out};

use crate::{
    config::Color,
    icon::{AppCategory, DesktopApp, IconLoader},
    paint::TextAlign,
    raster::{Canvas, ShellRenderer, buffer_from, rect},
};

const MIN_COLUMNS: usize = 5;
const MAX_COLUMNS: usize = 9;
const MIN_ROWS: usize = 3;
const MAX_ROWS: usize = 5;

const ICON_SIZE: i32 = 80;

#[must_use]
pub const fn icon_side() -> i32 {
    ICON_SIZE
}
const CELL_WIDTH: i32 = 144;
const CELL_HEIGHT: i32 = 148;
const CELL_PAD_TOP: i32 = glide_theme::spacing::SP3;
const CAPTION_GAP: i32 = glide_theme::spacing::SP2;
const CAPTION_LINE: i32 = 18;
const CAPTION_LINES: i32 = 2;
const CAPTION_HEIGHT: i32 = CAPTION_LINE * CAPTION_LINES;
const CAPTION_PAD_X: i32 = glide_theme::spacing::SP2;
const HOVER_LIFT: f64 = 4.0;
const HOVER_SCALE: f64 = 1.06;
const CAPTION_PILL_PAD_X: i32 = glide_theme::spacing::SP2;

const SIDE_MARGIN: i32 = 128;
const TOP_MARGIN: i32 = glide_theme::spacing::SP7;
const DOTS_BAND: i32 = 64;

const DOT_SIZE: i32 = 8;
const DOT_ACTIVE_WIDTH: i32 = 22;
const DOT_GAP: i32 = glide_theme::spacing::SP2;
const DOT_MARGIN: i32 = 32;
const DOT_HIT_PAD: i32 = 12;

const SEARCH_HEIGHT: i32 = 42;
const SEARCH_WIDTH: i32 = 480;
const SEARCH_GAP: i32 = glide_theme::spacing::SP4;
const SEARCH_TEXT: glide_theme::TextStyle = glide_theme::TextStyle::BODY;

const CHIP_HEIGHT: i32 = 34;
const CHIP_STEPS: u8 = 24;
const CHIP_GAP: i32 = glide_theme::spacing::SP2;
const CHIP_PAD_X: i32 = glide_theme::spacing::SP4;
const CHIP_MIN_WIDTH: i32 = 64;
const CHIP_BAND_GAP: i32 = glide_theme::spacing::SP6;
const CHIP_TEXT: glide_theme::TextStyle = glide_theme::TextStyle::MENU_ITEM;

const SEARCH_BAND: i32 = TOP_MARGIN + SEARCH_HEIGHT + SEARCH_GAP + CHIP_HEIGHT + CHIP_BAND_GAP;
const SEARCH_PAD_X: i32 = glide_theme::spacing::SP4;
const SEARCH_GLYPH: f32 = 16.0;
const SEARCH_GLYPH_STROKE: f32 = 1.6;

const CELL_RADIUS: f32 = glide_theme::shape::R3;

const CLEAR_SIZE: i32 = 28;
const CLEAR_INSET: i32 = (SEARCH_HEIGHT - CLEAR_SIZE) / 2;
const CLEAR_GLYPH: f32 = 10.0;
const CLEAR_GLYPH_STROKE: f32 = 1.5;
const CARET_WIDTH: i32 = 2;
const CARET_HEIGHT: i32 = 20;

fn clear_rect(area: Rectangle<i32, Logical>) -> Rectangle<i32, Logical> {
    let field = search_rect(area);
    Rectangle::new(
        Point::from((
            field.loc.x + field.size.w - CLEAR_INSET - CLEAR_SIZE,
            field.loc.y + (SEARCH_HEIGHT - CLEAR_SIZE) / 2,
        )),
        Size::from((CLEAR_SIZE, CLEAR_SIZE)),
    )
}

fn search_text_left() -> i32 {
    let glyph = SEARCH_GLYPH as i32;
    SEARCH_PAD_X + glyph + glide_theme::spacing::SP3
}

fn search_text_right(width: i32, clear: bool) -> i32 {
    if clear { width - CLEAR_INSET - CLEAR_SIZE - glide_theme::spacing::SP1 } else { width - SEARCH_PAD_X }
}

const MENU_RISE: f64 = 6.0;
const MENU_SCALE: f64 = 0.94;

fn control_radius(height: i32) -> f32 {
    glide_theme::shape::R2.min(height as f32 / 2.0)
}

fn filled(width: i32, height: i32, scale: i32, fill: Color, radius: f32) -> Option<Canvas<'static>> {
    let mut c = Canvas::new(width, height, scale)?;
    c.panel(fill, radius);
    Some(c)
}

fn bordered(width: i32, height: i32, scale: i32, fill: Color, border: Color, radius: f32) -> Option<Canvas<'static>> {
    let mut c = filled(width, height, scale, fill, radius)?;
    let s = scale as f32;
    crate::paint::draw_inset_border(&mut c.buf, width * scale, height * scale, s, radius * s, border);
    Some(c)
}

fn cached<K: std::hash::Hash + Eq>(
    cache: &mut std::collections::HashMap<K, MemoryRenderBuffer>, key: K,
    draw: impl FnOnce() -> Option<MemoryRenderBuffer>,
) -> Option<&MemoryRenderBuffer> {
    match cache.entry(key) {
        std::collections::hash_map::Entry::Occupied(entry) => Some(entry.into_mut()),
        std::collections::hash_map::Entry::Vacant(entry) => draw().map(|buffer| &*entry.insert(buffer)),
    }
}

fn keyed<K: PartialEq>(slot: &mut Option<(K, ScaledBuffers)>, key: K) -> &mut ScaledBuffers {
    if slot.as_ref().is_some_and(|(cached, _)| *cached != key) {
        *slot = None;
    }
    &mut slot.get_or_insert_with(|| (key, ScaledBuffers::new())).1
}

struct Frame<'a, R>
where
    R: ShellRenderer,
{
    renderer: &'a mut R,
    painter: &'a crate::paint::Painter,
    theme: &'a LauncherTheme,
    area: Rectangle<i32, Logical>,
    rise: i32,
    alpha: f32,
    scale: Scale<f64>,
    output_scale: i32,
    centre: Point<i32, Physical>,
    zoom: f64,
    elements: Vec<LauncherRenderElement<R>>,
}

impl<R> Frame<'_, R>
where
    R: ShellRenderer,
{
    fn push(
        &mut self, buffer: &MemoryRenderBuffer, loc: Point<i32, Logical>,
        scaled: Option<(Size<i32, Logical>, Size<i32, Logical>)>, alpha: f32,
    ) {
        if alpha <= 0.0 {
            return;
        }
        if let Ok(element) = MemoryRenderBufferRenderElement::from_buffer(
            self.renderer,
            loc.to_f64().to_physical(self.scale),
            buffer,
            Some(alpha),
            scaled.map(|(source, _)| Rectangle::from_size(source.to_f64())),
            scaled.map(|(_, size)| size),
            Kind::Unspecified,
        ) {
            self.elements.push(LauncherRenderElement::Scaled(RescaleRenderElement::from_element(
                element,
                self.centre,
                self.zoom,
            )));
        }
    }

    fn put(&mut self, buffer: &MemoryRenderBuffer, x: i32, y: i32) {
        self.push(buffer, Point::from((x, y + self.rise)), None, self.alpha);
    }

    fn layer(&mut self, progress: f64, distance: f64) {
        self.rise = (distance * (1.0 - progress)).round() as i32;
        self.alpha = glide_theme::motion::assist(progress) as f32;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Grid {
    columns: usize,
    rows: usize,
    rect: Rectangle<i32, Logical>,
}

impl Grid {
    fn per_page(self) -> usize {
        (self.columns * self.rows).max(1)
    }

    fn cell(self, slot: usize) -> Rectangle<i32, Logical> {
        let column = i32::try_from(slot % self.columns).unwrap_or(0);
        let row = i32::try_from(slot / self.columns).unwrap_or(0);
        Rectangle::new(
            Point::from((self.rect.loc.x + column * CELL_WIDTH, self.rect.loc.y + row * CELL_HEIGHT)),
            Size::from((CELL_WIDTH, CELL_HEIGHT)),
        )
    }
}

fn dots_width(pages: usize) -> i32 {
    (i32::try_from(pages).unwrap_or(1) - 1) * (DOT_SIZE + DOT_GAP) + DOT_ACTIVE_WIDTH
}

fn dot_layout(pages: usize, position: f64) -> Vec<(i32, i32, f64)> {
    let mut x = 0;
    (0..pages)
        .map(|page| {
            let active = (1.0 - (position - page_as_f64(page)).abs()).clamp(0.0, 1.0);
            let grow = (f64::from(DOT_ACTIVE_WIDTH - DOT_SIZE) * active).round() as i32;
            let width = DOT_SIZE + grow;
            let dot = (x, width, active);
            x += width + DOT_GAP;
            dot
        })
        .collect()
}

fn dots_rect(area: Rectangle<i32, Logical>, pages: usize) -> Rectangle<i32, Logical> {
    let width = dots_width(pages);
    Rectangle::new(
        Point::from((
            area.loc.x + (area.size.w - width) / 2,
            area.loc.y + area.size.h - DOT_MARGIN - DOT_SIZE - DOT_HIT_PAD,
        )),
        Size::from((width, DOT_SIZE + DOT_HIT_PAD * 2)),
    )
}

fn search_rect(area: Rectangle<i32, Logical>) -> Rectangle<i32, Logical> {
    let width = SEARCH_WIDTH.min((area.size.w - SIDE_MARGIN * 2).max(CELL_WIDTH));
    Rectangle::new(
        Point::from((area.loc.x + (area.size.w - width) / 2, area.loc.y + TOP_MARGIN)),
        Size::from((width, SEARCH_HEIGHT)),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CategoryFilter {
    #[default]
    All,
    Only(AppCategory),
}

impl CategoryFilter {
    const ORDER: [Self; 7] = [
        Self::All,
        Self::Only(AppCategory::Internet),
        Self::Only(AppCategory::Office),
        Self::Only(AppCategory::Media),
        Self::Only(AppCategory::Utilities),
        Self::Only(AppCategory::System),
        Self::Only(AppCategory::Other),
    ];

    fn label(self) -> &'static str {
        use crate::i18n::{Key, t};
        match self {
            Self::All => t(Key::CategoryAll),
            Self::Only(AppCategory::Internet) => t(Key::CategoryInternet),
            Self::Only(AppCategory::Office) => t(Key::CategoryOffice),
            Self::Only(AppCategory::Media) => t(Key::CategoryMedia),
            Self::Only(AppCategory::Utilities) => t(Key::CategoryUtilities),
            Self::Only(AppCategory::System) => t(Key::CategorySystem),
            Self::Only(AppCategory::Other) => t(Key::CategoryOther),
        }
    }

    fn accepts(self, app: &DesktopApp) -> bool {
        match self {
            Self::All => true,
            Self::Only(wanted) => app.category == wanted,
        }
    }

    #[must_use]
    pub const fn is_all(self) -> bool {
        matches!(self, Self::All)
    }
}

fn chip_width(painter: &crate::paint::Painter, label: &str) -> i32 {
    let text = painter.text_width_styled(label, &CHIP_TEXT).ceil() as i32;
    (text + CHIP_PAD_X * 2).max(CHIP_MIN_WIDTH)
}

fn grid_for(area: Rectangle<i32, Logical>) -> Grid {
    let usable_w = (area.size.w - SIDE_MARGIN * 2).max(CELL_WIDTH);
    let usable_h = (area.size.h - SEARCH_BAND - DOTS_BAND).max(CELL_HEIGHT);

    let fit =
        |available: i32, cell: i32, lo: usize, hi: usize| usize::try_from(available / cell).unwrap_or(lo).clamp(lo, hi);
    let mut columns = fit(usable_w, CELL_WIDTH, MIN_COLUMNS, MAX_COLUMNS);
    if columns % 2 == 0 {
        columns = (columns - 1).max(MIN_COLUMNS);
    }
    let rows = fit(usable_h, CELL_HEIGHT, MIN_ROWS, MAX_ROWS);

    let size =
        Size::from((i32::try_from(columns).unwrap_or(1) * CELL_WIDTH, i32::try_from(rows).unwrap_or(1) * CELL_HEIGHT));
    let band_top = area.loc.y + SEARCH_BAND;
    let band_height = (area.size.h - SEARCH_BAND - DOTS_BAND).max(size.h);
    let rect = Rectangle::new(
        Point::from((area.loc.x + (area.size.w - size.w) / 2, band_top + (band_height - size.h) / 2)),
        size,
    );
    Grid { columns, rows, rect }
}

fn open_duration() -> Duration {
    glide_theme::motion::t4()
}

fn close_duration() -> Duration {
    glide_theme::motion::t3()
}

fn list_anim() -> Duration {
    glide_theme::motion::t2()
}

fn swap_anim() -> Duration {
    glide_theme::motion::t2()
}

const OPEN_SCALE: f64 = 0.94;
const FIELD_RISE: f64 = 24.0;
const CHIP_RISE: f64 = 32.0;
const ROW_RISE: f64 = 48.0;
const DOT_RISE: f64 = 48.0;
const LIST_DROP: f64 = -24.0;
const LIST_STAGGER: f64 = 0.05;
const GRID_SINK: f64 = 32.0;
const GRID_SHRINK: f64 = 0.04;
const SWAP_SHIFT: f64 = 48.0;
const CAPTION_PILL_FROM: (f64, f64) = (0.6, 0.8);
const FOCUS_RING: i32 = glide_theme::component::FOCUS_RING as i32;
const FIELD_HALO: i32 = glide_theme::component::FIELD_HALO as i32;

#[derive(Debug, Clone, Copy, PartialEq)]
struct Layers {
    field: f64,
    chips: f64,
    rows: [f64; MAX_ROWS],
    dots: f64,
    scale: f64,
}

fn layers(p: f64, k: f64) -> Layers {
    use glide_theme::motion::window;
    let p = p.clamp(0.0, 1.0);
    let mut rows = [0.0; MAX_ROWS];
    for (row, value) in rows.iter_mut().enumerate() {
        let start = 0.08 + 0.05 * row as f64;
        *value = ease_out(window(p, start, (start + 0.74).min(1.0))) * k;
    }
    Layers {
        field: ease_out(window(p, 0.0, 0.76)) * k,
        chips: ease_out(window(p, 0.06, 0.82)) * k,
        rows,
        dots: ease_out(window(p, 0.3, 1.0)) * k,
        scale: (1.0 - OPEN_SCALE).mul_add(ease_out(p) * k, OPEN_SCALE),
    }
}

fn list_row_progress(list: f64, row: usize) -> f64 {
    use glide_theme::motion::window;
    let start = (LIST_STAGGER * row as f64).min(0.35);
    ease_out(window(list, start, (0.65 + LIST_STAGGER * row as f64).min(1.0)))
}

fn hover_anim() -> Duration {
    glide_theme::motion::t2()
}

fn page_anim() -> Duration {
    glide_theme::motion::t3()
}

fn menu_anim() -> Duration {
    glide_theme::motion::t2()
}

fn layer_anim() -> Duration {
    glide_theme::motion::t1()
}

fn chip_anim() -> Duration {
    glide_theme::motion::t2()
}

fn dot_anim() -> Duration {
    glide_theme::motion::t2()
}

const DOT_STEPS: f64 = 24.0;

fn page_as_f64(page: usize) -> f64 {
    f64::from(u32::try_from(page).unwrap_or(u32::MAX))
}

fn chip_step(progress: f64) -> u8 {
    (progress.clamp(0.0, 1.0) * f64::from(CHIP_STEPS)).round() as u8
}

fn chip_step_progress(step: u8) -> f32 {
    f32::from(step) / f32::from(CHIP_STEPS)
}

type ScaledBuffers = std::collections::HashMap<i32, MemoryRenderBuffer>;
type CaptionBuffers = std::collections::HashMap<(i32, CaptionState), MemoryRenderBuffer>;
type ChipBuffers = std::collections::HashMap<(usize, u8, i32), MemoryRenderBuffer>;
type LayerTweens = std::collections::HashMap<usize, glide_ui::Tween>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Opening,
    Open,
    Closing,
    Interactive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum CaptionState {
    Rest,
    Active,
}

fn wrap_caption(painter: &crate::paint::Painter, text: &str, max_width: f32) -> (String, Option<String>) {
    let style = &glide_theme::TextStyle::LAUNCHER_CAPTION;
    if painter.text_width_styled(text, style) <= max_width {
        return (text.to_string(), None);
    }

    let mut first = String::new();
    let mut rest = String::new();
    for word in text.split_whitespace() {
        if rest.is_empty() {
            let candidate = if first.is_empty() { word.to_string() } else { format!("{first} {word}") };
            if first.is_empty() || painter.text_width_styled(&candidate, style) <= max_width {
                first = candidate;
                continue;
            }
        }
        if !rest.is_empty() {
            rest.push(' ');
        }
        rest.push_str(word);
    }

    if first.is_empty() {
        return (text.to_string(), None);
    }
    (first, (!rest.is_empty()).then_some(rest))
}

pub(crate) fn fallback_initial(name: &str) -> String {
    name.chars().find(|c| c.is_alphanumeric()).map_or_else(|| "?".to_string(), |c| c.to_uppercase().to_string())
}

fn filter_apps(
    apps: &[DesktopApp], query: &str, category: CategoryFilter, history: &history::LaunchHistory, now: u64,
) -> Vec<usize> {
    let candidates: Vec<usize> =
        apps.iter().enumerate().filter(|(_, app)| category.accepts(app)).map(|(index, _)| index).collect();
    if !query.trim().is_empty() {
        return search::rank_apps(apps, &candidates, query, history, now);
    }
    if category.is_all() {
        search::frequent_first(apps, &candidates, history, now, search::FREQUENT_LIMIT)
    } else {
        candidates
    }
}

fn layer_alpha(layers: &LayerTweens, index: usize) -> f64 {
    layers.get(&index).map_or(0.0, |tween| tween.value(layer_anim(), ease_io))
}

fn caret_half_period() -> Duration {
    glide_theme::motion::t4()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchOrigin {
    pub key: String,
    pub rect: Rectangle<i32, Logical>,
    pub at: Instant,
}

impl LaunchOrigin {
    #[must_use]
    pub fn claims(&self, key: &str, now: Instant) -> bool {
        self.key.eq_ignore_ascii_case(key) && now.saturating_duration_since(self.at) < crate::dock::LAUNCH_TIMEOUT
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Origin {
    Desktop,
    Overview,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Prompt {
    Search,
    Run,
}

#[derive(Debug)]
pub struct Launcher {
    apps: Vec<DesktopApp>,
    page: usize,
    phase: Phase,
    started: Instant,
    from: f64,
    from_k: f64,
    release: Option<glide_theme::motion::Release>,
    scene: Duration,
    scroll: f64,
    scroll_latched: bool,
    page_offset: glide_ui::Tween,
    hover: glide_ui::HoverAnimation<usize>,
    pointer_selected: bool,
    hover_layers: LayerTweens,
    chip_thumb: glide_ui::Tween,
    chip_hover: glide_ui::HoverAnimation<CategoryFilter>,
    list: glide_ui::Tween,
    swap: glide_ui::Tween,
    swap_dir: f64,
    origin: Origin,
    area: Rectangle<i32, Logical>,

    field: field::TextField,
    category: CategoryFilter,
    results: Vec<usize>,
    entries: Vec<search::Entry>,
    windows: Vec<search::OpenWindow>,
    recent: Vec<recent::RecentFile>,
    providers: Option<providers::Handle>,
    provided: Vec<providers::Found>,
    provider_query: String,
    provider_generation: u64,
    options: search::Options,
    prompt: Prompt,
    selected: Option<usize>,
    history: history::LaunchHistory,
    list_scroll: usize,
    row_layers: LayerTweens,
    dot_tween: glide_ui::Tween,
    caret_epoch: Instant,
    caret_timer_armed: bool,
    clear_hover: glide_ui::Tween,

    scrim: SolidColorBuffer,
    tiles: Vec<ScaledBuffers>,
    icon_missing: Vec<bool>,
    captions: Vec<CaptionBuffers>,
    cell_layer: ScaledBuffers,
    focus_ring: ScaledBuffers,
    pills: std::collections::HashMap<(i32, i32, i32), MemoryRenderBuffer>,
    search_field: Option<(SearchFieldKey, ScaledBuffers)>,
    search_halo: Option<(i32, ScaledBuffers)>,
    chip_thumbs: std::collections::HashMap<(i32, i32), MemoryRenderBuffer>,
    caret: ScaledBuffers,
    clear_glyph: ScaledBuffers,
    clear_layer: ScaledBuffers,
    chips: ChipBuffers,
    chip_layers: std::collections::HashMap<(i32, i32), MemoryRenderBuffer>,
    chip_layout: Vec<(CategoryFilter, Rectangle<i32, Logical>)>,
    empty_label: Option<(String, ScaledBuffers)>,
    dots: Option<((usize, i64), ScaledBuffers)>,
    list_cache: list::ListCache,
    menu: Option<LauncherMenu>,
    menu_cache: std::collections::HashMap<LauncherMenuKey, MemoryRenderBuffer>,
    menu_rows: LayerTweens,
    menu_row_layer: std::collections::HashMap<i32, MemoryRenderBuffer>,
    menu_shadow: std::collections::HashMap<(i32, i32, i32), MemoryRenderBuffer>,
    raster_rev: u64,
    runs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SearchFieldKey {
    shown: String,
    selection: Option<(usize, usize)>,
    clear: bool,
}

type LauncherMenuKey = (String, Vec<LauncherMenuItem>, i32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LauncherMenuItem {
    Open,
    Pin,
    Unpin,
}

impl LauncherMenuItem {
    #[must_use]
    pub fn label(self) -> &'static str {
        use crate::i18n::{Key, t};
        match self {
            Self::Open => t(Key::Open),
            Self::Pin => t(Key::PinToDock),
            Self::Unpin => t(Key::UnpinFromDock),
        }
    }
}

#[derive(Debug, Clone)]
struct LauncherMenu {
    result: usize,
    items: Vec<LauncherMenuItem>,
    rect: Rectangle<i32, Logical>,
    hovered: Option<usize>,
    reveal: glide_ui::Reveal,
}

impl LauncherMenu {
    fn progress(&self) -> f64 {
        let ease: fn(f64) -> f64 = if self.reveal.is_open() { ease_out } else { ease_io };
        self.reveal.progress(menu_anim(), ease)
    }
}

impl Launcher {
    #[must_use]
    pub fn new(apps: Vec<DesktopApp>, area: Rectangle<i32, Logical>) -> Self {
        let count = apps.len();
        Self {
            results: (0..count).collect(),
            apps,
            page: 0,
            phase: Phase::Opening,
            started: Instant::now(),
            from: 0.0,
            from_k: 1.0,
            release: None,
            scene: open_duration(),
            scroll: 0.0,
            scroll_latched: false,
            page_offset: glide_ui::Tween::at(0.0),
            hover: glide_ui::HoverAnimation::default(),
            pointer_selected: false,
            chip_hover: glide_ui::HoverAnimation::default(),
            hover_layers: LayerTweens::new(),
            chip_thumb: glide_ui::Tween::at(0.0),
            list: glide_ui::Tween::at(0.0),
            swap: glide_ui::Tween::at(1.0),
            swap_dir: 1.0,
            origin: Origin::Desktop,
            area,
            field: field::TextField::default(),
            category: CategoryFilter::All,
            entries: Vec::new(),
            windows: Vec::new(),
            recent: Vec::new(),
            providers: None,
            provided: Vec::new(),
            provider_query: String::new(),
            provider_generation: 0,
            options: search::Options::default(),
            prompt: Prompt::Search,
            selected: None,
            history: history::LaunchHistory::default(),
            list_scroll: 0,
            row_layers: LayerTweens::new(),
            dot_tween: glide_ui::Tween::at(0.0),
            caret_epoch: Instant::now(),
            caret_timer_armed: false,
            clear_hover: glide_ui::Tween::at(0.0),
            scrim: SolidColorBuffer::default(),
            tiles: vec![std::collections::HashMap::new(); count],
            icon_missing: vec![false; count],
            captions: vec![CaptionBuffers::new(); count],
            cell_layer: std::collections::HashMap::new(),
            focus_ring: ScaledBuffers::new(),
            pills: std::collections::HashMap::new(),
            search_field: None,
            search_halo: None,
            chip_thumbs: std::collections::HashMap::new(),
            caret: ScaledBuffers::new(),
            clear_glyph: ScaledBuffers::new(),
            clear_layer: ScaledBuffers::new(),
            chips: ChipBuffers::new(),
            chip_layers: std::collections::HashMap::new(),
            chip_layout: Vec::new(),
            empty_label: None,
            dots: None,
            list_cache: list::ListCache::default(),
            menu: None,
            menu_cache: std::collections::HashMap::new(),
            menu_rows: LayerTweens::new(),
            menu_row_layer: std::collections::HashMap::new(),
            menu_shadow: std::collections::HashMap::new(),
            raster_rev: 0,
            runs: Vec::new(),
        }
    }

    pub fn open_menu(&mut self, result: usize, area: Rectangle<i32, Logical>, pinned: bool) -> bool {
        if self.is_listing() || self.app_index(result).is_none() {
            return false;
        }
        let Some(slot) = self.page_range().position(|candidate| candidate == result) else {
            return false;
        };
        let Some(anchor) = self.icon_rects(area).into_iter().nth(slot) else {
            return false;
        };
        let items = vec![LauncherMenuItem::Open, if pinned { LauncherMenuItem::Unpin } else { LauncherMenuItem::Pin }];
        let rect = crate::popup_menu::place_above(anchor, area, items.len());
        let mut reveal = match self.menu.take() {
            Some(menu) if menu.result == result && menu.rect == rect => menu.reveal,
            _ => glide_ui::Reveal::closed(),
        };
        reveal.open(menu_anim());
        self.menu = Some(LauncherMenu { result, items, rect, hovered: None, reveal });
        self.menu_cache.clear();
        self.menu_rows.clear();
        true
    }

    pub fn close_menu(&mut self) -> bool {
        let Some(menu) = self.menu.as_mut() else {
            return false;
        };
        menu.reveal.close(menu_anim())
    }

    fn open_menu_ref(&self) -> Option<&LauncherMenu> {
        self.menu.as_ref().filter(|menu| menu.reveal.is_open())
    }

    #[must_use]
    pub fn menu_result(&self) -> Option<usize> {
        self.open_menu_ref().map(|menu| menu.result)
    }

    #[must_use]
    pub fn menu_rect(&self) -> Option<Rectangle<i32, Logical>> {
        self.open_menu_ref().map(|menu| menu.rect)
    }

    #[must_use]
    pub fn menu_item_at(&self, pos: Point<f64, Logical>) -> Option<LauncherMenuItem> {
        let menu = self.open_menu_ref()?;
        let row = crate::popup_menu::row_at(menu.rect, menu.items.len(), pos)?;
        menu.items.get(row).copied()
    }

    pub fn set_menu_hover(&mut self, pos: Point<f64, Logical>) -> bool {
        let Some(menu) = self.menu.as_mut().filter(|menu| menu.reveal.is_open()) else {
            return false;
        };
        let hovered = crate::popup_menu::row_at(menu.rect, menu.items.len(), pos);
        if menu.hovered == hovered {
            return false;
        }
        let previous = std::mem::replace(&mut menu.hovered, hovered);
        let duration = layer_anim();
        if let Some(tween) = previous.and_then(|row| self.menu_rows.get_mut(&row)) {
            tween.retarget(0.0, duration, ease_io);
        }
        if let Some(row) = hovered {
            self.menu_rows.entry(row).or_insert_with(|| glide_ui::Tween::at(0.0)).retarget(1.0, duration, ease_io);
        }
        true
    }

    #[must_use]
    pub fn query(&self) -> &str {
        self.field.text()
    }

    #[must_use]
    pub fn is_listing(&self) -> bool {
        self.run_mode() || !self.field.text().trim().is_empty()
    }

    pub fn begin_run_mode(&mut self) {
        if self.run_mode() {
            return;
        }
        self.prompt = Prompt::Run;
        self.runs = if cfg!(test) { Vec::new() } else { history::load_runs() };
        self.refilter();
    }

    #[must_use]
    pub const fn run_mode(&self) -> bool {
        matches!(self.prompt, Prompt::Run)
    }

    #[must_use]
    pub fn entries(&self) -> &[search::Entry] {
        &self.entries
    }

    fn edit(&mut self, text_changed: bool, op: impl FnOnce(&mut field::TextField) -> bool) -> bool {
        let changed = op(&mut self.field);
        if changed {
            self.caret_epoch = Instant::now();
            self.search_field = None;
            if text_changed {
                self.refilter();
            }
        }
        changed
    }

    pub fn push_query(&mut self, ch: char) -> bool {
        self.edit(true, |field| field.insert(ch))
    }

    pub fn insert_text(&mut self, text: &str) -> bool {
        self.edit(true, |field| field.insert_str(text))
    }

    pub fn pop_query(&mut self) -> bool {
        self.edit(true, field::TextField::backspace)
    }

    pub fn delete_forward(&mut self) -> bool {
        self.edit(true, field::TextField::delete)
    }

    pub fn delete_word(&mut self) -> bool {
        self.edit(true, field::TextField::delete_word_back)
    }

    pub fn move_caret(&mut self, movement: field::CaretMove) -> bool {
        self.edit(false, |field| field.move_caret(movement))
    }

    pub fn select_all(&mut self) -> bool {
        self.edit(false, field::TextField::select_all)
    }

    pub fn arrow(&mut self, dx: i32) -> bool {
        if !self.is_listing() {
            return self.move_selection(dx, 0);
        }
        let movement = if dx < 0 { field::CaretMove::Left } else { field::CaretMove::Right };
        self.move_caret(movement)
    }

    pub fn clear_text(&mut self) -> bool {
        self.edit(true, field::TextField::clear)
    }

    pub fn clear_query(&mut self) -> bool {
        if self.field.is_empty() && self.category.is_all() {
            return false;
        }
        self.field.clear();
        self.caret_epoch = Instant::now();
        self.category = CategoryFilter::All;
        self.sync_chip_tweens();
        self.refilter();
        true
    }

    pub fn set_history(&mut self, history: history::LaunchHistory) {
        self.history = history;
    }

    pub fn set_windows(&mut self, windows: Vec<search::OpenWindow>) {
        self.windows = windows;
        if !self.field.text().trim().is_empty() {
            self.refilter();
        }
    }

    #[must_use]
    pub fn window(&self, index: usize) -> Option<&search::OpenWindow> {
        self.windows.get(index)
    }

    pub fn set_recent(&mut self, recent: Vec<recent::RecentFile>) {
        self.recent = recent;
        if !self.field.text().trim().is_empty() {
            self.refilter();
        }
    }

    #[must_use]
    pub fn recent_file(&self, index: usize) -> Option<&recent::RecentFile> {
        self.recent.get(index)
    }

    pub fn set_providers(&mut self, handle: Option<providers::Handle>) {
        self.providers = handle;
    }

    pub fn set_provided(&mut self, generation: u64, found: Vec<providers::Found>) {
        if generation != self.provider_generation {
            return;
        }
        self.provided = found;
        self.refilter();
    }

    #[must_use]
    pub fn provided(&self, index: usize) -> Option<(&providers::Found, &providers::Handle, Vec<String>)> {
        let handle = self.providers.as_ref()?;
        Some((self.provided.get(index)?, handle, providers::terms(self.field.text())))
    }

    fn ask_providers(&mut self) {
        let query = self.field.text().trim().to_string();
        if query == self.provider_query {
            return;
        }
        self.provider_query.clone_from(&query);
        self.provided.clear();
        self.provider_generation += 1;
        if let Some(handle) = self.providers.as_ref()
            && self.options.providers
            && query.chars().count() >= providers::MIN_QUERY
        {
            handle.query(self.provider_generation, providers::terms(&query));
        }
    }

    fn provided_entries(&self) -> Vec<search::Entry> {
        self.provided
            .iter()
            .enumerate()
            .filter(|(_, found)| self.options.allows_provider(&found.desktop_id))
            .map(|(index, found)| search::Entry::Provided {
                index,
                app: self.apps.iter().position(|app| app.id.trim_end_matches(".desktop") == found.desktop_id),
            })
            .collect()
    }

    pub fn record_launch(&mut self, app_id: &str) -> history::LaunchHistory {
        self.history.record(app_id, history::now_unix());
        self.history.clone()
    }

    #[must_use]
    pub fn selected_entry(&self) -> Option<search::Entry> {
        let selected = self.selected?;
        if self.is_listing() {
            self.entries.get(selected).cloned()
        } else {
            self.app_index(selected).map(search::Entry::App)
        }
    }

    #[must_use]
    pub fn caret_visible(&self) -> bool {
        let half = caret_half_period();
        if half.is_zero() {
            return true;
        }
        let phase = self.caret_epoch.elapsed().as_nanos() / half.as_nanos().max(1);
        phase.is_multiple_of(2)
    }

    #[must_use]
    pub fn next_caret_toggle(&self) -> Option<Instant> {
        let half = caret_half_period();
        if half.is_zero() || self.phase == Phase::Closing {
            return None;
        }
        let elapsed = self.caret_epoch.elapsed().as_nanos();
        let step = half.as_nanos().max(1);
        let next = (elapsed / step + 1) * step;
        let next = u64::try_from(next).ok()?;
        Some(self.caret_epoch + Duration::from_nanos(next))
    }

    pub fn arm_caret_timer(&mut self) -> bool {
        !std::mem::replace(&mut self.caret_timer_armed, true)
    }

    pub fn disarm_caret_timer(&mut self) {
        self.caret_timer_armed = false;
    }

    #[must_use]
    pub fn clear_contains(&self, area: Rectangle<i32, Logical>, pos: Point<f64, Logical>) -> bool {
        !self.field.is_empty() && clear_rect(area).to_f64().contains(pos)
    }

    pub fn set_clear_hover(&mut self, hovered: bool) -> bool {
        let target = if hovered && !self.field.is_empty() { 1.0 } else { 0.0 };
        self.clear_hover.retarget(target, layer_anim(), ease_io)
    }

    #[must_use]
    pub const fn category(&self) -> CategoryFilter {
        self.category
    }

    #[must_use]
    pub fn categories(&self) -> Vec<CategoryFilter> {
        CategoryFilter::ORDER
            .into_iter()
            .filter(|category| category.is_all() || self.apps.iter().any(|app| category.accepts(app)))
            .collect()
    }

    pub fn toggle_category(&mut self, category: CategoryFilter) -> bool {
        let next = if self.category == category { CategoryFilter::All } else { category };
        if self.category == next {
            return false;
        }
        self.category = next;
        self.sync_chip_tweens();
        self.search_field = None;
        self.refilter();
        true
    }

    fn chip_slot(&self, category: CategoryFilter) -> f64 {
        let slot = self.categories().iter().position(|candidate| *candidate == category).unwrap_or(0);
        page_as_f64(slot)
    }

    fn sync_chip_tweens(&mut self) {
        let from = self.chip_thumb.target();
        let to = self.chip_slot(self.category);
        if (to - from).abs() > f64::EPSILON {
            self.swap_dir = if to > from { 1.0 } else { -1.0 };
            self.swap.jump(0.0);
            self.swap.retarget(1.0, swap_anim(), |t| t);
        }
        self.chip_thumb.retarget(to, chip_anim(), ease_out);
    }

    fn chip_position(&self) -> f64 {
        self.chip_thumb.value(chip_anim(), ease_out)
    }

    fn chip_progress(&self, category: CategoryFilter) -> f64 {
        (1.0 - (self.chip_position() - self.chip_slot(category)).abs()).clamp(0.0, 1.0)
    }

    fn swap_progress(&self) -> f64 {
        ease_out(self.swap.value(swap_anim(), |t| t))
    }

    fn refilter(&mut self) {
        let now = history::now_unix();
        self.results = filter_apps(&self.apps, self.field.text(), self.category, &self.history, now);
        self.entries = if self.run_mode() {
            search::run_entries(self.field.text(), &self.runs)
        } else {
            let sources = search::Sources {
                windows: if self.options.windows { &self.windows } else { &[] },
                files: if self.options.files { &self.recent } else { &[] },
                hide_settings: !self.options.settings,
            };
            search::build_entries(&self.apps, &self.results, &sources, self.field.text())
        };
        if !self.run_mode() {
            self.ask_providers();
            let provided = self.provided_entries();
            self.entries.extend(provided);
        }
        self.page = 0;
        if self.page_is_sliding() {
            self.page_offset.retarget(0.0, page_anim(), ease_out);
        } else {
            self.page_offset.jump(0.0);
        }
        self.dot_tween.retarget(0.0, dot_anim(), ease_out);
        self.dots = None;
        self.scroll = 0.0;
        self.list_scroll = 0;
        self.search_field = None;
        self.selected = (self.is_listing() && !self.entries.is_empty()).then_some(0);
        self.pointer_selected = false;
        if !self.is_listing() {
            self.row_layers.clear();
        }
        let listing = if self.is_listing() { 1.0 } else { 0.0 };
        self.list.retarget(listing, list_anim(), |t| t);
        self.sync_hover_to_selection();
    }

    fn list_progress(&self) -> f64 {
        self.list.value(list_anim(), |t| t)
    }

    fn sync_hover_to_selection(&mut self) {
        self.hover.retarget_without_restarting(self.selected);
        self.sync_hover_layers();
    }

    fn sync_hover_layers(&mut self) {
        let listing = self.is_listing();
        let target =
            if listing { self.hover.target() } else { self.hover.target().and_then(|result| self.app_index(result)) };
        let layers = if listing { &mut self.row_layers } else { &mut self.hover_layers };
        let duration = layer_anim();
        for (&index, tween) in layers.iter_mut() {
            if Some(index) != target {
                tween.retarget(0.0, duration, ease_io);
            }
        }
        if let Some(index) = target {
            layers.entry(index).or_insert_with(|| glide_ui::Tween::at(0.0)).retarget(1.0, duration, ease_io);
        }
    }

    fn hover_layer_alpha(&self, index: usize) -> f64 {
        layer_alpha(&self.hover_layers, index)
    }

    fn row_layer_alpha(&self, entry: usize) -> f64 {
        layer_alpha(&self.row_layers, entry)
    }

    fn slide_to(&mut self, page: usize) {
        self.page = page;
        self.page_offset.retarget(page_as_f64(page), page_anim(), ease_out);
        self.dot_tween.retarget(page_as_f64(page), dot_anim(), ease_out);
        self.dots = None;
    }

    fn page_position(&self) -> f64 {
        self.page_offset.value(page_anim(), ease_out)
    }

    fn page_is_sliding(&self) -> bool {
        self.page_offset.is_animating(page_anim())
    }

    #[must_use]
    pub fn app_index(&self, result: usize) -> Option<usize> {
        self.results.get(result).copied()
    }

    #[must_use]
    pub const fn selected(&self) -> Option<usize> {
        self.selected
    }

    pub fn move_selection(&mut self, dx: i32, dy: i32) -> bool {
        if self.is_listing() {
            return self.move_list_selection(dx + dy);
        }
        if self.results.is_empty() {
            return false;
        }
        let grid = self.grid();
        let columns = i32::try_from(grid.columns).unwrap_or(1).max(1);
        let per_page = i32::try_from(grid.per_page()).unwrap_or(1).max(1);

        let Some(current) = self.selected else {
            let first = i32::try_from(self.page).unwrap_or(0) * per_page;
            self.select_index(first);
            return true;
        };
        let current = i32::try_from(current).unwrap_or(0);

        let last = i32::try_from(self.results.len()).unwrap_or(1) - 1;
        let next = (current + dx + dy * columns).clamp(0, last);
        if next == current {
            return false;
        }
        self.select_index(next);
        true
    }

    fn select_index(&mut self, index: i32) {
        let last = i32::try_from(self.results.len()).unwrap_or(1) - 1;
        let index = index.clamp(0, last.max(0));
        let Ok(index) = usize::try_from(index) else {
            return;
        };
        self.selected = Some(index);
        self.pointer_selected = false;
        self.sync_hover_to_selection();

        let per_page = self.grid().per_page();
        let page = index / per_page;
        if page != self.page && page < self.page_count() {
            self.slide_to(page);
        }
    }

    pub fn go_to_page(&mut self, page: usize) -> bool {
        if page >= self.page_count() || page == self.page {
            return false;
        }
        self.slide_to(page);
        self.selected = None;
        self.hover.retarget_without_restarting(None);
        self.sync_hover_layers();
        true
    }

    pub fn turn_page(&mut self, forward: bool) -> bool {
        if self.is_listing() {
            return false;
        }
        let next = if forward { self.page + 1 } else { self.page.saturating_sub(1) };
        self.go_to_page(next)
    }

    fn invalidate_stale_buffers(&mut self, theme: &LauncherTheme) {
        let current = crate::paint::raster_revision(theme.revision());
        if current == self.raster_rev {
            return;
        }
        self.raster_rev = current;
        self.dots = None;
        self.menu_cache.clear();
        self.menu_shadow.clear();
        self.cell_layer.clear();
        self.focus_ring.clear();
        self.pills.clear();
        self.search_field = None;
        self.search_halo = None;
        self.chip_thumbs.clear();
        self.caret.clear();
        self.clear_glyph.clear();
        self.clear_layer.clear();
        self.list_cache = list::ListCache::default();
        self.chips.clear();
        self.chip_layers.clear();
        self.empty_label = None;
        for cache in &mut self.tiles {
            cache.clear();
        }
        for cache in &mut self.captions {
            cache.clear();
        }
    }

    #[must_use]
    pub fn apps(&self) -> &[DesktopApp] {
        &self.apps
    }

    pub fn set_apps(&mut self, apps: Vec<DesktopApp>, pixels: &[Option<Vec<u8>>], scale: i32) {
        let scale = scale.max(1);
        self.tiles = pixels
            .iter()
            .map(|pixels| {
                pixels.iter().map(|pixels| (scale, buffer_from(pixels, ICON_SIZE, ICON_SIZE, scale))).collect()
            })
            .collect();
        self.icon_missing = pixels.iter().map(Option::is_none).collect();
        self.captions = vec![CaptionBuffers::new(); apps.len()];
        self.apps = apps;
        self.hover_layers.clear();
        self.row_layers.clear();
        self.list_cache = list::ListCache::default();
        self.chips.clear();
        self.refilter();
    }

    #[must_use]
    pub fn page_count(&self) -> usize {
        if self.is_listing() {
            return 1;
        }
        self.results.len().div_ceil(self.grid().per_page()).max(1)
    }

    #[must_use]
    pub const fn with_scene(mut self, scene: Duration) -> Self {
        self.scene = scene;
        self
    }

    #[must_use]
    pub fn with_options(mut self, options: search::Options) -> Self {
        self.options = options;
        self
    }

    fn duration(&self) -> Duration {
        match (self.phase, self.release) {
            (_, Some(release)) => release.duration,
            (Phase::Closing, None) => close_duration(),
            _ => self.scene,
        }
    }

    fn elapsed_fraction(&self) -> f64 {
        let duration = self.duration();
        if duration.is_zero() {
            return 1.0;
        }
        let elapsed = self.started.elapsed().as_secs_f64() / duration.as_secs_f64();
        elapsed.clamp(0.0, 1.0)
    }

    fn scene(&self) -> (f64, f64) {
        let f = self.elapsed_fraction();
        match (self.phase, self.release) {
            (Phase::Open, _) => (1.0, 1.0),
            (Phase::Interactive, _) => (self.from, 1.0),
            (Phase::Opening, Some(release)) => ((1.0 - self.from).mul_add(release.at(f), self.from), 1.0),
            (Phase::Opening, None) => {
                ((1.0 - self.from).mul_add(f, self.from), (1.0 - self.from_k).mul_add(ease_out(f), self.from_k))
            }
            (Phase::Closing, Some(release)) => (self.from * (1.0 - release.at(f)), 1.0),
            (Phase::Closing, None) => (self.from, self.from_k * (1.0 - ease_io(f))),
        }
    }

    fn layers(&self) -> Layers {
        let (p, k) = self.scene();
        layers(p, k)
    }

    fn progress(&self) -> f64 {
        let (p, k) = self.scene();
        p * k
    }

    #[must_use]
    pub fn openness(&self) -> f64 {
        self.progress()
    }

    #[must_use]
    pub fn backdrop_openness(&self) -> f64 {
        let (p, k) = self.scene();
        ease_io(p) * k
    }

    fn start(&mut self, phase: Phase, release: Option<glide_theme::motion::Release>) {
        let (p, k) = self.scene();
        (self.from, self.from_k) = if release.is_some() { (p * k, 1.0) } else { (p, k) };
        self.phase = phase;
        self.release = release;
        self.started = Instant::now();
    }

    pub fn begin_close(&mut self) {
        self.close_menu();
        if self.phase != Phase::Closing {
            self.start(Phase::Closing, None);
        }
    }

    pub fn reopen(&mut self) {
        if matches!(self.phase, Phase::Closing | Phase::Interactive) {
            self.start(Phase::Opening, None);
        }
    }

    pub fn set_interactive_progress(&mut self, progress: f64) {
        self.close_menu();
        self.phase = Phase::Interactive;
        self.release = None;
        self.from = progress.clamp(0.0, 1.0);
        self.from_k = 1.0;
    }

    pub fn catch(&mut self) -> bool {
        if !matches!(self.phase, Phase::Opening | Phase::Closing) {
            return false;
        }
        self.set_interactive_progress(self.progress());
        true
    }

    pub fn release_gesture(&mut self, open: bool, velocity: f64) {
        let current = self.progress();
        let distance = if open { 1.0 - current } else { -current };
        let release = glide_theme::motion::Release::new(distance, velocity, self.scene);
        self.start(if open { Phase::Opening } else { Phase::Closing }, Some(release));
    }

    #[must_use]
    pub fn is_interactive(&self) -> bool {
        self.phase == Phase::Interactive
    }

    #[must_use]
    pub fn is_closing(&self) -> bool {
        self.phase == Phase::Closing
    }

    #[must_use]
    pub fn is_settled(&self) -> bool {
        self.phase == Phase::Open
    }

    pub fn take_over_overview(&mut self) {
        self.origin = Origin::Overview;
    }

    #[must_use]
    pub const fn takes_over_overview(&self) -> bool {
        matches!(self.origin, Origin::Overview)
    }

    pub fn tick(&mut self) -> bool {
        self.settle();
        let settled = self.started.elapsed() >= self.duration();
        match self.phase {
            Phase::Opening if settled => {
                self.phase = Phase::Open;
                true
            }
            Phase::Closing if settled => false,
            _ => true,
        }
    }

    pub fn settle(&mut self) {
        let menu_duration = menu_anim();
        if let Some(menu) = self.menu.as_mut() {
            menu.reveal.settle(menu_duration);
            if !menu.reveal.is_open() && !menu.reveal.is_visible(menu_duration) {
                self.menu = None;
                self.menu_cache.clear();
            }
        }
        let layer_duration = layer_anim();
        for layers in [&mut self.hover_layers, &mut self.row_layers, &mut self.menu_rows] {
            layers.retain(|_, tween| tween.target() > 0.0 || tween.is_animating(layer_duration));
        }
        if !self.chip_thumb.is_animating(chip_anim()) {
            self.chips.retain(|&(_, step, _), _| step == 0 || step == CHIP_STEPS);
        }
    }

    #[must_use]
    pub fn is_animating(&self) -> bool {
        let scene = matches!(self.phase, Phase::Opening | Phase::Closing);
        let lift = self.hover.is_animating(hover_anim());
        let page = self.page_is_sliding();
        let menu = self.menu.as_ref().is_some_and(|menu| menu.reveal.is_animating(menu_anim()));
        let layer_duration = layer_anim();
        let layers = self
            .hover_layers
            .values()
            .chain(self.row_layers.values())
            .chain(self.menu_rows.values())
            .any(|tween| tween.is_animating(layer_duration));
        let clear = self.clear_hover.is_animating(layer_duration);
        let chips = self.chip_thumb.is_animating(chip_anim()) || self.chip_hover.is_animating(layer_duration);
        let dots = self.dot_tween.is_animating(dot_anim());
        let switching = self.list.is_animating(list_anim()) || self.swap.is_animating(swap_anim());
        scene || lift || page || menu || layers || clear || chips || dots || switching
    }

    pub fn scroll_by(&mut self, amount: f64, gesture: bool) -> bool {
        const DETENT: f64 = 15.0;
        if self.scroll_latched && !self.is_listing() {
            return false;
        }
        if self.scroll != 0.0 && self.scroll.signum() != amount.signum() {
            self.scroll = 0.0;
        }
        self.scroll += amount;
        let mut turned = false;
        while self.scroll.abs() >= DETENT {
            let forward = self.scroll > 0.0;
            self.scroll -= DETENT * self.scroll.signum();
            if !self.is_listing() {
                self.scroll = 0.0;
                self.scroll_latched = gesture;
                return self.turn_page(forward);
            }
            if self.scroll_list(forward) {
                turned = true;
            } else {
                self.scroll = 0.0;
                break;
            }
        }
        turned
    }

    pub fn end_scroll(&mut self) {
        self.scroll = 0.0;
        self.scroll_latched = false;
    }

    fn page_passes(&self) -> Vec<(std::ops::Range<usize>, f64)> {
        let position = self.page_position().max(0.0);
        let (first, second) = (position.floor() as usize, position.ceil() as usize);
        (first..=second)
            .filter(|&page| page < self.page_count())
            .map(|page| {
                let offset = page as f64 - position;
                (self.page_range_of(page), offset)
            })
            .collect()
    }

    fn page_range(&self) -> std::ops::Range<usize> {
        self.page_range_of(self.page)
    }

    fn page_range_of(&self, page: usize) -> std::ops::Range<usize> {
        let per_page = self.grid().per_page();
        let start = page * per_page;
        let end = (start + per_page).min(self.results.len());
        start.min(self.results.len())..end
    }

    fn grid(&self) -> Grid {
        grid_for(self.area)
    }

    pub fn set_area(&mut self, area: Rectangle<i32, Logical>) {
        if self.area == area {
            return;
        }
        self.area = area;
        self.page = self.page.min(self.page_count() - 1);
        self.dots = None;
        self.search_field = None;
        self.chips.clear();
        self.empty_label = None;
        if let Some(selected) = self.selected.filter(|_| !self.is_listing()) {
            self.page = selected / self.grid().per_page();
        }
        self.page_offset.jump(page_as_f64(self.page));
        self.dot_tween.jump(page_as_f64(self.page));
        self.list_scroll = 0;
        self.ensure_selection_visible();
        self.sync_hover_to_selection();
    }

    #[must_use]
    pub fn dot_at(&self, area: Rectangle<i32, Logical>, pos: Point<f64, Logical>) -> Option<usize> {
        let pages = self.page_count();
        if pages <= 1 {
            return None;
        }
        let strip = dots_rect(area, pages);
        if !strip.to_f64().contains(pos) {
            return None;
        }
        let position = self.dot_tween.value(dot_anim(), ease_out);
        dot_layout(pages, position).into_iter().position(|(x, width, _)| {
            Rectangle::new(
                Point::from((strip.loc.x + x - DOT_GAP / 2, strip.loc.y)),
                Size::from((width + DOT_GAP, strip.size.h)),
            )
            .to_f64()
            .contains(pos)
        })
    }

    #[must_use]
    #[allow(clippy::unused_self)]
    pub fn search_contains(&self, area: Rectangle<i32, Logical>, pos: Point<f64, Logical>) -> bool {
        search_rect(area).to_f64().contains(pos)
    }

    pub(crate) fn layout_chips(
        &mut self, painter: &crate::paint::Painter, area: Rectangle<i32, Logical>,
    ) -> Vec<(CategoryFilter, Rectangle<i32, Logical>)> {
        let rects = self.chip_rects(painter, area);
        self.chip_layout.clone_from(&rects);
        rects
    }

    fn chip_rects(
        &self, painter: &crate::paint::Painter, area: Rectangle<i32, Logical>,
    ) -> Vec<(CategoryFilter, Rectangle<i32, Logical>)> {
        let categories = self.categories();
        if categories.len() <= 1 {
            return Vec::new();
        }

        let widths: Vec<i32> = categories.iter().map(|category| chip_width(painter, category.label())).collect();
        let total: i32 = widths.iter().sum::<i32>() + (i32::try_from(widths.len()).unwrap_or(1) - 1) * CHIP_GAP;

        let mut x = area.loc.x + (area.size.w - total) / 2;
        let y = area.loc.y + TOP_MARGIN + SEARCH_HEIGHT + SEARCH_GAP;
        categories
            .into_iter()
            .zip(widths)
            .map(|(category, width)| {
                let rect = Rectangle::new(Point::from((x, y)), Size::from((width, CHIP_HEIGHT)));
                x += width + CHIP_GAP;
                (category, rect)
            })
            .collect()
    }

    #[must_use]
    pub fn category_at(&self, pos: Point<f64, Logical>) -> Option<CategoryFilter> {
        self.chip_layout.iter().find(|(_, rect)| rect.to_f64().contains(pos)).map(|&(category, _)| category)
    }

    #[must_use]
    pub fn launch_rect(&self, entry: &search::Entry) -> Option<Rectangle<i32, Logical>> {
        if self.is_listing() {
            let index = self.entries.iter().position(|candidate| candidate == entry)?;
            let placed = list::layout(&self.entries, self.area, self.list_scroll);
            let (_, row) = placed.into_iter().find(|(row, _)| *row == list::ListRow::Item(index))?;
            return Some(list::row_icon_rect(row));
        }
        let search::Entry::App(app) = entry else {
            return None;
        };
        let grid = self.grid();
        self.page_range()
            .enumerate()
            .find(|&(_, result)| self.app_index(result) == Some(*app))
            .map(|(slot, _)| icon_rect_in(grid.cell(slot)))
    }

    #[must_use]
    pub fn icon_rects(&self, area: Rectangle<i32, Logical>) -> Vec<Rectangle<i32, Logical>> {
        let grid = grid_for(area);
        self.page_range().enumerate().map(|(slot, _)| icon_rect_in(grid.cell(slot))).collect()
    }

    #[must_use]
    pub fn app_at(&self, area: Rectangle<i32, Logical>, pos: Point<f64, Logical>) -> Option<usize> {
        if self.is_listing() {
            return None;
        }
        let grid = grid_for(area);
        self.page_range()
            .enumerate()
            .find(|&(slot, _)| grid.cell(slot).to_f64().contains(pos))
            .map(|(_, result)| result)
    }

    pub fn set_hover(&mut self, result: Option<usize>) -> bool {
        if result.is_some() {
            self.selected = result;
            self.pointer_selected = true;
        } else if self.pointer_selected && !self.is_listing() {
            self.selected = None;
            self.pointer_selected = false;
        }
        let changed = self.hover.set_with(self.selected, hover_anim());
        if changed {
            self.sync_hover_layers();
        }
        changed
    }

    pub fn set_chip_hover(&mut self, category: Option<CategoryFilter>) -> bool {
        self.chip_hover.set_with(category, layer_anim())
    }
}

fn icon_rect_in(cell: Rectangle<i32, Logical>) -> Rectangle<i32, Logical> {
    Rectangle::new(
        Point::from((cell.loc.x + (CELL_WIDTH - ICON_SIZE) / 2, cell.loc.y + CELL_PAD_TOP)),
        Size::from((ICON_SIZE, ICON_SIZE)),
    )
}

fn scale_about(rect: Rectangle<i32, Logical>, anchor: (f64, f64), rise: f64, factor: f64) -> Rectangle<i32, Logical> {
    let x = (anchor.0 + (f64::from(rect.loc.x) - anchor.0) * factor).round() as i32;
    let y = (anchor.1 + (f64::from(rect.loc.y) + rise - anchor.1) * factor).round() as i32;
    let w = (f64::from(rect.size.w) * factor).round().max(1.0) as i32;
    let h = (f64::from(rect.size.h) * factor).round().max(1.0) as i32;
    Rectangle::new(Point::from((x, y)), Size::from((w, h)))
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub fn render<R>(
    launcher: &mut Launcher, renderer: &mut R, icons: &mut IconLoader, painter: &crate::paint::Painter,
    theme: &LauncherTheme, area: Rectangle<i32, Logical>, scale: Scale<f64>, output_scale: i32,
) -> Vec<LauncherRenderElement<R>>
where
    R: ShellRenderer,
{
    if launcher.progress() <= 0.0 {
        return Vec::new();
    }

    launcher.set_area(area);
    launcher.settle();
    launcher.invalidate_stale_buffers(theme);

    let layers = launcher.layers();
    let list = ease_io(launcher.list_progress());
    let centre = Point::<i32, Logical>::from((area.loc.x + area.size.w / 2, area.loc.y + area.size.h / 2))
        .to_f64()
        .to_physical(scale)
        .to_i32_round();

    let mut frame = Frame {
        renderer,
        painter,
        theme,
        area,
        rise: 0,
        alpha: 1.0,
        scale,
        output_scale,
        centre,
        zoom: layers.scale,
        elements: Vec::new(),
    };
    frame.layer(layers.field, FIELD_RISE);
    push_menu(launcher, &mut frame);
    push_search_field(launcher, &mut frame);
    if launcher.is_listing() {
        launcher.chip_layout.clear();
    }
    if list > 0.0 {
        let rows = launcher.list_progress();
        list::push_list(launcher, &mut frame, icons, rows, layers.chips);
        if launcher.is_listing() && launcher.entries.is_empty() && !launcher.field.text().trim().is_empty() {
            push_empty_state(launcher, &mut frame, list_row_progress(rows, 0) * layers.chips);
        }
    }
    if list < 1.0 {
        let grid = Recede { sink: GRID_SINK * list, alpha: glide_theme::motion::assist(1.0 - list) };
        frame.zoom = layers.scale * GRID_SHRINK.mul_add(-list, 1.0);
        push_grid(launcher, &mut frame, icons, &layers, grid);
        push_category_chips(launcher, &mut frame, layers.chips, grid);
        push_dots(launcher, &mut frame, layers.dots, grid);
    }

    let mut elements = frame.elements;

    let mut scrim_color = theme.background.0;
    scrim_color[3] *= launcher.backdrop_openness().clamp(0.0, 1.0) as f32;
    launcher.scrim.update((area.size.w, area.size.h), scrim_color);
    elements.push(LauncherRenderElement::Solid(SolidColorRenderElement::from_buffer(
        &launcher.scrim,
        area.loc.to_f64().to_physical(scale).to_i32_round(),
        scale,
        1.0,
        Kind::Unspecified,
    )));

    elements
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Recede {
    sink: f64,
    alpha: f64,
}

impl<R: ShellRenderer> Frame<'_, R> {
    fn layer_in(&mut self, progress: f64, distance: f64, recede: Recede) {
        self.rise = distance.mul_add(1.0 - progress, recede.sink).round() as i32;
        self.alpha = (glide_theme::motion::assist(progress) * recede.alpha) as f32;
    }
}

fn draw_ring(
    width: i32, height: i32, scale: i32, color: Color, radius: f32, thickness: f32,
) -> Option<MemoryRenderBuffer> {
    let mut c = Canvas::new(width, height, scale)?;
    let s = scale as f32;
    crate::paint::draw_inset_border(&mut c.buf, width * scale, height * scale, thickness * s, radius * s, color);
    Some(c.finish())
}

fn push_grid<R: ShellRenderer>(
    launcher: &mut Launcher, frame: &mut Frame<'_, R>, icons: &mut IconLoader, layers: &Layers, recede: Recede,
) {
    let grid = grid_for(frame.area);
    let grid_width = f64::from(grid.rect.size.w + CELL_WIDTH);
    let swap = launcher.swap_progress();
    let swap_shift = (launcher.swap_dir * SWAP_SHIFT * (1.0 - swap)).round() as i32;
    let recede = Recede { alpha: recede.alpha * glide_theme::motion::assist(swap), ..recede };
    let ring = launcher.selected.filter(|_| !launcher.pointer_selected);
    for (range, offset) in launcher.page_passes() {
        let shift = (offset * grid_width).round() as i32 + swap_shift;
        for (slot, result) in range.enumerate() {
            let Some(index) = launcher.app_index(result) else {
                continue;
            };
            let row = (slot / grid.columns.max(1)).min(MAX_ROWS - 1);
            frame.layer_in(layers.rows[row], ROW_RISE, recede);
            let mut cell = grid.cell(slot);
            cell.loc.x += shift;
            let rect = icon_rect_in(cell);
            let cell_hover_t = glide_theme::motion::E_OVER.ease_f64(launcher.hover.level(result, hover_anim()));
            let layer_t = launcher.hover_layer_alpha(index);
            if ring == Some(result) {
                push_focus_ring(launcher, frame, cell);
            }
            push_icon(launcher, frame, icons, index, rect, cell_hover_t);
            push_caption(launcher, frame, index, rect, cell_hover_t);
            push_cell_layer(launcher, frame, cell, layer_t);
        }
    }
}

fn push_focus_ring<R: ShellRenderer>(launcher: &mut Launcher, frame: &mut Frame<'_, R>, cell: Rectangle<i32, Logical>) {
    let (theme, output_scale) = (frame.theme, frame.output_scale);
    let side = FOCUS_RING;
    let (width, height) = (CELL_WIDTH + side * 2, CELL_HEIGHT + side * 2);
    if let Some(buffer) = cached(&mut launcher.focus_ring, output_scale, || {
        draw_ring(width, height, output_scale, theme.focus_ring, CELL_RADIUS + side as f32, side as f32)
    }) {
        let loc = Point::from((cell.loc.x - side, cell.loc.y - side + frame.rise));
        frame.push(buffer, loc, None, frame.alpha);
    }
}

fn push_menu<R: ShellRenderer>(launcher: &mut Launcher, frame: &mut Frame<'_, R>) {
    let Some(menu) = launcher.menu.clone() else {
        return;
    };
    if !menu.reveal.is_visible(menu_anim()) {
        return;
    }
    let output_scale = frame.output_scale;
    let progress = menu.progress();
    let menu_alpha = progress.clamp(0.0, 1.0) as f32 * frame.alpha;
    let rise = (1.0 - progress) * MENU_RISE;
    let factor = MENU_SCALE + (1.0 - MENU_SCALE) * progress;
    let anchor = (
        f64::from(menu.rect.loc.x) + f64::from(menu.rect.size.w) / 2.0,
        f64::from(menu.rect.loc.y) + f64::from(menu.rect.size.h) + rise,
    );
    let panel = scale_about(menu.rect, anchor, rise, factor);
    let title = launcher
        .app_index(menu.result)
        .and_then(|index| launcher.apps.get(index))
        .map(|app| app.name.clone())
        .unwrap_or_default();

    let key = (title, menu.items.clone(), output_scale);
    if !launcher.menu_cache.contains_key(&key) {
        launcher.menu_cache.retain(|(title, items, _), _| (title, items) == (&key.0, &key.1));
        let rows: Vec<_> = menu
            .items
            .iter()
            .map(|item| crate::popup_menu::MenuRow { label: item.label().into(), danger: false, muted: false })
            .collect();
        if let Some(buffer) =
            crate::popup_menu::draw(frame.painter, &frame.theme.menu, &key.0, &rows, None, output_scale)
        {
            launcher.menu_cache.insert(key.clone(), buffer);
        }
    }
    let row_width = crate::popup_menu::WIDTH - crate::popup_menu::PADDING * 2;
    let hover_color = frame.theme.menu.item_hover;
    for row in 0..menu.items.len() {
        let alpha = layer_alpha(&launcher.menu_rows, row) as f32;
        if alpha <= 0.0 {
            continue;
        }
        if let Some(buffer) = cached(&mut launcher.menu_row_layer, output_scale, || {
            Some(
                filled(
                    row_width,
                    crate::popup_menu::ROW_HEIGHT,
                    output_scale,
                    hover_color,
                    crate::popup_menu::ROW_RADIUS,
                )?
                .finish(),
            )
        }) {
            let source = crate::popup_menu::row_rect(menu.rect, row);
            let target = scale_about(source, anchor, rise, factor);
            frame.push(buffer, target.loc, Some((source.size, target.size)), alpha * menu_alpha);
        }
    }
    if let Some(buffer) = launcher.menu_cache.get(&key) {
        frame.push(buffer, panel.loc, Some((menu.rect.size, panel.size)), menu_alpha);
    }

    let appearance = frame.theme.menu.appearance;
    let shadow_key = (menu.rect.size.w, menu.rect.size.h, output_scale);
    if let Some(buffer) = cached(&mut launcher.menu_shadow, shadow_key, || {
        crate::popup_menu::draw_shadow(menu.rect.size, crate::popup_menu::RADIUS, output_scale, appearance)
    }) {
        let margin = crate::popup_menu::shadow_margin();
        let source = Size::from((menu.rect.size.w + margin * 2, menu.rect.size.h + margin * 2));
        let shadow = scale_about(
            Rectangle::new(Point::from((menu.rect.loc.x - margin, menu.rect.loc.y - margin)), source),
            anchor,
            rise,
            factor,
        );
        frame.push(buffer, shadow.loc, Some((source, shadow.size)), menu_alpha);
    }
}

fn push_icon<R: ShellRenderer>(
    launcher: &mut Launcher, frame: &mut Frame<'_, R>, icons: &mut IconLoader, index: usize,
    rect: Rectangle<i32, Logical>, hover_t: f64,
) {
    ensure_tile(launcher, icons, frame.painter, frame.theme, index, frame.output_scale);
    if let Some(buffer) = launcher.tiles[index].get(&frame.output_scale) {
        let lift = (HOVER_LIFT * hover_t).round() as i32;
        let lifted = Rectangle::new(Point::from((rect.loc.x, rect.loc.y - lift + frame.rise)), rect.size);
        let centre = (
            f64::from(lifted.loc.x) + f64::from(rect.size.w) / 2.0,
            f64::from(lifted.loc.y) + f64::from(rect.size.h) / 2.0,
        );
        let grown = scale_about(lifted, centre, 0.0, 1.0 + (HOVER_SCALE - 1.0) * hover_t);
        frame.push(buffer, grown.loc, Some((rect.size, grown.size)), frame.alpha);
    }
}

fn ensure_tile(
    launcher: &mut Launcher, icons: &mut IconLoader, painter: &crate::paint::Painter, theme: &LauncherTheme,
    index: usize, output_scale: i32,
) {
    let Some(app) = launcher.apps.get(index) else {
        return;
    };
    if launcher.tiles.get(index).is_none_or(|cache| cache.contains_key(&output_scale)) {
        return;
    }
    let known_missing = launcher.icon_missing.get(index).copied().unwrap_or(false);
    let pixels = if known_missing {
        None
    } else {
        icons.load_named(&app.icon, ICON_SIZE * output_scale.max(1)).map(<[u8]>::to_vec)
    };
    let buffer = match pixels {
        Some(pixels) => Some(buffer_from(&pixels, ICON_SIZE, ICON_SIZE, output_scale)),
        None => draw_fallback_icon(painter, theme, &app.name, output_scale),
    };
    if let Some(buffer) = buffer {
        launcher.tiles[index].insert(output_scale, buffer);
    }
}

fn draw_fallback_icon(
    painter: &crate::paint::Painter, theme: &LauncherTheme, name: &str, scale: i32,
) -> Option<MemoryRenderBuffer> {
    let (fill, border) = (theme.fallback_background, theme.fallback_border);
    let mut c = bordered(ICON_SIZE, ICON_SIZE, scale, fill, border, glide_theme::shape::R3)?;
    let style = &glide_theme::TextStyle::FALLBACK_INITIAL;
    c.label(painter, (0, ICON_SIZE), TextAlign::Center, &fallback_initial(name), theme.fallback_label, style);
    Some(c.finish())
}

fn push_caption<R: ShellRenderer>(
    launcher: &mut Launcher, frame: &mut Frame<'_, R>, index: usize, rect: Rectangle<i32, Logical>, hover_t: f64,
) {
    let loc =
        Point::from((rect.loc.x + (ICON_SIZE - CELL_WIDTH) / 2, rect.loc.y + ICON_SIZE + CAPTION_GAP + frame.rise));
    let hover_alpha = hover_t.clamp(0.0, 1.0) as f32;
    let layers =
        [(CaptionState::Rest, frame.alpha * (1.0 - hover_alpha)), (CaptionState::Active, frame.alpha * hover_alpha)];
    let (painter, theme, output_scale) = (frame.painter, frame.theme, frame.output_scale);
    let name = launcher.apps[index].name.clone();
    for (state, layer_alpha) in layers {
        if let Some(buffer) = cached(&mut launcher.captions[index], (output_scale, state), || {
            draw_caption(painter, theme, &name, state, output_scale)
        }) {
            frame.push(buffer, loc, None, layer_alpha);
        }
    }
    if hover_t <= 0.0 {
        return;
    }
    let (width, height) = caption_pill_size(painter, &name);
    let Some(buffer) = cached(&mut launcher.pills, (width, height, output_scale), || {
        Some(
            bordered(
                width,
                height,
                output_scale,
                theme.caption_pill,
                theme.caption_pill_border,
                control_radius(height),
            )?
            .finish(),
        )
    }) else {
        return;
    };
    let (from_x, from_y) = CAPTION_PILL_FROM;
    let shown_w = (f64::from(width) * (1.0 - from_x).mul_add(hover_t, from_x)).round().max(1.0) as i32;
    let shown_h = (f64::from(height) * (1.0 - from_y).mul_add(hover_t.min(1.0), from_y)).round().max(1.0) as i32;
    let centre = (loc.x + CELL_WIDTH / 2, loc.y + height / 2);
    let shown = Point::from((centre.0 - shown_w / 2, centre.1 - shown_h / 2));
    let alpha = frame.alpha * glide_theme::motion::assist(hover_t) as f32;
    frame.push(buffer, shown, Some((Size::from((width, height)), Size::from((shown_w, shown_h)))), alpha);
}

fn caption_pill_size(painter: &crate::paint::Painter, name: &str) -> (i32, i32) {
    let (first, second) = wrap_caption(painter, name, (CELL_WIDTH - CAPTION_PAD_X * 2) as f32);
    let style = &glide_theme::TextStyle::LAUNCHER_CAPTION;
    let text_w = std::iter::once(first.as_str())
        .chain(second.as_deref())
        .map(|text| painter.text_width_styled(text, style))
        .fold(0.0_f32, f32::max)
        .ceil() as i32;
    let height = if second.is_some() { CAPTION_HEIGHT } else { CAPTION_LINE };
    ((text_w + CAPTION_PILL_PAD_X * 2).min(CELL_WIDTH), height)
}

fn draw_caption(
    painter: &crate::paint::Painter, theme: &LauncherTheme, name: &str, state: CaptionState, scale: i32,
) -> Option<MemoryRenderBuffer> {
    let mut c = Canvas::new(CELL_WIDTH, CAPTION_HEIGHT, scale)?;
    let (x_min, x_max) = (CAPTION_PAD_X, CELL_WIDTH - CAPTION_PAD_X);
    let (first, second) = wrap_caption(painter, name, (x_max - x_min) as f32);
    let colour = match state {
        CaptionState::Rest => theme.label,
        CaptionState::Active => theme.label_active,
    };
    let style = &glide_theme::TextStyle::LAUNCHER_CAPTION;

    let bands: &[(i32, i32, &str)] = match &second {
        Some(second) => &[(0, CAPTION_LINE, first.as_str()), (CAPTION_LINE, CAPTION_HEIGHT, second.as_str())],
        None => &[(0, CAPTION_LINE, first.as_str())],
    };
    for &(y_min, y_max, text) in bands {
        c.text(painter, (x_min, x_max, y_min, y_max), TextAlign::Center, text, colour, style);
    }
    Some(c.finish())
}

fn push_cell_layer<R: ShellRenderer>(
    launcher: &mut Launcher, frame: &mut Frame<'_, R>, cell: Rectangle<i32, Logical>, hover_t: f64,
) {
    if hover_t <= 0.0 {
        return;
    }
    let (theme, output_scale) = (frame.theme, frame.output_scale);
    if let Some(buffer) = cached(&mut launcher.cell_layer, output_scale, || {
        Some(
            bordered(CELL_WIDTH, CELL_HEIGHT, output_scale, theme.cell_layer, theme.cell_border, CELL_RADIUS)?.finish(),
        )
    }) {
        let layer_alpha = (frame.alpha * hover_t as f32).clamp(0.0, 1.0);
        let loc = Point::from((cell.loc.x, cell.loc.y + frame.rise));
        frame.push(buffer, loc, None, layer_alpha);
    }
}

fn push_category_chips<R: ShellRenderer>(
    launcher: &mut Launcher, frame: &mut Frame<'_, R>, progress: f64, recede: Recede,
) {
    let (painter, theme, output_scale) = (frame.painter, frame.theme, frame.output_scale);
    frame.layer_in(progress, CHIP_RISE, recede);
    let chips = launcher.layout_chips(painter, frame.area);
    for (slot, (category, rect)) in chips.iter().copied().enumerate() {
        let step = chip_step(launcher.chip_progress(category));
        if let Some(buffer) = cached(&mut launcher.chips, (slot, step, output_scale), || {
            draw_chip(painter, theme, category.label(), rect.size.w, chip_step_progress(step), output_scale)
        }) {
            frame.put(buffer, rect.loc.x, rect.loc.y);
        }
        let hover = ease_io(launcher.chip_hover.level(category, layer_anim())) as f32;
        if hover > 0.0
            && let Some(buffer) = cached(&mut launcher.chip_layers, (rect.size.w, output_scale), || {
                let radius = control_radius(CHIP_HEIGHT);
                Some(filled(rect.size.w, CHIP_HEIGHT, output_scale, theme.cell_layer, radius)?.finish())
            })
        {
            let loc = Point::from((rect.loc.x, rect.loc.y + frame.rise));
            frame.push(buffer, loc, None, frame.alpha * hover);
        }
    }
    let Some(thumb) = chip_thumb_rect(&chips, launcher.chip_position()) else {
        return;
    };
    if launcher.chip_thumbs.len() > CHIP_THUMB_CACHE {
        launcher.chip_thumbs.clear();
    }
    if let Some(buffer) = cached(&mut launcher.chip_thumbs, (thumb.size.w, output_scale), || {
        Some(filled(thumb.size.w, CHIP_HEIGHT, output_scale, theme.chip_active, control_radius(CHIP_HEIGHT))?.finish())
    }) {
        frame.put(buffer, thumb.loc.x, thumb.loc.y);
    }
}

const CHIP_THUMB_CACHE: usize = 48;

fn chip_thumb_rect(
    chips: &[(CategoryFilter, Rectangle<i32, Logical>)], position: f64,
) -> Option<Rectangle<i32, Logical>> {
    let last = chips.len().checked_sub(1)?;
    let position = position.clamp(0.0, page_as_f64(last));
    let below = (position.floor() as usize).min(last);
    let above = (below + 1).min(last);
    let t = position - page_as_f64(below);
    let (from, to) = (chips[below].1, chips[above].1);
    let mix = |a: i32, b: i32| glide_theme::motion::lerp(f64::from(a), f64::from(b), t).round() as i32;
    Some(Rectangle::new(
        Point::from((mix(from.loc.x, to.loc.x), from.loc.y)),
        Size::from((mix(from.size.w, to.size.w).max(1), CHIP_HEIGHT)),
    ))
}

fn draw_chip(
    painter: &crate::paint::Painter, theme: &LauncherTheme, label: &str, width: i32, active_t: f32, scale: i32,
) -> Option<MemoryRenderBuffer> {
    let text = theme.chip_label.lerp(theme.chip_active_label, active_t);
    let mut c = Canvas::new(width, CHIP_HEIGHT, scale)?;
    c.label(painter, (CHIP_PAD_X, width - CHIP_PAD_X), TextAlign::Center, label, text, &CHIP_TEXT);
    Some(c.finish())
}

fn push_search_field<R: ShellRenderer>(launcher: &mut Launcher, frame: &mut Frame<'_, R>) {
    let (painter, theme, output_scale) = (frame.painter, frame.theme, frame.output_scale);
    let rect = search_rect(frame.area);
    let prefix =
        if launcher.run_mode() { format!("{}: ", crate::i18n::t(crate::i18n::Key::RunPrefix)) } else { String::new() };
    let shown = format!("{prefix}{}", launcher.field.text());
    let clear = !launcher.field.is_empty();
    let key = SearchFieldKey {
        selection: launcher.field.selection().map(|range| (range.start + prefix.len(), range.end + prefix.len())),
        shown,
        clear,
    };
    let caret_prefix = &key.shown[..prefix.len() + launcher.field.caret()];
    let caret_x = (painter.text_width_styled(caret_prefix, &SEARCH_TEXT).round() as i32 + search_text_left())
        .min(search_text_right(rect.size.w, clear));
    if launcher.caret_visible()
        && launcher.field.selection().is_none()
        && let Some(buffer) = cached(&mut launcher.caret, output_scale, || {
            Some(filled(CARET_WIDTH, CARET_HEIGHT, output_scale, theme.caret, 0.0)?.finish())
        })
    {
        let y = rect.loc.y + (SEARCH_HEIGHT - CARET_HEIGHT) / 2;
        frame.put(buffer, rect.loc.x + caret_x, y);
    }

    if clear {
        push_clear_button(launcher, frame);
    }

    let cache = keyed(&mut launcher.search_field, key.clone());
    if let Some(buffer) =
        cached(cache, output_scale, || draw_search_field(painter, theme, &key, rect.size.w, output_scale))
    {
        frame.put(buffer, rect.loc.x, rect.loc.y);
    }

    let halo: Size<i32, Logical> = Size::from((rect.size.w + FIELD_HALO * 2, SEARCH_HEIGHT + FIELD_HALO * 2));
    let cache = keyed(&mut launcher.search_halo, halo.w);
    if let Some(buffer) = cached(cache, output_scale, || {
        let radius = control_radius(SEARCH_HEIGHT) + FIELD_HALO as f32;
        draw_ring(halo.w, halo.h, output_scale, theme.search_ring, radius, FIELD_HALO as f32)
    }) {
        frame.put(buffer, rect.loc.x - FIELD_HALO, rect.loc.y - FIELD_HALO);
    }
}

fn push_clear_button<R: ShellRenderer>(launcher: &mut Launcher, frame: &mut Frame<'_, R>) {
    let (theme, output_scale) = (frame.theme, frame.output_scale);
    let glyph = cached(&mut launcher.clear_glyph, output_scale, || {
        let mut c = Canvas::new(CLEAR_SIZE, CLEAR_SIZE, output_scale)?;
        let (glyph, stroke, colour) = (CLEAR_GLYPH, CLEAR_GLYPH_STROKE, theme.search_placeholder);
        crate::paint::draw_close_glyph_over(&mut c.buf, CLEAR_SIZE, CLEAR_SIZE, glyph, stroke, output_scale, colour);
        Some(c.finish())
    });
    let layer = cached(&mut launcher.clear_layer, output_scale, || {
        Some(filled(CLEAR_SIZE, CLEAR_SIZE, output_scale, theme.clear_hover, glide_theme::shape::R2)?.finish())
    });

    let rect = clear_rect(frame.area);
    let loc = Point::from((rect.loc.x, rect.loc.y + frame.rise));
    let hover = launcher.clear_hover.value(layer_anim(), ease_io) as f32;
    for (buffer, layer_alpha) in [(glyph, frame.alpha), (layer, frame.alpha * hover)] {
        if let Some(buffer) = buffer {
            frame.push(buffer, loc, None, layer_alpha);
        }
    }
}

fn draw_search_field(
    painter: &crate::paint::Painter, theme: &LauncherTheme, key: &SearchFieldKey, width: i32, scale: i32,
) -> Option<MemoryRenderBuffer> {
    let query = key.shown.as_str();
    let (fill, border) = (theme.search_background, theme.search_border);
    let mut c = bordered(width, SEARCH_HEIGHT, scale, fill, border, control_radius(SEARCH_HEIGHT))?;
    let glyph_centre_x = SEARCH_PAD_X as f32 + SEARCH_GLYPH / 2.0;
    crate::paint::draw_search_glyph_over(
        &mut c.buf,
        width,
        SEARCH_HEIGHT,
        glyph_centre_x,
        SEARCH_HEIGHT as f32 / 2.0,
        SEARCH_GLYPH,
        SEARCH_GLYPH_STROKE,
        scale,
        theme.search_placeholder,
    );

    let text_left = search_text_left();
    let text_right = search_text_right(width, key.clear);
    if let Some((start, end)) = key.selection
        && let (Some(head), Some(selected)) = (query.get(..start), query.get(..end))
    {
        let style = &SEARCH_TEXT;
        let from = text_left + painter.text_width_styled(head, style).round() as i32;
        let to = (text_left + painter.text_width_styled(selected, style).round() as i32).min(text_right);
        if to > from {
            let band = style.line_height_px.round() as i32;
            c.fill(rect(from, (SEARCH_HEIGHT - band) / 2, to - from, band), theme.selection, glide_theme::shape::R1);
        }
    }
    let (text, colour) = if query.is_empty() {
        (crate::i18n::t(crate::i18n::Key::SearchApps), theme.search_placeholder)
    } else {
        (query, theme.search_text)
    };
    c.label(painter, (text_left, text_right), TextAlign::Left, text, colour, &SEARCH_TEXT);
    Some(c.finish())
}

fn push_empty_state<R: ShellRenderer>(launcher: &mut Launcher, frame: &mut Frame<'_, R>, progress: f64) {
    let (painter, theme, output_scale) = (frame.painter, frame.theme, frame.output_scale);
    frame.layer(progress, LIST_DROP);
    let message = crate::i18n::t(crate::i18n::Key::NoResultsFor).replacen("{}", launcher.field.text(), 1);
    let list = list::list_rect(frame.area);
    let width = list.size.w;
    let cache = keyed(&mut launcher.empty_label, message.clone());
    if let Some(buffer) =
        cached(cache, output_scale, || draw_empty_state(painter, theme, &message, width, output_scale))
    {
        frame.put(buffer, list.loc.x, list.loc.y + EMPTY_TOP);
    }
}

const EMPTY_WELL: i32 = 56;
const EMPTY_GLYPH: f32 = 22.0;
const EMPTY_TOP: i32 = glide_theme::spacing::SP7;
const EMPTY_TITLE: i32 = 22;
const EMPTY_HINT: i32 = 19;

fn draw_empty_state(
    painter: &crate::paint::Painter, theme: &LauncherTheme, message: &str, width: i32, scale: i32,
) -> Option<MemoryRenderBuffer> {
    let gap = glide_theme::spacing::SP3;
    let height = EMPTY_WELL + gap + EMPTY_TITLE + glide_theme::spacing::SP1 + EMPTY_HINT;
    let mut c = Canvas::new(width, height, scale)?;
    let well = rect((width - EMPTY_WELL) / 2, 0, EMPTY_WELL, EMPTY_WELL);
    c.fill(well, theme.glyph_tile, glide_theme::shape::R3);
    crate::paint::draw_search_glyph_over(
        &mut c.buf,
        width,
        height,
        (width / 2) as f32,
        (EMPTY_WELL / 2) as f32,
        EMPTY_GLYPH,
        SEARCH_GLYPH_STROKE,
        scale,
        theme.search_placeholder,
    );
    let title_top = EMPTY_WELL + gap;
    let hint_top = title_top + EMPTY_TITLE + glide_theme::spacing::SP1;
    c.text(
        painter,
        (0, width, title_top, title_top + EMPTY_TITLE),
        TextAlign::Center,
        message,
        theme.row_title,
        &glide_theme::TextStyle::T3,
    );
    c.text(
        painter,
        (0, width, hint_top, hint_top + EMPTY_HINT),
        TextAlign::Center,
        crate::i18n::t(crate::i18n::Key::NoResultsHint),
        theme.row_subtitle,
        &glide_theme::TextStyle::CALL,
    );
    Some(c.finish())
}

fn push_dots<R: ShellRenderer>(launcher: &mut Launcher, frame: &mut Frame<'_, R>, progress: f64, recede: Recede) {
    let pages = launcher.page_count();
    if pages <= 1 {
        return;
    }
    frame.layer_in(progress, DOT_RISE, recede);
    let (theme, output_scale, area) = (frame.theme, frame.output_scale, frame.area);
    let width = dots_width(pages);
    let position = launcher.dot_tween.value(dot_anim(), ease_out);
    let step = (position * DOT_STEPS).round() as i64;
    let cache = keyed(&mut launcher.dots, (pages, step));
    if let Some(buffer) = cached(cache, output_scale, || {
        let mut c = Canvas::new(width, DOT_SIZE, output_scale)?;
        for (x, dot_width, active) in dot_layout(pages, step as f64 / DOT_STEPS) {
            let color = theme.dot.lerp(theme.active_dot, active as f32);
            c.fill(rect(x, 0, dot_width, DOT_SIZE), color, DOT_SIZE as f32 / 2.0);
        }
        Some(c.finish())
    }) {
        frame.put(buffer, area.loc.x + (area.size.w - width) / 2, area.loc.y + area.size.h - DOT_MARGIN - DOT_SIZE);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LauncherTheme {
    pub background: Color,
    pub label: Color,
    pub label_active: Color,
    pub caption_pill: Color,
    pub caption_pill_border: Color,
    pub cell_layer: Color,
    pub cell_border: Color,
    pub fallback_background: Color,
    pub fallback_border: Color,
    pub fallback_label: Color,
    pub search_background: Color,
    pub search_border: Color,
    pub search_ring: Color,
    pub focus_ring: Color,
    pub search_text: Color,
    pub search_placeholder: Color,
    pub chip_background: Color,
    pub chip_border: Color,
    pub chip_label: Color,
    pub chip_active: Color,
    pub chip_active_label: Color,
    pub dot: Color,
    pub active_dot: Color,
    pub caret: Color,
    pub selection: Color,
    pub clear_hover: Color,
    pub row_title: Color,
    pub row_subtitle: Color,
    pub section_label: Color,
    pub glyph_tile: Color,
    pub glyph: Color,
    pub menu: crate::popup_menu::MenuColors,
}

impl LauncherTheme {
    #[must_use]
    pub fn from_dock(dock: &glide_theme::DockTheme, tokens: &glide_theme::Tokens) -> Self {
        Self {
            background: dock.background,
            active_dot: dock.focused_color,
            menu: crate::popup_menu::MenuColors::from_dock(dock),
            ..Self::from_tokens(tokens)
        }
    }

    #[must_use]
    pub fn from_tokens(tokens: &glide_theme::Tokens) -> Self {
        let state = tokens.state.with_alpha(glide_theme::motion::ST_HOVER);
        let clear = tokens.line.with_alpha(0.0);
        let mut dock = glide_theme::DockTheme::default();
        dock.repaint(tokens);
        Self {
            background: tokens.shell_bg,
            label: tokens.fg_2,
            label_active: tokens.fg_1,
            caption_pill: tokens.popover_bg,
            caption_pill_border: tokens.line,
            cell_layer: state,
            cell_border: clear,
            fallback_background: tokens.fill_2,
            fallback_border: clear,
            fallback_label: tokens.fg_2,
            search_background: tokens.fill_1,
            search_border: tokens.acc_bg,
            search_ring: tokens.acc_ring,
            focus_ring: tokens.acc_bg,
            search_text: tokens.fg_1,
            search_placeholder: tokens.fg_3,
            chip_background: clear,
            chip_border: clear,
            chip_label: tokens.fg_2,
            chip_active: tokens.acc_soft,
            chip_active_label: tokens.acc_soft_fg,
            dot: tokens.fg_4,
            active_dot: tokens.acc_bg,
            caret: tokens.acc_bg,
            selection: tokens.acc_soft,
            clear_hover: tokens.fill_2,
            row_title: tokens.fg_1,
            row_subtitle: tokens.fg_3,
            section_label: tokens.fg_3,
            glyph_tile: tokens.fill_2,
            glyph: tokens.fg_2,
            menu: crate::popup_menu::MenuColors::from_dock(&dock),
        }
    }

    #[must_use]
    pub fn revision(&self) -> u64 {
        glide_theme::Revision::new()
            .color(self.background)
            .color(self.label)
            .color(self.label_active)
            .color(self.caption_pill)
            .color(self.caption_pill_border)
            .color(self.cell_layer)
            .color(self.cell_border)
            .color(self.fallback_background)
            .color(self.fallback_border)
            .color(self.fallback_label)
            .color(self.search_background)
            .color(self.search_border)
            .color(self.search_ring)
            .color(self.focus_ring)
            .color(self.search_text)
            .color(self.search_placeholder)
            .color(self.chip_background)
            .color(self.chip_border)
            .color(self.chip_label)
            .color(self.chip_active)
            .color(self.chip_active_label)
            .color(self.dot)
            .color(self.active_dot)
            .color(self.caret)
            .color(self.selection)
            .color(self.clear_hover)
            .color(self.row_title)
            .color(self.row_subtitle)
            .color(self.section_label)
            .color(self.glyph_tile)
            .color(self.glyph)
            .color(self.menu.background)
            .color(self.menu.border)
            .color(self.menu.text)
            .color(self.menu.text_muted)
            .color(self.menu.text_danger)
            .color(self.menu.item_hover)
            .finish()
    }
}

impl Default for LauncherTheme {
    fn default() -> Self {
        Self::from_tokens(&glide_theme::Tokens::new(glide_theme::Appearance::Dark, glide_theme::Accent::Blue))
    }
}

smithay::render_elements! {
    pub LauncherRenderElement<R> where R: ImportAll + ImportMem;
    Buffer = MemoryRenderBufferRenderElement<R>,
    Scaled = RescaleRenderElement<MemoryRenderBufferRenderElement<R>>,
    Solid = SolidColorRenderElement,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icon::AppCategory;

    fn app(id: &str, name: &str, category: AppCategory) -> DesktopApp {
        DesktopApp {
            id: id.to_string(),
            name: name.to_string(),
            command: "true".to_string(),
            icon: "application-x-executable".to_string(),
            category,
            ..DesktopApp::default()
        }
    }

    fn apps(count: usize) -> Vec<DesktopApp> {
        (0..count).map(|i| app(&format!("app-{i}"), &format!("App {i}"), AppCategory::Other)).collect()
    }

    fn named(names: &[&str]) -> Vec<DesktopApp> {
        names.iter().map(|name| app(&name.to_lowercase(), name, AppCategory::Other)).collect()
    }

    fn result_names(launcher: &Launcher) -> Vec<&str> {
        launcher.results.iter().map(|&i| launcher.apps[i].name.as_str()).collect()
    }

    fn typed(launcher: &mut Launcher, text: &str) {
        for ch in text.chars() {
            launcher.push_query(ch);
        }
    }

    fn centre(rect: Rectangle<i32, Logical>) -> Point<f64, Logical> {
        Point::from((f64::from(rect.loc.x + rect.size.w / 2), f64::from(rect.loc.y + rect.size.h / 2)))
    }

    fn rect(w: i32, h: i32) -> Rectangle<i32, Logical> {
        Rectangle::new(Point::from((0, 0)), Size::from((w, h)))
    }

    fn area() -> Rectangle<i32, Logical> {
        rect(1920, 1000)
    }

    fn launcher(count: usize) -> Launcher {
        Launcher::new(apps(count), area())
    }

    fn per_page() -> usize {
        grid_for(area()).per_page()
    }

    mod search {
        use super::*;

        fn browsers() -> Launcher {
            Launcher::new(
                named(&["Files", "Firefox", "Text Editor", "Chromium", "Fire Emblem", "Disk Usage Analyzer"]),
                area(),
            )
        }

        #[test]
        fn typing_narrows_the_grid_ignoring_case_and_backspace_restores_it() {
            let mut launcher = browsers();
            assert_eq!(launcher.results.len(), 6);
            assert_eq!(result_names(&launcher)[0], "Files");
            assert_eq!(launcher.selected(), None, "nothing is preselected");
            assert!(!launcher.push_query('\n') && !launcher.push_query('\t'));
            for ch in "fire".chars() {
                assert!(launcher.push_query(ch));
            }
            assert_eq!(launcher.query(), "fire");
            assert_eq!(result_names(&launcher), vec!["Firefox", "Fire Emblem"]);
            for _ in 0..4 {
                assert!(launcher.pop_query());
            }
            assert!(!launcher.pop_query(), "nothing left to delete");
            assert_eq!(launcher.results.len(), 6);
            typed(&mut launcher, "FIREFOX");
            assert_eq!(result_names(&launcher), vec!["Firefox"]);
            assert!(launcher.clear_query());
            assert!(!launcher.clear_query(), "an empty query lets Escape close the grid");
            typed(&mut launcher, "zzz");
            assert!(launcher.results.is_empty() && launcher.selected().is_none());
        }

        #[test]
        fn a_name_prefix_outranks_a_word_prefix_which_outranks_a_substring() {
            let mut launcher = Launcher::new(named(&["Disk Usage Analyzer", "Use Me", "Usage Meter"]), area());
            typed(&mut launcher, "us");
            assert_eq!(
                result_names(&launcher),
                vec!["Use Me", "Usage Meter", "Disk Usage Analyzer"],
                "prefix, then word-prefix, then a plain substring"
            );
        }

        #[test]
        fn the_best_match_is_preselected_and_slots_index_the_app_list() {
            let mut launcher = browsers();
            launcher.push_query('f');
            assert_eq!(launcher.selected(), Some(0));
            let name = |l: &Launcher| l.app_index(0).map(|i| l.apps[i].name.clone());
            assert_eq!(name(&launcher).as_deref(), Some("Files"));
            launcher.clear_query();
            typed(&mut launcher, "chrom");
            assert_eq!(launcher.results.len(), 1);
            assert_eq!(launcher.app_index(0), Some(3), "slot 0 runs Chromium, not Files");

            let mut paged = self::launcher(per_page() + 5);
            assert!(paged.turn_page(true));
            paged.push_query('a');
            assert_eq!(paged.page, 0, "a query never hides results on page two");
        }
    }

    mod categories {
        use super::*;

        fn mixed() -> Launcher {
            let apps = vec![
                app("firefox", "Firefox", AppCategory::Internet),
                app("fire-editor", "Fire Editor", AppCategory::Office),
                app("chromium", "Chromium", AppCategory::Internet),
            ];
            Launcher::new(apps, area())
        }

        #[test]
        fn chips_exist_only_for_filled_categories_and_toggle_with_the_query() {
            let chips = mixed().categories();
            for (filter, offered) in [
                (CategoryFilter::All, true),
                (CategoryFilter::Only(AppCategory::Internet), true),
                (CategoryFilter::Only(AppCategory::Office), true),
                (CategoryFilter::Only(AppCategory::Media), false),
            ] {
                assert_eq!(chips.contains(&filter), offered, "{filter:?}");
            }

            let mut launcher = mixed();
            let internet = CategoryFilter::Only(AppCategory::Internet);
            assert!(!launcher.toggle_category(CategoryFilter::All), "already unfiltered");
            assert!(launcher.toggle_category(internet));
            assert_eq!(result_names(&launcher), vec!["Firefox", "Chromium"]);
            typed(&mut launcher, "fire");
            assert_eq!(result_names(&launcher), vec!["Firefox"], "Fire Editor is Office");
            launcher.clear_query();
            assert_eq!(launcher.category(), CategoryFilter::All, "Escape clears the chip too");
            assert!(!launcher.clear_query());
            assert!(launcher.toggle_category(internet) && launcher.toggle_category(internet));
            assert_eq!((launcher.category(), launcher.results.len()), (CategoryFilter::All, 3));
        }

        #[test]
        fn a_category_change_slides_one_thumb_and_brings_the_grid_in_from_that_side() {
            let mut launcher = mixed();
            let office = CategoryFilter::Only(AppCategory::Office);
            assert!(launcher.toggle_category(office));
            let slot = launcher.chip_slot(office);
            assert!(slot > 0.0);
            assert!((launcher.chip_thumb.target() - slot).abs() < f64::EPSILON, "one thumb heads for the chip");
            assert!((launcher.swap_dir - 1.0).abs() < f64::EPSILON, "the new set comes in from the right");
            if !swap_anim().is_zero() {
                assert!(launcher.swap_progress() < 1.0, "the grid travels in rather than appearing");
            }
            assert!(launcher.toggle_category(office));
            assert!((launcher.swap_dir + 1.0).abs() < f64::EPSILON, "going back to All comes in from the left");
        }

        #[test]
        fn a_launch_starts_from_the_icon_that_was_activated() {
            use crate::launcher::search::Entry;
            let mut launcher = mixed();
            let grid_rect = launcher.launch_rect(&Entry::App(2)).expect("Chromium is on the first page");
            let slot = launcher.page_range().position(|result| launcher.app_index(result) == Some(2)).expect("slot");
            assert_eq!(grid_rect, launcher.icon_rects(area())[slot]);
            typed(&mut launcher, "chrom");
            let row = launcher.launch_rect(&Entry::App(2)).expect("the list row");
            assert_eq!(row.size, Size::from((40, 40)), "a list row hands over its 40 px icon");
            assert!(launcher.launch_rect(&Entry::Run("x".into())).is_none());

            let now = Instant::now();
            let origin = LaunchOrigin { key: "Chromium".into(), rect: row, at: now };
            assert!(origin.claims("chromium", now), "dock keys compare without case");
            assert!(!origin.claims("firefox", now));
            assert!(!origin.claims("chromium", now + crate::dock::LAUNCH_TIMEOUT), "a stale launch is ignored");
        }

        #[test]
        fn the_thumb_spans_the_chips_it_travels_between() {
            let chips = vec![
                (CategoryFilter::All, Rectangle::new(Point::from((100, 10)), Size::from((64, CHIP_HEIGHT)))),
                (
                    CategoryFilter::Only(AppCategory::Office),
                    Rectangle::new(Point::from((172, 10)), Size::from((100, CHIP_HEIGHT))),
                ),
            ];
            let start = chip_thumb_rect(&chips, 0.0).expect("thumb");
            let middle = chip_thumb_rect(&chips, 0.5).expect("thumb");
            let end = chip_thumb_rect(&chips, 1.0).expect("thumb");
            assert_eq!((start.loc.x, start.size.w), (100, 64));
            assert_eq!((middle.loc.x, middle.size.w), (136, 82));
            assert_eq!((end.loc.x, end.size.w), (172, 100));
            assert!(chip_thumb_rect(&[], 0.0).is_none());
        }

        #[test]
        fn a_chip_click_hits_only_a_drawn_chip_under_the_pointer() {
            let mut launcher = mixed();
            assert_eq!(launcher.category_at(Point::from((960.0, 130.0))), None, "not drawn yet");
            let painter = crate::paint::Painter::new(14.0, None);
            let chips = launcher.layout_chips(&painter, area());
            assert!(chips.len() >= 2);
            for (category, rect) in chips {
                assert_eq!(launcher.category_at(centre(rect)), Some(category));
            }
            assert_eq!(launcher.category_at(Point::from((5.0, 5.0))), None);
        }

        #[test]
        fn one_lone_category_is_not_worth_a_chip_row() {
            let launcher = launcher(4);
            assert_eq!(launcher.categories().len(), 2, "the fixtures are all Other, so only All and Other qualify");

            let mut single = Launcher::new(Vec::new(), area());
            let painter = crate::paint::Painter::new(14.0, None);
            assert!(
                single.layout_chips(&painter, area()).is_empty(),
                "a row with nothing to choose between must not be drawn"
            );
        }
    }

    mod keyboard_selection {
        use super::*;

        #[test]
        fn arrows_walk_cells_and_rows_and_stop_at_the_ends() {
            let columns = grid_for(area()).columns;
            let mut launcher = launcher(per_page());
            assert_eq!(launcher.selected(), None);
            assert!(launcher.move_selection(1, 0));
            assert_eq!(launcher.selected(), Some(0), "the first press picks cell 0");
            assert!(!launcher.move_selection(-1, 0), "already at the first cell");
            launcher.move_selection(1, 0);
            assert_eq!(launcher.selected(), Some(1));
            launcher.move_selection(-1, 0);
            launcher.move_selection(0, 1);
            assert_eq!(launcher.selected(), Some(columns), "down steps a whole row");

            let mut short = self::launcher(3);
            for _ in 0..10 {
                short.move_selection(1, 0);
            }
            assert_eq!(short.selected(), Some(2), "clamped to the last result");
        }

        #[test]
        fn moving_past_the_end_of_a_page_turns_it() {
            let full = per_page();
            let mut launcher = launcher(full + 3);
            launcher.move_selection(1, 0);
            for _ in 0..full {
                launcher.move_selection(1, 0);
            }
            assert_eq!(launcher.page, 1, "the selection dragged the page with it");
            assert_eq!(launcher.selected(), Some(full));
        }

        #[test]
        fn an_empty_result_list_has_nothing_to_move_through() {
            let mut launcher = launcher(0);
            assert!(!launcher.move_selection(1, 0));
            assert_eq!(launcher.selected(), None);
        }
    }

    mod pagination_dots {
        use super::*;

        fn paged() -> Launcher {
            launcher(per_page() * 2 + 1)
        }

        fn dot_centre(launcher: &Launcher, page: usize) -> Point<f64, Logical> {
            let strip = dots_rect(area(), launcher.page_count());
            let mut x = strip.loc.x;
            for candidate in 0..page {
                let width = if candidate == launcher.page { DOT_ACTIVE_WIDTH } else { DOT_SIZE };
                x += width + DOT_GAP;
            }
            Point::from((f64::from(x + DOT_SIZE / 2), f64::from(strip.loc.y + strip.size.h / 2)))
        }

        #[test]
        fn dots_report_and_jump_to_their_page_only_when_there_is_more_than_one() {
            let mut launcher = paged();
            for page in [1, 2] {
                assert_eq!(launcher.dot_at(area(), dot_centre(&launcher, page)), Some(page));
            }
            assert_eq!(launcher.dot_at(area(), Point::from((5.0, 5.0))), None);
            assert!(launcher.turn_page(true));
            assert!((launcher.dot_tween.target() - 1.0).abs() < f64::EPSILON);
            assert!(launcher.go_to_page(2));
            assert_eq!(launcher.page, 2);
            assert!(!launcher.go_to_page(2), "already there");
            assert!(!launcher.go_to_page(99), "out of range");

            let single = self::launcher(3);
            assert_eq!(single.page_count(), 1);
            assert_eq!(single.dot_at(area(), dot_centre(&single, 0)), None);
        }

        #[test]
        fn the_active_dot_is_a_wide_pill_that_slides_its_width_and_accent() {
            let dots = dot_layout(3, 1.0);
            let widths: Vec<i32> = dots.iter().map(|d| d.1).collect();
            assert_eq!(widths, [DOT_SIZE, DOT_ACTIVE_WIDTH, DOT_SIZE]);
            assert!((dots[1].2 - 1.0).abs() < f64::EPSILON);
            assert_eq!(dots[2].0 + dots[2].1, dots_width(3), "the strip spans the dots");

            let dots = dot_layout(3, 0.5);
            assert!(dots[0].1 > DOT_SIZE && dots[0].1 < DOT_ACTIVE_WIDTH);
            assert_eq!(dots[0].1, dots[1].1);
            assert!((dots[0].2 + dots[1].2 - 1.0).abs() < 1e-9);
            assert_eq!(dots[2].1, DOT_SIZE);
        }
    }

    #[test]
    fn captions_wrap_at_word_boundaries_into_two_lines_that_fit() {
        let painter = crate::paint::Painter::new(14.0, None);
        let width = (CELL_WIDTH - CAPTION_PAD_X * 2) as f32;
        let wrap = |name| wrap_caption(&painter, name, width);
        assert_eq!(wrap("Files"), ("Files".to_string(), None));
        let word = "Supercalifragilisticexpialidocious";
        assert_eq!(wrap(word), (word.to_string(), None), "no boundary to break on");

        let name = "Advanced Network Configuration";
        let (first, second) = wrap(name);
        let second = second.expect("a name this long needs a second line");
        assert!(name.starts_with(&first) && name.ends_with(&second));
        assert!(!first.contains("  ") && !second.starts_with(' '));
        let (first, _) = wrap("GNOME System Monitor");
        let style = glide_theme::TextStyle::LAUNCHER_CAPTION;
        assert!(painter.text_width_styled(&first, &style) <= width);

        let name = "Qt V4L2 video capture test utility";
        let (first, second) = wrap(name);
        let joined = second.map_or_else(|| first.clone(), |second| format!("{first} {second}"));
        assert!(joined.split_whitespace().eq(name.split_whitespace()));

        for (name, initial) in [("Firefox", "F"), ("  gimp", "G"), ("7-Zip", "7"), ("", "?"), ("--- ", "?")] {
            assert_eq!(fallback_initial(name), initial, "{name:?}");
        }
    }

    const _: () = assert!(
        CELL_PAD_TOP + ICON_SIZE + CAPTION_GAP + CAPTION_HEIGHT <= CELL_HEIGHT
            && (CELL_WIDTH - ICON_SIZE) / 2 >= 24
            && CAPTION_HEIGHT == CAPTION_LINE * CAPTION_LINES
    );

    #[test]
    fn the_icon_is_centred_in_a_cell_with_room_for_two_caption_lines() {
        assert!(CAPTION_LINE as f32 >= glide_theme::TextStyle::LAUNCHER_CAPTION.line_height_px);
        let icon = launcher(1).icon_rects(area())[0];
        let cell = grid_for(area()).cell(0);
        let right = (cell.loc.x + CELL_WIDTH) - (icon.loc.x + ICON_SIZE);
        assert_eq!(icon.loc.x - cell.loc.x, right);
    }

    #[test]
    fn search_chips_grid_and_dots_stack_centred_without_overlapping() {
        let area = area();
        let centred = |rect: Rectangle<i32, Logical>| {
            let right = (area.loc.x + area.size.w) - (rect.loc.x + rect.size.w);
            ((rect.loc.x - area.loc.x) - right).abs() <= 1
        };
        let (search, grid) = (search_rect(area), grid_for(area).rect);
        assert!(centred(search) && centred(grid));
        assert!(search.loc.y + search.size.h <= grid.loc.y);
        let grid_bottom = grid.loc.y + grid.size.h;
        assert!(grid_bottom <= dots_rect(area, 3).loc.y);
        assert!(grid_bottom <= area.loc.y + area.size.h - DOT_MARGIN - DOT_SIZE);

        let mut launcher = launcher(6);
        let painter = crate::paint::Painter::new(14.0, None);
        let chips = launcher.layout_chips(&painter, area);
        for (_, rect) in &chips {
            assert!(rect.loc.y >= search.loc.y + search.size.h);
            assert!(rect.loc.y + rect.size.h <= grid.loc.y);
        }
        if let (Some(first), Some(last)) = (chips.first(), chips.last()) {
            let row = Rectangle::new(first.1.loc, Size::from((last.1.loc.x + last.1.size.w - first.1.loc.x, 1)));
            assert!(centred(row));
        }

        let narrow = rect(420, 900);
        let search = search_rect(narrow);
        assert!(search.loc.x >= narrow.loc.x && search.size.w <= narrow.size.w);
    }

    #[test]
    fn the_grid_grows_with_the_screen_between_its_bounds() {
        let grid = grid_for(area());
        assert!(grid.per_page() > 24, "more than the old fixed 6x4");
        assert!(f64::from(grid.rect.size.w) / f64::from(area().size.w) > 0.55);
        let small = grid_for(rect(640, 480));
        assert!(small.columns >= MIN_COLUMNS && small.rows >= MIN_ROWS);
        let huge = grid_for(rect(7680, 4320));
        assert_eq!((huge.columns, huge.rows), (MAX_COLUMNS, MAX_ROWS));
        let columns: Vec<usize> =
            [800, 1280, 1600, 1920, 2560, 3840].into_iter().map(|width| grid_for(rect(width, 1000)).columns).collect();
        assert!(columns.windows(2).all(|pair| pair[0] <= pair[1]), "{columns:?}");
    }

    #[test]
    fn a_right_click_menu_opens_above_the_app_and_offers_pin_or_unpin() {
        let mut launcher = launcher(4);
        let icon = launcher.icon_rects(area())[2];
        assert!(launcher.open_menu(2, area(), false));
        let menu = launcher.menu_rect().expect("menu rect");
        assert!(menu.loc.y + menu.size.h < icon.loc.y);
        assert_eq!(launcher.menu_result(), Some(2));

        let centre = centre(crate::popup_menu::row_rect(menu, 1));
        assert_eq!(launcher.menu_item_at(centre), Some(LauncherMenuItem::Pin));
        assert!(launcher.set_menu_hover(centre));
        assert!(!launcher.set_menu_hover(centre));
        assert!(launcher.close_menu());
        assert!(!launcher.close_menu());

        assert!(launcher.open_menu(2, area(), true));
        assert_eq!(launcher.menu_item_at(centre), Some(LauncherMenuItem::Unpin));
        assert!(!launcher.open_menu(99, area(), false), "no such result");
    }

    #[test]
    fn closing_the_launcher_plays_the_menu_close_reveal_and_drops_its_hit_area() {
        if menu_anim().is_zero() {
            return;
        }
        let mut launcher = launcher(4);
        assert!(launcher.open_menu(2, area(), false));
        let menu = launcher.menu_rect().expect("menu rect");
        let centre = centre(crate::popup_menu::row_rect(menu, 0));

        launcher.begin_close();
        assert!(launcher.open_menu_ref().is_none());
        assert!(
            launcher.menu.as_ref().is_some_and(|menu| menu.reveal.is_visible(menu_anim())),
            "the menu fades out instead of vanishing"
        );
        assert!(launcher.is_animating());
        assert_eq!(launcher.menu_item_at(centre), None);
        assert_eq!(launcher.menu_rect(), None);
    }

    #[test]
    fn icons_lay_out_row_major() {
        let columns = grid_for(area()).columns;
        let launcher = launcher(per_page());
        let rects = launcher.icon_rects(area());
        assert_eq!(rects.len(), per_page());

        for pair in rects[..columns].windows(2) {
            assert_eq!(pair[0].loc.y, pair[1].loc.y);
            assert_eq!(pair[1].loc.x - pair[0].loc.x, CELL_WIDTH);
        }
        assert_eq!(rects[columns].loc.x, rects[0].loc.x);
        assert_eq!(rects[columns].loc.y - rects[0].loc.y, CELL_HEIGHT);
    }

    #[test]
    fn pages_hold_what_fits_and_paging_stops_at_both_ends() {
        let full = per_page();
        assert_eq!(launcher(full).page_count(), 1);
        assert_eq!(Launcher::new(Vec::new(), area()).page_count(), 1);
        let mut launcher = launcher(full + 6);
        assert_eq!(launcher.page_count(), 2);
        assert_eq!(launcher.page_range(), 0..full);
        assert!(!launcher.turn_page(false), "already on the first page");
        launcher.set_hover(Some(2));
        assert!(launcher.turn_page(true));
        assert_eq!(launcher.hover.target(), None, "turning drops a stale hover");
        assert_eq!(launcher.page_range(), full..full + 6);
        assert_eq!(launcher.icon_rects(area()).len(), 6);
        assert!(!launcher.turn_page(true), "already on the last page");
    }

    #[test]
    fn growing_the_area_clamps_the_current_page_instead_of_stranding_it() {
        let small = rect(640, 480);
        let small_page = grid_for(small).per_page();
        assert!(small_page < per_page(), "the two areas must disagree for this to test anything");

        let mut launcher = Launcher::new(apps(small_page + 1), small);
        assert!(launcher.turn_page(true));
        assert_eq!(launcher.page, 1);

        launcher.set_area(area());
        assert_eq!(launcher.page_count(), 1);
        assert_eq!(launcher.page, 0, "the page must not point past the end");
        assert!(!launcher.page_range().is_empty());
    }

    #[test]
    fn cells_and_captions_hit_their_own_app_and_everything_else_hits_nothing() {
        let grid = grid_for(area());
        let full = launcher(grid.per_page());
        for slot in 0..grid.per_page() {
            assert_eq!(full.app_at(area(), centre(grid.cell(slot))), Some(slot));
        }
        assert_eq!(full.app_at(area(), Point::from((5.0, 5.0))), None);
        let short = launcher(3);
        assert_eq!(short.app_at(area(), centre(grid.cell(3))), None);
        let icon = short.icon_rects(area())[0];
        let caption =
            Point::from((f64::from(icon.loc.x + icon.size.w / 2), f64::from(icon.loc.y + ICON_SIZE + CAPTION_GAP + 2)));
        assert_eq!(short.app_at(area(), caption), Some(0));
    }

    mod animation {
        use super::*;

        #[test]
        fn layers_enter_one_after_another_inside_one_scene() {
            let early = layers(0.3, 1.0);
            assert!(early.field > early.chips && early.chips > early.rows[0], "{early:?}");
            assert!(early.rows.windows(2).all(|pair| pair[0] >= pair[1]), "rows follow top to bottom: {early:?}");
            let done = layers(1.0, 1.0);
            assert!(done.rows.iter().chain([&done.field, &done.chips, &done.dots]).all(|v| (v - 1.0).abs() < 1e-9));
            assert!((done.scale - 1.0).abs() < 1e-9, "the last row lands with the end of the scene");
            let start = layers(0.0, 1.0);
            assert!((start.scale - OPEN_SCALE).abs() < 1e-9 && start.field == 0.0 && start.dots == 0.0);
        }

        #[test]
        fn movement_not_opacity_carries_every_layer() {
            for travel in [FIELD_RISE, CHIP_RISE, ROW_RISE, DOT_RISE, GRID_SINK, SWAP_SHIFT, LIST_DROP.abs()] {
                assert!(travel >= 24.0, "scene content travels at least 24 px (AGENTS.md §8): {travel}");
            }
            const { assert!(1.0 - OPEN_SCALE >= 0.06, "the scene also grows, at least 6 %") };
            let row = list_row_progress(0.2, 0);
            assert!(glide_theme::motion::assist(row) >= row, "opacity leads the travel instead of carrying it");
        }

        #[test]
        fn a_plain_close_is_shorter_than_the_opening_and_moves_every_layer_together() {
            let mut launcher = launcher(1);
            launcher.phase = Phase::Open;
            launcher.begin_close();
            assert_eq!(launcher.duration(), close_duration());
            assert!(close_duration() <= open_duration());
            launcher.started = Instant::now().checked_sub(close_duration() / 2).expect("process uptime");
            let halfway = launcher.layers();
            if !close_duration().is_zero() {
                assert!(halfway.field < 1.0 && halfway.field > 0.0);
            }
            assert!(halfway.rows.iter().all(|row| (row - halfway.field).abs() < 1e-9), "no cascade on the way out");
        }

        #[test]
        fn reopening_mid_close_carries_on_from_where_the_scene_is() {
            let mut launcher = launcher(1);
            launcher.phase = Phase::Open;
            launcher.begin_close();
            launcher.started = Instant::now().checked_sub(close_duration() / 2).expect("process uptime");
            let before = launcher.progress();
            launcher.reopen();
            assert!((launcher.progress() - before).abs() < 0.05, "{before} vs {}", launcher.progress());
        }

        #[test]
        fn typing_sends_the_grid_away_and_brings_the_list_out_of_the_field() {
            let mut launcher = launcher(4);
            assert!(launcher.list.target() < f64::EPSILON);
            typed(&mut launcher, "app");
            assert!((launcher.list.target() - 1.0).abs() < f64::EPSILON);
            const { assert!(LIST_DROP < 0.0, "rows come down from the field, not up from the grid") };
            assert!(list_row_progress(0.3, 0) > list_row_progress(0.3, 4), "rows follow one another");
            launcher.clear_text();
            assert!(launcher.list.target() < f64::EPSILON);
        }

        #[test]
        fn the_scene_runs_one_linear_progress_and_the_backdrop_eases_on_e_io() {
            let scene = Duration::from_millis(400);
            let mut launcher = launcher(1).with_scene(scene);
            assert_eq!(launcher.duration(), scene, "the overview's scene length, not a shorter one of its own");
            launcher.started = Instant::now().checked_sub(scene / 4).expect("process uptime");
            let (content, backdrop) = (launcher.openness(), launcher.backdrop_openness());
            assert!(content > backdrop, "the backdrop eases in and out like the overview's: {content} vs {backdrop}");
            assert_eq!(open_duration(), glide_theme::motion::t4());
            launcher.phase = Phase::Open;
            assert!((launcher.backdrop_openness() - 1.0).abs() < f64::EPSILON);
        }

        #[test]
        fn it_opens_to_one_closes_back_and_asks_to_be_dropped_when_done() {
            let mut launcher = launcher(1);
            assert!(launcher.progress() < 1.0);
            launcher.phase = Phase::Open;
            assert!((launcher.progress() - 1.0).abs() < f64::EPSILON);
            launcher.begin_close();
            assert!(launcher.is_closing() && launcher.progress() <= 1.0);
            launcher.started =
                Instant::now().checked_sub(open_duration() * 2 + Duration::from_millis(10)).expect("process uptime");
            assert!(!launcher.tick(), "a finished close must not linger");
        }

        #[test]
        fn a_cell_the_pointer_leaves_fades_down_instead_of_snapping() {
            if hover_anim().is_zero() {
                return;
            }
            let mut launcher = launcher(12);
            launcher.set_hover(Some(2));
            std::thread::sleep(hover_anim() / 2);
            launcher.set_hover(Some(3));
            assert!(launcher.hover.level(2, hover_anim()) > 0.0);
            assert!(launcher.is_animating());
        }
    }

    mod pointer {
        use super::*;

        #[test]
        fn leaving_drops_a_pointer_highlight_but_keeps_keyboard_and_list_selections() {
            let mut launcher = launcher(12);
            launcher.set_hover(Some(4));
            assert_eq!(launcher.selected(), Some(4));
            launcher.move_selection(1, 0);
            assert_eq!(launcher.selected(), Some(5), "arrows continue from the hover");
            let mut pointer = self::launcher(12);
            pointer.set_hover(Some(4));
            pointer.set_hover(None);
            assert_eq!((pointer.selected(), pointer.hover.target()), (None, None));

            let mut keyboard = self::launcher(12);
            keyboard.move_selection(1, 0);
            keyboard.set_hover(None);
            assert_eq!(keyboard.selected(), Some(0));

            let mut list = Launcher::new(named(&["Files", "Firefox"]), area());
            typed(&mut list, "fi");
            list.set_hover(Some(1));
            list.set_hover(None);
            assert_eq!(list.selected(), Some(1), "Enter still launches the entry");
        }

        #[test]
        fn one_touchpad_swipe_turns_one_page_however_long_it_runs() {
            let mut launcher = launcher(per_page() * 3 + 1);
            let mut turned = 0;
            for _ in 0..40 {
                if launcher.scroll_by(20.0, true) {
                    turned += 1;
                }
            }
            assert_eq!((turned, launcher.page), (1, 1));
            launcher.end_scroll();
            assert!(launcher.scroll_by(20.0, true), "a new swipe turns again");
            assert_eq!(launcher.page, 2);
        }

        #[test]
        fn wheel_detents_turn_a_page_each() {
            let mut launcher = launcher(per_page() * 3 + 1);
            assert!(launcher.scroll_by(15.0, false));
            assert!(launcher.scroll_by(15.0, false));
            assert_eq!(launcher.page, 2);
        }

        #[test]
        fn a_chip_under_the_pointer_takes_the_hover_layer() {
            let mut launcher = launcher(4);
            let chip = CategoryFilter::Only(AppCategory::Other);
            assert!(launcher.set_chip_hover(Some(chip)));
            assert!(!launcher.set_chip_hover(Some(chip)));
            assert_eq!(launcher.chip_hover.target(), Some(chip));
            assert!(launcher.set_chip_hover(None));
        }
    }
}

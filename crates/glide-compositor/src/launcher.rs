use std::time::{Duration, Instant};

use smithay::{
    backend::renderer::{
        ImportAll, ImportMem, Renderer,
        element::{
            Kind,
            memory::{MemoryRenderBuffer, MemoryRenderBufferRenderElement},
            solid::{SolidColorBuffer, SolidColorRenderElement},
        },
    },
    utils::{Logical, Point, Rectangle, Scale, Size, Transform},
};

use crate::{
    config::Color,
    icon::{AppCategory, DesktopApp, IconLoader},
};

const MIN_COLUMNS: usize = 5;
const MAX_COLUMNS: usize = 9;
const MIN_ROWS: usize = 3;
const MAX_ROWS: usize = 5;

const ICON_SIZE: i32 = 88;

#[must_use]
pub const fn icon_side() -> i32 {
    ICON_SIZE
}
const CELL_WIDTH: i32 = 172;
const CELL_HEIGHT: i32 = 176;
const CELL_PAD_TOP: i32 = 12;
const CAPTION_GAP: i32 = 10;
const CAPTION_LINE: i32 = 19;
const CAPTION_LINES: i32 = 2;
const CAPTION_HEIGHT: i32 = CAPTION_LINE * CAPTION_LINES;
const CAPTION_PAD_X: i32 = glide_theme::spacing::SP2;
const HOVER_LIFT: f64 = 3.0;

const SIDE_MARGIN: i32 = 128;
const TOP_MARGIN: i32 = 40;
const DOTS_BAND: i32 = 64;

const DOT_SIZE: i32 = 8;
const DOT_ACTIVE_WIDTH: i32 = 22;
const DOT_GAP: i32 = 10;
const DOT_MARGIN: i32 = 32;
const DOT_HIT_PAD: i32 = 12;

const SEARCH_HEIGHT: i32 = 42;
const SEARCH_WIDTH: i32 = 460;
const SEARCH_GAP: i32 = 18;

const CHIP_HEIGHT: i32 = 28;
const CHIP_STEPS: u8 = 24;
const CHIP_GAP: i32 = 8;
const CHIP_PAD_X: i32 = glide_theme::spacing::SP3;
const CHIP_MIN_WIDTH: i32 = 56;
const CHIP_BAND_GAP: i32 = 32;

const SEARCH_BAND: i32 = TOP_MARGIN + SEARCH_HEIGHT + SEARCH_GAP + CHIP_HEIGHT + CHIP_BAND_GAP;
const SEARCH_PAD_X: i32 = glide_theme::spacing::SP4;
const SEARCH_GLYPH: f32 = 15.0;
const SEARCH_GLYPH_STROKE: f32 = 1.6;

const CELL_RADIUS: f32 = glide_theme::shape::R4;

const EMPTY_HEIGHT: i32 = 24;

const MENU_RISE: f64 = 6.0;
const MENU_SCALE: f64 = 0.94;

fn pill_radius(height: i32) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    glide_theme::shape::R_ROUND.min(height as f32 / 2.0)
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
            Point::from((
                self.rect.loc.x + column * CELL_WIDTH,
                self.rect.loc.y + row * CELL_HEIGHT,
            )),
            Size::from((CELL_WIDTH, CELL_HEIGHT)),
        )
    }
}

fn dots_width(pages: usize) -> i32 {
    (i32::try_from(pages).unwrap_or(1) - 1) * (DOT_SIZE + DOT_GAP) + DOT_ACTIVE_WIDTH
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
        Point::from((
            area.loc.x + (area.size.w - width) / 2,
            area.loc.y + TOP_MARGIN,
        )),
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

    fn ordinal(self) -> usize {
        Self::ORDER
            .iter()
            .position(|candidate| *candidate == self)
            .unwrap_or(0)
    }
}

fn chip_width(painter: &crate::paint::Painter, label: &str) -> i32 {
    #[allow(clippy::cast_possible_truncation)]
    let text = painter
        .text_width_styled(label, &glide_theme::TextStyle::LAUNCHER_CHIP)
        .ceil() as i32;
    (text + CHIP_PAD_X * 2).max(CHIP_MIN_WIDTH)
}

fn grid_for(area: Rectangle<i32, Logical>) -> Grid {
    let usable_w = (area.size.w - SIDE_MARGIN * 2).max(CELL_WIDTH);
    let usable_h = (area.size.h - SEARCH_BAND - DOTS_BAND).max(CELL_HEIGHT);

    let fit = |available: i32, cell: i32, lo: usize, hi: usize| {
        usize::try_from(available / cell)
            .unwrap_or(lo)
            .clamp(lo, hi)
    };
    let mut columns = fit(usable_w, CELL_WIDTH, MIN_COLUMNS, MAX_COLUMNS);
    if columns % 2 == 0 {
        columns = (columns - 1).max(MIN_COLUMNS);
    }
    let rows = fit(usable_h, CELL_HEIGHT, MIN_ROWS, MAX_ROWS);

    let size = Size::from((
        i32::try_from(columns).unwrap_or(1) * CELL_WIDTH,
        i32::try_from(rows).unwrap_or(1) * CELL_HEIGHT,
    ));
    let band_top = area.loc.y + SEARCH_BAND;
    let band_height = (area.size.h - SEARCH_BAND - DOTS_BAND).max(size.h);
    let rect = Rectangle::new(
        Point::from((
            area.loc.x + (area.size.w - size.w) / 2,
            band_top + (band_height - size.h) / 2,
        )),
        size,
    );
    Grid {
        columns,
        rows,
        rect,
    }
}

fn open_duration() -> Duration {
    glide_theme::motion::t3()
}
const OPEN_RISE: f64 = 16.0;
const OPEN_SCALE: f64 = 0.97;

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

fn ease_out(t: f64) -> f64 {
    glide_theme::motion::E_OUT.ease_f64(t)
}

fn ease_io(t: f64) -> f64 {
    glide_theme::motion::E_IO.ease_f64(t)
}

fn page_as_f64(page: usize) -> f64 {
    f64::from(u32::try_from(page).unwrap_or(u32::MAX))
}

fn chip_step(progress: f64) -> u8 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    {
        (progress.clamp(0.0, 1.0) * f64::from(CHIP_STEPS)).round() as u8
    }
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum CaptionState {
    Rest,
    Active,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum MatchRank {
    Prefix,
    WordPrefix,
    Substring,
}

fn match_rank(name: &str, needle: &str) -> Option<MatchRank> {
    let name = name.to_lowercase();
    if name.starts_with(needle) {
        return Some(MatchRank::Prefix);
    }
    if name
        .split(|c: char| c.is_whitespace() || c == '-' || c == '_')
        .any(|word| word.starts_with(needle))
    {
        return Some(MatchRank::WordPrefix);
    }
    name.contains(needle).then_some(MatchRank::Substring)
}

fn wrap_caption(
    painter: &crate::paint::Painter,
    text: &str,
    max_width: f32,
) -> (String, Option<String>) {
    let style = &glide_theme::TextStyle::LAUNCHER_CAPTION;
    if painter.text_width_styled(text, style) <= max_width {
        return (text.to_string(), None);
    }

    let mut first = String::new();
    let mut rest = String::new();
    for word in text.split_whitespace() {
        if rest.is_empty() {
            let candidate = if first.is_empty() {
                word.to_string()
            } else {
                format!("{first} {word}")
            };
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
    if rest.is_empty() {
        (first, None)
    } else {
        (first, Some(rest))
    }
}

fn fallback_initial(name: &str) -> String {
    name.chars()
        .find(|c| c.is_alphanumeric())
        .map_or_else(|| "?".to_string(), |c| c.to_uppercase().to_string())
}

fn filter_apps(apps: &[DesktopApp], query: &str, category: CategoryFilter) -> Vec<usize> {
    let in_category = |app: &DesktopApp| category.accepts(app);

    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return apps
            .iter()
            .enumerate()
            .filter(|(_, app)| in_category(app))
            .map(|(index, _)| index)
            .collect();
    }

    let mut ranked: Vec<(MatchRank, usize)> = apps
        .iter()
        .enumerate()
        .filter(|(_, app)| in_category(app))
        .filter_map(|(index, app)| Some((match_rank(&app.name, &needle)?, index)))
        .collect();
    ranked.sort_by_key(|&(rank, index)| (rank, index));
    ranked.into_iter().map(|(_, index)| index).collect()
}

#[derive(Debug)]
pub struct Launcher {
    apps: Vec<DesktopApp>,
    page: usize,
    phase: Phase,
    started: Instant,
    from: f64,
    scroll: f64,
    page_offset: glide_ui::Tween,
    hover: glide_ui::HoverAnimation<usize>,
    hover_layers: LayerTweens,
    chip_tweens: Vec<glide_ui::Tween>,
    area: Rectangle<i32, Logical>,

    query: String,
    category: CategoryFilter,
    results: Vec<usize>,
    selected: Option<usize>,

    scrim: SolidColorBuffer,
    tiles: Vec<ScaledBuffers>,
    captions: Vec<CaptionBuffers>,
    cell_layer: ScaledBuffers,
    search_field: Option<(String, bool, ScaledBuffers)>,
    chips: ChipBuffers,
    chip_layout: Vec<(CategoryFilter, Rectangle<i32, Logical>)>,
    empty_label: Option<(String, ScaledBuffers)>,
    dots: Option<(usize, usize, ScaledBuffers)>,
    menu: Option<LauncherMenu>,
    menu_cache: std::collections::HashMap<LauncherMenuKey, MemoryRenderBuffer>,
    menu_shadow: std::collections::HashMap<(i32, i32, i32), MemoryRenderBuffer>,
    raster_rev: u64,
}

type LauncherMenuKey = (String, Vec<LauncherMenuItem>, Option<usize>, i32);

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
        let duration = menu_anim();
        if self.reveal.is_open() {
            self.reveal.progress(duration, ease_out)
        } else {
            self.reveal.progress(duration, ease_io)
        }
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
            scroll: 0.0,
            page_offset: glide_ui::Tween::at(0.0),
            hover: glide_ui::HoverAnimation::default(),
            hover_layers: LayerTweens::new(),
            chip_tweens: CategoryFilter::ORDER
                .iter()
                .map(|category| glide_ui::Tween::at(if category.is_all() { 1.0 } else { 0.0 }))
                .collect(),
            area,
            query: String::new(),
            category: CategoryFilter::All,
            selected: None,
            scrim: SolidColorBuffer::default(),
            tiles: vec![std::collections::HashMap::new(); count],
            captions: vec![CaptionBuffers::new(); count],
            cell_layer: std::collections::HashMap::new(),
            search_field: None,
            chips: ChipBuffers::new(),
            chip_layout: Vec::new(),
            empty_label: None,
            dots: None,
            menu: None,
            menu_cache: std::collections::HashMap::new(),
            menu_shadow: std::collections::HashMap::new(),
            raster_rev: 0,
        }
    }

    pub fn open_menu(
        &mut self,
        result: usize,
        area: Rectangle<i32, Logical>,
        pinned: bool,
    ) -> bool {
        if self.app_index(result).is_none() {
            return false;
        }
        let Some(slot) = self.page_range().position(|candidate| candidate == result) else {
            return false;
        };
        let Some(anchor) = self.icon_rects(area).into_iter().nth(slot) else {
            return false;
        };
        let items = vec![
            LauncherMenuItem::Open,
            if pinned {
                LauncherMenuItem::Unpin
            } else {
                LauncherMenuItem::Pin
            },
        ];
        let rect = crate::popup_menu::place_above(anchor, area, items.len());
        let duration = menu_anim();
        if let Some(menu) = self.menu.as_mut()
            && menu.result == result
            && menu.rect == rect
        {
            menu.items = items;
            menu.hovered = None;
            menu.reveal.open(duration);
            self.menu_cache.clear();
            return true;
        }
        let mut reveal = glide_ui::Reveal::closed();
        reveal.open(duration);
        self.menu = Some(LauncherMenu {
            result,
            items,
            rect,
            hovered: None,
            reveal,
        });
        self.menu_cache.clear();
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
    pub fn menu_is_open(&self) -> bool {
        self.open_menu_ref().is_some()
    }

    #[must_use]
    pub fn menu_is_visible(&self) -> bool {
        self.menu
            .as_ref()
            .is_some_and(|menu| menu.reveal.is_visible(menu_anim()))
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
        menu.hovered = hovered;
        true
    }

    #[must_use]
    pub fn query(&self) -> &str {
        &self.query
    }

    #[must_use]
    pub fn result_count(&self) -> usize {
        self.results.len()
    }

    pub fn push_query(&mut self, ch: char) -> bool {
        if ch.is_control() {
            return false;
        }
        self.query.push(ch);
        self.refilter();
        true
    }

    pub fn pop_query(&mut self) -> bool {
        if self.query.pop().is_none() {
            return false;
        }
        self.refilter();
        true
    }

    pub fn clear_query(&mut self) -> bool {
        if self.query.is_empty() && self.category.is_all() {
            return false;
        }
        self.query.clear();
        self.category = CategoryFilter::All;
        self.sync_chip_tweens();
        self.refilter();
        true
    }

    #[must_use]
    pub const fn category(&self) -> CategoryFilter {
        self.category
    }

    #[must_use]
    pub fn categories(&self) -> Vec<CategoryFilter> {
        CategoryFilter::ORDER
            .into_iter()
            .filter(|category| {
                category.is_all() || self.apps.iter().any(|app| category.accepts(app))
            })
            .collect()
    }

    pub fn toggle_category(&mut self, category: CategoryFilter) -> bool {
        let next = if self.category == category {
            CategoryFilter::All
        } else {
            category
        };
        if self.category == next {
            return false;
        }
        self.category = next;
        self.sync_chip_tweens();
        self.search_field = None;
        self.refilter();
        true
    }

    fn sync_chip_tweens(&mut self) {
        let active = self.category.ordinal();
        let duration = chip_anim();
        for (ordinal, tween) in self.chip_tweens.iter_mut().enumerate() {
            let target = if ordinal == active { 1.0 } else { 0.0 };
            tween.retarget(target, duration, ease_out);
        }
    }

    fn chip_progress(&self, category: CategoryFilter) -> f64 {
        self.chip_tweens
            .get(category.ordinal())
            .map_or(0.0, |tween| tween.value(chip_anim(), ease_out))
    }

    fn refilter(&mut self) {
        self.results = filter_apps(&self.apps, &self.query, self.category);
        self.page = 0;
        if self.page_is_sliding() {
            self.page_offset.retarget(0.0, page_anim(), ease_out);
        } else {
            self.page_offset.jump(0.0);
        }
        self.dots = None;
        self.scroll = 0.0;
        self.search_field = None;
        self.selected = (!self.query.is_empty() && !self.results.is_empty()).then_some(0);
        self.sync_hover_to_selection();
    }

    fn sync_hover_to_selection(&mut self) {
        self.hover.retarget_without_restarting(self.selected);
        self.sync_hover_layers();
    }

    fn sync_hover_layers(&mut self) {
        let target = self
            .hover
            .target()
            .and_then(|result| self.app_index(result));
        let duration = layer_anim();
        for (&index, tween) in &mut self.hover_layers {
            if Some(index) != target {
                tween.retarget(0.0, duration, ease_io);
            }
        }
        if let Some(index) = target {
            self.hover_layers
                .entry(index)
                .or_insert_with(|| glide_ui::Tween::at(0.0))
                .retarget(1.0, duration, ease_io);
        }
    }

    fn hover_layer_alpha(&self, index: usize) -> f64 {
        self.hover_layers
            .get(&index)
            .map_or(0.0, |tween| tween.value(layer_anim(), ease_io))
    }

    fn slide_to(&mut self, page: usize) {
        self.page = page;
        self.page_offset
            .retarget(page_as_f64(page), page_anim(), ease_out);
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

    fn invalidate_stale_buffers(&mut self, theme: &LauncherTheme) {
        let current = glide_theme::Revision::new()
            .u64(theme.revision())
            .u64(glide_paint::painter_generation())
            .finish();
        if current == self.raster_rev {
            return;
        }
        self.raster_rev = current;
        self.dots = None;
        self.menu_cache.clear();
        self.menu_shadow.clear();
        self.cell_layer.clear();
        self.search_field = None;
        self.chips.clear();
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
                let mut cache = ScaledBuffers::new();
                if let Some(pixels) = pixels.as_ref() {
                    cache.insert(
                        scale,
                        MemoryRenderBuffer::from_slice(
                            pixels,
                            smithay::backend::allocator::Fourcc::Argb8888,
                            (ICON_SIZE * scale, ICON_SIZE * scale),
                            scale,
                            Transform::Normal,
                            None,
                        ),
                    );
                }
                cache
            })
            .collect();
        self.captions = vec![CaptionBuffers::new(); apps.len()];
        self.apps = apps;
        self.hover_layers.clear();
        self.chips.clear();
        self.refilter();
    }

    #[must_use]
    pub fn page_count(&self) -> usize {
        self.results.len().div_ceil(self.grid().per_page()).max(1)
    }

    fn elapsed_fraction(&self) -> f64 {
        let duration = open_duration();
        if duration.is_zero() {
            return 1.0;
        }
        let elapsed = self.started.elapsed().as_secs_f64() / duration.as_secs_f64();
        elapsed.clamp(0.0, 1.0)
    }

    fn progress(&self) -> f64 {
        let eased = glide_theme::motion::E_OUT.ease_f64(self.elapsed_fraction());
        match self.phase {
            Phase::Open => 1.0,
            Phase::Opening => self.from + (1.0 - self.from) * eased,
            Phase::Closing => self.from * (1.0 - eased),
        }
    }

    #[must_use]
    pub fn openness(&self) -> f64 {
        self.progress()
    }

    pub fn begin_close(&mut self) {
        if self.phase != Phase::Closing {
            self.from = self.progress();
            self.phase = Phase::Closing;
            self.started = Instant::now();
        }
    }

    pub fn reopen(&mut self) {
        if self.phase == Phase::Closing {
            self.from = self.progress();
            self.phase = Phase::Opening;
            self.started = Instant::now();
        }
    }

    #[must_use]
    pub fn is_closing(&self) -> bool {
        self.phase == Phase::Closing
    }

    pub fn tick(&mut self) -> bool {
        self.settle();
        let settled = self.started.elapsed() >= open_duration();
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
        self.hover_layers
            .retain(|_, tween| tween.target() > 0.0 || tween.is_animating(layer_duration));
        let chip_duration = chip_anim();
        let chips_settled = !self
            .chip_tweens
            .iter()
            .any(|tween| tween.is_animating(chip_duration));
        if chips_settled {
            self.chips
                .retain(|&(_, step, _), _| step == 0 || step == CHIP_STEPS);
        }
    }

    #[must_use]
    pub fn is_animating(&self) -> bool {
        let scene = self.phase != Phase::Open;
        let lift = self.hover.is_animating(hover_anim());
        let page = self.page_is_sliding();
        let menu = self
            .menu
            .as_ref()
            .is_some_and(|menu| menu.reveal.is_animating(menu_anim()));
        let layer_duration = layer_anim();
        let layers = self
            .hover_layers
            .values()
            .any(|tween| tween.is_animating(layer_duration));
        let chip_duration = chip_anim();
        let chips = self
            .chip_tweens
            .iter()
            .any(|tween| tween.is_animating(chip_duration));
        scene || lift || page || menu || layers || chips
    }

    pub fn scroll_by(&mut self, amount: f64) -> bool {
        const DETENT: f64 = 15.0;
        if self.scroll != 0.0 && self.scroll.signum() != amount.signum() {
            self.scroll = 0.0;
        }
        self.scroll += amount;
        let mut turned = false;
        while self.scroll.abs() >= DETENT {
            let forward = self.scroll > 0.0;
            self.scroll -= DETENT * self.scroll.signum();
            if self.turn_page(forward) {
                turned = true;
            } else {
                self.scroll = 0.0;
                break;
            }
        }
        turned
    }

    pub fn turn_page(&mut self, forward: bool) -> bool {
        let last = self.page_count() - 1;
        let next = if forward {
            (self.page + 1).min(last)
        } else {
            self.page.saturating_sub(1)
        };
        if next == self.page {
            return false;
        }
        self.slide_to(next);
        self.selected = None;
        self.hover.retarget_without_restarting(None);
        self.sync_hover_layers();
        true
    }

    fn page_passes(&self) -> Vec<(std::ops::Range<usize>, f64)> {
        let position = self.page_position();
        let mut passes = Vec::new();
        for candidate in [position.floor(), position.ceil()] {
            if candidate < 0.0 || passes.iter().any(|&(_, seen)| seen == candidate) {
                continue;
            }
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let page = candidate as usize;
            if page < self.page_count() {
                passes.push((self.page_range_of(page), candidate));
            }
        }
        passes
            .into_iter()
            .map(|(range, page)| (range, page - position))
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
        if let Some(selected) = self.selected {
            self.page = selected / self.grid().per_page();
        }
        self.page_offset.jump(page_as_f64(self.page));
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

        let mut x = strip.loc.x;
        for page in 0..pages {
            let width = if page == self.page {
                DOT_ACTIVE_WIDTH
            } else {
                DOT_SIZE
            };
            let hit = Rectangle::new(
                Point::from((x - DOT_GAP / 2, strip.loc.y)),
                Size::from((width + DOT_GAP, strip.size.h)),
            );
            if hit.to_f64().contains(pos) {
                return Some(page);
            }
            x += width + DOT_GAP;
        }
        None
    }

    #[must_use]
    #[allow(clippy::unused_self)]
    pub fn search_contains(&self, area: Rectangle<i32, Logical>, pos: Point<f64, Logical>) -> bool {
        search_rect(area).to_f64().contains(pos)
    }

    pub(crate) fn layout_chips(
        &mut self,
        painter: &crate::paint::Painter,
        area: Rectangle<i32, Logical>,
    ) -> Vec<(CategoryFilter, Rectangle<i32, Logical>)> {
        let rects = self.chip_rects(painter, area);
        self.chip_layout.clone_from(&rects);
        rects
    }

    fn chip_rects(
        &self,
        painter: &crate::paint::Painter,
        area: Rectangle<i32, Logical>,
    ) -> Vec<(CategoryFilter, Rectangle<i32, Logical>)> {
        let categories = self.categories();
        if categories.len() <= 1 {
            return Vec::new();
        }

        let widths: Vec<i32> = categories
            .iter()
            .map(|category| chip_width(painter, category.label()))
            .collect();
        let total: i32 =
            widths.iter().sum::<i32>() + (i32::try_from(widths.len()).unwrap_or(1) - 1) * CHIP_GAP;

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
        self.chip_layout
            .iter()
            .find(|(_, rect)| rect.to_f64().contains(pos))
            .map(|&(category, _)| category)
    }

    #[must_use]
    pub fn icon_rects(&self, area: Rectangle<i32, Logical>) -> Vec<Rectangle<i32, Logical>> {
        let grid = grid_for(area);
        self.page_range()
            .enumerate()
            .map(|(slot, _)| icon_rect_in(grid.cell(slot)))
            .collect()
    }

    #[must_use]
    pub fn app_at(&self, area: Rectangle<i32, Logical>, pos: Point<f64, Logical>) -> Option<usize> {
        let grid = grid_for(area);
        for (slot, result) in self.page_range().enumerate() {
            if grid.cell(slot).to_f64().contains(pos) {
                return Some(result);
            }
        }
        None
    }

    pub fn set_hover(&mut self, result: Option<usize>) -> bool {
        if result.is_some() {
            self.selected = result;
        }
        let changed = self.hover.set(result.or(self.selected));
        if changed {
            self.sync_hover_layers();
        }
        changed
    }
}

fn icon_rect_in(cell: Rectangle<i32, Logical>) -> Rectangle<i32, Logical> {
    Rectangle::new(
        Point::from((
            cell.loc.x + (CELL_WIDTH - ICON_SIZE) / 2,
            cell.loc.y + CELL_PAD_TOP,
        )),
        Size::from((ICON_SIZE, ICON_SIZE)),
    )
}

fn scale_about(
    rect: Rectangle<i32, Logical>,
    anchor: (f64, f64),
    rise: f64,
    factor: f64,
) -> Rectangle<i32, Logical> {
    #[allow(clippy::cast_possible_truncation)]
    let x = (anchor.0 + (f64::from(rect.loc.x) - anchor.0) * factor).round() as i32;
    #[allow(clippy::cast_possible_truncation)]
    let y = (anchor.1 + (f64::from(rect.loc.y) + rise - anchor.1) * factor).round() as i32;
    #[allow(clippy::cast_possible_truncation)]
    let w = (f64::from(rect.size.w) * factor).round().max(1.0) as i32;
    #[allow(clippy::cast_possible_truncation)]
    let h = (f64::from(rect.size.h) * factor).round().max(1.0) as i32;
    Rectangle::new(Point::from((x, y)), Size::from((w, h)))
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub fn render<R>(
    launcher: &mut Launcher,
    renderer: &mut R,
    icons: &mut IconLoader,
    painter: &crate::paint::Painter,
    theme: &LauncherTheme,
    area: Rectangle<i32, Logical>,
    scale: Scale<f64>,
    output_scale: i32,
) -> Vec<LauncherRenderElement<R>>
where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Send + Clone + 'static,
{
    let t = launcher.progress();
    if t <= 0.0 {
        return Vec::new();
    }

    launcher.set_area(area);
    launcher.settle();
    launcher.invalidate_stale_buffers(theme);

    #[allow(clippy::cast_possible_truncation)]
    let alpha = t.clamp(0.0, 1.0) as f32;
    #[allow(clippy::cast_possible_truncation)]
    let rise = ((1.0 - t) * OPEN_RISE).round() as i32;
    let open_scale = OPEN_SCALE + (1.0 - OPEN_SCALE) * t;
    let centre = (
        f64::from(area.loc.x) + f64::from(area.size.w) / 2.0,
        f64::from(area.loc.y) + f64::from(area.size.h) / 2.0,
    );
    let place = |rect: Rectangle<i32, Logical>| -> Rectangle<i32, Logical> {
        #[allow(clippy::cast_possible_truncation)]
        let x = (centre.0 + (f64::from(rect.loc.x) - centre.0) * open_scale).round() as i32;
        #[allow(clippy::cast_possible_truncation)]
        let y = (centre.1 + (f64::from(rect.loc.y) - centre.1) * open_scale).round() as i32;
        Rectangle::new(Point::from((x, y)), rect.size)
    };

    let mut elements = Vec::new();
    let hovered = launcher.hover.target();
    let hover_t = glide_theme::motion::E_OVER.ease_f64(launcher.hover.progress(hover_anim()));

    if t >= 1.0 {
        push_menu(
            launcher,
            renderer,
            painter,
            theme,
            scale,
            output_scale,
            &mut elements,
        );
    }

    let grid = grid_for(area);
    let grid_width = f64::from(grid.rect.size.w + CELL_WIDTH);
    let passes: Vec<(std::ops::Range<usize>, i32)> = launcher
        .page_passes()
        .into_iter()
        .map(|(range, offset)| {
            #[allow(clippy::cast_possible_truncation)]
            let shift = (offset * grid_width).round() as i32;
            (range, shift)
        })
        .collect();

    for (range, shift) in passes {
        for (slot, result) in range.enumerate() {
            let Some(index) = launcher.app_index(result) else {
                continue;
            };
            let unshifted = grid.cell(slot);
            let mut rect = place(icon_rect_in(unshifted));
            rect.loc.x += shift;
            let cell_hover_t = if hovered == Some(result) {
                hover_t
            } else {
                0.0
            };
            let layer_t = launcher.hover_layer_alpha(index);

            let mut cell = place(unshifted);
            cell.loc.x += shift;

            push_icon(
                launcher,
                renderer,
                icons,
                painter,
                theme,
                index,
                rect,
                rise,
                cell_hover_t,
                alpha,
                scale,
                output_scale,
                &mut elements,
            );
            push_caption(
                launcher,
                renderer,
                painter,
                theme,
                index,
                rect,
                rise,
                cell_hover_t,
                alpha,
                scale,
                output_scale,
                &mut elements,
            );
            push_cell_layer(
                launcher,
                renderer,
                theme,
                cell,
                rise,
                layer_t,
                alpha,
                scale,
                output_scale,
                &mut elements,
            );
        }
    }

    if launcher.results.is_empty() && !launcher.query.is_empty() {
        push_empty_state(
            launcher,
            renderer,
            painter,
            theme,
            area,
            rise,
            alpha,
            scale,
            output_scale,
            &mut elements,
        );
    }

    push_category_chips(
        launcher,
        renderer,
        painter,
        theme,
        area,
        rise,
        alpha,
        scale,
        output_scale,
        &mut elements,
    );

    push_search_field(
        launcher,
        renderer,
        painter,
        theme,
        area,
        rise,
        alpha,
        scale,
        output_scale,
        &mut elements,
    );

    push_dots(
        launcher,
        renderer,
        theme,
        area,
        rise,
        alpha,
        scale,
        output_scale,
        &mut elements,
    );

    let mut scrim_color = theme.background.0;
    scrim_color[3] *= alpha;
    launcher
        .scrim
        .update((area.size.w, area.size.h), scrim_color);
    elements.push(LauncherRenderElement::Solid(
        SolidColorRenderElement::from_buffer(
            &launcher.scrim,
            area.loc.to_f64().to_physical(scale).to_i32_round(),
            scale,
            1.0,
            Kind::Unspecified,
        ),
    ));

    elements
}

fn push_menu<R>(
    launcher: &mut Launcher,
    renderer: &mut R,
    painter: &crate::paint::Painter,
    theme: &LauncherTheme,
    scale: Scale<f64>,
    output_scale: i32,
    elements: &mut Vec<LauncherRenderElement<R>>,
) where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Send + Clone + 'static,
{
    let Some(menu) = launcher.menu.clone() else {
        return;
    };
    if !menu.reveal.is_visible(menu_anim()) {
        return;
    }
    let progress = menu.progress();
    #[allow(clippy::cast_possible_truncation)]
    let menu_alpha = progress.clamp(0.0, 1.0) as f32;
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

    let key = (
        title.clone(),
        menu.items.clone(),
        menu.hovered,
        output_scale,
    );
    if !launcher.menu_cache.contains_key(&key) {
        launcher.menu_cache.clear();
        let rows: Vec<_> = menu
            .items
            .iter()
            .map(|item| crate::popup_menu::MenuRow {
                label: item.label(),
                danger: false,
            })
            .collect();
        let buffer = crate::popup_menu::draw(
            painter,
            &theme.menu,
            &title,
            &rows,
            menu.hovered,
            output_scale,
        );
        if let Some(buffer) = buffer {
            launcher.menu_cache.insert(key.clone(), buffer);
        }
    }
    if let Some(buffer) = launcher.menu_cache.get(&key)
        && let Ok(element) = MemoryRenderBufferRenderElement::from_buffer(
            renderer,
            panel.loc.to_f64().to_physical(scale),
            buffer,
            Some(menu_alpha),
            None,
            Some(panel.size),
            Kind::Unspecified,
        )
    {
        elements.push(LauncherRenderElement::Buffer(element));
    }

    let shadow_key = (menu.rect.size.w, menu.rect.size.h, output_scale);
    if let std::collections::hash_map::Entry::Vacant(entry) = launcher.menu_shadow.entry(shadow_key)
    {
        let buffer =
            crate::popup_menu::draw_shadow(menu.rect.size, crate::popup_menu::RADIUS, output_scale);
        if let Some(buffer) = buffer {
            entry.insert(buffer);
        }
    }
    if let Some(buffer) = launcher.menu_shadow.get(&shadow_key) {
        let margin = crate::popup_menu::shadow_margin();
        let shadow = scale_about(
            Rectangle::new(
                Point::from((menu.rect.loc.x - margin, menu.rect.loc.y - margin)),
                Size::from((menu.rect.size.w + margin * 2, menu.rect.size.h + margin * 2)),
            ),
            anchor,
            rise,
            factor,
        );
        if let Ok(element) = MemoryRenderBufferRenderElement::from_buffer(
            renderer,
            shadow.loc.to_f64().to_physical(scale),
            buffer,
            Some(menu_alpha),
            None,
            Some(shadow.size),
            Kind::Unspecified,
        ) {
            elements.push(LauncherRenderElement::Buffer(element));
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn push_icon<R>(
    launcher: &mut Launcher,
    renderer: &mut R,
    icons: &mut IconLoader,
    painter: &crate::paint::Painter,
    theme: &LauncherTheme,
    index: usize,
    rect: Rectangle<i32, Logical>,
    offset: i32,
    hover_t: f64,
    alpha: f32,
    scale: Scale<f64>,
    output_scale: i32,
    elements: &mut Vec<LauncherRenderElement<R>>,
) where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Send + Clone + 'static,
{
    if !launcher.tiles[index].contains_key(&output_scale) {
        let pixels = icons
            .load_named(&launcher.apps[index].icon, ICON_SIZE * output_scale.max(1))
            .map(<[u8]>::to_vec);
        let buffer = match pixels {
            Some(pixels) => Some(MemoryRenderBuffer::from_slice(
                &pixels,
                smithay::backend::allocator::Fourcc::Argb8888,
                (ICON_SIZE * output_scale, ICON_SIZE * output_scale),
                output_scale,
                Transform::Normal,
                None,
            )),
            None => draw_fallback_icon(painter, theme, &launcher.apps[index].name, output_scale),
        };
        if let Some(buffer) = buffer {
            launcher.tiles[index].insert(output_scale, buffer);
        }
    }

    let Some(buffer) = launcher.tiles[index].get(&output_scale) else {
        return;
    };
    #[allow(clippy::cast_possible_truncation)]
    let lift = (HOVER_LIFT * hover_t).round() as i32;
    let pos = Point::from((rect.loc.x, rect.loc.y + offset - lift))
        .to_f64()
        .to_physical(scale);
    if let Ok(element) = MemoryRenderBufferRenderElement::from_buffer(
        renderer,
        pos,
        buffer,
        Some(alpha),
        None,
        None,
        Kind::Unspecified,
    ) {
        elements.push(LauncherRenderElement::Buffer(element));
    }
}

fn draw_fallback_icon(
    painter: &crate::paint::Painter,
    theme: &LauncherTheme,
    name: &str,
    scale: i32,
) -> Option<MemoryRenderBuffer> {
    let pixels = usize::try_from(ICON_SIZE * scale * ICON_SIZE * scale * 4).ok()?;
    let mut buf = vec![0u8; pixels];

    crate::paint::Painter::fill_panel_background(
        &mut buf,
        ICON_SIZE,
        ICON_SIZE,
        scale,
        theme.fallback_background,
        glide_theme::shape::R4,
        [true; 4],
    );
    #[allow(clippy::cast_precision_loss)]
    crate::paint::draw_inset_border(
        &mut buf,
        ICON_SIZE * scale,
        ICON_SIZE * scale,
        scale as f32,
        glide_theme::shape::R4 * scale as f32,
        theme.fallback_border,
    );
    painter.draw_label_over_styled(
        &mut buf,
        ICON_SIZE,
        ICON_SIZE,
        0,
        ICON_SIZE,
        scale,
        crate::paint::TextAlign::Center,
        &fallback_initial(name),
        theme.fallback_label.to_buffer_bytes(),
        &glide_theme::TextStyle::FALLBACK_INITIAL,
    );

    Some(MemoryRenderBuffer::from_slice(
        &buf,
        smithay::backend::allocator::Fourcc::Argb8888,
        (ICON_SIZE * scale, ICON_SIZE * scale),
        scale,
        Transform::Normal,
        None,
    ))
}

#[allow(clippy::too_many_arguments)]
fn push_caption<R>(
    launcher: &mut Launcher,
    renderer: &mut R,
    painter: &crate::paint::Painter,
    theme: &LauncherTheme,
    index: usize,
    rect: Rectangle<i32, Logical>,
    offset: i32,
    hover_t: f64,
    alpha: f32,
    scale: Scale<f64>,
    output_scale: i32,
    elements: &mut Vec<LauncherRenderElement<R>>,
) where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Send + Clone + 'static,
{
    for state in [CaptionState::Rest, CaptionState::Active] {
        let key = (output_scale, state);
        if launcher.captions[index].contains_key(&key) {
            continue;
        }
        if let Some(buffer) = draw_caption(
            painter,
            theme,
            &launcher.apps[index].name,
            state,
            output_scale,
        ) {
            launcher.captions[index].insert(key, buffer);
        }
    }

    let pos = Point::from((
        rect.loc.x + (ICON_SIZE - CELL_WIDTH) / 2,
        rect.loc.y + ICON_SIZE + CAPTION_GAP + offset,
    ))
    .to_f64()
    .to_physical(scale);

    #[allow(clippy::cast_possible_truncation)]
    let hover_alpha = hover_t.clamp(0.0, 1.0) as f32;
    let layers = [
        (CaptionState::Rest, alpha * (1.0 - hover_alpha)),
        (CaptionState::Active, alpha * hover_alpha),
    ];
    for (state, layer_alpha) in layers {
        if layer_alpha <= 0.0 {
            continue;
        }
        let Some(buffer) = launcher.captions[index].get(&(output_scale, state)) else {
            continue;
        };
        if let Ok(element) = MemoryRenderBufferRenderElement::from_buffer(
            renderer,
            pos,
            buffer,
            Some(layer_alpha),
            None,
            None,
            Kind::Unspecified,
        ) {
            elements.push(LauncherRenderElement::Buffer(element));
        }
    }
}

fn draw_caption(
    painter: &crate::paint::Painter,
    theme: &LauncherTheme,
    name: &str,
    state: CaptionState,
    scale: i32,
) -> Option<MemoryRenderBuffer> {
    let pixels = usize::try_from(CELL_WIDTH * scale * CAPTION_HEIGHT * scale * 4).ok()?;
    let mut buf = vec![0u8; pixels];

    let (x_min, x_max) = (CAPTION_PAD_X, CELL_WIDTH - CAPTION_PAD_X);
    #[allow(clippy::cast_precision_loss)]
    let (first, second) = wrap_caption(painter, name, (x_max - x_min) as f32);
    let colour = match state {
        CaptionState::Rest => theme.label,
        CaptionState::Active => theme.label_active,
    }
    .to_buffer_bytes();

    let bands: &[(i32, i32, &str)] = match &second {
        Some(second) => &[
            (0, CAPTION_LINE, first.as_str()),
            (CAPTION_LINE, CAPTION_HEIGHT, second.as_str()),
        ],
        None => &[(0, CAPTION_LINE, first.as_str())],
    };
    for &(y_min, y_max, text) in bands {
        painter.draw_label_over_styled_in_band(
            &mut buf,
            CELL_WIDTH,
            CAPTION_HEIGHT,
            x_min,
            x_max,
            y_min,
            y_max,
            scale,
            crate::paint::TextAlign::Center,
            text,
            colour,
            &glide_theme::TextStyle::LAUNCHER_CAPTION,
        );
    }

    Some(MemoryRenderBuffer::from_slice(
        &buf,
        smithay::backend::allocator::Fourcc::Argb8888,
        (CELL_WIDTH * scale, CAPTION_HEIGHT * scale),
        scale,
        Transform::Normal,
        None,
    ))
}

#[allow(clippy::too_many_arguments)]
fn push_cell_layer<R>(
    launcher: &mut Launcher,
    renderer: &mut R,
    theme: &LauncherTheme,
    cell: Rectangle<i32, Logical>,
    offset: i32,
    hover_t: f64,
    alpha: f32,
    scale: Scale<f64>,
    output_scale: i32,
    elements: &mut Vec<LauncherRenderElement<R>>,
) where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Send + Clone + 'static,
{
    if hover_t <= 0.0 {
        return;
    }

    if let std::collections::hash_map::Entry::Vacant(entry) =
        launcher.cell_layer.entry(output_scale)
        && let Ok(pixels) =
            usize::try_from(CELL_WIDTH * output_scale * CELL_HEIGHT * output_scale * 4)
    {
        let mut buf = vec![0u8; pixels];
        crate::paint::Painter::fill_panel_background(
            &mut buf,
            CELL_WIDTH,
            CELL_HEIGHT,
            output_scale,
            theme.cell_layer,
            CELL_RADIUS,
            [true; 4],
        );
        #[allow(clippy::cast_precision_loss)]
        crate::paint::draw_inset_border(
            &mut buf,
            CELL_WIDTH * output_scale,
            CELL_HEIGHT * output_scale,
            output_scale as f32,
            CELL_RADIUS * output_scale as f32,
            theme.cell_border,
        );
        entry.insert(MemoryRenderBuffer::from_slice(
            &buf,
            smithay::backend::allocator::Fourcc::Argb8888,
            (CELL_WIDTH * output_scale, CELL_HEIGHT * output_scale),
            output_scale,
            Transform::Normal,
            None,
        ));
    }

    let Some(buffer) = launcher.cell_layer.get(&output_scale) else {
        return;
    };
    #[allow(clippy::cast_possible_truncation)]
    let layer_alpha = (alpha * hover_t as f32).clamp(0.0, 1.0);
    let pos = Point::from((cell.loc.x, cell.loc.y + offset))
        .to_f64()
        .to_physical(scale);
    if let Ok(element) = MemoryRenderBufferRenderElement::from_buffer(
        renderer,
        pos,
        buffer,
        Some(layer_alpha),
        None,
        None,
        Kind::Unspecified,
    ) {
        elements.push(LauncherRenderElement::Buffer(element));
    }
}

#[allow(clippy::too_many_arguments)]
fn push_category_chips<R>(
    launcher: &mut Launcher,
    renderer: &mut R,
    painter: &crate::paint::Painter,
    theme: &LauncherTheme,
    area: Rectangle<i32, Logical>,
    offset: i32,
    alpha: f32,
    scale: Scale<f64>,
    output_scale: i32,
    elements: &mut Vec<LauncherRenderElement<R>>,
) where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Send + Clone + 'static,
{
    let chips = launcher.layout_chips(painter, area);
    if chips.is_empty() {
        return;
    }

    for (slot, (category, rect)) in chips.into_iter().enumerate() {
        let step = chip_step(launcher.chip_progress(category));
        let key = (slot, step, output_scale);
        if !launcher.chips.contains_key(&key)
            && let Some(buffer) = draw_chip(
                painter,
                theme,
                category.label(),
                rect.size.w,
                chip_step_progress(step),
                output_scale,
            )
        {
            launcher.chips.insert(key, buffer);
        }

        let Some(buffer) = launcher.chips.get(&key) else {
            continue;
        };
        let pos = Point::from((rect.loc.x, rect.loc.y + offset))
            .to_f64()
            .to_physical(scale);
        if let Ok(element) = MemoryRenderBufferRenderElement::from_buffer(
            renderer,
            pos,
            buffer,
            Some(alpha),
            None,
            None,
            Kind::Unspecified,
        ) {
            elements.push(LauncherRenderElement::Buffer(element));
        }
    }
}

fn draw_chip(
    painter: &crate::paint::Painter,
    theme: &LauncherTheme,
    label: &str,
    width: i32,
    active_t: f32,
    scale: i32,
) -> Option<MemoryRenderBuffer> {
    let pixels = usize::try_from(width * scale * CHIP_HEIGHT * scale * 4).ok()?;
    let mut buf = vec![0u8; pixels];

    let radius = pill_radius(CHIP_HEIGHT);
    let background = theme.chip_background.lerp(theme.chip_active, active_t);
    let border = theme.chip_border.lerp(theme.chip_active, active_t);
    let text = theme.chip_label.lerp(theme.chip_active_label, active_t);

    crate::paint::Painter::fill_panel_background(
        &mut buf,
        width,
        CHIP_HEIGHT,
        scale,
        background,
        radius,
        [true; 4],
    );
    #[allow(clippy::cast_precision_loss)]
    crate::paint::draw_inset_border(
        &mut buf,
        width * scale,
        CHIP_HEIGHT * scale,
        scale as f32,
        radius * scale as f32,
        border,
    );
    painter.draw_label_over_styled(
        &mut buf,
        width,
        CHIP_HEIGHT,
        CHIP_PAD_X,
        width - CHIP_PAD_X,
        scale,
        crate::paint::TextAlign::Center,
        label,
        text.to_buffer_bytes(),
        &glide_theme::TextStyle::LAUNCHER_CHIP,
    );

    Some(MemoryRenderBuffer::from_slice(
        &buf,
        smithay::backend::allocator::Fourcc::Argb8888,
        (width * scale, CHIP_HEIGHT * scale),
        scale,
        Transform::Normal,
        None,
    ))
}

#[allow(clippy::too_many_arguments)]
fn push_search_field<R>(
    launcher: &mut Launcher,
    renderer: &mut R,
    painter: &crate::paint::Painter,
    theme: &LauncherTheme,
    area: Rectangle<i32, Logical>,
    offset: i32,
    alpha: f32,
    scale: Scale<f64>,
    output_scale: i32,
    elements: &mut Vec<LauncherRenderElement<R>>,
) where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Send + Clone + 'static,
{
    let rect = search_rect(area);
    let shown: std::borrow::Cow<'_, str> = if launcher.category.is_all() {
        std::borrow::Cow::Borrowed(&launcher.query)
    } else {
        std::borrow::Cow::Owned(format!("{}: {}", launcher.category.label(), launcher.query))
    };
    let typing = !launcher.query.is_empty() || !launcher.category.is_all();
    let outdated = launcher
        .search_field
        .as_ref()
        .is_none_or(|(cached, cached_typing, _)| *cached != *shown || *cached_typing != typing);
    if outdated {
        launcher.search_field = Some((shown.to_string(), typing, ScaledBuffers::new()));
    }

    let needs = launcher
        .search_field
        .as_ref()
        .is_none_or(|(_, _, cache)| !cache.contains_key(&output_scale));
    if needs
        && let Some(buffer) = draw_search_field(painter, theme, &shown, rect.size.w, output_scale)
        && let Some((_, _, cache)) = launcher.search_field.as_mut()
    {
        cache.insert(output_scale, buffer);
    }

    let Some(buffer) = launcher
        .search_field
        .as_ref()
        .and_then(|(_, _, cache)| cache.get(&output_scale))
    else {
        return;
    };
    let pos = Point::from((rect.loc.x, rect.loc.y + offset))
        .to_f64()
        .to_physical(scale);
    if let Ok(element) = MemoryRenderBufferRenderElement::from_buffer(
        renderer,
        pos,
        buffer,
        Some(alpha),
        None,
        None,
        Kind::Unspecified,
    ) {
        elements.push(LauncherRenderElement::Buffer(element));
    }
}

fn draw_search_field(
    painter: &crate::paint::Painter,
    theme: &LauncherTheme,
    query: &str,
    width: i32,
    scale: i32,
) -> Option<MemoryRenderBuffer> {
    let pixels = usize::try_from(width * scale * SEARCH_HEIGHT * scale * 4).ok()?;
    let mut buf = vec![0u8; pixels];

    let radius = pill_radius(SEARCH_HEIGHT);
    crate::paint::Painter::fill_panel_background(
        &mut buf,
        width,
        SEARCH_HEIGHT,
        scale,
        theme.search_background,
        radius,
        [true; 4],
    );
    #[allow(clippy::cast_precision_loss)]
    crate::paint::draw_inset_border(
        &mut buf,
        width * scale,
        SEARCH_HEIGHT * scale,
        scale as f32,
        radius * scale as f32,
        theme.search_border,
    );

    #[allow(clippy::cast_precision_loss)]
    let glyph_centre_x = SEARCH_PAD_X as f32 + SEARCH_GLYPH / 2.0;
    #[allow(clippy::cast_precision_loss)]
    crate::paint::draw_search_glyph_over(
        &mut buf,
        width,
        SEARCH_HEIGHT,
        glyph_centre_x,
        SEARCH_HEIGHT as f32 / 2.0,
        SEARCH_GLYPH,
        SEARCH_GLYPH_STROKE,
        scale,
        theme.search_placeholder,
    );

    #[allow(clippy::cast_possible_truncation)]
    let text_left = SEARCH_PAD_X + SEARCH_GLYPH as i32 + glide_theme::spacing::SP3;
    let (text, colour) = if query.is_empty() {
        (
            crate::i18n::t(crate::i18n::Key::Search),
            theme.search_placeholder,
        )
    } else {
        (query, theme.search_text)
    };
    painter.draw_label_over_styled(
        &mut buf,
        width,
        SEARCH_HEIGHT,
        text_left,
        width - SEARCH_PAD_X,
        scale,
        crate::paint::TextAlign::Left,
        text,
        colour.to_buffer_bytes(),
        &glide_theme::TextStyle::MENU_ITEM,
    );

    Some(MemoryRenderBuffer::from_slice(
        &buf,
        smithay::backend::allocator::Fourcc::Argb8888,
        (width * scale, SEARCH_HEIGHT * scale),
        scale,
        Transform::Normal,
        None,
    ))
}

#[allow(clippy::too_many_arguments)]
fn push_empty_state<R>(
    launcher: &mut Launcher,
    renderer: &mut R,
    painter: &crate::paint::Painter,
    theme: &LauncherTheme,
    area: Rectangle<i32, Logical>,
    offset: i32,
    alpha: f32,
    scale: Scale<f64>,
    output_scale: i32,
    elements: &mut Vec<LauncherRenderElement<R>>,
) where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Send + Clone + 'static,
{
    let message = format!("No results for \u{201c}{}\u{201d}", launcher.query);
    let outdated = launcher
        .empty_label
        .as_ref()
        .is_none_or(|(cached, _)| cached != &message);
    if outdated {
        launcher.empty_label = Some((message.clone(), ScaledBuffers::new()));
    }

    let grid = grid_for(area);
    let width = grid.rect.size.w.max(CELL_WIDTH);

    let needs = launcher
        .empty_label
        .as_ref()
        .is_none_or(|(_, cache)| !cache.contains_key(&output_scale));
    if needs
        && let Ok(pixels) = usize::try_from(width * output_scale * EMPTY_HEIGHT * output_scale * 4)
    {
        let mut buf = vec![0u8; pixels];
        painter.draw_label_over_styled(
            &mut buf,
            width,
            EMPTY_HEIGHT,
            0,
            width,
            output_scale,
            crate::paint::TextAlign::Center,
            &message,
            theme.search_placeholder.to_buffer_bytes(),
            &glide_theme::TextStyle::CAP,
        );
        let buffer = MemoryRenderBuffer::from_slice(
            &buf,
            smithay::backend::allocator::Fourcc::Argb8888,
            (width * output_scale, EMPTY_HEIGHT * output_scale),
            output_scale,
            Transform::Normal,
            None,
        );
        if let Some((_, cache)) = launcher.empty_label.as_mut() {
            cache.insert(output_scale, buffer);
        }
    }

    let Some(buffer) = launcher
        .empty_label
        .as_ref()
        .and_then(|(_, cache)| cache.get(&output_scale))
    else {
        return;
    };
    let pos = Point::from((
        grid.rect.loc.x,
        grid.rect.loc.y + (grid.rect.size.h - EMPTY_HEIGHT) / 2 + offset,
    ))
    .to_f64()
    .to_physical(scale);
    if let Ok(element) = MemoryRenderBufferRenderElement::from_buffer(
        renderer,
        pos,
        buffer,
        Some(alpha),
        None,
        None,
        Kind::Unspecified,
    ) {
        elements.push(LauncherRenderElement::Buffer(element));
    }
}

#[allow(clippy::too_many_arguments)]
fn push_dots<R>(
    launcher: &mut Launcher,
    renderer: &mut R,
    theme: &LauncherTheme,
    area: Rectangle<i32, Logical>,
    offset: i32,
    alpha: f32,
    scale: Scale<f64>,
    output_scale: i32,
    elements: &mut Vec<LauncherRenderElement<R>>,
) where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Send + Clone + 'static,
{
    let pages = launcher.page_count();
    if pages <= 1 {
        return;
    }

    let width = dots_width(pages);
    let content_changed = launcher
        .dots
        .as_ref()
        .is_none_or(|(cached_pages, cached_page, _)| {
            *cached_pages != pages || *cached_page != launcher.page
        });
    if content_changed {
        launcher.dots = Some((pages, launcher.page, ScaledBuffers::new()));
    }

    let needs = launcher
        .dots
        .as_ref()
        .is_none_or(|(_, _, cache)| !cache.contains_key(&output_scale));

    if needs && let Ok(pixels) = usize::try_from(width * output_scale * DOT_SIZE * output_scale * 4)
    {
        let mut buf = vec![0u8; pixels];
        let mut x = 0;
        for page in 0..pages {
            let active = page == launcher.page;
            let dot_width = if active { DOT_ACTIVE_WIDTH } else { DOT_SIZE };
            let color = if active { theme.active_dot } else { theme.dot };
            #[allow(clippy::cast_precision_loss)]
            crate::paint::Painter::fill_rounded_rect_over(
                &mut buf,
                width,
                DOT_SIZE,
                x,
                0,
                dot_width,
                DOT_SIZE,
                output_scale,
                color,
                DOT_SIZE as f32 / 2.0,
            );
            x += dot_width + DOT_GAP;
        }
        let buffer = MemoryRenderBuffer::from_slice(
            &buf,
            smithay::backend::allocator::Fourcc::Argb8888,
            (width * output_scale, DOT_SIZE * output_scale),
            output_scale,
            Transform::Normal,
            None,
        );
        if let Some((_, _, cache)) = launcher.dots.as_mut() {
            cache.insert(output_scale, buffer);
        }
    }

    let Some(buffer) = launcher
        .dots
        .as_ref()
        .and_then(|(_, _, cache)| cache.get(&output_scale))
    else {
        return;
    };
    let pos = Point::from((
        area.loc.x + (area.size.w - width) / 2,
        area.loc.y + area.size.h - DOT_MARGIN - DOT_SIZE + offset,
    ))
    .to_f64()
    .to_physical(scale);
    if let Ok(element) = MemoryRenderBufferRenderElement::from_buffer(
        renderer,
        pos,
        buffer,
        Some(alpha),
        None,
        None,
        Kind::Unspecified,
    ) {
        elements.push(LauncherRenderElement::Buffer(element));
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LauncherTheme {
    pub background: Color,
    pub label: Color,
    pub label_active: Color,
    pub cell_layer: Color,
    pub cell_border: Color,
    pub fallback_background: Color,
    pub fallback_border: Color,
    pub fallback_label: Color,
    pub search_background: Color,
    pub search_border: Color,
    pub search_text: Color,
    pub search_placeholder: Color,
    pub chip_background: Color,
    pub chip_border: Color,
    pub chip_label: Color,
    pub chip_active: Color,
    pub chip_active_label: Color,
    pub dot: Color,
    pub active_dot: Color,
    pub menu: crate::popup_menu::MenuColors,
}

impl LauncherTheme {
    #[must_use]
    pub fn from_dock(dock: &glide_theme::DockTheme) -> Self {
        let defaults = Self::default();
        Self {
            background: dock.background,
            active_dot: dock.focused_color,
            menu: crate::popup_menu::MenuColors::from_dock(dock),
            ..defaults
        }
    }

    #[must_use]
    pub fn revision(&self) -> u64 {
        glide_theme::Revision::new()
            .color(self.background)
            .color(self.label)
            .color(self.label_active)
            .color(self.cell_layer)
            .color(self.cell_border)
            .color(self.fallback_background)
            .color(self.fallback_border)
            .color(self.fallback_label)
            .color(self.search_background)
            .color(self.search_border)
            .color(self.search_text)
            .color(self.search_placeholder)
            .color(self.chip_background)
            .color(self.chip_border)
            .color(self.chip_label)
            .color(self.chip_active)
            .color(self.chip_active_label)
            .color(self.dot)
            .color(self.active_dot)
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
        let tokens =
            glide_theme::Tokens::new(glide_theme::Appearance::Dark, glide_theme::Accent::Blue);
        let shell_fg = tokens.shell_fg;
        let tint = |alpha: f32| Color([shell_fg.0[0], shell_fg.0[1], shell_fg.0[2], alpha]);
        Self {
            background: tokens.shell_bg,
            label: tokens.fg_2,
            label_active: tokens.fg_1,
            cell_layer: tint(glide_theme::motion::ST_HOVER),
            cell_border: tint(0.10),
            fallback_background: tint(0.12),
            fallback_border: tint(0.10),
            fallback_label: tokens.fg_2,
            search_background: tint(0.10),
            search_border: tokens.shell_line,
            search_text: tokens.fg_1,
            search_placeholder: tokens.fg_3,
            chip_background: tint(0.08),
            chip_border: tint(0.10),
            chip_label: tokens.fg_2,
            chip_active: tokens.acc_bg,
            chip_active_label: tokens.acc_fg,
            dot: tokens.fg_4,
            active_dot: tokens.acc_bg,
            menu: crate::popup_menu::MenuColors::from_dock(&glide_theme::DockTheme::default()),
        }
    }
}

smithay::render_elements! {
    pub LauncherRenderElement<R> where R: ImportAll + ImportMem;
    Buffer = MemoryRenderBufferRenderElement<R>,
    Solid = SolidColorRenderElement,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icon::AppCategory;

    fn apps(count: usize) -> Vec<DesktopApp> {
        (0..count)
            .map(|i| DesktopApp {
                id: format!("app-{i}"),
                name: format!("App {i}"),
                command: "true".to_string(),
                icon: "application-x-executable".to_string(),
                category: AppCategory::Other,
            })
            .collect()
    }

    fn named(names: &[&str]) -> Vec<DesktopApp> {
        names
            .iter()
            .map(|name| DesktopApp {
                id: name.to_lowercase(),
                name: (*name).to_string(),
                command: "true".to_string(),
                icon: "application-x-executable".to_string(),
                category: AppCategory::Other,
            })
            .collect()
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
                named(&[
                    "Files",
                    "Firefox",
                    "Text Editor",
                    "Chromium",
                    "Fire Emblem",
                    "Disk Usage Analyzer",
                ]),
                area(),
            )
        }

        fn result_names(launcher: &Launcher) -> Vec<&str> {
            launcher
                .results
                .iter()
                .map(|&i| launcher.apps[i].name.as_str())
                .collect()
        }

        #[test]
        fn an_empty_query_keeps_every_app_in_its_original_order() {
            let launcher = browsers();
            assert_eq!(launcher.result_count(), 6);
            assert_eq!(result_names(&launcher)[0], "Files");
        }

        #[test]
        fn typing_narrows_the_grid() {
            let mut launcher = browsers();
            for ch in "fire".chars() {
                assert!(launcher.push_query(ch));
            }
            assert_eq!(launcher.query(), "fire");
            assert_eq!(result_names(&launcher), vec!["Firefox", "Fire Emblem"]);
        }

        #[test]
        fn a_name_prefix_outranks_a_word_prefix_which_outranks_a_substring() {
            let mut launcher = Launcher::new(
                named(&["Disk Usage Analyzer", "Use Me", "Usage Meter"]),
                area(),
            );
            for ch in "us".chars() {
                launcher.push_query(ch);
            }
            assert_eq!(
                result_names(&launcher),
                vec!["Use Me", "Usage Meter", "Disk Usage Analyzer"],
                "prefix, then word-prefix, then a plain substring"
            );
        }

        #[test]
        fn matching_ignores_case() {
            let mut launcher = browsers();
            for ch in "FIREFOX".chars() {
                launcher.push_query(ch);
            }
            assert_eq!(result_names(&launcher), vec!["Firefox"]);
        }

        #[test]
        fn searching_preselects_the_best_match_so_enter_launches_it() {
            let mut launcher = browsers();
            launcher.push_query('f');
            assert_eq!(launcher.selected(), Some(0));
            assert_eq!(
                launcher
                    .app_index(0)
                    .map(|i| launcher.apps[i].name.as_str()),
                Some("Files")
            );
        }

        #[test]
        fn nothing_is_preselected_without_a_query() {
            let launcher = browsers();
            assert_eq!(launcher.selected(), None);
        }

        #[test]
        fn a_query_with_no_matches_selects_nothing() {
            let mut launcher = browsers();
            for ch in "zzz".chars() {
                launcher.push_query(ch);
            }
            assert_eq!(launcher.result_count(), 0);
            assert_eq!(launcher.selected(), None);
        }

        #[test]
        fn backspace_walks_the_query_back_and_restores_the_full_list() {
            let mut launcher = browsers();
            for ch in "fire".chars() {
                launcher.push_query(ch);
            }
            for _ in 0..4 {
                assert!(launcher.pop_query());
            }
            assert!(!launcher.pop_query(), "nothing left to delete");
            assert_eq!(launcher.result_count(), 6);
        }

        #[test]
        fn clearing_reports_whether_there_was_anything_to_clear() {
            let mut launcher = browsers();
            assert!(
                !launcher.clear_query(),
                "an empty query must fall through so Escape can close the grid"
            );
            launcher.push_query('f');
            assert!(launcher.clear_query());
            assert_eq!(launcher.result_count(), 6);
        }

        #[test]
        fn control_characters_never_enter_the_query() {
            let mut launcher = browsers();
            assert!(!launcher.push_query('\n'));
            assert!(!launcher.push_query('\t'));
            assert_eq!(launcher.query(), "");
        }

        #[test]
        fn a_query_resets_paging_so_results_are_never_hidden_on_page_two() {
            let mut launcher = launcher(per_page() + 5);
            assert!(launcher.turn_page(true));
            assert_eq!(launcher.page, 1);

            launcher.push_query('a');
            assert_eq!(launcher.page, 0);
        }

        #[test]
        fn results_index_the_app_list_not_the_grid_slot() {
            let mut launcher = browsers();
            for ch in "chrom".chars() {
                launcher.push_query(ch);
            }
            assert_eq!(launcher.result_count(), 1);
            assert_eq!(
                launcher.app_index(0),
                Some(3),
                "Chromium is the fourth app; launching slot 0 must run it, not Files"
            );
        }
    }

    mod categories {
        use super::*;

        fn mixed() -> Launcher {
            let apps = vec![
                DesktopApp {
                    id: "firefox".into(),
                    name: "Firefox".into(),
                    command: "true".into(),
                    icon: String::new(),
                    category: AppCategory::Internet,
                },
                DesktopApp {
                    id: "fire-editor".into(),
                    name: "Fire Editor".into(),
                    command: "true".into(),
                    icon: String::new(),
                    category: AppCategory::Office,
                },
                DesktopApp {
                    id: "chromium".into(),
                    name: "Chromium".into(),
                    command: "true".into(),
                    icon: String::new(),
                    category: AppCategory::Internet,
                },
            ];
            Launcher::new(apps, area())
        }

        fn names(launcher: &Launcher) -> Vec<&str> {
            launcher
                .results
                .iter()
                .map(|&i| launcher.apps[i].name.as_str())
                .collect()
        }

        #[test]
        fn only_categories_with_apps_get_a_chip() {
            let chips = mixed().categories();
            assert!(
                chips.contains(&CategoryFilter::All),
                "All is always offered"
            );
            assert!(chips.contains(&CategoryFilter::Only(AppCategory::Internet)));
            assert!(chips.contains(&CategoryFilter::Only(AppCategory::Office)));
            assert!(
                !chips.contains(&CategoryFilter::Only(AppCategory::Media)),
                "an empty category must not get a chip that filters to nothing"
            );
        }

        #[test]
        fn selecting_a_chip_narrows_the_grid() {
            let mut launcher = mixed();
            assert!(launcher.toggle_category(CategoryFilter::Only(AppCategory::Internet)));
            assert_eq!(names(&launcher), vec!["Firefox", "Chromium"]);
        }

        #[test]
        fn clicking_the_active_chip_again_clears_it() {
            let mut launcher = mixed();
            launcher.toggle_category(CategoryFilter::Only(AppCategory::Internet));
            assert!(launcher.toggle_category(CategoryFilter::Only(AppCategory::Internet)));
            assert_eq!(launcher.category(), CategoryFilter::All);
            assert_eq!(launcher.result_count(), 3);
        }

        #[test]
        fn selecting_all_while_already_unfiltered_changes_nothing() {
            let mut launcher = mixed();
            assert!(!launcher.toggle_category(CategoryFilter::All));
        }

        #[test]
        fn a_category_and_a_query_apply_together() {
            let mut launcher = mixed();
            launcher.toggle_category(CategoryFilter::Only(AppCategory::Internet));
            for ch in "fire".chars() {
                launcher.push_query(ch);
            }
            assert_eq!(
                names(&launcher),
                vec!["Firefox"],
                "Fire Editor matches the text but is filed under Office"
            );
        }

        #[test]
        fn escape_clears_the_category_as_well_as_the_text() {
            let mut launcher = mixed();
            launcher.toggle_category(CategoryFilter::Only(AppCategory::Office));
            assert!(launcher.clear_query());
            assert_eq!(launcher.category(), CategoryFilter::All);
            assert!(
                !launcher.clear_query(),
                "with nothing filtered Escape must fall through to closing"
            );
        }

        #[test]
        fn a_chip_click_lands_on_the_chip_under_the_pointer() {
            let mut launcher = mixed();
            let painter = crate::paint::Painter::new(14.0, None);
            let chips = launcher.layout_chips(&painter, area());
            assert!(chips.len() >= 2);

            for (category, rect) in chips {
                let centre = Point::from((
                    f64::from(rect.loc.x + rect.size.w / 2),
                    f64::from(rect.loc.y + rect.size.h / 2),
                ));
                assert_eq!(launcher.category_at(centre), Some(category));
            }
        }

        #[test]
        fn a_point_away_from_the_row_hits_no_chip() {
            let mut launcher = mixed();
            let painter = crate::paint::Painter::new(14.0, None);
            launcher.layout_chips(&painter, area());
            assert_eq!(launcher.category_at(Point::from((5.0, 5.0))), None);
        }

        #[test]
        fn a_chip_cannot_be_clicked_before_it_has_been_drawn() {
            let launcher = mixed();
            assert_eq!(
                launcher.category_at(Point::from((960.0, 130.0))),
                None,
                "hit-testing must follow what was laid out, never a guess"
            );
        }

        #[test]
        fn one_lone_category_is_not_worth_a_chip_row() {
            let launcher = launcher(4);
            assert_eq!(
                launcher.categories().len(),
                2,
                "the fixtures are all Other, so only All and Other qualify"
            );

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
        fn the_first_arrow_press_selects_the_first_cell_of_the_page() {
            let mut launcher = launcher(12);
            assert_eq!(launcher.selected(), None);
            assert!(launcher.move_selection(1, 0));
            assert_eq!(launcher.selected(), Some(0));
        }

        #[test]
        fn horizontal_movement_walks_one_cell_at_a_time() {
            let mut launcher = launcher(12);
            launcher.move_selection(1, 0);
            launcher.move_selection(1, 0);
            assert_eq!(launcher.selected(), Some(1));
        }

        #[test]
        fn vertical_movement_steps_a_whole_row() {
            let columns = grid_for(area()).columns;
            let mut launcher = launcher(per_page());
            launcher.move_selection(1, 0);
            launcher.move_selection(0, 1);
            assert_eq!(launcher.selected(), Some(columns));
        }

        #[test]
        fn selection_stops_at_the_ends_instead_of_wrapping() {
            let mut launcher = launcher(3);
            launcher.move_selection(1, 0);
            assert!(!launcher.move_selection(-1, 0), "already at the first cell");

            for _ in 0..10 {
                launcher.move_selection(1, 0);
            }
            assert_eq!(launcher.selected(), Some(2), "clamped to the last result");
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

        #[test]
        fn hovering_moves_the_same_selection_the_keyboard_uses() {
            let mut launcher = launcher(12);
            launcher.set_hover(Some(4));
            assert_eq!(launcher.selected(), Some(4));

            launcher.move_selection(1, 0);
            assert_eq!(
                launcher.selected(),
                Some(5),
                "arrows continue from the hover"
            );
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
                let width = if candidate == launcher.page {
                    DOT_ACTIVE_WIDTH
                } else {
                    DOT_SIZE
                };
                x += width + DOT_GAP;
            }
            Point::from((
                f64::from(x + DOT_SIZE / 2),
                f64::from(strip.loc.y + strip.size.h / 2),
            ))
        }

        #[test]
        fn a_dot_reports_its_page() {
            let launcher = paged();
            assert_eq!(launcher.dot_at(area(), dot_centre(&launcher, 1)), Some(1));
            assert_eq!(launcher.dot_at(area(), dot_centre(&launcher, 2)), Some(2));
        }

        #[test]
        fn clicking_a_dot_jumps_straight_to_that_page() {
            let mut launcher = paged();
            assert!(launcher.go_to_page(2));
            assert_eq!(launcher.page, 2);
            assert!(!launcher.go_to_page(2), "already there");
            assert!(!launcher.go_to_page(99), "out of range");
        }

        #[test]
        fn a_single_page_shows_no_clickable_strip() {
            let launcher = launcher(3);
            assert_eq!(launcher.page_count(), 1);
            assert_eq!(launcher.dot_at(area(), dot_centre(&launcher, 0)), None);
        }

        #[test]
        fn a_point_away_from_the_strip_hits_no_dot() {
            let launcher = paged();
            assert_eq!(launcher.dot_at(area(), Point::from((5.0, 5.0))), None);
        }
    }

    mod caption_wrapping {
        use super::*;

        fn painter() -> crate::paint::Painter {
            crate::paint::Painter::new(14.0, None)
        }

        fn caption_width() -> f32 {
            #[allow(clippy::cast_precision_loss)]
            {
                (CELL_WIDTH - CAPTION_PAD_X * 2) as f32
            }
        }

        #[test]
        fn a_short_name_stays_on_one_line() {
            let (first, second) = wrap_caption(&painter(), "Files", caption_width());
            assert_eq!(first, "Files");
            assert_eq!(second, None);
        }

        #[test]
        fn a_long_name_breaks_at_a_word_boundary() {
            let name = "Advanced Network Configuration";
            let (first, second) = wrap_caption(&painter(), name, caption_width());
            let second = second.expect("a name this long needs a second line");

            assert!(
                name.starts_with(&first),
                "the break must fall between words, not mid-token: {first:?}"
            );
            assert!(name.ends_with(&second), "second line {second:?}");
            assert!(!first.contains("  ") && !second.starts_with(' '));
        }

        #[test]
        fn the_first_line_fits_the_cell() {
            let p = painter();
            let (first, _) = wrap_caption(&p, "GNOME System Monitor", caption_width());
            assert!(
                p.text_width_styled(&first, &glide_theme::TextStyle::LAUNCHER_CAPTION)
                    <= caption_width(),
                "first line {first:?} overflows the cell"
            );
        }

        #[test]
        fn a_single_unbreakable_word_is_left_for_the_ellipsis() {
            let (first, second) = wrap_caption(
                &painter(),
                "Supercalifragilisticexpialidocious",
                caption_width(),
            );
            assert_eq!(second, None, "there is no boundary to break on");
            assert_eq!(first, "Supercalifragilisticexpialidocious");
        }

        #[test]
        fn every_word_lands_on_one_of_the_two_lines() {
            let name = "Qt V4L2 video capture test utility";
            let (first, second) = wrap_caption(&painter(), name, caption_width());
            let joined = match second {
                Some(second) => format!("{first} {second}"),
                None => first,
            };
            assert_eq!(
                joined.split_whitespace().collect::<Vec<_>>(),
                name.split_whitespace().collect::<Vec<_>>(),
                "wrapping must not drop or duplicate words"
            );
        }
    }

    mod fallback_icons {
        use super::*;

        #[test]
        fn the_initial_is_the_first_alphanumeric_character() {
            assert_eq!(fallback_initial("Firefox"), "F");
            assert_eq!(fallback_initial("  gimp"), "G");
            assert_eq!(fallback_initial("7-Zip"), "7");
        }

        #[test]
        fn a_nameless_app_still_gets_a_glyph() {
            assert_eq!(fallback_initial(""), "?");
            assert_eq!(fallback_initial("--- "), "?");
        }
    }

    mod cell_rhythm {
        use super::*;

        #[test]
        fn the_icon_and_caption_fit_inside_the_cell() {
            let used = CELL_PAD_TOP + ICON_SIZE + CAPTION_GAP + CAPTION_HEIGHT;
            assert!(
                used <= CELL_HEIGHT,
                "cell content needs {used}px but the cell is {CELL_HEIGHT}px"
            );
        }

        #[test]
        fn the_icon_leaves_a_gutter_on_both_sides() {
            let gutter = (CELL_WIDTH - ICON_SIZE) / 2;
            assert!(
                gutter >= 24,
                "only {gutter}px between an icon and the cell edge; neighbours will touch"
            );
        }

        #[test]
        fn the_caption_band_is_tall_enough_for_two_lines() {
            assert_eq!(CAPTION_HEIGHT, CAPTION_LINE * CAPTION_LINES);
            #[allow(clippy::cast_precision_loss)]
            let line = CAPTION_LINE as f32;
            assert!(
                line >= glide_theme::TextStyle::LAUNCHER_CAPTION.line_height_px,
                "a caption line is shorter than the style's own line height"
            );
        }

        #[test]
        fn the_icon_rect_is_centred_in_its_cell() {
            let launcher = launcher(1);
            let grid = grid_for(area());
            let icon = launcher.icon_rects(area())[0];
            let cell = grid.cell(0);
            let left = icon.loc.x - cell.loc.x;
            let right = (cell.loc.x + CELL_WIDTH) - (icon.loc.x + ICON_SIZE);
            assert_eq!(left, right);
        }
    }

    mod chrome_geometry {
        use super::*;

        #[test]
        fn the_search_field_sits_above_the_grid_and_never_overlaps_it() {
            let area = area();
            let search = search_rect(area);
            let grid = grid_for(area);
            assert!(
                search.loc.y + search.size.h <= grid.rect.loc.y,
                "search ends at {}, grid starts at {}",
                search.loc.y + search.size.h,
                grid.rect.loc.y
            );
        }

        #[test]
        fn the_search_field_is_centred() {
            let area = area();
            let search = search_rect(area);
            let left = search.loc.x - area.loc.x;
            let right = (area.loc.x + area.size.w) - (search.loc.x + search.size.w);
            assert!((left - right).abs() <= 1);
        }

        #[test]
        fn the_search_field_shrinks_rather_than_overflowing_a_narrow_screen() {
            let narrow = rect(420, 900);
            let search = search_rect(narrow);
            assert!(search.loc.x >= narrow.loc.x);
            assert!(search.size.w <= narrow.size.w);
        }

        #[test]
        fn the_chip_row_sits_between_the_search_field_and_the_grid() {
            let area = area();
            let mut launcher = launcher(6);
            let painter = crate::paint::Painter::new(14.0, None);
            let chips = launcher.layout_chips(&painter, area);
            let search = search_rect(area);
            let grid = grid_for(area);

            for (_, rect) in chips {
                assert!(
                    rect.loc.y >= search.loc.y + search.size.h,
                    "a chip overlaps the search field"
                );
                assert!(
                    rect.loc.y + rect.size.h <= grid.rect.loc.y,
                    "a chip overlaps the grid"
                );
            }
        }

        #[test]
        fn the_chip_row_is_centred() {
            let area = area();
            let mut launcher = launcher(6);
            let painter = crate::paint::Painter::new(14.0, None);
            let chips = launcher.layout_chips(&painter, area);
            let (Some(first), Some(last)) = (chips.first(), chips.last()) else {
                return;
            };
            let left = first.1.loc.x - area.loc.x;
            let right = (area.loc.x + area.size.w) - (last.1.loc.x + last.1.size.w);
            assert!((left - right).abs() <= 1, "{left} vs {right}");
        }

        #[test]
        fn the_grid_still_clears_the_pagination_strip() {
            let area = area();
            let grid = grid_for(area);
            let strip = dots_rect(area, 3);
            assert!(
                grid.rect.loc.y + grid.rect.size.h <= strip.loc.y,
                "grid reaches {}, dots start at {}",
                grid.rect.loc.y + grid.rect.size.h,
                strip.loc.y
            );
        }
    }

    mod responsive_grid {
        use super::*;

        #[test]
        fn a_1080p_screen_fits_far_more_than_the_old_fixed_six_by_four() {
            let grid = grid_for(area());
            assert!(
                grid.per_page() > 24,
                "a fixed 6x4 grid left most of a 1920x1000 area empty; got {}x{}",
                grid.columns,
                grid.rows
            );
        }

        #[test]
        fn the_grid_uses_a_healthy_share_of_the_width() {
            let grid = grid_for(area());
            let ratio = f64::from(grid.rect.size.w) / f64::from(area().size.w);
            assert!(
                ratio > 0.55,
                "grid spans only {ratio:.2} of the screen width"
            );
        }

        #[test]
        fn a_small_area_still_yields_a_usable_grid() {
            let grid = grid_for(rect(640, 480));
            assert!(grid.columns >= MIN_COLUMNS);
            assert!(grid.rows >= MIN_ROWS);
            assert!(grid.per_page() >= MIN_COLUMNS * MIN_ROWS);
        }

        #[test]
        fn a_huge_area_is_capped_rather_than_growing_without_bound() {
            let grid = grid_for(rect(7680, 4320));
            assert_eq!(grid.columns, MAX_COLUMNS);
            assert_eq!(grid.rows, MAX_ROWS);
        }

        #[test]
        fn a_wider_screen_never_fits_fewer_columns() {
            let mut previous = 0;
            for width in [800, 1280, 1600, 1920, 2560, 3840] {
                let columns = grid_for(rect(width, 1000)).columns;
                assert!(
                    columns >= previous,
                    "width {width} fit fewer columns than the one before it"
                );
                previous = columns;
            }
        }

        #[test]
        fn the_grid_leaves_room_below_itself_for_the_pagination_dots() {
            let area = area();
            let grid = grid_for(area);
            let bottom = grid.rect.loc.y + grid.rect.size.h;
            let dots_top = area.loc.y + area.size.h - DOT_MARGIN - DOT_SIZE;
            assert!(
                bottom <= dots_top,
                "grid reaches {bottom}, dots start at {dots_top}"
            );
        }
    }

    #[test]
    fn a_right_click_menu_opens_above_the_app_and_offers_pin_or_unpin() {
        let mut launcher = launcher(4);
        let icon = launcher.icon_rects(area())[2];
        assert!(launcher.open_menu(2, area(), false));
        let menu = launcher.menu_rect().expect("menu rect");
        assert!(menu.loc.y + menu.size.h < icon.loc.y);
        assert_eq!(launcher.menu_result(), Some(2));

        let row = crate::popup_menu::row_rect(menu, 1);
        let centre = Point::from((
            f64::from(row.loc.x + row.size.w / 2),
            f64::from(row.loc.y + row.size.h / 2),
        ));
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
    fn page_count_follows_what_actually_fits() {
        let full = per_page();
        assert_eq!(launcher(full).page_count(), 1);
        assert_eq!(launcher(full + 1).page_count(), 2);
    }

    #[test]
    fn an_empty_launcher_still_reports_one_page() {
        assert_eq!(Launcher::new(Vec::new(), area()).page_count(), 1);
    }

    #[test]
    fn the_grid_is_centred_horizontally_in_the_area() {
        let grid = grid_for(area()).rect;
        let left = grid.loc.x - area().loc.x;
        let right = (area().loc.x + area().size.w) - (grid.loc.x + grid.size.w);
        assert!((left - right).abs() <= 1, "{left} vs {right}");
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
    fn a_second_page_shows_the_remaining_apps() {
        let full = per_page();
        let mut launcher = launcher(full + 6);
        assert_eq!(launcher.page_range(), 0..full);

        assert!(launcher.turn_page(true));
        assert_eq!(launcher.page_range(), full..full + 6);
        assert_eq!(launcher.icon_rects(area()).len(), 6);
    }

    #[test]
    fn paging_stops_at_both_ends() {
        let mut launcher = launcher(per_page() + 6);
        assert!(!launcher.turn_page(false), "already on the first page");
        assert!(launcher.turn_page(true));
        assert!(!launcher.turn_page(true), "already on the last page");
    }

    #[test]
    fn growing_the_area_clamps_the_current_page_instead_of_stranding_it() {
        let small = rect(640, 480);
        let small_page = grid_for(small).per_page();
        assert!(
            small_page < per_page(),
            "the two areas must disagree for this to test anything"
        );

        let mut launcher = Launcher::new(apps(small_page + 1), small);
        assert!(launcher.turn_page(true));
        assert_eq!(launcher.page, 1);

        launcher.set_area(area());
        assert_eq!(launcher.page_count(), 1);
        assert_eq!(launcher.page, 0, "the page must not point past the end");
        assert!(!launcher.page_range().is_empty());
    }

    mod hit_testing {
        use super::*;

        #[test]
        fn every_cell_reports_its_own_app() {
            let grid = grid_for(area());
            let launcher = launcher(grid.per_page());
            for slot in 0..grid.per_page() {
                let cell = grid.cell(slot);
                let centre = Point::from((
                    f64::from(cell.loc.x + CELL_WIDTH / 2),
                    f64::from(cell.loc.y + CELL_HEIGHT / 2),
                ));
                assert_eq!(launcher.app_at(area(), centre), Some(slot));
            }
        }

        #[test]
        fn the_caption_area_hits_the_same_app() {
            let launcher = launcher(1);
            let icon = launcher.icon_rects(area())[0];
            let below = Point::from((
                f64::from(icon.loc.x + icon.size.w / 2),
                f64::from(icon.loc.y + ICON_SIZE + CAPTION_GAP + 2),
            ));
            assert_eq!(launcher.app_at(area(), below), Some(0));
        }

        #[test]
        fn a_point_outside_the_grid_hits_nothing() {
            let launcher = launcher(per_page());
            assert_eq!(launcher.app_at(area(), Point::from((5.0, 5.0))), None);
        }

        #[test]
        fn an_empty_cell_on_a_short_page_hits_nothing() {
            let grid = grid_for(area());
            let launcher = launcher(3);
            let fourth = grid.cell(3);
            let centre = Point::from((
                f64::from(fourth.loc.x + CELL_WIDTH / 2),
                f64::from(fourth.loc.y + CELL_HEIGHT / 2),
            ));
            assert_eq!(launcher.app_at(area(), centre), None);
        }
    }

    mod animation {
        use super::*;

        #[test]
        fn it_opens_from_zero_and_settles_at_one() {
            let mut launcher = launcher(1);
            assert!(launcher.progress() < 1.0);

            launcher.phase = Phase::Open;
            assert!((launcher.progress() - 1.0).abs() < f64::EPSILON);
        }

        #[test]
        fn closing_runs_back_towards_zero() {
            let mut launcher = launcher(1);
            launcher.phase = Phase::Open;
            launcher.begin_close();
            assert!(launcher.is_closing());
            assert!(launcher.progress() <= 1.0);
        }

        #[test]
        fn a_finished_close_asks_to_be_dropped() {
            let mut launcher = launcher(1);
            launcher.begin_close();
            launcher.started = Instant::now()
                .checked_sub(open_duration() * 2 + Duration::from_millis(10))
                .expect("process uptime");
            assert!(!launcher.tick(), "a finished close must not linger");
        }

        #[test]
        fn turning_the_page_drops_a_stale_hover() {
            let mut launcher = launcher(per_page() + 6);
            launcher.set_hover(Some(2));
            launcher.turn_page(true);
            assert_eq!(launcher.hover.target(), None);
        }
    }
}

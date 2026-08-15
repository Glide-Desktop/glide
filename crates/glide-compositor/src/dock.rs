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
const RUN_OFFSET: i32 = 5;
const RUN_THICKNESS: i32 = 4;

const SEPARATOR_WIDTH: i32 = 1;
const SEPARATOR_MARGIN: i32 = 3;

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
        Self {
            name: "Applications".to_string(),
            command: String::new(),
            app_id: String::new(),
            glyph: Glyph::Grid,
            color_from: tokens.fill_1,
            color_to: tokens.fill_1,
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
    icons: crate::icon::IconLoader,

    shell: std::collections::HashMap<(i32, i32, i32), MemoryRenderBuffer>,
    shadow: std::collections::HashMap<(i32, i32, i32), MemoryRenderBuffer>,
    tiles: Vec<std::collections::HashMap<i32, MemoryRenderBuffer>>,
    runs: std::collections::HashMap<(i32, bool, i32), MemoryRenderBuffer>,
    shell_backdrop_key: Option<(u64, i32, i32)>,
    raster_rev: u64,
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
            run_changed: vec![Instant::now(); count],
            hover: glide_ui::HoverAnimation::default(),
            icons: crate::icon::IconLoader::new(config.theme.icon_theme.clone()),
            shell: std::collections::HashMap::new(),
            shadow: std::collections::HashMap::new(),
            tiles: vec![std::collections::HashMap::new(); count],
            runs: std::collections::HashMap::new(),
            shell_backdrop_key: None,
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
        let settled = now
            .checked_sub(run_anim().max(glide_theme::motion::T3))
            .unwrap_or(now);
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
            let hover_moved = self.hover.target().is_some_and(|index| {
                match (self.items.get(index), items.get(index)) {
                    (Some(old), Some(new)) => old.app_id != new.app_id,
                    _ => true,
                }
            });
            if hover_moved {
                self.hover.retarget_without_restarting(None);
            }
        }

        self.items = items;
        self.run = run;
        true
    }

    pub fn set_hover(&mut self, index: Option<usize>) -> bool {
        self.hover.set(index)
    }

    #[must_use]
    pub fn is_animating(&self) -> bool {
        let run = run_anim();
        self.hover.is_animating(hover_anim())
            || (!run.is_zero() && self.run_changed.iter().any(|stamp| stamp.elapsed() < run))
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
        Size::from((width, theme.padding_y * 2 + theme.item_size))
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
        let shell = self.rect(area);
        let theme = &self.theme;
        let mut x = shell.loc.x + theme.padding_x;
        let y = shell.loc.y + theme.padding_y;

        self.items
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
        self.is_enabled() && self.rect(area).to_f64().contains(pos)
    }
}

pub fn render<R>(
    dock: &mut Dock,
    renderer: &mut R,
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

    let mut shell = dock.rect(area);
    #[allow(clippy::cast_possible_truncation)]
    let slide = (f64::from(shell.size.h + dock.theme.bottom_margin) * hidden).round() as i32;
    shell.loc.y += slide;
    #[allow(clippy::cast_possible_truncation)]
    let opacity = (1.0 - hidden) as f32;
    let item_rects: Vec<Rectangle<i32, Logical>> = dock
        .item_rects(area)
        .into_iter()
        .map(|mut rect| {
            rect.loc.y += slide;
            rect
        })
        .collect();
    let hovered = dock.hover.target();
    let hover_t = dock.hover_progress();
    let theme = dock.theme.clone();

    let mut elements = Vec::new();

    for (index, rect) in item_rects.iter().enumerate() {
        let hover = HoverGeometry::for_tile(&theme, *rect, hovered == Some(index), hover_t);
        push_tile(
            dock,
            renderer,
            index,
            &theme,
            &hover,
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

#[allow(clippy::too_many_arguments)]
fn push_shell<R>(
    dock: &mut Dock,
    renderer: &mut R,
    theme: &DockTheme,
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
    let shell_key = (shell.size.w, shell.size.h, output_scale);
    let backdrop_key = backdrop.map(|b| (b.generation, shell.loc.x, shell.loc.y));
    if dock.shell_backdrop_key != backdrop_key {
        dock.shell_backdrop_key = backdrop_key;
        dock.shell.clear();
    }
    if let std::collections::hash_map::Entry::Vacant(entry) = dock.shell.entry(shell_key) {
        if let Some(buffer) = draw_shell(
            theme,
            &dock.items,
            item_rects,
            shell,
            output_scale,
            backdrop,
        ) {
            entry.insert(buffer);
        }
    }
    if let Some(buffer) = dock.shell.get(&shell_key) {
        if let Ok(element) = MemoryRenderBufferRenderElement::from_buffer(
            renderer,
            shell.loc.to_f64().to_physical(scale),
            buffer,
            Some(opacity),
            None,
            None,
            Kind::Unspecified,
        ) {
            elements.push(element);
        }
    }

    if let std::collections::hash_map::Entry::Vacant(entry) = dock.shadow.entry(shell_key) {
        if let Some(buffer) = draw_shell_shadow(theme, shell, output_scale) {
            entry.insert(buffer);
        }
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

fn shell_shadow_margin() -> i32 {
    #[allow(clippy::cast_possible_truncation)]
    let widest = shell_shadow_layers()
        .iter()
        .map(|layer| layer.blur)
        .fold(0.0_f32, f32::max)
        .ceil() as i32;
    crate::paint::shadow_fade_margin(widest)
}

fn shell_shadow_layers() -> Vec<crate::paint::ShadowLayerSpec> {
    glide_theme::elevation::layers(
        glide_theme::elevation::Level::El2,
        glide_theme::Appearance::Dark,
    )
    .into_iter()
    .map(|layer| crate::paint::ShadowLayerSpec {
        dy: layer.dy,
        blur: layer.blur,
        color: layer.color,
    })
    .collect()
}

fn draw_shell_shadow(
    theme: &DockTheme,
    shell: Rectangle<i32, Logical>,
    output_scale: i32,
) -> Option<MemoryRenderBuffer> {
    let margin = shell_shadow_margin();
    let (w, h) = (shell.size.w + margin * 2, shell.size.h + margin * 2);
    let (pw, ph) = (w * output_scale, h * output_scale);
    let mut buf = vec![0u8; usize::try_from(pw * ph * 4).ok()?];

    #[allow(clippy::cast_precision_loss)]
    let scale_f = output_scale as f32;
    let layers: Vec<_> = shell_shadow_layers()
        .into_iter()
        .map(|layer| crate::paint::ShadowLayerSpec {
            dy: layer.dy * scale_f,
            blur: layer.blur * scale_f,
            color: layer.color,
        })
        .collect();
    #[allow(clippy::cast_precision_loss)]
    crate::paint::draw_outset_shadow_and_border(
        &mut buf,
        pw,
        ph,
        shell.size.w as f32 * scale_f,
        shell.size.h as f32 * scale_f,
        &layers,
        0.0,
        crate::config::Color([0.0, 0.0, 0.0, 0.0]),
        theme.radius * scale_f,
        Some(theme.radius * scale_f),
    );

    Some(MemoryRenderBuffer::from_slice(
        &buf,
        smithay::backend::allocator::Fourcc::Argb8888,
        (pw, ph),
        output_scale,
        Transform::Normal,
        None,
    ))
}

struct HoverGeometry {
    position: Point<f64, Logical>,
    side: i32,
}

impl HoverGeometry {
    fn for_tile(
        theme: &DockTheme,
        rect: Rectangle<i32, Logical>,
        is_hovered: bool,
        hover_t: f64,
    ) -> Self {
        if !is_hovered {
            return Self {
                position: Point::from((f64::from(rect.loc.x), f64::from(rect.loc.y))),
                side: theme.item_size,
            };
        }

        #[allow(clippy::cast_possible_truncation)]
        let lift = (f64::from(theme.hover_lift) * hover_t).round() as i32;
        #[allow(clippy::cast_possible_truncation)]
        let side = (f64::from(theme.item_size) * (1.0 + (theme.hover_scale - 1.0) * hover_t))
            .round() as i32;

        let inset = f64::from(side - theme.item_size) / 2.0;
        Self {
            position: Point::from((
                f64::from(rect.loc.x) - inset,
                f64::from(rect.loc.y) - inset - f64::from(lift),
            )),
            side,
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
    scale: Scale<f64>,
    output_scale: i32,
    opacity: f32,
    elements: &mut Vec<MemoryRenderBufferRenderElement<R>>,
) where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Send + Clone + 'static,
{
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
fn push_run_indicator<R>(
    dock: &mut Dock,
    renderer: &mut R,
    index: usize,
    theme: &DockTheme,
    rect: Rectangle<i32, Logical>,
    scale: Scale<f64>,
    output_scale: i32,
    opacity: f32,
    elements: &mut Vec<MemoryRenderBufferRenderElement<R>>,
) where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Send + Clone + 'static,
{
    let state = dock.run.get(index).copied().unwrap_or_default();
    if !state.running {
        return;
    }
    let width = dock.run_width(index);

    let key = (width, state.focused, output_scale);
    if let std::collections::hash_map::Entry::Vacant(entry) = dock.runs.entry(key) {
        if let Some(buffer) = draw_run_indicator(theme, width, state.focused, output_scale) {
            entry.insert(buffer);
        }
    }
    let Some(buffer) = dock.runs.get(&key) else {
        return;
    };

    let pos = Point::from((
        rect.loc.x + (theme.item_size - width) / 2,
        rect.loc.y + theme.item_size + RUN_OFFSET - RUN_THICKNESS,
    ));
    if let Ok(element) = MemoryRenderBufferRenderElement::from_buffer(
        renderer,
        pos.to_f64().to_physical(scale),
        buffer,
        Some(opacity),
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
        crate::paint::Painter::fill_rounded_rect_over(
            &mut buf,
            w,
            h,
            x,
            theme.padding_y + 4,
            SEPARATOR_WIDTH,
            theme.item_size - 8,
            scale,
            theme.separator,
            0.0,
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
    crate::paint::draw_dock_glyph(&mut buf, side, side, scale, item.glyph.into(), theme.icon);

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
        Color([r, g, b, a * 0.8])
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
}

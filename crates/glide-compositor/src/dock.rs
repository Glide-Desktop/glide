//! The bottom dock (`.dash`), spec §13.4.
//!
//! A compositor-drawn shell element: a rounded, translucent bar centred on
//! the bottom of the work area holding one square tile per pinned app,
//! each with a running indicator underneath. It is not a Wayland surface,
//! so it is laid out, rasterised and hit-tested here rather than by a
//! client.

use std::time::Duration;

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

/// `.dash-item` hover runs on `--t-2` with the overshoot curve.
const HOVER_ANIM: Duration = glide_theme::motion::T2;

/// `.dash-item .run` — 4px dot, 16px bar when focused.
const RUN_DOT: i32 = 4;
const RUN_BAR: i32 = 16;
/// `bottom: -5px` relative to the tile.
const RUN_OFFSET: i32 = 5;
const RUN_THICKNESS: i32 = 4;

/// Width of the `.dash-sep` divider plus the margin either side.
const SEPARATOR_WIDTH: i32 = 1;
const SEPARATOR_MARGIN: i32 = 3;

/// Built-in tile artwork.
///
/// Real `.desktop`/icon-theme lookup is a subsystem of its own; the design
/// draws simple stroked glyphs on gradient tiles, and these reproduce
/// exactly that set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Glyph {
    Terminal,
    Files,
    Settings,
    Music,
    Mail,
    Generic,
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

/// One pinned app.
#[derive(Debug, Clone)]
pub struct DockItem {
    pub name: String,
    pub command: String,
    pub app_id: String,
    pub glyph: Glyph,
    pub color_from: Color,
    pub color_to: Color,
    pub separator_before: bool,
}

impl DockItem {
    /// A tile for a running app that is not pinned. It carries no command
    /// — clicking it focuses the window it stands for.
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
        }
    }
}

/// Whether an app currently has windows, and whether one of them is
/// focused — drives the running indicator.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RunState {
    pub running: bool,
    pub focused: bool,
}

/// An app with at least one open window, as reported by the compositor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunningApp {
    pub app_id: String,
    /// Human-readable name for a tile that is not pinned.
    pub name: String,
    pub focused: bool,
}

/// Gradient pairs handed out to running apps that are not pinned.
///
/// Picked deterministically from the app id so a given app keeps the same
/// tile across restarts, and so two apps side by side rarely collide.
const DYNAMIC_PALETTE: [(&str, &str); 6] = [
    ("#5b7fa8", "#2f4b6b"),
    ("#7fae94", "#2f7a56"),
    ("#c9a06a", "#8a5c2a"),
    ("#a87f9e", "#6b3f63"),
    ("#8a8377", "#4c4740"),
    ("#a86a6a", "#6b2f2f"),
];

fn dynamic_colors(app_id: &str) -> (Color, Color) {
    // FNV-1a: tiny, stable, and good enough to spread ids across six
    // buckets.
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

/// The dock's layout and interaction state.
#[derive(Debug)]
pub struct Dock {
    theme: DockTheme,
    /// Apps pinned in the config; always leftmost, always present.
    pinned: Vec<DockItem>,
    /// `pinned` followed by a tile for every running app that is not
    /// pinned, so an open window always has somewhere to show up.
    items: Vec<DockItem>,
    run: Vec<RunState>,
    hover: glide_ui::HoverAnimation<usize>,
    icons: crate::icon::IconLoader,

    /// Cached shell background, keyed by its size and the output scale.
    shell: Option<(i32, i32, i32, MemoryRenderBuffer)>,
    /// Cached tiles, one per item, keyed by output scale.
    tiles: Vec<Option<(i32, MemoryRenderBuffer)>>,
    /// Cached running indicators, keyed by (width, focused, scale).
    runs: Vec<Option<(i32, bool, i32, MemoryRenderBuffer)>>,
}

impl Dock {
    #[must_use]
    pub fn new(config: &DockConfig) -> Self {
        let pinned: Vec<DockItem> = config.items.iter().map(DockItem::from_config).collect();
        let items = pinned.clone();
        let count = items.len();
        Self {
            theme: config.theme.clone(),
            pinned,
            items,
            run: vec![RunState::default(); count],
            hover: glide_ui::HoverAnimation::default(),
            icons: crate::icon::IconLoader::new(config.theme.icon_theme.clone()),
            shell: None,
            tiles: vec![None; count],
            runs: vec![None; count],
        }
    }

    /// Rebuilds from a (possibly reloaded) config, dropping caches — the
    /// icon theme may have changed along with everything else.
    pub fn reconfigure(&mut self, config: &DockConfig) {
        *self = Self::new(config);
    }

    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.theme.enabled && !self.items.is_empty()
    }

    /// Whether the tile at `index` came from the config rather than from a
    /// running window.
    #[must_use]
    pub fn is_pinned(&self, index: usize) -> bool {
        index < self.pinned.len()
    }

    #[must_use]
    pub fn items(&self) -> &[DockItem] {
        &self.items
    }

    /// Rebuilds the dock from the apps that currently have windows.
    ///
    /// Pinned tiles keep their place and gain a running dot; anything else
    /// with an open window is appended so a new window always appears
    /// somewhere. Returns whether anything changed, so the caller can skip
    /// a redraw.
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
            // Divide the pinned block from the running-only one, as the
            // design separates its groups.
            item.separator_before = extras == 0 && !self.pinned.is_empty();
            items.push(item);
            extras += 1;
        }

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

        if layout_changed {
            // Caches are indexed by position, so a changed line-up
            // invalidates all of them.
            self.tiles = vec![None; items.len()];
            self.runs = vec![None; items.len()];
            self.shell = None;
            if self
                .hover
                .target()
                .is_some_and(|index| index >= items.len())
            {
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
        self.hover.is_animating(HOVER_ANIM)
    }

    fn hover_progress(&self) -> f64 {
        glide_theme::motion::E_OVER.ease_f64(self.hover.progress(HOVER_ANIM))
    }

    /// Total size of the dock shell.
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

    /// Where the dock sits inside `area`, centred on its bottom edge.
    #[must_use]
    pub fn rect(&self, area: Rectangle<i32, Logical>) -> Rectangle<i32, Logical> {
        let size = self.size();
        let x = area.loc.x + (area.size.w - size.w) / 2;
        let y = area.loc.y + area.size.h - size.h - self.theme.bottom_margin;
        Rectangle::new(Point::from((x, y)), size)
    }

    /// Resting rect of every tile, in the same space as `rect`.
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

    /// The tile under `pos`, if any.
    ///
    /// Tests the *resting* rects rather than the lifted ones: a tile that
    /// moved out from under the pointer because it was hovered would
    /// otherwise immediately un-hover and drop back, flickering.
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

    /// Whether `pos` is anywhere over the dock, tile or not.
    #[must_use]
    pub fn contains(&self, area: Rectangle<i32, Logical>, pos: Point<f64, Logical>) -> bool {
        self.rect(area).to_f64().contains(pos)
    }
}

/// Render elements for the dock, front-to-back.
pub fn render<R>(
    dock: &mut Dock,
    renderer: &mut R,
    area: Rectangle<i32, Logical>,
    scale: Scale<f64>,
    output_scale: i32,
) -> Vec<MemoryRenderBufferRenderElement<R>>
where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Send + Clone + 'static,
{
    if !dock.is_enabled() {
        return Vec::new();
    }

    let shell = dock.rect(area);
    let item_rects = dock.item_rects(area);
    let hovered = dock.hover.target();
    let hover_t = dock.hover_progress();
    let theme = dock.theme.clone();

    let mut elements = Vec::new();

    // Tiles and their indicators sit in front of the shell.
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
            &mut elements,
        );
    }

    // Shell background last, so it lands behind the tiles.
    let needs_shell = dock.shell.as_ref().is_none_or(|(w, h, cached_scale, _)| {
        *w != shell.size.w || *h != shell.size.h || *cached_scale != output_scale
    });
    if needs_shell {
        if let Some(buffer) = draw_shell(&theme, &dock.items, &item_rects, shell, output_scale) {
            dock.shell = Some((shell.size.w, shell.size.h, output_scale, buffer));
        }
    }
    if let Some((_, _, _, buffer)) = dock.shell.as_ref() {
        if let Ok(element) = MemoryRenderBufferRenderElement::from_buffer(
            renderer,
            shell.loc.to_f64().to_physical(scale),
            buffer,
            Some(1.0),
            None,
            None,
            Kind::Unspecified,
        ) {
            elements.push(element);
        }
    }

    elements
}

/// Where a tile is drawn once the hover lift and swell are applied.
struct HoverGeometry {
    /// Top-left in logical px, already lifted and centred on the growth.
    position: Point<f64, Logical>,
    /// Side length after the swell.
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

        // Grow about the tile's centre so it swells in place rather than
        // creeping toward one corner.
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
    elements: &mut Vec<MemoryRenderBufferRenderElement<R>>,
) where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Send + Clone + 'static,
{
    if dock.tiles[index]
        .as_ref()
        .is_none_or(|(cached, _)| *cached != output_scale)
    {
        // The icon is looked up here rather than inside `draw_tile` so the
        // loader's cache is borrowed separately from the item list.
        let icon = {
            let item = &dock.items[index];
            let glyph_side = theme.item_size * scale_or_one(output_scale);
            dock.icons
                .load(&item.app_id, glyph_side)
                .map(<[u8]>::to_vec)
        };
        if let Some(buffer) = draw_tile(theme, &dock.items[index], icon.as_deref(), output_scale) {
            dock.tiles[index] = Some((output_scale, buffer));
        }
    }
    let Some((_, buffer)) = dock.tiles[index].as_ref() else {
        return;
    };

    // `src` must be stated explicitly: left as `None` smithay derives it
    // from the destination size, which samples a larger region instead of
    // scaling the tile.
    let src = Rectangle::from_size(Size::from((
        f64::from(theme.item_size),
        f64::from(theme.item_size),
    )));
    if let Ok(element) = MemoryRenderBufferRenderElement::from_buffer(
        renderer,
        hover.position.to_physical(scale),
        buffer,
        Some(1.0),
        Some(src),
        Some(Size::from((hover.side, hover.side))),
        Kind::Unspecified,
    ) {
        elements.push(element);
    }
}

/// The dot (or bar) under a running app.
///
/// Pinned to the tile's *resting* rect, so it stays put while the tile
/// lifts away from it on hover — as it does in the design, where `.run` is
/// positioned against the button rather than riding its transform.
#[allow(clippy::too_many_arguments)]
fn push_run_indicator<R>(
    dock: &mut Dock,
    renderer: &mut R,
    index: usize,
    theme: &DockTheme,
    rect: Rectangle<i32, Logical>,
    scale: Scale<f64>,
    output_scale: i32,
    elements: &mut Vec<MemoryRenderBufferRenderElement<R>>,
) where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Send + Clone + 'static,
{
    let state = dock.run.get(index).copied().unwrap_or_default();
    if !state.running {
        return;
    }
    let width = if state.focused { RUN_BAR } else { RUN_DOT };

    if dock.runs[index]
        .as_ref()
        .is_none_or(|(cached_w, cached_focus, cached_scale, _)| {
            *cached_w != width || *cached_focus != state.focused || *cached_scale != output_scale
        })
    {
        if let Some(buffer) = draw_run_indicator(theme, width, state.focused, output_scale) {
            dock.runs[index] = Some((width, state.focused, output_scale, buffer));
        }
    }
    let Some((_, _, _, buffer)) = dock.runs[index].as_ref() else {
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
        Some(1.0),
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

/// The translucent bar plus its hairline and any dividers.
fn draw_shell(
    theme: &DockTheme,
    items: &[DockItem],
    item_rects: &[Rectangle<i32, Logical>],
    shell: Rectangle<i32, Logical>,
    scale: i32,
) -> Option<MemoryRenderBuffer> {
    let (w, h) = (shell.size.w, shell.size.h);
    let pixels = usize::try_from(w * scale * h * scale * 4).ok()?;
    let mut buf = vec![0u8; pixels];

    crate::paint::Painter::fill_panel_background(
        &mut buf,
        w,
        h,
        scale,
        theme.background,
        theme.radius,
        [true; 4],
    );
    #[allow(clippy::cast_precision_loss)]
    crate::paint::draw_inset_border(
        &mut buf,
        w * scale,
        h * scale,
        scale as f32,
        theme.radius * scale as f32,
        theme.border,
    );

    // Dividers sit in the gap before the item that asked for one.
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

/// One app tile.
///
/// A real themed icon is drawn on its own, the way a desktop icon is meant
/// to look. Only when none was found does the tile fall back to the
/// design's gradient plate with a built-in glyph, so an app without an
/// installed icon still reads as a proper tile rather than a blank.
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

/// The dot (or bar, when focused) under a running app.
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
        // `.dash-item.running .run { opacity: .8 }`
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
    fn an_empty_dock_is_disabled() {
        let dock = Dock::new(&config(0));
        assert!(!dock.is_enabled());
        assert_eq!(dock.size(), Size::from((0, 0)));
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
        assert_eq!(rects.len(), 4);

        let theme = DockTheme::default();
        for pair in rects.windows(2) {
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
            assert_eq!(dock.items().len(), 2, "no tile should be appended");
            assert!(dock.run[0].running && dock.run[0].focused);
            assert!(!dock.run[1].running);
        }

        /// The reported bug: a window from an app nobody pinned had
        /// nowhere to appear.
        #[test]
        fn an_unpinned_running_app_gets_its_own_tile() {
            let mut dock = Dock::new(&config(1));
            assert!(dock.set_running(&[app("org.example.Editor", true)]));

            assert_eq!(dock.items().len(), 2);
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
            assert_eq!(dock.items().len(), 2);

            assert!(dock.set_running(&[]));
            assert_eq!(dock.items().len(), 1, "the tile must go with the window");
        }

        #[test]
        fn matching_a_pinned_app_ignores_case() {
            let mut config = config(1);
            config.items[0].app_id = Some("Alacritty".to_string());
            let mut dock = Dock::new(&config);

            dock.set_running(&[app("alacritty", false)]);
            assert_eq!(dock.items().len(), 1, "it is the same app");
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
            assert!(!dock.is_enabled(), "an empty dock stays hidden");

            dock.set_running(&[app("org.example.Editor", true)]);
            assert!(dock.is_enabled(), "a running app should bring it back");
            assert_eq!(dock.items().len(), 1);
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
            dock.set_hover(Some(1));

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

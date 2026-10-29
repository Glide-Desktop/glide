use std::{collections::HashMap, time::Instant};

use glide_layout::WindowId;
use smithay::{
    backend::renderer::element::{
        Kind,
        memory::{MemoryRenderBuffer, MemoryRenderBufferRenderElement},
        utils::RescaleRenderElement,
    },
    utils::{Logical, Point, Rectangle, Scale, Size},
};

use crate::{
    launcher::LauncherTheme,
    power_dialog::{paint, rect},
};

const CELL: i32 = 104;
const MIN_CELL: i32 = 48;
const ICON_RATIO: f64 = 0.72;
const PAD: i32 = 16;
const LABEL_H: i32 = 32;
const EDGE_MARGIN: i32 = 48;
const OPEN_SCALE: f64 = 0.94;
const DOT: i32 = 4;
const DOT_RADIUS: f32 = 2.0;

pub type SwitcherRenderElement<R> = RescaleRenderElement<MemoryRenderBufferRenderElement<R>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub id: WindowId,
    pub app_id: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub id: WindowId,
    pub app_id: String,
    pub title: String,
    pub label: String,
    pub windows: Vec<WindowId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Apps,
    Windows,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Trigger {
    pub alt: bool,
    pub logo: bool,
    pub ctrl: bool,
}

impl Trigger {
    #[must_use]
    pub const fn is_empty(self) -> bool {
        !self.alt && !self.logo && !self.ctrl
    }

    #[must_use]
    pub const fn released(self, held: Self) -> bool {
        let still_held = (self.alt && held.alt) || (self.logo && held.logo) || (self.ctrl && held.ctrl);
        !self.is_empty() && !still_held
    }
}

#[must_use]
pub fn fallback_app_name(app_id: &str) -> String {
    let tail = app_id.rsplit('.').find(|part| !part.is_empty()).unwrap_or(app_id);
    let mut chars = tail.chars();
    chars.next().map_or_else(String::new, |first| first.to_uppercase().chain(chars).collect())
}

fn window_label(candidate: &Candidate, app_name: impl FnOnce() -> String) -> String {
    let title = candidate.title.trim();
    if title.is_empty() { app_name() } else { title.to_string() }
}

#[must_use]
pub fn app_entries(windows: &[Candidate], mut name: impl FnMut(&str) -> Option<String>) -> Vec<Entry> {
    let mut entries: Vec<Entry> = Vec::new();
    for window in windows {
        if !window.app_id.is_empty()
            && let Some(entry) = entries.iter_mut().find(|entry| entry.app_id == window.app_id)
        {
            entry.windows.push(window.id);
            continue;
        }
        let label = if window.app_id.is_empty() {
            window_label(window, String::new)
        } else {
            name(&window.app_id).unwrap_or_else(|| fallback_app_name(&window.app_id))
        };
        entries.push(Entry {
            id: window.id,
            app_id: window.app_id.clone(),
            title: window.title.clone(),
            label,
            windows: vec![window.id],
        });
    }
    entries
}

#[must_use]
pub fn sibling_entries(candidates: &[Candidate], anchor: WindowId) -> Vec<Entry> {
    let Some(app_id) =
        candidates.iter().find(|candidate| candidate.id == anchor).map(|candidate| candidate.app_id.as_str())
    else {
        return Vec::new();
    };
    window_entries(
        candidates.iter().filter(
            |candidate| {
                if app_id.is_empty() { candidate.id == anchor } else { candidate.app_id == app_id }
            },
        ),
    )
}

fn window_entries<'a>(windows: impl IntoIterator<Item = &'a Candidate>) -> Vec<Entry> {
    windows
        .into_iter()
        .map(|window| Entry {
            id: window.id,
            app_id: window.app_id.clone(),
            title: window.title.clone(),
            label: window_label(window, || fallback_app_name(&window.app_id)),
            windows: vec![window.id],
        })
        .collect()
}

struct Raster {
    key: u64,
    panel: MemoryRenderBuffer,
    icons: MemoryRenderBuffer,
    highlight: MemoryRenderBuffer,
    label: Option<(usize, MemoryRenderBuffer)>,
}

pub struct Switcher {
    entries: Vec<Entry>,
    mode: Mode,
    trigger: Trigger,
    selected: usize,
    created: Instant,
    reveal: glide_ui::Reveal,
    highlight: glide_ui::Tween,
    swap: glide_ui::Tween,
    closing: bool,
    rasters: HashMap<i32, Raster>,
    first: std::cell::Cell<usize>,
}

const WINDOWS_MIN_WIDTH: i32 = 480;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Geometry {
    pub panel: Rectangle<i32, Logical>,
    pub cell: i32,
    pub columns: usize,
    pub first: usize,
    strip_x: i32,
}

impl Geometry {
    #[must_use]
    pub fn new(count: usize, area: Rectangle<i32, Logical>) -> Self {
        Self::with_min_width(count, area, 0)
    }

    #[must_use]
    pub fn with_min_width(count: usize, area: Rectangle<i32, Logical>, min_width: i32) -> Self {
        let room = (area.size.w - EDGE_MARGIN * 2 - PAD * 2).max(MIN_CELL);
        let fit = usize::try_from(room / MIN_CELL).unwrap_or(1).max(1);
        let columns = count.clamp(1, fit);
        let visible = i32::try_from(columns).unwrap_or(1);
        let cell = (room / visible).clamp(MIN_CELL, CELL);
        let strip = cell * visible;
        let width = (PAD * 2 + strip).max(min_width.min(room + PAD * 2));
        let height = PAD + cell + LABEL_H + PAD / 2;
        let x = area.loc.x + (area.size.w - width) / 2;
        let y = area.loc.y + (area.size.h - height) / 2;
        Self {
            panel: Rectangle::new(Point::from((x, y)), Size::from((width, height))),
            cell,
            columns,
            first: 0,
            strip_x: (width - strip) / 2,
        }
    }

    #[must_use]
    pub fn scrolled_to(self, count: usize, first: usize, selected: usize) -> Self {
        let max_first = count.saturating_sub(self.columns);
        let mut first = first.min(max_first);
        if selected < first {
            first = selected;
        } else if selected >= first + self.columns {
            first = selected + 1 - self.columns;
        }
        Self { first: first.min(max_first), ..self }
    }

    #[must_use]
    pub fn visible(&self, count: usize) -> std::ops::Range<usize> {
        self.first.min(count)..(self.first + self.columns).min(count)
    }

    #[must_use]
    pub fn icon_side(&self) -> i32 {
        (f64::from(self.cell) * ICON_RATIO).round() as i32
    }

    #[must_use]
    pub fn cell_origin(&self, index: f64) -> Point<f64, Logical> {
        let (first, last) = (self.first as f64, (self.first + self.columns.saturating_sub(1)) as f64);
        let slot = index.clamp(first, last) - first;
        Point::from((f64::from(self.strip_x) + f64::from(self.cell) * slot, f64::from(PAD)))
    }

    #[must_use]
    pub fn cell_at(&self, count: usize, pos: Point<f64, Logical>) -> Option<usize> {
        let local_x = pos.x - f64::from(self.panel.loc.x + self.strip_x);
        let local_y = pos.y - f64::from(self.panel.loc.y + PAD);
        if local_x < 0.0 || local_y < 0.0 || local_y >= f64::from(self.cell) {
            return None;
        }
        let slot = (local_x / f64::from(self.cell)).floor() as usize;
        let index = self.first + slot;
        (slot < self.columns && index < count).then_some(index)
    }
}

impl Switcher {
    #[must_use]
    pub fn new(entries: Vec<Entry>, selected: usize, mode: Mode, trigger: Trigger) -> Self {
        let selected = selected.min(entries.len().saturating_sub(1));
        let highlight = glide_ui::Tween::at(selected as f64);
        Self {
            entries,
            mode,
            trigger,
            selected,
            created: Instant::now(),
            reveal: glide_ui::Reveal::closed(),
            highlight,
            swap: glide_ui::Tween::at(1.0),
            closing: false,
            rasters: HashMap::new(),
            first: std::cell::Cell::new(0),
        }
    }

    #[must_use]
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    #[must_use]
    pub const fn selected(&self) -> usize {
        self.selected
    }

    #[must_use]
    pub fn selected_id(&self) -> Option<WindowId> {
        self.entries.get(self.selected).map(|entry| entry.id)
    }

    #[must_use]
    pub fn selected_entry(&self) -> Option<&Entry> {
        self.entries.get(self.selected)
    }

    #[must_use]
    pub const fn mode(&self) -> Mode {
        self.mode
    }

    #[must_use]
    pub const fn trigger(&self) -> Trigger {
        self.trigger
    }

    pub fn replace(&mut self, entries: Vec<Entry>, selected: usize, mode: Mode) {
        self.selected = selected.min(entries.len().saturating_sub(1));
        self.entries = entries;
        self.mode = mode;
        self.highlight.jump(self.selected as f64);
        self.rasters.clear();
        self.begin_swap();
    }

    fn begin_swap(&mut self) {
        self.swap = glide_ui::Tween::at(0.35);
        self.swap.retarget(1.0, glide_theme::motion::t2(), glide_theme::motion::ease_out);
    }

    pub fn relabel_app(&mut self, app_id: &str, name: &str) -> bool {
        if self.mode != Mode::Apps {
            return false;
        }
        let mut changed = false;
        for entry in &mut self.entries {
            if entry.app_id == app_id && entry.label != name {
                entry.label = name.to_string();
                changed = true;
            }
        }
        if changed {
            self.rasters.clear();
        }
        changed
    }

    pub fn remove_selected(&mut self) -> Option<Entry> {
        if self.selected >= self.entries.len() {
            return None;
        }
        let entry = self.entries.remove(self.selected);
        self.clamp_after_removal(self.selected);
        Some(entry)
    }

    fn clamp_after_removal(&mut self, index: usize) {
        if index < self.selected || self.selected >= self.entries.len() {
            self.selected = self.selected.saturating_sub(1);
        }
        self.highlight.jump(self.selected as f64);
        self.rasters.clear();
        self.begin_swap();
    }

    #[must_use]
    pub const fn is_closing(&self) -> bool {
        self.closing
    }

    pub fn step(&mut self, delta: i32) -> bool {
        let Ok(len) = i32::try_from(self.entries.len()) else {
            return false;
        };
        if len == 0 {
            return false;
        }
        let current = i32::try_from(self.selected).unwrap_or(0);
        let next = (current + delta).rem_euclid(len);
        self.select(usize::try_from(next).unwrap_or(0))
    }

    pub fn select(&mut self, index: usize) -> bool {
        if index >= self.entries.len() || index == self.selected {
            return false;
        }
        self.selected = index;
        let to = index as f64;
        self.highlight.retarget(to, glide_theme::motion::t2(), |t| glide_theme::motion::E_OUT.ease_f64(t));
        true
    }

    pub fn remove(&mut self, id: WindowId) -> bool {
        let Some(index) = self.entries.iter().position(|entry| entry.windows.contains(&id)) else {
            return false;
        };
        let entry = &mut self.entries[index];
        entry.windows.retain(|window| *window != id);
        if let Some(&next) = entry.windows.first() {
            entry.id = next;
            self.rasters.clear();
            return true;
        }
        self.entries.remove(index);
        self.clamp_after_removal(index);
        true
    }

    pub fn begin_close(&mut self) {
        self.closing = true;
        self.reveal.close(glide_theme::motion::t2());
    }

    fn show_delay_elapsed(&self) -> bool {
        self.created.elapsed() >= glide_theme::motion::t1()
    }

    pub fn tick(&mut self) -> bool {
        let duration = glide_theme::motion::t2();
        if !self.closing && !self.reveal.is_open() && self.show_delay_elapsed() {
            self.reveal.open(duration);
        }
        self.reveal.settle(duration);
        !self.closing || self.reveal.is_visible(duration)
    }

    #[must_use]
    pub fn is_animating(&self) -> bool {
        let duration = glide_theme::motion::t2();
        (!self.closing && !self.reveal.is_open())
            || self.reveal.is_animating(duration)
            || self.highlight.is_animating(duration)
            || self.swap.is_animating(duration)
    }

    #[must_use]
    pub fn openness(&self) -> f64 {
        self.reveal.progress(glide_theme::motion::t2(), |t| glide_theme::motion::E_OUT.ease_f64(t)).clamp(0.0, 1.0)
    }

    #[must_use]
    pub fn geometry(&self, area: Rectangle<i32, Logical>) -> Geometry {
        let min_width = if self.mode == Mode::Windows { WINDOWS_MIN_WIDTH } else { 0 };
        let count = self.entries.len();
        let geometry =
            Geometry::with_min_width(count, area, min_width).scrolled_to(count, self.first.get(), self.selected);
        self.first.set(geometry.first);
        geometry
    }

    fn raster_key(
        &self, theme: &LauncherTheme, geometry: Geometry, backdrop: Option<&glide_shell::raster::Backdrop<'_>>,
    ) -> u64 {
        let mut revision = glide_theme::Revision::new()
            .u64(theme.revision())
            .u64(crate::paint::painter_generation())
            .u64(backdrop.map_or(0, |source| source.generation))
            .u64(backdrop.map_or(0, |source| source.fractional_scale.to_bits()))
            .u64(u64::from(geometry.panel.loc.x.unsigned_abs()))
            .u64(u64::from(geometry.panel.loc.y.unsigned_abs()))
            .u64(u64::from(geometry.panel.size.w.unsigned_abs()))
            .u64(u64::from(geometry.cell.unsigned_abs()))
            .u64(u64::try_from(geometry.first).unwrap_or(u64::MAX))
            .u64(u64::try_from(geometry.columns).unwrap_or(u64::MAX))
            .u64(u64::from(self.mode == Mode::Windows));
        for entry in &self.entries {
            revision = revision.u64(entry.id.as_u64()).u64(u64::from(entry.windows.len() > 1));
        }
        revision.finish()
    }
}

fn draw_panel(
    theme: &LauncherTheme, geometry: Geometry, painter: &crate::paint::Painter, scale: i32,
    backdrop: Option<&glide_shell::raster::Backdrop<'_>>,
) -> Option<MemoryRenderBuffer> {
    let (w, h) = (geometry.panel.size.w, geometry.panel.size.h);
    let radius = glide_theme::shape::R3;
    paint((w, h), scale, painter, |sheet| {
        if backdrop.is_some_and(|source| source.blit(&mut sheet.buf, w, h, radius, geometry.panel, scale)) {
            sheet.fill(rect(0, 0, w, h), theme.background, radius);
        } else {
            let fill = theme.background.with_alpha(0.96);
            sheet.panel(fill, radius);
        }
        let border = theme.menu.border;
        sheet.stroke(rect(0, 0, w, h), 1.0, radius, border);
    })
}

fn draw_highlight(
    theme: &LauncherTheme, geometry: Geometry, painter: &crate::paint::Painter, scale: i32,
) -> Option<MemoryRenderBuffer> {
    let side = geometry.cell;
    paint((side, side), scale, painter, |sheet| {
        let (fill, radius) = (theme.fallback_background, glide_theme::shape::R3);
        sheet.panel(fill, radius);
    })
}

fn draw_icons(
    entries: &[Entry], theme: &LauncherTheme, geometry: Geometry, icons: &mut crate::icon::IconLoader,
    painter: &crate::paint::Painter, scale: i32,
) -> Option<MemoryRenderBuffer> {
    let (w, h) = (geometry.panel.size.w, geometry.panel.size.h);
    let side = geometry.icon_side();
    let inset = (geometry.cell - side) / 2;
    let visible = geometry.visible(entries.len());
    paint((w, h), scale, painter, |sheet| {
        for (index, entry) in entries.iter().enumerate().skip(visible.start).take(visible.len()) {
            let origin = geometry.cell_origin(index as f64);
            let x = origin.x.round() as i32 + inset;
            let y = PAD + inset;
            if entry.windows.len() > 1 {
                let dot = rect(x + (side - DOT) / 2, y + side + (inset - DOT) / 2, DOT, DOT);
                sheet.fill(dot, theme.label_active, DOT_RADIUS);
            }
            if !entry.app_id.is_empty()
                && let Some(pixels) = icons.load(&entry.app_id, side * scale)
            {
                sheet.blit_square(pixels, side, (x, y));
                continue;
            }
            let tile = rect(x, y, side, side);
            sheet.fill(tile, theme.fallback_background, glide_theme::shape::R3);
            let initial: String = entry
                .app_id
                .chars()
                .chain(entry.title.chars())
                .find(|c| c.is_alphanumeric())
                .map(|c| c.to_uppercase().collect())
                .unwrap_or_default();
            let style = &glide_theme::TextStyle::FALLBACK_INITIAL;
            sheet.text(tile, &initial, theme.fallback_label, style);
        }
    })
}

fn draw_label(
    entry: &Entry, theme: &LauncherTheme, geometry: Geometry, painter: &crate::paint::Painter, scale: i32,
) -> Option<MemoryRenderBuffer> {
    let w = geometry.panel.size.w;
    let text = if entry.label.trim().is_empty() { entry.app_id.as_str() } else { entry.label.trim() };
    paint((w, LABEL_H), scale, painter, |sheet| {
        let band = rect(PAD, 0, w - PAD * 2, LABEL_H);
        sheet.text(band, text, theme.label_active, &glide_theme::TextStyle::HEAD);
    })
}

#[allow(clippy::too_many_arguments)]
pub fn render<R>(
    switcher: &mut Switcher, renderer: &mut R, icons: &mut crate::icon::IconLoader, painter: &crate::paint::Painter,
    theme: &LauncherTheme, area: Rectangle<i32, Logical>, scale: Scale<f64>, output_scale: i32,
    backdrop: Option<&glide_shell::raster::Backdrop<'_>>,
) -> Vec<SwitcherRenderElement<R>>
where
    R: crate::raster::ShellRenderer,
{
    let openness = switcher.openness();
    if openness <= 0.0 || switcher.entries.is_empty() {
        return Vec::new();
    }
    let geometry = switcher.geometry(area);
    icons.expire_missing();
    let key = glide_theme::Revision::new()
        .u64(switcher.raster_key(theme, geometry, backdrop))
        .u64(icons.generation())
        .finish();

    if switcher.rasters.get(&output_scale).is_none_or(|raster| raster.key != key) {
        switcher.rasters.remove(&output_scale);
        let panel = draw_panel(theme, geometry, painter, output_scale, backdrop);
        let highlight = draw_highlight(theme, geometry, painter, output_scale);
        let icon_row = draw_icons(&switcher.entries, theme, geometry, icons, painter, output_scale);
        if let (Some(panel), Some(highlight), Some(icons)) = (panel, highlight, icon_row) {
            switcher.rasters.insert(output_scale, Raster { key, panel, icons, highlight, label: None });
        }
    }

    let selected = switcher.selected;
    let Some(entry) = switcher.entries.get(selected).cloned() else {
        return Vec::new();
    };
    let Some(raster) = switcher.rasters.get_mut(&output_scale) else {
        return Vec::new();
    };
    if raster.label.as_ref().is_none_or(|(index, _)| *index != selected) {
        raster.label = draw_label(&entry, theme, geometry, painter, output_scale).map(|buf| (selected, buf));
    }

    let highlight_at = switcher.highlight.value(glide_theme::motion::t2(), |t| glide_theme::motion::E_OUT.ease_f64(t));
    let panel_loc = geometry.panel.loc.to_f64();
    let cell_loc = panel_loc + geometry.cell_origin(highlight_at);
    let label_loc = panel_loc + Point::from((0.0, f64::from(PAD + geometry.cell)));

    let alpha = openness as f32;
    let grow = OPEN_SCALE + (1.0 - OPEN_SCALE) * openness;
    let centre = Point::<i32, Logical>::from((
        geometry.panel.loc.x + geometry.panel.size.w / 2,
        geometry.panel.loc.y + geometry.panel.size.h / 2,
    ))
    .to_f64()
    .to_physical(scale)
    .to_i32_round();

    let settled = switcher.swap.value(glide_theme::motion::t2(), glide_theme::motion::ease_out) as f32;
    let label = raster.label.as_ref().map(|(_, label)| (label, label_loc, settled));
    label
        .into_iter()
        .chain([
            (&raster.icons, panel_loc, settled),
            (&raster.highlight, cell_loc, settled),
            (&raster.panel, panel_loc, 1.0),
        ])
        .filter_map(|(buffer, loc, fade)| {
            MemoryRenderBufferRenderElement::from_buffer(
                renderer,
                loc.to_physical(scale),
                buffer,
                Some(alpha * fade),
                None,
                None,
                Kind::Unspecified,
            )
            .ok()
        })
        .map(|element| RescaleRenderElement::from_element(element, centre, grow))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(title: &str) -> Entry {
        let id = WindowId::next();
        Entry {
            id,
            app_id: title.to_lowercase(),
            title: title.to_string(),
            label: title.to_string(),
            windows: vec![id],
        }
    }

    fn candidate(app_id: &str, title: &str) -> Candidate {
        Candidate { id: WindowId::next(), app_id: app_id.to_string(), title: title.to_string() }
    }

    const ALT: Trigger = Trigger { alt: true, logo: false, ctrl: false };

    fn switcher(entries: Vec<Entry>, selected: usize) -> Switcher {
        Switcher::new(entries, selected, Mode::Apps, ALT)
    }

    fn area() -> Rectangle<i32, Logical> {
        Rectangle::from_size(Size::from((1920, 1080)))
    }

    #[test]
    fn apps_are_ordered_by_their_most_recent_window() {
        let windows = [
            candidate("firefox", "Mail"),
            candidate("kitty", "shell"),
            candidate("firefox", "Docs"),
            candidate("", "Untitled"),
            candidate("", "Scratch"),
        ];
        let entries = app_entries(&windows, |app_id| (app_id == "firefox").then(|| "Firefox Web Browser".to_string()));
        let apps: Vec<&str> = entries.iter().map(|entry| entry.app_id.as_str()).collect();
        assert_eq!(apps, ["firefox", "kitty", "", ""]);
        assert_eq!(entries[0].windows, vec![windows[0].id, windows[2].id]);
        assert_eq!(entries[0].id, windows[0].id);
        assert_eq!(entries[0].label, "Firefox Web Browser");
        assert_eq!(entries[1].label, "Kitty");
        assert_eq!(entries[3].label, "Scratch", "a window without an app id is its own entry, named by its title");
    }

    #[test]
    fn removing_an_entry_fades_the_strip_back_in_instead_of_snapping() {
        let mut switcher = switcher(vec![entry("A"), entry("B")], 0);
        let duration = glide_theme::motion::t2();
        assert!(!switcher.swap.is_animating(duration));
        assert!(switcher.remove_selected().is_some());
        if !glide_theme::motion::is_disabled() {
            assert!(switcher.swap.is_animating(duration));
            assert!(switcher.swap.value(duration, glide_theme::motion::ease_out) < 1.0);
        }
    }

    #[test]
    fn window_entries_keep_mru_order_and_use_titles() {
        let windows = [candidate("kitty", "one"), candidate("kitty", "  ")];
        let entries = window_entries(&windows);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].label, "one");
        assert_eq!(entries[1].label, "Kitty");
    }

    #[test]
    fn window_mode_lists_only_the_anchor_apps_windows() {
        let windows =
            [candidate("kitty", "one"), candidate("firefox", "web"), candidate("kitty", "two"), candidate("", "loose")];
        let ids: Vec<WindowId> = sibling_entries(&windows, windows[2].id).iter().map(|entry| entry.id).collect();
        assert_eq!(ids, vec![windows[0].id, windows[2].id]);
        let loose = sibling_entries(&windows, windows[3].id);
        assert_eq!(loose.len(), 1, "an app-less window has no siblings");
        assert!(sibling_entries(&windows, WindowId::next()).is_empty());
    }

    #[test]
    fn the_fallback_name_is_the_capitalised_last_segment() {
        assert_eq!(fallback_app_name("org.gnome.Nautilus"), "Nautilus");
        assert_eq!(fallback_app_name("kitty"), "Kitty");
        assert_eq!(fallback_app_name(""), "");
    }

    #[test]
    fn the_switcher_commits_once_every_opening_modifier_is_released() {
        let alt = ALT;
        let logo_ctrl = Trigger { logo: true, ctrl: true, ..Trigger::default() };
        assert!(!alt.released(alt));
        assert!(alt.released(Trigger::default()));
        assert!(
            alt.released(Trigger { logo: true, ..Trigger::default() }),
            "an unrelated modifier does not keep an Alt switcher open"
        );
        assert!(!logo_ctrl.released(Trigger { ctrl: true, ..Trigger::default() }));
        assert!(logo_ctrl.released(Trigger::default()));
        assert!(!Trigger::default().released(Trigger::default()), "a modifier-less binding waits for Enter or Esc");
    }

    #[test]
    fn closing_an_app_removes_its_entry_and_clamps_the_selection() {
        let entries = vec![entry("A"), entry("B"), entry("C")];
        let mut switcher = switcher(entries, 2);
        let removed = switcher.remove_selected().map(|entry| entry.label);
        assert_eq!(removed.as_deref(), Some("C"));
        assert_eq!(switcher.selected(), 1);
        assert_eq!(switcher.entries().len(), 2);
        switcher.remove_selected();
        switcher.remove_selected();
        assert!(switcher.entries().is_empty());
        assert!(switcher.remove_selected().is_none());
    }

    #[test]
    fn closing_one_window_of_an_app_keeps_the_app_with_its_next_window() {
        let windows = [candidate("kitty", "a"), candidate("kitty", "b")];
        let entries = app_entries(&windows, |_| None);
        let mut switcher = switcher(entries, 0);
        assert!(switcher.remove(windows[0].id));
        assert_eq!(switcher.entries().len(), 1);
        assert_eq!(switcher.selected_id(), Some(windows[1].id));
        assert!(switcher.remove(windows[1].id));
        assert!(switcher.entries().is_empty());
        assert!(!switcher.remove(windows[1].id));
    }

    #[test]
    fn replacing_the_list_switches_mode_and_clamps_the_selection() {
        let mut switcher = switcher(vec![entry("A"), entry("B")], 1);
        switcher.replace(vec![entry("X")], 1, Mode::Windows);
        assert_eq!(switcher.mode(), Mode::Windows);
        assert_eq!(switcher.selected(), 0);
    }

    #[test]
    fn a_resolved_name_relabels_only_app_entries() {
        let mut switcher = switcher(vec![entry("A")], 0);
        assert!(switcher.relabel_app("a", "Alpha"));
        assert_eq!(switcher.entries()[0].label, "Alpha");
        assert!(!switcher.relabel_app("a", "Alpha"));
    }

    #[test]
    fn switcher_geometry_sits_on_the_4px_grid() {
        for value in [CELL, MIN_CELL, PAD, LABEL_H, EDGE_MARGIN, DOT] {
            assert_eq!(value % 4, 0, "{value} is off the 4px grid");
        }
    }

    #[test]
    fn stepping_wraps_around_both_ends() {
        let mut switcher = switcher(vec![entry("A"), entry("B"), entry("C")], 1);
        assert!(switcher.step(1));
        assert_eq!(switcher.selected(), 2);
        assert!(switcher.step(1));
        assert_eq!(switcher.selected(), 0);
        assert!(switcher.step(-1));
        assert_eq!(switcher.selected(), 2);
    }

    #[test]
    fn a_single_window_cannot_be_stepped_away_from() {
        let mut switcher = switcher(vec![entry("A")], 1);
        assert_eq!(switcher.selected(), 0);
        assert!(!switcher.step(1));
    }

    #[test]
    fn removing_the_selected_window_keeps_the_selection_in_range() {
        let entries = vec![entry("A"), entry("B")];
        let last = entries[1].id;
        let mut switcher = switcher(entries, 1);
        assert!(switcher.remove(last));
        assert_eq!(switcher.selected(), 0);
        assert_eq!(switcher.entries().len(), 1);
    }

    #[test]
    fn a_quick_release_closes_without_ever_showing_the_panel() {
        let mut switcher = switcher(vec![entry("A"), entry("B")], 1);
        switcher.begin_close();
        assert!(!switcher.tick());
    }

    #[test]
    fn the_panel_is_centred_and_fits_the_output() {
        let geometry = Geometry::new(3, area());
        assert_eq!(geometry.cell, CELL);
        assert_eq!(geometry.panel.loc.x * 2 + geometry.panel.size.w, area().size.w);

        let crowded = Geometry::new(200, area());
        assert!(crowded.cell >= MIN_CELL);
        assert!(crowded.columns < 200);
        let fitting = Geometry::new(20, area());
        assert!(fitting.panel.size.w <= area().size.w - EDGE_MARGIN * 2);
    }

    fn laptop() -> Rectangle<i32, Logical> {
        Rectangle::from_size(Size::from((1366, 768)))
    }

    #[test]
    fn a_crowded_strip_stays_on_screen_and_scrolls_to_the_selection() {
        let count = 60;
        let geometry = Geometry::new(count, laptop());
        assert!(geometry.panel.loc.x >= 0);
        assert!(geometry.panel.loc.x + geometry.panel.size.w <= laptop().size.w);
        assert!(geometry.columns < count);

        let at_end = geometry.scrolled_to(count, 0, count - 1);
        assert_eq!(at_end.visible(count).end, count);
        let last = at_end.panel.loc.to_f64() + at_end.cell_origin(59.0) + Point::from((4.0, 4.0));
        assert_eq!(at_end.cell_at(count, last), Some(count - 1));
        assert!(last.x < f64::from(at_end.panel.loc.x + at_end.panel.size.w));

        let stepped_back = at_end.scrolled_to(count, at_end.first, count - 2);
        assert_eq!(stepped_back.first, at_end.first, "no scroll while in view");
        let wrapped = at_end.scrolled_to(count, at_end.first, 0);
        assert_eq!(wrapped.first, 0);
    }

    #[test]
    fn hovering_the_edge_cell_does_not_scroll_the_strip() {
        let count = 60;
        let mut switcher = switcher((0..count).map(|i| entry(&format!("E{i}"))).collect(), 0);
        let geometry = switcher.geometry(laptop());
        let edge = geometry.columns - 1;
        let pos = geometry.panel.loc.to_f64()
            + geometry.cell_origin(f64::from(u32::try_from(edge).expect("small")))
            + Point::from((4.0, 4.0));
        assert_eq!(geometry.cell_at(count, pos), Some(edge));
        switcher.select(edge);
        let after = switcher.geometry(laptop());
        assert_eq!(after.first, geometry.first);
        assert_eq!(after.cell_at(count, pos), Some(edge));
    }

    #[test]
    fn a_short_window_list_leaves_room_for_the_title() {
        let entries = vec![entry("Only")];
        let windows = Switcher::new(entries.clone(), 0, Mode::Windows, ALT);
        let apps = Switcher::new(entries, 0, Mode::Apps, ALT);
        let wide = windows.geometry(area());
        assert_eq!(wide.panel.size.w, WINDOWS_MIN_WIDTH);
        assert!(apps.geometry(area()).panel.size.w < WINDOWS_MIN_WIDTH);
        let centre = wide.panel.loc.to_f64() + wide.cell_origin(0.0) + Point::from((f64::from(wide.cell) / 2.0, 4.0));
        assert_eq!(wide.cell_at(1, centre), Some(0));
        assert!((centre.x - f64::from(wide.panel.loc.x + wide.panel.size.w / 2)).abs() <= 1.0);
    }

    #[test]
    fn a_pointer_over_a_cell_maps_to_its_index() {
        let geometry = Geometry::new(3, area());
        let origin = geometry.panel.loc.to_f64();
        let inside_second = origin + Point::from((f64::from(PAD + geometry.cell) + 4.0, f64::from(PAD) + 4.0));
        assert_eq!(geometry.cell_at(3, inside_second), Some(1));
        assert_eq!(geometry.cell_at(3, origin), None);
    }
}

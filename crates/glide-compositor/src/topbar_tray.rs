use std::collections::HashMap;
use std::time::Instant;

use glide_appmenu::MenuItem;
use glide_theme::motion::{self, ease_out as ease};
use glide_ui::{Reveal, Tween};
use smithay::{
    backend::renderer::element::{
        Kind,
        memory::{MemoryRenderBuffer, MemoryRenderBufferRenderElement},
    },
    utils::{Logical, Point, Rectangle, Scale, Size},
};

use super::{BUTTON_HEIGHT, HEIGHT, MenuPath, OpenPopup, Slot, Topbar, TopbarTheme};
use crate::{
    config::Color,
    paint::TextAlign,
    raster::{Canvas, ShellRenderer, buffer_from},
};

pub const ICON: i32 = 16;
pub const SLOT_WIDTH: i32 = 24;
pub const MAX_SCALE: i32 = 3;
const TOOLTIP_HEIGHT: i32 = 26;
const TOOLTIP_PAD_X: i32 = glide_theme::spacing::SP3;
const TOOLTIP_GAP: i32 = glide_theme::spacing::SP2;
const TOOLTIP_RADIUS: f32 = glide_theme::shape::R2;
const EDGE_MARGIN: i32 = glide_theme::spacing::SP2;
const OPEN_RADIUS: f32 = glide_theme::shape::R2;

const BTN_RIGHT: u32 = 0x111;
const BTN_MIDDLE: u32 = 0x112;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IconArt {
    pub symbolic: bool,
    pub rasters: Vec<(i32, Vec<u8>)>,
}

impl IconArt {
    fn best_for(&self, scale: i32) -> Option<(i32, &[u8])> {
        self.rasters
            .iter()
            .find(|(at, _)| *at == scale)
            .or_else(|| self.rasters.iter().max_by_key(|(at, _)| *at))
            .map(|(at, pixels)| (*at, pixels.as_slice()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayIcon {
    pub key: String,
    pub bus: String,
    pub label: String,
    pub menu: Option<String>,
    pub item_is_menu: bool,
    pub status: glide_tray::Status,
    pub art: IconArt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrayButton {
    Primary,
    Secondary,
    Context,
}

impl TrayButton {
    #[must_use]
    pub const fn from_code(code: u32) -> Self {
        match code {
            BTN_RIGHT => Self::Context,
            BTN_MIDDLE => Self::Secondary,
            _ => Self::Primary,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TrayClick {
    pub id: u16,
    pub button: TrayButton,
}

#[derive(Debug, Clone)]
struct Entry {
    id: u16,
    icon: TrayIcon,
    presence: Tween,
    gone: bool,
    stamp: u64,
}

fn presence_of(tween: &Tween) -> f64 {
    tween.value(motion::t2(), ease).clamp(0.0, 1.0)
}

fn aim(tween: &mut Tween, shown: bool) -> bool {
    tween.retarget(f64::from(u8::from(shown)), motion::t2(), ease)
}

fn retired(entry: &Entry) -> bool {
    entry.gone && entry.presence.target() <= 0.0 && !entry.presence.is_animating(motion::t2())
}

#[must_use]
pub fn slot_width(presence: f64) -> i32 {
    (f64::from(SLOT_WIDTH) * presence.clamp(0.0, 1.0)).round() as i32
}

#[must_use]
pub fn place(entries: &[(u16, f64)], right: i32, limit: i32, y: i32) -> Vec<(u16, Rectangle<i32, Logical>)> {
    let mut x = right;
    let mut out = Vec::new();
    for &(id, presence) in entries {
        let width = slot_width(presence);
        if width <= 0 {
            continue;
        }
        if x - width < limit {
            break;
        }
        x -= width;
        out.push((id, Rectangle::new(Point::from((x, y)), Size::from((width, BUTTON_HEIGHT)))));
    }
    out
}

#[derive(Debug, Default)]
pub struct TrayStrip {
    enabled: bool,
    entries: Vec<Entry>,
    next_id: u16,
    stamp: u64,
    moving: bool,
    output: Rectangle<i32, Logical>,
    rects: Vec<(u16, Rectangle<i32, Logical>)>,
    menu: Option<(u16, Vec<MenuItem>)>,
    hover: Option<u16>,
    hover_since: Option<Instant>,
    tooltip: Option<(u16, String)>,
    tooltip_reveal: Reveal,
    rasters: HashMap<(u16, i32), (u64, u64, MemoryRenderBuffer)>,
    tooltips: HashMap<i32, (String, u64, MemoryRenderBuffer)>,
    shadows: HashMap<(i32, i32, bool), MemoryRenderBuffer>,
    open_fill: HashMap<i32, (u64, MemoryRenderBuffer)>,
}

impl TrayStrip {
    fn allocate_id(&mut self) -> u16 {
        loop {
            let id = self.next_id;
            self.next_id = self.next_id.wrapping_add(1);
            if !self.entries.iter().any(|entry| entry.id == id) {
                return id;
            }
        }
    }

    fn upsert(&mut self, icon: TrayIcon) -> bool {
        let shown = icon.status.visible();
        self.stamp = self.stamp.wrapping_add(1);
        let stamp = self.stamp;
        if let Some(entry) = self.entries.iter_mut().find(|e| e.icon.key == icon.key) {
            if entry.icon == icon && !entry.gone {
                return false;
            }
            if entry.icon.art != icon.art {
                entry.stamp = stamp;
            }
            entry.icon = icon;
            entry.gone = false;
            aim(&mut entry.presence, shown);
            return true;
        }
        let id = self.allocate_id();
        let mut presence = Tween::at(0.0);
        aim(&mut presence, shown);
        self.entries.push(Entry { id, icon, presence, gone: false, stamp });
        true
    }

    fn remove(&mut self, key: &str) -> Option<u16> {
        let entry = self.entries.iter_mut().find(|e| e.icon.key == key)?;
        entry.gone = true;
        aim(&mut entry.presence, false);
        Some(entry.id)
    }

    fn presences(&self) -> Vec<(u16, f64)> {
        self.entries.iter().map(|entry| (entry.id, presence_of(&entry.presence))).collect()
    }

    fn entry(&self, id: u16) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.id == id)
    }

    fn settle(&mut self, popup_open: bool) -> bool {
        let duration = motion::t2();
        let moving = self.entries.iter().any(|entry| entry.presence.is_animating(duration));
        let before = self.entries.len();
        self.entries.retain(|entry| !retired(entry));
        let retired_any = self.entries.len() != before;
        if retired_any {
            let live: Vec<u16> = self.entries.iter().map(|entry| entry.id).collect();
            self.rasters.retain(|(id, _), _| live.contains(id));
        }
        self.sync_tooltip(popup_open);
        let relayout = moving || self.moving || retired_any;
        self.moving = moving;
        relayout
    }

    fn tooltip_target(&self, popup_open: bool) -> Option<(u16, String)> {
        if popup_open {
            return None;
        }
        let since = self.hover_since?;
        if since.elapsed() < crate::dock::TOOLTIP_DELAY {
            return None;
        }
        let id = self.hover?;
        let label = self.entry(id)?.icon.label.trim().to_string();
        (!label.is_empty()).then_some((id, label))
    }

    fn sync_tooltip(&mut self, popup_open: bool) {
        let duration = motion::t1();
        match self.tooltip_target(popup_open) {
            Some(target) => {
                self.tooltip = Some(target);
                self.tooltip_reveal.open(duration);
            }
            None => {
                self.tooltip_reveal.close(duration);
            }
        }
        self.tooltip_reveal.settle(duration);
        if !self.tooltip_reveal.is_open() && !self.tooltip_reveal.is_visible(duration) {
            self.tooltip = None;
        }
    }

    fn hover(&mut self, slot: Option<Slot>) {
        let id = match slot {
            Some(Slot::Tray(id)) => Some(id),
            _ => None,
        };
        if id != self.hover {
            self.hover = id;
            self.hover_since = id.map(|_| Instant::now());
        }
    }

    fn is_animating(&self) -> bool {
        if !self.enabled {
            return false;
        }
        let duration = motion::t2();
        let pending = self.hover_since.is_some_and(|since| since.elapsed() < crate::dock::TOOLTIP_DELAY);
        self.moving
            || pending
            || self.tooltip_reveal.is_animating(motion::t1())
            || self.entries.iter().any(|entry| entry.presence.is_animating(duration))
    }

    fn layout(&mut self, right: i32, limit: i32, y: i32) -> &[(u16, Rectangle<i32, Logical>)] {
        self.rects = if self.enabled { place(&self.presences(), right, limit, y) } else { Vec::new() };
        &self.rects
    }

    fn rect_of(&self, id: u16) -> Option<Rectangle<i32, Logical>> {
        self.rects.iter().find(|(candidate, _)| *candidate == id).map(|(_, rect)| *rect)
    }
}

impl Topbar {
    pub fn set_tray_enabled(&mut self, on: bool) -> bool {
        if self.tray.enabled == on {
            return false;
        }
        self.tray.enabled = on;
        if !on && self.tray.menu.is_some() && self.open_popup == Some(OpenPopup::AppMenu) {
            self.close_popup();
        }
        self.touch();
        true
    }

    pub fn upsert_tray_item(&mut self, icon: TrayIcon) -> bool {
        let changed = self.tray.upsert(icon);
        if changed {
            self.touch();
        }
        changed
    }

    pub fn remove_tray_item(&mut self, key: &str) -> Option<u16> {
        let id = self.tray.remove(key)?;
        if self.tray_menu_showing(id) {
            self.close_popup();
        }
        self.touch();
        Some(id)
    }

    #[must_use]
    pub fn tray_item(&self, id: u16) -> Option<&TrayIcon> {
        self.tray.entry(id).filter(|entry| !entry.gone).map(|entry| &entry.icon)
    }

    #[must_use]
    pub fn tray_menu_id(&self) -> Option<u16> {
        self.tray.menu.as_ref().map(|(id, _)| *id)
    }

    #[must_use]
    pub fn tray_menu_showing(&self, id: u16) -> bool {
        self.open_popup == Some(OpenPopup::AppMenu) && self.tray_menu_id() == Some(id)
    }

    pub const fn suppress_tray_tooltip(&mut self) {
        self.tray.hover_since = None;
    }

    pub fn open_tray_menu(&mut self, id: u16, items: Vec<MenuItem>) -> bool {
        if items.is_empty() || self.tray_item(id).is_none() {
            return false;
        }
        self.tray.menu = Some((id, vec![MenuItem::submenu("", items)]));
        self.menu_rasters.clear();
        self.menu_open = MenuPath::top(0);
        self.suppress_tray_tooltip();
        if self.open_popup == Some(OpenPopup::AppMenu) {
            self.set_focused(None);
            self.touch();
            return true;
        }
        self.toggle_popup(OpenPopup::AppMenu)
    }

    pub fn set_tray_menu_items(&mut self, id: u16, items: Vec<MenuItem>) -> bool {
        let Some((current, bar)) = self.tray.menu.as_mut() else {
            return false;
        };
        if *current != id {
            return false;
        }
        let next = vec![MenuItem::submenu("", items)];
        if *bar == next {
            return false;
        }
        if next[0].children.is_empty() {
            if self.open_popup == Some(OpenPopup::AppMenu) {
                self.close_popup();
            }
            return true;
        }
        *bar = next;
        self.menu_rasters.clear();
        while !self.menu_open.is_empty()
            && !crate::global_menu::item_at(self.menu_items(), self.menu_open).is_some_and(MenuItem::is_submenu)
        {
            match self.menu_open.parent() {
                Some(parent) => self.menu_open = parent,
                None => break,
            }
        }
        self.touch();
        true
    }

    pub(super) fn release_tray_menu(&mut self) {
        if self.tray.menu.take().is_some() {
            self.menu_rasters.clear();
        }
    }

    pub(super) fn menu_items(&self) -> &[MenuItem] {
        self.tray.menu.as_ref().map_or(self.menu_bar.as_slice(), |(_, bar)| bar.as_slice())
    }

    pub(super) fn menu_anchor(&self, titles: &[(usize, Rectangle<i32, Logical>)]) -> Option<Rectangle<i32, Logical>> {
        match &self.tray.menu {
            Some((id, _)) => self.tray.rect_of(*id),
            None => titles.iter().find(|(index, _)| *index == self.menu_open.top_index()).map(|(_, title)| *title),
        }
    }

    pub(super) fn tray_hover(&mut self, slot: Option<Slot>) {
        self.tray.hover(slot);
    }

    pub(super) fn tray_settle(&mut self) {
        let popup_open = self.open_popup.is_some();
        if self.tray.settle(popup_open) {
            self.touch();
        }
    }

    pub(super) fn tray_is_animating(&self) -> bool {
        self.tray.is_animating()
    }

    pub(super) fn layout_tray(
        &mut self, output: Rectangle<i32, Logical>, (right, limit, y): (i32, i32, i32),
        slots: &mut Vec<(Slot, Rectangle<i32, Logical>)>,
    ) {
        self.tray.output = output;
        let placed = self.tray.layout(right, limit, y);
        slots.extend(placed.iter().map(|(id, rect)| (Slot::Tray(*id), *rect)));
    }
}

fn tint(mask: &[u8], color: Color) -> Vec<u8> {
    let mut out = Vec::with_capacity(mask.len());
    for pixel in mask.chunks_exact(4) {
        let alpha = f32::from(pixel[3]) / 255.0 * color.0[3];
        let channel = |value: f32| (value * alpha * 255.0).round().clamp(0.0, 255.0) as u8;
        out.extend_from_slice(&[channel(color.0[2]), channel(color.0[1]), channel(color.0[0]), channel(1.0)]);
    }
    out
}

fn icon_pixels(art: &IconArt, scale: i32, color: Color) -> Option<Vec<u8>> {
    let side = ICON * scale;
    let (at, pixels) = art.best_for(scale)?;
    let pixels = if at == scale {
        pixels.to_vec()
    } else {
        let source = ICON * at;
        glide_tray::fit_square(pixels, source, source, side)?
    };
    Some(if art.symbolic { tint(&pixels, color) } else { pixels })
}

fn draw_icon(painter: &crate::paint::Painter, icon: &TrayIcon, color: Color, scale: i32) -> Option<MemoryRenderBuffer> {
    if let Some(pixels) = icon_pixels(&icon.art, scale, color) {
        return Some(buffer_from(&pixels, ICON, ICON, scale));
    }
    let initial: String = icon
        .label
        .chars()
        .find(|c| c.is_alphanumeric())
        .map_or_else(|| String::from("•"), |c| c.to_uppercase().collect());
    let mut c = Canvas::new(ICON, ICON, scale)?;
    c.stroke(c.full(), 1.0, glide_theme::shape::R1, color);
    c.label(painter, (0, ICON), TextAlign::Center, &initial, color, &glide_theme::TextStyle::CAP);
    Some(c.finish())
}

fn tooltip_width(painter: &crate::paint::Painter, label: &str) -> i32 {
    let text = painter.text_width_styled(label, &glide_theme::TextStyle::CAP).ceil() as i32;
    text + TOOLTIP_PAD_X * 2
}

fn draw_tooltip(
    painter: &crate::paint::Painter, theme: &TopbarTheme, label: &str, width: i32, scale: i32,
) -> Option<MemoryRenderBuffer> {
    let mut c = Canvas::popover((width, TOOLTIP_HEIGHT), scale, TOOLTIP_RADIUS, theme.popover_bg, theme.border)?;
    let style = &glide_theme::TextStyle::CAP;
    c.label(painter, (TOOLTIP_PAD_X, width - TOOLTIP_PAD_X), TextAlign::Center, label, theme.label, style);
    Some(c.finish())
}

fn draw_open_fill(theme: &TopbarTheme, scale: i32) -> Option<MemoryRenderBuffer> {
    let mut c = Canvas::new(SLOT_WIDTH, BUTTON_HEIGHT, scale)?;
    c.fill(c.full(), theme.open_fill, OPEN_RADIUS);
    Some(c.finish())
}

fn push_alpha<R>(
    renderer: &mut R, buffer: &MemoryRenderBuffer, position: Point<i32, Logical>, alpha: f32, scale: Scale<f64>,
    elements: &mut Vec<MemoryRenderBufferRenderElement<R>>,
) where
    R: ShellRenderer,
{
    if alpha <= 0.0 {
        return;
    }
    if let Ok(element) = MemoryRenderBufferRenderElement::from_buffer(
        renderer,
        position.to_f64().to_physical(scale),
        buffer,
        Some(alpha.min(1.0)),
        None,
        None,
        Kind::Unspecified,
    ) {
        elements.push(element);
    }
}

fn push_tooltip<R>(
    topbar: &mut Topbar, renderer: &mut R, painter: &crate::paint::Painter, scale: Scale<f64>, output_scale: i32,
    elements: &mut Vec<MemoryRenderBufferRenderElement<R>>,
) where
    R: ShellRenderer,
{
    let output = topbar.tray.output;
    let Some((id, label)) = topbar.tray.tooltip.clone() else {
        return;
    };
    let Some(anchor) = topbar.tray.rect_of(id) else {
        return;
    };
    let alpha = topbar.tray.tooltip_reveal.progress(motion::t1(), ease) as f32;
    if alpha <= 0.0 {
        return;
    }
    let width = tooltip_width(painter, &label);
    let stamp =
        glide_theme::Revision::new().u64(topbar.theme.revision()).u64(crate::paint::painter_generation()).finish();
    let fresh = topbar.tray.tooltips.get(&output_scale).is_some_and(|(cached, at, _)| *cached == label && *at == stamp);
    if !fresh && let Some(buffer) = draw_tooltip(painter, &topbar.theme, &label, width, output_scale) {
        topbar.tray.tooltips.insert(output_scale, (label.clone(), stamp, buffer));
    }
    let left = output.loc.x + EDGE_MARGIN;
    let right = (output.loc.x + output.size.w - EDGE_MARGIN - width).max(left);
    let x = (anchor.loc.x + (anchor.size.w - width) / 2).clamp(left, right);
    let position = Point::from((x, output.loc.y + HEIGHT + TOOLTIP_GAP));
    if let Some((_, _, buffer)) = topbar.tray.tooltips.get(&output_scale) {
        push_alpha(renderer, buffer, position, alpha, scale, elements);
    }

    let dark = topbar.theme.dark;
    let shadow_key = (width, output_scale, dark);
    if let std::collections::hash_map::Entry::Vacant(entry) = topbar.tray.shadows.entry(shadow_key)
        && let Some(buffer) = crate::popup_menu::draw_elevation(
            Size::from((width, TOOLTIP_HEIGHT)),
            TOOLTIP_RADIUS,
            super::scale_or_one(output_scale),
            glide_theme::elevation::Level::El2,
            if dark { glide_theme::Appearance::Dark } else { glide_theme::Appearance::Light },
        )
    {
        entry.insert(buffer);
    }
    if let Some(buffer) = topbar.tray.shadows.get(&shadow_key) {
        let margin = crate::popup_menu::shadow_margin();
        let at = Point::from((position.x - margin, position.y - margin));
        push_alpha(renderer, buffer, at, alpha, scale, elements);
    }
}

pub(super) fn push_tray<R>(
    topbar: &mut Topbar, renderer: &mut R, painter: &crate::paint::Painter, scale: Scale<f64>, output_scale: i32,
    elements: &mut Vec<MemoryRenderBufferRenderElement<R>>,
) where
    R: ShellRenderer,
{
    if !topbar.tray.enabled {
        return;
    }
    push_tooltip(topbar, renderer, painter, scale, output_scale, elements);
    let output_scale = super::scale_or_one(output_scale);
    let theme_rev =
        glide_theme::Revision::new().u64(topbar.theme.revision()).u64(crate::paint::painter_generation()).finish();
    let open = topbar.tray_menu_id().filter(|_| topbar.shown_popup() == Some(OpenPopup::AppMenu));
    let color = topbar.theme.label;
    for (id, rect) in topbar.tray.rects.clone() {
        let Some(entry) = topbar.tray.entry(id) else {
            continue;
        };
        let alpha = presence_of(&entry.presence) as f32;
        let stamp = entry.stamp;
        let key = (id, output_scale);
        let fresh = topbar.tray.rasters.get(&key).is_some_and(|(at, rev, _)| *at == stamp && *rev == theme_rev);
        if !fresh && let Some(buffer) = draw_icon(painter, &entry.icon, color, output_scale) {
            topbar.tray.rasters.insert(key, (stamp, theme_rev, buffer));
        }
        if let Some((_, _, buffer)) = topbar.tray.rasters.get(&key) {
            let position = Point::from((rect.loc.x + (rect.size.w - ICON) / 2, rect.loc.y + (rect.size.h - ICON) / 2));
            push_alpha(renderer, buffer, position, alpha, scale, elements);
        }
        if open == Some(id) {
            let fresh = topbar.tray.open_fill.get(&output_scale).is_some_and(|(rev, _)| *rev == theme_rev);
            if !fresh && let Some(buffer) = draw_open_fill(&topbar.theme, output_scale) {
                topbar.tray.open_fill.insert(output_scale, (theme_rev, buffer));
            }
            if let Some((_, buffer)) = topbar.tray.open_fill.get(&output_scale) {
                let position = Point::from((rect.loc.x + (rect.size.w - SLOT_WIDTH) / 2, rect.loc.y));
                push_alpha(renderer, buffer, position, alpha, scale, elements);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn icon(key: &str, status: glide_tray::Status) -> TrayIcon {
        TrayIcon {
            key: key.into(),
            bus: ":1.1".into(),
            label: key.into(),
            menu: None,
            item_is_menu: false,
            status,
            art: IconArt::default(),
        }
    }

    #[test]
    fn a_slot_grows_with_its_presence() {
        assert_eq!(slot_width(0.0), 0);
        assert_eq!(slot_width(0.5), SLOT_WIDTH / 2);
        assert_eq!(slot_width(1.0), SLOT_WIDTH);
        assert_eq!(slot_width(7.0), SLOT_WIDTH, "overshoot never widens a slot");
    }

    #[test]
    fn items_pack_leftwards_from_the_status_cluster() {
        let placed = place(&[(1, 1.0), (2, 1.0)], 500, 0, 5);
        assert_eq!(placed.len(), 2);
        assert_eq!(placed[0].1.loc.x, 500 - SLOT_WIDTH);
        assert_eq!(placed[1].1.loc.x, 500 - SLOT_WIDTH * 2);
        assert_eq!(placed[0].1.size.h, BUTTON_HEIGHT);
        assert_eq!(placed[1].1.loc.y, 5);
    }

    #[test]
    fn a_half_arrived_item_only_pushes_its_neighbours_half_a_slot() {
        let placed = place(&[(1, 0.5), (2, 1.0)], 500, 0, 0);
        assert_eq!(placed[0].1.size.w, SLOT_WIDTH / 2);
        assert_eq!(placed[1].1.loc.x, 500 - SLOT_WIDTH / 2 - SLOT_WIDTH);
    }

    #[test]
    fn hidden_items_take_no_room_and_the_clock_limit_is_respected() {
        let placed = place(&[(1, 0.0), (2, 1.0), (3, 1.0)], 100, 100 - SLOT_WIDTH - 1, 0);
        assert_eq!(placed.len(), 1, "the second visible item would cross the limit");
        assert_eq!(placed[0].0, 2);
    }

    #[test]
    fn passive_items_are_tracked_but_never_placed() {
        let mut strip = TrayStrip { enabled: true, ..TrayStrip::default() };
        assert!(strip.upsert(icon("a", glide_tray::Status::Passive)));
        assert!(!strip.upsert(icon("a", glide_tray::Status::Passive)));
        assert!(strip.layout(500, 0, 0).is_empty());
        assert!(strip.entries[0].presence.target() <= 0.0);
        assert!(strip.upsert(icon("a", glide_tray::Status::NeedsAttention)));
        assert!(strip.entries[0].presence.target() >= 1.0);
    }

    #[test]
    fn updates_keep_an_items_id_and_new_items_get_fresh_ones() {
        let mut strip = TrayStrip::default();
        strip.upsert(icon("a", glide_tray::Status::Active));
        strip.upsert(icon("b", glide_tray::Status::Active));
        let a = strip.entries[0].id;
        let mut changed = icon("a", glide_tray::Status::Active);
        changed.label = "renamed".into();
        assert!(strip.upsert(changed));
        assert_eq!(strip.entries[0].id, a);
        assert_ne!(strip.entries[1].id, a);
    }

    #[test]
    fn a_removed_item_fades_before_it_is_retired() {
        let mut strip = TrayStrip::default();
        strip.upsert(icon("a", glide_tray::Status::Active));
        strip.entries[0].presence = Tween::at(1.0);
        let id = strip.remove("a").expect("known");
        assert_eq!(strip.entries[0].id, id);
        assert!(strip.entries[0].presence.target() <= 0.0);
        if !motion::t2().is_zero() {
            assert!(!retired(&strip.entries[0]), "it must shrink first");
        }
        strip.entries[0].presence = Tween::at(0.0);
        assert!(retired(&strip.entries[0]));
        strip.settle(false);
        assert!(strip.entries.is_empty());
        assert_eq!(strip.remove("a"), None);
    }

    #[test]
    fn a_reappearing_item_is_revived_instead_of_duplicated() {
        let mut strip = TrayStrip::default();
        strip.upsert(icon("a", glide_tray::Status::Active));
        strip.remove("a");
        assert!(strip.upsert(icon("a", glide_tray::Status::Active)));
        assert_eq!(strip.entries.len(), 1);
        assert!(!strip.entries[0].gone);
    }

    #[test]
    fn mouse_buttons_map_to_the_three_item_actions() {
        assert_eq!(TrayButton::from_code(0x110), TrayButton::Primary);
        assert_eq!(TrayButton::from_code(BTN_MIDDLE), TrayButton::Secondary);
        assert_eq!(TrayButton::from_code(BTN_RIGHT), TrayButton::Context);
    }

    #[test]
    fn symbolic_masks_take_the_glyph_colour() {
        let mask = [255u8, 255, 255, 255, 0, 0, 0, 0];
        let red = Color([1.0, 0.0, 0.0, 1.0]);
        assert_eq!(tint(&mask, red), vec![0, 0, 255, 255, 0, 0, 0, 0]);
    }

    #[test]
    fn a_missing_scale_is_resampled_from_the_largest_raster() {
        let side = usize::try_from(ICON * 2).expect("positive");
        let art = IconArt { symbolic: false, rasters: vec![(2, [10u8, 20, 30, 255].repeat(side * side))] };
        let one = icon_pixels(&art, 1, Color([1.0; 4])).expect("resampled");
        assert_eq!(one.len(), usize::try_from(ICON * ICON * 4).expect("positive"));
        assert_eq!(&one[..4], &[10, 20, 30, 255]);
        assert!(icon_pixels(&IconArt::default(), 1, Color([1.0; 4])).is_none());
    }

    #[test]
    fn the_tray_menu_rides_the_app_menu_popup_and_yields_it_back() {
        let mut bar = Topbar::new(TopbarTheme::default());
        bar.set_tray_enabled(true);
        bar.upsert_tray_item(icon("a", glide_tray::Status::Active));
        let id = bar.tray.entries[0].id;
        let items = vec![MenuItem::action("Quit", glide_appmenu::Activation::DbusMenu(4))];
        assert!(bar.open_tray_menu(id, items));
        assert!(bar.tray_menu_showing(id));
        assert_eq!(bar.menu_items().len(), 1);
        assert_eq!(bar.menu_items()[0].children[0].label, "Quit");
        match bar.app_menu_row(MenuPath::top(0).child(0).expect("depth")) {
            super::super::MenuRowOutcome::Activate(activation) => {
                assert_eq!(activation, glide_appmenu::Activation::DbusMenu(4));
            }
            other => panic!("expected an activation, got {other:?}"),
        }
        assert!(!bar.tray_menu_showing(id));
        assert_eq!(bar.tray_menu_id(), Some(id), "the source outlives the close");
        bar.release_tray_menu();
        assert!(bar.menu_items().is_empty());
    }

    #[test]
    fn tray_items_sit_left_of_the_status_cluster_and_right_of_the_clock() {
        let output = Rectangle::new(Point::from((0, 0)), Size::from((1920, 1080)));
        let painter = crate::paint::Painter::new(14.0, None);
        let mut bar = Topbar::new(TopbarTheme::default());
        bar.set_layouts(vec!["US".into(), "UA".into()], 0);
        bar.set_tray_enabled(true);
        bar.upsert_tray_item(icon("a", glide_tray::Status::Active));
        bar.upsert_tray_item(icon("b", glide_tray::Status::Active));
        for entry in &mut bar.tray.entries {
            entry.presence = Tween::at(1.0);
        }
        bar.touch();
        bar.layout_slots(&painter, output);
        let first = bar.slot_rect(Slot::Tray(bar.tray.entries[0].id)).expect("placed");
        let second = bar.slot_rect(Slot::Tray(bar.tray.entries[1].id)).expect("placed");
        let dnd = bar.slot_rect(Slot::DoNotDisturb).expect("dnd");
        let clock = bar.slot_rect(Slot::Clock).expect("clock");
        assert!(first.loc.x + first.size.w <= dnd.loc.x);
        assert_eq!(second.loc.x + second.size.w, first.loc.x, "newer items go left");
        assert!(second.loc.x > clock.loc.x + clock.size.w);
        let hit = Point::from((f64::from(first.loc.x + first.size.w / 2), f64::from(first.loc.y + first.size.h / 2)));
        assert!(matches!(
            bar.action_at(hit),
            Some(super::super::TopbarAction::Tray(TrayClick { button: TrayButton::Primary, .. }))
        ));
    }

    #[test]
    fn removing_the_item_closes_its_menu() {
        let mut bar = Topbar::new(TopbarTheme::default());
        bar.set_tray_enabled(true);
        bar.upsert_tray_item(icon("a", glide_tray::Status::Active));
        let id = bar.tray.entries[0].id;
        let items = vec![MenuItem::action("Quit", glide_appmenu::Activation::DbusMenu(4))];
        bar.open_tray_menu(id, items);
        assert_eq!(bar.remove_tray_item("a"), Some(id));
        assert!(!bar.tray_menu_showing(id));
        assert!(bar.tray_item(id).is_none());
    }
}

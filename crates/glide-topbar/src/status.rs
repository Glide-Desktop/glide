use super::battery;
use super::glyphs::{Glyph, glyph, svg_glyph, themed, wifi_bars};
use super::{
    Anim, BUTTON_HEIGHT, BUTTON_RADIUS, Cache, Color, E_IO, E_OUT, HEIGHT, ITEM_GAP, Logical, MOTION_QUANTUM,
    MemoryRenderBuffer, MemoryRenderBufferRenderElement, OpenPopup, PAD_X, POPUP_GAP, POPUP_RADIUS, Placed, Point,
    Rectangle, STATUS_SEP_WIDTH, Scale, Size, Slot, Topbar, TopbarTheme, lerp, local_rect, motion, numeric_label_width,
    push_buffer, push_placed, push_popup_buffer, push_popup_shadow, quantize,
};
use crate::i18n::{Key, t};
use crate::paint::TextAlign;
use crate::raster::{Canvas, ShellRenderer, rect};
use crate::system_info::SystemStatus;
use glide_theme::spacing::{SP1, SP2, SP3};
use std::collections::HashMap;
use std::time::{Duration, Instant};

const CONTROL_W: i32 = 28;
const BLOCK_PAD: i32 = SP2;
const BLOCK_GAP: i32 = 6;
const PCT_GAP: i32 = SP1;
pub(super) const ICON: i32 = 16;
pub(super) const PCT_STYLE: glide_theme::TextStyle = glide_theme::TextStyle {
    size_px: 12.0,
    line_height_px: 16.0,
    weight: 550,
    tracking_em: 0.0,
    uppercase: false,
    tabular: true,
};
const DOT: i32 = 8;
const DOT_GAP: i32 = SP1;
const DOT_PAD: i32 = SP2;
const DOT_SCALE_FROM: f64 = 0.4;
const POPOVER_W: i32 = 264;
const SHEEN_LIVE: Duration = Duration::from_millis(100);
const MICRO_H: i32 = 20;
const PRIVACY_ROW_H: i32 = 40;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrivacyKind {
    Microphone,
    Camera,
    Screen,
}

impl PrivacyKind {
    const ALL: [Self; 3] = [Self::Microphone, Self::Camera, Self::Screen];

    const fn label(self) -> Key {
        match self {
            Self::Microphone => Key::TbMicrophone,
            Self::Camera => Key::TbCamera,
            Self::Screen => Key::TbScreenRecording,
        }
    }

    const fn colour(self, theme: &TopbarTheme) -> Color {
        match self {
            Self::Microphone => theme.privacy_mic,
            Self::Camera => theme.privacy_camera,
            Self::Screen => theme.privacy_screen,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Indicator {
    Wifi,
    Ethernet,
    Bluetooth,
    Vpn,
    Volume,
    Battery,
}

type ClusterKey = (u64, u64, Option<OpenPopup>, Option<Slot>, [u8; 3], i32, (bool, bool));

#[derive(Debug, Default)]
pub(super) struct Indicators {
    dots: [Anim; 3],
    reserved: [bool; 3],
    screen_clients: Vec<String>,
    rev: u64,
    cluster_cache: Cache<ClusterKey>,
    privacy_cache: Cache<(u64, u64, i32)>,
    swap: ClusterSwap,
    charge: ChargeSheen,
}

type SheenKey = (i32, i32, [u32; 4], i32);

#[derive(Debug, Default)]
struct ChargeSheen {
    since: Option<Instant>,
    drawn: Option<Instant>,
    wake: Option<Instant>,
    buffers: HashMap<SheenKey, MemoryRenderBuffer>,
}

impl ChargeSheen {
    fn live(&self) -> bool {
        self.drawn.is_some_and(|at| at.elapsed() < SHEEN_LIVE)
    }

    fn stop(&mut self) {
        self.since = None;
        self.drawn = None;
        self.wake = None;
    }

    fn advance(&mut self, cycle: battery::Cycle, now: Instant) -> Option<f64> {
        let since = *self.since.get_or_insert(now);
        let elapsed = now.saturating_duration_since(since);
        let progress = cycle.progress(elapsed);
        if progress.is_some() {
            self.drawn = Some(now);
            self.wake = None;
        } else {
            self.wake = Some(now + cycle.until_next(elapsed));
        }
        progress
    }
}

#[derive(Debug, Default)]
struct ClusterSwap {
    seen: Option<(usize, Rectangle<i32, Logical>, MemoryRenderBuffer)>,
    old: Option<(Rectangle<i32, Logical>, MemoryRenderBuffer)>,
    fade: Anim,
}

impl Indicators {
    pub(super) fn clear(&mut self) {
        self.cluster_cache.clear();
        self.privacy_cache.clear();
    }

    pub(super) fn is_animating(&self) -> bool {
        self.dots.iter().any(Anim::is_animating) || self.swap.fade.is_animating() || self.charge.live()
    }

    pub(super) const fn next_wake(&self) -> Option<Instant> {
        self.charge.wake
    }

    fn reserved_count(&self) -> i32 {
        i32::try_from(self.reserved.iter().filter(|on| **on).count()).unwrap_or(0)
    }

    fn levels(&self) -> [f64; 3] {
        self.dots.map(|dot| dot.value().clamp(0.0, 1.0))
    }
}

#[must_use]
pub fn privacy_entries(status: &SystemStatus, screen_clients: &[String]) -> Vec<(PrivacyKind, Vec<String>)> {
    let mut screen: Vec<String> =
        status.privacy.screen.iter().chain(screen_clients).filter(|name| !name.is_empty()).cloned().collect();
    screen.sort();
    screen.dedup();
    if screen.is_empty() && !status.privacy.screen.is_empty() {
        screen.push(String::new());
    }
    [
        (PrivacyKind::Microphone, status.privacy.microphone.clone()),
        (PrivacyKind::Camera, status.privacy.camera.clone()),
        (PrivacyKind::Screen, screen),
    ]
    .into_iter()
    .filter(|(_, apps)| !apps.is_empty())
    .collect()
}

#[must_use]
pub fn cluster_order(privacy_dots: bool, headset: bool) -> Vec<Slot> {
    let mut order = vec![Slot::Status];
    if headset {
        order.push(Slot::Headset);
    }
    order.push(Slot::Search);
    order.push(Slot::Passwords);
    if privacy_dots {
        order.push(Slot::Privacy);
    }
    order
}

#[must_use]
pub fn block_indicators(status: &SystemStatus) -> Vec<Indicator> {
    let wifi_in_use = status.wifi_available && status.wifi_enabled && !status.wifi_ssid.is_empty();
    let wired = status.ethernet_available && (status.ethernet_connected || !status.wifi_available) && !wifi_in_use;
    let mut items = vec![if wired { Indicator::Ethernet } else { Indicator::Wifi }];
    if status.bluetooth_on {
        items.push(Indicator::Bluetooth);
    }
    if status.vpn_active {
        items.push(Indicator::Vpn);
    }
    items.push(Indicator::Volume);
    if status.battery_pct.is_some() {
        items.push(Indicator::Battery);
    }
    items
}

const fn privacy_width(dots: i32) -> i32 {
    let gaps = if dots > 1 { dots - 1 } else { 0 };
    DOT_PAD * 2 + dots * DOT + gaps * DOT_GAP
}

fn battery_width(painter: &crate::paint::Painter, pct: u8, show_pct: bool) -> i32 {
    let mut width = battery::STRIP_W;
    if show_pct {
        width += PCT_GAP + numeric_label_width(painter, &format!("{pct}%"), &PCT_STYLE);
    }
    width
}

fn indicator_width(
    painter: &crate::paint::Painter, status: &SystemStatus, show_pct: bool, indicator: Indicator,
) -> i32 {
    match indicator {
        Indicator::Battery => battery_width(painter, status.battery_pct.unwrap_or(0), show_pct),
        Indicator::Wifi | Indicator::Ethernet | Indicator::Volume | Indicator::Bluetooth | Indicator::Vpn => ICON,
    }
}

fn block_layout(painter: &crate::paint::Painter, status: &SystemStatus, show_pct: bool) -> Vec<(Indicator, i32, i32)> {
    let mut x = BLOCK_PAD;
    block_indicators(status)
        .into_iter()
        .enumerate()
        .map(|(index, indicator)| {
            if index > 0 {
                x += BLOCK_GAP;
            }
            let width = indicator_width(painter, status, show_pct, indicator);
            let item = (indicator, x, width);
            x += width;
            item
        })
        .collect()
}

fn block_width(painter: &crate::paint::Painter, status: &SystemStatus, show_pct: bool) -> i32 {
    block_layout(painter, status, show_pct).last().map_or(BLOCK_PAD * 2, |(_, x, width)| x + width + BLOCK_PAD)
}

fn privacy_popover_height(rows: usize) -> i32 {
    SP3 + MICRO_H + i32::try_from(rows).unwrap_or(0) * PRIVACY_ROW_H + SP2
}

fn place_under(
    anchor: Rectangle<i32, Logical>, output: Rectangle<i32, Logical>, strip_y: i32, size: Size<i32, Logical>,
) -> Rectangle<i32, Logical> {
    let centred = anchor.loc.x + (anchor.size.w - size.w) / 2;
    let min_x = output.loc.x + PAD_X;
    let max_x = (output.loc.x + output.size.w - PAD_X - size.w).max(min_x);
    Rectangle::new(Point::from((centred.clamp(min_x, max_x), strip_y + HEIGHT + POPUP_GAP)), size)
}

impl Topbar {
    pub fn set_screen_capture(&mut self, clients: Vec<String>) -> bool {
        if self.indicators.screen_clients == clients {
            return false;
        }
        self.indicators.screen_clients = clients;
        self.sync_indicators();
        true
    }

    #[must_use]
    pub fn sharing_screen(&self) -> bool {
        !self.system_status.privacy.screen.is_empty() || !self.indicators.screen_clients.is_empty()
    }

    fn privacy_active(&self) -> [bool; 3] {
        let entries = privacy_entries(&self.system_status, &self.indicators.screen_clients);
        PrivacyKind::ALL.map(|kind| entries.iter().any(|(active, _)| *active == kind))
    }

    pub(super) fn sync_indicators(&mut self) {
        let active = self.privacy_active();
        let mut relayout = false;
        for (index, on) in active.into_iter().enumerate() {
            let dot = &mut self.indicators.dots[index];
            if on {
                dot.retarget(1.0, motion::t2(), E_OUT);
                if !self.indicators.reserved[index] {
                    self.indicators.reserved[index] = true;
                    relayout = true;
                }
            } else {
                dot.retarget(0.0, motion::t2(), E_IO);
            }
        }
        self.indicators.rev = self.indicators.rev.wrapping_add(1);
        if relayout {
            self.touch();
        }
    }

    pub(super) fn settle_indicators(&mut self) {
        let mut changed = false;
        for index in 0..PrivacyKind::ALL.len() {
            let dot = self.indicators.dots[index];
            if self.indicators.reserved[index] && dot.target() <= 0.0 && !dot.is_animating() {
                self.indicators.reserved[index] = false;
                changed = true;
            }
        }
        if !changed {
            return;
        }
        self.indicators.rev = self.indicators.rev.wrapping_add(1);
        if self.indicators.reserved_count() == 0 && self.open_popup == Some(OpenPopup::Privacy) {
            self.close_popup();
        }
        self.touch();
    }

    #[must_use]
    pub fn privacy_dots(&self) -> [bool; 3] {
        self.indicators.reserved
    }

    fn cluster_width(&self, painter: &crate::paint::Painter, slot: Slot) -> i32 {
        match slot {
            Slot::Status => block_width(painter, &self.system_status, self.theme.battery_percentage),
            Slot::Privacy => privacy_width(self.indicators.reserved_count()),
            Slot::Headset => {
                self.headset().map_or(CONTROL_W, |headset| super::headset_view::applet_width(painter, headset))
            }
            Slot::Phone => self.phone().map_or(CONTROL_W, |phone| super::phone_view::applet_width(painter, phone)),
            Slot::Passwords => super::passwords_view::APPLET_W,
            _ => CONTROL_W,
        }
    }

    pub(super) fn layout_status_cluster(
        &self, painter: &crate::paint::Painter, mut right: i32, y: i32,
        slots: &mut Vec<(Slot, Rectangle<i32, Logical>)>,
    ) -> i32 {
        let mut order = cluster_order(self.indicators.reserved_count() > 0, self.headset().is_some());
        if self.phone().is_some() {
            let insert_at = if self.headset().is_some() { 2 } else { 1 };
            order.insert(insert_at, Slot::Phone);
        }
        for (index, slot) in order.into_iter().enumerate() {
            if index > 0 {
                right -= ITEM_GAP;
            }
            let width = self.cluster_width(painter, slot);
            right -= width;
            slots.push((slot, local_rect(right, y, width, BUTTON_HEIGHT)));
        }
        right
    }

    pub(super) fn layout_status_popup(
        &self, slots: &[(Slot, Rectangle<i32, Logical>)], output: Rectangle<i32, Logical>, strip_y: i32,
    ) -> Option<Rectangle<i32, Logical>> {
        if self.shown_popup()? != OpenPopup::Privacy {
            return None;
        }
        let anchor = slots.iter().find(|(slot, _)| *slot == Slot::Privacy).map(|(_, rect)| *rect)?;
        let rows = privacy_entries(&self.system_status, &self.indicators.screen_clients);
        let height = privacy_popover_height(rows.len());
        Some(place_under(anchor, output, strip_y, Size::from((POPOVER_W, height))))
    }
}

struct ClusterPaint<'a> {
    painter: &'a crate::paint::Painter,
    theme: &'a TopbarTheme,
    status: &'a SystemStatus,
    width: i32,
    scale: i32,
    hovered: Option<Slot>,
    open: Option<OpenPopup>,
    levels: [f64; 3],
    reserved: [bool; 3],
    headset: Option<&'a crate::headset::Headset>,
    phone: Option<&'a crate::phone::PhoneState>,
    passwords: &'a super::passwords_view::Applet,
}

pub(super) fn push_cluster<R>(
    topbar: &mut Topbar, renderer: &mut R, painter: &crate::paint::Painter, icons: &mut crate::icon::IconLoader,
    scale: Scale<f64>, output_scale: i32, elements: &mut Vec<MemoryRenderBufferRenderElement<R>>,
) where
    R: ShellRenderer,
{
    let items: Vec<(Slot, Rectangle<i32, Logical>)> =
        topbar.slots.iter().filter(|(slot, _)| slot.in_status_cluster()).copied().collect();
    let (Some(left), Some(right)) =
        (items.iter().map(|(_, rect)| rect.loc.x).min(), items.iter().map(|(_, rect)| rect.loc.x + rect.size.w).max())
    else {
        return;
    };
    let y = items.first().map_or(0, |(_, rect)| rect.loc.y);
    let width = right - left;

    let theme = &topbar.theme;
    if let Some(buffer) = topbar.separator_cache.resolve((), output_scale, || draw_separator(theme, output_scale)) {
        push_buffer(renderer, buffer, Point::from((left - STATUS_SEP_WIDTH, y)), scale, elements);
    }

    let open = topbar.open_popup;
    let hovered = topbar.hovered.filter(|slot| slot.in_status_cluster());
    let levels = topbar.indicators.levels();
    let key: ClusterKey = (
        topbar.status_rev,
        topbar.indicators.rev,
        open,
        hovered,
        levels.map(|level| quantize(level, MOTION_QUANTUM)),
        width,
        (topbar.passwords.locked, topbar.passwords.offer),
    );
    let ctx = ClusterPaint {
        painter,
        theme,
        status: &topbar.system_status,
        width,
        scale: output_scale,
        hovered,
        open,
        levels,
        reserved: topbar.indicators.reserved,
        headset: topbar.headsets.last(),
        phone: topbar.phone.as_ref(),
        passwords: &topbar.passwords,
    };
    let local: Vec<(Slot, i32, i32)> =
        items.iter().map(|(slot, rect)| (*slot, rect.loc.x - left, rect.size.w)).collect();
    let Some(buffer) =
        topbar.indicators.cluster_cache.resolve(key, output_scale, || draw_cluster(&ctx, &local, Some(icons)))
    else {
        return;
    };
    let buffer = buffer.clone();
    let height = items.first().map_or(HEIGHT, |(_, rect)| rect.size.h);
    let area = Rectangle::new(Point::from((left, y)), Size::from((width, height)));
    let count = items.iter().filter(|(slot, _)| *slot != Slot::Privacy).count();
    let swap = &mut topbar.indicators.swap;
    if let Some((seen, old_area, old)) = swap.seen.take()
        && seen != count
    {
        swap.old = Some((old_area, old));
        swap.fade = Anim::at(0.0);
        swap.fade.retarget(1.0, motion::t2(), E_OUT);
    }
    swap.seen = Some((count, area, buffer.clone()));
    let fade = swap.fade.value().clamp(0.0, 1.0);
    if fade < 1.0
        && let Some((old_area, old)) = &swap.old
    {
        let placed = Placed { popup: *old_area, element: *old_area, progress: 1.0, alpha: (1.0 - fade) as f32 };
        push_placed(renderer, old, placed, scale, elements);
    }
    push_buffer(renderer, &buffer, Point::from((left, y)), scale, elements);
    let status_left = items.iter().find(|(slot, _)| *slot == Slot::Status).map(|(_, rect)| rect.loc.x);
    push_charge_sheen(topbar, renderer, painter, (status_left, y), (scale, output_scale), elements);
}

fn push_charge_sheen<R>(
    topbar: &mut Topbar, renderer: &mut R, painter: &crate::paint::Painter, (left, y): (Option<i32>, i32),
    (scale, output_scale): (Scale<f64>, i32), elements: &mut Vec<MemoryRenderBufferRenderElement<R>>,
) where
    R: ShellRenderer,
{
    let status = &topbar.system_status;
    let charging = status.battery_charging && status.battery_pct.is_some_and(|pct| pct < 100);
    let (Some(left), Some(pct), Some(cycle), true) = (left, status.battery_pct, battery::Cycle::current(), charging)
    else {
        topbar.indicators.charge.stop();
        return;
    };
    let charge = &mut topbar.indicators.charge;
    let Some(progress) = charge.advance(cycle, Instant::now()) else {
        return;
    };
    let Some(x) = block_layout(painter, status, topbar.theme.battery_percentage)
        .into_iter()
        .find(|(indicator, _, _)| *indicator == Indicator::Battery)
        .map(|(_, x, _)| x)
    else {
        return;
    };
    let cell = battery_cell(left + x);
    let Some(sheen) = battery::sheen(&cell, pct, progress) else {
        return;
    };
    let colour = battery::sheen_colour(topbar.theme.ok);
    let key = (sheen.size.w, sheen.size.h, colour.0.map(f32::to_bits), output_scale);
    let buffer = match charge.buffers.entry(key) {
        std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
        std::collections::hash_map::Entry::Vacant(entry) => {
            let Some(mut c) = Canvas::new(sheen.size.w, sheen.size.h, output_scale) else {
                return;
            };
            c.fill(c.full(), colour, 1.0);
            entry.insert(c.finish())
        }
    };
    push_buffer(renderer, buffer, Point::from((sheen.loc.x, y + sheen.loc.y)), scale, elements);
}

const fn opens(slot: Slot) -> Option<OpenPopup> {
    match slot {
        Slot::Status => Some(OpenPopup::QuickSettings),
        Slot::Privacy => Some(OpenPopup::Privacy),
        Slot::Headset => Some(OpenPopup::Headset),
        Slot::Phone => Some(OpenPopup::Phone),
        Slot::Passwords => Some(OpenPopup::Passwords),
        _ => None,
    }
}

fn column(x: i32, width: i32) -> Rectangle<i32, Logical> {
    rect(x, 0, width, BUTTON_HEIGHT)
}

fn draw_dots(c: &mut Canvas<'_>, ctx: &ClusterPaint<'_>, x: i32) {
    let mut column = 0;
    for (index, kind) in PrivacyKind::ALL.into_iter().enumerate() {
        if !ctx.reserved[index] {
            continue;
        }
        let centre_x = x + DOT_PAD + column * (DOT + DOT_GAP) + DOT / 2;
        column += 1;
        let level = ctx.levels[index];
        if level <= 0.0 {
            continue;
        }
        let side = (f64::from(DOT) * lerp(DOT_SCALE_FROM, 1.0, level)).round() as i32;
        let alpha = level as f32;
        c.fill(
            rect(centre_x - side / 2, (BUTTON_HEIGHT - side) / 2, side, side),
            kind.colour(ctx.theme).with_alpha(alpha),
            glide_theme::shape::R_ROUND,
        );
    }
}

fn draw_cluster(
    ctx: &ClusterPaint<'_>, items: &[(Slot, i32, i32)], icons: Option<&mut crate::icon::IconLoader>,
) -> Option<MemoryRenderBuffer> {
    let mut c = Canvas::new(ctx.width, BUTTON_HEIGHT, ctx.scale)?.with_icons(icons);
    for &(slot, x, width) in items {
        let open = opens(slot).is_some_and(|popup| ctx.open == Some(popup));
        if open {
            c.fill(rect(x, 0, width, BUTTON_HEIGHT), ctx.theme.open_fill, BUTTON_RADIUS);
        }
        let fg = if open || ctx.hovered == Some(slot) { ctx.theme.label } else { ctx.theme.muted };
        match slot {
            Slot::Status => {
                for (indicator, dx, width) in block_layout(ctx.painter, ctx.status, ctx.theme.battery_percentage) {
                    draw_indicator(&mut c, ctx, (indicator, x + dx, width), fg);
                }
            }
            Slot::Search => glyph(&mut c, Glyph::Search, ICON, column(x, width), fg),
            Slot::Privacy => draw_dots(&mut c, ctx, x),
            Slot::Headset => {
                if let Some(headset) = ctx.headset {
                    super::headset_view::draw_applet(&mut c, ctx.painter, ctx.theme, headset, x, fg);
                }
            }
            Slot::Phone => {
                if let Some(phone) = ctx.phone {
                    super::phone_view::draw_applet(&mut c, ctx.painter, ctx.theme, phone, x, fg);
                }
            }
            Slot::Passwords => super::passwords_view::draw_applet(&mut c, ctx.theme, ctx.passwords, x, fg),
            _ => {}
        }
    }
    Some(c.finish())
}

fn draw_separator(theme: &TopbarTheme, scale: i32) -> Option<MemoryRenderBuffer> {
    let mut c = Canvas::new(STATUS_SEP_WIDTH, BUTTON_HEIGHT, scale)?;
    c.fill(rect(STATUS_SEP_WIDTH / 2, (BUTTON_HEIGHT - 16) / 2, 1, 16), theme.border, 0.0);
    Some(c.finish())
}

const fn speaker(muted: bool, pct: u8) -> Glyph {
    match (muted, pct) {
        (true, _) => Glyph::SpeakerMuted,
        (false, 0) => Glyph::SpeakerSilent,
        (false, 1..=60) => Glyph::SpeakerLow,
        (false, _) => Glyph::SpeakerHigh,
    }
}

const WIRED: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\" fill=\"#000\"><path d=\"M5.5 1.5h5a1 1 0 0 1 1 1v3a1 1 0 0 1-1 1h-1.75v2h4.25a1 1 0 0 1 1 1v2h1a.5.5 0 0 1 .5.5v2.5a.5.5 0 0 1-.5.5h-3a.5.5 0 0 1-.5-.5V12a.5.5 0 0 1 .5-.5h1v-1.5H3.5v1.5h1a.5.5 0 0 1 .5.5v2.5a.5.5 0 0 1-.5.5h-3a.5.5 0 0 1-.5-.5V12a.5.5 0 0 1 .5-.5h1v-2a1 1 0 0 1 1-1h4.25v-2H5.5a1 1 0 0 1-1-1v-3a1 1 0 0 1 1-1z\"/></svg>";

pub(super) fn battery_cell(x: i32) -> battery::Cell {
    battery::Cell::new(x, (BUTTON_HEIGHT - battery::STRIP_H) / 2, battery::STRIP_W, battery::STRIP_H)
}

fn draw_indicator(c: &mut Canvas<'_>, ctx: &ClusterPaint<'_>, (indicator, x, width): (Indicator, i32, i32), fg: Color) {
    let status = ctx.status;
    let cell = column(x, width);
    match indicator {
        Indicator::Battery => draw_status_battery(c, ctx, x, fg),
        Indicator::Wifi => {
            let name =
                if status.wifi_enabled { super::wifi_icon_name(status.wifi_level) } else { "network-wireless-offline" };
            if !themed(c, name, ICON, cell, fg) {
                let level = if status.wifi_enabled { status.wifi_level } else { 0 };
                svg_glyph(c, &wifi_bars(level), ICON, cell, fg);
            }
        }
        Indicator::Ethernet => {
            let name = if status.ethernet_connected { "network-wired" } else { "network-wired-disconnected" };
            if !themed(c, name, ICON, cell, fg) {
                let ink = if status.ethernet_connected { fg } else { fg.with_alpha(fg.0[3] * 0.5) };
                svg_glyph(c, WIRED, ICON, cell, ink);
            }
        }
        Indicator::Bluetooth => {
            if !(status.bluetooth_connected && themed(c, "bluetooth-active", ICON, cell, fg)) {
                glyph(c, Glyph::Bluetooth, ICON, cell, fg);
            }
        }
        Indicator::Vpn => glyph(c, Glyph::Shield, ICON, cell, fg),
        Indicator::Volume => {
            if !themed(c, super::volume_icon_name(status.volume_muted, status.volume_pct), ICON, cell, fg) {
                glyph(c, speaker(status.volume_muted, status.volume_pct), ICON, cell, fg);
            }
        }
    }
}

fn strip_fill(theme: &TopbarTheme, pct: u8, charging: bool, colour: Color) -> Color {
    if charging && pct < 100 { theme.ok } else { colour }
}

fn draw_status_battery(c: &mut Canvas<'_>, ctx: &ClusterPaint<'_>, x: i32, fg: Color) {
    let Some(pct) = ctx.status.battery_pct else {
        return;
    };
    let charging = ctx.status.battery_charging;
    let low = !charging && pct <= ctx.theme.battery_low_pct;
    let colour = if low { ctx.theme.danger } else { fg };
    let fill = strip_fill(ctx.theme, pct, charging, colour);
    battery::draw(c, &battery_cell(x), battery::Look { pct, charging, ink: fg, fill });
    if ctx.theme.battery_percentage {
        let left = x + battery::STRIP_W + PCT_GAP;
        let text = format!("{pct}%");
        let width = numeric_label_width(ctx.painter, &text, &PCT_STYLE);
        c.text(ctx.painter, (left, left + width, 0, BUTTON_HEIGHT), TextAlign::Right, &text, colour, &PCT_STYLE);
    }
}

pub(super) fn push_popups<R>(
    topbar: &mut Topbar, renderer: &mut R, painter: &crate::paint::Painter, scale: Scale<f64>, output_scale: i32,
    elements: &mut Vec<MemoryRenderBufferRenderElement<R>>,
) where
    R: ShellRenderer,
{
    if topbar.shown_popup() != Some(OpenPopup::Privacy) {
        return;
    }
    let Some(rect) = topbar.popup_rect else {
        return;
    };
    let reveal = topbar.popup_progress();
    let theme = &topbar.theme;
    let size = rect.size;
    let entries = privacy_entries(&topbar.system_status, &topbar.indicators.screen_clients);
    let Some(buffer) = topbar.indicators.privacy_cache.resolve(
        (topbar.status_rev, topbar.indicators.rev, size.h),
        output_scale,
        || draw_privacy_popover(painter, theme, &entries, size, output_scale),
    ) else {
        return;
    };
    push_popup_buffer(renderer, buffer, rect, rect, reveal, scale, elements);
    push_popup_shadow(topbar, renderer, rect, reveal, scale, output_scale, elements);
}

fn draw_privacy_popover(
    painter: &crate::paint::Painter, theme: &TopbarTheme, entries: &[(PrivacyKind, Vec<String>)],
    size: Size<i32, Logical>, scale: i32,
) -> Option<MemoryRenderBuffer> {
    use glide_theme::TextStyle;
    let mut c = Canvas::popover((size.w, size.h), scale, POPUP_RADIUS, theme.popover_bg, theme.border)?;
    let (left, right) = (SP3, size.w - SP3);
    c.text(
        painter,
        (left, right, SP3, SP3 + MICRO_H),
        TextAlign::Left,
        t(Key::TbPrivacy),
        theme.faint,
        &TextStyle::MICRO,
    );
    let mut y = SP3 + MICRO_H;
    for (kind, apps) in entries {
        c.fill(rect(left, y + (PRIVACY_ROW_H - DOT) / 2, DOT, DOT), kind.colour(theme), glide_theme::shape::R_ROUND);
        let text_left = left + DOT + SP2;
        c.text(
            painter,
            (text_left, right, y + 2, y + 20),
            TextAlign::Left,
            t(kind.label()),
            theme.label,
            &TextStyle::HEAD,
        );
        let names: Vec<&str> =
            apps.iter().map(|name| if name.is_empty() { t(Key::TbUnknownApp) } else { name.as_str() }).collect();
        c.text(
            painter,
            (text_left, right, y + 20, y + 36),
            TextAlign::Left,
            &names.join(", "),
            theme.muted,
            &TextStyle::CAP,
        );
        y += PRIVACY_ROW_H;
    }
    Some(c.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system_info::Privacy;

    fn laptop() -> SystemStatus {
        SystemStatus { battery_pct: Some(80), bluetooth_on: true, vpn_active: true, ..SystemStatus::default() }
    }

    #[test]
    fn a_charging_battery_resting_between_sweeps_is_not_animating() {
        let cycle = battery::Cycle { travel: Duration::from_millis(300), rest: Duration::from_secs(5) };
        let start = Instant::now().checked_sub(Duration::from_secs(1)).expect("the clock is past one second");
        let mut sheen = ChargeSheen::default();
        assert!(sheen.advance(cycle, start).is_some());
        assert_eq!(sheen.wake, None, "a travelling sheen keeps the frame pump itself");
        assert!(sheen.advance(cycle, start + Duration::from_millis(400)).is_none());
        assert!(!sheen.live(), "the rest between sweeps draws nothing, so it must not keep redrawing");
        assert_eq!(sheen.wake, Some(start + cycle.travel + cycle.rest), "the next sweep wakes the pump");
    }

    #[test]
    fn the_cluster_is_the_status_block_then_search_then_passwords_then_privacy() {
        assert_eq!(cluster_order(true, false), vec![Slot::Status, Slot::Search, Slot::Passwords, Slot::Privacy]);
        assert_eq!(cluster_order(false, false), vec![Slot::Status, Slot::Search, Slot::Passwords]);
        assert_eq!(
            cluster_order(false, true),
            vec![Slot::Status, Slot::Headset, Slot::Search, Slot::Passwords],
            "a connected headset sits beside the status block"
        );
    }

    #[test]
    fn the_screen_counts_as_shared_through_the_portal_or_a_direct_capture() {
        let mut bar = crate::Topbar::new(crate::TopbarTheme::default());
        assert!(!bar.sharing_screen());
        bar.set_screen_capture(vec!["wf-recorder".into()]);
        assert!(bar.sharing_screen());
        bar.set_screen_capture(Vec::new());
        assert!(!bar.sharing_screen());
    }

    #[test]
    fn the_block_reads_network_then_sound_then_battery() {
        assert_eq!(
            block_indicators(&laptop()),
            vec![Indicator::Wifi, Indicator::Bluetooth, Indicator::Vpn, Indicator::Volume, Indicator::Battery,]
        );
    }

    #[test]
    fn absent_hardware_and_idle_services_leave_no_gap_in_the_block() {
        assert_eq!(block_indicators(&SystemStatus::default()), vec![Indicator::Wifi, Indicator::Volume]);
    }

    #[test]
    fn indicators_sit_inside_the_block_without_overlapping() {
        let painter = crate::paint::Painter::new(14.0, None);
        let status = laptop();
        let items = block_layout(&painter, &status, true);
        let width = block_width(&painter, &status, true);
        assert_eq!(items.first().map(|(_, x, _)| *x), Some(BLOCK_PAD));
        for pair in items.windows(2) {
            assert_eq!(pair[0].1 + pair[0].2 + BLOCK_GAP, pair[1].1);
        }
        let (_, x, w) = items.last().copied().expect("battery");
        assert_eq!(x + w + BLOCK_PAD, width, "the same padding on both ends");
        assert!(
            block_width(&painter, &SystemStatus::default(), true) < width,
            "fewer indicators make a narrower block"
        );
    }

    #[test]
    fn privacy_rows_list_each_capture_kind_with_its_apps() {
        let status = SystemStatus {
            privacy: Privacy { microphone: vec!["Firefox".into()], camera: Vec::new(), screen: vec![String::new()] },
            ..SystemStatus::default()
        };
        let rows = privacy_entries(&status, &["obs".to_string()]);
        assert_eq!(
            rows,
            vec![
                (PrivacyKind::Microphone, vec!["Firefox".to_string()]),
                (PrivacyKind::Screen, vec!["obs".to_string()]),
            ]
        );
        let unnamed = privacy_entries(&status, &[]);
        assert_eq!(unnamed[1], (PrivacyKind::Screen, vec![String::new()]));
        assert!(privacy_entries(&SystemStatus::default(), &[]).is_empty());
    }

    #[test]
    fn privacy_dot_width_grows_by_one_dot_and_gap() {
        assert_eq!(privacy_width(1), DOT_PAD * 2 + DOT);
        assert_eq!(privacy_width(3) - privacy_width(2), DOT + DOT_GAP);
    }

    #[test]
    #[ignore = "writes preview PNGs to $GLIDE_QS_PREVIEW"]
    fn status_strip_preview() {
        let Some(dir) = std::env::var_os("GLIDE_QS_PREVIEW") else {
            return;
        };
        let painter = crate::paint::Painter::new(14.0, None);
        let mut icons = crate::icon::IconLoader::default();
        let passwords = super::super::passwords_view::Applet::default();
        let cases = [("discharging", 64, false), ("charging", 37, true), ("low", 6, false), ("full", 100, true)];
        for appearance in [glide_theme::Appearance::Dark, glide_theme::Appearance::Light] {
            let theme = TopbarTheme::from_tokens(&glide_theme::Tokens::new(appearance, glide_theme::Accent::Blue));
            for (name, pct, charging) in cases {
                let status = SystemStatus {
                    battery_pct: Some(pct),
                    battery_charging: charging,
                    wifi_enabled: true,
                    wifi_ssid: "Home".into(),
                    wifi_level: 3,
                    bluetooth_on: true,
                    ..SystemStatus::default()
                };
                let block = block_width(&painter, &status, true);
                let items = [(Slot::Status, 0, block), (Slot::Search, block + 2, CONTROL_W)];
                let width = block + 2 + CONTROL_W;
                let ctx = ClusterPaint {
                    painter: &painter,
                    theme: &theme,
                    status: &status,
                    width,
                    scale: 2,
                    hovered: None,
                    open: None,
                    levels: [0.0; 3],
                    reserved: [false; 3],
                    headset: None,
                    phone: None,
                    passwords: &passwords,
                };
                let mut c = Canvas::new(width + 16, HEIGHT, 2).expect("canvas");
                c.fill(c.full(), theme.background, 0.0);
                let mut cluster = Canvas::new(width, BUTTON_HEIGHT, 2).expect("cluster").with_icons(Some(&mut icons));
                for &(slot, x, w) in &items {
                    if slot == Slot::Status {
                        for (indicator, dx, iw) in block_layout(&painter, &status, true) {
                            draw_indicator(&mut cluster, &ctx, (indicator, x + dx, iw), theme.muted);
                        }
                    } else {
                        glyph(&mut cluster, Glyph::Search, ICON, column(x, w), theme.muted);
                    }
                }
                crate::paint::blit_rect_over(
                    &mut c.buf,
                    c.w,
                    c.h,
                    &cluster.buf,
                    width,
                    BUTTON_HEIGHT,
                    8,
                    (HEIGHT - BUTTON_HEIGHT) / 2,
                    2,
                );
                let (w, h) = ((c.w * c.scale) as u32, (c.h * c.scale) as u32);
                let image = glide_shell::image::Image::from_bgra(w, h, c.buf.clone()).expect("image");
                let file = format!("strip-{name}-{appearance:?}.png").to_ascii_lowercase();
                std::fs::write(std::path::Path::new(&dir).join(file), image.png().expect("png")).expect("write");
            }
        }
    }

    #[test]
    fn a_wired_desktop_shows_its_cable_instead_of_an_offline_wifi() {
        let wired = SystemStatus {
            wifi_available: false,
            ethernet_available: true,
            ethernet_connected: true,
            ..SystemStatus::default()
        };
        assert_eq!(block_indicators(&wired)[0], Indicator::Ethernet);
        let unplugged = SystemStatus { ethernet_connected: false, ..wired.clone() };
        assert_eq!(block_indicators(&unplugged)[0], Indicator::Ethernet, "no Wi-Fi card: the cable is the network");
        let both = SystemStatus { wifi_available: true, wifi_enabled: true, ..wired.clone() };
        assert_eq!(block_indicators(&both)[0], Indicator::Ethernet, "a laptop on a cable with Wi-Fi idle");
        let roaming = SystemStatus { wifi_ssid: "Home".into(), ..both };
        assert_eq!(block_indicators(&roaming)[0], Indicator::Wifi, "Wi-Fi in use wins");
        let offline = SystemStatus { ethernet_connected: false, wifi_available: true, ..wired };
        assert_eq!(block_indicators(&offline)[0], Indicator::Wifi);
    }
}

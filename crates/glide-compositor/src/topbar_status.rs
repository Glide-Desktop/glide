use super::glyphs::{Glyph, battery_cell, glyph, svg_glyph, themed, wifi_bars};
use super::{
    Anim, BUTTON_HEIGHT, BUTTON_RADIUS, Cache, Color, E_IO, E_OUT, HEIGHT, ITEM_GAP, Logical, MOTION_QUANTUM,
    MemoryRenderBuffer, MemoryRenderBufferRenderElement, OpenPopup, PAD_X, POPUP_GAP, POPUP_RADIUS, Point, Rectangle,
    STATUS_SEP_WIDTH, Scale, Size, Slot, Topbar, TopbarTheme, lerp, local_rect, motion, numeric_label_width,
    push_buffer, push_popup_buffer, push_popup_shadow, quantize,
};
use crate::i18n::{Key, t};
use crate::paint::TextAlign;
use crate::raster::{Canvas, ShellRenderer, rect};
use crate::system_info::SystemStatus;
use glide_theme::spacing::{SP1, SP2, SP3};

const CONTROL_W: i32 = 28;
const BLOCK_PAD: i32 = SP2;
const BLOCK_GAP: i32 = 6;
const PCT_GAP: i32 = SP1;
pub(super) const ICON: i32 = 16;
pub(super) const PCT_STYLE: glide_theme::TextStyle = glide_theme::TextStyle {
    size_px: 11.0,
    line_height_px: 14.0,
    weight: 500,
    tracking_em: 0.0,
    uppercase: false,
    tabular: true,
};
const DOT: i32 = 8;
const DOT_GAP: i32 = SP1;
const DOT_PAD: i32 = SP2;
const DOT_SCALE_FROM: f64 = 0.4;
const POPOVER_W: i32 = 264;
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
    Bluetooth,
    Vpn,
    Volume,
    Battery,
}

type ClusterKey = (u64, u64, Option<OpenPopup>, Option<Slot>, [u8; 3], i32);

#[derive(Debug, Default)]
pub(super) struct Indicators {
    dots: [Anim; 3],
    reserved: [bool; 3],
    screen_clients: Vec<String>,
    rev: u64,
    cluster_cache: Cache<ClusterKey>,
    privacy_cache: Cache<(u64, u64, i32)>,
}

impl Indicators {
    pub(super) fn clear(&mut self) {
        self.cluster_cache.clear();
        self.privacy_cache.clear();
    }

    pub(super) fn is_animating(&self) -> bool {
        self.dots.iter().any(Anim::is_animating)
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
pub fn cluster_order(privacy_dots: bool) -> Vec<Slot> {
    let mut order = vec![Slot::Status, Slot::Search];
    if privacy_dots {
        order.push(Slot::Privacy);
    }
    order
}

#[must_use]
pub fn block_indicators(status: &SystemStatus) -> Vec<Indicator> {
    let mut items = vec![Indicator::Wifi];
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
    let mut width = ICON;
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
        Indicator::Wifi | Indicator::Volume | Indicator::Bluetooth | Indicator::Vpn => ICON,
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
            _ => CONTROL_W,
        }
    }

    pub(super) fn layout_status_cluster(
        &self, painter: &crate::paint::Painter, mut right: i32, y: i32,
        slots: &mut Vec<(Slot, Rectangle<i32, Logical>)>,
    ) -> i32 {
        let order = cluster_order(self.indicators.reserved_count() > 0);
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
    };
    let local: Vec<(Slot, i32, i32)> =
        items.iter().map(|(slot, rect)| (*slot, rect.loc.x - left, rect.size.w)).collect();
    let Some(buffer) =
        topbar.indicators.cluster_cache.resolve(key, output_scale, || draw_cluster(&ctx, &local, Some(icons)))
    else {
        return;
    };
    push_buffer(renderer, buffer, Point::from((left, y)), scale, elements);
}

const fn opens(slot: Slot) -> Option<OpenPopup> {
    match slot {
        Slot::Status => Some(OpenPopup::QuickSettings),
        Slot::Privacy => Some(OpenPopup::Privacy),
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

fn battery_icon_names(pct: u8, charging: bool) -> [String; 2] {
    let level = u32::from(pct.min(100)).div_ceil(10) * 10;
    let suffix = match (charging, level) {
        (true, 100) => "-charged",
        (true, _) => "-charging",
        (false, _) => "",
    };
    let legacy = (u32::from(pct.min(100)) + 10) / 20 * 20;
    let legacy_suffix = if charging { "-charging" } else { "" };
    [format!("battery-level-{level}{suffix}"), format!("battery-{legacy:03}{legacy_suffix}")]
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

fn draw_status_battery(c: &mut Canvas<'_>, ctx: &ClusterPaint<'_>, x: i32, fg: Color) {
    let Some(pct) = ctx.status.battery_pct else {
        return;
    };
    let charging = ctx.status.battery_charging;
    let low = !charging && pct <= ctx.theme.battery_low_pct;
    let colour = if low { ctx.theme.danger } else { fg };
    let cell = column(x, ICON);
    let drawn = battery_icon_names(pct, charging).iter().any(|name| themed(c, name, ICON, cell, colour));
    if !drawn {
        svg_glyph(c, &battery_cell(pct), ICON, cell, colour);
    }
    if ctx.theme.battery_percentage {
        let left = x + ICON + PCT_GAP;
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
    fn the_cluster_is_the_status_block_then_search_then_privacy() {
        assert_eq!(cluster_order(true), vec![Slot::Status, Slot::Search, Slot::Privacy]);
        assert_eq!(cluster_order(false), vec![Slot::Status, Slot::Search]);
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
    fn battery_names_round_up_to_the_theme_levels() {
        assert_eq!(battery_icon_names(82, false), ["battery-level-90", "battery-080"]);
        assert_eq!(battery_icon_names(0, true), ["battery-level-0-charging", "battery-000-charging"]);
        assert_eq!(battery_icon_names(100, true), ["battery-level-100-charged", "battery-100-charging"]);
        assert_eq!(battery_icon_names(200, false), ["battery-level-100", "battery-100"]);
    }
}

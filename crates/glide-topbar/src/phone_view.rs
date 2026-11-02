use super::glyphs::{Glyph, glyph};
use super::{
    BUTTON_HEIGHT, HEIGHT, Logical, MemoryRenderBuffer, MemoryRenderBufferRenderElement, OpenPopup, PAD_X, POPUP_GAP,
    POPUP_RADIUS, Point, QS_BATTERY_PCT, Rectangle, Scale, Size, Slot, Topbar, TopbarTheme, hover_colour, local_rect,
    numeric_label_width, push_popup_buffer, push_popup_shadow,
};
use crate::i18n::{Key, t};
use crate::paint::TextAlign;
use crate::phone::{PhonePairState, PhoneState};
use crate::raster::{Canvas, ShellRenderer};
use glide_theme::TextStyle;
use glide_theme::shape::{R_ROUND, R2};
use glide_theme::spacing::{SP1, SP2, SP3};

const APPLET_PAD: i32 = SP2;
const APPLET_ICON: i32 = super::status::ICON;
const APPLET_GAP: i32 = SP1;
const WIDTH: i32 = 304;
const WELL: i32 = 40;
const WELL_ICON: i32 = 20;
const TILE_H: i32 = 58;
const TILE_PAD: i32 = 10;
const TILE_LABEL: i32 = 14;
const TILE_VALUE: i32 = 20;
const LEVEL_H: i32 = 4;
const NOTIF_ROW_H: i32 = 42;
const NOTIF_GAP: i32 = 6;
const MAX_NOTIFS: usize = 3;
const ROW_H: i32 = 34;
const ROW_PAD: i32 = SP2;
const DIVIDER: i32 = SP1 * 2 + 1;
const SUBTITLE: TextStyle = TextStyle { size_px: 11.5, line_height_px: 15.0, ..TextStyle::CAP };

pub(super) type PhoneKey = (u64, Option<Slot>);

#[derive(Debug, Clone, PartialEq, Eq)]
struct Geometry {
    head: Rectangle<i32, Logical>,
    tiles: [Rectangle<i32, Logical>; 2],
    notifs: Vec<Rectangle<i32, Logical>>,
    divider: i32,
    rows: Vec<(Slot, Rectangle<i32, Logical>)>,
    height: i32,
}

fn geometry(phone: &PhoneState) -> Geometry {
    let inner = WIDTH - SP3 * 2;
    let head = local_rect(SP3, SP3, inner, WELL);
    let mut y = head.loc.y + WELL + SP3;
    let half = (inner - SP2) / 2;
    let tiles = [local_rect(SP3, y, half, TILE_H), local_rect(SP3 + half + SP2, y, half, TILE_H)];
    y += TILE_H + SP2;

    let mut notifs = Vec::new();
    let visible_notifs = phone.notifications.len().min(MAX_NOTIFS);
    for index in 0..visible_notifs {
        if index > 0 {
            y += NOTIF_GAP;
        }
        notifs.push(local_rect(SP3, y, inner, NOTIF_ROW_H));
        y += NOTIF_ROW_H;
    }
    if visible_notifs > 0 {
        y += SP2;
    }

    let divider = y + SP1;
    y += DIVIDER;
    let mut rows = Vec::new();
    let mut row = |slot: Slot| {
        rows.push((slot, local_rect(SP1, y, WIDTH - SP1 * 2, ROW_H)));
        y += ROW_H;
    };
    if phone.pair_state == PhonePairState::RequestedByPeer {
        row(Slot::PhoneAcceptPair);
        row(Slot::PhoneRejectPair);
    } else {
        if phone.connected || phone.ringing {
            row(Slot::PhoneRing);
        }
        if !phone.notifications.is_empty() {
            row(Slot::PhoneClearNotifs);
        }
    }
    row(Slot::PhoneOpenApp);
    Geometry { head, tiles, notifs, divider, rows, height: y + SP1 }
}

fn place(anchor: Rectangle<i32, Logical>, output: Rectangle<i32, Logical>, strip_y: i32) -> Point<i32, Logical> {
    let centred = anchor.loc.x + (anchor.size.w - WIDTH) / 2;
    let min_x = output.loc.x + PAD_X;
    let max_x = (output.loc.x + output.size.w - PAD_X - WIDTH).max(min_x);
    Point::from((centred.clamp(min_x, max_x), strip_y + HEIGHT + POPUP_GAP))
}

pub(super) fn applet_width(painter: &crate::paint::Painter, phone: &PhoneState) -> i32 {
    let pct = phone
        .battery
        .map_or(0, |level| APPLET_GAP + numeric_label_width(painter, &format!("{level}%"), &super::status::PCT_STYLE));
    APPLET_PAD * 2 + APPLET_ICON + pct
}

fn low(theme: &TopbarTheme, phone: &PhoneState) -> bool {
    !phone.charging && phone.battery.is_some_and(|level| level <= theme.battery_low_pct)
}

pub(super) fn draw_applet(
    c: &mut Canvas<'_>, painter: &crate::paint::Painter, theme: &TopbarTheme, phone: &PhoneState, x: i32,
    fg: glide_theme::Color,
) {
    let colour = if low(theme, phone) {
        theme.danger
    } else if phone.ringing || phone.pair_state == PhonePairState::RequestedByPeer {
        theme.active_background
    } else {
        fg
    };
    let left = x + APPLET_PAD;
    glyph(c, Glyph::Phone, APPLET_ICON, local_rect(left, 0, APPLET_ICON, BUTTON_HEIGHT), colour);
    if let Some(level) = phone.battery {
        let text = format!("{level}%");
        let style = &super::status::PCT_STYLE;
        let start = left + APPLET_ICON + APPLET_GAP;
        let width = numeric_label_width(painter, &text, style);
        c.text(painter, (start, start + width, 0, BUTTON_HEIGHT), TextAlign::Right, &text, colour, style);
    }
}

impl Topbar {
    pub(super) fn layout_phone_popup(
        &self, slots: &[(Slot, Rectangle<i32, Logical>)], output: Rectangle<i32, Logical>, strip_y: i32,
        popup_slots: &mut Vec<(Slot, Rectangle<i32, Logical>)>,
    ) -> Option<Rectangle<i32, Logical>> {
        if self.shown_popup()? != OpenPopup::Phone {
            return None;
        }
        let phone = self.phone()?;
        let anchor = slots.iter().find(|(slot, _)| *slot == Slot::Phone).map(|(_, rect)| *rect)?;
        let layout = geometry(phone);
        let origin = place(anchor, output, strip_y);
        for (slot, rect) in layout.rows {
            popup_slots.push((slot, Rectangle::new(rect.loc + origin, rect.size)));
        }
        Some(Rectangle::new(origin, Size::from((WIDTH, layout.height))))
    }
}

pub(super) fn push_popup<R>(
    topbar: &mut Topbar, renderer: &mut R, painter: &crate::paint::Painter, icons: &mut crate::icon::IconLoader,
    scale: Scale<f64>, output_scale: i32, elements: &mut Vec<MemoryRenderBufferRenderElement<R>>,
) where
    R: ShellRenderer,
{
    if topbar.shown_popup() != Some(OpenPopup::Phone) {
        return;
    }
    let Some(rect) = topbar.popup_rect else {
        return;
    };
    let Some(phone) = topbar.phone.as_ref() else {
        return;
    };
    let reveal = topbar.popup_progress();
    let hovered = topbar.hovered.filter(|slot| slot.in_phone_popup());
    let key: PhoneKey = (topbar.status_rev, hovered);
    let theme = &topbar.theme;
    let size = rect.size;
    let Some(buffer) = topbar
        .phone_cache
        .resolve(key, output_scale, || draw_popover(painter, theme, phone, hovered, size, output_scale, Some(icons)))
    else {
        return;
    };
    push_popup_buffer(renderer, buffer, rect, rect, reveal, scale, elements);
    push_popup_shadow(topbar, renderer, rect, reveal, scale, output_scale, elements);
}

fn draw_popover(
    painter: &crate::paint::Painter, theme: &TopbarTheme, phone: &PhoneState, hovered: Option<Slot>,
    size: Size<i32, Logical>, scale: i32, icons: Option<&mut crate::icon::IconLoader>,
) -> Option<MemoryRenderBuffer> {
    paint_popover(painter, theme, phone, hovered, size, scale, icons).map(|canvas| canvas.finish())
}

fn paint_popover<'a>(
    painter: &crate::paint::Painter, theme: &TopbarTheme, phone: &PhoneState, hovered: Option<Slot>,
    size: Size<i32, Logical>, scale: i32, icons: Option<&'a mut crate::icon::IconLoader>,
) -> Option<Canvas<'a>> {
    let mut c =
        Canvas::popover((size.w, size.h), scale, POPUP_RADIUS, theme.popover_bg, theme.border)?.with_icons(icons);
    let layout = geometry(phone);

    let head = layout.head;
    let well = local_rect(head.loc.x, head.loc.y, WELL, WELL);
    let (well_fill, well_ink) = if phone.connected {
        (theme.active_background, theme.active_label)
    } else {
        (theme.active_soft, theme.active_soft_label)
    };
    c.fill(well, well_fill, R2);
    glyph(&mut c, Glyph::Phone, WELL_ICON, well, well_ink);
    let text_left = well.loc.x + WELL + SP3;
    let right = head.loc.x + head.size.w;
    let title_h = TextStyle::HEAD.line_height_px as i32;
    let top = head.loc.y + (WELL - title_h - SUBTITLE.line_height_px as i32) / 2;
    c.text(
        painter,
        (text_left, right, top, top + title_h),
        TextAlign::Left,
        &phone.name,
        theme.label,
        &TextStyle::HEAD,
    );

    let status_text = if phone.pair_state == PhonePairState::RequestedByPeer {
        t(Key::PhPairingAsk).to_string()
    } else if phone.connected {
        let charge_state = if phone.battery.is_some() {
            format!(" \u{b7} {}", t(if phone.charging { Key::PhCharging } else { Key::PhDischarging }))
        } else {
            String::new()
        };
        format!("{}{charge_state}", t(Key::PhConnected))
    } else {
        t(Key::PhOffline).to_string()
    };
    let sub_top = top + title_h;
    let sub_band = (text_left, right, sub_top, sub_top + SUBTITLE.line_height_px as i32);
    c.text(painter, sub_band, TextAlign::Left, &status_text, theme.muted, &SUBTITLE);

    let bat_value = phone.battery.map(|level| format!("{level}%"));
    let bat_colour = if low(theme, phone) { theme.danger } else { theme.label };
    draw_tile(&mut c, painter, theme, layout.tiles[0], (Key::HsBattery, bat_value), (phone.battery, bat_colour));

    let notif_count = phone.notifications.len();
    let notif_value = if notif_count == 0 {
        t(Key::PhNone).to_string()
    } else {
        t(Key::PhActiveCount).replace("{n}", &notif_count.to_string())
    };
    draw_tile(&mut c, painter, theme, layout.tiles[1], (Key::Notifications, Some(notif_value)), (None, theme.label));

    for (rect, notif) in layout.notifs.iter().zip(phone.notifications.iter()) {
        c.fill(*rect, theme.track_soft, R2);
        c.stroke(*rect, 1.0, R2, theme.line);
        let left = rect.loc.x + TILE_PAD;
        let right = rect.loc.x + rect.size.w - TILE_PAD;
        let heading = match (notif.app_name.is_empty(), notif.title.is_empty()) {
            (false, false) => format!("{} \u{b7} {}", notif.app_name, notif.title),
            (false, true) => notif.app_name.clone(),
            (true, false) => notif.title.clone(),
            (true, true) => t(Key::Notifications).to_string(),
        };
        c.text(
            painter,
            (left, right, rect.loc.y + 4, rect.loc.y + 20),
            TextAlign::Left,
            &heading,
            theme.label,
            &TextStyle::CAP,
        );
        let body = if notif.body.is_empty() { "\u{2014}" } else { notif.body.as_str() };
        c.text(
            painter,
            (left, right, rect.loc.y + 20, rect.loc.y + rect.size.h - 4),
            TextAlign::Left,
            body,
            theme.muted,
            &SUBTITLE,
        );
    }

    let divider = local_rect(SP3, layout.divider, WIDTH - SP3 * 2, 1);
    c.fill(divider, theme.border, 0.0);
    for (slot, rect) in &layout.rows {
        let label = t(match slot {
            Slot::PhoneAcceptPair => Key::PhAcceptPair,
            Slot::PhoneRejectPair => Key::PhRejectPair,
            Slot::PhoneRing => {
                if phone.ringing {
                    Key::PhStopAlarm
                } else {
                    Key::PhRing
                }
            }
            Slot::PhoneClearNotifs => Key::PhClearNotifs,
            _ => Key::PhOpenApp,
        });
        let band = (rect.loc.x + ROW_PAD, rect.loc.x + rect.size.w - ROW_PAD, rect.loc.y, rect.loc.y + rect.size.h);
        c.text(painter, band, TextAlign::Left, label, hover_colour(theme, hovered == Some(*slot)), &TextStyle::TOPBAR);
    }
    Some(c)
}

fn draw_tile(
    c: &mut Canvas<'_>, painter: &crate::paint::Painter, theme: &TopbarTheme, rect: Rectangle<i32, Logical>,
    (label, value): (Key, Option<String>), (level, colour): (Option<u8>, glide_theme::Color),
) {
    c.fill(rect, theme.track_soft, R2);
    c.stroke(rect, 1.0, R2, theme.line);
    let (left, right) = (rect.loc.x + TILE_PAD, rect.loc.x + rect.size.w - TILE_PAD);
    let mut y = rect.loc.y + SP2;
    c.text(painter, (left, right, y, y + TILE_LABEL), TextAlign::Left, t(label), theme.faint, &TextStyle::CAP);
    y += TILE_LABEL;
    let value = value.unwrap_or_else(|| "\u{2014}".to_string());
    c.text(painter, (left, right, y, y + TILE_VALUE), TextAlign::Left, &value, colour, &QS_BATTERY_PCT);
    if let Some(level) = level {
        let bar_y = rect.loc.y + rect.size.h - SP2 - LEVEL_H;
        let track = local_rect(left, bar_y, right - left, LEVEL_H);
        c.fill(track, theme.fill_2, R_ROUND);
        let filled = (f64::from(track.size.w) * f64::from(level.min(100)) / 100.0).round() as i32;
        if filled > 0 {
            let tint = if colour == theme.label { theme.active_background } else { colour };
            c.fill(local_rect(left, bar_y, filled, LEVEL_H), tint, R_ROUND);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phone::PhoneNotification;

    fn sample_phone() -> PhoneState {
        PhoneState {
            id: "pixel8".into(),
            name: "Pixel 8 Pro".into(),
            connected: true,
            pair_state: PhonePairState::Paired,
            battery: Some(84),
            charging: true,
            ringing: false,
            notifications: Vec::new(),
        }
    }

    #[test]
    fn connected_phone_without_notifications_shows_ring_and_open_app() {
        let layout = geometry(&sample_phone());
        let slots: Vec<Slot> = layout.rows.iter().map(|(slot, _)| *slot).collect();
        assert_eq!(slots, vec![Slot::PhoneRing, Slot::PhoneOpenApp]);
        assert!(layout.notifs.is_empty());
    }

    #[test]
    fn notifications_add_preview_rows_and_clear_action_up_to_three() {
        let mut phone = sample_phone();
        for index in 0..5 {
            phone.notifications.push(PhoneNotification {
                id: index.to_string(),
                app_name: "Telegram".into(),
                title: "Artem".into(),
                body: "Привіт".into(),
            });
        }
        let layout = geometry(&phone);
        let slots: Vec<Slot> = layout.rows.iter().map(|(slot, _)| *slot).collect();
        assert_eq!(layout.notifs.len(), MAX_NOTIFS);
        assert_eq!(slots, vec![Slot::PhoneRing, Slot::PhoneClearNotifs, Slot::PhoneOpenApp]);
    }

    #[test]
    fn peer_pairing_request_replaces_ring_with_accept_and_reject() {
        let mut phone = sample_phone();
        phone.pair_state = PhonePairState::RequestedByPeer;
        let layout = geometry(&phone);
        let slots: Vec<Slot> = layout.rows.iter().map(|(slot, _)| *slot).collect();
        assert_eq!(slots, vec![Slot::PhoneAcceptPair, Slot::PhoneRejectPair, Slot::PhoneOpenApp]);
    }
}

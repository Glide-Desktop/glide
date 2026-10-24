use super::glyphs::{Glyph, glyph, themed};
use super::{
    BUTTON_HEIGHT, Drawer, HEIGHT, Logical, MemoryRenderBuffer, MemoryRenderBufferRenderElement, OpenPopup, PAD_X,
    POPUP_GAP, POPUP_RADIUS, Point, QS_BATTERY_PCT, Rectangle, Scale, Size, Slot, Topbar, TopbarTheme, hover_colour,
    local_rect, numeric_label_width, push_popup_buffer, push_popup_shadow,
};
use crate::headset::{Headset, Mode};
use crate::i18n::{Key, t};
use crate::paint::TextAlign;
use crate::qs_net::AudioOutput;
use crate::raster::{Canvas, ShellRenderer};
use glide_theme::TextStyle;
use glide_theme::shape::{R_ROUND, R2};
use glide_theme::spacing::{SP1, SP2, SP3};

const APPLET_PAD: i32 = SP2;
const APPLET_ICON: i32 = super::status::ICON;
const APPLET_GAP: i32 = SP1;
const WIDTH: i32 = 288;
const WELL: i32 = 40;
const WELL_ICON: i32 = 20;
const TILE_H: i32 = 58;
const TILE_PAD: i32 = 10;
const TILE_LABEL: i32 = 14;
const TILE_VALUE: i32 = 20;
const LEVEL_H: i32 = 4;
const ROW_H: i32 = 34;
const ROW_PAD: i32 = SP2;
const DIVIDER: i32 = SP1 * 2 + 1;
const SUBTITLE: TextStyle = TextStyle { size_px: 11.5, line_height_px: 15.0, ..TextStyle::CAP };

pub(super) type HeadsetKey = (u64, u64, Option<Slot>, u8);

#[derive(Debug, Clone, PartialEq, Eq)]
struct Geometry {
    head: Rectangle<i32, Logical>,
    tiles: [Rectangle<i32, Logical>; 4],
    divider: i32,
    rows: Vec<(Slot, Rectangle<i32, Logical>)>,
    height: i32,
}

fn geometry(offers_output: bool) -> Geometry {
    let inner = WIDTH - SP3 * 2;
    let head = local_rect(SP3, SP3, inner, WELL);
    let mut y = head.loc.y + WELL + SP3;
    let half = (inner - SP2) / 2;
    let mut tiles = [local_rect(0, 0, 0, 0); 4];
    for (index, tile) in tiles.iter_mut().enumerate() {
        let column = i32::from(index % 2 == 1);
        let row = i32::from(index >= 2);
        *tile = local_rect(SP3 + column * (half + SP2), y + row * (TILE_H + SP2), half, TILE_H);
    }
    y += TILE_H * 2 + SP2 + SP2;
    let divider = y + SP1;
    y += DIVIDER;
    let mut rows = Vec::new();
    let mut row = |slot: Slot| {
        rows.push((slot, local_rect(SP1, y, WIDTH - SP1 * 2, ROW_H)));
        y += ROW_H;
    };
    if offers_output {
        row(Slot::HeadsetOutput);
    }
    row(Slot::HeadsetDisconnect);
    row(Slot::HeadsetSettings);
    Geometry { head, tiles, divider, rows, height: y + SP1 }
}

fn place(anchor: Rectangle<i32, Logical>, output: Rectangle<i32, Logical>, strip_y: i32) -> Point<i32, Logical> {
    let centred = anchor.loc.x + (anchor.size.w - WIDTH) / 2;
    let min_x = output.loc.x + PAD_X;
    let max_x = (output.loc.x + output.size.w - PAD_X - WIDTH).max(min_x);
    Point::from((centred.clamp(min_x, max_x), strip_y + HEIGHT + POPUP_GAP))
}

pub(super) fn applet_width(painter: &crate::paint::Painter, headset: &Headset) -> i32 {
    let pct = headset
        .battery
        .map_or(0, |level| APPLET_GAP + numeric_label_width(painter, &format!("{level}%"), &super::status::PCT_STYLE));
    APPLET_PAD * 2 + APPLET_ICON + pct
}

fn low(theme: &TopbarTheme, headset: &Headset) -> bool {
    headset.battery.is_some_and(|level| level <= theme.battery_low_pct)
}

fn headset_glyph(
    c: &mut Canvas<'_>, headset: &Headset, side: i32, cell: Rectangle<i32, Logical>, colour: crate::config::Color,
) {
    if !themed(c, &headset.icon, side, cell, colour) {
        glyph(c, Glyph::Headphones, side, cell, colour);
    }
}

pub(super) fn draw_applet(
    c: &mut Canvas<'_>, painter: &crate::paint::Painter, theme: &TopbarTheme, headset: &Headset, x: i32,
    fg: crate::config::Color,
) {
    let colour = if low(theme, headset) { theme.danger } else { fg };
    let left = x + APPLET_PAD;
    headset_glyph(c, headset, APPLET_ICON, local_rect(left, 0, APPLET_ICON, BUTTON_HEIGHT), colour);
    if let Some(level) = headset.battery {
        let text = format!("{level}%");
        let style = &super::status::PCT_STYLE;
        let start = left + APPLET_ICON + APPLET_GAP;
        let width = numeric_label_width(painter, &text, style);
        c.text(painter, (start, start + width, 0, BUTTON_HEIGHT), TextAlign::Right, &text, colour, style);
    }
}

fn output_for<'a>(panel: &'a super::panels::DetailPanel, headset: &Headset) -> Option<&'a AudioOutput> {
    let mac = headset.mac.to_ascii_uppercase();
    panel.devices(Drawer::Output).iter().find(|output| output.name.to_ascii_uppercase().contains(&mac))
}

impl Topbar {
    #[must_use]
    pub fn headset_output(&self) -> Option<&AudioOutput> {
        output_for(&self.qs_panel, self.headset()?)
    }

    fn offers_output(&self) -> bool {
        self.headset_output().is_some_and(|output| !output.active)
    }

    pub(super) fn layout_headset_popup(
        &self, slots: &[(Slot, Rectangle<i32, Logical>)], output: Rectangle<i32, Logical>, strip_y: i32,
        popup_slots: &mut Vec<(Slot, Rectangle<i32, Logical>)>,
    ) -> Option<Rectangle<i32, Logical>> {
        if self.shown_popup()? != OpenPopup::Headset || self.headset().is_none() {
            return None;
        }
        let anchor = slots.iter().find(|(slot, _)| *slot == Slot::Headset).map(|(_, rect)| *rect)?;
        let geometry = geometry(self.offers_output());
        let origin = place(anchor, output, strip_y);
        for (slot, rect) in geometry.rows {
            popup_slots.push((slot, Rectangle::new(rect.loc + origin, rect.size)));
        }
        Some(Rectangle::new(origin, Size::from((WIDTH, geometry.height))))
    }
}

struct Facts<'a> {
    headset: &'a Headset,
    output: Option<&'a AudioOutput>,
    volume: Option<u8>,
}

fn facts<'a>(
    headsets: &'a [Headset], panel: &'a super::panels::DetailPanel, status: &crate::system_info::SystemStatus,
) -> Option<Facts<'a>> {
    let headset = headsets.last()?;
    let output = output_for(panel, headset);
    let playing_here = output.is_some_and(|output| output.active);
    let volume = headset.volume.or_else(|| playing_here.then_some(status.volume_pct));
    Some(Facts { headset, output, volume })
}

pub(super) fn push_popup<R>(
    topbar: &mut Topbar, renderer: &mut R, painter: &crate::paint::Painter, icons: &mut crate::icon::IconLoader,
    scale: Scale<f64>, output_scale: i32, elements: &mut Vec<MemoryRenderBufferRenderElement<R>>,
) where
    R: ShellRenderer,
{
    if topbar.shown_popup() != Some(OpenPopup::Headset) {
        return;
    }
    let Some(rect) = topbar.popup_rect else {
        return;
    };
    let reveal = topbar.popup_progress();
    let hovered = topbar.hovered.filter(|slot| slot.in_headset_popup());
    let key: HeadsetKey = (topbar.status_rev, topbar.qs_panel.rev, hovered, topbar.system_status.volume_pct);
    let Some(facts) = facts(&topbar.headsets, &topbar.qs_panel, &topbar.system_status) else {
        return;
    };
    let theme = &topbar.theme;
    let size = rect.size;
    let Some(buffer) = topbar
        .headset_cache
        .resolve(key, output_scale, || draw_popover(painter, theme, &facts, hovered, size, output_scale, Some(icons)))
    else {
        return;
    };
    push_popup_buffer(renderer, buffer, rect, rect, reveal, scale, elements);
    push_popup_shadow(topbar, renderer, rect, reveal, scale, output_scale, elements);
}

fn mode_label(mode: Option<Mode>) -> Option<&'static str> {
    mode.map(|mode| {
        t(match mode {
            Mode::Music => Key::HsMusic,
            Mode::Calls => Key::HsCalls,
        })
    })
}

fn draw_popover(
    painter: &crate::paint::Painter, theme: &TopbarTheme, facts: &Facts<'_>, hovered: Option<Slot>,
    size: Size<i32, Logical>, scale: i32, icons: Option<&mut crate::icon::IconLoader>,
) -> Option<MemoryRenderBuffer> {
    paint_popover(painter, theme, facts, hovered, size, scale, icons).map(|canvas| canvas.finish())
}

fn paint_popover<'a>(
    painter: &crate::paint::Painter, theme: &TopbarTheme, facts: &Facts<'_>, hovered: Option<Slot>,
    size: Size<i32, Logical>, scale: i32, icons: Option<&'a mut crate::icon::IconLoader>,
) -> Option<Canvas<'a>> {
    let headset = facts.headset;
    let mut c =
        Canvas::popover((size.w, size.h), scale, POPUP_RADIUS, theme.popover_bg, theme.border)?.with_icons(icons);
    let layout = geometry(facts.output.is_some_and(|output| !output.active));

    let head = layout.head;
    let well = local_rect(head.loc.x, head.loc.y, WELL, WELL);
    c.fill(well, theme.active_soft, R2);
    headset_glyph(&mut c, headset, WELL_ICON, well, theme.active_soft_label);
    let text_left = well.loc.x + WELL + SP3;
    let right = head.loc.x + head.size.w;
    let title_h = TextStyle::HEAD.line_height_px as i32;
    let top = head.loc.y + (WELL - title_h - SUBTITLE.line_height_px as i32) / 2;
    c.text(
        painter,
        (text_left, right, top, top + title_h),
        TextAlign::Left,
        &headset.name,
        theme.label,
        &TextStyle::HEAD,
    );
    let playing_here = facts.output.is_some_and(|output| output.active);
    let subtitle = if playing_here {
        format!("Bluetooth \u{b7} {}", t(Key::HsPlayingHere))
    } else {
        format!("Bluetooth \u{b7} {}", headset.address)
    };
    let sub_top = top + title_h;
    let sub_band = (text_left, right, sub_top, sub_top + SUBTITLE.line_height_px as i32);
    c.text(painter, sub_band, TextAlign::Left, &subtitle, theme.muted, &SUBTITLE);

    let pct = |level: u8| format!("{level}%");
    let tiles = [
        (Key::HsBattery, headset.battery.map(pct)),
        (Key::HsVolume, facts.volume.map(pct)),
        (Key::HsMode, mode_label(headset.profile.mode).map(str::to_string)),
        (Key::HsCodec, headset.profile.codec.clone()),
    ];
    for (index, (rect, (label, value))) in layout.tiles.iter().zip(tiles).enumerate() {
        let colour = if index == 0 && low(theme, headset) { theme.danger } else { theme.label };
        let level = match index {
            0 => headset.battery,
            1 => facts.volume,
            _ => None,
        };
        draw_tile(&mut c, painter, theme, *rect, (label, value), (level, colour));
    }

    let divider = local_rect(SP3, layout.divider, WIDTH - SP3 * 2, 1);
    c.fill(divider, theme.border, 0.0);
    for (slot, rect) in &layout.rows {
        let label = t(match slot {
            Slot::HeadsetOutput => Key::HsUseOutput,
            Slot::HeadsetDisconnect => Key::QsDisconnect,
            _ => Key::QsBluetoothSettings,
        });
        let band = (rect.loc.x + ROW_PAD, rect.loc.x + rect.size.w - ROW_PAD, rect.loc.y, rect.loc.y + rect.size.h);
        c.text(painter, band, TextAlign::Left, label, hover_colour(theme, hovered == Some(*slot)), &TextStyle::TOPBAR);
    }
    Some(c)
}

fn draw_tile(
    c: &mut Canvas<'_>, painter: &crate::paint::Painter, theme: &TopbarTheme, rect: Rectangle<i32, Logical>,
    (label, value): (Key, Option<String>), (level, colour): (Option<u8>, crate::config::Color),
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

    #[test]
    fn the_popover_offers_sound_here_only_when_the_headset_is_not_the_output() {
        let with = geometry(true);
        let without = geometry(false);
        let slots = |geometry: &Geometry| geometry.rows.iter().map(|(slot, _)| *slot).collect::<Vec<_>>();
        assert_eq!(slots(&with), vec![Slot::HeadsetOutput, Slot::HeadsetDisconnect, Slot::HeadsetSettings]);
        assert_eq!(slots(&without), vec![Slot::HeadsetDisconnect, Slot::HeadsetSettings]);
        assert_eq!(with.height - without.height, ROW_H);
    }

    #[test]
    fn the_tiles_fill_two_rows_inside_the_padding_without_overlapping() {
        let layout = geometry(false);
        let [a, b, c, d] = layout.tiles;
        assert_eq!(a.loc.y, b.loc.y);
        assert_eq!(c.loc.y, d.loc.y);
        assert_eq!(a.loc.x, SP3);
        assert_eq!(b.loc.x + b.size.w, WIDTH - SP3);
        assert!(a.loc.x + a.size.w < b.loc.x && a.loc.y + a.size.h < c.loc.y);
        assert!(c.loc.y + c.size.h < layout.divider, "the rows start under the tiles");
        assert!(layout.rows.iter().all(|(_, rect)| rect.loc.y > layout.divider));
    }

    #[test]
    #[ignore = "writes preview PNGs to $GLIDE_HEADSET_PREVIEW"]
    fn preview() {
        let Some(dir) = std::env::var_os("GLIDE_HEADSET_PREVIEW") else {
            return;
        };
        let painter = crate::paint::Painter::new(14.0, None);
        let mut icons = crate::icon::IconLoader::default();
        let mut applet_icons = crate::icon::IconLoader::default();
        let headset = Headset {
            mac: "AA_BB_CC_DD_EE_FF".into(),
            name: "Sony WH-1000XM5".into(),
            icon: "audio-headset".into(),
            battery: Some(64),
            address: "AA:BB:CC:DD:EE:FF".into(),
            volume: Some(48),
            profile: crate::headset::parse_profile("a2dp-sink-ldac"),
        };
        let output = AudioOutput {
            name: "bluez_output.AA_BB_CC_DD_EE_FF.1".into(),
            description: "Sony WH-1000XM5".into(),
            icon: "audio-headset",
            active: false,
        };
        let scale = 2;
        for appearance in [glide_theme::Appearance::Dark, glide_theme::Appearance::Light] {
            let theme = TopbarTheme::from_tokens(&glide_theme::Tokens::new(appearance, glide_theme::Accent::Blue));
            let facts = Facts { headset: &headset, output: Some(&output), volume: headset.volume };
            let height = geometry(true).height;
            let canvas = paint_popover(
                &painter,
                &theme,
                &facts,
                Some(Slot::HeadsetDisconnect),
                Size::from((WIDTH, height)),
                scale,
                Some(&mut icons),
            )
            .expect("canvas");
            let width = applet_width(&painter, &headset);
            let mut applet =
                Canvas::new(width, BUTTON_HEIGHT, scale).expect("applet").with_icons(Some(&mut applet_icons));
            applet.fill(applet.full(), theme.background, 0.0);
            draw_applet(&mut applet, &painter, &theme, &headset, 0, theme.muted);
            for (name, c) in [("popover", &canvas), ("applet", &applet)] {
                let (w, h) = ((c.w * c.scale) as u32, (c.h * c.scale) as u32);
                let image = crate::shot::compose::Image::from_bgra(w, h, c.buf.clone()).expect("image");
                let png = crate::backend::screenshot::save_rgba(&image.into_straight(), w, h, false).expect("png").png;
                let file = format!("headset-{name}-{appearance:?}.png").to_ascii_lowercase();
                std::fs::write(std::path::Path::new(&dir).join(file), png).expect("write");
            }
        }
    }
}

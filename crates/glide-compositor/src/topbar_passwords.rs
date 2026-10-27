use glide_config::passwords::ClickAction;
use glide_ipc::keychain::ItemRef;
use glide_theme::TextStyle;
use glide_theme::shape::{R_ROUND, R1, R2, R3};
use glide_theme::spacing::{SP1, SP2, SP3};

use super::glyphs::svg_glyph;
use super::{
    BUTTON_HEIGHT, HEIGHT, LocalRect, Logical, MemoryRenderBuffer, MemoryRenderBufferRenderElement, OpenPopup, PAD_X,
    POPUP_GAP, Point, Rectangle, Scale, Size, Slot, Topbar, TopbarTheme, local_rect, push_popup_buffer,
    push_popup_shadow,
};
use crate::config::Color;
use crate::i18n::{Key, t};
use crate::launcher::field::TextField;
use crate::paint::{Painter, TextAlign};
use crate::popup_menu::text_px as text_w;
use crate::raster::{Canvas, ShellRenderer};

pub const WIDTH: i32 = 368;
pub const APPLET_W: i32 = 28;
const FIELD_H: i32 = 34;
const HEADING_H: i32 = 28;
const ROW_H: i32 = 48;
const TILE: i32 = 32;
const FOOTER_H: i32 = 34;
const HINT_H: i32 = 36;
const MENU_W: i32 = 264;
const MENU_ROW: i32 = 34;
const MENU_HEAD: i32 = 26;
const MENU_NOTE: i32 = 40;
const EMPTY_H: i32 = 236;
const DOT: i32 = 6;
const BADGE_DOT: i32 = 7;
pub const LIMIT: usize = 6;

const KEY_SVG: &str = "<circle cx=\"8\" cy=\"15\" r=\"4\"/><path d=\"M10.8 12.2 20 3M16 7l3 3M14 9l2 2\"/>";
const LOCK_SVG: &str =
    "<rect x=\"5\" y=\"11\" width=\"14\" height=\"10\" rx=\"2\"/><path d=\"M8 11V7a4 4 0 0 1 8 0v4\"/>";
const SEARCH_SVG: &str = "<circle cx=\"11\" cy=\"11\" r=\"7\"/><path d=\"m20 20-3.5-3.5\"/>";
const WAND_SVG: &str = "<path d=\"M4 20 15 9M14 4v3M12.5 5.5h3M19 9v3M17.5 10.5h3M8 3v2M7 4h2\"/>";
const EXTERNAL_SVG: &str = "<path d=\"M14 4h6v6M20 4l-9 9M18 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h5\"/>";
const CHECK_SVG: &str = "<path d=\"m5 12 5 5 9-10\"/>";
const COPY_SVG: &str =
    "<rect x=\"9\" y=\"9\" width=\"11\" height=\"11\" rx=\"2\"/><path d=\"M5 15V6a2 2 0 0 1 2-2h8\"/>";

const NAME: TextStyle = TextStyle::HEAD;
const SUB: TextStyle = TextStyle::CAP;
const MICRO: TextStyle = TextStyle::MICRO;
const ITEM: TextStyle = TextStyle::MENU_ITEM;
const KEYCAP: TextStyle =
    TextStyle { size_px: 10.5, line_height_px: 14.0, weight: 500, tracking_em: 0.0, uppercase: false, tabular: true };

fn svg(body: &str) -> String {
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" \
         stroke=\"#000\" stroke-width=\"1.8\" stroke-linecap=\"round\" stroke-linejoin=\"round\">{body}</svg>"
    )
}

fn icon(c: &mut Canvas<'_>, body: &str, side: i32, rect: LocalRect, colour: Color) {
    svg_glyph(c, &svg(body), side, rect, colour);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Phase {
    #[default]
    Loading,
    Open,
    Locked,
    Empty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Dot {
    Synced,
    Pending,
    Local,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Row {
    pub item: ItemRef,
    pub name: String,
    pub sub: String,
    pub dot: Dot,
    pub username: String,
    pub has_totp: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MenuAction {
    Password,
    Username,
    Both,
    Code,
    Copy,
    Open,
}

impl MenuAction {
    #[must_use]
    pub const fn of(click: ClickAction) -> Self {
        match click {
            ClickAction::Password => Self::Password,
            ClickAction::Username => Self::Username,
            ClickAction::Both => Self::Both,
            ClickAction::Code => Self::Code,
        }
    }

    const fn key(self) -> Key {
        match self {
            Self::Password => Key::PwInsertPassword,
            Self::Username => Key::PwInsertUsername,
            Self::Both => Key::PwInsertBoth,
            Self::Code => Key::PwInsertCode,
            Self::Copy => Key::PwCopyPassword,
            Self::Open => Key::PwOpenInApp,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Applet {
    pub phase: Phase,
    pub field: TextField,
    pub app: String,
    pub fitting: Vec<Row>,
    pub found: Vec<Row>,
    pub selected: usize,
    pub menu: Option<usize>,
    pub menu_selected: usize,
    pub code: Option<String>,
    pub click: ClickAction,
    pub locked: bool,
    pub offer: bool,
    pub rev: u64,
}

impl Applet {
    #[must_use]
    pub fn searching(&self) -> bool {
        !self.field.text().trim().is_empty()
    }

    #[must_use]
    pub fn sections(&self) -> Vec<(String, &[Row])> {
        if self.searching() {
            return vec![(t(Key::PwFound).replace("{n}", &self.found.len().to_string()), &self.found)];
        }
        let mut sections = Vec::new();
        if !self.fitting.is_empty() {
            sections.push((t(Key::PwForApp).replace("{app}", &self.app), self.fitting.as_slice()));
        }
        if !self.found.is_empty() {
            sections.push((t(Key::PwRecent).to_string(), self.found.as_slice()));
        }
        sections
    }

    #[must_use]
    pub fn rows(&self) -> Vec<&Row> {
        self.sections().into_iter().flat_map(|(_, rows)| rows.iter()).collect()
    }

    #[must_use]
    pub fn row(&self, index: usize) -> Option<&Row> {
        self.rows().get(index).copied()
    }

    #[must_use]
    pub fn menu_actions(&self, index: usize) -> Vec<MenuAction> {
        let mut actions = vec![MenuAction::Password, MenuAction::Username, MenuAction::Both];
        if self.row(index).is_some_and(|row| row.has_totp) {
            actions.push(MenuAction::Code);
        }
        actions.extend([MenuAction::Copy, MenuAction::Open]);
        actions
    }

    pub fn step(&mut self, delta: i32) -> bool {
        if let Some(index) = self.menu {
            let count = self.menu_actions(index).len() as i32;
            let next = (self.menu_selected as i32 + delta).rem_euclid(count.max(1));
            self.menu_selected = next as usize;
            self.rev += 1;
            return true;
        }
        let count = self.rows().len() as i32;
        if count == 0 {
            return false;
        }
        self.selected = (self.selected as i32 + delta).rem_euclid(count) as usize;
        self.rev += 1;
        true
    }

    pub fn touch(&mut self) {
        self.rev += 1;
        let count = self.rows().len();
        if self.selected >= count {
            self.selected = count.saturating_sub(1);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Geometry {
    height: i32,
    field: LocalRect,
    headings: Vec<(String, i32)>,
    rows: Vec<LocalRect>,
    footer: [LocalRect; 2],
    divider: i32,
    hint: LocalRect,
    button: Option<LocalRect>,
    second: Option<LocalRect>,
}

fn geometry(applet: &Applet) -> Geometry {
    let field = local_rect(SP3, SP3, WIDTH - SP3 * 2, FIELD_H);
    let mut geometry = Geometry {
        height: EMPTY_H,
        field,
        headings: Vec::new(),
        rows: Vec::new(),
        footer: [local_rect(0, 0, 0, 0); 2],
        divider: 0,
        hint: local_rect(0, 0, 0, 0),
        button: None,
        second: None,
    };
    match applet.phase {
        Phase::Locked => {
            geometry.button = Some(local_rect(SP5_X, 160, WIDTH - SP5_X * 2, 34));
            return geometry;
        }
        Phase::Empty => {
            let half = (WIDTH - SP5_X * 2 - SP2) / 2;
            geometry.second = Some(local_rect(SP5_X, 168, half, 34));
            geometry.button = Some(local_rect(SP5_X + half + SP2, 168, half, 34));
            return geometry;
        }
        Phase::Loading | Phase::Open => {}
    }
    let mut y = SP3 + FIELD_H + SP2;
    for (title, rows) in applet.sections() {
        geometry.headings.push((title, y));
        y += HEADING_H;
        for _ in rows {
            geometry.rows.push(local_rect(SP1, y, WIDTH - SP1 * 2, ROW_H));
            y += ROW_H;
        }
    }
    if applet.phase == Phase::Open && applet.rows().is_empty() {
        geometry.headings.push((t(Key::PwNothing).to_string(), y));
        y += HEADING_H;
    }
    y += SP1;
    geometry.divider = y;
    y += 1 + SP1;
    let half = (WIDTH - SP1 * 2 - SP1) / 2;
    geometry.footer = [local_rect(SP1, y, half, FOOTER_H), local_rect(SP1 * 2 + half, y, half, FOOTER_H)];
    y += FOOTER_H + SP1;
    geometry.hint = local_rect(0, y, WIDTH, HINT_H);
    geometry.height = y + HINT_H;
    geometry
}

const SP5_X: i32 = 24;

fn grouped_code(code: &str) -> String {
    let digits: Vec<char> = code.chars().collect();
    if digits.len() < 6 || !digits.len().is_multiple_of(2) || !digits.iter().all(char::is_ascii_digit) {
        return code.to_string();
    }
    let (head, tail) = digits.split_at(digits.len() / 2);
    format!("{} {}", head.iter().collect::<String>(), tail.iter().collect::<String>())
}

fn menu_geometry(applet: &Applet, index: usize) -> (i32, Vec<LocalRect>) {
    let count = applet.menu_actions(index).len() as i32;
    let mut y = SP1 + MENU_HEAD;
    let mut rows = Vec::new();
    for position in 0..count {
        if position == count - 2 {
            y += 1 + SP1 * 2;
        }
        rows.push(local_rect(SP1, y, MENU_W - SP1 * 2, MENU_ROW));
        y += MENU_ROW;
    }
    y += 1 + SP1 * 2 + MENU_NOTE;
    (y, rows)
}

fn place(anchor: Rectangle<i32, Logical>, output: Rectangle<i32, Logical>, strip_y: i32) -> Point<i32, Logical> {
    let right_aligned = anchor.loc.x + anchor.size.w - WIDTH + 44;
    let min_x = output.loc.x + PAD_X;
    let max_x = (output.loc.x + output.size.w - PAD_X - WIDTH).max(min_x);
    Point::from((right_aligned.clamp(min_x, max_x), strip_y + HEIGHT + POPUP_GAP))
}

pub(super) fn draw_applet(c: &mut Canvas<'_>, theme: &TopbarTheme, applet: &Applet, x: i32, fg: Color) {
    let cell = local_rect(x, 0, APPLET_W, BUTTON_HEIGHT);
    icon(c, KEY_SVG, super::status::ICON, cell, fg);
    let dot = if applet.locked {
        Some(theme.faint)
    } else if applet.offer {
        Some(theme.active_background)
    } else {
        None
    };
    if let Some(colour) = dot {
        let rect = local_rect(x + APPLET_W - 4 - BADGE_DOT, BUTTON_HEIGHT - 5 - BADGE_DOT, BADGE_DOT, BADGE_DOT);
        c.fill(local_rect(rect.loc.x - 1, rect.loc.y - 1, BADGE_DOT + 2, BADGE_DOT + 2), theme.background, R_ROUND);
        c.fill(rect, colour, R_ROUND);
    }
}

impl super::TopbarAction {
    #[must_use]
    pub const fn for_passwords(self) -> bool {
        matches!(
            self,
            Self::TogglePasswords
                | Self::PasswordsField
                | Self::PasswordsRow(_)
                | Self::PasswordsMenu(_)
                | Self::PasswordsGenerate
                | Self::PasswordsOpen
                | Self::PasswordsUnlock
                | Self::PasswordsCreate
                | Self::PasswordsConnect
        )
    }
}

impl Topbar {
    #[must_use]
    pub const fn passwords(&self) -> &Applet {
        &self.passwords
    }

    pub fn passwords_mut(&mut self) -> &mut Applet {
        self.touch();
        &mut self.passwords
    }

    pub(super) fn layout_passwords_popup(
        &mut self, slots: &[(Slot, Rectangle<i32, Logical>)], output: Rectangle<i32, Logical>, strip_y: i32,
        popup_slots: &mut Vec<(Slot, Rectangle<i32, Logical>)>,
    ) -> Option<Rectangle<i32, Logical>> {
        self.passwords_menu_rect = None;
        self.passwords_rect = None;
        if self.shown_popup()? != OpenPopup::Passwords {
            return None;
        }
        let anchor = slots.iter().find(|(slot, _)| *slot == Slot::Passwords).map(|(_, rect)| *rect)?;
        let origin = place(anchor, output, strip_y);
        let geometry = geometry(&self.passwords);
        let at = |rect: LocalRect| Rectangle::new(rect.loc + origin, rect.size);
        match self.passwords.phase {
            Phase::Locked => {
                if let Some(button) = geometry.button {
                    popup_slots.push((Slot::PwUnlock, at(button)));
                }
            }
            Phase::Empty => {
                if let (Some(first), Some(second)) = (geometry.button, geometry.second) {
                    popup_slots.push((Slot::PwCreate, at(first)));
                    popup_slots.push((Slot::PwConnect, at(second)));
                }
            }
            Phase::Loading | Phase::Open => {
                popup_slots.push((Slot::PwField, at(geometry.field)));
                for (index, rect) in geometry.rows.iter().enumerate() {
                    popup_slots.push((Slot::PwRow(index as u8), at(*rect)));
                }
                popup_slots.push((Slot::PwGenerate, at(geometry.footer[0])));
                popup_slots.push((Slot::PwOpen, at(geometry.footer[1])));
            }
        }
        if let Some(index) = self.passwords.menu
            && let Some(row) = geometry.rows.get(index)
        {
            let (height, rows) = menu_geometry(&self.passwords, index);
            let menu_origin = Point::from((origin.x - MENU_W - SP2, origin.y + row.loc.y - SP1));
            for (position, rect) in rows.into_iter().enumerate() {
                popup_slots.push((Slot::PwMenu(position as u8), Rectangle::new(rect.loc + menu_origin, rect.size)));
            }
            self.passwords_menu_rect = Some(Rectangle::new(menu_origin, Size::from((MENU_W, height))));
        }
        let rect = Rectangle::new(origin, Size::from((WIDTH, geometry.height)));
        self.passwords_rect = Some(rect);
        Some(rect)
    }

    #[must_use]
    pub(super) fn passwords_mask(&self) -> Vec<LocalRect> {
        if self.shown_popup() != Some(OpenPopup::Passwords) {
            return Vec::new();
        }
        self.passwords_rect.into_iter().chain(self.passwords_menu_rect).collect()
    }
}

pub(super) type PasswordsKey = (u64, Option<Slot>, String, u64);

pub(super) fn push_popup<R>(
    topbar: &mut Topbar, renderer: &mut R, painter: &Painter, icons: &mut crate::icon::IconLoader, scale: Scale<f64>,
    output_scale: i32, elements: &mut Vec<MemoryRenderBufferRenderElement<R>>,
) where
    R: ShellRenderer,
{
    if topbar.shown_popup() != Some(OpenPopup::Passwords) {
        return;
    }
    let Some(rect) = topbar.popup_rect else {
        return;
    };
    let reveal = topbar.popup_progress();
    let hovered = topbar.hovered.filter(|slot| slot.in_passwords_popup());
    let key: PasswordsKey =
        (topbar.passwords.rev, hovered, topbar.passwords.field.text().to_string(), topbar.status_rev);
    let menu_rect = topbar.passwords_menu_rect;
    let theme = topbar.theme.clone();
    let applet = topbar.passwords.clone();
    if let Some(menu) = menu_rect
        && let Some(index) = applet.menu
        && let Some(buffer) =
            topbar.passwords_menu_cache.resolve((key.0, None, key.2.clone(), key.3), output_scale, || {
                draw_menu(painter, &theme, &applet, index, menu.size, output_scale)
            })
    {
        push_popup_buffer(renderer, buffer, menu, menu, reveal, scale, elements);
        push_popup_shadow(topbar, renderer, menu, reveal, scale, output_scale, elements);
    }
    let size = rect.size;
    let Some(buffer) = topbar.passwords_cache.resolve(key, output_scale, || {
        draw_popover(painter, &theme, &applet, hovered, size, output_scale, Some(icons))
    }) else {
        return;
    };
    push_popup_buffer(renderer, buffer, rect, rect, reveal, scale, elements);
    push_popup_shadow(topbar, renderer, rect, reveal, scale, output_scale, elements);
}

fn keycap(c: &mut Canvas<'_>, painter: &Painter, theme: &TopbarTheme, label: &str, x: i32, centre_y: i32) -> i32 {
    let width = (text_w(painter, label, &KEYCAP) + 10).max(20);
    let rect = local_rect(x, centre_y - 10, width, 20);
    c.fill(rect, theme.fill_2, R1);
    c.stroke(rect, 1.0, R1, theme.line);
    c.text(painter, (x, x + width, rect.loc.y, rect.loc.y + 20), TextAlign::Center, label, theme.muted, &KEYCAP);
    width
}

fn letter(name: &str) -> String {
    name.chars().find(|ch| ch.is_alphanumeric()).map_or_else(|| "?".to_string(), |ch| ch.to_uppercase().collect())
}

fn draw_field(c: &mut Canvas<'_>, painter: &Painter, theme: &TopbarTheme, applet: &Applet, rect: LocalRect) {
    let halo = local_rect(rect.loc.x - 3, rect.loc.y - 3, rect.size.w + 6, rect.size.h + 6);
    c.fill(halo, theme.acc_ring, R2 + 3.0);
    c.fill(rect, theme.view_bg, R2);
    c.stroke(rect, 1.0, R2, theme.active_background);
    let lead = local_rect(rect.loc.x + 10, rect.loc.y, 16, rect.size.h);
    icon(c, SEARCH_SVG, 16, lead, theme.faint);
    let chip_label = t(Key::PwAllVaults);
    let chip_w = text_w(painter, chip_label, &SUB) + 12;
    let chip = local_rect(rect.loc.x + rect.size.w - 7 - chip_w, rect.loc.y + (rect.size.h - 20) / 2, chip_w, 20);
    c.fill(chip, theme.fill_2, R1);
    c.text(
        painter,
        (chip.loc.x, chip.loc.x + chip_w, chip.loc.y, chip.loc.y + 20),
        TextAlign::Center,
        chip_label,
        theme.muted,
        &SUB,
    );
    let left = lead.loc.x + 16 + SP2;
    let right = chip.loc.x - SP2;
    let band = (left, right, rect.loc.y, rect.loc.y + rect.size.h);
    let style = TextStyle::CALL;
    let text = applet.field.text();
    if text.is_empty() {
        c.text(painter, band, TextAlign::Left, t(Key::PwSearch), theme.faint, &style);
    } else {
        if applet.field.selection().is_some() {
            let width = text_w(painter, text, &style).min(right - left);
            c.fill(local_rect(left, rect.loc.y + 7, width, rect.size.h - 14), theme.active_soft, 0.0);
        }
        c.text(painter, band, TextAlign::Left, text, theme.label, &style);
    }
    let before = &text[..applet.field.caret()];
    let caret_x = (left + text_w(painter, before, &style)).min(right);
    c.fill(local_rect(caret_x, rect.loc.y + 8, 1, rect.size.h - 16), theme.active_background, 0.0);
}

#[allow(clippy::too_many_lines)] // The popover is drawn top to bottom, one block per part of the design
fn paint_popover<'a>(
    painter: &Painter, theme: &TopbarTheme, applet: &Applet, hovered: Option<Slot>, size: Size<i32, Logical>,
    scale: i32, icons: Option<&'a mut crate::icon::IconLoader>,
) -> Option<Canvas<'a>> {
    let mut c = Canvas::popover((size.w, size.h), scale, R3, theme.popover_bg, theme.border)?.with_icons(icons);
    let geometry = geometry(applet);
    match applet.phase {
        Phase::Locked | Phase::Empty => {
            let tile = local_rect((WIDTH - 40) / 2, 32, 40, 40);
            let (well, ink, body) = if applet.phase == Phase::Locked {
                (theme.active_soft, theme.active_soft_label, LOCK_SVG)
            } else {
                (theme.fill_2, theme.muted, KEY_SVG)
            };
            c.fill(tile, well, R2);
            icon(&mut c, body, 20, tile, ink);
            let (heading, text) = if applet.phase == Phase::Locked {
                (t(Key::PwLockedTitle).to_string(), t(Key::PwLockedBody).replace("{app}", &applet.app))
            } else {
                (t(Key::PwEmptyTitle).to_string(), t(Key::PwEmptyBody).to_string())
            };
            c.text(painter, (SP5_X, WIDTH - SP5_X, 84, 104), TextAlign::Center, &heading, theme.label, &NAME);
            let lines = crate::notifications::wrap_lines(painter, &text, &TextStyle::CALL, WIDTH - SP5_X * 2, 3);
            for (index, line) in lines.iter().enumerate() {
                let y = 108 + index as i32 * 19;
                c.text(
                    painter,
                    (SP5_X, WIDTH - SP5_X, y, y + 19),
                    TextAlign::Center,
                    line,
                    theme.muted,
                    &TextStyle::CALL,
                );
            }
            if let Some(button) = geometry.button {
                let hover = hovered.is_some_and(|slot| matches!(slot, Slot::PwUnlock | Slot::PwCreate));
                c.fill(button, if hover { theme.focus_line } else { theme.active_background }, R2);
                let label = if applet.phase == Phase::Locked { t(Key::PwUnlock) } else { t(Key::PwCreate) };
                let band = (button.loc.x, button.loc.x + button.size.w, button.loc.y, button.loc.y + button.size.h);
                c.text(painter, band, TextAlign::Center, label, theme.active_label, &ITEM);
            }
            if let Some(second) = geometry.second {
                c.stroke(second, 1.0, R2, theme.line_2);
                let band = (second.loc.x, second.loc.x + second.size.w, second.loc.y, second.loc.y + second.size.h);
                c.text(painter, band, TextAlign::Center, t(Key::PwConnect), theme.label, &ITEM);
            }
            if applet.phase == Phase::Locked {
                c.text(painter, (0, WIDTH, 200, 216), TextAlign::Center, t(Key::PwPromptAbove), theme.faint, &SUB);
            }
            return Some(c);
        }
        Phase::Loading | Phase::Open => {}
    }
    draw_field(&mut c, painter, theme, applet, geometry.field);
    for (title, y) in &geometry.headings {
        c.text(painter, (SP3 + 6, WIDTH - SP3, y + 8, y + 22), TextAlign::Left, title, theme.faint, &MICRO);
    }
    let rows = applet.rows();
    let action = MenuAction::of(applet.click);
    for (index, (rect, row)) in geometry.rows.iter().zip(&rows).enumerate() {
        let selected = applet.selected == index || applet.menu == Some(index);
        if selected {
            c.fill(*rect, theme.fill_2, R2);
        }
        let tile = local_rect(rect.loc.x + SP2 + 2, rect.loc.y + (ROW_H - TILE) / 2, TILE, TILE);
        let (well, ink) =
            if selected { (theme.active_soft, theme.active_soft_label) } else { (theme.fill_2, theme.muted) };
        c.fill(tile, well, R2);
        c.text(
            painter,
            (tile.loc.x, tile.loc.x + TILE, tile.loc.y, tile.loc.y + TILE),
            TextAlign::Center,
            &letter(&row.name),
            ink,
            &NAME,
        );
        let left = tile.loc.x + TILE + SP3;
        let mut right = rect.loc.x + rect.size.w - SP3;
        if selected {
            let label = t(action.key());
            let cap_w = 20;
            let label_w = text_w(painter, label, &SUB);
            let cap_x = right - cap_w;
            keycap(&mut c, painter, theme, "↵", cap_x, rect.loc.y + ROW_H / 2);
            c.text(
                painter,
                (cap_x - 6 - label_w, cap_x - 6, rect.loc.y, rect.loc.y + ROW_H),
                TextAlign::Right,
                label,
                theme.muted,
                &SUB,
            );
            right = cap_x - 6 - label_w - SP2;
        } else {
            let colour = match row.dot {
                Dot::Synced => theme.ok,
                Dot::Pending => theme.privacy_mic,
                Dot::Local => theme.faint,
            };
            c.fill(local_rect(right - DOT, rect.loc.y + (ROW_H - DOT) / 2, DOT, DOT), colour, R_ROUND);
            right -= DOT + SP2;
        }
        let top = rect.loc.y + (ROW_H - 36) / 2;
        let query = applet.field.text().trim();
        let name_band = (left, right, top, top + 20);
        if let Some(start) = (!query.is_empty()).then(|| row.name.to_lowercase().find(&query.to_lowercase())).flatten()
            && start == 0
            && row.name.is_char_boundary(query.len())
        {
            let (head, tail) = row.name.split_at(query.len());
            let bold = TextStyle { weight: 760, ..NAME };
            let head_w = text_w(painter, head, &bold);
            c.text(painter, (left, right, top, top + 20), TextAlign::Left, head, theme.label, &bold);
            c.text(painter, (left + head_w, right, top, top + 20), TextAlign::Left, tail, theme.label, &NAME);
        } else {
            c.text(painter, name_band, TextAlign::Left, &row.name, theme.label, &NAME);
        }
        c.text(painter, (left, right, top + 20, top + 36), TextAlign::Left, &row.sub, theme.faint, &SUB);
    }
    c.fill(local_rect(0, geometry.divider, WIDTH, 1), theme.line, 0.0);
    for (rect, (body, key)) in geometry.footer.iter().zip([(WAND_SVG, Key::PwGenerate), (EXTERNAL_SVG, Key::PwOpenApp)])
    {
        icon(&mut c, body, 16, local_rect(rect.loc.x + 10, rect.loc.y, 16, rect.size.h), theme.faint);
        let band = (rect.loc.x + 34, rect.loc.x + rect.size.w, rect.loc.y, rect.loc.y + rect.size.h);
        c.text(painter, band, TextAlign::Left, t(key), theme.label, &ITEM);
    }
    let hint = geometry.hint;
    c.fill(hint, theme.fill_2.with_alpha(theme.fill_2.0[3] * 0.55), 0.0);
    let mut x = SP3 + 2;
    let centre = hint.loc.y + HINT_H / 2;
    for (cap, key) in [("↵", None), ("→", Some(Key::PwMoreActions)), ("Esc", Some(Key::PwClose))] {
        let width = keycap(&mut c, painter, theme, cap, x, centre);
        x += width + 6;
        let label = key.map_or_else(|| lowercase_first(t(action.key())), |key| t(key).to_string());
        let label_w = text_w(painter, &label, &SUB);
        c.text(painter, (x, x + label_w, hint.loc.y, hint.loc.y + HINT_H), TextAlign::Left, &label, theme.faint, &SUB);
        x += label_w + SP3;
    }
    Some(c)
}

fn lowercase_first(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| first.to_lowercase().chain(chars).collect())
}

fn draw_popover(
    painter: &Painter, theme: &TopbarTheme, applet: &Applet, hovered: Option<Slot>, size: Size<i32, Logical>,
    scale: i32, icons: Option<&mut crate::icon::IconLoader>,
) -> Option<MemoryRenderBuffer> {
    paint_popover(painter, theme, applet, hovered, size, scale, icons).map(|canvas| canvas.finish())
}

fn draw_menu(
    painter: &Painter, theme: &TopbarTheme, applet: &Applet, index: usize, size: Size<i32, Logical>, scale: i32,
) -> Option<MemoryRenderBuffer> {
    menu_canvas(painter, theme, applet, index, size, scale).map(|c| c.finish())
}

fn menu_canvas(
    painter: &Painter, theme: &TopbarTheme, applet: &Applet, index: usize, size: Size<i32, Logical>, scale: i32,
) -> Option<Canvas<'static>> {
    let mut c = Canvas::popover((size.w, size.h), scale, R3, theme.popover_bg, theme.border)?;
    let row = applet.row(index)?;
    let head = if row.username.is_empty() { row.name.clone() } else { format!("{} \u{b7} {}", row.name, row.username) };
    c.text(painter, (SP3 - 2, MENU_W - SP3, SP1 + 6, SP1 + 22), TextAlign::Left, &head, theme.faint, &SUB);
    let (_, rows) = menu_geometry(applet, index);
    let actions = applet.menu_actions(index);
    let default = MenuAction::of(applet.click);
    for (position, (rect, action)) in rows.iter().zip(&actions).enumerate() {
        if position == actions.len() - 2 {
            c.fill(local_rect(0, rect.loc.y - SP1 - 1, MENU_W, 1), theme.line, 0.0);
        }
        if applet.menu_selected == position {
            c.fill(*rect, theme.fill_2, R2);
        }
        let lead = local_rect(rect.loc.x + 10, rect.loc.y, 16, rect.size.h);
        match action {
            _ if *action == default => icon(&mut c, CHECK_SVG, 16, lead, theme.active_background),
            MenuAction::Copy => icon(&mut c, COPY_SVG, 16, lead, theme.faint),
            MenuAction::Open => icon(&mut c, EXTERNAL_SVG, 16, lead, theme.faint),
            _ => {}
        }
        let band = (rect.loc.x + 36, rect.loc.x + rect.size.w - 10, rect.loc.y, rect.loc.y + rect.size.h);
        c.text(painter, band, TextAlign::Left, t(action.key()), theme.label, &ITEM);
        if *action == default {
            keycap(&mut c, painter, theme, "↵", rect.loc.x + rect.size.w - 30, rect.loc.y + MENU_ROW / 2);
        } else if *action == MenuAction::Code
            && let Some(code) = &applet.code
        {
            let style = TextStyle { tabular: true, ..SUB };
            c.text(painter, band, TextAlign::Right, &grouped_code(code), theme.faint, &style);
        }
    }
    let note_y = size.h - SP1 - MENU_NOTE;
    c.fill(local_rect(0, note_y - SP1, MENU_W, 1), theme.line, 0.0);
    let line = SUB.line_height_px as i32;
    let lines = crate::notifications::wrap_lines(painter, t(Key::PwDefaultNote), &SUB, MENU_W - SP2 - SP3 + 2, 2);
    let mut y = note_y + (MENU_NOTE - line * lines.len() as i32) / 2;
    for text in &lines {
        c.text(painter, (SP3 - 2, MENU_W - SP2, y, y + line), TextAlign::Left, text, theme.faint, &SUB);
        y += line;
    }
    Some(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_split_in_halves_like_the_canvas() {
        assert_eq!(grouped_code("482913"), "482 913");
        assert_eq!(grouped_code("12345678"), "1234 5678");
        assert_eq!(grouped_code("12345"), "12345");
        assert_eq!(grouped_code("abcdef"), "abcdef");
    }

    fn design_row(name: &str, sub: &str, dot: Dot, user: &str, totp: bool) -> Row {
        Row {
            item: ItemRef::new("local", name),
            name: name.into(),
            sub: sub.into(),
            dot,
            username: user.into(),
            has_totp: totp,
        }
    }

    fn write_png(dir: &std::ffi::OsStr, name: &str, buffer: Vec<u8>, size: (i32, i32), scale: i32, dark: bool) {
        let width = (size.0 * scale) as u32 + 64;
        let height = (size.1 * scale) as u32 + 64;
        let mut image = crate::shot::compose::Image::new(width, height).expect("image");
        let shade = if dark { 20 } else { 230 };
        let backdrop = vec![shade; (width * height * 4) as usize];
        image.draw(&crate::shot::compose::Image::from_bgra(width, height, backdrop).expect("bg"), 0, 0);
        let drawn = crate::shot::compose::Image::from_bgra((size.0 * scale) as u32, (size.1 * scale) as u32, buffer)
            .expect("p");
        image.draw(&drawn, 32, 32);
        let png = crate::backend::screenshot::save_rgba(&image.into_straight(), width, height, false).expect("png").png;
        std::fs::write(std::path::Path::new(dir).join(name), png).expect("write");
    }

    #[test]
    #[ignore = "writes preview PNGs to $GLIDE_PASSWORDS_PREVIEW"]
    fn preview() {
        let Some(dir) = std::env::var_os("GLIDE_PASSWORDS_PREVIEW") else {
            return;
        };
        let painter = Painter::new(14.0, None);
        let scale = 2;
        let open = Applet {
            phase: Phase::Open,
            app: "Telegram Desktop".into(),
            fitting: vec![design_row("Telegram Desktop", "@artem_k · Особисте", Dot::Local, "@artem_k", false)],
            found: vec![
                design_row("GitHub", "artem-dev · Bitwarden", Dot::Synced, "artem-dev", true),
                design_row("Google", "artem@example.com · Bitwarden", Dot::Synced, "artem@example.com", true),
                design_row("Nextcloud", "artem · Bitwarden · дім", Dot::Pending, "artem", false),
            ],
            ..Applet::default()
        };
        let mut search = Applet {
            found: vec![
                design_row("GitHub", "artem-dev · Bitwarden · є код", Dot::Synced, "artem-dev", true),
                design_row("GitLab", "artem · Bitwarden · дім", Dot::Pending, "artem", false),
                design_row("Gitea", "admin · Особисте", Dot::Local, "admin", false),
            ],
            menu: Some(0),
            code: Some("482913".into()),
            ..open.clone()
        };
        for ch in "git".chars() {
            search.field.insert(ch);
        }
        let locked = Applet { phase: Phase::Locked, locked: true, ..open.clone() };
        let empty = Applet { phase: Phase::Empty, ..open.clone() };
        for appearance in [glide_theme::Appearance::Light, glide_theme::Appearance::Dark] {
            let dark = appearance == glide_theme::Appearance::Dark;
            let theme = TopbarTheme::from_tokens(&glide_theme::Tokens::new(appearance, glide_theme::Accent::Blue));
            for (name, applet) in [("open", &open), ("search", &search), ("locked", &locked), ("empty", &empty)] {
                let size = Size::from((WIDTH, geometry(applet).height));
                let buffer = paint_popover(&painter, &theme, applet, None, size, scale, None).expect("popover");
                let file = format!("passwords-{name}-{appearance:?}.png").to_lowercase();
                write_png(&dir, &file, buffer.buf.clone(), (size.w, size.h), scale, dark);
                if let Some(index) = applet.menu {
                    let (height, _) = menu_geometry(applet, index);
                    let size = Size::from((MENU_W, height));
                    let menu = menu_canvas(&painter, &theme, applet, index, size, scale).expect("menu");
                    let file = format!("passwords-menu-{appearance:?}.png").to_lowercase();
                    write_png(&dir, &file, menu.buf.clone(), (size.w, size.h), scale, dark);
                }
            }
        }
    }

    fn row(name: &str, totp: bool) -> Row {
        Row {
            item: ItemRef::new("local", name),
            name: name.into(),
            sub: String::new(),
            dot: Dot::Local,
            username: "me".into(),
            has_totp: totp,
        }
    }

    #[test]
    fn sections_put_the_focused_app_first_until_a_search_starts() {
        let mut applet = Applet {
            phase: Phase::Open,
            app: "Telegram".into(),
            fitting: vec![row("Telegram", false)],
            found: vec![row("GitHub", true), row("Google", false)],
            ..Applet::default()
        };
        let titles: Vec<String> = applet.sections().into_iter().map(|(title, _)| title).collect();
        assert_eq!(titles.len(), 2);
        assert_eq!(applet.rows().len(), 3);
        applet.field.insert('g');
        assert_eq!(applet.sections().len(), 1);
        assert_eq!(applet.rows().len(), 2);
    }

    #[test]
    fn the_code_action_appears_only_with_a_totp_key() {
        let applet = Applet { phase: Phase::Open, found: vec![row("a", false), row("b", true)], ..Applet::default() };
        assert!(!applet.menu_actions(0).contains(&MenuAction::Code));
        assert!(applet.menu_actions(1).contains(&MenuAction::Code));
    }

    #[test]
    fn stepping_wraps_through_the_rows_and_the_menu() {
        let mut applet =
            Applet { phase: Phase::Open, found: vec![row("a", false), row("b", false)], ..Applet::default() };
        applet.step(-1);
        assert_eq!(applet.selected, 1);
        applet.menu = Some(1);
        applet.step(1);
        assert_eq!(applet.menu_selected, 1);
    }

    #[test]
    fn every_row_gets_a_rect_below_the_field_and_above_the_footer() {
        let applet = Applet {
            phase: Phase::Open,
            fitting: vec![row("a", false)],
            found: vec![row("b", false)],
            ..Applet::default()
        };
        let geometry = geometry(&applet);
        assert_eq!(geometry.rows.len(), 2);
        assert!(geometry.rows[0].loc.y > geometry.field.loc.y + FIELD_H);
        assert!(geometry.rows[1].loc.y + ROW_H <= geometry.divider);
        assert!(geometry.footer[0].loc.y > geometry.divider);
    }
}

use glide_theme::TextStyle;
use glide_theme::shape::{R_ROUND, R1, R2, R3};
use smithay::backend::renderer::element::memory::MemoryRenderBuffer;

use super::glyphs::svg_glyph;
use super::{LocalRect, TopbarTheme, local_rect};
use crate::i18n::{Key, t};
use crate::paint::{Painter, TextAlign};
use crate::polkit::{Control, Hint, Stage, View};
use crate::popup_menu::text_px as text_w;
use crate::raster::Canvas;
use glide_theme::Color;

pub(super) const WIDTH: i32 = 712;
pub(super) const RADIUS: f32 = R3;
const PAD_TOP: i32 = 22;
const PAD_X: i32 = 24;
const PAD_BOTTOM: i32 = 14;
const ICON: i32 = 56;
const ICON_GAP: i32 = 18;
const BADGE: i32 = 24;
const BADGE_RING: i32 = 3;
const BADGE_OUT: i32 = 5;
const CLOSE: i32 = 26;
const CLOSE_INSET: i32 = 14;
const TITLE_LINES: usize = 2;
const BODY_LINES: usize = 3;
const TITLE_GAP: i32 = 5;
const FIELD_GAP: i32 = 18;
const FIELD_H: i32 = 44;
const CHIP_H: i32 = 30;
const AVATAR: i32 = 22;
const EYE: i32 = 30;
const GO: i32 = 32;
const HINT_GAP: i32 = 10;
const HINT_H: i32 = 18;
const KEYCAP_H: i32 = 18;
const DETAILS_GAP: i32 = 14;
const DETAILS_PAD_Y: i32 = 12;
const DETAILS_PAD_X: i32 = 14;
const DETAILS_LABEL: i32 = 104;
const DETAILS_ROW: i32 = 17;
const DETAILS_ROW_GAP: i32 = 7;
const FOOTER_GAP: i32 = 16;
const FOOTER_PAD: i32 = 12;
const FOOTER_H: i32 = 30;
const SHIELD_H: i32 = 26;
const DETAILS_H: i32 = 28;
const CANCEL_H: i32 = 30;
const UNDERSTOOD_H: i32 = 32;
const POPOVER_W: i32 = 420;
const POPOVER_GAP: i32 = 10;
const POPOVER_PAD: i32 = 14;
const POPOVER_PAD_BOTTOM: i32 = 8;
const POPOVER_BADGE: i32 = 30;
const ITEM_WELL: i32 = 28;
const ITEM_PAD: i32 = 9;
const ITEM_GAP: i32 = 12;
pub(super) const SPINNER: i32 = 14;
pub(super) const SPINNER_STEPS: u32 = 12;
pub(super) const SWEEP_H: i32 = 2;

const fn style(size_px: f32, line_height_px: f32, weight: u16) -> TextStyle {
    TextStyle { size_px, line_height_px, weight, tracking_em: 0.0, uppercase: false, tabular: false }
}

const TITLE: TextStyle = style(17.0, 22.0, 650);
const BODY: TextStyle = style(13.5, 19.0, 400);
const NAME: TextStyle = style(12.5, 16.0, 600);
const INITIAL: TextStyle = style(11.0, 14.0, 700);
const FIELD: TextStyle = style(14.0, 18.0, 400);
const BULLETS: TextStyle = TextStyle { tracking_em: 0.12, ..style(14.0, 18.0, 400) };
const HINT: TextStyle = style(12.5, 18.0, 400);
const KEYCAP: TextStyle = style(10.5, 14.0, 600);
const DETAIL_LABEL: TextStyle = style(12.0, 17.0, 400);
const DETAIL_VALUE: TextStyle = TextStyle { tabular: true, ..style(12.0, 17.0, 400) };
const SHIELD: TextStyle = style(12.0, 16.0, 600);
const LINK: TextStyle = style(12.5, 16.0, 500);
const BUTTON: TextStyle = style(13.0, 16.0, 500);
const PRIMARY: TextStyle = style(13.0, 16.0, 600);
const POP_TITLE: TextStyle = style(13.5, 18.0, 650);
const POP_SUB: TextStyle = style(12.0, 16.0, 400);
const ITEM_TITLE: TextStyle = style(12.5, 17.0, 600);
const ITEM_BODY: TextStyle = style(12.0, 17.0, 400);

const LOCK: &str = "<rect x=\"5\" y=\"11\" width=\"14\" height=\"9\" rx=\"2\"/><path d=\"M8 11V8a4 4 0 0 1 8 0v3\"/>";
const SHIELD_CHECK: &str =
    "<path d=\"M12 2.5 4 6v6c0 4.8 3.4 8.4 8 9.5 4.6-1.1 8-4.7 8-9.5V6z\"/><path d=\"m8.8 12 2.2 2.2 4.4-4.4\"/>";
const EYE_SVG: &str =
    "<path d=\"M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7S2 12 2 12z\"/><circle cx=\"12\" cy=\"12\" r=\"3\"/>";
const EYE_OFF: &str = "<path d=\"M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7S2 12 2 12z\"/><circle cx=\"12\" cy=\"12\" r=\"3\"/><path d=\"M4 4l16 16\"/>";
const ARROW: &str = "<path d=\"M5 12h14M13 6l6 6-6 6\"/>";
const CHEVRON_RIGHT: &str = "<path d=\"m9 6 6 6-6 6\"/>";
const CHEVRON_DOWN: &str = "<path d=\"m6 9 6 6 6-6\"/>";
const CLOSE_SVG: &str = "<path d=\"M6 6l12 12M18 6 6 18\"/>";
const CHECK: &str = "<path d=\"m5 12.5 4.5 4.5L19 7.5\"/>";
const CLOCK: &str = "<circle cx=\"12\" cy=\"12\" r=\"8.5\"/><path d=\"M12 7.5V12l3 2\"/>";
const ALERT: &str = "<circle cx=\"12\" cy=\"12\" r=\"9\"/><path d=\"M12 7.5v5.5M12 16.5v.1\"/>";
const KEYBOARD: &str = "<rect x=\"2.5\" y=\"6\" width=\"19\" height=\"12\" rx=\"2\"/><path d=\"M6.5 10h.01M10 10h.01M14 10h.01M17.5 10h.01M7.5 14h9\"/>";
const SCREEN_OFF: &str =
    "<rect x=\"3\" y=\"4\" width=\"18\" height=\"12\" rx=\"2\"/><path d=\"M8 20h8M12 16v4M3.5 3.5l17 17\"/>";
const CLIPBOARD_OFF: &str =
    "<rect x=\"6\" y=\"4.5\" width=\"12\" height=\"16\" rx=\"2\"/><path d=\"M9.5 4.5V3h5v1.5M3.5 3.5l17 17\"/>";
const LAYERS: &str = "<path d=\"m12 3 9 5-9 5-9-5z\"/><path d=\"m3 13 9 5 9-5\"/>";
const KEY: &str = "<circle cx=\"8\" cy=\"15\" r=\"4\"/><path d=\"m11 12 9-9M16.5 6.5l3 3\"/>";

fn svg(body: &str, stroke: f32) -> String {
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" \
         stroke=\"#000\" stroke-width=\"{stroke}\" stroke-linecap=\"round\" stroke-linejoin=\"round\">{body}</svg>"
    )
}

fn icon(c: &mut Canvas<'_>, body: &str, stroke: f32, side: i32, rect: LocalRect, colour: Color) {
    svg_glyph(c, &svg(body, stroke), side, rect, colour);
}

#[must_use]
pub(super) fn spinner_svg(step: u32) -> String {
    let degrees = step * 360 / SPINNER_STEPS;
    svg(
        &format!(
            "<circle cx=\"12\" cy=\"12\" r=\"9\" stroke-opacity=\"0.35\"/><path d=\"M12 3a9 9 0 0 1 9 9\" transform=\"rotate({degrees} 12 12)\"/>"
        ),
        3.0,
    )
}

const fn line_h(style: &TextStyle) -> i32 {
    style.line_height_px as i32
}

fn text_width(painter: &Painter, text: &str, style: &TextStyle) -> i32 {
    text_w(painter, text, style)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Layout {
    pub height: i32,
    pub close: LocalRect,
    icon: LocalRect,
    text_x: i32,
    title: Vec<String>,
    title_y: i32,
    body: Vec<String>,
    body_y: i32,
    field: Option<LocalRect>,
    chip: Option<LocalRect>,
    reveal: Option<LocalRect>,
    submit: Option<LocalRect>,
    hint_y: i32,
    details: Option<(LocalRect, Vec<(&'static str, String)>)>,
    footer_y: i32,
    shield: Option<LocalRect>,
    toggle: Option<LocalRect>,
    cancel: Option<LocalRect>,
    understood: Option<LocalRect>,
    pub popover: Option<LocalRect>,
}

fn title_text(view: &View) -> (Option<&str>, String) {
    match view.stage {
        Stage::Done => (None, t(Key::PkAllowed).to_string()),
        Stage::Locked => (None, t(Key::PkLockedTitle).to_string()),
        _ => match view.app.as_deref() {
            Some(app) => (Some(app), t(Key::PkWants).to_string()),
            None => (None, t(Key::PkNeedsRights).to_string()),
        },
    }
}

fn body_text(view: &View) -> String {
    match view.stage {
        Stage::Done => t(Key::PkAllowedBody).to_string(),
        Stage::Locked => view.note.clone().unwrap_or_else(|| t(Key::PkLockedBody).to_string()),
        _ => view.message.clone(),
    }
}

fn title_lines(painter: &Painter, view: &View, width: i32) -> Vec<String> {
    let (app, rest) = title_text(view);
    let whole = app.map_or_else(|| rest.clone(), |app| format!("{app} {rest}"));
    crate::notifications::wrap_lines(painter, &whole, &TITLE, width, TITLE_LINES)
}

fn detail_rows(view: &View) -> Vec<(&'static str, String)> {
    let mut rows = vec![(t(Key::PkAction), view.action.clone())];
    if let Some(program) = view.program.clone() {
        rows.push((t(Key::PkProgram), program));
    }
    rows.push((t(Key::PkAsUser), view.user.clone()));
    rows
}

#[must_use]
pub(super) fn layout(painter: &Painter, view: &View) -> Layout {
    let close = local_rect(WIDTH - CLOSE_INSET - CLOSE, CLOSE_INSET, CLOSE, CLOSE);
    let icon = local_rect(PAD_X, PAD_TOP, ICON, ICON);
    let text_x = PAD_X + ICON + ICON_GAP;
    let title_y = PAD_TOP + 2;
    let title = title_lines(painter, view, close.loc.x - 6 - text_x);
    let body_y = title_y + line_h(&TITLE) * title.len() as i32 + TITLE_GAP;
    let body =
        crate::notifications::wrap_paragraphs(painter, &body_text(view), &BODY, WIDTH - PAD_X - text_x, BODY_LINES);
    let head_bottom = (PAD_TOP + ICON).max(body_y + line_h(&BODY) * body.len() as i32);
    let mut layout = Layout {
        height: 0,
        close,
        icon,
        text_x,
        title,
        title_y,
        body,
        body_y,
        field: None,
        chip: None,
        reveal: None,
        submit: None,
        hint_y: 0,
        details: None,
        footer_y: 0,
        shield: None,
        toggle: None,
        cancel: None,
        understood: None,
        popover: None,
    };
    match view.stage {
        Stage::Done => {
            layout.height = head_bottom + PAD_TOP;
            return layout;
        }
        Stage::Locked => {
            let width = text_width(painter, t(Key::PkUnderstood), &PRIMARY) + 36;
            let y = head_bottom + FOOTER_GAP;
            layout.understood = Some(local_rect(WIDTH - PAD_X - width, y, width, UNDERSTOOD_H));
            layout.height = y + UNDERSTOOD_H + PAD_TOP - 4;
            return layout;
        }
        Stage::Ask | Stage::Checking | Stage::Wrong => {}
    }
    let field_y = head_bottom + FIELD_GAP;
    let field = local_rect(PAD_X, field_y, WIDTH - PAD_X * 2, FIELD_H);
    let chip_w = 4 + AVATAR + 7 + text_width(painter, &view.display, &NAME) + 10;
    layout.chip = Some(local_rect(field.loc.x + 7, field_y + (FIELD_H - CHIP_H) / 2, chip_w, CHIP_H));
    let submit = local_rect(field.loc.x + field.size.w - 1 - 6 - GO, field_y + (FIELD_H - GO) / 2, GO, GO);
    layout.submit = Some(submit);
    if view.stage != Stage::Checking {
        layout.reveal = Some(local_rect(submit.loc.x - 4 - EYE, field_y + (FIELD_H - EYE) / 2, EYE, EYE));
    }
    layout.field = Some(field);
    layout.hint_y = field_y + FIELD_H + HINT_GAP;
    let mut below = layout.hint_y + HINT_H;
    if view.details {
        let rows = detail_rows(view);
        let count = rows.len() as i32;
        let height = DETAILS_PAD_Y * 2 + count * DETAILS_ROW + (count - 1).max(0) * DETAILS_ROW_GAP;
        let rect = local_rect(PAD_X, below + DETAILS_GAP, WIDTH - PAD_X * 2, height);
        below = rect.loc.y + height;
        layout.details = Some((rect, rows));
    }
    layout.footer_y = below + FOOTER_GAP;
    let row_y = layout.footer_y + 1 + FOOTER_PAD;
    let shield_w = 8 + 13 + 6 + text_width(painter, t(Key::PkProtected), &SHIELD) + 10;
    let shield = local_rect(PAD_X + 2, row_y + (FOOTER_H - SHIELD_H) / 2, shield_w, SHIELD_H);
    layout.shield = Some(shield);
    let cancel_w = text_width(painter, t(Key::PkCancel), &BUTTON) + 32;
    let cancel = local_rect(WIDTH - PAD_X - cancel_w, row_y, cancel_w, CANCEL_H);
    layout.cancel = Some(cancel);
    let label = if view.details { t(Key::PkHideDetails) } else { t(Key::PkDetails) };
    let toggle_w = 6 + 12 + 5 + text_width(painter, label, &LINK) + 8;
    layout.toggle =
        Some(local_rect(cancel.loc.x - 8 - toggle_w, row_y + (FOOTER_H - DETAILS_H) / 2, toggle_w, DETAILS_H));
    layout.height = row_y + FOOTER_H + PAD_BOTTOM;
    if view.shield {
        let size = popover_size(painter);
        layout.popover = Some(local_rect(shield.loc.x - 6, row_y + FOOTER_H + POPOVER_GAP, size.0, size.1));
    }
    layout
}

#[must_use]
pub(super) fn slots(layout: &Layout, view: &View) -> Vec<(Control, LocalRect)> {
    let mut slots = if view.stage == Stage::Done { Vec::new() } else { vec![(Control::Close, layout.close)] };
    let controls = [
        (Control::Submit, layout.submit.filter(|_| view.ready)),
        (Control::Reveal, layout.reveal),
        (Control::Shield, layout.shield),
        (Control::Details, layout.toggle),
        (Control::Cancel, layout.cancel),
        (Control::Understood, layout.understood),
        (Control::Field, layout.field),
    ];
    slots.extend(controls.into_iter().filter_map(|(control, rect)| Some((control, rect?))));
    slots
}

#[must_use]
pub(super) const fn layer_radius(control: Control) -> Option<f32> {
    match control {
        Control::Field => None,
        Control::Close | Control::Submit | Control::Shield => Some(R_ROUND),
        Control::Reveal | Control::Details | Control::Cancel | Control::Understood => Some(R2),
    }
}

struct Ink<'a> {
    painter: &'a Painter,
    theme: &'a TopbarTheme,
}

fn draw_app_icon(c: &mut Canvas<'_>, ink: &Ink<'_>, view: &View, rect: LocalRect) {
    let theme = ink.theme;
    let side = rect.size.w;
    let pixels = view
        .icon
        .as_deref()
        .and_then(|id| c.icons.as_deref_mut().and_then(|icons| icons.load(id, side * c.scale)).map(<[u8]>::to_vec));
    match view.stage {
        Stage::Done => {
            let halo = local_rect(rect.loc.x - 6, rect.loc.y - 6, side + 12, side + 12);
            c.fill(halo, theme.ok.with_alpha(0.06), R_ROUND);
            c.fill(rect, theme.ok_soft, R_ROUND);
            icon(c, CHECK, 2.4, 26, rect, theme.ok);
            return;
        }
        Stage::Locked => {
            c.fill(rect, theme.danger_soft, R3);
            icon(c, CLOCK, 1.9, 26, rect, theme.danger);
            return;
        }
        Stage::Ask | Stage::Checking | Stage::Wrong => {}
    }
    if let Some(pixels) = pixels {
        c.blit_square(&pixels, side, (rect.loc.x, rect.loc.y));
    } else {
        c.fill(rect, theme.fill_2, R3);
        icon(c, LOCK, 1.6, 28, rect, theme.label);
    }
    let x = rect.loc.x + side + BADGE_OUT - BADGE;
    let y = rect.loc.y + side + BADGE_OUT - BADGE;
    let ring = local_rect(x - BADGE_RING, y - BADGE_RING, BADGE + BADGE_RING * 2, BADGE + BADGE_RING * 2);
    c.fill(ring, theme.island_bg, R_ROUND);
    let badge = local_rect(x, y, BADGE, BADGE);
    c.fill(badge, theme.active_background, R_ROUND);
    icon(c, LOCK, 2.6, 12, badge, theme.active_label);
}

fn draw_title(c: &mut Canvas<'_>, ink: &Ink<'_>, view: &View, layout: &Layout) {
    let (app, _) = title_text(view);
    let right = layout.close.loc.x - 6;
    let height = line_h(&TITLE);
    let mut y = layout.title_y;
    for (index, line) in layout.title.iter().enumerate() {
        let accent = app.filter(|app| index == 0 && line.starts_with(app));
        let band = |left: i32, y: i32| (left, right, y, y + height);
        if let Some(app) = accent {
            c.text(ink.painter, band(layout.text_x, y), TextAlign::Left, app, ink.theme.active_soft_label, &TITLE);
            let rest = &line[app.len()..];
            let left = layout.text_x + text_width(ink.painter, app, &TITLE);
            c.text(ink.painter, band(left, y), TextAlign::Left, rest, ink.theme.label, &TITLE);
        } else {
            c.text(ink.painter, band(layout.text_x, y), TextAlign::Left, line, ink.theme.label, &TITLE);
        }
        y += height;
    }
    let mut y = layout.body_y;
    for line in &layout.body {
        let band = (layout.text_x, WIDTH - PAD_X, y, y + line_h(&BODY));
        c.text(ink.painter, band, TextAlign::Left, line, ink.theme.muted, &BODY);
        y += line_h(&BODY);
    }
}

fn draw_close(c: &mut Canvas<'_>, ink: &Ink<'_>, layout: &Layout) {
    c.fill(layout.close, ink.theme.track_soft, R_ROUND);
    icon(c, CLOSE_SVG, 2.6, 11, layout.close, ink.theme.muted);
}

fn initial(name: &str) -> String {
    name.chars().next().unwrap_or('?').to_uppercase().to_string()
}

fn tail_that_fits<'a>(painter: &Painter, text: &'a str, width: i32, style: &TextStyle) -> &'a str {
    let starts: Vec<usize> = text.char_indices().map(|(index, _)| index).collect();
    let first = starts.partition_point(|&start| text_width(painter, &text[start..], style) > width);
    starts.get(first).map_or("", |&start| &text[start..])
}

fn draw_field(c: &mut Canvas<'_>, ink: &Ink<'_>, view: &View, layout: &Layout) {
    let (Some(field), Some(chip), Some(submit)) = (layout.field, layout.chip, layout.submit) else {
        return;
    };
    let theme = ink.theme;
    let wrong = view.stage == Stage::Wrong;
    let (border, ring) = if wrong { (theme.danger, theme.danger_soft) } else { (theme.focus_line, theme.acc_ring) };
    let halo = local_rect(field.loc.x - 3, field.loc.y - 3, field.size.w + 6, field.size.h + 6);
    c.fill(halo, ring, R3 + 3.0);
    c.fill(field, theme.view_bg, R3);
    c.stroke(field, 1.0, R3, border);
    let checking = view.stage == Stage::Checking;
    let dim = |colour: Color| if checking { colour.with_alpha(colour.0[3] * 0.75) } else { colour };

    c.fill(chip, theme.track_soft, R_ROUND);
    let avatar = local_rect(chip.loc.x + 4, chip.loc.y + (CHIP_H - AVATAR) / 2, AVATAR, AVATAR);
    let top = theme.active_background.state_layer(theme.active_label, glide_theme::motion::ST_DRAG);
    c.gradient(avatar, R_ROUND, top, theme.active_background);
    c.text_in(ink.painter, avatar, TextAlign::Center, &initial(&view.display), theme.active_label, &INITIAL);
    let name_x = avatar.loc.x + AVATAR + 7;
    let band = (name_x, chip.loc.x + chip.size.w, chip.loc.y, chip.loc.y + CHIP_H);
    c.text(ink.painter, band, TextAlign::Left, &view.display, dim(theme.label), &NAME);

    let text_left = chip.loc.x + chip.size.w + 12;
    let text_right = layout.reveal.map_or(submit.loc.x, |eye| eye.loc.x) - 6;
    let band = (text_left, text_right, field.loc.y, field.loc.y + FIELD_H);
    let (shown, style) = match view.shown.as_deref() {
        Some(text) => (text.to_string(), &FIELD),
        None => ("\u{2022}".repeat(view.bullets), &BULLETS),
    };
    if shown.is_empty() {
        c.text(ink.painter, band, TextAlign::Left, t(Key::PkPassword), theme.faint, &FIELD);
    } else {
        let tail = tail_that_fits(ink.painter, &shown, text_right - text_left - 4, style);
        c.text(ink.painter, band, TextAlign::Left, tail, dim(theme.label), style);
    }
    if !checking {
        let typed = if shown.is_empty() {
            0
        } else {
            text_width(ink.painter, tail_that_fits(ink.painter, &shown, text_right - text_left - 4, style), style)
        };
        let caret_x = text_left + typed + 1;
        if caret_x < text_right {
            c.fill(local_rect(caret_x, field.loc.y + (FIELD_H - 18) / 2, 1, 18), theme.label, 0.0);
        }
    }
    if let Some(eye) = layout.reveal {
        let body = if view.shown.is_some() { EYE_OFF } else { EYE_SVG };
        icon(c, body, 1.8, 16, eye, theme.muted);
    }
    let go = if view.ready || checking { theme.active_background } else { theme.active_background.with_alpha(0.4) };
    c.fill(submit, go, R_ROUND);
    if !checking {
        let colour = if view.ready { theme.active_label } else { theme.active_label.with_alpha(0.6) };
        icon(c, ARROW, 2.4, 15, submit, colour);
    }
}

fn keycap(c: &mut Canvas<'_>, ink: &Ink<'_>, x: i32, y: i32, label: &str) -> i32 {
    let width = text_width(ink.painter, label, &KEYCAP) + 10;
    let rect = local_rect(x, y + (HINT_H - KEYCAP_H) / 2, width, KEYCAP_H);
    c.fill(rect, ink.theme.track_soft, R1);
    c.fill(local_rect(rect.loc.x + 1, rect.loc.y + KEYCAP_H - 1, width - 2, 1), ink.theme.line, 0.0);
    c.text_in(ink.painter, rect, TextAlign::Center, label, ink.theme.muted, &KEYCAP);
    x + width
}

fn draw_hint(c: &mut Canvas<'_>, ink: &Ink<'_>, view: &View, layout: &Layout) {
    let theme = ink.theme;
    let y = layout.hint_y;
    let band = |x: i32| (x, WIDTH - PAD_X, y, y + HINT_H);
    let warn = |c: &mut Canvas<'_>, text: &str| {
        icon(c, ALERT, 2.2, 13, local_rect(PAD_X, y, 13, HINT_H), theme.danger);
        c.text(ink.painter, band(PAD_X + 13 + 6), TextAlign::Left, text, theme.danger, &HINT);
    };
    match &view.hint {
        Hint::Checking => c.text(ink.painter, band(PAD_X), TextAlign::Left, t(Key::PkChecking), theme.muted, &HINT),
        Hint::Wrong { left } => {
            let text = format!("{} {}", t(Key::PkWrong), crate::polkit::attempts_left(crate::i18n::language(), *left));
            warn(c, &text);
        }
        Hint::Broken => warn(c, t(Key::PkBroken)),
        Hint::CapsLock => warn(c, t(Key::PkCapsLock)),
        Hint::Keys => {
            let mut x = keycap(c, ink, PAD_X, y, "Enter") + 6;
            let allow = t(Key::PkAllowKey);
            c.text(ink.painter, band(x), TextAlign::Left, allow, theme.faint, &HINT);
            x += text_width(ink.painter, allow, &HINT) + 12;
            x = keycap(c, ink, x, y, "Esc") + 6;
            c.text(ink.painter, band(x), TextAlign::Left, t(Key::PkCancelKey), theme.faint, &HINT);
        }
    }
}

fn draw_details(c: &mut Canvas<'_>, ink: &Ink<'_>, layout: &Layout) {
    let Some((rect, rows)) = layout.details.as_ref() else {
        return;
    };
    let theme = ink.theme;
    c.fill(*rect, theme.track_soft, R2);
    c.stroke(*rect, 1.0, R2, theme.line);
    let mut y = rect.loc.y + DETAILS_PAD_Y;
    let left = rect.loc.x + DETAILS_PAD_X;
    let right = rect.loc.x + rect.size.w - DETAILS_PAD_X;
    for (label, value) in rows {
        c.text(
            ink.painter,
            (left, left + DETAILS_LABEL, y, y + DETAILS_ROW),
            TextAlign::Left,
            label,
            theme.faint,
            &DETAIL_LABEL,
        );
        c.text(
            ink.painter,
            (left + DETAILS_LABEL, right, y, y + DETAILS_ROW),
            TextAlign::Left,
            value,
            theme.label,
            &DETAIL_VALUE,
        );
        y += DETAILS_ROW + DETAILS_ROW_GAP;
    }
}

fn draw_footer(c: &mut Canvas<'_>, ink: &Ink<'_>, view: &View, layout: &Layout) {
    let theme = ink.theme;
    if let Some(understood) = layout.understood {
        c.fill(understood, theme.active_background, R2);
        c.text_in(ink.painter, understood, TextAlign::Center, t(Key::PkUnderstood), theme.active_label, &PRIMARY);
    }
    let (Some(shield), Some(toggle), Some(cancel)) = (layout.shield, layout.toggle, layout.cancel) else {
        return;
    };
    c.fill(local_rect(PAD_X, layout.footer_y, WIDTH - PAD_X * 2, 1), theme.line, 0.0);
    let soft = if view.shield { theme.ok.with_alpha(0.18) } else { theme.ok_soft };
    c.fill(shield, soft, R_ROUND);
    icon(c, SHIELD_CHECK, 2.2, 13, local_rect(shield.loc.x + 8, shield.loc.y, 13, SHIELD_H), theme.ok);
    let band = (shield.loc.x + 8 + 13 + 6, shield.loc.x + shield.size.w, shield.loc.y, shield.loc.y + SHIELD_H);
    c.text(ink.painter, band, TextAlign::Left, t(Key::PkProtected), theme.ok, &SHIELD);

    let chevron = if view.details { CHEVRON_DOWN } else { CHEVRON_RIGHT };
    icon(c, chevron, 2.4, 12, local_rect(toggle.loc.x + 6, toggle.loc.y, 12, DETAILS_H), theme.muted);
    let label = if view.details { t(Key::PkHideDetails) } else { t(Key::PkDetails) };
    let band = (toggle.loc.x + 6 + 12 + 5, toggle.loc.x + toggle.size.w, toggle.loc.y, toggle.loc.y + DETAILS_H);
    c.text(ink.painter, band, TextAlign::Left, label, theme.muted, &LINK);

    c.fill(cancel, theme.track_soft, R2);
    c.text_in(ink.painter, cancel, TextAlign::Center, t(Key::PkCancel), theme.label, &BUTTON);
}

pub(super) fn paint(
    painter: &Painter, theme: &TopbarTheme, view: &View, layout: &Layout, scale: i32,
    icons: Option<&mut crate::icon::IconLoader>,
) -> Option<MemoryRenderBuffer> {
    let mut c = Canvas::new(WIDTH, layout.height, scale)?.with_icons(icons);
    paint_into(&mut c, painter, theme, view, layout);
    Some(c.finish())
}

fn paint_into(c: &mut Canvas<'_>, painter: &Painter, theme: &TopbarTheme, view: &View, layout: &Layout) {
    let ink = Ink { painter, theme };
    draw_app_icon(c, &ink, view, layout.icon);
    draw_title(c, &ink, view, layout);
    if view.stage != Stage::Done {
        draw_close(c, &ink, layout);
    }
    draw_field(c, &ink, view, layout);
    if layout.field.is_some() {
        draw_hint(c, &ink, view, layout);
    }
    draw_details(c, &ink, layout);
    draw_footer(c, &ink, view, layout);
}

const ITEMS: [(&str, Key, Key); 5] = [
    (KEYBOARD, Key::PkShieldKeys, Key::PkShieldKeysBody),
    (SCREEN_OFF, Key::PkShieldScreen, Key::PkShieldScreenBody),
    (CLIPBOARD_OFF, Key::PkShieldClipboard, Key::PkShieldClipboardBody),
    (LAYERS, Key::PkShieldTop, Key::PkShieldTopBody),
    (KEY, Key::PkShieldDirect, Key::PkShieldDirectBody),
];

const POP_TEXT_X: i32 = POPOVER_PAD + 2 + POPOVER_BADGE + 10;

fn sub_lines(painter: &Painter) -> Vec<String> {
    crate::notifications::wrap_lines(painter, t(Key::PkShieldSub), &POP_SUB, POPOVER_W - POPOVER_PAD - POP_TEXT_X, 2)
}

fn head_height(painter: &Painter) -> i32 {
    (line_h(&POP_TITLE) + 2 + line_h(&POP_SUB) * sub_lines(painter).len() as i32).max(POPOVER_BADGE)
}

fn item_text_width() -> i32 {
    POPOVER_W - POPOVER_PAD * 2 - 4 - ITEM_WELL - ITEM_GAP
}

fn item_lines(painter: &Painter, body: Key) -> Vec<String> {
    crate::notifications::wrap_lines(painter, t(body), &ITEM_BODY, item_text_width(), 4)
}

fn item_height(painter: &Painter, body: Key) -> i32 {
    let text = line_h(&ITEM_TITLE) + 2 + line_h(&ITEM_BODY) * item_lines(painter, body).len() as i32;
    ITEM_PAD * 2 + text.max(ITEM_WELL)
}

#[must_use]
pub(super) fn popover_size(painter: &Painter) -> (i32, i32) {
    let items: i32 = ITEMS.iter().map(|(_, _, body)| item_height(painter, *body)).sum();
    (POPOVER_W, POPOVER_PAD + head_height(painter) + 12 + 1 + 4 + items + POPOVER_PAD_BOTTOM)
}

pub(super) fn paint_popover(painter: &Painter, theme: &TopbarTheme, scale: i32) -> Option<MemoryRenderBuffer> {
    popover_canvas(painter, theme, scale).map(|c| c.finish())
}

fn popover_canvas(painter: &Painter, theme: &TopbarTheme, scale: i32) -> Option<Canvas<'static>> {
    let size = popover_size(painter);
    let mut c = Canvas::popover(size, scale, R3, theme.popover_bg, theme.line)?;
    c.fill(local_rect(1, 1, size.0 - 2, 1), theme.bezel_hi, 0.0);
    let left = POPOVER_PAD + 2;
    let top = POPOVER_PAD;
    let head = head_height(painter);
    let badge = local_rect(left, top, POPOVER_BADGE, POPOVER_BADGE);
    c.fill(badge, theme.ok_soft, R_ROUND);
    icon(&mut c, SHIELD_CHECK, 2.2, 15, badge, theme.ok);
    let text_x = POP_TEXT_X;
    let right = size.0 - POPOVER_PAD;
    let title_band = (text_x, right, top, top + line_h(&POP_TITLE));
    c.text(painter, title_band, TextAlign::Left, t(Key::PkShieldTitle), theme.label, &POP_TITLE);
    let mut sub_y = top + line_h(&POP_TITLE) + 2;
    for sub in sub_lines(painter) {
        c.text(painter, (text_x, right, sub_y, sub_y + line_h(&POP_SUB)), TextAlign::Left, &sub, theme.muted, &POP_SUB);
        sub_y += line_h(&POP_SUB);
    }
    let divider_y = top + head + 12;
    c.fill(local_rect(POPOVER_PAD, divider_y, size.0 - POPOVER_PAD * 2, 1), theme.line, 0.0);
    let mut y = divider_y + 1 + 4;
    for (glyph, title, body) in ITEMS {
        let height = item_height(painter, body);
        let well = local_rect(left, y + ITEM_PAD, ITEM_WELL, ITEM_WELL);
        c.fill(well, theme.track_soft, R2);
        icon(&mut c, glyph, 1.9, 15, well, theme.muted);
        let x = well.loc.x + ITEM_WELL + ITEM_GAP;
        let mut line_y = y + ITEM_PAD;
        c.text(
            painter,
            (x, right, line_y, line_y + line_h(&ITEM_TITLE)),
            TextAlign::Left,
            t(title),
            theme.label,
            &ITEM_TITLE,
        );
        line_y += line_h(&ITEM_TITLE) + 2;
        for line in item_lines(painter, body) {
            c.text(
                painter,
                (x, right, line_y, line_y + line_h(&ITEM_BODY)),
                TextAlign::Left,
                &line,
                theme.muted,
                &ITEM_BODY,
            );
            line_y += line_h(&ITEM_BODY);
        }
        y += height;
    }
    Some(c)
}

pub(super) fn paint_spinner(step: u32, colour: Color, scale: i32) -> Option<MemoryRenderBuffer> {
    let mut c = Canvas::new(SPINNER, SPINNER, scale)?;
    let full = c.full();
    svg_glyph(&mut c, &spinner_svg(step), SPINNER, full, colour);
    Some(c.finish())
}

#[must_use]
pub(super) fn spinner_rect(layout: &Layout) -> Option<LocalRect> {
    let submit = layout.submit?;
    Some(local_rect(submit.loc.x + (GO - SPINNER) / 2, submit.loc.y + (GO - SPINNER) / 2, SPINNER, SPINNER))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::polkit::Hint;

    fn view(stage: Stage) -> View {
        View {
            stage,
            app: Some("Налаштування".into()),
            icon: None,
            message: "Для зміни системного часу потрібна автентифікація.".into(),
            user: "artem".into(),
            display: "Артем".into(),
            action: "org.freedesktop.timedate1.set-time".into(),
            program: Some("/usr/bin/glide-settings · pid 4182".into()),
            bullets: 0,
            shown: None,
            details: false,
            shield: false,
            hint: Hint::Keys,
            note: None,
            ready: false,
        }
    }

    fn painter() -> Painter {
        Painter::new(14.0, None)
    }

    #[test]
    fn the_field_row_matches_the_concept_and_keeps_its_parts_apart() {
        let painter = painter();
        let layout = layout(&painter, &view(Stage::Ask));
        let field = layout.field.expect("a field");
        assert_eq!((field.loc.x, field.size.w, field.size.h), (PAD_X, WIDTH - 2 * PAD_X, FIELD_H));
        let (chip, eye, go) = (layout.chip.expect("chip"), layout.reveal.expect("eye"), layout.submit.expect("go"));
        assert!(chip.loc.x > field.loc.x && chip.loc.x + chip.size.w < eye.loc.x);
        assert!(eye.loc.x + eye.size.w < go.loc.x);
        assert_eq!(go.loc.x + go.size.w + 7, field.loc.x + field.size.w);
        let (shield, toggle, cancel) =
            (layout.shield.expect("s"), layout.toggle.expect("d"), layout.cancel.expect("c"));
        assert_eq!(cancel.loc.x + cancel.size.w, WIDTH - PAD_X, "Cancel ends the footer on the right");
        assert!(shield.loc.x + shield.size.w < toggle.loc.x && toggle.loc.x + toggle.size.w < cancel.loc.x);
        assert_eq!(layout.height, cancel.loc.y + CANCEL_H + PAD_BOTTOM);
        assert!(layout.title.len() == 1, "{:?}", layout.title);
        assert!(layout.popover.is_none());
    }

    #[test]
    fn details_and_the_shield_grow_the_prompt_without_moving_the_field() {
        let painter = painter();
        let plain = layout(&painter, &view(Stage::Ask));
        let open = layout(&painter, &View { details: true, shield: true, ..view(Stage::Ask) });
        assert_eq!(plain.field, open.field);
        let (rect, rows) = open.details.as_ref().expect("details");
        assert_eq!(rows.len(), 3);
        assert!(rect.loc.y > plain.hint_y && open.height > plain.height);
        let popover = open.popover.expect("popover");
        assert!(popover.loc.y > open.cancel.expect("cancel").loc.y, "the popover hangs below the footer");
        let checking = layout(&painter, &View { stage: Stage::Checking, ..view(Stage::Checking) });
        assert!(checking.reveal.is_none(), "the eye hides while checking");
        let slots = slots(&checking, &view(Stage::Checking));
        assert!(!slots.iter().any(|(control, _)| *control == Control::Submit), "nothing to submit while checking");
    }

    #[test]
    fn done_and_locked_drop_the_field_and_footer() {
        let painter = painter();
        let done = layout(&painter, &view(Stage::Done));
        assert!(done.field.is_none() && done.cancel.is_none() && done.understood.is_none());
        let locked = layout(&painter, &view(Stage::Locked));
        assert!(locked.field.is_none() && locked.understood.is_some());
        assert!(locked.height > done.height);
    }

    #[test]
    fn every_icon_and_spinner_step_is_valid_svg() {
        for body in [
            LOCK,
            SHIELD_CHECK,
            EYE_SVG,
            EYE_OFF,
            ARROW,
            CHEVRON_RIGHT,
            CHEVRON_DOWN,
            CLOSE_SVG,
            CHECK,
            CLOCK,
            ALERT,
            KEYBOARD,
            SCREEN_OFF,
            CLIPBOARD_OFF,
            LAYERS,
            KEY,
        ] {
            assert!(crate::paint::render_svg(svg(body, 2.0).as_bytes(), 24).is_some(), "{body}");
        }
        for step in 0..SPINNER_STEPS {
            assert!(crate::paint::render_svg(spinner_svg(step).as_bytes(), 24).is_some());
        }
    }

    #[test]
    #[ignore = "writes preview PNGs to $GLIDE_POLKIT_PREVIEW"]
    fn preview() {
        let Some(dir) = std::env::var_os("GLIDE_POLKIT_PREVIEW") else {
            return;
        };
        let painter = painter();
        let scale = 2;
        for appearance in [glide_theme::Appearance::Dark, glide_theme::Appearance::Light] {
            let theme = TopbarTheme::from_tokens(&glide_theme::Tokens::new(appearance, glide_theme::Accent::Blue));
            let cases = [
                ("ask", view(Stage::Ask)),
                ("typed", View { bullets: 8, ready: true, ..view(Stage::Ask) }),
                ("checking", View { bullets: 8, hint: Hint::Checking, ..view(Stage::Checking) }),
                ("wrong", View { hint: Hint::Wrong { left: 2 }, ..view(Stage::Wrong) }),
                ("caps", View { hint: Hint::CapsLock, bullets: 3, ready: true, ..view(Stage::Ask) }),
                ("details", View { details: true, bullets: 4, ready: true, ..view(Stage::Ask) }),
                ("shown", View { shown: Some("hunter2".into()), bullets: 7, ready: true, ..view(Stage::Ask) }),
                ("shield", View { shield: true, ..view(Stage::Ask) }),
                ("locked", view(Stage::Locked)),
                ("done", view(Stage::Done)),
                ("unknown", View { app: None, ..view(Stage::Ask) }),
            ];
            for (name, case) in cases {
                let layout = layout(&painter, &case);
                let mut surface =
                    Canvas::popover((WIDTH, layout.height), scale, RADIUS, theme.island_bg, theme.island_line)
                        .expect("surface");
                paint_into(&mut surface, &painter, &theme, &case, &layout);
                let pop = layout.popover.map(|_| popover_size(&painter));
                let extra = pop.map_or(0, |(_, tall)| tall + POPOVER_GAP);
                let width = (WIDTH * scale) as u32 + 64;
                let height = ((layout.height + extra) * scale) as u32 + 64;
                let mut image = glide_shell::image::Image::new(width, height).expect("image");
                let shade = if appearance == glide_theme::Appearance::Dark { 20 } else { 230 };
                let backdrop = vec![shade; (width * height * 4) as usize];
                image.draw(&glide_shell::image::Image::from_bgra(width, height, backdrop).expect("bg"), 0, 0);
                let content = (WIDTH * scale) as u32;
                let tall = (layout.height * scale) as u32;
                let prompt = glide_shell::image::Image::from_bgra(content, tall, surface.buf.clone()).expect("p");
                image.draw(&prompt, 32, 32);
                if let (Some(rect), Some(size)) = (layout.popover, pop) {
                    let drawn = popover_canvas(&painter, &theme, scale).expect("popover").buf;
                    let (across, down) = ((size.0 * scale) as u32, (size.1 * scale) as u32);
                    let popover = glide_shell::image::Image::from_bgra(across, down, drawn).expect("pop");
                    image.draw(&popover, 32 + rect.loc.x * scale, 32 + rect.loc.y * scale);
                }
                let png = image.png().expect("png");
                let file = format!("polkit-{name}-{appearance:?}.png").to_lowercase();
                std::fs::write(std::path::Path::new(&dir).join(file), png).expect("write");
            }
        }
    }
}

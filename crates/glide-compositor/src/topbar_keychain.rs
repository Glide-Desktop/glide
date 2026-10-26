use glide_theme::TextStyle;
use glide_theme::shape::{R_ROUND, R1, R2, R3};
use smithay::backend::renderer::element::memory::MemoryRenderBuffer;

use super::glyphs::svg_glyph;
use super::{LocalRect, TopbarTheme, local_rect};
use crate::config::Color;
use crate::i18n::{Key, t};
use crate::keychain_prompt::{Chip, Control, Hint, View};
use crate::paint::{Painter, TextAlign};
use crate::popup_menu::text_px as text_w;
use crate::raster::Canvas;

pub(super) const WIDTH: i32 = 712;
pub(super) const RADIUS: f32 = R3;
const PAD_TOP: i32 = 20;
const PAD_X: i32 = 24;
const PAD_BOTTOM: i32 = 16;
const GAP: i32 = 16;
const TILE: i32 = 56;
const BADGE: i32 = 22;
const BADGE_RING: i32 = 2;
const BADGE_OUT: i32 = 4;
const BUTTON: i32 = 28;
const CHIP_H: i32 = 22;
const CHIP_PAD: i32 = 8;
const CHIP_GAP: i32 = 8;
const CHIP_ICON: i32 = 14;
const CHIP_DOT: i32 = 6;
const FIELD_H: i32 = 42;
const FIELD_PAD: i32 = 6;
const USER_H: i32 = 30;
const AVATAR: i32 = 22;
const HINT_H: i32 = 20;
const KBD_H: i32 = 20;
const FOOTER_PAD_Y: i32 = 10;
const PILL_H: i32 = 20;
pub(super) const SPINNER: i32 = 14;

const fn style(size_px: f32, line_height_px: f32, weight: u16) -> TextStyle {
    TextStyle { size_px, line_height_px, weight, tracking_em: 0.0, uppercase: false, tabular: false }
}

const TITLE: TextStyle = style(15.0, 20.0, 650);
const CALL: TextStyle = style(13.0, 19.0, 400);
const CHIP: TextStyle = style(12.0, 16.0, 500);
const CHIP_ON: TextStyle = style(12.0, 16.0, 600);
const USER: TextStyle = style(12.0, 16.0, 500);
const INITIAL: TextStyle = style(11.0, 14.0, 630);
const BULLETS: TextStyle = TextStyle { tracking_em: 0.12, tabular: true, ..style(16.0, 20.0, 400) };
const FIELD: TextStyle = style(14.0, 18.0, 400);
const CAP: TextStyle = style(12.0, 16.0, 450);
const KBD: TextStyle = TextStyle { tabular: true, ..style(11.0, 14.0, 400) };
const PILL: TextStyle = style(12.0, 16.0, 600);
const SMALL: TextStyle = style(12.5, 16.0, 520);

const KEY: &str = "<circle cx=\"8\" cy=\"15\" r=\"4\"/><path d=\"M10.8 12.2 20 3M16 7l3 3M14 9l2 2\"/>";
const LOCK: &str = "<rect x=\"5\" y=\"11\" width=\"14\" height=\"10\" rx=\"2\"/><path d=\"M8 11V7a4 4 0 0 1 8 0v4\"/>";
const CLOSE: &str = "<path d=\"M6 6l12 12M18 6 6 18\"/>";
const CHECK: &str = "<path d=\"m5 12 5 5 9-10\"/>";
const EYE: &str = "<path d=\"M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7S2 12 2 12z\"/><circle cx=\"12\" cy=\"12\" r=\"3\"/>";
const EYE_OFF: &str = "<path d=\"M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7S2 12 2 12z\"/><circle cx=\"12\" cy=\"12\" r=\"3\"/><path d=\"M4 4l16 16\"/>";
const ARROW: &str = "<path d=\"M5 12h14M13 6l6 6-6 6\"/>";
const ALERT: &str = "<path d=\"M12 3 2 20h20z\"/><path d=\"M12 10v4M12 17h.01\"/>";
const SHIELD: &str = "<path d=\"M12 3 4 6v6c0 5 3.4 8.4 8 9 4.6-.6 8-4 8-9V6z\"/><path d=\"m9 12 2 2 4-4\"/>";

fn svg(body: &str, stroke: f32) -> String {
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" \
         stroke=\"#000\" stroke-width=\"{stroke}\" stroke-linecap=\"round\" stroke-linejoin=\"round\">{body}</svg>"
    )
}

fn icon(c: &mut Canvas<'_>, body: &str, stroke: f32, side: i32, rect: LocalRect, colour: Color) {
    svg_glyph(c, &svg(body, stroke), side, rect, colour);
}

const fn line_h(style: &TextStyle) -> i32 {
    style.line_height_px as i32
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Layout {
    pub height: i32,
    close: LocalRect,
    tile: LocalRect,
    text_x: i32,
    title_y: i32,
    body: String,
    body_y: i32,
    chips: Vec<LocalRect>,
    field: LocalRect,
    user: LocalRect,
    reveal: Option<LocalRect>,
    submit: LocalRect,
    hint_y: i32,
    footer_y: i32,
    pill: LocalRect,
    cancel: LocalRect,
}

fn chip_width(painter: &Painter, label: &str, state: Chip) -> i32 {
    let lead = match state {
        Chip::Done => CHIP_ICON + 6,
        Chip::Current => CHIP_DOT + 6,
        Chip::Next | Chip::Later => 0,
    };
    let style = if state == Chip::Current { &CHIP_ON } else { &CHIP };
    CHIP_PAD + lead + text_w(painter, label, style) + CHIP_PAD
}

#[must_use]
pub(super) fn layout(painter: &Painter, view: &View) -> Layout {
    let tile = local_rect(PAD_X, PAD_TOP, TILE, TILE);
    let close = local_rect(WIDTH - PAD_X - BUTTON, PAD_TOP + (TILE - BUTTON) / 2, BUTTON, BUTTON);
    let text_x = PAD_X + TILE + GAP;
    let title_y = PAD_TOP + (TILE - line_h(&TITLE) - line_h(&CALL)) / 2;
    let body_width = close.loc.x - GAP - text_x;
    let body = crate::notifications::wrap_lines(painter, &view.body, &CALL, body_width, 1).into_iter().next();
    let mut y = PAD_TOP + TILE + GAP;
    let mut chips = Vec::new();
    if !view.chips.is_empty() {
        let mut x = PAD_X;
        for (label, state) in &view.chips {
            let width = chip_width(painter, label, *state);
            chips.push(local_rect(x, y, width, CHIP_H));
            x += width + CHIP_GAP;
        }
        y += CHIP_H + GAP;
    }
    let field = local_rect(PAD_X, y, WIDTH - PAD_X * 2, FIELD_H);
    let inner = field.loc.x + 1 + FIELD_PAD;
    let user_w = 4 + AVATAR + 8 + text_w(painter, &view.user, &USER) + 10;
    let user = local_rect(inner, y + (FIELD_H - USER_H) / 2, user_w, USER_H);
    let submit =
        local_rect(field.loc.x + field.size.w - 1 - FIELD_PAD - BUTTON, y + (FIELD_H - BUTTON) / 2, BUTTON, BUTTON);
    let reveal =
        (!view.checking).then(|| local_rect(submit.loc.x - 8 - BUTTON, y + (FIELD_H - BUTTON) / 2, BUTTON, BUTTON));
    let hint_y = y + FIELD_H + GAP;
    let footer_y = hint_y + HINT_H + PAD_BOTTOM;
    let row_y = footer_y + 1 + FOOTER_PAD_Y;
    let pill_w = 7 + CHIP_ICON + 4 + text_w(painter, t(Key::PkProtected), &PILL) + 7;
    let pill = local_rect(PAD_X, row_y + (BUTTON - PILL_H) / 2, pill_w, PILL_H);
    let cancel_w = 11 + text_w(painter, t(Key::PkCancel), &SMALL) + 11;
    let cancel = local_rect(WIDTH - PAD_X - cancel_w, row_y, cancel_w, BUTTON);
    Layout {
        height: row_y + BUTTON + FOOTER_PAD_Y,
        close,
        tile,
        text_x,
        title_y,
        body: body.unwrap_or_default(),
        body_y: title_y + line_h(&TITLE),
        chips,
        field,
        user,
        reveal,
        submit,
        hint_y,
        footer_y,
        pill,
        cancel,
    }
}

#[must_use]
pub(super) fn slots(layout: &Layout, view: &View) -> Vec<(Control, LocalRect)> {
    let controls = [
        (Control::Close, Some(layout.close)),
        (Control::Submit, view.ready.then_some(layout.submit)),
        (Control::Reveal, layout.reveal),
        (Control::Cancel, Some(layout.cancel)),
        (Control::Field, Some(layout.field)),
    ];
    controls.into_iter().filter_map(|(control, rect)| Some((control, rect?))).collect()
}

#[must_use]
pub(super) const fn layer_radius(control: Control) -> Option<f32> {
    match control {
        Control::Field => None,
        Control::Submit => Some(R1),
        Control::Close | Control::Reveal | Control::Cancel => Some(R2),
    }
}

#[must_use]
pub(super) fn spinner_rect(layout: &Layout) -> LocalRect {
    let submit = layout.submit;
    local_rect(submit.loc.x + (BUTTON - SPINNER) / 2, submit.loc.y + (BUTTON - SPINNER) / 2, SPINNER, SPINNER)
}

struct Ink<'a> {
    painter: &'a Painter,
    theme: &'a TopbarTheme,
}

fn draw_head(c: &mut Canvas<'_>, ink: &Ink<'_>, view: &View, layout: &Layout) {
    let theme = ink.theme;
    let tile = layout.tile;
    c.fill(tile, theme.active_soft, R3);
    icon(c, KEY, 1.8, 24, tile, theme.active_soft_label);
    let x = tile.loc.x + TILE + BADGE_OUT - BADGE;
    let y = tile.loc.y + TILE + BADGE_OUT - BADGE;
    let ring = local_rect(x - BADGE_RING, y - BADGE_RING, BADGE + BADGE_RING * 2, BADGE + BADGE_RING * 2);
    c.fill(ring, theme.island_bg, R_ROUND);
    let badge = local_rect(x, y, BADGE, BADGE);
    c.fill(badge, theme.active_background, R_ROUND);
    icon(c, LOCK, 2.4, 12, badge, theme.active_label);

    let right = layout.close.loc.x - GAP;
    let top = layout.title_y;
    let band = |left: i32| (left, right, top, top + line_h(&TITLE));
    let (app, asks) = match &view.app {
        Some(app) => (app.as_str(), Key::KcAsksOne),
        None => (t(Key::KcApp), Key::KcAsks),
    };
    c.text(ink.painter, band(layout.text_x), TextAlign::Left, app, theme.active_soft_label, &TITLE);
    let rest = format!(" {}", t(asks));
    let left = layout.text_x + text_w(ink.painter, app, &TITLE);
    c.text(ink.painter, band(left), TextAlign::Left, &rest, theme.label, &TITLE);
    let band = (layout.text_x, right, layout.body_y, layout.body_y + line_h(&CALL));
    c.text(ink.painter, band, TextAlign::Left, &layout.body, theme.muted, &CALL);
    icon(c, CLOSE, 1.8, 16, layout.close, theme.muted);
}

fn draw_chips(c: &mut Canvas<'_>, ink: &Ink<'_>, view: &View, layout: &Layout) {
    let theme = ink.theme;
    for ((label, state), rect) in view.chips.iter().zip(&layout.chips) {
        let (fill, ink_colour, style) = match state {
            Chip::Done => (theme.ok_soft, theme.ok, &CHIP),
            Chip::Current => (theme.active_soft, theme.active_soft_label, &CHIP_ON),
            Chip::Next | Chip::Later => (theme.track_soft, theme.muted, &CHIP),
        };
        c.fill(*rect, fill, R1);
        let mut x = rect.loc.x + CHIP_PAD;
        match state {
            Chip::Done => {
                icon(c, CHECK, 1.8, CHIP_ICON, local_rect(x, rect.loc.y, CHIP_ICON, CHIP_H), ink_colour);
                x += CHIP_ICON + 6;
            }
            Chip::Current => {
                let dot = local_rect(x, rect.loc.y + (CHIP_H - CHIP_DOT) / 2, CHIP_DOT, CHIP_DOT);
                c.fill(dot, theme.active_background, R_ROUND);
                x += CHIP_DOT + 6;
            }
            Chip::Next | Chip::Later => {}
        }
        let band = (x, rect.loc.x + rect.size.w, rect.loc.y, rect.loc.y + CHIP_H);
        c.text(ink.painter, band, TextAlign::Left, label, ink_colour, style);
    }
}

fn initial(name: &str) -> String {
    name.chars().next().unwrap_or('?').to_uppercase().to_string()
}

fn tail_that_fits<'a>(painter: &Painter, text: &'a str, width: i32, style: &TextStyle) -> &'a str {
    let starts: Vec<usize> = text.char_indices().map(|(index, _)| index).collect();
    let first = starts.partition_point(|&start| text_w(painter, &text[start..], style) > width);
    starts.get(first).map_or("", |&start| &text[start..])
}

fn draw_field(c: &mut Canvas<'_>, ink: &Ink<'_>, view: &View, layout: &Layout) {
    let theme = ink.theme;
    let field = layout.field;
    let wrong = matches!(view.hint, Hint::Wrong { .. });
    let (border, ring) =
        if wrong { (theme.danger, theme.danger_soft) } else { (theme.active_background, theme.acc_ring) };
    let halo = local_rect(field.loc.x - 3, field.loc.y - 3, field.size.w + 6, field.size.h + 6);
    c.fill(halo, ring, R2 + 3.0);
    c.fill(field, theme.view_bg, R2);
    c.stroke(field, 1.0, R2, border);

    let user = layout.user;
    c.fill(user, theme.track_soft, R1);
    let avatar = local_rect(user.loc.x + 4, user.loc.y + (USER_H - AVATAR) / 2, AVATAR, AVATAR);
    c.fill(avatar, theme.active_soft, R_ROUND);
    c.text_in(ink.painter, avatar, TextAlign::Center, &initial(&view.user), theme.active_soft_label, &INITIAL);
    let band = (avatar.loc.x + AVATAR + 8, user.loc.x + user.size.w, user.loc.y, user.loc.y + USER_H);
    c.text(ink.painter, band, TextAlign::Left, &view.user, theme.label, &USER);

    let left = user.loc.x + user.size.w + 8;
    let right = layout.reveal.map_or(layout.submit.loc.x, |eye| eye.loc.x) - 8;
    let (shown, style) = match view.shown.as_deref() {
        Some(text) => (text.to_string(), &FIELD),
        None => ("\u{2022}".repeat(view.bullets), &BULLETS),
    };
    let tail = tail_that_fits(ink.painter, &shown, right - left - 4, style);
    let band = (left, right, field.loc.y, field.loc.y + FIELD_H);
    if tail.is_empty() {
        let placeholder = if view.master { t(Key::KcMaster) } else { t(Key::PkPassword) };
        c.text(ink.painter, band, TextAlign::Left, placeholder, theme.faint, &FIELD);
    } else {
        let colour = if view.checking { theme.label.with_alpha(theme.label.0[3] * 0.75) } else { theme.label };
        c.text(ink.painter, band, TextAlign::Left, tail, colour, style);
    }
    if !view.checking {
        let typed = if tail.is_empty() { 0 } else { text_w(ink.painter, tail, style) };
        let caret_x = left + typed + 1;
        if caret_x < right {
            c.fill(local_rect(caret_x, field.loc.y + (FIELD_H - 17) / 2, 2, 17), theme.active_background, 0.0);
        }
    }
    if let Some(eye) = layout.reveal {
        icon(c, if view.shown.is_some() { EYE_OFF } else { EYE }, 1.8, 16, eye, theme.muted);
    }
    let ready = view.ready || view.checking;
    let fill = if ready { theme.active_background } else { theme.active_background.with_alpha(0.4) };
    c.fill(layout.submit, fill, R1);
    if !view.checking {
        let colour = if view.ready { theme.active_label } else { theme.active_label.with_alpha(0.6) };
        icon(c, ARROW, 1.8, 16, layout.submit, colour);
    }
}

fn kbd(c: &mut Canvas<'_>, ink: &Ink<'_>, x: i32, y: i32, label: &str) -> i32 {
    let width = (text_w(ink.painter, label, &KBD) + 10).max(20);
    let rect = local_rect(x, y + (HINT_H - KBD_H) / 2, width, KBD_H);
    c.fill(rect, ink.theme.track_soft, R1);
    c.stroke(rect, 1.0, R1, ink.theme.line);
    c.text_in(ink.painter, rect, TextAlign::Center, label, ink.theme.muted, &KBD);
    x + width
}

fn draw_hint(c: &mut Canvas<'_>, ink: &Ink<'_>, view: &View, layout: &Layout) {
    let theme = ink.theme;
    let y = layout.hint_y;
    let band = |x: i32| (x, WIDTH - PAD_X, y, y + HINT_H);
    let mut x = PAD_X;
    let say = |c: &mut Canvas<'_>, x: &mut i32, text: &str, colour: Color| {
        c.text(ink.painter, band(*x), TextAlign::Left, text, colour, &CAP);
        *x += text_w(ink.painter, text, &CAP) + GAP;
    };
    match &view.hint {
        Hint::Checking => say(c, &mut x, t(Key::KcChecking), theme.faint),
        Hint::CapsLock => say(c, &mut x, t(Key::PkCapsLock), theme.warn),
        Hint::Wrong { master, caps } => {
            icon(c, ALERT, 1.8, CHIP_ICON, local_rect(x, y, CHIP_ICON, HINT_H), theme.danger);
            x += CHIP_ICON + 6;
            say(c, &mut x, t(if *master { Key::KcWrongMaster } else { Key::KcWrong }), theme.danger);
            if *caps {
                say(c, &mut x, t(Key::PkCapsLock), theme.warn);
            }
        }
        Hint::Keys { skip } => {
            let skip = skip.as_ref().map(|name| t(Key::KcSkip).replace("{name}", name));
            let keys = [
                ("\u{21b5}", Some(t(Key::KcUnlockKey).to_string())),
                ("Esc", Some(t(Key::PkCancelKey).to_string())),
                ("Tab", skip),
            ];
            for (key, text) in keys {
                let Some(text) = text else {
                    continue;
                };
                x = kbd(c, ink, x, y, key) + 4;
                say(c, &mut x, &text, theme.faint);
            }
        }
    }
}

fn draw_footer(c: &mut Canvas<'_>, ink: &Ink<'_>, layout: &Layout) {
    let theme = ink.theme;
    let footer = theme.island_bg.state_layer(theme.label, theme.track_soft.0[3]);
    let bottom = layout.height - layout.footer_y;
    c.fill(local_rect(0, layout.footer_y, WIDTH, bottom), footer, RADIUS);
    c.fill(local_rect(0, layout.footer_y, WIDTH, RADIUS as i32 + 1), footer, 0.0);
    c.fill(local_rect(0, layout.footer_y, WIDTH, 1), theme.island_line, 0.0);

    let pill = layout.pill;
    c.fill(pill, theme.ok_soft, R_ROUND);
    icon(c, SHIELD, 1.8, CHIP_ICON, local_rect(pill.loc.x + 7, pill.loc.y, CHIP_ICON, PILL_H), theme.ok);
    let band = (pill.loc.x + 7 + CHIP_ICON + 4, pill.loc.x + pill.size.w, pill.loc.y, pill.loc.y + PILL_H);
    c.text(ink.painter, band, TextAlign::Left, t(Key::PkProtected), theme.ok, &PILL);

    let cancel = layout.cancel;
    let left = pill.loc.x + pill.size.w + 12;
    let right = cancel.loc.x - 12;
    let note = crate::notifications::wrap_lines(ink.painter, t(Key::KcShieldNote), &CAP, right - left, 1);
    if let Some(note) = note.first() {
        c.text(
            ink.painter,
            (left, right, cancel.loc.y, cancel.loc.y + BUTTON),
            TextAlign::Left,
            note,
            theme.faint,
            &CAP,
        );
    }
    c.fill(cancel, theme.track_soft, R2);
    c.text_in(ink.painter, cancel, TextAlign::Center, t(Key::PkCancel), theme.label, &SMALL);
}

pub(super) fn paint(
    painter: &Painter, theme: &TopbarTheme, view: &View, layout: &Layout, scale: i32,
) -> Option<MemoryRenderBuffer> {
    let mut c = Canvas::new(WIDTH, layout.height, scale)?;
    paint_into(&mut c, painter, theme, view, layout);
    Some(c.finish())
}

fn paint_into(c: &mut Canvas<'_>, painter: &Painter, theme: &TopbarTheme, view: &View, layout: &Layout) {
    let ink = Ink { painter, theme };
    draw_footer(c, &ink, layout);
    draw_head(c, &ink, view, layout);
    draw_chips(c, &ink, view, layout);
    draw_field(c, &ink, view, layout);
    draw_hint(c, &ink, view, layout);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view() -> View {
        View {
            app: None,
            chips: vec![
                ("Особисте".into(), Chip::Done),
                ("Bitwarden".into(), Chip::Current),
                ("Bitwarden · дім · далі".into(), Chip::Next),
            ],
            body: "Майстер-пароль Bitwarden для artem@example.com".into(),
            user: "artem".into(),
            master: true,
            bullets: 12,
            shown: None,
            hint: Hint::Keys { skip: Some("Bitwarden".into()) },
            ready: true,
            checking: false,
        }
    }

    fn painter() -> Painter {
        Painter::new(14.0, None)
    }

    #[test]
    fn the_prompt_stacks_head_chips_field_hint_and_footer_like_the_canvas() {
        let painter = painter();
        let layout = layout(&painter, &view());
        assert_eq!(layout.tile, local_rect(24, 20, 56, 56));
        assert_eq!(layout.chips[0].loc.y, 92);
        assert_eq!(layout.field.loc.y, 92 + CHIP_H + GAP);
        assert_eq!(layout.field.size, (664, 42).into());
        assert_eq!(layout.hint_y, layout.field.loc.y + FIELD_H + GAP);
        assert_eq!(layout.footer_y, layout.hint_y + HINT_H + PAD_BOTTOM);
        assert_eq!(layout.height, layout.footer_y + 1 + FOOTER_PAD_Y * 2 + BUTTON);
        assert_eq!(layout.cancel.loc.x + layout.cancel.size.w, WIDTH - PAD_X);
        let (user, eye, go) = (layout.user, layout.reveal.expect("eye"), layout.submit);
        assert!(user.loc.x + user.size.w < eye.loc.x && eye.loc.x + eye.size.w < go.loc.x);
        assert!(layout.chips.windows(2).all(|pair| pair[0].loc.x + pair[0].size.w + CHIP_GAP == pair[1].loc.x));
    }

    #[test]
    fn one_vault_drops_the_chip_row_and_checking_drops_the_eye_and_the_submit_slot() {
        let painter = painter();
        let single = View { chips: Vec::new(), ..view() };
        assert_eq!(layout(&painter, &single).field.loc.y, 92);
        let checking = View { checking: true, ready: false, hint: Hint::Checking, ..view() };
        let laid = layout(&painter, &checking);
        assert!(laid.reveal.is_none());
        assert!(!slots(&laid, &checking).iter().any(|(control, _)| *control == Control::Submit));
    }

    #[test]
    fn every_icon_is_valid_svg() {
        for body in [KEY, LOCK, CLOSE, CHECK, EYE, EYE_OFF, ARROW, ALERT, SHIELD] {
            assert!(crate::paint::render_svg(svg(body, 1.8).as_bytes(), 24).is_some(), "{body}");
        }
    }

    #[test]
    #[ignore = "writes preview PNGs to $GLIDE_KEYCHAIN_PREVIEW"]
    fn preview() {
        let Some(dir) = std::env::var_os("GLIDE_KEYCHAIN_PREVIEW") else {
            return;
        };
        let painter = painter();
        let scale = 2;
        for appearance in [glide_theme::Appearance::Dark, glide_theme::Appearance::Light] {
            let theme = TopbarTheme::from_tokens(&glide_theme::Tokens::new(appearance, glide_theme::Accent::Blue));
            let cases = [
                ("ask", view()),
                ("program", View { app: Some("Google Chrome".into()), chips: Vec::new(), ..view() }),
                ("wrong", View { hint: Hint::Wrong { master: true, caps: true }, bullets: 0, ready: false, ..view() }),
                ("checking", View { hint: Hint::Checking, checking: true, ready: false, ..view() }),
                (
                    "single",
                    View {
                        chips: Vec::new(),
                        hint: Hint::Keys { skip: None },
                        bullets: 0,
                        ready: false,
                        master: false,
                        body: "Пароль сховища «Особисте»".into(),
                        ..view()
                    },
                ),
            ];
            for (name, case) in cases {
                let layout = layout(&painter, &case);
                let mut surface =
                    Canvas::popover((WIDTH, layout.height), scale, RADIUS, theme.island_bg, theme.island_line)
                        .expect("surface");
                paint_into(&mut surface, &painter, &theme, &case, &layout);
                let width = (WIDTH * scale) as u32 + 64;
                let height = (layout.height * scale) as u32 + 64;
                let mut image = crate::shot::compose::Image::new(width, height).expect("image");
                let shade = if appearance == glide_theme::Appearance::Dark { 20 } else { 230 };
                let backdrop = vec![shade; (width * height * 4) as usize];
                image.draw(&crate::shot::compose::Image::from_bgra(width, height, backdrop).expect("bg"), 0, 0);
                let prompt = crate::shot::compose::Image::from_bgra(
                    (WIDTH * scale) as u32,
                    (layout.height * scale) as u32,
                    surface.buf.clone(),
                )
                .expect("prompt");
                image.draw(&prompt, 32, 32);
                let png = crate::backend::screenshot::save_rgba(&image.into_straight(), width, height, false)
                    .expect("png")
                    .png;
                let file = format!("keychain-{name}-{appearance:?}.png").to_lowercase();
                std::fs::write(std::path::Path::new(&dir).join(file), png).expect("write");
            }
        }
    }
}

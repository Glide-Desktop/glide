use std::fmt::Write as _;

use crate::elevation::{self, Level};
use crate::motion::{self, CubicBezier};
use crate::{Appearance, Color, TextStyle, Tokens, component, shape, spacing};

pub const VERSION: u32 = 1;

fn hex(color: Color) -> String {
    let [r, g, b, a] = color.0.map(|channel| (channel.clamp(0.0, 1.0) * 255.0).round() as u8);
    format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
}

fn number(value: f32) -> String {
    let text = format!("{value:.4}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text.is_empty() || text == "-" { "0".into() } else { text.into() }
}

#[must_use]
pub fn colors(t: &Tokens) -> Vec<(&'static str, Color)> {
    vec![
        ("win-bg", t.win_bg),
        ("view-bg", t.view_bg),
        ("sidebar-bg", t.sidebar_bg),
        ("bar-bg", t.bar_bg),
        ("bar-bg-idle", t.bar_bg_idle),
        ("card-bg", t.card_bg),
        ("popover-bg", t.popover_bg),
        ("sunken-bg", t.sunken_bg),
        ("inverse-bg", t.inverse_bg),
        ("inverse-fg", t.inverse_fg),
        ("fg-1", t.fg_1),
        ("fg-2", t.fg_2),
        ("fg-3", t.fg_3),
        ("fg-4", t.fg_4),
        ("line", t.line),
        ("line-2", t.line_2),
        ("fill-1", t.fill_1),
        ("fill-2", t.fill_2),
        ("fill-3", t.fill_3),
        ("state", t.state),
        ("acc-bg", t.acc_bg),
        ("acc-bg-hover", t.acc_bg_hover),
        ("acc-fg", t.acc_fg),
        ("acc-text", t.acc_text),
        ("acc-soft", t.acc_soft),
        ("acc-soft-fg", t.acc_soft_fg),
        ("acc-ring", t.acc_ring),
        ("ok", t.ok),
        ("ok-soft", t.ok_soft),
        ("warn", t.warn),
        ("warn-soft", t.warn_soft),
        ("danger", t.danger),
        ("danger-soft", t.danger_soft),
        ("switch-thumb", t.switch_thumb()),
        ("segment-thumb", t.segment_thumb()),
        ("term-bg", t.term_bg),
        ("term-fg", t.term_fg),
    ]
}

const TYPE: [(&str, TextStyle); 11] = [
    ("display", TextStyle::DISPLAY),
    ("t1", TextStyle::T1),
    ("t2", TextStyle::T2),
    ("t3", TextStyle::T3),
    ("head", TextStyle::HEAD),
    ("body", TextStyle::BODY),
    ("call", TextStyle::CALL),
    ("cap", TextStyle::CAP),
    ("micro", TextStyle::MICRO),
    ("num", TextStyle::NUM),
    ("menu-item", TextStyle::MENU_ITEM),
];

fn object<T>(out: &mut String, items: impl IntoIterator<Item = (impl AsRef<str>, T)>, value: impl Fn(&mut String, T)) {
    out.push('{');
    for (index, (key, item)) in items.into_iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        let _ = write!(out, "\"{}\":", key.as_ref());
        value(out, item);
    }
    out.push('}');
}

fn bezier(curve: CubicBezier) -> String {
    format!("[{},{},{},{}]", number(curve.x1), number(curve.y1), number(curve.x2), number(curve.y2))
}

fn palette(out: &mut String, tokens: &Tokens) {
    object(out, colors(tokens), |out, color| {
        let _ = write!(out, "\"{}\"", hex(color));
    });
}

fn shadows(out: &mut String, appearance: Appearance) {
    let levels = [("el1", Level::El1), ("el2", Level::El2), ("el3", Level::El3), ("el-win", Level::ElWin)];
    object(out, levels, |out, level| {
        out.push('[');
        for (index, layer) in elevation::layers(level, appearance).into_iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            let _ = write!(
                out,
                "{{\"dx\":{},\"dy\":{},\"blur\":{},\"spread\":{},\"color\":\"{}\"}}",
                number(layer.dx),
                number(layer.dy),
                number(layer.blur),
                number(layer.spread),
                hex(layer.color)
            );
        }
        out.push(']');
    });
}

#[must_use]
pub fn json(tokens: &Tokens) -> String {
    let mut out = String::new();
    let _ = write!(
        out,
        "{{\"version\":{VERSION},\"appearance\":\"{}\",\"accent\":\"{}\",\"colors\":",
        tokens.appearance.key(),
        tokens.accent.key()
    );
    palette(&mut out, tokens);
    let other = Tokens::new(
        match tokens.appearance {
            Appearance::Light => Appearance::Dark,
            Appearance::Dark => Appearance::Light,
        },
        tokens.accent,
    );
    let _ = write!(out, ",\"{}\":", other.appearance.key());
    palette(&mut out, &other);
    out.push_str(",\"radius\":");
    object(&mut out, [("r1", shape::R1), ("r2", shape::R2), ("r3", shape::R3), ("r4", shape::R4)], |out, value| {
        out.push_str(&number(value));
    });
    out.push_str(",\"spacing\":");
    object(
        &mut out,
        spacing::STEPS.iter().enumerate().map(|(index, step)| (format!("sp{}", index + 1), *step)),
        |out, value| {
            let _ = write!(out, "{value}");
        },
    );
    out.push_str(",\"type\":");
    object(&mut out, TYPE, |out, style| {
        let _ = write!(
            out,
            "{{\"size\":{},\"line-height\":{},\"weight\":{},\"tracking-em\":{},\"uppercase\":{},\"tabular\":{}}}",
            number(style.size_px),
            number(style.line_height_px),
            style.weight,
            number(style.tracking_em),
            style.uppercase,
            style.tabular
        );
    });
    let _ = write!(
        out,
        ",\"motion\":{{\"t1-ms\":{},\"t2-ms\":{},\"t3-ms\":{},\"t4-ms\":{},\"e-out\":{},\"e-io\":{},\"e-over\":{},\"hover\":{},\"press\":{},\"drag\":{}}}",
        motion::T1.as_millis(),
        motion::T2.as_millis(),
        motion::T3.as_millis(),
        motion::T4.as_millis(),
        bezier(motion::E_OUT),
        bezier(motion::E_IO),
        bezier(motion::E_OVER),
        number(motion::ST_HOVER),
        number(motion::ST_PRESS),
        number(motion::ST_DRAG)
    );
    out.push_str(",\"elevation\":");
    shadows(&mut out, tokens.appearance);
    let _ = write!(
        out,
        ",\"component\":{{\"control\":{},\"small-control\":{},\"large-control\":{},\"window-header\":{},\"menu-item\":{},\"field-halo\":{},\"idle-controls\":{},\"tooltip-delay-ms\":{},\"button-pressed-scale\":{},\"popover-closed-scale\":{},\"popover-rise\":{},\"dialog-closed-scale\":{}}}}}",
        number(component::CONTROL),
        number(component::SMALL_CONTROL),
        number(component::LARGE_CONTROL),
        number(component::WINDOW_HEADER),
        number(component::MENU_ITEM),
        number(component::FIELD_HALO),
        number(component::IDLE_CONTROLS),
        component::TOOLTIP_DELAY.as_millis(),
        number(component::BUTTON_PRESSED_SCALE),
        number(component::POPOVER_CLOSED_SCALE),
        number(component::POPOVER_RISE),
        number(component::DIALOG_CLOSED_SCALE)
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Accent;

    #[test]
    fn the_export_is_valid_json_with_both_palettes() {
        let tokens = Tokens::new(Appearance::Dark, Accent::Teal);
        let value: serde_json::Value = serde_json::from_str(&json(&tokens)).expect("valid JSON");
        assert_eq!(value["version"], VERSION);
        assert_eq!(value["appearance"], "dark");
        assert_eq!(value["accent"], "teal");
        assert_eq!(value["colors"]["acc-bg"], hex(tokens.acc_bg));
        let light = Tokens::new(Appearance::Light, Accent::Teal);
        assert_eq!(value["light"]["win-bg"], hex(light.win_bg));
        let float = |value: &serde_json::Value| value.as_f64().expect("a number");
        assert_eq!(float(&value["radius"]["r3"]), 12.0);
        assert_eq!(value["spacing"]["sp4"], 16);
        assert_eq!(value["type"]["body"]["weight"], TextStyle::BODY.weight);
        let curve: Vec<f64> = value["motion"]["e-out"].as_array().expect("a curve").iter().map(float).collect();
        assert_eq!(curve, [0.2, 0.0, 0.0, 1.0]);
        assert!(value["elevation"]["el3"].as_array().is_some_and(|layers| !layers.is_empty()));
        assert_eq!(float(&value["component"]["control"]), 34.0);
    }

    #[test]
    fn colours_print_as_eight_digit_hex() {
        assert_eq!(hex(Color([1.0, 0.0, 0.5, 0.5])), "#ff008080");
        assert_eq!(number(0.0), "0");
        assert_eq!(number(-0.012), "-0.012");
        assert_eq!(number(12.0), "12");
    }
}

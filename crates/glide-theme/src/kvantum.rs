use std::fmt::Write;

use crate::export::{DISABLED_OPACITY as DISABLED, over};
use crate::motion::{ST_HOVER, ST_PRESS};
use crate::shape::{R1, R2, R3};
use crate::{Color, Tokens};

pub const THEME: &str = "Glide";

const STATES: [&str; 5] = ["normal", "focused", "pressed", "toggled", "disabled"];
const EDGE: f32 = 10.0;
const GAP: f32 = 4.0;
const ROW: f32 = 1200.0;

#[derive(Clone, Copy)]
struct Look {
    fill: Color,
    border: Color,
}

const CLEAR: Color = Color([0.0, 0.0, 0.0, 0.0]);

impl Look {
    const fn new(fill: Color, border: Color) -> Self {
        Self { fill, border }
    }

    const fn flat(fill: Color) -> Self {
        Self { fill, border: CLEAR }
    }
}

fn channel(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn hex(color: Color) -> String {
    let [r, g, b, _] = color.0;
    format!("#{:02x}{:02x}{:02x}", channel(r), channel(g), channel(b))
}

fn paint(color: Color) -> String {
    format!("fill=\"{}\" fill-opacity=\"{:.3}\"", hex(color), color.0[3].clamp(0.0, 1.0))
}

fn ink(color: Color, width: f32) -> String {
    format!(
        "fill=\"none\" stroke=\"{}\" stroke-opacity=\"{:.3}\" stroke-width=\"{width}\" stroke-linecap=\"round\" stroke-linejoin=\"round\"",
        hex(color),
        color.0[3].clamp(0.0, 1.0)
    )
}

const fn visible(color: Color) -> bool {
    color.0[3] > 0.001
}

struct Svg {
    body: String,
    x: f32,
    y: f32,
    tallest: f32,
}

impl Svg {
    const fn new() -> Self {
        Self { body: String::new(), x: 0.0, y: 0.0, tallest: 0.0 }
    }

    fn place(&mut self, w: f32, h: f32) -> (f32, f32) {
        if self.x + w > ROW {
            self.x = 0.0;
            self.y += self.tallest + GAP;
            self.tallest = 0.0;
        }
        let at = (self.x, self.y);
        self.x += w + GAP;
        self.tallest = self.tallest.max(h);
        at
    }

    fn group(&mut self, id: &str, w: f32, h: f32, draw: impl FnOnce(f32, f32) -> String) {
        let (x, y) = self.place(w, h);
        let _ = writeln!(
            self.body,
            "<g id=\"{id}\"><rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" fill=\"#000000\" fill-opacity=\"0\"/>{}</g>",
            draw(x, y)
        );
    }

    fn rect(x: f32, y: f32, w: f32, h: f32, color: Color) -> String {
        if visible(color) {
            format!("<rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" {}/>", paint(color))
        } else {
            String::new()
        }
    }

    fn path(d: &str, fill: Color) -> String {
        if visible(fill) { format!("<path d=\"{d}\" {}/>", paint(fill)) } else { String::new() }
    }

    fn stroke(d: &str, color: Color, width: f32) -> String {
        if visible(color) { format!("<path d=\"{d}\" {}/>", ink(color, width)) } else { String::new() }
    }

    fn frame(&mut self, id: &str, look: Look, r: f32, b: f32) {
        let (fill, border) = (look.fill, look.border);
        let i = r - b / 2.0;
        self.group(id, EDGE, EDGE, |x, y| Self::rect(x, y, EDGE, EDGE, fill));
        self.group(&format!("{id}-top"), EDGE, r, |x, y| {
            Self::rect(x, y, EDGE, r, fill) + &Self::rect(x, y, EDGE, b, border)
        });
        self.group(&format!("{id}-bottom"), EDGE, r, |x, y| {
            Self::rect(x, y, EDGE, r, fill) + &Self::rect(x, y + r - b, EDGE, b, border)
        });
        self.group(&format!("{id}-left"), r, EDGE, |x, y| {
            Self::rect(x, y, r, EDGE, fill) + &Self::rect(x, y, b, EDGE, border)
        });
        self.group(&format!("{id}-right"), r, EDGE, |x, y| {
            Self::rect(x, y, r, EDGE, fill) + &Self::rect(x + r - b, y, b, EDGE, border)
        });
        let h = b / 2.0;
        self.group(&format!("{id}-topleft"), r, r, |x, y| {
            Self::path(&format!("M{},{} H{x} A{r},{r} 0 0 1 {},{y} Z", x + r, y + r, x + r), fill)
                + &Self::stroke(&format!("M{},{} A{i},{i} 0 0 1 {},{}", x + h, y + r, x + r, y + h), border, b)
        });
        self.group(&format!("{id}-topright"), r, r, |x, y| {
            Self::path(&format!("M{x},{} V{y} A{r},{r} 0 0 1 {},{} Z", y + r, x + r, y + r), fill)
                + &Self::stroke(&format!("M{x},{} A{i},{i} 0 0 1 {},{}", y + h, x + r - h, y + r), border, b)
        });
        self.group(&format!("{id}-bottomright"), r, r, |x, y| {
            Self::path(&format!("M{x},{y} H{} A{r},{r} 0 0 1 {x},{} Z", x + r, y + r), fill)
                + &Self::stroke(&format!("M{},{y} A{i},{i} 0 0 1 {x},{}", x + r - h, y + r - h), border, b)
        });
        self.group(&format!("{id}-bottomleft"), r, r, |x, y| {
            Self::path(&format!("M{},{y} V{} A{r},{r} 0 0 1 {x},{y} Z", x + r, y + r), fill)
                + &Self::stroke(&format!("M{},{} A{i},{i} 0 0 1 {},{y}", x + r, y + r - h, x + h), border, b)
        });
    }

    fn frames(&mut self, name: &str, looks: [Look; 5], r: f32, b: f32) {
        for (state, look) in STATES.into_iter().zip(looks) {
            self.frame(&format!("{name}-{state}"), look, r, b);
        }
    }

    fn edges(&mut self, id: &str, w: f32, fill: Color, bars: [Color; 4]) {
        let [top, right, bottom, left] = bars;
        let side = |x: f32, y: f32, sw: f32, sh: f32, bar: Color, at: (f32, f32, f32, f32)| {
            Self::rect(x, y, sw, sh, fill) + &Self::rect(at.0, at.1, at.2, at.3, bar)
        };
        self.group(id, EDGE, EDGE, |x, y| Self::rect(x, y, EDGE, EDGE, fill));
        self.group(&format!("{id}-top"), EDGE, w, |x, y| side(x, y, EDGE, w, top, (x, y, EDGE, w)));
        self.group(&format!("{id}-bottom"), EDGE, w, |x, y| side(x, y, EDGE, w, bottom, (x, y, EDGE, w)));
        self.group(&format!("{id}-left"), w, EDGE, |x, y| side(x, y, w, EDGE, left, (x, y, w, EDGE)));
        self.group(&format!("{id}-right"), w, EDGE, |x, y| side(x, y, w, EDGE, right, (x, y, w, EDGE)));
        for (corner, vertical, horizontal) in [
            ("topleft", top, left),
            ("topright", top, right),
            ("bottomleft", bottom, left),
            ("bottomright", bottom, right),
        ] {
            let bar = if visible(vertical) { vertical } else { horizontal };
            self.group(&format!("{id}-{corner}"), w, w, |x, y| side(x, y, w, w, bar, (x, y, w, w)));
        }
    }

    fn tab(&mut self, id: &str, fill: Color, bar: Color) {
        let (r, b) = (R2, TAB_BAR);
        let base = |x: f32, y: f32, w: f32| Self::rect(x, y, w, b, fill) + &Self::rect(x, y, w, b, bar);
        self.group(id, EDGE, EDGE, |x, y| Self::rect(x, y, EDGE, EDGE, fill));
        self.group(&format!("{id}-top"), EDGE, r, |x, y| Self::rect(x, y, EDGE, r, fill));
        self.group(&format!("{id}-right"), r, EDGE, |x, y| Self::rect(x, y, r, EDGE, fill));
        self.group(&format!("{id}-bottom"), EDGE, b, |x, y| base(x, y, EDGE));
        self.group(&format!("{id}-bottomright"), r, b, |x, y| base(x, y, r));
        self.group(&format!("{id}-left"), r, EDGE, |x, y| Self::rect(x, y, r, EDGE, fill));
        self.group(&format!("{id}-bottomleft"), r, b, |x, y| base(x, y, r));
        self.group(&format!("{id}-topleft"), r, r, |x, y| {
            Self::path(&format!("M{},{} H{x} A{r},{r} 0 0 1 {},{y} Z", x + r, y + r, x + r), fill)
        });
        self.group(&format!("{id}-topright"), r, r, |x, y| {
            Self::path(&format!("M{x},{} V{y} A{r},{r} 0 0 1 {},{} Z", y + r, x + r, y + r), fill)
        });
    }

    fn finish(self) -> String {
        let height = self.y + self.tallest;
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{ROW}\" height=\"{height}\" viewBox=\"0 0 {ROW} {height}\">\n{}</svg>\n",
            self.body
        )
    }
}

struct Palette {
    t: Tokens,
}

impl Palette {
    fn on_win(&self, color: Color) -> Color {
        over(color, self.t.win_bg)
    }

    fn layer(&self, opacity: f32) -> Color {
        self.t.state.with_alpha(opacity)
    }

    fn faded(color: Color) -> Color {
        color.with_alpha(color.0[3] * 0.5)
    }

    fn button(&self) -> [Look; 5] {
        let t = &self.t;
        let rest = self.on_win(t.fill_1);
        [
            Look::flat(rest),
            Look::flat(over(self.layer(ST_HOVER), rest)),
            Look::flat(over(self.layer(ST_PRESS), rest)),
            Look::flat(self.on_win(t.acc_soft)),
            Look::flat(rest.with_alpha(DISABLED)),
        ]
    }

    fn tool(&self) -> [Look; 5] {
        let t = &self.t;
        [
            Look::flat(CLEAR),
            Look::flat(self.layer(ST_HOVER)),
            Look::flat(self.layer(ST_PRESS)),
            Look::flat(self.on_win(t.acc_soft)),
            Look::flat(CLEAR),
        ]
    }

    fn field(&self) -> [Look; 5] {
        let t = &self.t;
        let fill = t.view_bg;
        let border = over(t.line_2, t.view_bg);
        [
            Look::new(fill, border),
            Look::new(fill, t.acc_bg),
            Look::new(fill, t.acc_bg),
            Look::new(fill, t.acc_bg),
            Look::new(fill.with_alpha(DISABLED), border.with_alpha(DISABLED)),
        ]
    }

    fn card(&self, fill: Color) -> [Look; 5] {
        let look = Look::new(fill, over(self.t.line, fill));
        [look; 5]
    }

    fn row(&self, base: Color, rest: Color) -> [Look; 5] {
        let t = &self.t;
        [
            Look::flat(CLEAR),
            Look::flat(over(rest, base)),
            Look::flat(over(t.acc_soft, base)),
            Look::flat(over(t.acc_soft, base)),
            Look::flat(CLEAR),
        ]
    }

    fn menu_item(&self) -> [Look; 5] {
        let hover = Look::flat(over(self.layer(ST_HOVER), self.t.popover_bg));
        let press = Look::flat(over(self.layer(ST_PRESS), self.t.popover_bg));
        [Look::flat(CLEAR), hover, press, hover, Look::flat(CLEAR)]
    }

    fn tab(&self) -> [(Color, Color); 5] {
        let t = &self.t;
        [
            (CLEAR, CLEAR),
            (self.layer(ST_HOVER), CLEAR),
            (self.layer(ST_PRESS), CLEAR),
            (CLEAR, t.acc_bg),
            (CLEAR, CLEAR),
        ]
    }

    fn groove(&self) -> [Look; 5] {
        let t = &self.t;
        let groove = Look::flat(self.on_win(t.fill_2));
        [groove, groove, groove, Look::flat(t.acc_bg), Look::flat(Self::faded(self.on_win(t.fill_2)))]
    }

    fn accent(&self) -> [Look; 5] {
        let t = &self.t;
        let fill = Look::flat(t.acc_bg);
        [fill, Look::flat(t.acc_bg_hover), fill, fill, Look::flat(Self::faded(t.acc_bg))]
    }

    fn thumb(&self) -> [Look; 5] {
        let t = &self.t;
        [
            Look::flat(self.on_win(t.fg_4)),
            Look::flat(self.on_win(t.fg_3)),
            Look::flat(self.on_win(t.fg_2)),
            Look::flat(self.on_win(t.fg_2)),
            Look::flat(Self::faded(self.on_win(t.fg_4))),
        ]
    }

    fn header(&self) -> [Look; 5] {
        let t = &self.t;
        let fill = over(t.fill_1, t.view_bg);
        let look = Look::new(fill, over(t.line, fill));
        [look, Look::new(over(t.fill_2, t.view_bg), look.border), look, look, look]
    }

    fn glyph(&self, state: &str) -> Color {
        let t = &self.t;
        match state {
            "disabled" => self.on_win(t.fg_4),
            "focused" | "pressed" => t.fg_1,
            _ => self.on_win(t.fg_2),
        }
    }
}

const ARROWS: [(&str, &str); 4] = [
    ("down", "M2.5,3.75 L5,6.25 L7.5,3.75"),
    ("up", "M2.5,6.25 L5,3.75 L7.5,6.25"),
    ("left", "M6.25,2.5 L3.75,5 L6.25,7.5"),
    ("right", "M3.75,2.5 L6.25,5 L3.75,7.5"),
];

fn moved(d: &str, x: f32, y: f32) -> String {
    let mut out = String::new();
    for token in d.split(' ') {
        let (command, rest) = token.split_at(1);
        match rest.split_once(',') {
            Some((px, py)) => {
                let (px, py) = (px.parse::<f32>().unwrap_or(0.0), py.parse::<f32>().unwrap_or(0.0));
                let _ = write!(out, "{command}{},{} ", px + x, py + y);
            }
            None => out.push_str(token),
        }
    }
    out.trim_end().to_string()
}

fn indicators(svg: &mut Svg, p: &Palette) {
    let t = &p.t;
    for state in STATES {
        let ink = p.glyph(state);
        for (name, d) in ARROWS {
            svg.group(&format!("arrow-{name}-{state}"), 10.0, 10.0, |x, y| Svg::stroke(&moved(d, x, y), ink, 1.5));
        }
        let plus = "M2.5,5 L7.5,5 M5,2.5 L5,7.5";
        let minus = "M2.5,5 L7.5,5";
        for (name, d) in [("plus", plus), ("minus", minus), ("up", ARROWS[1].1), ("down", ARROWS[0].1)] {
            svg.group(&format!("spin-{name}-{state}"), 10.0, 10.0, |x, y| Svg::stroke(&moved(d, x, y), ink, 1.5));
        }
        for (name, d) in [("plus", ARROWS[3].1), ("minus", ARROWS[0].1)] {
            svg.group(&format!("tree-{name}-{state}"), 10.0, 10.0, |x, y| Svg::stroke(&moved(d, x, y), ink, 1.5));
        }

        let disabled = state == "disabled";
        let fade = |color: Color| if disabled { color.with_alpha(color.0[3] * DISABLED) } else { color };
        let box_fill = fade(t.view_bg);
        let box_border = fade(match state {
            "focused" | "pressed" => over(t.fg_3, t.view_bg),
            _ => over(t.line_2, t.view_bg),
        });
        let on = fade(if state == "focused" { t.acc_bg_hover } else { t.acc_bg });
        let mark = fade(t.acc_fg);
        let (size, half) = (CHECK, CHECK / 2.0);
        let inner = size - BOX_STROKE;
        let square = |x: f32, y: f32, fill: Color, border: Color| {
            format!(
                "<rect x=\"{}\" y=\"{}\" width=\"{inner}\" height=\"{inner}\" rx=\"{CHECK_RADIUS}\" {} stroke=\"{}\" stroke-opacity=\"{:.3}\" stroke-width=\"{BOX_STROKE}\"/>",
                x + BOX_STROKE / 2.0,
                y + BOX_STROKE / 2.0,
                paint(fill),
                hex(border),
                border.0[3]
            )
        };
        let tick = |x: f32, y: f32, d: &str| Svg::stroke(&moved(d, x, y), mark, 2.0);
        svg.group(&format!("checkbox-{state}"), size, size, |x, y| square(x, y, box_fill, box_border));
        svg.group(&format!("checkbox-checked-{state}"), size, size, |x, y| {
            square(x, y, on, on) + &tick(x, y, "M5,9.25 L7.75,12 L13,6.25")
        });
        svg.group(&format!("checkbox-tristate-{state}"), size, size, |x, y| {
            square(x, y, on, on) + &tick(x, y, "M5.5,9 L12.5,9")
        });
        let circle = |x: f32, y: f32, r: f32, fill: Color, border: Color| {
            format!(
                "<circle cx=\"{}\" cy=\"{}\" r=\"{r}\" {} stroke=\"{}\" stroke-opacity=\"{:.3}\" stroke-width=\"{BOX_STROKE}\"/>",
                x + half,
                y + half,
                paint(fill),
                hex(border),
                border.0[3]
            )
        };
        let radius = half - BOX_STROKE / 2.0;
        svg.group(&format!("radio-{state}"), size, size, |x, y| circle(x, y, radius, box_fill, box_border));
        svg.group(&format!("radio-checked-{state}"), size, size, |x, y| {
            circle(x, y, radius, on, on)
                + &format!("<circle cx=\"{}\" cy=\"{}\" r=\"3.5\" {}/>", x + half, y + half, paint(mark))
        });
        let thumb = fade(match state {
            "focused" | "pressed" => t.acc_bg_hover,
            _ => t.acc_bg,
        });
        let handle = HANDLE / 2.0;
        svg.group(&format!("slidercursor-{state}"), HANDLE, HANDLE, |x, y| {
            format!("<circle cx=\"{}\" cy=\"{}\" r=\"{handle}\" {}/>", x + handle, y + handle, paint(thumb))
        });
    }
}

const TAB_BAR: f32 = 3.0;
const CHECK: f32 = 18.0;
const CHECK_RADIUS: f32 = 5.0;
const BOX_STROKE: f32 = 1.5;
const HANDLE: f32 = 18.0;
const PROGRESS: f32 = 6.0;

#[must_use]
pub fn svg(tokens: &Tokens) -> String {
    let p = Palette { t: *tokens };
    let t = &p.t;
    let mut svg = Svg::new();
    svg.frames("button", p.button(), R2, 1.0);
    svg.frames("tbutton", p.tool(), R2, 1.0);
    svg.frames("lineedit", p.field(), R2, 1.0);
    svg.frames("menu", p.card(t.popover_bg), R3, 1.0);
    svg.frames("menuitem", p.menu_item(), R2, 1.0);
    svg.frames("itemview", p.row(t.view_bg, p.layer(ST_HOVER)), R1, 1.0);
    for (state, (fill, bar)) in STATES.into_iter().zip(p.tab()) {
        svg.tab(&format!("tab-{state}"), fill, bar);
        svg.edges(&format!("tabframe-{state}"), 1.0, CLEAR, [p.on_win(t.line), CLEAR, CLEAR, CLEAR]);
    }
    svg.frames("group", p.card(t.card_bg), R3, 1.0);
    svg.frames("common", [Look::new(CLEAR, p.on_win(t.line)); 5], R2, 1.0);
    svg.frames("tooltip", p.card(t.popover_bg), R2, 1.0);
    svg.frames("header", p.header(), 1.0, 1.0);
    svg.frames("slider", p.groove(), 3.0, 1.0);
    svg.frames("progress", p.groove(), 3.0, 1.0);
    svg.frames("progress-pattern", p.accent(), 3.0, 1.0);
    svg.frames("scrollbarslider", p.thumb(), R1, 1.0);
    let rule = p.on_win(t.line_2);
    for id in ["toolbar-handle", "toolbar-separator", "header-separator"] {
        svg.group(id, 8.0, 10.0, |x, y| Svg::rect(x + 3.5, y, 1.0, 10.0, rule));
    }
    indicators(&mut svg, &p);
    svg.finish()
}

fn section(out: &mut String, name: &str, keys: &[(&str, String)]) {
    let _ = writeln!(out, "[{name}]");
    for (key, value) in keys {
        let _ = writeln!(out, "{key}={value}");
    }
    out.push('\n');
}

fn framed(element: &str, width: f32, interior: bool) -> Vec<(&'static str, String)> {
    let width = format!("{}", width.round() as i32);
    let mut keys = vec![
        ("frame", "true".to_string()),
        ("frame.element", element.to_string()),
        ("frame.top", width.clone()),
        ("frame.bottom", width.clone()),
        ("frame.left", width.clone()),
        ("frame.right", width),
        ("interior", interior.to_string()),
    ];
    if interior {
        keys.push(("interior.element", element.to_string()));
    }
    keys
}

fn bare() -> Vec<(&'static str, String)> {
    vec![("frame", "false".to_string()), ("interior", "false".to_string())]
}

fn text(normal: Color, toggled: Color) -> Vec<(&'static str, String)> {
    vec![
        ("text.normal.color", hex(normal)),
        ("text.focus.color", hex(normal)),
        ("text.press.color", hex(normal)),
        ("text.toggle.color", hex(toggled)),
    ]
}

fn with(mut keys: Vec<(&'static str, String)>, extra: &[(&'static str, &str)]) -> Vec<(&'static str, String)> {
    keys.extend(extra.iter().map(|(key, value)| (*key, (*value).to_string())));
    keys
}

#[must_use]
pub fn config(tokens: &Tokens) -> String {
    let t = tokens;
    let p = Palette { t: *tokens };
    let fg = t.fg_1;
    let mut out = String::new();
    section(
        &mut out,
        "%General",
        &[
            ("author", "Glide".into()),
            ("comment", "Generated by Glide from its design tokens".into()),
            ("x11drag", "menubar_and_primary_toolbar".into()),
            ("alt_mnemonic", "true".into()),
            ("composite", "true".into()),
            ("translucent_windows", "false".into()),
            ("blurring", "false".into()),
            ("popup_blurring", "false".into()),
            ("menu_shadow_depth", "0".into()),
            ("tooltip_shadow_depth", "0".into()),
            ("submenu_overlap", "0".into()),
            ("animate_states", "true".into()),
            ("respect_DE", "true".into()),
            ("scroll_arrows", "false".into()),
            ("scrollbar_in_view", "true".into()),
            ("transient_scrollbar", "true".into()),
            ("scroll_width", "10".into()),
            ("scroll_min_extent", "40".into()),
            ("slider_width", PROGRESS.to_string()),
            ("slider_handle_width", HANDLE.to_string()),
            ("slider_handle_length", HANDLE.to_string()),
            ("check_size", CHECK.to_string()),
            ("progressbar_thickness", PROGRESS.to_string()),
            ("spread_progressbar", "true".into()),
            ("attach_active_tab", "false".into()),
            ("joined_inactive_tabs", "false".into()),
            ("tooltip_delay", "550".into()),
            ("small_icon_size", "16".into()),
            ("large_icon_size", "32".into()),
            ("button_icon_size", "16".into()),
            ("toolbar_icon_size", "16".into()),
            ("layout_spacing", "6".into()),
            ("layout_margin", "8".into()),
            ("button_contents_shift", "false".into()),
            ("fill_rubberband", "true".into()),
            ("groupbox_top_label", "true".into()),
            ("tree_branch_line", "false".into()),
            ("menubar_mouse_tracking", "true".into()),
            ("combo_as_lineedit", "false".into()),
            ("combo_menu", "true".into()),
            ("hide_combo_checkboxes", "true".into()),
            ("inline_spin_indicators", "true".into()),
            ("spin_button_width", "24".into()),
            ("no_inactiveness", "false".into()),
            ("dark_titlebar", (t.appearance == crate::Appearance::Dark).to_string()),
        ],
    );
    section(
        &mut out,
        "GeneralColors",
        &[
            ("window.color", hex(t.win_bg)),
            ("base.color", hex(t.view_bg)),
            ("alt.base.color", hex(over(t.fill_1, t.view_bg))),
            ("button.color", hex(p.on_win(t.fill_1))),
            ("light.color", hex(p.on_win(t.fill_2))),
            ("mid.light.color", hex(p.on_win(t.line_2))),
            ("dark.color", hex(p.on_win(t.line_2))),
            ("mid.color", hex(p.on_win(t.line))),
            ("highlight.color", hex(t.acc_bg)),
            ("inactive.highlight.color", hex(over(t.acc_soft, t.view_bg))),
            ("text.color", hex(fg)),
            ("window.text.color", hex(fg)),
            ("button.text.color", hex(fg)),
            ("disabled.text.color", hex(p.on_win(t.fg_3))),
            ("tooltip.text.color", hex(fg)),
            ("highlight.text.color", hex(t.acc_fg)),
            ("link.color", hex(t.acc_text)),
            ("link.visited.color", hex(t.acc_text)),
            ("progress.indicator.text.color", hex(fg)),
            ("progress.inactive.indicator.text.color", hex(p.on_win(t.fg_3))),
            ("shadow.color", "#000000".into()),
        ],
    );
    let disabled = (crate::export::DISABLED_OPACITY * 100.0).round().to_string();
    section(
        &mut out,
        "Hacks",
        &[
            ("respect_darkness", "true".into()),
            ("transparent_dolphin_view", "false".into()),
            ("transparent_menutitle", "true".into()),
            ("kcapacitybar_as_progressbar", "true".into()),
            ("normal_default_pushbutton", "true".into()),
            ("no_selection_tint", "true".into()),
            ("tint_on_mouseover", "0".into()),
            ("centered_forms", "false".into()),
            ("disabled_icon_opacity", disabled),
        ],
    );
    widget_sections(&mut out, &p);
    out
}

fn widget_sections(out: &mut String, p: &Palette) {
    let t = &p.t;
    let fg = t.fg_1;
    let soft = t.acc_soft_fg;
    let button = || {
        let mut keys = framed("button", R2, true);
        keys.extend(text(fg, soft));
        keys.push(("indicator.element", "arrow".into()));
        keys.push(("indicator.size", "10".into()));
        keys.push(("text.margin.left", "10".into()));
        keys.push(("text.margin.right", "10".into()));
        keys.push(("text.margin.top", "4".into()));
        keys.push(("text.margin.bottom", "4".into()));
        keys
    };
    section(out, "PanelButtonCommand", &button());
    section(out, "ComboBox", &button());
    section(out, "ToolboxTab", &button());
    let tool = || {
        let mut keys = framed("tbutton", R2, true);
        keys.extend(text(fg, soft));
        keys.push(("indicator.element", "arrow".into()));
        keys.push(("indicator.size", "10".into()));
        keys
    };
    section(out, "PanelButtonTool", &tool());
    section(out, "ToolbarButton", &tool());
    section(out, "DropDownButton", &with(bare(), &[("indicator.element", "arrow"), ("indicator.size", "10")]));
    section(out, "IndicatorArrow", &with(bare(), &[("indicator.element", "arrow"), ("indicator.size", "10")]));
    section(out, "IndicatorSpinBox", &with(bare(), &[("indicator.element", "spin"), ("indicator.size", "10")]));
    section(out, "TreeExpander", &with(bare(), &[("indicator.element", "tree"), ("indicator.size", "10")]));
    for (name, element) in [("CheckBox", "checkbox"), ("RadioButton", "radio")] {
        let mut keys =
            with(vec![("frame", "false".to_string())], &[("interior", "true"), ("interior.element", element)]);
        keys.extend(text(fg, fg));
        section(out, name, &keys);
    }
    let mut field = framed("lineedit", R2, true);
    field.extend(text(fg, fg));
    section(out, "LineEdit", &field);
    let mut menu = framed("menu", R3, true);
    menu.extend(text(fg, fg));
    section(out, "Menu", &menu);
    let mut item = framed("menuitem", R2, true);
    item.extend(text(fg, fg));
    item.push(("indicator.element", "arrow".into()));
    item.push(("text.margin.left", "8".into()));
    item.push(("text.margin.right", "8".into()));
    item.push(("text.margin.top", "6".into()));
    item.push(("text.margin.bottom", "6".into()));
    section(out, "MenuItem", &item);
    section(out, "MenuBarItem", &item);
    section(out, "MenuBar", &bare());
    let mut rows = framed("itemview", R1, true);
    rows.extend(text(fg, soft));
    section(out, "ItemView", &rows);
    let mut tab = framed("tab", R2, true);
    tab.retain(|(key, _)| *key != "frame.bottom");
    tab.extend([
        ("frame.bottom", TAB_BAR.to_string()),
        ("text.normal.color", hex(p.on_win(t.fg_2))),
        ("text.focus.color", hex(fg)),
        ("text.press.color", hex(fg)),
        ("text.toggle.color", hex(t.acc_text)),
        ("text.margin.left", "14".into()),
        ("text.margin.right", "14".into()),
        ("text.margin.top", "6".into()),
        ("text.margin.bottom", "6".into()),
    ]);
    section(out, "Tab", &tab);
    section(out, "TabFrame", &framed("tabframe", 1.0, false));
    section(out, "TabBarFrame", &bare());
    section(out, "Dock", &bare());
    let mut dock_title = bare();
    dock_title.extend(text(p.on_win(t.fg_2), fg));
    section(out, "DockTitle", &dock_title);
    let mut group = framed("group", R3, true);
    group.extend(text(fg, fg));
    section(out, "GroupBox", &group);
    section(out, "GenericFrame", &framed("common", R2, false));
    let mut tooltip = framed("tooltip", R2, true);
    tooltip.extend(text(fg, fg));
    section(out, "ToolTip", &tooltip);
    section(out, "Focus", &bare());
    let mut header = framed("header", 1.0, true);
    header.extend(text(p.on_win(t.fg_2), fg));
    section(out, "HeaderSection", &header);
    section(out, "Slider", &framed("slider", 3.0, true));
    section(out, "SliderCursor", &with(bare(), &[("interior", "true"), ("interior.element", "slidercursor")]));
    section(out, "Progressbar", &framed("progress", 3.0, true));
    section(out, "ProgressbarContents", &framed("progress-pattern", 3.0, true));
    section(out, "ScrollbarSlider", &framed("scrollbarslider", R1, true));
    section(out, "ScrollbarGroove", &bare());
    section(out, "Scrollbar", &with(bare(), &[("indicator.element", "arrow"), ("indicator.size", "10")]));
    section(out, "Toolbar", &with(bare(), &[("indicator.element", "toolbar"), ("indicator.size", "5")]));
    for name in ["Window", "WindowTranslucent", "StatusBar", "Splitter", "SizeGrip", "Dial", "TitleBar"] {
        section(out, name, &bare());
    }
}

#[must_use]
pub fn manager_config(current: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut in_general = false;
    let mut written = false;
    for line in current.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            if in_general && !written {
                out.push(format!("theme={THEME}"));
                written = true;
            }
            in_general = trimmed == "[General]";
        } else if in_general && trimmed.starts_with("theme=") {
            if !written {
                out.push(format!("theme={THEME}"));
                written = true;
            }
            continue;
        }
        out.push(line.to_string());
    }
    if !written {
        if !current.contains("[General]") {
            out.push("[General]".into());
        }
        out.push(format!("theme={THEME}"));
    }
    out.join("\n") + "\n"
}

#[must_use]
pub fn replaces_theme(current: &str) -> bool {
    let theme =
        current.lines().map(str::trim).find_map(|line| line.strip_prefix("theme=")).map(str::trim).unwrap_or_default();
    matches!(theme, "" | "Default" | "KvFlat" | "KvFlatLight" | "KvDefault" | THEME)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Accent, Appearance};

    fn elements(config: &str) -> Vec<(String, bool)> {
        let mut found = Vec::new();
        let mut frame = false;
        for line in config.lines() {
            if line.starts_with('[') {
                frame = false;
            }
            if line == "frame=true" {
                frame = true;
            }
            for key in ["frame.element=", "interior.element="] {
                if let Some(name) = line.strip_prefix(key) {
                    found.push((name.to_string(), key == "frame.element=" && frame));
                }
            }
        }
        found
    }

    #[test]
    fn every_element_the_config_names_is_drawn_in_every_state() {
        for appearance in [Appearance::Light, Appearance::Dark] {
            let tokens = Tokens::new(appearance, Accent::default());
            let (config, svg) = (config(&tokens), svg(&tokens));
            for (element, frame) in elements(&config) {
                for id in STATES.iter().map(|state| format!("{element}-{state}")) {
                    assert!(svg.contains(&format!("id=\"{id}\"")), "{id}");
                    if frame {
                        for side in
                            ["top", "bottom", "left", "right", "topleft", "topright", "bottomleft", "bottomright"]
                        {
                            assert!(svg.contains(&format!("id=\"{id}-{side}\"")), "{id}-{side}");
                        }
                    }
                }
            }
            assert!(
                section_of(&config, "Focus").contains("frame=false"),
                "Kvantum draws the focus frame on mouse focus too, which the spec reserves for the keyboard"
            );
            for indicator in ["arrow-down", "spin-plus", "tree-minus", "checkbox-checked", "radio", "slidercursor"] {
                assert!(svg.contains(&format!("id=\"{indicator}-normal\"")), "{indicator}");
            }
            assert!(!svg.contains("NaN"), "no broken coordinates");
        }
    }

    #[test]
    fn the_manager_points_at_glide_and_keeps_other_keys() {
        assert_eq!(manager_config(""), "[General]\ntheme=Glide\n");
        assert_eq!(manager_config("[General]\ntheme=KvFlat\n\n[Apps]\nx=1"), "[General]\ntheme=Glide\n\n[Apps]\nx=1\n");
        assert!(replaces_theme("[General]\ntheme=KvFlat\n"));
        assert!(replaces_theme(""));
        assert!(!replaces_theme("[General]\ntheme=KvArc\n"));
    }

    fn section_of<'a>(config: &'a str, name: &str) -> &'a str {
        let start = config.find(&format!("[{name}]\n")).expect("section");
        let body = &config[start..];
        body.find("\n\n").map_or(body, |end| &body[..end])
    }

    fn group<'a>(svg: &'a str, id: &str) -> &'a str {
        let start = svg.find(&format!("<g id=\"{id}\">")).expect("group");
        let body = &svg[start..];
        &body[..body.find("</g>").expect("closed")]
    }

    #[test]
    fn check_boxes_and_radios_are_interiors_because_kvantum_never_draws_them_as_indicators() {
        let config = config(&Tokens::new(Appearance::Dark, Accent::Blue));
        for (name, element) in [("CheckBox", "checkbox"), ("RadioButton", "radio")] {
            let section = section_of(&config, name);
            assert!(section.contains("interior=true") && section.contains(&format!("interior.element={element}")));
            assert!(!section.contains("indicator.element"), "{name}");
        }
    }

    #[test]
    fn buttons_are_borderless_fills_and_tabs_mark_the_current_one_with_an_accent_bar() {
        let tokens = Tokens::new(Appearance::Light, Accent::Blue);
        let svg = svg(&tokens);
        for state in STATES {
            assert!(!group(&svg, &format!("button-{state}-top")).contains("height=\"1\""), "{state} has no border");
        }
        assert!(group(&svg, "button-toggled").contains(&hex(over(tokens.acc_soft, tokens.win_bg))));
        assert!(group(&svg, "tab-toggled-bottom").contains(&hex(tokens.acc_bg)));
        assert!(!group(&svg, "tab-normal-bottom").contains(&hex(tokens.acc_bg)));
        assert!(group(&svg, "tabframe-normal-top").contains(&hex(over(tokens.line, tokens.win_bg))));
        assert!(section_of(&config(&tokens), "Tab").contains(&format!("text.toggle.color={}", hex(tokens.acc_text))));
    }

    #[test]
    fn moved_paths_shift_every_point() {
        assert_eq!(moved("M1,2 L3,4", 10.0, 20.0), "M11,22 L13,24");
    }
}

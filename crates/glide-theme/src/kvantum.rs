use std::fmt::Write;

use crate::export::over;
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

    fn frame(&mut self, name: &str, state: &str, look: Look, r: f32, b: f32) {
        let (fill, border) = (look.fill, look.border);
        let i = r - b / 2.0;
        self.group(&format!("{name}-{state}"), EDGE, EDGE, |x, y| Self::rect(x, y, EDGE, EDGE, fill));
        self.group(&format!("{name}-{state}-top"), EDGE, r, |x, y| {
            Self::rect(x, y, EDGE, r, fill) + &Self::rect(x, y, EDGE, b, border)
        });
        self.group(&format!("{name}-{state}-bottom"), EDGE, r, |x, y| {
            Self::rect(x, y, EDGE, r, fill) + &Self::rect(x, y + r - b, EDGE, b, border)
        });
        self.group(&format!("{name}-{state}-left"), r, EDGE, |x, y| {
            Self::rect(x, y, r, EDGE, fill) + &Self::rect(x, y, b, EDGE, border)
        });
        self.group(&format!("{name}-{state}-right"), r, EDGE, |x, y| {
            Self::rect(x, y, r, EDGE, fill) + &Self::rect(x + r - b, y, b, EDGE, border)
        });
        let h = b / 2.0;
        self.group(&format!("{name}-{state}-topleft"), r, r, |x, y| {
            Self::path(&format!("M{},{} H{x} A{r},{r} 0 0 1 {},{y} Z", x + r, y + r, x + r), fill)
                + &Self::stroke(&format!("M{},{} A{i},{i} 0 0 1 {},{}", x + h, y + r, x + r, y + h), border, b)
        });
        self.group(&format!("{name}-{state}-topright"), r, r, |x, y| {
            Self::path(&format!("M{x},{} V{y} A{r},{r} 0 0 1 {},{} Z", y + r, x + r, y + r), fill)
                + &Self::stroke(&format!("M{x},{} A{i},{i} 0 0 1 {},{}", y + h, x + r - h, y + r), border, b)
        });
        self.group(&format!("{name}-{state}-bottomright"), r, r, |x, y| {
            Self::path(&format!("M{x},{y} H{} A{r},{r} 0 0 1 {x},{} Z", x + r, y + r), fill)
                + &Self::stroke(&format!("M{},{y} A{i},{i} 0 0 1 {x},{}", x + r - h, y + r - h), border, b)
        });
        self.group(&format!("{name}-{state}-bottomleft"), r, r, |x, y| {
            Self::path(&format!("M{},{y} V{} A{r},{r} 0 0 1 {x},{y} Z", x + r, y + r), fill)
                + &Self::stroke(&format!("M{},{} A{i},{i} 0 0 1 {},{y}", x + r, y + r - h, x + h), border, b)
        });
    }

    fn frames(&mut self, name: &str, looks: [Look; 5], r: f32, b: f32) {
        for (state, look) in STATES.into_iter().zip(looks) {
            self.frame(name, state, look, r, b);
        }
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
        let border = self.on_win(t.line_2);
        [
            Look::new(self.on_win(t.fill_1), border),
            Look::new(self.on_win(t.fill_2), border),
            Look::new(self.on_win(t.fill_3), border),
            Look::new(self.on_win(t.acc_soft), t.acc_bg),
            Look::new(Self::faded(self.on_win(t.fill_1)), Self::faded(border)),
        ]
    }

    fn tool(&self) -> [Look; 5] {
        let t = &self.t;
        [
            Look::flat(CLEAR),
            Look::flat(self.layer(0.08)),
            Look::flat(self.layer(0.13)),
            Look::flat(self.on_win(t.acc_soft)),
            Look::flat(CLEAR),
        ]
    }

    fn field(&self) -> [Look; 5] {
        let t = &self.t;
        let fill = over(t.fill_1, t.view_bg);
        let border = over(t.line_2, t.view_bg);
        [
            Look::new(fill, border),
            Look::new(fill, over(t.fg_4, t.view_bg)),
            Look::new(fill, t.acc_bg),
            Look::new(fill, t.acc_bg),
            Look::new(Self::faded(fill), Self::faded(border)),
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
        let hover = Look::flat(over(self.t.fill_2, self.t.popover_bg));
        [Look::flat(CLEAR), hover, hover, hover, Look::flat(CLEAR)]
    }

    fn tab(&self) -> [Look; 5] {
        let t = &self.t;
        [
            Look::flat(CLEAR),
            Look::flat(self.layer(0.08)),
            Look::flat(self.layer(0.13)),
            Look::new(t.card_bg, over(t.line, t.card_bg)),
            Look::flat(CLEAR),
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
        let box_fill = match state {
            "focused" | "pressed" => p.on_win(t.fill_2),
            _ => p.on_win(t.fill_1),
        };
        let box_border = p.on_win(t.line_2);
        let on = if disabled { Palette::faded(t.acc_bg) } else { t.acc_bg };
        let mark = t.acc_fg;
        let square = |x: f32, y: f32, fill: Color, border: Color| {
            format!(
                "<rect x=\"{}\" y=\"{}\" width=\"15\" height=\"15\" rx=\"{R1}\" {} stroke=\"{}\" stroke-opacity=\"{:.3}\"/>",
                x + 0.5,
                y + 0.5,
                paint(fill),
                hex(border),
                border.0[3]
            )
        };
        svg.group(&format!("checkbox-{state}"), 16.0, 16.0, |x, y| square(x, y, box_fill, box_border));
        svg.group(&format!("checkbox-checked-{state}"), 16.0, 16.0, |x, y| {
            square(x, y, on, on) + &Svg::stroke(&moved("M4.5,8.25 L7,10.75 L11.5,5.5", x, y), mark, 2.0)
        });
        svg.group(&format!("checkbox-tristate-{state}"), 16.0, 16.0, |x, y| {
            square(x, y, on, on) + &Svg::stroke(&moved("M4.5,8 L11.5,8", x, y), mark, 2.0)
        });
        let circle = |x: f32, y: f32, fill: Color, border: Color| {
            format!(
                "<circle cx=\"{}\" cy=\"{}\" r=\"7.5\" {} stroke=\"{}\" stroke-opacity=\"{:.3}\"/>",
                x + 8.0,
                y + 8.0,
                paint(fill),
                hex(border),
                border.0[3]
            )
        };
        svg.group(&format!("radio-{state}"), 16.0, 16.0, |x, y| circle(x, y, box_fill, box_border));
        svg.group(&format!("radio-checked-{state}"), 16.0, 16.0, |x, y| {
            circle(x, y, on, on) + &format!("<circle cx=\"{}\" cy=\"{}\" r=\"3\" {}/>", x + 8.0, y + 8.0, paint(mark))
        });
        let thumb = if disabled { Palette::faded(t.card_bg) } else { t.card_bg };
        let ring = if state == "focused" || state == "pressed" { t.acc_bg } else { p.on_win(t.line_2) };
        svg.group(&format!("slidercursor-{state}"), 16.0, 16.0, |x, y| circle(x, y, thumb, ring));
    }
}

#[must_use]
pub fn svg(tokens: &Tokens) -> String {
    let p = Palette { t: *tokens };
    let t = &p.t;
    let mut svg = Svg::new();
    svg.frames("button", p.button(), R2, 1.0);
    svg.frames("tbutton", p.tool(), R2, 1.0);
    svg.frames("lineedit", p.field(), R2, 1.0);
    svg.frames("menu", p.card(t.popover_bg), R2, 1.0);
    svg.frames("menuitem", p.menu_item(), R2, 1.0);
    svg.frames("itemview", p.row(t.view_bg, p.layer(0.08)), R1, 1.0);
    svg.frames("tab", p.tab(), R2, 1.0);
    svg.frames("tabframe", p.card(t.card_bg), R3, 1.0);
    svg.frames("group", p.card(t.card_bg), R3, 1.0);
    svg.frames("common", [Look::new(CLEAR, p.on_win(t.line)); 5], R2, 1.0);
    svg.frames("tooltip", p.card(t.popover_bg), R2, 1.0);
    svg.frames("focus", [Look::new(CLEAR, t.acc_ring); 5], R2, 2.0);
    svg.frames("header", p.header(), 1.0, 1.0);
    svg.frames("slider", p.groove(), 3.0, 1.0);
    svg.frames("progress", p.groove(), 3.0, 1.0);
    svg.frames("progress-pattern", p.accent(), 3.0, 1.0);
    svg.frames("scrollbarslider", p.thumb(), R1, 1.0);
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
            ("slider_width", "6".into()),
            ("slider_handle_width", "16".into()),
            ("slider_handle_length", "16".into()),
            ("check_size", "16".into()),
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
    section(out, "CheckBox", &with(bare(), &[("indicator.element", "checkbox")]));
    section(out, "RadioButton", &with(bare(), &[("indicator.element", "radio")]));
    let mut field = framed("lineedit", R2, true);
    field.extend(text(fg, fg));
    section(out, "LineEdit", &field);
    let mut menu = framed("menu", R2, true);
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
    tab.extend(text(p.on_win(t.fg_2), fg));
    section(out, "Tab", &tab);
    section(out, "TabFrame", &framed("tabframe", R3, true));
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
    section(out, "Focus", &framed("focus", 2.0, false));
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
    for name in ["Toolbar", "Window", "WindowTranslucent", "StatusBar", "Splitter", "SizeGrip", "Dial", "TitleBar"] {
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
                for state in STATES {
                    assert!(svg.contains(&format!("id=\"{element}-{state}\"")), "{element}-{state}");
                    if frame {
                        for side in
                            ["top", "bottom", "left", "right", "topleft", "topright", "bottomleft", "bottomright"]
                        {
                            assert!(
                                svg.contains(&format!("id=\"{element}-{state}-{side}\"")),
                                "{element}-{state}-{side}"
                            );
                        }
                    }
                }
            }
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

    #[test]
    fn moved_paths_shift_every_point() {
        assert_eq!(moved("M1,2 L3,4", 10.0, 20.0), "M11,22 L13,24");
    }
}

use std::fmt::Write as _;

use crate::elevation::{self, Level};
use crate::{Accent, Appearance, Color, Tokens, shape};

fn channel(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

#[must_use]
pub fn css_color(color: Color) -> String {
    let [r, g, b, a] = color.0;
    if a >= 0.999 {
        format!("#{:02x}{:02x}{:02x}", channel(r), channel(g), channel(b))
    } else {
        format!("rgba({}, {}, {}, {:.3})", channel(r), channel(g), channel(b), a.clamp(0.0, 1.0))
    }
}

#[must_use]
pub fn over(top: Color, bottom: Color) -> Color {
    let alpha = top.0[3].clamp(0.0, 1.0);
    let mix = |index: usize| top.0[index] * alpha + bottom.0[index] * (1.0 - alpha);
    Color([mix(0), mix(1), mix(2), 1.0])
}

fn hex_argb(color: Color) -> String {
    let [r, g, b, a] = color.0;
    format!("#{:02x}{:02x}{:02x}{:02x}", channel(a), channel(r), channel(g), channel(b))
}

fn rgb_triplet(color: Color) -> String {
    let [r, g, b, _] = color.0;
    format!("{},{},{}", channel(r), channel(g), channel(b))
}

fn on_status(tokens: &Tokens) -> Color {
    match tokens.appearance {
        Appearance::Light => Color([1.0, 1.0, 1.0, 1.0]),
        Appearance::Dark => tokens.sunken_bg,
    }
}

#[must_use]
pub fn adwaita_colors(tokens: &Tokens) -> Vec<(&'static str, Color)> {
    let t = tokens;
    let on = on_status(t);
    vec![
        ("accent_bg_color", t.acc_bg),
        ("accent_fg_color", t.acc_fg),
        ("accent_color", t.acc_text),
        ("destructive_bg_color", t.danger),
        ("destructive_fg_color", on),
        ("destructive_color", t.danger),
        ("success_bg_color", t.ok),
        ("success_fg_color", on),
        ("success_color", t.ok),
        ("warning_bg_color", t.warn),
        ("warning_fg_color", on),
        ("warning_color", t.warn),
        ("error_bg_color", t.danger),
        ("error_fg_color", on),
        ("error_color", t.danger),
        ("window_bg_color", t.win_bg),
        ("window_fg_color", t.fg_1),
        ("view_bg_color", t.view_bg),
        ("view_fg_color", t.fg_1),
        ("headerbar_bg_color", t.bar_bg),
        ("headerbar_fg_color", t.fg_1),
        ("headerbar_border_color", t.fg_1),
        ("headerbar_backdrop_color", t.bar_bg_idle),
        ("headerbar_shade_color", t.line),
        ("headerbar_darker_shade_color", t.line_2),
        ("sidebar_bg_color", t.sidebar_bg),
        ("sidebar_fg_color", t.fg_1),
        ("sidebar_backdrop_color", t.sidebar_bg),
        ("sidebar_border_color", t.line),
        ("sidebar_shade_color", t.line),
        ("secondary_sidebar_bg_color", t.sidebar_bg),
        ("secondary_sidebar_fg_color", t.fg_1),
        ("secondary_sidebar_backdrop_color", t.sidebar_bg),
        ("secondary_sidebar_border_color", t.line),
        ("secondary_sidebar_shade_color", t.line),
        ("card_bg_color", t.card_bg),
        ("card_fg_color", t.fg_1),
        ("card_shade_color", t.line),
        ("dialog_bg_color", t.popover_bg),
        ("dialog_fg_color", t.fg_1),
        ("popover_bg_color", t.popover_bg),
        ("popover_fg_color", t.fg_1),
        ("popover_shade_color", t.line),
        ("thumbnail_bg_color", t.card_bg),
        ("thumbnail_fg_color", t.fg_1),
        ("shade_color", t.line),
        ("scrollbar_outline_color", t.line),
    ]
}

fn define_colors(out: &mut String, colors: &[(&str, Color)]) {
    for (name, color) in colors {
        let _ = writeln!(out, "@define-color {name} {};", css_color(*color));
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Gtk {
    Three,
    Four,
}

impl Gtk {
    const fn focus(self) -> &'static str {
        match self {
            Self::Three => ":focus",
            Self::Four => ":focus-within",
        }
    }
}

fn shadow(level: Level, appearance: Appearance) -> String {
    elevation::layers(level, appearance)
        .iter()
        .map(|layer| {
            format!("{}px {}px {}px {}px {}", layer.dx, layer.dy, layer.blur, layer.spread, css_color(layer.color))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

struct Paint {
    fill_1: String,
    fill_2: String,
    fill_3: String,
    line: String,
    line_2: String,
    view: String,
    popover: String,
    thumb: String,
    switch_thumb: String,
    fg_1: String,
    fg_2: String,
    fg_3: String,
    acc: String,
    acc_text: String,
    soft: String,
    soft_fg: String,
    ring: String,
    danger: String,
    danger_soft: String,
    warn: String,
    warn_soft: String,
    ok: String,
    ok_soft: String,
    el_1: String,
    el_2: String,
    el_3: String,
}

impl Paint {
    fn build(t: &Tokens, mut pick: impl FnMut(&'static str, String) -> String) -> Self {
        let a = t.appearance;
        Self {
            fill_1: pick("fill-1", css_color(t.fill_1)),
            fill_2: pick("fill-2", css_color(t.fill_2)),
            fill_3: pick("fill-3", css_color(t.fill_3)),
            line: pick("line", css_color(t.line)),
            line_2: pick("line-2", css_color(t.line_2)),
            view: pick("view-bg", css_color(t.view_bg)),
            popover: pick("popover-bg", css_color(t.popover_bg)),
            thumb: pick("segment-thumb", css_color(t.segment_thumb())),
            switch_thumb: pick("switch-thumb", css_color(t.switch_thumb())),
            fg_1: pick("fg-1", css_color(t.fg_1)),
            fg_2: pick("fg-2", css_color(t.fg_2)),
            fg_3: pick("fg-3", css_color(t.fg_3)),
            acc: pick("acc-bg", css_color(t.acc_bg)),
            acc_text: pick("acc-text", css_color(t.acc_text)),
            soft: pick("acc-soft", css_color(t.acc_soft)),
            soft_fg: pick("acc-soft-fg", css_color(t.acc_soft_fg)),
            ring: pick("acc-ring", css_color(t.acc_ring)),
            danger: pick("danger", css_color(t.danger)),
            danger_soft: pick("danger-soft", css_color(t.danger_soft)),
            warn: pick("warn", css_color(t.warn)),
            warn_soft: pick("warn-soft", css_color(t.warn_soft)),
            ok: pick("ok", css_color(t.ok)),
            ok_soft: pick("ok-soft", css_color(t.ok_soft)),
            el_1: pick("el-1", shadow(Level::El1, a)),
            el_2: pick("el-2", shadow(Level::El2, a)),
            el_3: pick("el-3", shadow(Level::El3, a)),
        }
    }

    fn literal(t: &Tokens) -> Self {
        Self::build(t, |_, value| value)
    }

    fn variables(t: &Tokens) -> Self {
        Self::build(t, |name, _| format!("var(--glide-{name})"))
    }

    fn declare(out: &mut String, t: &Tokens) {
        Self::build(t, |name, value| {
            let _ = writeln!(out, "  --glide-{name}: {value};");
            value
        });
    }
}

const HOVER: &str = "alpha(currentColor, 0.08)";
const PRESS: &str = "alpha(currentColor, 0.13)";

fn window_rules(out: &mut String) {
    let r3 = shape::R3;
    let _ = write!(
        out,
        "
window.csd:not(.maximized):not(.tiled):not(.tiled-left):not(.tiled-right):not(.fullscreen),
window.csd:not(.maximized):not(.tiled):not(.fullscreen) > .titlebar,
window.csd:not(.maximized):not(.tiled):not(.fullscreen) headerbar.titlebar {{ border-top-left-radius: {r3}px; border-top-right-radius: {r3}px; }}
window.csd:not(.maximized):not(.tiled):not(.fullscreen) {{ border-radius: {r3}px; }}
"
    );
}

fn toggle_rules(out: &mut String, p: &Paint) {
    let Paint { fill_1, thumb, fg_1, fg_2, soft, soft_fg, el_1, .. } = p;
    let r2 = shape::R2;
    let _ = write!(
        out,
        "
button.toggle:checked {{ background-color: {soft}; color: {soft_fg}; background-image: none; }}
button.toggle:checked:hover {{ background-image: image({HOVER}); }}
button.toggle:checked:active {{ background-image: image({PRESS}); }}
stackswitcher {{ background-color: {fill_1}; border-radius: {r2}px; padding: 3px; }}
stackswitcher > button.toggle {{ min-height: 18px; border-radius: 6px; background-color: transparent; background-image: none; box-shadow: none; color: {fg_2}; }}
stackswitcher > button.toggle:hover {{ color: {fg_1}; }}
stackswitcher > button.toggle:checked {{ background-color: {thumb}; color: {fg_1}; box-shadow: {el_1}; }}
toggle-group {{ background-color: {fill_1}; }}
toggle-group > toggle:checked {{ box-shadow: {el_1}; }}
"
    );
}

fn entry_rules(out: &mut String, p: &Paint, gtk: Gtk) {
    let Paint {
        fill_1,
        fill_2,
        fill_3,
        line_2,
        view,
        acc,
        ring,
        danger,
        danger_soft,
        warn,
        warn_soft,
        ok,
        ok_soft,
        ..
    } = p;
    let focus = gtk.focus();
    let _ = write!(
        out,
        "
entry, spinbutton {{ background-color: {view}; background-image: none; box-shadow: inset 0 0 0 1px {line_2}; }}
entry:hover, spinbutton:hover {{ box-shadow: inset 0 0 0 1px {fill_3}; }}
entry{focus}, spinbutton{focus} {{ box-shadow: inset 0 0 0 1px {acc}; outline: 3px solid {ring}; outline-offset: 0; }}
entry.error, spinbutton.error, entry.error:hover {{ box-shadow: inset 0 0 0 1px {danger}; }}
entry.error{focus}, spinbutton.error{focus} {{ box-shadow: inset 0 0 0 1px {danger}; outline-color: {danger_soft}; }}
entry.warning, spinbutton.warning, entry.warning:hover {{ box-shadow: inset 0 0 0 1px {warn}; }}
entry.warning{focus}, spinbutton.warning{focus} {{ box-shadow: inset 0 0 0 1px {warn}; outline-color: {warn_soft}; }}
entry.success, spinbutton.success, entry.success:hover {{ box-shadow: inset 0 0 0 1px {ok}; }}
entry.success{focus}, spinbutton.success{focus} {{ box-shadow: inset 0 0 0 1px {ok}; outline-color: {ok_soft}; }}
entry.search, searchbar entry {{ background-color: {fill_1}; box-shadow: none; }}
entry.search:hover, searchbar entry:hover {{ background-color: {fill_2}; box-shadow: none; }}
entry.search{focus}, searchbar entry{focus} {{ background-color: {view}; box-shadow: inset 0 0 0 1px {acc}; }}
entry.flat, spinbutton.flat, entry.flat:hover, spinbutton.flat:hover, spinbutton > entry, spinbutton > entry:hover,
entry.flat{focus}, spinbutton.flat{focus}, spinbutton > entry{focus} {{ background-color: transparent; box-shadow: none; outline-width: 0; }}
"
    );
}

fn range_rules(out: &mut String, p: &Paint, gtk: Gtk) {
    let Paint { acc, ring, switch_thumb, .. } = p;
    let r1 = shape::R1;
    let trough = match gtk {
        Gtk::Three => format!(
            "scale.horizontal:not(.color) > contents > trough {{ min-height: {TRACK}px; }}
scale.vertical:not(.color) > contents > trough {{ min-width: {TRACK}px; }}"
        ),
        Gtk::Four => String::from(
            "scale.horizontal:not(.color) > trough { padding-top: 1px; padding-bottom: 1px; }
scale.vertical:not(.color) > trough { padding-left: 1px; padding-right: 1px; }",
        ),
    };
    let _ = write!(
        out,
        "
check {{ border-radius: {r2}px; }}
scale:not(.color) slider {{ background-color: {acc}; background-image: none; box-shadow: 0 1px 3px rgba(0, 0, 0, 0.25); }}
scale:not(.color) slider:hover {{ box-shadow: 0 0 0 4px {ring}, 0 1px 3px rgba(0, 0, 0, 0.25); }}
{trough}
progressbar:not(.osd) > trough, progressbar:not(.osd) > trough > progress {{ border-radius: {r1}px; }}
progressbar.horizontal:not(.osd) > trough, progressbar.horizontal:not(.osd) > trough > progress {{ min-height: {TRACK}px; }}
progressbar.vertical:not(.osd) > trough, progressbar.vertical:not(.osd) > trough > progress {{ min-width: {TRACK}px; }}
switch > slider {{ background-color: {switch_thumb}; }}
switch:checked > slider {{ background-color: #ffffff; }}
",
        r2 = shape::R2
    );
}

const TRACK: u32 = 6;

fn notebook_rules(out: &mut String, p: &Paint) {
    let Paint { line, fill_1, fg_1, fg_2, .. } = p;
    let r2 = shape::R2;
    let _ = write!(
        out,
        "
notebook.frame {{ border-color: transparent; }}
notebook > header {{ background-color: transparent; border-color: transparent; }}
notebook > header.top {{ box-shadow: inset 0 -1px {line}; }}
notebook > header.bottom {{ box-shadow: inset 0 1px {line}; }}
notebook > header.left {{ box-shadow: inset -1px 0 {line}; }}
notebook > header.right {{ box-shadow: inset 1px 0 {line}; }}
notebook > header > tabs > tab {{ color: {fg_2}; }}
notebook > header > tabs > tab:hover {{ color: {fg_1}; background-color: {fill_1}; }}
notebook > header.top > tabs > tab {{ border-radius: {r2}px {r2}px 0 0; }}
notebook > header.bottom > tabs > tab {{ border-radius: 0 0 {r2}px {r2}px; }}
notebook > stack:not(:only-child) {{ background-color: transparent; }}
"
    );
}

fn list_rules(out: &mut String, p: &Paint) {
    let Paint { line, soft, soft_fg, acc, acc_text, el_1, el_2, .. } = p;
    let r2 = shape::R2;
    let _ = write!(
        out,
        "
row:selected, .navigation-sidebar > row:selected, placessidebar row:selected, treeview.view:selected {{ background-color: {soft}; color: {soft_fg}; }}
row:selected:hover {{ background-image: image({HOVER}); }}
row:selected:active {{ background-image: image({PRESS}); }}
.navigation-sidebar > row, placessidebar row {{ border-radius: {r2}px; min-height: 34px; }}
.navigation-sidebar > row:selected, placessidebar row:selected {{ background-image: linear-gradient({acc}, {acc}); background-size: 3px 18px; background-position: left center; background-repeat: no-repeat; }}
.navigation-sidebar > row:selected:hover, placessidebar row:selected:hover {{ background-image: linear-gradient({acc}, {acc}), image({HOVER}); background-size: 3px 18px, auto; }}
.navigation-sidebar > row:selected:active, placessidebar row:selected:active {{ background-image: linear-gradient({acc}, {acc}), image({PRESS}); background-size: 3px 18px, auto; }}
selection {{ background-color: {soft}; color: {soft_fg}; }}
.card, list.boxed-list, .boxed-list, list.boxed-list-separate > row {{ box-shadow: 0 0 0 1px {line}, {el_1}; }}
button.card:hover, .card.activatable:hover {{ box-shadow: 0 0 0 1px {line}, {el_2}; }}
notebook > header > tabs > tab:checked {{ color: {acc_text}; }}
"
    );
}

fn overlay_rules(out: &mut String, p: &Paint, gtk: Gtk) {
    let Paint { line, popover, fg_1, el_3, .. } = p;
    let (r2, r3) = (shape::R2, shape::R3);
    let surface = match gtk {
        Gtk::Three => "popover.background",
        Gtk::Four => "popover > contents",
    };
    let _ = write!(
        out,
        "
{surface} {{ border-radius: {r3}px; box-shadow: 0 0 0 1px {line}, {el_3}; }}
popover modelbutton {{ border-radius: {r2}px; min-height: 34px; }}
tooltip {{ border-radius: {r2}px; }}
tooltip.background {{ background-color: {popover}; color: {fg_1}; border: 1px solid {line}; }}
tooltip label {{ color: {fg_1}; }}
"
    );
}

fn gtk4_only_rules(out: &mut String, p: &Paint) {
    let Paint { line, line_2, view, popover, fg_1, fg_3, ring, acc_text, danger, warn, warn_soft, ok, el_3, .. } = p;
    let r3 = shape::R3;
    let _ = write!(
        out,
        "
banner > revealer > widget {{ background-color: {warn_soft}; color: {warn}; box-shadow: inset 3px 0 {warn}; }}
check:not(:checked):not(:indeterminate), radio:not(:checked):not(:indeterminate) {{ background-color: {view}; box-shadow: inset 0 0 0 1.5px {line_2}; }}
check:not(:checked):not(:indeterminate):hover, radio:not(:checked):not(:indeterminate):hover {{ box-shadow: inset 0 0 0 1.5px {fg_3}; }}
*:focus-visible {{ outline-color: {ring}; }}
headerbar .title, windowtitle > .title {{ font-weight: 640; }}
toast {{ background: {popover}; color: {fg_1}; border-radius: {r3}px; box-shadow: 0 0 0 1px {line}, {el_3}; --accent-color: {acc_text}; --destructive-color: {danger}; --error-color: {danger}; --warning-color: {warn}; --success-color: {ok}; }}
"
    );
}

fn titlebutton_rules(out: &mut String, p: &Paint, gtk: Gtk) {
    let Paint { fill_1, fg_2, acc, danger, .. } = p;
    let button = match gtk {
        Gtk::Three => "button.titlebutton",
        Gtk::Four => "windowcontrols > button",
    };
    let _ = write!(
        out,
        "
{button} > image {{ background-color: {fill_1}; color: {fg_2}; box-shadow: none; }}
{button}:hover > image, {button}:active > image {{ background-color: {acc}; color: #ffffff; }}
{button}.close:hover > image, {button}.close:active > image {{ background-color: {danger}; color: #ffffff; }}
"
    );
}

fn press_rules(out: &mut String, p: &Paint) {
    let Paint { fill_1, .. } = p;
    let plain = ":not(.suggested-action):not(.destructive-action):not(.osd):not(:checked):not(:disabled)";
    let _ = write!(
        out,
        "
button:not(.flat){plain}:active {{ background-color: {fill_1}; background-image: image({PRESS}); }}
button.flat{plain}:active {{ background-color: transparent; background-image: image({PRESS}); }}
"
    );
}

fn component_rules(out: &mut String, p: &Paint, gtk: Gtk) {
    window_rules(out);
    press_rules(out, p);
    titlebutton_rules(out, p, gtk);
    toggle_rules(out, p);
    entry_rules(out, p, gtk);
    range_rules(out, p, gtk);
    notebook_rules(out, p);
    list_rules(out, p);
    overlay_rules(out, p, gtk);
    if gtk == Gtk::Four {
        gtk4_only_rules(out, p);
    }
}

fn percent(alpha: f32) -> String {
    format!("{}%", (alpha.clamp(0.0, 1.0) * 100.0).round())
}

const ADWAITA_ACCENTS: [(&str, Accent); 8] = [
    ("blue", Accent::Blue),
    ("teal", Accent::Teal),
    ("green", Accent::Green),
    ("yellow", Accent::Amber),
    ("orange", Accent::Orange),
    ("red", Accent::Red),
    ("purple", Accent::Purple),
    ("slate", Accent::Graphite),
];

fn root_variables(out: &mut String, tokens: &Tokens) {
    let t = tokens;
    for (name, color) in adwaita_colors(t) {
        let _ = writeln!(out, "  --{}: {};", name.replace('_', "-"), css_color(color));
    }
    for (name, accent) in ADWAITA_ACCENTS {
        let _ = writeln!(out, "  --accent-{name}: {};", css_color(Tokens::new(t.appearance, accent).acc_bg));
    }
    for (name, value) in [
        ("border-color", css_color(t.line_2)),
        ("border-opacity", percent(t.line_2.0[3])),
        ("dim-opacity", percent(t.fg_2.0[3])),
        ("disabled-opacity", percent(DISABLED_OPACITY)),
        ("active-toggle-bg-color", css_color(t.segment_thumb())),
        ("active-toggle-fg-color", css_color(t.fg_1)),
        ("overview-bg-color", css_color(t.sunken_bg)),
        ("overview-fg-color", css_color(t.fg_1)),
        ("window-radius", format!("{}px", shape::R3)),
    ] {
        let _ = writeln!(out, "  --{name}: {value};");
    }
    Paint::declare(out, t);
}

pub const DISABLED_OPACITY: f32 = 0.42;

#[must_use]
pub fn gtk4_css(tokens: &Tokens) -> String {
    let mut out = String::from("/* Generated by Glide from its design tokens. Edits are overwritten. */\n");
    define_colors(&mut out, &adwaita_colors(tokens));
    let light = Tokens::new(Appearance::Light, tokens.accent);
    let dark = Tokens::new(Appearance::Dark, tokens.accent);
    out.push_str(":root {\n");
    root_variables(&mut out, &light);
    out.push_str("}\n@media (prefers-color-scheme: dark) {\n:root {\n");
    root_variables(&mut out, &dark);
    out.push_str("}\n}\n");
    component_rules(&mut out, &Paint::variables(tokens), Gtk::Four);
    out
}

#[must_use]
pub fn gtk3_css(tokens: &Tokens) -> String {
    let t = tokens;
    let mut out = String::from("/* Generated by Glide from its design tokens. Edits are overwritten. */\n");
    let mut colors = adwaita_colors(t);
    let fg_disabled = over(t.fg_3, t.win_bg);
    colors.extend([
        ("theme_bg_color", t.win_bg),
        ("theme_fg_color", t.fg_1),
        ("theme_base_color", t.view_bg),
        ("theme_text_color", t.fg_1),
        ("theme_selected_bg_color", t.acc_bg),
        ("theme_selected_fg_color", t.acc_fg),
        ("theme_unfocused_bg_color", t.win_bg),
        ("theme_unfocused_fg_color", t.fg_1),
        ("theme_unfocused_base_color", t.view_bg),
        ("theme_unfocused_text_color", t.fg_1),
        ("theme_unfocused_selected_bg_color", t.acc_bg),
        ("theme_unfocused_selected_fg_color", t.acc_fg),
        ("insensitive_bg_color", t.win_bg),
        ("insensitive_fg_color", fg_disabled),
        ("insensitive_base_color", t.view_bg),
        ("borders", over(t.line_2, t.win_bg)),
        ("unfocused_borders", over(t.line, t.win_bg)),
        ("content_view_bg", t.view_bg),
        ("text_view_bg", t.view_bg),
        ("wm_title", t.fg_1),
        ("wm_unfocused_title", over(t.fg_2, t.bar_bg_idle)),
        ("wm_bg", t.bar_bg),
        ("wm_unfocused_bg", t.bar_bg_idle),
    ]);
    define_colors(&mut out, &colors);
    component_rules(&mut out, &Paint::literal(t), Gtk::Three);
    out
}

fn qt_roles(tokens: &Tokens, state: QtState) -> Vec<Color> {
    let t = tokens;
    let solid = |color: Color, base: Color| over(color, base);
    let text = match state {
        QtState::Disabled => solid(t.fg_3, t.win_bg),
        _ => t.fg_1,
    };
    let view_text = match state {
        QtState::Disabled => solid(t.fg_3, t.view_bg),
        _ => t.fg_1,
    };
    let button = solid(t.fill_1, t.win_bg);
    let highlight = match state {
        QtState::Inactive => solid(t.acc_soft, t.view_bg),
        _ => t.acc_bg,
    };
    let highlighted_text = match state {
        QtState::Inactive => solid(t.acc_soft_fg, solid(t.acc_soft, t.view_bg)),
        _ => t.acc_fg,
    };
    vec![
        text,
        button,
        solid(t.fill_1, t.view_bg),
        solid(t.fill_2, t.win_bg),
        solid(t.line_2, t.win_bg),
        solid(t.line, t.win_bg),
        view_text,
        Color([1.0, 1.0, 1.0, 1.0]),
        text,
        t.view_bg,
        t.win_bg,
        Color([0.0, 0.0, 0.0, 1.0]),
        highlight,
        highlighted_text,
        t.acc_text,
        t.acc_bg_hover,
        solid(t.fill_1, t.view_bg),
        t.win_bg,
        t.popover_bg,
        t.fg_1,
        solid(t.fg_3, t.view_bg),
        t.acc_bg,
    ]
}

#[derive(Clone, Copy)]
enum QtState {
    Active,
    Inactive,
    Disabled,
}

#[must_use]
pub fn qtct_colors(tokens: &Tokens, roles: usize) -> String {
    let line =
        |state: QtState| qt_roles(tokens, state).into_iter().take(roles).map(hex_argb).collect::<Vec<_>>().join(", ");
    format!(
        "[ColorScheme]\nactive_colors={}\ndisabled_colors={}\ninactive_colors={}\n",
        line(QtState::Active),
        line(QtState::Disabled),
        line(QtState::Inactive)
    )
}

#[must_use]
pub fn kde_color_scheme(tokens: &Tokens, name: &str) -> String {
    let t = tokens;
    let groups: [(&str, Color, Color); 7] = [
        ("Window", t.win_bg, t.win_bg),
        ("View", t.view_bg, over(t.fill_1, t.view_bg)),
        ("Button", over(t.fill_1, t.win_bg), over(t.fill_2, t.win_bg)),
        ("Selection", t.acc_bg, t.acc_bg_hover),
        ("Tooltip", t.popover_bg, t.popover_bg),
        ("Complementary", t.sidebar_bg, t.sidebar_bg),
        ("Header", t.bar_bg, t.bar_bg),
    ];
    let mut out = String::new();
    for (group, background, alternate) in groups {
        let base = background;
        let (normal, inactive) = if group == "Selection" { (t.acc_fg, t.acc_fg) } else { (t.fg_1, over(t.fg_2, base)) };
        let _ = writeln!(out, "[Colors:{group}]");
        let _ = writeln!(out, "BackgroundNormal={}", rgb_triplet(background));
        let _ = writeln!(out, "BackgroundAlternate={}", rgb_triplet(alternate));
        let _ = writeln!(out, "ForegroundNormal={}", rgb_triplet(normal));
        let _ = writeln!(out, "ForegroundInactive={}", rgb_triplet(inactive));
        let _ = writeln!(out, "ForegroundActive={}", rgb_triplet(t.acc_text));
        let _ = writeln!(out, "ForegroundLink={}", rgb_triplet(t.acc_text));
        let _ = writeln!(out, "ForegroundVisited={}", rgb_triplet(t.acc_bg_hover));
        let _ = writeln!(out, "ForegroundNegative={}", rgb_triplet(t.danger));
        let _ = writeln!(out, "ForegroundNeutral={}", rgb_triplet(t.warn));
        let _ = writeln!(out, "ForegroundPositive={}", rgb_triplet(t.ok));
        let _ = writeln!(out, "DecorationFocus={}", rgb_triplet(t.acc_bg));
        let _ = writeln!(out, "DecorationHover={}", rgb_triplet(t.acc_bg));
        out.push('\n');
    }
    let _ = writeln!(out, "[ColorEffects:Inactive]\nEnable=false\n");
    let _ = writeln!(
        out,
        "[ColorEffects:Disabled]\nEnable=true\nColorEffect=0\nIntensityEffect=0\nContrastEffect=1\nContrastAmount={:.2}\n",
        1.0 - DISABLED_OPACITY
    );
    let _ = writeln!(out, "[General]\nColorScheme={name}\nName={name}\nAccentColor={}\n", rgb_triplet(t.acc_bg));
    let _ = writeln!(
        out,
        "[WM]\nactiveBackground={}\nactiveForeground={}\ninactiveBackground={}\ninactiveForeground={}",
        rgb_triplet(t.bar_bg),
        rgb_triplet(t.fg_1),
        rgb_triplet(t.bar_bg_idle),
        rgb_triplet(over(t.fg_2, t.bar_bg_idle))
    );
    out
}

fn hex(color: Color) -> String {
    let [r, g, b, _] = color.0;
    format!("#{:02x}{:02x}{:02x}", channel(r), channel(g), channel(b))
}

#[must_use]
pub fn sourceview_scheme(tokens: &Tokens, id: &str, name: &str, pair: &str) -> String {
    let t = tokens;
    let base = t.view_bg;
    let solid = |color: Color| hex(over(color, base));
    let ansi = t.terminal_ansi();
    let (variant, other) = match t.appearance {
        Appearance::Light => ("light", "dark-variant"),
        Appearance::Dark => ("dark", "light-variant"),
    };
    let mut out = String::new();
    let _ = writeln!(out, "<?xml version=\"1.0\" encoding=\"UTF-8\"?>");
    let _ = writeln!(out, "<style-scheme id=\"{id}\" name=\"{name}\" version=\"1.0\">");
    let _ = writeln!(out, "  <author>Glide</author>");
    let _ = writeln!(out, "  <description>Generated by Glide from its design tokens.</description>");
    let _ = writeln!(
        out,
        "  <metadata>\n    <property name=\"variant\">{variant}</property>\n    <property name=\"{other}\">{pair}</property>\n  </metadata>"
    );
    let styles: [(&str, String); 22] = [
        ("text", format!("foreground=\"{}\" background=\"{}\"", hex(t.fg_1), hex(base))),
        ("selection", format!("foreground=\"{}\" background=\"{}\"", solid(t.acc_soft_fg), solid(t.acc_soft))),
        ("selection-unfocused", format!("background=\"{}\"", solid(t.fill_2))),
        ("cursor", format!("foreground=\"{}\"", hex(t.acc_bg))),
        ("current-line", format!("background=\"{}\"", solid(t.fill_1))),
        ("current-line-number", format!("foreground=\"{}\" background=\"{}\"", hex(t.fg_1), solid(t.fill_1))),
        ("line-numbers", format!("foreground=\"{}\" background=\"{}\"", solid(t.fg_4), hex(base))),
        ("right-margin", format!("foreground=\"{}\" background=\"{}\"", solid(t.line), solid(t.fill_1))),
        ("bracket-match", format!("background=\"{}\" bold=\"true\"", solid(t.acc_soft))),
        ("bracket-mismatch", format!("foreground=\"{}\" underline=\"error\"", hex(t.danger))),
        ("search-match", format!("background=\"{}\"", solid(t.warn_soft))),
        ("def:comment", format!("foreground=\"{}\" italic=\"true\"", solid(t.fg_3))),
        ("def:string", format!("foreground=\"{}\"", hex(ansi[2]))),
        ("def:keyword", format!("foreground=\"{}\" bold=\"true\"", hex(ansi[5]))),
        ("def:type", format!("foreground=\"{}\" bold=\"true\"", hex(ansi[6]))),
        ("def:function", format!("foreground=\"{}\"", hex(ansi[4]))),
        ("def:identifier", format!("foreground=\"{}\"", hex(ansi[4]))),
        ("def:constant", format!("foreground=\"{}\"", hex(ansi[3]))),
        ("def:number", format!("foreground=\"{}\"", hex(ansi[3]))),
        ("def:preprocessor", format!("foreground=\"{}\"", hex(ansi[5]))),
        ("def:error", format!("underline=\"error\" underline-color=\"{}\"", hex(t.danger))),
        ("def:note", format!("foreground=\"{}\" bold=\"true\"", hex(t.acc_text))),
    ];
    for (style, attributes) in styles {
        let _ = writeln!(out, "  <style name=\"{style}\" {attributes}/>");
    }
    out.push_str("</style-scheme>\n");
    out
}

#[must_use]
pub fn qt_font(family: &str, points: u32) -> String {
    format!("\"{family},{points},-1,5,400,0,0,0,0,0,0,0,0,0,0,1\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Accent;

    #[test]
    fn opaque_colours_print_as_hex_and_translucent_ones_as_rgba() {
        assert_eq!(css_color(Color([1.0, 0.0, 0.0, 1.0])), "#ff0000");
        assert_eq!(css_color(Color([0.0, 0.0, 0.0, 0.5])), "rgba(0, 0, 0, 0.500)");
    }

    #[test]
    fn every_adwaita_colour_is_defined_in_both_gtk_versions() {
        for appearance in [Appearance::Light, Appearance::Dark] {
            let tokens = Tokens::new(appearance, Accent::Blue);
            let (four, three) = (gtk4_css(&tokens), gtk3_css(&tokens));
            for (name, _) in adwaita_colors(&tokens) {
                assert!(four.contains(&format!("@define-color {name} ")), "gtk4 lacks {name}");
                assert!(four.contains(&format!("--{}:", name.replace('_', "-"))), "gtk4 lacks the {name} variable");
                assert!(three.contains(&format!("@define-color {name} ")), "gtk3 lacks {name}");
            }
            assert!(four.contains(&css_color(tokens.acc_bg)), "the accent comes from the Glide ramp");
        }
    }

    #[test]
    fn qt_schemes_have_one_opaque_colour_per_role() {
        let tokens = Tokens::new(Appearance::Dark, Accent::Purple);
        let scheme = qtct_colors(&tokens, 21);
        for line in scheme.lines().skip(1) {
            let (_, values) = line.split_once('=').expect("key=value");
            let values: Vec<&str> = values.split(", ").collect();
            assert_eq!(values.len(), 21);
            assert!(values.iter().all(|value| value.starts_with("#ff") && value.len() == 9));
        }
    }

    #[test]
    fn gtk4_carries_both_palettes_so_running_apps_follow_the_switch() {
        let css = gtk4_css(&Tokens::new(Appearance::Light, Accent::Blue));
        let dark = Tokens::new(Appearance::Dark, Accent::Blue);
        let light = Tokens::new(Appearance::Light, Accent::Blue);
        let media = css.find("@media (prefers-color-scheme: dark)").expect("a dark block");
        assert!(css[..media].contains(&format!("--window-bg-color: {};", css_color(light.win_bg))));
        assert!(css[media..].contains(&format!("--window-bg-color: {};", css_color(dark.win_bg))));
        assert!(css.contains("var(--glide-fill-1)"), "shape rules follow the palette through variables");
        assert!(!gtk3_css(&dark).contains("var("), "GTK 3 has no CSS variables");
    }

    fn rules(css: &str) -> Vec<(Vec<String>, String)> {
        css.split('}')
            .filter_map(|chunk| {
                let (head, body) = chunk.rsplit_once('{')?;
                let head = head.rsplit([';', '{', '/']).next().unwrap_or_default();
                let selectors = head.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
                Some((selectors, body.to_string()))
            })
            .collect()
    }

    fn both_dialects() -> Vec<(String, &'static str)> {
        let mut out = Vec::new();
        for appearance in [Appearance::Light, Appearance::Dark] {
            let tokens = Tokens::new(appearance, Accent::Blue);
            out.push((gtk4_css(&tokens), ":focus-within"));
            out.push((gtk3_css(&tokens), ":focus"));
        }
        out
    }

    #[test]
    fn no_rule_restyles_a_bare_button_because_user_css_outranks_every_theme_state() {
        for (css, _) in both_dialects() {
            for (selectors, _) in rules(&css) {
                for selector in selectors {
                    let last = selector.rsplit([' ', '>']).next().unwrap_or_default();
                    assert_ne!(last, "button", "{selector} would flatten linked, split and spin buttons");
                }
            }
        }
    }

    #[test]
    fn the_entry_block_restates_every_state_a_bare_entry_rule_would_erase() {
        for (css, focus) in both_dialects() {
            let all = rules(&css);
            assert!(all.iter().any(|(selectors, _)| selectors.iter().any(|s| s == "entry")));
            for state in [".error", ".warning", ".success", ".flat", ".search", focus] {
                let wanted = format!("entry{state}");
                assert!(
                    all.iter().any(|(selectors, _)| selectors.iter().any(|s| s == &wanted)),
                    "{wanted} is missing, so the toolkit's own look for it is lost"
                );
            }
        }
    }

    #[test]
    fn gtk3_gets_only_syntax_its_parser_knows() {
        for (css, focus) in both_dialects() {
            if focus == ":focus" {
                assert!(!css.contains("var(") && !css.contains(":focus-within") && !css.contains("--"));
                assert!(!css.contains("toast") && !css.contains(":focus-visible"));
            }
        }
    }

    #[test]
    fn libadwaita_reads_the_glide_opacities_toggles_and_accent_palette() {
        let css = gtk4_css(&Tokens::new(Appearance::Dark, Accent::Blue));
        let media = css.find("@media (prefers-color-scheme: dark)").expect("a dark block");
        for block in [&css[..media], &css[media..]] {
            for name in [
                "--border-opacity",
                "--dim-opacity",
                "--disabled-opacity: 42%",
                "--active-toggle-bg-color",
                "--overview-bg-color",
                "--accent-teal",
                "--accent-slate",
                "--glide-el-3",
            ] {
                assert!(block.contains(name), "{name}");
            }
        }
        let dark = Tokens::new(Appearance::Dark, Accent::Blue);
        assert!(css[media..].contains(&format!("--dim-opacity: {}", percent(dark.fg_2.0[3]))));
        assert!(
            css[media..].contains(&format!(
                "--accent-green: {};",
                css_color(Tokens::new(Appearance::Dark, Accent::Green).acc_bg)
            ))
        );
    }

    #[test]
    fn controls_keep_the_toolkit_height_because_min_height_excludes_padding() {
        let padded = ["modelbutton", "row", "stackswitcher > button.toggle", "> trough", "> progress"];
        for (css, _) in both_dialects() {
            for (selectors, body) in rules(&css) {
                if !body.contains("min-height") {
                    continue;
                }
                for selector in selectors {
                    assert!(
                        padded.iter().any(|end| selector.ends_with(end)),
                        "{selector} would grow past the 34 px the toolkit already draws"
                    );
                }
            }
        }
    }

    #[test]
    fn notebooks_draw_underlined_tabs_and_sidebars_mark_the_selected_row_like_the_spec() {
        for (css, _) in both_dialects() {
            let all = rules(&css);
            let body = |selector: &str| {
                all.iter()
                    .filter(|(selectors, _)| selectors.iter().any(|s| s == selector))
                    .map(|(_, body)| body.as_str())
                    .collect::<String>()
            };
            assert!(body("notebook > header").contains("background-color: transparent"));
            assert!(body("notebook > header.top").contains("inset 0 -1px"));
            assert!(body("notebook.frame").contains("border-color: transparent"));
            assert!(body(".navigation-sidebar > row:selected").contains("background-size: 3px 18px"));
            assert!(body("switch > slider").contains("background-color"));
            let pressed = all
                .iter()
                .filter(|(selectors, _)| {
                    selectors.iter().any(|s| s.starts_with("button") && s.ends_with(":not(:disabled):active"))
                })
                .collect::<Vec<_>>();
            assert_eq!(pressed.len(), 2, "plain and flat buttons press with the 13 % layer");
            for (selectors, body) in pressed {
                assert!(body.contains(PRESS), "{selectors:?}");
                assert!(
                    selectors.iter().all(|s| s.contains(":not(.suggested-action)") && s.contains(":not(:checked)")),
                    "accent, destructive, OSD and toggled buttons keep the toolkit's own press"
                );
            }
        }
    }

    #[test]
    fn sourceview_schemes_pair_up_and_stay_opaque() {
        let scheme =
            sourceview_scheme(&Tokens::new(Appearance::Dark, Accent::Blue), "glide-dark", "Glide Dark", "glide");
        assert!(scheme.contains("id=\"glide-dark\""));
        assert!(scheme.contains("<property name=\"variant\">dark</property>"));
        assert!(scheme.contains("<property name=\"light-variant\">glide</property>"));
        assert!(!scheme.contains("rgba"), "GtkSourceView wants opaque colours");
        assert_eq!(scheme.matches("<style ").count(), 22);
    }

    #[test]
    fn the_kde_scheme_covers_every_colour_group() {
        let scheme = kde_color_scheme(&Tokens::new(Appearance::Light, Accent::Teal), "GlideLight");
        for group in ["Window", "View", "Button", "Selection", "Tooltip", "Complementary", "Header"] {
            assert!(scheme.contains(&format!("[Colors:{group}]")));
        }
        let teal = Tokens::new(Appearance::Light, Accent::Teal).acc_bg;
        assert!(scheme.contains(&format!("AccentColor={}", rgb_triplet(teal))));
        assert!(scheme.contains("[ColorEffects:Inactive]\nEnable=false"), "Glide does not wash out inactive windows");
        assert!(scheme.contains("ContrastAmount=0.58"), "disabled text fades to the spec's 42 %");
    }
}

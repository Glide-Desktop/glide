use std::path::PathBuf;

use crate::icons::FAMILY;

pub const PREFIX: &str = "Glide-";

pub const PLATE_RGB: [u8; 3] = [0x4d, 0x4d, 0x4d];

pub const PLATE: &str = "#4d4d4d";

pub const STAMP: &str = "glide-plates.stamp";

const COVERAGE: f32 = 0.8;

const LIGHT: u8 = 0xe6;

const SHADOW: f32 = 0.5;

#[must_use]
pub fn wants(theme: &str) -> bool {
    theme.starts_with(FAMILY) && theme.ends_with("-Dark")
}

#[must_use]
pub fn derived(theme: &str) -> String {
    format!("{PREFIX}{theme}")
}

#[must_use]
pub fn ready(roots: &[PathBuf], theme: &str) -> bool {
    let name = derived(theme);
    roots.iter().any(|root| {
        let dir = root.join(&name);
        dir.join("index.theme").is_file() && dir.join(STAMP).is_file()
    })
}

#[must_use]
pub fn prefer(theme: String, roots: &[PathBuf]) -> String {
    if wants(&theme) && ready(roots, &theme) { derived(&theme) } else { theme }
}

#[must_use]
pub fn index_theme(theme: &str) -> String {
    format!(
        "[Icon Theme]\nName={theme} (Glide)\nComment=App icons of {theme} on dark plates\nInherits={theme},hicolor\nHidden=true\nDirectories=apps/scalable\n\n[apps/scalable]\nSize=64\nContext=Applications\nMinSize=16\nMaxSize=512\nType=Scalable\n"
    )
}

#[must_use]
pub fn darken(svg: &str) -> Option<String> {
    let tags = tags(svg);
    let canvas = tags.iter().find(|tag| tag.name == "svg").and_then(|tag| canvas(tag.text))?;
    let plate = &tags[find_plate(&tags, canvas)?];
    if !property(plate.text, "fill").and_then(Colour::parse).is_some_and(Colour::is_light) {
        return None;
    }
    let dark = format!("{}{}{}", &svg[..plate.start], repaint(plate.text), &svg[plate.start + plate.text.len()..]);
    (dark != svg).then_some(dark)
}

struct Tag<'a> {
    start: usize,
    text: &'a str,
    name: &'a str,
    closing: bool,
    empty: bool,
}

fn tags(svg: &str) -> Vec<Tag<'_>> {
    let mut found = Vec::new();
    let mut rest = 0;
    while let Some(open) = svg[rest..].find('<').map(|at| rest + at) {
        let Some(close) = svg[open..].find('>').map(|at| open + at + 1) else {
            break;
        };
        let text = &svg[open..close];
        let body = text[1..text.len() - 1].trim_start();
        let closing = body.starts_with('/');
        let body = body.trim_start_matches('/');
        let name = body.split(|c: char| c.is_whitespace() || c == '/').next().unwrap_or_default();
        found.push(Tag { start: open, text, name, closing, empty: text.ends_with("/>") });
        rest = close;
    }
    found
}

fn attr<'a>(tag: &'a str, key: &str) -> Option<&'a str> {
    let mut rest = tag;
    while let Some(at) = rest.find(key) {
        let before = rest[..at].chars().next_back();
        let after = &rest[at + key.len()..];
        if before.is_some_and(char::is_whitespace)
            && let Some(value) = after.trim_start().strip_prefix('=')
        {
            let value = value.trim_start();
            let quote = value.chars().next()?;
            if quote == '"' || quote == '\'' {
                let value = &value[1..];
                return value.find(quote).map(|end| &value[..end]);
            }
        }
        rest = after;
    }
    None
}

fn style<'a>(tag: &'a str, key: &str) -> Option<&'a str> {
    attr(tag, "style")?.split(';').find_map(|rule| {
        let (name, value) = rule.split_once(':')?;
        (name.trim() == key).then(|| value.trim())
    })
}

fn property<'a>(tag: &'a str, key: &str) -> Option<&'a str> {
    style(tag, key).or_else(|| attr(tag, key))
}

fn number(value: &str) -> Option<f32> {
    value.trim().trim_end_matches("px").parse().ok()
}

fn canvas(tag: &str) -> Option<(f32, f32)> {
    if let Some(view) = attr(tag, "viewBox") {
        let parts: Vec<f32> = view.split([' ', ',']).filter(|part| !part.is_empty()).filter_map(number).collect();
        if let [_, _, width, height] = parts[..] {
            return Some((width, height));
        }
    }
    Some((number(attr(tag, "width")?)?, number(attr(tag, "height")?)?))
}

fn scale_of(tag: &str) -> f32 {
    attr(tag, "transform")
        .and_then(|transform| transform.split_once("scale(")?.1.split([')', ' ', ',']).next().and_then(number))
        .unwrap_or(1.0)
}

fn is_shadow(tag: &str) -> bool {
    attr(tag, "filter").is_some() || property(tag, "opacity").and_then(number).is_some_and(|alpha| alpha < SHADOW)
}

fn find_plate(tags: &[Tag<'_>], (width, height): (f32, f32)) -> Option<usize> {
    let mut hidden = 0usize;
    let mut scales = vec![1.0f32];
    for (index, tag) in tags.iter().enumerate() {
        if matches!(tag.name, "defs" | "clipPath" | "mask" | "filter" | "linearGradient" | "radialGradient") {
            if tag.closing {
                hidden = hidden.saturating_sub(1);
            } else if !tag.empty {
                hidden += 1;
            }
            continue;
        }
        if hidden > 0 {
            continue;
        }
        let scale = scales.last().copied().unwrap_or(1.0);
        match tag.name {
            "g" if tag.closing => {
                scales.pop();
            }
            "g" if !tag.empty => scales.push(scale * scale_of(tag.text)),
            "rect" if is_shadow(tag.text) => {}
            "rect" => {
                let rounded =
                    ["rx", "ry"].iter().any(|key| attr(tag.text, key).and_then(number).is_some_and(|r| r > 0.0));
                let size = |key| attr(tag.text, key).and_then(number).unwrap_or_default() * scale;
                if rounded && size("width") >= width * COVERAGE && size("height") >= height * COVERAGE {
                    return Some(index);
                }
                if property(tag.text, "fill").is_none_or(|fill| fill.trim() != "none") {
                    return None;
                }
            }
            "path" | "circle" | "ellipse" | "polygon" | "polyline" | "line" | "use" | "text" => return None,
            _ => {}
        }
    }
    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Colour([u8; 3]);

impl Colour {
    fn parse(value: &str) -> Option<Self> {
        let value = value.trim();
        if let Some(hex) = value.strip_prefix('#') {
            let digit = |at: usize, len: usize| u8::from_str_radix(hex.get(at..at + len)?, 16).ok();
            return match hex.len() {
                3 => Some(Self([digit(0, 1)? * 17, digit(1, 1)? * 17, digit(2, 1)? * 17])),
                6 => Some(Self([digit(0, 2)?, digit(2, 2)?, digit(4, 2)?])),
                _ => None,
            };
        }
        value.eq_ignore_ascii_case("white").then_some(Self([255; 3]))
    }

    fn is_light(self) -> bool {
        self.0.iter().all(|&channel| channel >= LIGHT)
    }
}

fn repaint(tag: &str) -> String {
    let mut text = tag.to_string();
    if let Some(fill) = attr(tag, "fill") {
        text = replace_value(&text, "fill", fill);
    }
    if let Some(style) = attr(&text, "style").map(str::to_string) {
        let rules: Vec<String> = style
            .split(';')
            .map(|rule| match rule.split_once(':') {
                Some((name, _)) if name.trim() == "fill" => format!("{name}:{PLATE}"),
                _ => rule.to_string(),
            })
            .collect();
        text = text.replacen(&format!("style=\"{style}\""), &format!("style=\"{}\"", rules.join(";")), 1);
    }
    text
}

fn replace_value(tag: &str, key: &str, value: &str) -> String {
    for quote in ['"', '\''] {
        let old = format!(" {key}={quote}{value}{quote}");
        if tag.contains(&old) {
            return tag.replacen(&old, &format!(" {key}={quote}{PLATE}{quote}"), 1);
        }
    }
    tag.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SETTINGS: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64"><rect width="64" height="64" fill="none" rx="0" ry="0"/><image xlink:href="data:image/png;base64,iVBORw0KGgo=" width="64" height="64"/><rect width="56.002" height="56.002" x="4" y="3.969" fill="#f2f2f2" rx="13.002" ry="13.002"/><g transform="rotate(-90 57.905 60.909)"><circle cx="86.814" cy="35.004" r="23" fill="gray"/></g><path d="M3.998 45.998v1" opacity=".2"/></svg>"##;

    const FIREFOX: &str = r##"<svg width="64" height="64" viewBox="0 0 16.933 16.933"><image width="16" height="16"/><rect width="14.817" height="14.817" x="1.058" y="1.05" fill="#f2f2f2" rx="3.44" ry="3.44"/><path fill="#ff3944" d="M13.62 6.398"/></svg>"##;

    #[test]
    fn the_plate_rgb_and_the_plate_colour_name_the_same_grey() {
        let [r, g, b] = PLATE_RGB;
        assert_eq!(PLATE, format!("#{r:02x}{g:02x}{b:02x}"));
    }

    #[test]
    fn a_light_plate_becomes_the_dark_plate_and_nothing_else_changes() {
        let dark = darken(SETTINGS).expect("darkened");
        assert!(dark.contains(&format!(r#"x="4" y="3.969" fill="{PLATE}""#)), "{dark}");
        assert_eq!(dark.replace(PLATE, "#f2f2f2"), SETTINGS);
    }

    #[test]
    fn the_plate_is_measured_against_the_view_box_and_the_groups_scale() {
        let dark = darken(FIREFOX).expect("darkened");
        assert!(dark.contains(&format!(r#"fill="{PLATE}" rx="3.44""#)), "{dark}");
        let scaled = SETTINGS
            .replace(r#"height="64">"#, r#"height="64" viewBox="0 0 16.933 16.933"><g transform="scale(.26458)">"#)
            .replace("<path ", "</g><path ");
        assert!(darken(&scaled).is_some_and(|dark| dark.contains(PLATE)), "{scaled}");
    }

    #[test]
    fn a_blurred_or_faint_shadow_under_the_plate_is_not_the_plate() {
        let blurred = r##"<svg width="32" height="32"><defs><filter id="a"><feGaussianBlur stdDeviation=".56"/></filter></defs><rect width="28" height="28" x="2" y="3" filter="url(#a)" opacity=".15" rx="5" ry="5"/><rect width="28" height="28" x="2" y="2" fill="#f2f2f2" rx="5" ry="5"/><path fill="#3284ff" d="M25.496 15.731"/></svg>"##;
        assert!(darken(blurred).is_some_and(|dark| dark.contains(&format!(r#"y="2" fill="{PLATE}""#))));
        let faint = blurred.replace(r#" filter="url(#a)""#, "");
        assert!(darken(&faint).is_some_and(|dark| dark.contains(PLATE)));
    }

    #[test]
    fn a_fill_in_the_style_wins_over_the_attribute() {
        let svg = r#"<svg width="64" height="64"><rect width="56" height="56" x="4" y="4" fill="url(#a)" rx="13" style="fill:#ffffff;fill-opacity:1"/></svg>"#;
        let dark = darken(svg).expect("darkened");
        assert!(dark.contains(&format!("style=\"fill:{PLATE};fill-opacity:1\"")), "{dark}");
        let coloured = r##"<svg width="64" height="64"><rect width="56" height="56" fill="#fff" rx="13" style="fill:#9487b1"/></svg>"##;
        assert_eq!(darken(coloured), None);
        let unspaced =
            "<svg width=\"64\" height=\"64\"><rect width=\"56\" height=\"56\" rx=\"13\"\nfill=\"#fff\"/></svg>";
        assert_eq!(darken(unspaced), None);
    }

    #[test]
    fn coloured_grey_and_gradient_plates_are_left_alone() {
        for fill in ["#1eadf4", "#ccc", "#b3b3b3", "url(#a)"] {
            let svg = SETTINGS.replace("#f2f2f2", fill);
            assert_eq!(darken(&svg), None, "{fill}");
        }
    }

    #[test]
    fn a_shape_painted_before_the_plate_means_there_is_no_plate() {
        let svg = SETTINGS.replace("<image ", r##"<circle r="30" fill="#e00"/><image "##);
        assert_eq!(darken(&svg), None);
        let small = SETTINGS.replace(r#"width="56.002" height="56.002""#, r#"width="30" height="30""#);
        assert_eq!(darken(&small), None);
    }

    #[test]
    fn only_dark_colloid_variants_get_dark_plates() {
        assert!(wants("Colloid-Dark"));
        assert!(wants("Colloid-Yellow-Dark"));
        assert!(!wants("Colloid-Light"));
        assert!(!wants("Colloid"));
        assert!(!wants("Papirus-Dark"));
        assert_eq!(derived("Colloid-Teal-Dark"), "Glide-Colloid-Teal-Dark");
    }

    #[test]
    fn the_derived_theme_is_hidden_and_falls_through_to_its_source() {
        let index = index_theme("Colloid-Dark");
        assert!(index.contains("Inherits=Colloid-Dark,hicolor\n"));
        assert!(index.contains("Hidden=true\n"));
        assert!(index.contains("Directories=apps/scalable\n"));
    }

    #[test]
    fn the_derived_theme_is_preferred_only_once_its_stamp_is_written() {
        let root = std::env::temp_dir().join(format!("glide-plates-ready-{}", std::process::id()));
        let dir = root.join("Glide-Colloid-Dark");
        std::fs::create_dir_all(&dir).expect("dir");
        std::fs::write(dir.join("index.theme"), index_theme("Colloid-Dark")).expect("index");
        let roots = vec![root.clone()];
        assert_eq!(prefer("Colloid-Dark".into(), &roots), "Colloid-Dark");
        std::fs::write(dir.join(STAMP), "1\n").expect("stamp");
        assert_eq!(prefer("Colloid-Dark".into(), &roots), "Glide-Colloid-Dark");
        assert_eq!(prefer("Colloid-Light".into(), &roots), "Colloid-Light");
        std::fs::remove_dir_all(root).ok();
    }
}

pub(crate) const RULES_PATH: &str = "/usr/share/X11/xkb/rules/evdev.lst";

pub const SWITCH_OPTIONS: &[&str] = &[
    "grp:win_space_toggle",
    "grp:alt_shift_toggle",
    "grp:ctrl_shift_toggle",
    "grp:caps_toggle",
    "grp:alt_space_toggle",
];

const FALLBACK: &[(&str, &str)] = &[
    ("us", "English (US)"),
    ("gb", "English (UK)"),
    ("ua", "Ukrainian"),
    ("pl", "Polish"),
    ("de", "German"),
    ("fr", "French"),
    ("es", "Spanish"),
    ("it", "Italian"),
    ("cz", "Czech"),
    ("pt", "Portuguese"),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    pub code: String,
    pub name: String,
}

impl std::fmt::Display for Layout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub layout: String,
    pub variant: String,
}

#[must_use]
pub fn parse_layouts(text: &str) -> Vec<Layout> {
    let mut in_layouts = false;
    let mut layouts = Vec::new();
    for line in text.lines() {
        if let Some(header) = line.strip_prefix("! ") {
            in_layouts = header.trim() == "layout";
            continue;
        }
        if !in_layouts {
            continue;
        }
        let mut parts = line.trim().splitn(2, char::is_whitespace);
        let (Some(code), Some(name)) = (parts.next(), parts.next()) else {
            continue;
        };
        layouts.push(Layout { code: code.to_string(), name: name.trim().to_string() });
    }
    layouts.sort_by(|a, b| a.name.cmp(&b.name));
    layouts
}

#[must_use]
pub(crate) fn available_layouts() -> Vec<Layout> {
    let parsed = std::fs::read_to_string(RULES_PATH).map(|text| parse_layouts(&text)).unwrap_or_default();
    if parsed.is_empty() {
        FALLBACK.iter().map(|(code, name)| Layout { code: (*code).to_string(), name: (*name).to_string() }).collect()
    } else {
        parsed
    }
}

#[must_use]
pub fn split_sources(layout: &str, variant: &str) -> Vec<Source> {
    let variants: Vec<&str> = variant.split(',').collect();
    layout
        .split(',')
        .map(str::trim)
        .enumerate()
        .filter(|(_, code)| !code.is_empty())
        .map(|(index, code)| Source {
            layout: code.to_string(),
            variant: variants.get(index).map(|v| v.trim().to_string()).unwrap_or_default(),
        })
        .collect()
}

#[must_use]
pub fn join_sources(sources: &[Source]) -> (String, String) {
    let layout = sources.iter().map(|source| source.layout.as_str()).collect::<Vec<_>>().join(",");
    let variant = if sources.iter().all(|source| source.variant.is_empty()) {
        String::new()
    } else {
        sources.iter().map(|source| source.variant.as_str()).collect::<Vec<_>>().join(",")
    };
    (layout, variant)
}

#[must_use]
pub fn switch_option(options: &str) -> Option<&str> {
    options.split(',').map(str::trim).find(|option| option.starts_with("grp:"))
}

pub(crate) const COMPOSE_OPTIONS: &[&str] =
    &["compose:ralt", "compose:rwin", "compose:menu", "compose:rctrl", "compose:caps", "compose:prsc", "compose:sclk"];

pub(crate) const CAPS_OPTIONS: &[&str] =
    &["caps:escape", "caps:swapescape", "ctrl:nocaps", "ctrl:swapcaps", "caps:backspace", "caps:none"];

pub(crate) const SWAP_ALT_SUPER: &str = "altwin:swap_alt_win";
pub(crate) const BOTH_SHIFTS_CAPS: &str = "shift:both_capslock";
pub(crate) const NUMPAD_DIGITS: &str = "numpad:mac";

fn options_list(options: &str) -> impl Iterator<Item = &str> {
    options.split(',').map(str::trim).filter(|option| !option.is_empty())
}

#[must_use]
pub(crate) fn is_compose_option(option: &str) -> bool {
    option.starts_with("compose:")
}

#[must_use]
pub(crate) fn is_caps_option(option: &str) -> bool {
    option.starts_with("caps:") || matches!(option, "ctrl:nocaps" | "ctrl:swapcaps")
}

#[must_use]
pub(crate) fn find_option(options: &str, family: fn(&str) -> bool) -> Option<&str> {
    options_list(options).find(|option| family(option))
}

#[must_use]
pub(crate) fn replace_option(options: &str, family: fn(&str) -> bool, next: Option<&str>) -> String {
    let mut kept: Vec<&str> = options_list(options).filter(|option| !family(option)).collect();
    if let Some(next) = next.map(str::trim).filter(|next| !next.is_empty()) {
        kept.push(next);
    }
    kept.join(",")
}

#[must_use]
pub(crate) fn has_option(options: &str, wanted: &str) -> bool {
    options_list(options).any(|option| option == wanted)
}

#[must_use]
pub(crate) fn set_option(options: &str, wanted: &str, on: bool) -> String {
    let mut kept: Vec<&str> = options_list(options).filter(|option| *option != wanted).collect();
    if on {
        kept.push(wanted);
    }
    kept.join(",")
}

#[must_use]
pub fn replace_switch_option(options: &str, next: &str) -> String {
    let mut kept: Vec<&str> = options_list(options).filter(|option| !option.starts_with("grp:")).collect();
    if !next.is_empty() {
        kept.insert(0, next);
    }
    kept.join(",")
}

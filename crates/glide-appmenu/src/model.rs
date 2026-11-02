use std::collections::HashMap;

use zbus::zvariant::{OwnedValue, Value};

pub(crate) const GTK_MENUS_INTERFACE: &str = "org.gtk.Menus";
pub(crate) const GTK_ACTIONS_INTERFACE: &str = "org.gtk.Actions";
pub(crate) const DBUSMENU_INTERFACE: &str = "com.canonical.dbusmenu";

const MAX_DEPTH: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Source {
    Gtk { bus: String, menubar: String, app: Option<String>, window: Option<String> },
    DbusMenu { bus: String, path: String },
}

impl Source {
    #[must_use]
    pub fn bus(&self) -> &str {
        match self {
            Self::Gtk { bus, .. } | Self::DbusMenu { bus, .. } => bus,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Toggle {
    None,
    Check(bool),
    Radio(bool),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Activation {
    None,
    Gtk { action: String, target: Option<OwnedValue> },
    DbusMenu(i32),
    Local(u32),
}

#[derive(Debug, Clone, PartialEq)]
pub struct MenuItem {
    pub label: String,
    pub enabled: bool,
    pub separator: bool,
    pub toggle: Toggle,
    pub accel: Option<String>,
    pub activation: Activation,
    pub children: Vec<MenuItem>,
    pub opens_submenu: bool,
}

impl MenuItem {
    #[must_use]
    pub fn action(label: &str, activation: Activation) -> Self {
        Self {
            label: label.to_string(),
            enabled: true,
            separator: false,
            toggle: Toggle::None,
            accel: None,
            activation,
            children: Vec::new(),
            opens_submenu: false,
        }
    }

    #[must_use]
    pub fn separator() -> Self {
        Self { enabled: false, separator: true, ..Self::action("", Activation::None) }
    }

    #[must_use]
    pub fn submenu(label: &str, children: Vec<Self>) -> Self {
        Self { children, ..Self::action(label, Activation::None) }
    }

    #[must_use]
    pub fn is_submenu(&self) -> bool {
        self.opens_submenu || !self.children.is_empty()
    }
}

#[must_use]
pub fn strip_mnemonic(label: &str) -> String {
    let mut out = String::with_capacity(label.len());
    let mut chars = label.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '_' {
            if chars.peek() == Some(&'_') {
                out.push('_');
                chars.next();
            }
            continue;
        }
        out.push(ch);
    }
    out
}

fn modifier_name(raw: &str) -> Option<&'static str> {
    match raw.to_ascii_lowercase().as_str() {
        "primary" | "control" | "ctrl" | "ctl" => Some("Ctrl"),
        "shift" => Some("Shift"),
        "alt" | "mod1" => Some("Alt"),
        "super" | "meta" | "mod4" | "logo" => Some("Super"),
        _ => None,
    }
}

fn key_name(raw: &str) -> String {
    let named = match raw.to_ascii_lowercase().as_str() {
        "plus" => Some("+"),
        "minus" => Some("-"),
        "equal" => Some("="),
        "comma" => Some(","),
        "period" => Some("."),
        "slash" => Some("/"),
        "backslash" => Some("\\"),
        "semicolon" => Some(";"),
        "apostrophe" => Some("'"),
        "bracketleft" => Some("["),
        "bracketright" => Some("]"),
        "grave" => Some("`"),
        "space" => Some("Space"),
        "return" | "kp_enter" => Some("Enter"),
        "escape" => Some("Esc"),
        "delete" => Some("Del"),
        "backspace" => Some("Backspace"),
        "tab" => Some("Tab"),
        "page_up" | "prior" => Some("PgUp"),
        "page_down" | "next" => Some("PgDn"),
        "home" => Some("Home"),
        "end" => Some("End"),
        "insert" => Some("Ins"),
        "up" => Some("↑"),
        "down" => Some("↓"),
        "left" => Some("←"),
        "right" => Some("→"),
        _ => None,
    };
    if let Some(named) = named {
        return named.to_string();
    }
    let mut chars = raw.chars();
    match (chars.next(), chars.next()) {
        (Some(single), None) => single.to_uppercase().collect(),
        _ => raw.to_string(),
    }
}

fn join_accel(modifiers: &[&str], key: &str) -> Option<String> {
    if key.is_empty() {
        return None;
    }
    let mut parts: Vec<String> = modifiers.iter().map(ToString::to_string).collect();
    parts.push(key_name(key));
    Some(parts.join("+"))
}

#[must_use]
pub fn format_gtk_accel(accel: &str) -> Option<String> {
    let mut modifiers = Vec::new();
    let mut rest = accel.trim();
    while let Some(stripped) = rest.strip_prefix('<') {
        let (name, tail) = stripped.split_once('>')?;
        if let Some(modifier) = modifier_name(name)
            && !modifiers.contains(&modifier)
        {
            modifiers.push(modifier);
        }
        rest = tail;
    }
    join_accel(&modifiers, rest)
}

#[must_use]
pub fn format_dbusmenu_shortcut(shortcut: &[Vec<String>]) -> Option<String> {
    let chord = shortcut.first()?;
    let (key, modifiers) = chord.split_last()?;
    let modifiers: Vec<&str> = modifiers.iter().filter_map(|m| modifier_name(m)).collect();
    join_accel(&modifiers, key)
}

fn string_of(map: &HashMap<String, OwnedValue>, key: &str) -> Option<String> {
    map.get(key).and_then(|value| String::try_from(value.clone()).ok())
}

fn bool_of(map: &HashMap<String, OwnedValue>, key: &str) -> Option<bool> {
    map.get(key).and_then(|value| bool::try_from(value).ok())
}

fn pair_of(map: &HashMap<String, OwnedValue>, key: &str) -> Option<(u32, u32)> {
    <(u32, u32)>::try_from(map.get(key)?.clone()).ok()
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ActionInfo {
    pub enabled: bool,
    pub state: Option<OwnedValue>,
}

pub(crate) type GtkMenus = HashMap<(u32, u32), Vec<HashMap<String, OwnedValue>>>;

fn gtk_toggle(action: Option<&ActionInfo>, target: Option<&OwnedValue>) -> Toggle {
    let Some(state) = action.and_then(|info| info.state.as_ref()) else {
        return Toggle::None;
    };
    match (target, &**state) {
        (None, Value::Bool(on)) => Toggle::Check(*on),
        (Some(target), _) => Toggle::Radio(target == state),
        _ => Toggle::None,
    }
}

#[must_use]
pub(crate) fn build_gtk_menu(
    menus: &GtkMenus, root: (u32, u32), actions: Option<&HashMap<String, ActionInfo>>,
) -> Vec<MenuItem> {
    let mut budget = MAX_ITEMS;
    build_gtk_level(menus, root, actions, &mut Vec::new(), &mut budget)
}

const MAX_ITEMS: usize = 4096;

fn build_gtk_level(
    menus: &GtkMenus, at: (u32, u32), actions: Option<&HashMap<String, ActionInfo>>, path: &mut Vec<(u32, u32)>,
    budget: &mut usize,
) -> Vec<MenuItem> {
    if path.len() > MAX_DEPTH || path.contains(&at) || *budget == 0 {
        return Vec::new();
    }
    let Some(entries) = menus.get(&at) else {
        return Vec::new();
    };
    path.push(at);
    let mut out: Vec<MenuItem> = Vec::new();
    for entry in entries {
        if *budget == 0 {
            break;
        }
        *budget -= 1;
        if let Some(section) = pair_of(entry, ":section") {
            let items = build_gtk_level(menus, section, actions, path, budget);
            if items.is_empty() {
                continue;
            }
            if out.last().is_some_and(|last| !last.separator) {
                out.push(MenuItem::separator());
            }
            out.extend(items);
            out.push(MenuItem::separator());
            continue;
        }

        let label = string_of(entry, "label").map(|label| strip_mnemonic(&label)).unwrap_or_default();
        if let Some(submenu) = pair_of(entry, ":submenu") {
            let children = build_gtk_level(menus, submenu, actions, path, budget);
            if !children.is_empty() {
                out.push(MenuItem::submenu(&label, children));
            }
            continue;
        }

        let action = string_of(entry, "action");
        let info = match (&action, actions) {
            (Some(name), Some(actions)) => actions.get(name),
            _ => None,
        };
        let missing = action.is_some() && actions.is_some() && info.is_none();
        let enabled = action.is_some() && info.is_none_or(|info| info.enabled) && !missing;
        match string_of(entry, "hidden-when").as_deref() {
            Some("action-missing") if missing => continue,
            Some("action-disabled") if !enabled => continue,
            _ => {}
        }
        let target = entry.get("target").cloned();
        out.push(MenuItem {
            enabled,
            toggle: gtk_toggle(info, target.as_ref()),
            accel: string_of(entry, "accel").and_then(|accel| format_gtk_accel(&accel)),
            ..MenuItem::action(&label, action.map_or(Activation::None, |action| Activation::Gtk { action, target }))
        });
    }
    path.pop();
    tidy_separators(&mut out);
    out
}

fn tidy_separators(items: &mut Vec<MenuItem>) {
    let mut previous_was_separator = true;
    items.retain(|item| {
        let keep = !(item.separator && previous_was_separator);
        if keep {
            previous_was_separator = item.separator;
        }
        keep
    });
    while items.last().is_some_and(|item| item.separator) {
        items.pop();
    }
}

#[must_use]
pub(crate) fn gtk_referenced_groups(menus: &GtkMenus) -> Vec<u32> {
    let mut groups: Vec<u32> = menus
        .values()
        .flatten()
        .filter_map(|entry| pair_of(entry, ":section").or_else(|| pair_of(entry, ":submenu")))
        .map(|(group, _)| group)
        .collect();
    groups.sort_unstable();
    groups.dedup();
    groups
}

pub(crate) type DbusMenuNode = (i32, HashMap<String, OwnedValue>, Vec<OwnedValue>);

#[must_use]
pub(crate) fn build_dbusmenu(root: &DbusMenuNode) -> Vec<MenuItem> {
    let mut budget = MAX_ITEMS;
    build_dbusmenu_children(&root.2, 0, &mut budget)
}

fn build_dbusmenu_children(children: &[OwnedValue], depth: usize, budget: &mut usize) -> Vec<MenuItem> {
    if depth > MAX_DEPTH {
        return Vec::new();
    }
    let mut out = Vec::new();
    for child in children {
        if *budget == 0 {
            break;
        }
        *budget -= 1;
        if let Ok(node) = DbusMenuNode::try_from(child.clone())
            && let Some(item) = build_dbusmenu_node(&node, depth, budget)
        {
            out.push(item);
        }
    }
    tidy_separators(&mut out);
    out
}

fn build_dbusmenu_node(node: &DbusMenuNode, depth: usize, budget: &mut usize) -> Option<MenuItem> {
    let (id, props, children) = node;
    if bool_of(props, "visible") == Some(false) {
        return None;
    }
    if string_of(props, "type").as_deref() == Some("separator") {
        return Some(MenuItem::separator());
    }
    let label = strip_mnemonic(&string_of(props, "label").unwrap_or_default());
    let toggle_state = props.get("toggle-state").and_then(|value| i32::try_from(value).ok()) == Some(1);
    let toggle = match string_of(props, "toggle-type").as_deref() {
        Some("checkmark") => Toggle::Check(toggle_state),
        Some("radio") => Toggle::Radio(toggle_state),
        _ => Toggle::None,
    };
    let children = build_dbusmenu_children(children, depth + 1, budget);
    let is_submenu = string_of(props, "children-display").as_deref() == Some("submenu");
    if is_submenu && children.is_empty() && label.is_empty() {
        return None;
    }
    Some(MenuItem {
        enabled: bool_of(props, "enabled").unwrap_or(true),
        toggle,
        accel: props
            .get("shortcut")
            .and_then(|value| Vec::<Vec<String>>::try_from(value.clone()).ok())
            .and_then(|shortcut| format_dbusmenu_shortcut(&shortcut)),
        children,
        opens_submenu: is_submenu,
        ..MenuItem::action(&label, Activation::DbusMenu(*id))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value<'a>(v: impl Into<Value<'a>>) -> OwnedValue {
        OwnedValue::try_from(v.into()).expect("plain values own cleanly")
    }

    fn entry(pairs: &[(&str, OwnedValue)]) -> HashMap<String, OwnedValue> {
        pairs.iter().map(|(k, v)| ((*k).to_string(), v.clone())).collect()
    }

    #[test]
    fn mnemonics_are_stripped_and_doubled_underscores_survive() {
        assert_eq!(strip_mnemonic("_File"), "File");
        assert_eq!(strip_mnemonic("Save _As…"), "Save As…");
        assert_eq!(strip_mnemonic("snake__case"), "snake_case");
    }

    #[test]
    fn gtk_accels_read_like_shortcuts() {
        assert_eq!(format_gtk_accel("<Primary>q").as_deref(), Some("Ctrl+Q"));
        assert_eq!(format_gtk_accel("<Control><Shift>s").as_deref(), Some("Ctrl+Shift+S"));
        assert_eq!(format_gtk_accel("<Alt>F4").as_deref(), Some("Alt+F4"));
        assert_eq!(format_gtk_accel("<Primary>plus").as_deref(), Some("Ctrl++"));
        assert_eq!(format_gtk_accel(""), None);
    }

    #[test]
    fn dbusmenu_shortcuts_use_the_first_chord() {
        let shortcut = vec![vec!["Control".to_string(), "Shift".to_string(), "n".to_string()]];
        assert_eq!(format_dbusmenu_shortcut(&shortcut).as_deref(), Some("Ctrl+Shift+N"));
        assert_eq!(format_dbusmenu_shortcut(&[]), None);
    }

    fn sample_menus() -> GtkMenus {
        let mut menus = GtkMenus::new();
        menus.insert(
            (0, 0),
            vec![
                entry(&[("label", value("_File")), (":submenu", value((1u32, 0u32)))]),
                entry(&[("label", value("_Empty")), (":submenu", value((1u32, 9u32)))]),
            ],
        );
        menus.insert(
            (1, 0),
            vec![entry(&[(":section", value((1u32, 1u32)))]), entry(&[(":section", value((1u32, 2u32)))])],
        );
        menus.insert(
            (1, 1),
            vec![
                entry(&[("label", value("_New")), ("action", value("win.new")), ("accel", value("<Primary>n"))]),
                entry(&[("label", value("Word _Wrap")), ("action", value("win.wrap"))]),
            ],
        );
        menus.insert(
            (1, 2),
            vec![
                entry(&[("label", value("_Quit")), ("action", value("app.quit"))]),
                entry(&[
                    ("label", value("Gone")),
                    ("action", value("app.gone")),
                    ("hidden-when", value("action-missing")),
                ]),
            ],
        );
        menus
    }

    #[test]
    fn gtk_sections_flatten_with_separators_and_empty_submenus_vanish() {
        let actions: HashMap<String, ActionInfo> =
            [("win.new", true, None), ("win.wrap", true, Some(value(true))), ("app.quit", false, None)]
                .into_iter()
                .map(|(name, enabled, state)| (name.to_string(), ActionInfo { enabled, state }))
                .collect();

        let bar = build_gtk_menu(&sample_menus(), (0, 0), Some(&actions));
        assert_eq!(bar.len(), 1, "an empty submenu has nothing to open");
        let file = &bar[0];
        assert_eq!(file.label, "File");
        let labels: Vec<&str> = file.children.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels, ["New", "Word Wrap", "", "Quit"]);
        assert!(file.children[2].separator);
        assert_eq!(file.children[0].accel.as_deref(), Some("Ctrl+N"));
        assert_eq!(file.children[1].toggle, Toggle::Check(true));
        assert!(!file.children[3].enabled, "a disabled action greys out");
    }

    #[test]
    fn gtk_groups_referenced_by_sections_and_submenus_are_discovered() {
        assert_eq!(gtk_referenced_groups(&sample_menus()), [1]);
    }

    fn node(id: i32, props: &[(&str, OwnedValue)], children: Vec<OwnedValue>) -> OwnedValue {
        let props = entry(props);
        let structure = zbus::zvariant::StructureBuilder::new()
            .add_field(id)
            .add_field(props)
            .add_field(children)
            .build()
            .expect("a well-formed dbusmenu node");
        value(structure)
    }

    #[test]
    fn dbusmenu_layout_becomes_a_menu_tree() {
        let open = node(
            3,
            &[("label", value("_Open")), ("shortcut", value(vec![vec!["Control".to_string(), "o".to_string()]]))],
            Vec::new(),
        );
        let separator = node(4, &[("type", value("separator"))], Vec::new());
        let hidden = node(5, &[("label", value("x")), ("visible", value(false))], Vec::new());
        let check = node(
            6,
            &[
                ("label", value("Status bar")),
                ("toggle-type", value("checkmark")),
                ("toggle-state", value(1i32)),
                ("enabled", value(false)),
            ],
            Vec::new(),
        );
        let file = node(
            2,
            &[("label", value("_File")), ("children-display", value("submenu"))],
            vec![open, separator, hidden, check],
        );
        let root: DbusMenuNode = (0, HashMap::new(), vec![file]);

        let bar = build_dbusmenu(&root);
        assert_eq!(bar.len(), 1);
        assert_eq!(bar[0].label, "File");
        let children = &bar[0].children;
        assert_eq!(children.len(), 3, "the hidden item is dropped");
        assert_eq!(children[0].accel.as_deref(), Some("Ctrl+O"));
        assert_eq!(children[0].activation, Activation::DbusMenu(3));
        assert!(children[1].separator);
        assert_eq!(children[2].toggle, Toggle::Check(true));
        assert!(!children[2].enabled);
    }

    #[test]
    fn a_self_referencing_gtk_menu_builds_quickly_and_finitely() {
        let mut menus = GtkMenus::new();
        menus.insert((0, 0), vec![entry(&[("label", value("_Loop")), (":submenu", value((1u32, 0u32)))])]);
        let mut looping: Vec<HashMap<String, OwnedValue>> =
            (0..6).map(|_| entry(&[(":section", value((1u32, 0u32)))])).collect();
        looping.push(entry(&[("label", value("Again")), (":submenu", value((1u32, 0u32)))]));
        looping.push(entry(&[("label", value("_Leaf")), ("action", value("app.leaf"))]));
        menus.insert((1, 0), looping);

        let started = std::time::Instant::now();
        let bar = build_gtk_menu(&menus, (0, 0), None);
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
        assert_eq!(bar.len(), 1);
        let labels: Vec<&str> = bar[0].children.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels, ["Leaf"]);
    }

    #[test]
    fn an_unfilled_dbusmenu_submenu_can_still_be_opened() {
        let lazy = node(9, &[("label", value("_Recent")), ("children-display", value("submenu"))], Vec::new());
        let root: DbusMenuNode = (0, HashMap::new(), vec![lazy]);

        let bar = build_dbusmenu(&root);
        assert_eq!(bar.len(), 1);
        assert!(bar[0].children.is_empty());
        assert!(bar[0].is_submenu());
        assert_eq!(bar[0].activation, Activation::DbusMenu(9));
    }

    #[test]
    fn dbusmenu_item_budget_is_shared_across_submenus() {
        let leaf = node(2, &[("label", value("Leaf"))], Vec::new());
        let submenu = node(1, &[("label", value("Menu"))], vec![leaf; MAX_ITEMS]);
        let root = (0, HashMap::new(), vec![submenu.clone(), submenu]);
        let bar = build_dbusmenu(&root);
        assert_eq!(bar.len(), 1);
        assert_eq!(bar[0].children.len(), MAX_ITEMS - 1);
    }
}

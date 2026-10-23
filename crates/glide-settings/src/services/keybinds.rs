use std::sync::OnceLock;

use toml_edit::{Array, ArrayOfTables, DocumentMut, InlineTable, Item, Table, TableLike, Value};

use super::config::ConfigStore;

pub const DEFAULTS_TOML: &str = include_str!("default_keybinds.toml");
pub const CONFIG_KEY: &str = "keybinds";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Mod {
    Super,
    Ctrl,
    Alt,
    Shift,
}

impl Mod {
    pub const ALL: [Self; 4] = [Self::Super, Self::Ctrl, Self::Alt, Self::Shift];

    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Super => "super",
            Self::Ctrl => "ctrl",
            Self::Alt => "alt",
            Self::Shift => "shift",
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Super => "Super",
            Self::Ctrl => "Ctrl",
            Self::Alt => "Alt",
            Self::Shift => "Shift",
        }
    }

    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "super" | "logo" => Some(Self::Super),
            "ctrl" | "control" => Some(Self::Ctrl),
            "alt" => Some(Self::Alt),
            "shift" => Some(Self::Shift),
            _ => None,
        }
    }
}

pub const NAMED_ACTIONS: &[&str] = &[
    "close-window",
    "close-app",
    "maximize",
    "fullscreen",
    "minimize",
    "open-window-menu",
    "tile-left",
    "tile-right",
    "tile-maximize",
    "tile-restore",
    "move-window-to-monitor-left",
    "move-window-to-monitor-right",
    "force-quit",
    "show-desktop",
    "next-workspace",
    "prev-workspace",
    "move-window-to-next-workspace",
    "move-window-to-prev-workspace",
    "toggle-overview",
    "switch-windows",
    "switch-windows-backward",
    "switch-app-windows",
    "switch-app-windows-backward",
    "focus-top-bar",
    "focus-dock",
    "toggle-launcher",
    "run-command",
    "terminal",
    "screenshot",
    "screenshot-screen",
    "screenshot-window",
    "toggle-notification-center",
    "toggle-quick-settings",
    "toggle-do-not-disturb",
    "toggle-caffeine",
    "fill-password",
    "screen-mirroring",
    "lock",
    "power-dialog",
    "logout",
    "quit",
    "media-play-pause",
    "media-play",
    "media-pause",
    "media-stop",
    "media-next",
    "media-previous",
    "media-forward",
    "media-rewind",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Named(String),
    Exec(String),
    SwitchWorkspace(i64),
    MoveToWorkspace(i64),
    Vt(i64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Category {
    Windows,
    Workspaces,
    Switching,
    System,
    Media,
    Custom,
}

impl Category {
    pub const ALL: [Self; 6] =
        [Self::Windows, Self::Workspaces, Self::Switching, Self::System, Self::Media, Self::Custom];
}

const MEDIA_TOOLS: &[&str] = &["wpctl", "pactl", "amixer", "brightnessctl", "playerctl"];

impl Action {
    #[must_use]
    pub fn category(&self) -> Category {
        match self {
            Self::Named(name) => match name.as_str() {
                "close-window"
                | "close-app"
                | "maximize"
                | "fullscreen"
                | "minimize"
                | "open-window-menu"
                | "show-desktop"
                | "force-quit"
                | "tile-left"
                | "tile-right"
                | "tile-maximize"
                | "tile-restore"
                | "move-window-to-monitor-left"
                | "move-window-to-monitor-right" => Category::Windows,
                "next-workspace"
                | "prev-workspace"
                | "toggle-overview"
                | "move-window-to-next-workspace"
                | "move-window-to-prev-workspace" => Category::Workspaces,
                "switch-windows"
                | "switch-windows-backward"
                | "switch-app-windows"
                | "switch-app-windows-backward"
                | "focus-top-bar"
                | "focus-dock" => Category::Switching,
                "toggle-launcher"
                | "run-command"
                | "terminal"
                | "screenshot"
                | "screenshot-screen"
                | "screenshot-window"
                | "lock"
                | "power-dialog"
                | "logout"
                | "quit"
                | "toggle-notification-center"
                | "toggle-quick-settings"
                | "toggle-do-not-disturb"
                | "toggle-caffeine"
                | "fill-password"
                | "screen-mirroring" => Category::System,
                media if media.starts_with("media-") => Category::Media,
                _ => Category::Custom,
            },
            Self::SwitchWorkspace(_) | Self::MoveToWorkspace(_) => Category::Workspaces,
            Self::Vt(_) => Category::System,
            Self::Exec(command) => {
                let program = command.split_whitespace().next().unwrap_or_default();
                if MEDIA_TOOLS.contains(&program) {
                    Category::Media
                } else if command.trim() == "loginctl lock-session" {
                    Category::System
                } else {
                    Category::Custom
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keybind {
    pub mods: Vec<Mod>,
    pub key: String,
    pub action: Action,
}

impl Keybind {
    #[must_use]
    pub fn sorted_mods(&self) -> Vec<Mod> {
        let mut mods = self.mods.clone();
        mods.sort_unstable();
        mods.dedup();
        mods
    }

    #[must_use]
    pub fn same_combo(&self, other: &Self) -> bool {
        let key = |bind: &Self| normalize_key(&bind.key).unwrap_or_else(|| bind.key.to_ascii_lowercase());
        self.sorted_mods() == other.sorted_mods() && key(self) == key(other)
    }
}

pub const NAMED_KEYS: &[&str] = &[
    "Return",
    "Tab",
    "space",
    "Escape",
    "BackSpace",
    "Delete",
    "Insert",
    "Home",
    "End",
    "Prior",
    "Next",
    "Page_Up",
    "Page_Down",
    "Up",
    "Down",
    "Left",
    "Right",
    "Print",
    "Pause",
    "Scroll_Lock",
    "Menu",
    "grave",
    "minus",
    "equal",
    "bracketleft",
    "bracketright",
    "backslash",
    "semicolon",
    "apostrophe",
    "comma",
    "period",
    "slash",
];

const ALIASES: &[(&str, &str)] = &[
    ("enter", "Return"),
    ("esc", "Escape"),
    ("del", "Delete"),
    ("ins", "Insert"),
    ("pgup", "Prior"),
    ("pageup", "Prior"),
    ("page_up", "Prior"),
    ("pgdn", "Next"),
    ("pagedown", "Next"),
    ("page_down", "Next"),
    ("printscreen", "Print"),
    ("prtsc", "Print"),
    ("`", "grave"),
    ("-", "minus"),
    ("=", "equal"),
    ("[", "bracketleft"),
    ("]", "bracketright"),
    ("\\", "backslash"),
    (";", "semicolon"),
    ("'", "apostrophe"),
    (",", "comma"),
    (".", "period"),
    ("/", "slash"),
];

fn is_function_key(text: &str) -> bool {
    text.strip_prefix('F').and_then(|rest| rest.parse::<u8>().ok()).is_some_and(|n| (1..=35).contains(&n))
        && !text[1..].starts_with('0')
}

fn is_prefixed_symbol(text: &str, prefix: &str) -> bool {
    text.strip_prefix(prefix)
        .is_some_and(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
}

#[must_use]
pub fn is_valid_key(key: &str) -> bool {
    let mut chars = key.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        return c.is_ascii_alphanumeric();
    }
    is_function_key(key)
        || NAMED_KEYS.contains(&key)
        || is_prefixed_symbol(key, "XF86")
        || is_prefixed_symbol(key, "KP_")
}

#[must_use]
pub fn normalize_key(input: &str) -> Option<String> {
    if input == " " {
        return Some(String::from("space"));
    }
    let text = input.trim();
    if text.is_empty() {
        return None;
    }
    let lower = text.to_ascii_lowercase();
    if let Some((_, key)) = ALIASES.iter().find(|(alias, _)| *alias == lower) {
        return Some((*key).to_string());
    }
    if let Some(named) = NAMED_KEYS.iter().find(|name| name.eq_ignore_ascii_case(text)) {
        return Some((*named).to_string());
    }
    let mut chars = text.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        return c.is_ascii_alphanumeric().then(|| c.to_ascii_lowercase().to_string());
    }
    if lower.starts_with('f') {
        let upper = format!("F{}", &text[1..]);
        if is_function_key(&upper) {
            return Some(upper);
        }
    }
    is_valid_key(text).then(|| text.to_string())
}

#[must_use]
pub fn key_label(key: &str) -> String {
    let named = match key {
        "Return" => "Enter",
        "space" => "Space",
        "Escape" => "Esc",
        "BackSpace" => "Backspace",
        "Prior" | "Page_Up" => "Page Up",
        "Next" | "Page_Down" => "Page Down",
        "Up" => "↑",
        "Down" => "↓",
        "Left" => "←",
        "Right" => "→",
        "Print" => "PrtSc",
        "Scroll_Lock" => "Scroll Lock",
        "grave" => "`",
        "minus" => "-",
        "equal" => "=",
        "bracketleft" => "[",
        "bracketright" => "]",
        "backslash" => "\\",
        "semicolon" => ";",
        "apostrophe" => "'",
        "comma" => ",",
        "period" => ".",
        "slash" => "/",
        "XF86AudioRaiseVolume" => "Vol +",
        "XF86AudioLowerVolume" => "Vol −",
        "XF86AudioMute" => "Mute",
        "XF86AudioMicMute" => "Mic Mute",
        "XF86MonBrightnessUp" => "Bright +",
        "XF86MonBrightnessDown" => "Bright −",
        "XF86AudioPlay" => "Play",
        "XF86AudioPause" => "Pause",
        "XF86AudioStop" => "Stop",
        "XF86AudioNext" => "Next",
        "XF86AudioPrev" => "Prev",
        "XF86AudioForward" => "Fwd",
        "XF86AudioRewind" => "Rew",
        "XF86PowerOff" => "Power",
        _ => "",
    };
    if !named.is_empty() {
        return named.to_string();
    }
    if let Some(rest) = key.strip_prefix("XF86") {
        return rest.to_string();
    }
    if let Some(rest) = key.strip_prefix("KP_") {
        return format!("Num {rest}");
    }
    let mut chars = key.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => c.to_uppercase().to_string(),
        _ => key.to_string(),
    }
}

fn int(item: &Item) -> Result<i64, String> {
    item.as_integer().ok_or_else(|| String::from("expected a number"))
}

fn action_from_table(table: &dyn TableLike) -> Result<Action, String> {
    let mut entries = table.iter();
    let (Some((name, item)), None) = (entries.next(), entries.next()) else {
        return Err(String::from("an action table needs exactly one entry"));
    };
    match name {
        "exec" => item
            .as_str()
            .map(|command| Action::Exec(command.to_string()))
            .ok_or_else(|| String::from("exec needs a command line")),
        "switch-workspace" => int(item).map(Action::SwitchWorkspace),
        "move-window-to-workspace" => int(item).map(Action::MoveToWorkspace),
        "vt" => int(item).map(Action::Vt),
        other => Err(format!("unknown action {other}")),
    }
}

fn keybind_from_table(table: &dyn TableLike) -> Result<Keybind, String> {
    if let Some((name, _)) = table.iter().find(|(name, _)| !matches!(*name, "mods" | "key" | "action")) {
        return Err(format!("unknown keybind field {name}"));
    }
    let mods = match table.get("mods") {
        None => Vec::new(),
        Some(item) => item
            .as_array()
            .ok_or_else(|| String::from("mods must be a list"))?
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .and_then(Mod::parse)
                    .ok_or_else(|| format!("unknown modifier {}", value.to_string().trim()))
            })
            .collect::<Result<Vec<_>, _>>()?,
    };
    let key = table.get("key").and_then(Item::as_str).ok_or_else(|| String::from("a keybind needs a key"))?.to_string();
    let action = match table.get("action") {
        Some(item) => match item.as_str() {
            Some(name) => Action::Named(name.to_string()),
            None => action_from_table(
                item.as_table_like().ok_or_else(|| String::from("action must be a name or a table"))?,
            )?,
        },
        None => return Err(String::from("a keybind needs an action")),
    };
    Ok(Keybind { mods, key, action })
}

pub fn from_item(item: &Item) -> Result<Vec<Keybind>, String> {
    if let Some(tables) = item.as_array_of_tables() {
        return tables.iter().map(|table| keybind_from_table(table)).collect();
    }
    if let Some(array) = item.as_array() {
        return array
            .iter()
            .map(|value| {
                value
                    .as_inline_table()
                    .ok_or_else(|| String::from("keybinds must be tables"))
                    .and_then(|table| keybind_from_table(table))
            })
            .collect();
    }
    Err(String::from("keybinds must be a list of tables"))
}

pub fn parse_document(text: &str) -> Result<Vec<Keybind>, String> {
    let doc: DocumentMut = text.parse().map_err(|err| format!("{err}"))?;
    doc.get(CONFIG_KEY).map_or_else(|| Ok(Vec::new()), from_item)
}

#[must_use]
pub fn defaults() -> &'static [Keybind] {
    static DEFAULTS: OnceLock<Vec<Keybind>> = OnceLock::new();
    DEFAULTS.get_or_init(|| parse_document(DEFAULTS_TOML).unwrap_or_default())
}

fn action_item(action: &Action) -> Item {
    let (key, value): (&str, Value) = match action {
        Action::Named(name) => return toml_edit::value(name.as_str()),
        Action::Exec(command) => ("exec", command.as_str().into()),
        Action::SwitchWorkspace(index) => ("switch-workspace", (*index).into()),
        Action::MoveToWorkspace(index) => ("move-window-to-workspace", (*index).into()),
        Action::Vt(index) => ("vt", (*index).into()),
    };
    let mut table = InlineTable::new();
    table.insert(key, value);
    Item::Value(Value::InlineTable(table))
}

#[must_use]
pub fn to_item(binds: &[Keybind]) -> Item {
    let mut tables = ArrayOfTables::new();
    for bind in binds {
        let mut table = Table::new();
        let mods: Array = bind.mods.iter().map(|m| m.key()).collect();
        table.insert("mods", toml_edit::value(mods));
        table.insert("key", toml_edit::value(bind.key.as_str()));
        table.insert("action", action_item(&bind.action));
        tables.push(table);
    }
    Item::ArrayOfTables(tables)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loaded {
    pub binds: Vec<Keybind>,
    pub customised: bool,
}

pub fn load(config: &ConfigStore) -> Result<Loaded, String> {
    let binds = match config.root_item(CONFIG_KEY) {
        Some(item) => from_item(item)?,
        None => Vec::new(),
    };
    let customised = !binds.is_empty();
    Ok(Loaded { binds: if customised { binds } else { defaults().to_vec() }, customised })
}

#[must_use]
pub fn current(config: &ConfigStore) -> Vec<Keybind> {
    load(config).map_or_else(|_| defaults().to_vec(), |loaded| loaded.binds)
}

#[must_use]
pub fn combo_for(binds: &[Keybind], action: &str) -> Option<String> {
    let bind = binds.iter().find(|bind| bind.action == Action::Named(action.to_string()))?;
    let mut parts: Vec<String> = bind.sorted_mods().iter().map(|m| m.key().to_string()).collect();
    parts.push(bind.key.clone());
    Some(parts.join("+"))
}

pub fn store(config: &mut ConfigStore, binds: &[Keybind]) {
    if binds.is_empty() {
        return;
    }
    let defaults = defaults();
    let unchanged = binds.len() == defaults.len()
        && binds.iter().zip(defaults).all(|(bind, default)| bind.action == default.action && bind.same_combo(default));
    if unchanged {
        config.remove_root_item(CONFIG_KEY);
    } else {
        config.set_root_item(CONFIG_KEY, to_item(binds));
    }
}

pub fn reset(config: &mut ConfigStore) {
    config.remove_root_item(CONFIG_KEY);
}

#[must_use]
pub fn conflicts(binds: &[Keybind]) -> Vec<(usize, usize)> {
    let mut pairs = Vec::new();
    for (i, a) in binds.iter().enumerate() {
        for (j, b) in binds.iter().enumerate().skip(i + 1) {
            if a.same_combo(b) {
                pairs.push((i, j));
            }
        }
    }
    pairs
}

#[must_use]
pub fn default_for(binds: &[Keybind], index: Option<usize>, action: &Action) -> Option<&'static Keybind> {
    let mut candidates = defaults().iter().filter(|bind| bind.action == *action);
    let current = index.and_then(|index| binds.get(index));
    if let Some(current) = current
        && let Some(exact) = candidates.clone().find(|default| current.action == *action && default.same_combo(current))
    {
        return Some(exact);
    }
    let position = index.map_or(binds.len(), |index| index.min(binds.len()));
    let occurrence = binds[..position].iter().filter(|bind| bind.action == *action).count();
    candidates.clone().nth(occurrence).or_else(|| candidates.next())
}

use std::path::{Path, PathBuf};

use toml_edit::{ArrayOfTables, DocumentMut, Item, Table, value};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pin {
    pub app_id: String,
    pub name: String,
    pub command: String,
}

#[must_use]
pub(crate) fn pins_path() -> Option<PathBuf> {
    glide_config::xdg_path("glide", "pins.toml")
}

pub(crate) fn parse_pins(text: &str) -> Result<Vec<Pin>, String> {
    let doc = text.parse::<DocumentMut>().map_err(|err| err.message().trim().to_string())?;
    let Some(item) = doc.get("pinned") else {
        return Ok(Vec::new());
    };
    let tables = item.as_array_of_tables().ok_or_else(|| String::from("`pinned` is not a list of tables"))?;
    Ok(tables
        .iter()
        .filter_map(|table| {
            let text = |key: &str| table.get(key).and_then(Item::as_str).map(str::to_string);
            Some(Pin {
                app_id: text("app_id")?,
                name: text("name").unwrap_or_default(),
                command: text("command").unwrap_or_default(),
            })
        })
        .filter(|pin| !pin.app_id.trim().is_empty())
        .collect())
}

#[must_use]
pub(crate) fn render_pins(pins: &[Pin]) -> String {
    let mut array = ArrayOfTables::new();
    for pin in pins {
        let mut table = Table::new();
        table.insert("app_id", value(pin.app_id.as_str()));
        table.insert("name", value(pin.name.as_str()));
        table.insert("command", value(pin.command.as_str()));
        array.push(table);
    }
    let mut doc = DocumentMut::new();
    doc.insert("pinned", Item::ArrayOfTables(array));
    doc.to_string()
}

pub fn load_from(path: &Path) -> Result<Option<Vec<Pin>>, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => parse_pins(&text).map(Some),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err.to_string()),
    }
}

pub fn load() -> Result<Option<Vec<Pin>>, String> {
    let path = pins_path().ok_or_else(|| String::from("no configuration directory"))?;
    load_from(&path)
}

pub fn save_to(path: &Path, pins: &[Pin]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|err| err.to_string())?;
    }
    let target = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    super::config::write_atomic(&target, render_pins(pins).as_bytes()).map_err(|err| err.to_string())
}

pub fn save(pins: &[Pin]) -> Result<(), String> {
    let path = pins_path().ok_or_else(|| String::from("no configuration directory"))?;
    save_to(&path, pins)
}

#[must_use]
pub fn moved(pins: &[Pin], index: usize, up: bool) -> Vec<Pin> {
    let mut pins = pins.to_vec();
    let target = if up { index.checked_sub(1) } else { Some(index + 1).filter(|next| *next < pins.len()) };
    if let Some(target) = target
        && index < pins.len()
    {
        pins.swap(index, target);
    }
    pins
}

#[must_use]
pub fn same_app(a: &str, b: &str) -> bool {
    let key = |id: &str| id.trim().trim_end_matches(".desktop").to_ascii_lowercase();
    key(a) == key(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pin(app_id: &str) -> Pin {
        Pin { app_id: app_id.to_string(), name: app_id.to_uppercase(), command: app_id.to_lowercase() }
    }

    #[test]
    fn the_file_the_compositor_writes_parses() {
        let text = "[[pinned]]\napp_id = \"org.gnome.Nautilus\"\nname = \"Files\"\ncommand = \"nautilus\"\n\n[[pinned]]\napp_id = \"Alacritty\"\nname = \"Terminal\"\n";
        let pins = parse_pins(text).expect("valid pins");
        assert_eq!(pins.len(), 2);
        assert_eq!(pins[0].name, "Files");
        assert_eq!(pins[1].command, "", "command is optional in the compositor too");
    }

    #[test]
    fn rendering_round_trips_and_keeps_the_order() {
        let pins = vec![pin("b"), pin("a"), pin("c")];
        assert_eq!(parse_pins(&render_pins(&pins)).expect("valid"), pins);
        assert_eq!(parse_pins("").expect("empty file"), Vec::new());
    }

    #[test]
    fn a_malformed_file_is_an_error_not_an_empty_dock() {
        assert!(parse_pins("pinned = 3\n").is_err());
        assert!(parse_pins("[[pinned\n").is_err());
    }

    #[test]
    fn moving_swaps_neighbours_and_stops_at_the_ends() {
        let pins = vec![pin("a"), pin("b"), pin("c")];
        let ids = |list: Vec<Pin>| list.into_iter().map(|p| p.app_id).collect::<Vec<_>>();
        assert_eq!(ids(moved(&pins, 1, true)), ["b", "a", "c"]);
        assert_eq!(ids(moved(&pins, 1, false)), ["a", "c", "b"]);
        assert_eq!(ids(moved(&pins, 0, true)), ["a", "b", "c"]);
        assert_eq!(ids(moved(&pins, 2, false)), ["a", "b", "c"]);
    }

    #[test]
    fn desktop_ids_and_app_ids_compare_loosely() {
        assert!(same_app("org.gnome.Nautilus.desktop", "org.gnome.nautilus"));
        assert!(!same_app("firefox", "firefox-esr"));
    }

    #[test]
    fn saving_writes_the_whole_list_atomically() {
        let dir = std::env::temp_dir().join(format!("glide-pins-{}", std::process::id()));
        let path = dir.join("pins.toml");
        save_to(&path, &[pin("x"), pin("y")]).expect("writable");
        assert_eq!(load_from(&path).expect("readable"), Some(vec![pin("x"), pin("y")]));
        assert_eq!(load_from(&dir.join("missing.toml")).expect("ok"), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

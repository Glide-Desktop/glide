use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub const NAMESPACE: &str = "pins";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pin {
    pub app_id: String,
    pub name: String,
    #[serde(default)]
    pub command: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Stored {
    pub pinned: Option<Vec<Pin>>,
}

#[must_use]
pub fn legacy_path() -> Option<PathBuf> {
    crate::xdg_path("glide", "pins.toml")
}

#[must_use]
pub fn from_values(values: &crate::service::Values) -> Option<Vec<Pin>> {
    let stored: Stored = crate::service::read(values).unwrap_or_default();
    stored.pinned.map(|pins| pins.into_iter().filter(|pin| !pin.app_id.trim().is_empty()).collect())
}

pub fn load() -> io::Result<Option<Vec<Pin>>> {
    let values =
        crate::service::Client::session().and_then(|client| client.get_all(NAMESPACE)).map_err(io::Error::other)?;
    Ok(from_values(&values))
}

pub fn save(pins: &[Pin]) -> io::Result<()> {
    let value = serde_json::to_value(pins).map_err(io::Error::other)?;
    crate::service::Client::session()
        .and_then(|client| client.set_many(NAMESPACE, &crate::service::Values::from([("pinned".to_owned(), value)])))
        .map_err(io::Error::other)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unset_list_differs_from_an_empty_one_and_blank_ids_are_dropped() {
        let values = |pinned: serde_json::Value| crate::service::Values::from([("pinned".to_owned(), pinned)]);
        assert_eq!(from_values(&values(serde_json::Value::Null)), None, "never pinned: the dock seeds itself");
        assert_eq!(from_values(&values(serde_json::json!([]))), Some(Vec::new()), "everything unpinned on purpose");
        let pins = from_values(&values(serde_json::json!([
            { "app_id": "Alacritty", "name": "Terminal" },
            { "app_id": " ", "name": "blank" },
        ])));
        assert_eq!(
            pins,
            Some(vec![Pin { app_id: "Alacritty".into(), name: "Terminal".into(), command: String::new() }])
        );
    }
}

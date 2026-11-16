use std::collections::BTreeMap;

use serde_json::{Map, Value};

pub const BUS_NAME: &str = "org.glide.Config";
pub const BUS_PATH: &str = "/org/glide/Config1";
pub const BUS_INTERFACE: &str = "org.glide.Config1";
pub const CHANGED: &str = "Changed";

pub const SHELL: &str = "shell";

pub const GUARDED: &[&str] = &["permissions", "passwords"];
pub const TRUSTED_WRITERS: &[&str] = &["glide", "glide-settings", "glide-passwords"];

pub type Values = BTreeMap<String, Value>;

#[must_use]
pub fn nest(values: &Values) -> Value {
    let mut root = Map::new();
    for (key, value) in values {
        match key.split_once('.') {
            Some((table, field)) => {
                let slot = root.entry(table).or_insert_with(|| Value::Object(Map::new()));
                if let Value::Object(table) = slot {
                    table.insert(field.to_owned(), value.clone());
                }
            }
            None => {
                root.insert(key.clone(), value.clone());
            }
        }
    }
    Value::Object(root)
}

#[must_use]
pub fn flatten(value: Value) -> Values {
    let Value::Object(root) = value else { return Values::new() };
    let mut values = Values::new();
    for (key, value) in root {
        match value {
            Value::Object(table) => {
                for (field, value) in table {
                    values.insert(format!("{key}.{field}"), value);
                }
            }
            value => {
                values.insert(key, value);
            }
        }
    }
    values
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn dotted_keys_nest_into_tables_and_flatten_back() {
        let values = Values::from([
            ("topbar.accent".to_owned(), json!("teal")),
            ("topbar.tray".to_owned(), json!(false)),
            ("keybinds".to_owned(), json!([{ "key": "q" }])),
            ("dock.theme".to_owned(), json!({ "radius": 4 })),
        ]);
        let nested = nest(&values);
        assert_eq!(nested["topbar"], json!({ "accent": "teal", "tray": false }));
        assert_eq!(nested["keybinds"], json!([{ "key": "q" }]));
        assert_eq!(nested["dock"]["theme"], json!({ "radius": 4 }), "deeper tables stay one value");
        assert_eq!(flatten(nested), values);
    }
}

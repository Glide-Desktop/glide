use std::{collections::HashMap, fmt::Write};

use serde::de::DeserializeOwned;
use toml::{Table, Value};

const UNKNOWN_FIELD: &str = "unknown field `";
const MAX_REPAIRS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Segment {
    Key(String),
    Index(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repaired<T> {
    pub value: T,
    pub ignored: Vec<String>,
}

fn unknown_field(message: &str) -> Option<&str> {
    message.strip_prefix(UNKNOWN_FIELD)?.split('`').next().filter(|field| !field.is_empty())
}

fn header_path(name: &str, arrays: &HashMap<String, usize>) -> Vec<Segment> {
    let mut path = Vec::new();
    let mut prefix = String::new();
    for part in name.split('.').map(|part| part.trim().trim_matches(['"', '\''])) {
        if !prefix.is_empty() {
            prefix.push('.');
        }
        prefix.push_str(part);
        path.push(Segment::Key(part.to_string()));
        if let Some(count) = arrays.get(&prefix) {
            path.push(Segment::Index(count.saturating_sub(1)));
        }
    }
    path
}

fn table_path_at(text: &str, offset: usize) -> Vec<Segment> {
    let mut path = Vec::new();
    let mut arrays: HashMap<String, usize> = HashMap::new();
    let mut start = 0;
    for line in text.split_inclusive('\n') {
        if start > offset {
            break;
        }
        start += line.len();
        let line = line.trim();
        if let Some(name) = line.strip_prefix("[[").and_then(|rest| rest.strip_suffix("]]")) {
            *arrays.entry(name.trim().to_string()).or_insert(0) += 1;
            path = header_path(name, &arrays);
        } else if let Some(name) = line.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')) {
            path = header_path(name, &arrays);
        }
    }
    path
}

fn table_at<'a>(root: &'a mut Table, path: &[Segment]) -> Option<&'a mut Table> {
    let mut table = root;
    let mut segments = path.iter();
    while let Some(segment) = segments.next() {
        let Segment::Key(key) = segment else {
            return None;
        };
        table = match table.get_mut(key)? {
            Value::Table(inner) => inner,
            Value::Array(items) => {
                let Some(Segment::Index(index)) = segments.next() else {
                    return None;
                };
                items.get_mut(*index)?.as_table_mut()?
            }
            _ => return None,
        };
    }
    Some(table)
}

fn dotted(path: &[Segment], field: &str) -> String {
    let mut out = String::new();
    for segment in path {
        match segment {
            Segment::Key(key) => {
                if !out.is_empty() {
                    out.push('.');
                }
                out.push_str(key);
            }
            Segment::Index(index) => {
                let _ = write!(out, "[{index}]");
            }
        }
    }
    if !out.is_empty() {
        out.push('.');
    }
    out + field
}

pub fn parse<T: DeserializeOwned>(text: &str) -> Result<Repaired<T>, toml::de::Error> {
    let strict = match toml::from_str::<T>(text) {
        Ok(value) => return Ok(Repaired { value, ignored: Vec::new() }),
        Err(err) => err,
    };
    let Ok(mut table) = text.parse::<Table>() else {
        return Err(strict);
    };
    let mut ignored = Vec::new();
    for _ in 0..MAX_REPAIRS {
        let Ok(canonical) = toml::to_string(&table) else {
            break;
        };
        let err = match toml::from_str::<T>(&canonical) {
            Ok(value) => return Ok(Repaired { value, ignored }),
            Err(err) => err,
        };
        let (Some(field), Some(span)) = (unknown_field(err.message()), err.span()) else {
            break;
        };
        let mut path = table_path_at(&canonical, span.start);
        let mut removed = table_at(&mut table, &path).and_then(|owner| owner.remove(field)).is_some();
        if !removed {
            while matches!(path.last(), Some(Segment::Index(_))) {
                path.pop();
            }
            if path.last() == Some(&Segment::Key(field.to_string())) {
                path.pop();
                removed = table_at(&mut table, &path).and_then(|owner| owner.remove(field)).is_some();
            }
        }
        if !removed {
            break;
        }
        ignored.push(dotted(&path, field));
    }
    Err(strict)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Default, Deserialize, PartialEq)]
    #[serde(default, deny_unknown_fields)]
    struct Sample {
        general: General,
        binds: Vec<Bind>,
        dock: Dock,
    }

    #[derive(Debug, Default, Deserialize, PartialEq)]
    #[serde(default, deny_unknown_fields)]
    struct General {
        name: String,
        size: u32,
    }

    #[derive(Debug, Default, Deserialize, PartialEq)]
    #[serde(default, deny_unknown_fields)]
    struct Bind {
        key: String,
        action: Action,
    }

    #[derive(Debug, Default, Deserialize, PartialEq)]
    #[serde(default, deny_unknown_fields)]
    struct Action {
        exec: String,
    }

    #[derive(Debug, Default, Deserialize, PartialEq)]
    #[serde(default, deny_unknown_fields)]
    struct Dock {
        theme: Theme,
    }

    #[derive(Debug, Default, Deserialize, PartialEq)]
    #[serde(default, deny_unknown_fields)]
    struct Theme {
        size: u32,
    }

    #[test]
    fn a_clean_file_is_parsed_strictly_with_nothing_ignored() {
        let parsed = parse::<Sample>("[general]\nname = \"a\"\nsize = 2\n").expect("valid");
        assert_eq!(parsed.value.general, General { name: "a".into(), size: 2 });
        assert!(parsed.ignored.is_empty());
    }

    #[test]
    fn unknown_keys_are_dropped_one_by_one_and_the_rest_of_the_file_survives() {
        let text = "future = true\n\n[general]\nname = \"a\"\nsize = 2 # kept\nfrobnicate = 1\n\n\
                    [dock.theme]\nsize = 48\nglow = \"yes\"\n\n[newsection]\nx = 1\n";
        let parsed = parse::<Sample>(text).expect("repairable");
        assert_eq!(parsed.value.general, General { name: "a".into(), size: 2 });
        assert_eq!(parsed.value.dock.theme.size, 48);
        let mut ignored = parsed.ignored;
        ignored.sort();
        assert_eq!(ignored, ["dock.theme.glow", "future", "general.frobnicate", "newsection"]);
    }

    #[test]
    fn an_unknown_key_is_removed_from_the_right_array_element_and_inline_table() {
        let text = "[[binds]]\nkey = \"a\"\naction = { exec = \"one\" }\n\n\
                    [[binds]]\nkey = \"b\"\nextra = 1\naction = { exec = \"two\", exce = \"typo\" }\n";
        let parsed = parse::<Sample>(text).expect("repairable");
        let keys: Vec<(&str, &str)> =
            parsed.value.binds.iter().map(|bind| (bind.key.as_str(), bind.action.exec.as_str())).collect();
        assert_eq!(keys, [("a", "one"), ("b", "two")]);
        let mut ignored = parsed.ignored;
        ignored.sort();
        assert_eq!(ignored, ["binds[1].action.exce", "binds[1].extra"]);
    }

    #[test]
    fn a_wrong_value_or_broken_syntax_is_still_an_error_with_the_original_message() {
        let wrong_type = parse::<Sample>("[general]\nsize = \"big\"\nfuture = 1\n").expect_err("not repairable");
        let strict = toml::from_str::<Sample>("[general]\nsize = \"big\"\nfuture = 1\n").expect_err("strict");
        assert_eq!(wrong_type.message(), strict.message());
        assert!(parse::<Sample>("[general\nname = 1\n").is_err());
    }

    #[test]
    fn the_unknown_field_name_is_read_from_the_serde_message() {
        assert_eq!(unknown_field("unknown field `glow`, expected `size`"), Some("glow"));
        assert_eq!(unknown_field("unknown field `x`, there are no fields"), Some("x"));
        assert_eq!(unknown_field("invalid type: string \"big\", expected u32"), None);
    }
}

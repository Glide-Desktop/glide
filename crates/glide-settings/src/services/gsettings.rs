use std::collections::HashMap;

use super::run;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Snapshot {
    values: HashMap<(String, String), String>,
}

impl Snapshot {
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let mut values = HashMap::new();
        for line in text.lines() {
            let mut parts = line.splitn(3, ' ');
            let (Some(schema), Some(key), Some(value)) = (parts.next(), parts.next(), parts.next()) else {
                continue;
            };
            values.insert((schema.to_string(), key.to_string()), value.trim().to_string());
        }
        Self { values }
    }

    #[must_use]
    pub fn read(schemas: &[&str]) -> Self {
        let mut snapshot = Self::default();
        for schema in schemas {
            if let Some(text) = run::output("gsettings", &["list-recursively", schema]) {
                snapshot.values.extend(Self::parse(&text).values);
            }
        }
        snapshot
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    #[must_use]
    pub fn has_schema(&self, schema: &str) -> bool {
        self.values.keys().any(|(name, _)| name == schema)
    }

    #[must_use]
    pub fn raw(&self, schema: &str, key: &str) -> Option<&str> {
        self.values.get(&(schema.to_string(), key.to_string())).map(String::as_str)
    }

    #[must_use]
    pub fn bool(&self, schema: &str, key: &str) -> Option<bool> {
        self.raw(schema, key).and_then(parse_bool)
    }

    #[must_use]
    pub fn int(&self, schema: &str, key: &str) -> Option<i64> {
        self.raw(schema, key).and_then(parse_int)
    }

    #[must_use]
    pub fn double(&self, schema: &str, key: &str) -> Option<f64> {
        self.raw(schema, key).and_then(parse_double)
    }

    #[must_use]
    pub fn string(&self, schema: &str, key: &str) -> Option<String> {
        self.raw(schema, key).and_then(parse_string)
    }

    #[must_use]
    pub fn strings(&self, schema: &str, key: &str) -> Vec<String> {
        self.raw(schema, key).map(parse_string_array).unwrap_or_default()
    }

    pub fn insert(&mut self, schema: &str, key: &str, value: &str) {
        self.values.insert((schema.to_string(), key.to_string()), value.to_string());
    }
}

fn strip_type(text: &str) -> &str {
    let text = text.trim();
    for prefix in ["uint32 ", "int32 ", "uint64 ", "int64 ", "uint16 ", "int16 ", "byte ", "double "] {
        if let Some(rest) = text.strip_prefix(prefix) {
            return rest.trim();
        }
    }
    text
}

#[must_use]
pub fn parse_bool(text: &str) -> Option<bool> {
    match text.trim() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

#[must_use]
pub fn parse_int(text: &str) -> Option<i64> {
    strip_type(text).parse().ok()
}

#[must_use]
pub fn parse_double(text: &str) -> Option<f64> {
    let value: f64 = strip_type(text).parse().ok()?;
    value.is_finite().then_some(value)
}

fn read_quoted(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Option<String> {
    let quote = chars.next().filter(|c| *c == '\'' || *c == '"')?;
    let mut out = String::new();
    while let Some(c) = chars.next() {
        match c {
            '\\' => out.push(chars.next()?),
            c if c == quote => return Some(out),
            c => out.push(c),
        }
    }
    None
}

#[must_use]
pub fn parse_string(text: &str) -> Option<String> {
    let mut chars = text.trim().chars().peekable();
    let value = read_quoted(&mut chars)?;
    chars.next().is_none().then_some(value)
}

#[must_use]
pub fn parse_string_array(text: &str) -> Vec<String> {
    let text = text.trim();
    let text = text.strip_prefix("@as").map_or(text, str::trim);
    let Some(inner) = text.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')) else {
        return Vec::new();
    };
    let mut chars = inner.chars().peekable();
    let mut items = Vec::new();
    loop {
        while chars.peek().is_some_and(|c| c.is_whitespace() || *c == ',') {
            chars.next();
        }
        if chars.peek().is_none() {
            break;
        }
        match read_quoted(&mut chars) {
            Some(item) => items.push(item),
            None => break,
        }
    }
    items
}

#[must_use]
pub fn quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('\'');
    for c in text.chars() {
        if c == '\'' || c == '\\' {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('\'');
    out
}

#[must_use]
pub fn string_array(values: &[String]) -> String {
    if values.is_empty() {
        return "@as []".to_string();
    }
    let items: Vec<String> = values.iter().map(|value| quote(value)).collect();
    format!("[{}]", items.join(", "))
}

pub fn set(schema: &str, key: &str, value: &str) -> Result<String, String> {
    run::status("gsettings", &["set", schema, key, value])
}

pub fn set_bool(schema: &str, key: &str, value: bool) -> Result<String, String> {
    set(schema, key, if value { "true" } else { "false" })
}

pub fn set_string(schema: &str, key: &str, value: &str) -> Result<String, String> {
    set(schema, key, &quote(value))
}

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub const REQUEST_ENV: &str = "GLIDE_CHOOSER_REQUEST";
pub const FLAG: &str = "--chooser";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    #[default]
    Open,
    Save,
    SaveFiles,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Pattern {
    Glob(String),
    Mime(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Filter {
    pub name: String,
    pub patterns: Vec<Pattern>,
}

impl Filter {
    #[must_use]
    pub fn from_portal(name: String, patterns: Vec<(u32, String)>) -> Self {
        let patterns = patterns
            .into_iter()
            .filter_map(|(kind, pattern)| match kind {
                0 => Some(Pattern::Glob(pattern)),
                1 => Some(Pattern::Mime(pattern)),
                _ => None,
            })
            .collect();
        Self { name, patterns }
    }

    #[must_use]
    pub fn to_portal(&self) -> (String, Vec<(u32, String)>) {
        let patterns = self
            .patterns
            .iter()
            .map(|pattern| match pattern {
                Pattern::Glob(glob) => (0, glob.clone()),
                Pattern::Mime(mime) => (1, mime.clone()),
            })
            .collect();
        (self.name.clone(), patterns)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Choice {
    pub id: String,
    pub label: String,
    pub options: Vec<(String, String)>,
    pub default: String,
}

impl Choice {
    #[must_use]
    pub fn is_toggle(&self) -> bool {
        self.options.is_empty()
    }

    #[must_use]
    pub fn initial(&self) -> String {
        if self.is_toggle() {
            return if self.default == "true" { "true" } else { "false" }.to_string();
        }
        if self.options.iter().any(|(id, _)| *id == self.default) {
            return self.default.clone();
        }
        self.options.first().map(|(id, _)| id.clone()).unwrap_or_default()
    }

    #[must_use]
    pub fn next(&self, current: &str) -> String {
        if self.is_toggle() {
            return if current == "true" { "false" } else { "true" }.to_string();
        }
        let at = self.options.iter().position(|(id, _)| id == current).map_or(0, |index| index + 1);
        self.options.get(at % self.options.len().max(1)).map(|(id, _)| id.clone()).unwrap_or_default()
    }

    #[must_use]
    pub fn display(&self, current: &str) -> String {
        if self.is_toggle() {
            let mark = if current == "true" { "✓" } else { "○" };
            return format!("{mark} {}", self.label);
        }
        let option = self.options.iter().find(|(id, _)| id == current).map_or(current, |(_, label)| label.as_str());
        format!("{}: {option}", self.label)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Request {
    #[serde(default)]
    pub choices: Vec<Choice>,
    pub mode: Mode,
    pub title: String,
    pub accept_label: Option<String>,
    pub multiple: bool,
    pub directory: bool,
    pub filters: Vec<Filter>,
    pub current_filter: Option<usize>,
    pub current_name: Option<String>,
    pub current_folder: Option<PathBuf>,
    pub files: Vec<String>,
    pub parent: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Response {
    pub uris: Vec<String>,
    pub filter: Option<usize>,
    #[serde(default)]
    pub choices: Vec<(String, String)>,
}

impl Request {
    #[must_use]
    pub fn from_env() -> Option<Self> {
        serde_json::from_str(&std::env::var(REQUEST_ENV).ok()?).ok()
    }

    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    #[must_use]
    pub fn wayland_parent(&self) -> Option<&str> {
        self.parent.as_deref()?.strip_prefix("wayland:").filter(|handle| !handle.is_empty())
    }
}

impl Response {
    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        text.lines().rev().find_map(|line| serde_json::from_str(line.trim()).ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_survives_the_trip_through_the_environment_format() {
        let request = Request {
            mode: Mode::Save,
            title: "Save Image".into(),
            filters: vec![Filter::from_portal("Images".into(), vec![(0, "*.png".into()), (1, "image/jpeg".into())])],
            current_name: Some("shot.png".into()),
            parent: Some("wayland:abc".into()),
            ..Request::default()
        };
        let back: Request = serde_json::from_str(&request.to_json()).expect("json");
        assert_eq!(back, request);
        assert_eq!(back.wayland_parent(), Some("abc"));
        assert_eq!(back.filters[0].to_portal(), ("Images".into(), vec![(0, "*.png".into()), (1, "image/jpeg".into())]));
    }

    #[test]
    fn the_response_is_the_last_json_line_on_stdout() {
        let response = Response { uris: vec!["file:///tmp/a".into()], filter: Some(1), choices: Vec::new() };
        let stdout = format!("some log line\n{}\n", response.to_json());
        assert_eq!(Response::parse(&stdout), Some(response));
        assert_eq!(Response::parse("nothing here"), None);
    }

    #[test]
    fn toggles_flip_and_lists_cycle_through_their_options() {
        let toggle = Choice { id: "ro".into(), label: "Read only".into(), options: Vec::new(), default: String::new() };
        assert_eq!(toggle.initial(), "false");
        assert_eq!(toggle.next("false"), "true");
        assert_eq!(toggle.display("true"), "✓ Read only");
        let list = Choice {
            id: "enc".into(),
            label: "Encoding".into(),
            options: vec![("utf8".into(), "UTF-8".into()), ("latin1".into(), "Latin-1".into())],
            default: "missing".into(),
        };
        assert_eq!(list.initial(), "utf8", "an unknown default falls back to the first option");
        assert_eq!(list.next("utf8"), "latin1");
        assert_eq!(list.next("latin1"), "utf8");
        assert_eq!(list.display("latin1"), "Encoding: Latin-1");
    }

    #[test]
    fn unknown_portal_pattern_kinds_are_dropped() {
        let filter = Filter::from_portal("X".into(), vec![(7, "?".into())]);
        assert!(filter.patterns.is_empty());
    }
}

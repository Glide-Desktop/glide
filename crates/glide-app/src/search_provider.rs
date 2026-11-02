use std::collections::HashMap;

use zbus::zvariant::Value;

pub const INTERFACE: &str = "org.gnome.Shell.SearchProvider2";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub id: String,
    pub name: String,
    pub description: String,
}

pub trait Source: Send + Sync + 'static {
    fn search(&self, terms: &[String]) -> Vec<Item>;
    fn activate(&self, id: &str);
}

#[must_use]
pub fn matches(haystack: &str, terms: &[String]) -> bool {
    let haystack = haystack.to_lowercase();
    !terms.is_empty() && terms.iter().all(|term| haystack.contains(&term.to_lowercase()))
}

const REMEMBERED: usize = 256;

pub struct Provider {
    source: Box<dyn Source>,
    known: std::sync::Mutex<Vec<Item>>,
}

impl Provider {
    #[must_use]
    pub fn new(source: impl Source) -> Self {
        Self { source: Box::new(source), known: std::sync::Mutex::default() }
    }

    fn run(&self, terms: &[String]) -> Vec<String> {
        let items = self.source.search(terms);
        let ids = items.iter().map(|item| item.id.clone()).collect();
        remember(&mut self.known.lock().unwrap_or_else(std::sync::PoisonError::into_inner), items);
        ids
    }
}

fn remember(known: &mut Vec<Item>, items: Vec<Item>) {
    let fresh: Vec<Item> = items.into_iter().take(REMEMBERED).collect();
    known.retain(|item| !fresh.iter().any(|new| new.id == item.id));
    known.extend(fresh);
    let excess = known.len().saturating_sub(REMEMBERED);
    known.drain(..excess);
}

// Signatures are fixed by org.gnome.Shell.SearchProvider2.
#[allow(clippy::needless_pass_by_value, clippy::unused_self)]
#[zbus::interface(name = "org.gnome.Shell.SearchProvider2")]
impl Provider {
    fn get_initial_result_set(&self, terms: Vec<String>) -> Vec<String> {
        self.run(&terms)
    }

    fn get_subsearch_result_set(&self, previous: Vec<String>, terms: Vec<String>) -> Vec<String> {
        drop(previous);
        self.run(&terms)
    }

    fn get_result_metas(&self, identifiers: Vec<String>) -> Vec<HashMap<String, Value<'static>>> {
        let known = self.known.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        identifiers
            .iter()
            .filter_map(|id| known.iter().rev().find(|item| &item.id == id))
            .map(|item| {
                HashMap::from([
                    ("id".to_string(), Value::from(item.id.clone())),
                    ("name".to_string(), Value::from(item.name.clone())),
                    ("description".to_string(), Value::from(item.description.clone())),
                ])
            })
            .collect()
    }

    fn activate_result(&self, identifier: String, terms: Vec<String>, timestamp: u32) {
        drop((terms, timestamp));
        self.source.activate(&identifier);
    }

    fn launch_search(&self, terms: Vec<String>, timestamp: u32) {
        drop((terms, timestamp));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_term_must_appear() {
        let terms = vec!["doc".to_string(), "proj".to_string()];
        assert!(matches("Documents/Projects", &terms));
        assert!(!matches("Documents", &terms));
        assert!(!matches("anything", &[]));
    }

    fn item(id: &str, name: &str) -> Item {
        Item { id: id.into(), name: name.into(), description: String::new() }
    }

    #[test]
    fn metas_answer_for_earlier_searches_and_the_memory_stays_bounded() {
        let mut known = Vec::new();
        remember(&mut known, vec![item("a", "Alpha"), item("b", "Beta")]);
        remember(&mut known, vec![item("b", "Beta 2"), item("c", "Gamma")]);
        let names: Vec<_> = known.iter().map(|item| (item.id.as_str(), item.name.as_str())).collect();
        assert_eq!(names, [("a", "Alpha"), ("b", "Beta 2"), ("c", "Gamma")]);
        remember(&mut known, (0..REMEMBERED + 10).map(|n| item(&n.to_string(), "")).collect());
        assert_eq!(known.len(), REMEMBERED);
        assert_eq!(known.first().map(|item| item.id.as_str()), Some("0"));
    }
}

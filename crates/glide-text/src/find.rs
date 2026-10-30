use std::ops::Range;

use regex::Regex;

pub const LIMIT: usize = 100_000;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Query {
    pub text: String,
    pub case: bool,
    pub word: bool,
    pub regex: bool,
}

impl Query {
    pub fn compile(&self) -> Result<Option<Regex>, String> {
        if self.text.is_empty() {
            return Ok(None);
        }
        let body = if self.regex { self.text.clone() } else { regex::escape(&self.text) };
        let body = if self.word { format!(r"\b(?:{body})\b") } else { body };
        let flags = if self.case { "(?m)" } else { "(?mi)" };
        regex::RegexBuilder::new(&format!("{flags}{body}"))
            .size_limit(1 << 22)
            .build()
            .map(Some)
            .map_err(|error| error.to_string().lines().last().unwrap_or_default().trim().to_string())
    }
}

#[must_use]
pub fn matches(pattern: &Regex, text: &str) -> Vec<Range<usize>> {
    pattern.find_iter(text).filter(|found| !found.is_empty()).take(LIMIT).map(|found| found.range()).collect()
}

#[must_use]
pub fn next(found: &[Range<usize>], from: usize, forward: bool) -> Option<usize> {
    if found.is_empty() {
        return None;
    }
    if forward {
        Some(found.iter().position(|range| range.start >= from).unwrap_or(0))
    } else {
        Some(found.iter().rposition(|range| range.end <= from).unwrap_or(found.len() - 1))
    }
}

#[must_use]
pub fn replacement(pattern: &Regex, query: &Query, text: &str, found: &Range<usize>, with: &str) -> String {
    if !query.regex {
        return with.to_string();
    }
    let mut out = String::new();
    match pattern
        .captures_at(text, found.start)
        .filter(|captures| captures.get(0).map(|all| all.range()) == Some(found.clone()))
    {
        Some(captures) => captures.expand(with, &mut out),
        None => out.push_str(with),
    }
    out
}

#[must_use]
pub(crate) fn replace_all(pattern: &Regex, query: &Query, text: &str, with: &str) -> (String, usize) {
    let mut out = String::with_capacity(text.len());
    let (mut last, mut count) = (0, 0);
    for captures in pattern.captures_iter(text) {
        let Some(all) = captures.get(0).filter(|all| !all.is_empty()) else { continue };
        out.push_str(&text[last..all.start()]);
        if query.regex {
            captures.expand(with, &mut out);
        } else {
            out.push_str(with);
        }
        last = all.end();
        count += 1;
    }
    out.push_str(&text[last..]);
    (out, count)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(text: &str) -> Query {
        Query { text: text.into(), ..Query::default() }
    }

    #[test]
    fn literal_queries_escape_and_ignore_case_by_default() {
        let pattern = query("a.b").compile().unwrap().unwrap();
        assert_eq!(matches(&pattern, "A.B axb a.b"), vec![0..3, 8..11]);
        let exact = Query { case: true, ..query("Слово") }.compile().unwrap().unwrap();
        assert_eq!(matches(&exact, "слово Слово").len(), 1);
        assert!(matches!(query("").compile(), Ok(None)));
    }

    #[test]
    fn whole_words_and_regex_work_together() {
        let pattern = Query { word: true, ..query("cat") }.compile().unwrap().unwrap();
        assert_eq!(matches(&pattern, "cat concat cat."), vec![0..3, 11..14]);
        let regex = Query { regex: true, ..query(r"(\d+)-(\d+)") };
        let pattern = regex.compile().unwrap().unwrap();
        assert_eq!(replacement(&pattern, &regex, "1-2", &(0..3), "$2-$1"), "2-1");
        let context = Query { text: r"\Ba".into(), regex: true, ..Query::default() };
        let anchored = context.compile().unwrap().unwrap();
        assert_eq!(replacement(&anchored, &context, "ba", &(1..2), "X"), "X");
        let empty = Query { text: "x*".into(), regex: true, ..Query::default() };
        let stars = empty.compile().unwrap().unwrap();
        assert_eq!(replace_all(&stars, &empty, "ax", "-"), (String::from("a-"), 1));
        assert!(Query { regex: true, ..query("(") }.compile().is_err());
    }

    #[test]
    fn stepping_wraps_around_both_ways() {
        let found = vec![0..1, 5..6, 9..10];
        assert_eq!(next(&found, 2, true), Some(1));
        assert_eq!(next(&found, 10, true), Some(0));
        assert_eq!(next(&found, 5, false), Some(0));
        assert_eq!(next(&found, 0, false), Some(2));
        assert_eq!(next(&[], 0, true), None);
    }

    #[test]
    fn replace_all_keeps_dollars_literal_unless_regex() {
        let literal = query("x");
        let pattern = literal.compile().unwrap().unwrap();
        assert_eq!(replace_all(&pattern, &literal, "x y x", "$1"), ("$1 y $1".into(), 2));
        let regex = Query { regex: true, ..query("(x)") };
        let pattern = regex.compile().unwrap().unwrap();
        assert_eq!(replace_all(&pattern, &regex, "x y", "<$1>"), ("<x> y".into(), 1));
    }
}

use std::collections::HashSet;
use std::ffi::OsStr;
use std::fmt::Write;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

use super::operations::{occupied, same_file, unique_path, validate_name};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rule {
    Template(String),
    Replace { find: String, with: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Problem {
    Empty,
    Invalid,
    Repeats,
    Taken,
    Unchanged,
}

pub const NAME_TOKEN: &str = "[N]";

#[must_use]
pub fn split_name(name: &str, is_dir: bool) -> (&str, &str) {
    const COMPOUND: [&str; 8] =
        [".tar.gz", ".tar.xz", ".tar.bz2", ".tar.zst", ".tar.lz", ".tar.lzma", ".tar.lz4", ".tar.Z"];
    if is_dir {
        return (name, "");
    }
    if let Some(ext) = COMPOUND.iter().find(|ext| name.len() > ext.len() && name.ends_with(*ext)) {
        return name.split_at(name.len() - ext.len());
    }
    match name.rfind('.') {
        Some(dot) if dot > 0 && dot + 1 < name.len() => name.split_at(dot),
        _ => (name, ""),
    }
}

fn counter(token: &str) -> Option<(u64, usize)> {
    let digits = token.strip_prefix('[')?.strip_suffix(']')?;
    (!digits.is_empty() && digits.len() <= 9 && digits.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| digits.parse().ok().map(|start| (start, digits.len())))
        .flatten()
}

fn fill(template: &str, stem: &str, index: u64) -> String {
    let mut out = String::with_capacity(template.len() + stem.len());
    let mut rest = template;
    while let Some(open) = rest.find('[') {
        out.push_str(&rest[..open]);
        let tail = &rest[open..];
        let Some(close) = tail.find(']') else {
            out.push_str(tail);
            return out;
        };
        let token = &tail[..=close];
        if token.eq_ignore_ascii_case(NAME_TOKEN) {
            out.push_str(stem);
        } else if let Some((start, width)) = counter(token) {
            let _ = write!(out, "{:0width$}", start.saturating_add(index));
        } else {
            out.push_str(token);
        }
        rest = &tail[close + 1..];
    }
    out.push_str(rest);
    out
}

#[must_use]
pub fn counts(template: &str) -> bool {
    let mut rest = template;
    while let Some(open) = rest.find('[') {
        let tail = &rest[open..];
        let Some(close) = tail.find(']') else {
            return false;
        };
        if counter(&tail[..=close]).is_some() {
            return true;
        }
        rest = &tail[close + 1..];
    }
    false
}

#[must_use]
pub fn renamed(name: &str, is_dir: bool, rule: &Rule, index: u64) -> String {
    match rule {
        Rule::Template(template) => {
            let (stem, extension) = split_name(name, is_dir);
            format!("{}{extension}", fill(template.trim(), stem, index))
        }
        Rule::Replace { find, .. } if find.is_empty() => name.to_string(),
        Rule::Replace { find, with } => name.replace(find.as_str(), with),
    }
}

#[must_use]
pub fn plan(items: &[(PathBuf, bool)], rule: &Rule) -> Vec<(PathBuf, PathBuf)> {
    items
        .iter()
        .enumerate()
        .filter_map(|(index, (path, is_dir))| {
            let name = path.file_name()?.to_string_lossy().into_owned();
            let fresh = renamed(&name, *is_dir, rule, index as u64);
            Some((path.clone(), path.with_file_name(fresh)))
        })
        .collect()
}

#[must_use]
pub fn problem(plan: &[(PathBuf, PathBuf)], rule: &Rule, taken: impl Fn(&Path) -> bool) -> Option<Problem> {
    let blank = match rule {
        Rule::Template(template) => template.trim().is_empty(),
        Rule::Replace { find, .. } => find.is_empty(),
    };
    if blank || plan.is_empty() {
        return Some(Problem::Empty);
    }
    let named = |path: &Path| path.file_name().map(|name| name.to_string_lossy().into_owned());
    if plan.iter().any(|(from, to)| {
        to.parent() != from.parent()
            || named(to).is_none_or(|name| validate_name(&name).is_err() || name.trim() != name)
    }) {
        return Some(Problem::Invalid);
    }
    let sources: HashSet<&Path> = plan.iter().map(|(from, _)| from.as_path()).collect();
    let mut seen = HashSet::new();
    if plan.iter().any(|(_, to)| !seen.insert(to.as_path())) {
        return Some(Problem::Repeats);
    }
    if plan.iter().any(|(from, to)| from != to && !sources.contains(to.as_path()) && taken(to) && !same_file(from, to))
    {
        return Some(Problem::Taken);
    }
    plan.iter().all(|(from, to)| from == to).then_some(Problem::Unchanged)
}

pub fn apply(plan: &[(PathBuf, PathBuf)]) -> (Vec<(PathBuf, PathBuf)>, Option<io::Error>) {
    let moves: Vec<&(PathBuf, PathBuf)> = plan.iter().filter(|(from, to)| from != to).collect();
    let sources: HashSet<&Path> = moves.iter().map(|(from, _)| from.as_path()).collect();
    let crossing = moves.iter().any(|(_, to)| sources.contains(to.as_path()));
    let mut staged: Vec<(PathBuf, &(PathBuf, PathBuf))> = Vec::with_capacity(moves.len());
    for pair in moves {
        let (from, _) = pair;
        if !crossing {
            staged.push((from.clone(), pair));
            continue;
        }
        let (Some(dir), Some(name)) = (from.parent(), from.file_name()) else {
            continue;
        };
        let mut hidden = std::ffi::OsString::from(".");
        hidden.push(name);
        hidden.push(".renaming");
        let aside = unique_path(dir, OsStr::new(&hidden), false, occupied);
        if let Err(err) = std::fs::rename(from, &aside) {
            for (aside, (from, _)) in staged.iter().rev() {
                let _ = std::fs::rename(aside, from);
            }
            return (Vec::new(), Some(err));
        }
        staged.push((aside, pair));
    }
    let mut done = Vec::with_capacity(staged.len());
    let mut failure = None;
    for (held, (from, to)) in staged {
        if failure.is_some() {
            if held != *from {
                let _ = std::fs::rename(&held, from);
            }
            continue;
        }
        let placed = if occupied(to) && !same_file(&held, to) {
            Err(io::Error::new(ErrorKind::AlreadyExists, "name taken"))
        } else {
            std::fs::rename(&held, to)
        };
        match placed {
            Ok(()) => done.push((from.clone(), to.clone())),
            Err(err) => {
                if held != *from {
                    let _ = std::fs::rename(&held, from);
                }
                failure = Some(err);
            }
        }
    }
    (done, failure)
}

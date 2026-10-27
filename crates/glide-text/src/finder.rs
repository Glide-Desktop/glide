use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

use glide_sdk::anim::Reveal;
use glide_sdk::overlay::FinderRow;
use glide_sdk::theme::motion;
use regex::Regex;

use crate::find::Query;
use crate::syntax::Language;

pub const MAX_FILES: usize = 20_000;
pub const MAX_RESULTS: usize = 400;
pub const MAX_BYTES: u64 = 2 * 1024 * 1024;
const SKIPPED: [&str; 3] = [".git", "node_modules", "target"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Files,
    Symbols,
    Search,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    File(PathBuf),
    Line(usize),
    Match(PathBuf, usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub label: String,
    pub detail: String,
    pub target: Target,
}

#[derive(Debug, Clone)]
pub struct Finder {
    pub mode: Mode,
    pub query: String,
    pub entries: Vec<Entry>,
    pub hits: Vec<usize>,
    pub selected: usize,
    pub reveal: Reveal,
    pub root: Option<PathBuf>,
    pub busy: bool,
    pub generation: u64,
}

impl Finder {
    #[must_use]
    pub fn new(mode: Mode, root: Option<PathBuf>, generation: u64) -> Self {
        let mut reveal = Reveal::closed().with_ease(motion::ease_out);
        reveal.open(motion::t2());
        Self {
            mode,
            query: String::new(),
            entries: Vec::new(),
            hits: Vec::new(),
            selected: 0,
            reveal,
            root,
            busy: mode != Mode::Symbols,
            generation,
        }
    }

    pub fn fill(&mut self, entries: Vec<Entry>) {
        self.entries = entries;
        self.busy = false;
        self.rank();
    }

    pub fn rank(&mut self) {
        self.selected = 0;
        if self.mode == Mode::Search {
            self.hits = (0..self.entries.len()).collect();
            return;
        }
        let mut scored: Vec<(i32, usize)> = self
            .entries
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                let by_name = fuzzy(&self.query, &entry.label).map(|score| score + 100);
                let by_path = || fuzzy(&self.query, &format!("{}/{}", entry.detail, entry.label));
                Some((by_name.or_else(by_path)?, index))
            })
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        self.hits = scored.into_iter().map(|(_, index)| index).take(200).collect();
    }

    pub fn step(&mut self, forward: bool) {
        let count = self.hits.len();
        if count == 0 {
            return;
        }
        self.selected = if forward { (self.selected + 1) % count } else { (self.selected + count - 1) % count };
    }

    #[must_use]
    pub fn current(&self) -> Option<&Entry> {
        self.hits.get(self.selected).and_then(|index| self.entries.get(*index))
    }

    #[must_use]
    pub fn rows(&self) -> Vec<FinderRow> {
        self.hits
            .iter()
            .filter_map(|index| self.entries.get(*index))
            .map(|entry| FinderRow { label: entry.label.clone(), detail: entry.detail.clone() })
            .collect()
    }
}

#[must_use]
pub fn fuzzy(query: &str, text: &str) -> Option<i32> {
    let wanted: Vec<char> = query.chars().filter(|c| !c.is_whitespace()).flat_map(char::to_lowercase).collect();
    if wanted.is_empty() {
        return Some(0);
    }
    let chars: Vec<char> = text.chars().collect();
    let (mut at, mut score, mut previous) = (0, 0, None);
    for (index, c) in chars.iter().enumerate() {
        if at < wanted.len() && c.to_lowercase().next() == Some(wanted[at]) {
            score += 10;
            if index > 0 && previous == Some(index - 1) {
                score += 15;
            }
            if index == 0 || !chars[index - 1].is_alphanumeric() {
                score += 12;
            }
            previous = Some(index);
            at += 1;
        }
    }
    (at == wanted.len()).then(|| score - i32::try_from(chars.len() / 4).unwrap_or(0))
}

fn visible(name: &str, hidden: bool) -> bool {
    !SKIPPED.contains(&name) && (hidden || !name.starts_with('.'))
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root).unwrap_or(path).to_string_lossy().into_owned()
}

#[must_use]
pub fn index(root: &Path, hidden: bool) -> Vec<Entry> {
    let mut found = Vec::new();
    let mut folders = vec![root.to_path_buf()];
    while let Some(folder) = folders.pop() {
        let Ok(read) = std::fs::read_dir(&folder) else { continue };
        for item in read.filter_map(Result::ok) {
            let name = item.file_name().to_string_lossy().into_owned();
            if !visible(&name, hidden) {
                continue;
            }
            let path = item.path();
            match item.file_type() {
                Ok(kind) if kind.is_dir() => folders.push(path),
                Ok(kind) if kind.is_file() => {
                    let detail = relative(root, path.parent().unwrap_or(root));
                    found.push(Entry { label: name, detail, target: Target::File(path) });
                }
                _ => {}
            }
            if found.len() >= MAX_FILES {
                return found;
            }
        }
    }
    found.sort_by(|a, b| (&a.detail, &a.label).cmp(&(&b.detail, &b.label)));
    found
}

fn readable(path: &Path) -> Option<String> {
    let size = std::fs::metadata(path).ok()?.len();
    if size > MAX_BYTES {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    if bytes[..bytes.len().min(8000)].contains(&0) {
        return None;
    }
    String::from_utf8(bytes).ok()
}

pub fn search(root: &Path, query: &Query, hidden: bool, generation: u64, latest: &AtomicU64) -> Vec<Entry> {
    let Ok(Some(pattern)) = query.compile() else { return Vec::new() };
    let mut found = Vec::new();
    for entry in index(root, hidden) {
        if latest.load(Ordering::Relaxed) != generation || found.len() >= MAX_RESULTS {
            break;
        }
        let Target::File(path) = &entry.target else { continue };
        let Some(text) = readable(path) else { continue };
        for (number, line) in text.lines().enumerate() {
            if pattern.is_match(line) {
                let place = format!("{}:{}", relative(root, path), number + 1);
                found.push(Entry {
                    label: line.trim().chars().take(140).collect(),
                    detail: place,
                    target: Target::Match(path.clone(), number),
                });
                if found.len() >= MAX_RESULTS {
                    break;
                }
            }
        }
    }
    found
}

fn pattern(source: &'static str, cell: &'static OnceLock<Regex>) -> &'static Regex {
    cell.get_or_init(|| Regex::new(source).expect("symbol patterns are literals checked by tests"))
}

struct Rule {
    regex: &'static Regex,
    name: usize,
    kind: Option<usize>,
}

fn rules(language: Language) -> Vec<Rule> {
    macro_rules! rule {
        ($source:expr, $name:expr, $kind:expr) => {{
            static CELL: OnceLock<Regex> = OnceLock::new();
            Rule { regex: pattern($source, &CELL), name: $name, kind: $kind }
        }};
    }
    match language {
        Language::Rust => vec![
            rule!(
                r#"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:(?:async|const|unsafe|extern\s+"[^"]*")\s+)*(fn|struct|enum|trait|mod|type|union)\s+([A-Za-z_]\w*)"#,
                2,
                Some(1)
            ),
            rule!(r"^\s*impl(?:<[^>]*>)?\s+(.+?)\s*(?:\{|where\b|$)", 1, None),
        ],
        Language::Python => vec![rule!(r"^\s*(?:async\s+)?(def|class)\s+(\w+)", 2, Some(1))],
        Language::JavaScript | Language::TypeScript | Language::Tsx => vec![
            rule!(
                r"^\s*(?:export\s+)?(?:default\s+)?(?:async\s+)?(function\*?|class|interface|type|enum)\s+([A-Za-z_$][\w$]*)",
                2,
                Some(1)
            ),
            rule!(
                r"^\s*(?:export\s+)?(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*=\s*(?:async\s*)?(?:\([^)]*\)|[A-Za-z_$][\w$]*)\s*=>",
                1,
                None
            ),
        ],
        Language::Go => vec![
            rule!(r"^func\s+(?:\([^)]*\)\s*)?(\w+)", 1, None),
            rule!(r"^type\s+(\w+)\s+(struct|interface)", 1, Some(2)),
        ],
        Language::Java => vec![rule!(
            r"^\s*(?:(?:public|private|protected|static|final|abstract)\s+)*(class|interface|enum|record)\s+(\w+)",
            2,
            Some(1)
        )],
        Language::C | Language::Cpp => vec![
            rule!(r"^\s*(struct|class|enum|union|namespace)\s+(\w+)", 2, Some(1)),
            rule!(r"^[A-Za-z_][\w:<>,\s\*&~]*[\s\*&](\w+(?:::\w+)*)\s*\([^;]*$", 1, None),
        ],
        Language::Toml => vec![rule!(r"^\s*\[\[?([^\]]+)\]\]?", 1, None)],
        Language::Bash => vec![rule!(r"^\s*(?:function\s+)?([A-Za-z_][\w-]*)\s*\(\)\s*\{?", 1, None)],
        Language::Lua => vec![rule!(r"^\s*(?:local\s+)?function\s+([\w.:]+)", 1, None)],
        Language::Css => vec![rule!(r"^([^\s@/}][^{]*?)\s*\{\s*$", 1, None)],
        Language::Markdown | Language::Plain | Language::Json | Language::Yaml | Language::Html | Language::Xml => {
            Vec::new()
        }
    }
}

#[must_use]
pub fn symbols(language: Language, text: &str) -> Vec<Entry> {
    if language == Language::Markdown {
        return headings(text);
    }
    let rules = rules(language);
    text.lines()
        .enumerate()
        .filter_map(|(line, source)| {
            rules.iter().find_map(|rule| {
                let found = rule.regex.captures(source)?;
                let name = found.get(rule.name)?.as_str().trim().to_string();
                let kind = rule.kind.and_then(|group| found.get(group)).map_or("", |kind| kind.as_str());
                Some(Entry { label: name, detail: kind.to_string(), target: Target::Line(line) })
            })
        })
        .collect()
}

fn headings(text: &str) -> Vec<Entry> {
    static CELL: OnceLock<Regex> = OnceLock::new();
    let heading = pattern(r"^ {0,3}(#{1,6})\s+(.*?)\s*#*\s*$", &CELL);
    let mut fenced = false;
    let mut entries = Vec::new();
    for (line, source) in text.lines().enumerate() {
        if source.trim_start().starts_with("```") || source.trim_start().starts_with("~~~") {
            fenced = !fenced;
        } else if !fenced
            && let Some(found) = heading.captures(source)
            && !found[2].is_empty()
        {
            let level = found[1].len();
            entries.push(Entry {
                label: format!("{}{}", "  ".repeat(level - 1), &found[2]),
                detail: format!("H{level}"),
                target: Target::Line(line),
            });
        }
    }
    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(language: Language, text: &str) -> Vec<(String, String)> {
        symbols(language, text).into_iter().map(|entry| (entry.label, entry.detail)).collect()
    }

    fn pair(name: &str, kind: &str) -> (String, String) {
        (name.to_string(), kind.to_string())
    }

    #[test]
    fn every_symbol_pattern_compiles() {
        for language in Language::ALL {
            let _ = rules(language);
        }
    }

    #[test]
    fn rust_items_are_listed_with_their_kind_and_impls_by_type() {
        let text = "pub struct A;\nimpl<T> A<T> {\n    pub(crate) async fn go() {}\n}\nimpl Drop for A {}\nlet fn_x = 1;\nmod m;\n";
        assert_eq!(
            names(Language::Rust, text),
            [pair("A", "struct"), pair("A<T>", ""), pair("go", "fn"), pair("Drop for A", ""), pair("m", "mod")]
        );
    }

    #[test]
    fn python_js_and_go_symbols() {
        assert_eq!(
            names(Language::Python, "class A:\n    async def b(self):\n        pass\n"),
            [pair("A", "class"), pair("b", "def")]
        );
        assert_eq!(
            names(Language::TypeScript, "export default class X {}\nconst f = async (a) => a;\nconst n = 3;\n"),
            [pair("X", "class"), pair("f", "")]
        );
        assert_eq!(
            names(Language::Go, "func (s *S) Run() {}\ntype T struct {\n}\n"),
            [pair("Run", ""), pair("T", "struct")]
        );
    }

    #[test]
    fn markdown_headings_nest_and_skip_fenced_code() {
        let text = "# Title\n\n```sh\n# not a heading\n```\n## Part ##\n####### seven\n";
        assert_eq!(names(Language::Markdown, text), [pair("Title", "H1"), pair("  Part", "H2")]);
    }

    #[test]
    fn languages_without_a_notion_of_symbols_list_none() {
        assert!(names(Language::Plain, "fn a() {}\n").is_empty());
        assert!(names(Language::Json, "{\"a\": 1}\n").is_empty());
    }

    #[test]
    fn fuzzy_matching_wants_the_letters_in_order_and_prefers_tight_word_starts() {
        assert!(fuzzy("xyz", "abc").is_none());
        assert!(fuzzy("", "anything").is_some());
        let tight = fuzzy("chr", "chrome.rs").unwrap();
        let loose = fuzzy("chr", "a_c_h_r_x").unwrap();
        assert!(tight > loose);
        assert!(fuzzy("CHROME", "chrome.rs").is_some(), "case never matters");
        assert!(fuzzy("a b", "ab").is_some(), "spaces in the query are ignored");
    }

    #[test]
    fn files_rank_by_name_before_path_and_the_empty_query_lists_everything() {
        let entry = |label: &str, detail: &str| Entry {
            label: label.into(),
            detail: detail.into(),
            target: Target::File(PathBuf::from(detail).join(label)),
        };
        let mut finder = Finder::new(Mode::Files, None, 1);
        finder.fill(vec![entry("main.rs", "src"), entry("lib.rs", "src/app"), entry("app.rs", "src")]);
        assert_eq!(finder.hits.len(), 3);
        finder.query = "app".into();
        finder.rank();
        let first = finder.current().unwrap();
        assert_eq!(first.label, "app.rs", "a name match beats a folder match");
        assert_eq!(finder.hits.len(), 2);
        finder.step(true);
        assert_eq!(finder.current().unwrap().label, "lib.rs");
        finder.step(true);
        assert_eq!(finder.current().unwrap().label, "app.rs", "the selection wraps");
    }

    #[test]
    fn indexing_and_searching_skip_build_folders_binaries_and_stale_runs() {
        let root = std::env::temp_dir().join(format!("glide-text-finder-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join("target")).unwrap();
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::write(root.join("src/a.txt"), "one\nneedle here\nthree\n").unwrap();
        std::fs::write(root.join("target/b.txt"), "needle\n").unwrap();
        std::fs::write(root.join(".hidden"), "needle\n").unwrap();
        std::fs::write(root.join("blob.bin"), b"needle\0\0").unwrap();
        let files: Vec<String> = index(&root, false).into_iter().map(|entry| entry.label).collect();
        assert_eq!(files, ["blob.bin", "a.txt"]);
        let query = Query { text: "NEEDLE".into(), ..Query::default() };
        let latest = AtomicU64::new(7);
        let found = search(&root, &query, false, 7, &latest);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].detail, "src/a.txt:2");
        assert_eq!(found[0].target, Target::Match(root.join("src/a.txt"), 1));
        latest.store(8, Ordering::Relaxed);
        assert!(search(&root, &query, false, 7, &latest).is_empty(), "a newer query cancels the older run");
        let _ = std::fs::remove_dir_all(&root);
    }
}

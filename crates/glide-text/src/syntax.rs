use std::cell::RefCell;
use std::ops::Range;
use std::sync::{Arc, OnceLock};

use glide_sdk::theme::{TokenColor, Tokens};
use iced::advanced::text::highlighter;
use iced::{Color, Font};
use tree_sitter_highlight::{HighlightConfiguration, HighlightEvent};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    Plain,
    Markdown,
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Tsx,
    Json,
    Toml,
    Yaml,
    Bash,
    C,
    Cpp,
    Go,
    Java,
    Lua,
    Html,
    Css,
    Xml,
}

impl Language {
    pub const ALL: [Self; 19] = [
        Self::Plain,
        Self::Markdown,
        Self::Bash,
        Self::C,
        Self::Cpp,
        Self::Css,
        Self::Go,
        Self::Html,
        Self::Java,
        Self::JavaScript,
        Self::Json,
        Self::Lua,
        Self::Python,
        Self::Rust,
        Self::Toml,
        Self::Tsx,
        Self::TypeScript,
        Self::Xml,
        Self::Yaml,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Plain => "Plain Text",
            Self::Markdown => "Markdown",
            Self::Rust => "Rust",
            Self::Python => "Python",
            Self::JavaScript => "JavaScript",
            Self::TypeScript => "TypeScript",
            Self::Tsx => "TSX",
            Self::Json => "JSON",
            Self::Toml => "TOML",
            Self::Yaml => "YAML",
            Self::Bash => "Shell",
            Self::C => "C",
            Self::Cpp => "C++",
            Self::Go => "Go",
            Self::Java => "Java",
            Self::Lua => "Lua",
            Self::Html => "HTML",
            Self::Css => "CSS",
            Self::Xml => "XML",
        }
    }

    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Plain => "plain",
            Self::Markdown => "markdown",
            Self::Rust => "rust",
            Self::Python => "python",
            Self::JavaScript => "javascript",
            Self::TypeScript => "typescript",
            Self::Tsx => "tsx",
            Self::Json => "json",
            Self::Toml => "toml",
            Self::Yaml => "yaml",
            Self::Bash => "bash",
            Self::C => "c",
            Self::Cpp => "cpp",
            Self::Go => "go",
            Self::Java => "java",
            Self::Lua => "lua",
            Self::Html => "html",
            Self::Css => "css",
            Self::Xml => "xml",
        }
    }

    #[must_use]
    pub fn from_key(key: &str) -> Option<Self> {
        let key = key.trim().to_ascii_lowercase();
        let language = match key.as_str() {
            "plain" | "text" | "txt" | "plaintext" => Self::Plain,
            "markdown" | "md" | "mkd" => Self::Markdown,
            "rust" | "rs" => Self::Rust,
            "python" | "py" | "python3" | "py3" => Self::Python,
            "javascript" | "js" | "jsx" | "mjs" | "cjs" | "node" => Self::JavaScript,
            "typescript" | "ts" | "mts" | "cts" => Self::TypeScript,
            "tsx" => Self::Tsx,
            "json" | "jsonc" | "json5" => Self::Json,
            "toml" => Self::Toml,
            "yaml" | "yml" => Self::Yaml,
            "bash" | "sh" | "shell" | "zsh" | "console" | "shellsession" | "ksh" | "dash" => Self::Bash,
            "c" | "h" => Self::C,
            "cpp" | "c++" | "cxx" | "cc" | "hpp" => Self::Cpp,
            "go" | "golang" => Self::Go,
            "java" => Self::Java,
            "lua" => Self::Lua,
            "html" | "htm" | "xhtml" => Self::Html,
            "css" => Self::Css,
            "xml" | "svg" | "xsl" | "plist" => Self::Xml,
            _ => return None,
        };
        Some(language)
    }

    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Plain => "txt",
            Self::Markdown => "md",
            Self::Rust => "rs",
            Self::Python => "py",
            Self::JavaScript => "js",
            Self::TypeScript => "ts",
            Self::Tsx => "tsx",
            Self::Json => "json",
            Self::Toml => "toml",
            Self::Yaml => "yaml",
            Self::Bash => "sh",
            Self::C => "c",
            Self::Cpp => "cpp",
            Self::Go => "go",
            Self::Java => "java",
            Self::Lua => "lua",
            Self::Html => "html",
            Self::Css => "css",
            Self::Xml => "xml",
        }
    }

    #[must_use]
    pub const fn line_comment(self) -> Option<&'static str> {
        match self {
            Self::Rust
            | Self::JavaScript
            | Self::TypeScript
            | Self::Tsx
            | Self::C
            | Self::Cpp
            | Self::Go
            | Self::Java
            | Self::Json => Some("//"),
            Self::Python | Self::Toml | Self::Yaml | Self::Bash => Some("#"),
            Self::Lua => Some("--"),
            Self::Plain | Self::Markdown | Self::Html | Self::Css | Self::Xml => None,
        }
    }

    #[must_use]
    pub const fn block_comment(self) -> Option<(&'static str, &'static str)> {
        match self {
            Self::Markdown | Self::Html | Self::Xml => Some(("<!--", "-->")),
            Self::Css | Self::C | Self::Cpp | Self::Rust | Self::JavaScript | Self::TypeScript | Self::Tsx => {
                Some(("/*", "*/"))
            }
            Self::Go | Self::Java => Some(("/*", "*/")),
            Self::Lua => Some(("--[[", "]]")),
            Self::Plain | Self::Python | Self::Json | Self::Toml | Self::Yaml | Self::Bash => None,
        }
    }

    #[must_use]
    pub fn grammar(self) -> Option<tree_sitter::Language> {
        let function = match self {
            Self::Plain => return None,
            Self::Markdown => tree_sitter_md::LANGUAGE,
            Self::Rust => tree_sitter_rust::LANGUAGE,
            Self::Python => tree_sitter_python::LANGUAGE,
            Self::JavaScript => tree_sitter_javascript::LANGUAGE,
            Self::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT,
            Self::Tsx => tree_sitter_typescript::LANGUAGE_TSX,
            Self::Json => tree_sitter_json::LANGUAGE,
            Self::Toml => tree_sitter_toml_ng::LANGUAGE,
            Self::Yaml => tree_sitter_yaml::LANGUAGE,
            Self::Bash => tree_sitter_bash::LANGUAGE,
            Self::C => tree_sitter_c::LANGUAGE,
            Self::Cpp => tree_sitter_cpp::LANGUAGE,
            Self::Go => tree_sitter_go::LANGUAGE,
            Self::Java => tree_sitter_java::LANGUAGE,
            Self::Lua => tree_sitter_lua::LANGUAGE,
            Self::Html => tree_sitter_html::LANGUAGE,
            Self::Css => tree_sitter_css::LANGUAGE,
            Self::Xml => tree_sitter_xml::LANGUAGE_XML,
        };
        Some(function.into())
    }

    fn queries(self) -> (String, String) {
        let pair = |highlights: &[&str], injections: &str| (highlights.concat(), injections.to_string());
        match self {
            Self::Plain => (String::new(), String::new()),
            Self::Markdown => (
                tree_sitter_md::HIGHLIGHT_QUERY_BLOCK.replace("  (fenced_code_block)\n", ""),
                tree_sitter_md::INJECTION_QUERY_BLOCK
                    .replace(
                        "(#set! injection.language \"markdown_inline\"))",
                        "(#set! injection.language \"markdown_inline\")\n  (#set! injection.include-children))",
                    )
                    .replace(
                        "(code_fence_content) @injection.content)",
                        "(code_fence_content) @injection.content\n  (#set! injection.include-children))",
                    ),
            ),
            Self::Rust => pair(&[tree_sitter_rust::HIGHLIGHTS_QUERY], tree_sitter_rust::INJECTIONS_QUERY),
            Self::Python => pair(&[tree_sitter_python::HIGHLIGHTS_QUERY], ""),
            Self::JavaScript => pair(
                &[tree_sitter_javascript::JSX_HIGHLIGHT_QUERY, tree_sitter_javascript::HIGHLIGHT_QUERY],
                tree_sitter_javascript::INJECTIONS_QUERY,
            ),
            Self::TypeScript => pair(
                &[tree_sitter_typescript::HIGHLIGHTS_QUERY, tree_sitter_javascript::HIGHLIGHT_QUERY],
                tree_sitter_javascript::INJECTIONS_QUERY,
            ),
            Self::Tsx => pair(
                &[
                    tree_sitter_typescript::HIGHLIGHTS_QUERY,
                    tree_sitter_javascript::JSX_HIGHLIGHT_QUERY,
                    tree_sitter_javascript::HIGHLIGHT_QUERY,
                ],
                tree_sitter_javascript::INJECTIONS_QUERY,
            ),
            Self::Json => pair(&[tree_sitter_json::HIGHLIGHTS_QUERY], ""),
            Self::Toml => pair(&[tree_sitter_toml_ng::HIGHLIGHTS_QUERY], ""),
            Self::Yaml => pair(&[tree_sitter_yaml::HIGHLIGHTS_QUERY], ""),
            Self::Bash => pair(&[tree_sitter_bash::HIGHLIGHT_QUERY], ""),
            Self::C => pair(&[tree_sitter_c::HIGHLIGHT_QUERY], ""),
            Self::Cpp => pair(&[tree_sitter_cpp::HIGHLIGHT_QUERY, tree_sitter_c::HIGHLIGHT_QUERY], ""),
            Self::Go => pair(&[tree_sitter_go::HIGHLIGHTS_QUERY], ""),
            Self::Java => pair(&[tree_sitter_java::HIGHLIGHTS_QUERY], ""),
            Self::Lua => pair(&[tree_sitter_lua::HIGHLIGHTS_QUERY], tree_sitter_lua::INJECTIONS_QUERY),
            Self::Html => pair(&[tree_sitter_html::HIGHLIGHTS_QUERY], tree_sitter_html::INJECTIONS_QUERY),
            Self::Css => pair(&[tree_sitter_css::HIGHLIGHTS_QUERY], ""),
            Self::Xml => pair(&[tree_sitter_xml::XML_HIGHLIGHT_QUERY], ""),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tint {
    Comment,
    String,
    Escape,
    Keyword,
    Type,
    Function,
    Constant,
    Property,
    Attribute,
    Tag,
    Punctuation,
    Marker,
    Heading,
    Link,
    Code,
    Reference,
}

impl Tint {
    const COUNT: usize = 16;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Style {
    pub tint: Option<Tint>,
    pub bold: bool,
    pub italic: bool,
}

const fn tint(tint: Tint) -> Style {
    Style { tint: Some(tint), bold: false, italic: false }
}

const fn strong(tint: Tint) -> Style {
    Style { tint: Some(tint), bold: true, italic: false }
}

const PLAIN: Style = Style { tint: None, bold: false, italic: false };

const NAMES: [(&str, Style); 50] = [
    ("attribute", tint(Tint::Attribute)),
    ("boolean", tint(Tint::Constant)),
    ("character", tint(Tint::String)),
    ("comment", Style { tint: Some(Tint::Comment), bold: false, italic: true }),
    ("conditional", strong(Tint::Keyword)),
    ("constant", tint(Tint::Constant)),
    ("constructor", tint(Tint::Type)),
    ("embedded", PLAIN),
    ("escape", tint(Tint::Escape)),
    ("exception", strong(Tint::Keyword)),
    ("float", tint(Tint::Constant)),
    ("function", tint(Tint::Function)),
    ("function.macro", tint(Tint::Keyword)),
    ("include", strong(Tint::Keyword)),
    ("keyword", strong(Tint::Keyword)),
    ("label", tint(Tint::Property)),
    ("module", tint(Tint::Type)),
    ("namespace", tint(Tint::Type)),
    ("number", tint(Tint::Constant)),
    ("operator", tint(Tint::Punctuation)),
    ("preproc", tint(Tint::Keyword)),
    ("property", tint(Tint::Property)),
    ("punctuation", tint(Tint::Punctuation)),
    ("punctuation.special", tint(Tint::Marker)),
    ("repeat", strong(Tint::Keyword)),
    ("string", tint(Tint::String)),
    ("string.escape", tint(Tint::Escape)),
    ("string.special", tint(Tint::Escape)),
    ("tag", tint(Tint::Tag)),
    ("type", strong(Tint::Type)),
    ("variable.builtin", tint(Tint::Constant)),
    ("text.title", strong(Tint::Heading)),
    ("text.literal", tint(Tint::Code)),
    ("text.uri", tint(Tint::Link)),
    ("text.reference", tint(Tint::Reference)),
    ("text.emphasis", Style { tint: None, bold: false, italic: true }),
    ("text.strong", Style { tint: None, bold: true, italic: false }),
    ("markup.heading", strong(Tint::Heading)),
    ("markup.raw", tint(Tint::Code)),
    ("markup.link.url", tint(Tint::Link)),
    ("markup.link", tint(Tint::Reference)),
    ("markup.italic", Style { tint: None, bold: false, italic: true }),
    ("markup.bold", Style { tint: None, bold: true, italic: false }),
    ("markup.list", tint(Tint::Marker)),
    ("markup.quote", Style { tint: Some(Tint::Comment), bold: false, italic: true }),
    ("tag.attribute", tint(Tint::Attribute)),
    ("tag.delimiter", tint(Tint::Punctuation)),
    ("type.builtin", strong(Tint::Type)),
    ("function.builtin", tint(Tint::Function)),
    ("keyword.directive", tint(Tint::Keyword)),
];

const INLINE_KEY: &str = "markdown_inline";

fn build(
    language: tree_sitter::Language, name: &str, highlights: &str, injections: &str,
) -> Option<HighlightConfiguration> {
    let mut config = HighlightConfiguration::new(language, name, highlights, injections, "").ok()?;
    config.configure(&NAMES.map(|(name, _)| name));
    Some(config)
}

fn config(key: &str) -> Option<&'static HighlightConfiguration> {
    static CONFIGS: OnceLock<Vec<(&'static str, Option<HighlightConfiguration>)>> = OnceLock::new();
    let configs = CONFIGS.get_or_init(|| {
        let mut configs: Vec<_> = Language::ALL
            .into_iter()
            .filter_map(|language| {
                let grammar = language.grammar()?;
                let (highlights, injections) = language.queries();
                Some((language.key(), build(grammar, language.key(), &highlights, &injections)))
            })
            .collect();
        configs.push((
            INLINE_KEY,
            build(
                tree_sitter_md::INLINE_LANGUAGE.into(),
                INLINE_KEY,
                tree_sitter_md::HIGHLIGHT_QUERY_INLINE,
                tree_sitter_md::INJECTION_QUERY_INLINE,
            ),
        ));
        configs
    });
    configs.iter().find(|(name, _)| *name == key).and_then(|(_, config)| config.as_ref())
}

fn injected<'a>(name: &str) -> Option<&'a HighlightConfiguration> {
    if name == INLINE_KEY {
        return config(INLINE_KEY);
    }
    config(Language::from_key(name)?.key())
}

#[must_use]
pub fn available(language: Language) -> bool {
    language == Language::Plain || config(language.key()).is_some()
}

pub type Lines = Vec<Vec<(Range<usize>, Style)>>;

fn combine(stack: &[Style]) -> Style {
    let mut style = PLAIN;
    for layer in stack.iter().rev() {
        style.tint = style.tint.or(layer.tint);
        style.bold |= layer.bold;
        style.italic |= layer.italic;
    }
    style
}

#[must_use]
pub fn line_starts(text: &str) -> Vec<usize> {
    std::iter::once(0).chain(text.match_indices('\n').map(|(at, _)| at + 1)).collect()
}

fn push(lines: &mut Lines, starts: &[usize], text: &str, range: Range<usize>, style: Style) {
    let mut start = range.start;
    while start < range.end {
        let line = starts.partition_point(|&at| at <= start).saturating_sub(1);
        let line_start = starts[line];
        let line_end = starts.get(line + 1).map_or(text.len(), |next| next - 1);
        let end = range.end.min(line_end);
        if end > start
            && let Some(spans) = lines.get_mut(line)
        {
            let local = start - line_start..end - line_start;
            match spans.last_mut() {
                Some((last, previous)) if last.end == local.start && *previous == style => last.end = local.end,
                _ => spans.push((local, style)),
            }
        }
        start = line_end.max(end) + 1;
    }
}

#[must_use]
pub fn highlight(language: Language, text: &str) -> Lines {
    thread_local! {
        static HIGHLIGHTER: RefCell<tree_sitter_highlight::Highlighter> =
            RefCell::new(tree_sitter_highlight::Highlighter::new());
    }
    let starts = line_starts(text);
    let mut lines: Lines = vec![Vec::new(); starts.len()];
    let Some(config) = config(language.key()) else { return lines };
    HIGHLIGHTER.with_borrow_mut(|highlighter| {
        let Ok(events) = highlighter.highlight(config, text.as_bytes(), None, None, injected) else {
            return;
        };
        let mut stack = Vec::new();
        for event in events {
            match event {
                Ok(HighlightEvent::HighlightStart(highlight)) => stack.push(NAMES[highlight.0].1),
                Ok(HighlightEvent::HighlightEnd) => {
                    stack.pop();
                }
                Ok(HighlightEvent::Source { start, end }) => {
                    let style = combine(&stack);
                    if style != PLAIN {
                        push(&mut lines, &starts, text, start..end, style);
                    }
                }
                Err(_) => break,
            }
        }
    });
    lines
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    tints: [Color; Tint::COUNT],
    found: Color,
    current: Color,
    font: Font,
}

impl Palette {
    #[must_use]
    pub fn new(tokens: &Tokens, font: Font) -> Self {
        let ansi = tokens.terminal_ansi();
        let color = |color: TokenColor| {
            let [r, g, b, a] = color.0;
            Color { r, g, b, a }
        };
        let tints = [
            color(tokens.fg_3),
            color(ansi[2]),
            color(ansi[6]),
            color(ansi[5]),
            color(ansi[6]),
            color(ansi[4]),
            color(ansi[3]),
            color(ansi[1]),
            color(ansi[3]),
            color(ansi[1]),
            color(tokens.fg_2),
            color(tokens.acc_text),
            color(tokens.acc_text),
            color(ansi[4]),
            color(ansi[6]),
            color(ansi[4]),
        ];
        Self { tints, found: color(tokens.warn), current: color(tokens.acc_text), font }
    }

    fn mark(&self, current: bool) -> Highlight {
        let font = Font { weight: iced::font::Weight::Bold, ..self.font };
        Highlight { color: Some(if current { self.current } else { self.found }), font: Some(font) }
    }

    #[must_use]
    pub fn format(&self, style: Style) -> Highlight {
        let font = (style.bold || style.italic).then_some(Font {
            weight: if style.bold { iced::font::Weight::Bold } else { self.font.weight },
            style: if style.italic { iced::font::Style::Italic } else { self.font.style },
            ..self.font
        });
        Highlight { color: style.tint.map(|tint| self.tints[tint as usize]), font }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Highlight {
    pub color: Option<Color>,
    pub font: Option<Font>,
}

impl Highlight {
    #[must_use]
    pub fn to_format(&self, _: &iced::Theme) -> highlighter::Format<Font> {
        highlighter::Format { color: self.color, font: self.font }
    }
}

pub type Marks = Vec<(usize, Range<usize>, bool)>;

#[must_use]
pub fn marks(text: &str, found: &[Range<usize>], current: Option<usize>) -> Marks {
    let starts = line_starts(text);
    let mut marks = Marks::new();
    for (index, range) in found.iter().enumerate() {
        let mut start = range.start;
        while start < range.end {
            let line = starts.partition_point(|&at| at <= start).saturating_sub(1);
            let line_end = starts.get(line + 1).map_or(text.len(), |next| next - 1);
            let end = range.end.min(line_end);
            if end > start {
                marks.push((line, start - starts[line]..end - starts[line], current == Some(index)));
            }
            start = line_end.max(end) + 1;
        }
    }
    marks
}

#[derive(Debug, Clone)]
pub struct Settings {
    pub lines: Arc<Lines>,
    pub revision: u64,
    pub palette: Palette,
    pub marks: Arc<Marks>,
    pub marked: u64,
}

impl PartialEq for Settings {
    fn eq(&self, other: &Self) -> bool {
        self.revision == other.revision && self.palette == other.palette && self.marked == other.marked
    }
}

pub struct Bridge {
    lines: Arc<Lines>,
    palette: Palette,
    marks: Arc<Marks>,
    marked: u64,
    current: usize,
}

fn first_marked(marks: &Marks) -> usize {
    marks.first().map_or(usize::MAX, |(line, _, _)| *line)
}

fn clamp(line: &str, range: &Range<usize>) -> Option<Range<usize>> {
    let mut start = range.start.min(line.len());
    let mut end = range.end.min(line.len());
    while !line.is_char_boundary(start) {
        start -= 1;
    }
    while !line.is_char_boundary(end) {
        end += 1;
    }
    (end > start).then_some(start..end)
}

impl highlighter::Highlighter for Bridge {
    type Settings = Settings;
    type Highlight = Highlight;
    type Iterator<'a> = std::vec::IntoIter<(Range<usize>, Highlight)>;

    fn new(settings: &Self::Settings) -> Self {
        Self {
            lines: Arc::clone(&settings.lines),
            palette: settings.palette,
            marks: Arc::clone(&settings.marks),
            marked: settings.marked,
            current: 0,
        }
    }

    fn update(&mut self, settings: &Self::Settings) {
        let first = if settings.palette == self.palette {
            self.lines.iter().zip(settings.lines.iter()).position(|(old, new)| old != new).unwrap_or_else(|| {
                if self.lines.len() == settings.lines.len() {
                    usize::MAX
                } else {
                    self.lines.len().min(settings.lines.len())
                }
            })
        } else {
            0
        };
        let first = if settings.marked == self.marked {
            first
        } else {
            first.min(first_marked(&self.marks)).min(first_marked(&settings.marks))
        };
        self.lines = Arc::clone(&settings.lines);
        self.palette = settings.palette;
        self.marks = Arc::clone(&settings.marks);
        self.marked = settings.marked;
        self.current = self.current.min(first);
    }

    fn change_line(&mut self, line: usize) {
        self.current = self.current.min(line);
    }

    fn highlight_line(&mut self, line: &str) -> Self::Iterator<'_> {
        let mut spans: Vec<_> = self
            .lines
            .get(self.current)
            .map(|spans| {
                spans
                    .iter()
                    .filter_map(|(range, style)| Some((clamp(line, range)?, self.palette.format(*style))))
                    .collect()
            })
            .unwrap_or_default();
        let from = self.marks.partition_point(|(index, _, _)| *index < self.current);
        for (_, range, current) in self.marks[from..].iter().take_while(|(index, _, _)| *index == self.current) {
            if let Some(range) = clamp(line, range) {
                spans.push((range, self.palette.mark(*current)));
            }
        }
        self.current += 1;
        spans.into_iter()
    }

    fn current_line(&self) -> usize {
        self.current
    }
}

#[must_use]
pub fn shift(lines: &Lines, at: usize, removed: usize, added: usize) -> Lines {
    let mut shifted = lines.clone();
    let at = at.min(shifted.len());
    let removed = removed.min(shifted.len() - at);
    shifted.splice(at..at + removed, std::iter::repeat_n(Vec::new(), added));
    shifted
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::advanced::text::highlighter::Highlighter as _;

    fn tints(language: Language, text: &str) -> Vec<(usize, String, Option<Tint>)> {
        let lines: Vec<&str> = text.split('\n').collect();
        highlight(language, text)
            .into_iter()
            .enumerate()
            .flat_map(|(index, spans)| {
                let line = lines[index];
                spans.into_iter().map(move |(range, style)| (index, line[range].to_string(), style.tint))
            })
            .collect()
    }

    #[test]
    fn every_grammar_and_its_queries_compile() {
        for language in Language::ALL {
            assert!(available(language), "{} does not compile", language.name());
        }
        assert!(config(INLINE_KEY).is_some(), "the inline markdown grammar does not compile");
    }

    #[test]
    fn rust_keywords_strings_and_comments_get_their_own_tints() {
        let spans = tints(Language::Rust, "fn main() {\n    let name = \"Глайд\"; // привіт\n}");
        assert!(spans.contains(&(0, "fn".into(), Some(Tint::Keyword))));
        assert!(spans.contains(&(1, "\"Глайд\"".into(), Some(Tint::String))));
        assert!(spans.contains(&(1, "// привіт".into(), Some(Tint::Comment))));
    }

    #[test]
    fn markdown_highlights_headings_inline_code_and_fenced_code_through_injections() {
        let text = "# Заголовок\n\nТекст із `коду` і **жирним**.\n\n```rust\nfn main() {}\n```";
        let spans = tints(Language::Markdown, text);
        assert!(
            spans
                .iter()
                .any(|(line, text, tint)| *line == 0 && text.contains("Заголовок") && *tint == Some(Tint::Heading))
        );
        assert!(
            spans.iter().any(|(line, text, tint)| *line == 2 && text.contains("коду") && *tint == Some(Tint::Code)),
            "{spans:?}"
        );
        assert!(spans.contains(&(5, "fn".into(), Some(Tint::Keyword))), "{spans:?}");
        assert!(
            spans.contains(&(5, "main".into(), Some(Tint::Function))),
            "the whole fence is highlighted, not just its first token: {spans:?}"
        );
        let lines = highlight(Language::Markdown, text);
        let strong = &text.split('\n').nth(2).unwrap();
        assert!(lines[2].iter().any(|(range, style)| style.bold && strong[range.clone()].contains("жирним")));
    }

    #[test]
    fn spans_never_cross_a_line_and_never_cover_the_newline() {
        let text = "/* one\ntwo\nthree */ let x = 1;";
        let lines = highlight(Language::JavaScript, text);
        let source: Vec<&str> = text.split('\n').collect();
        for (index, spans) in lines.iter().enumerate() {
            for (range, _) in spans {
                assert!(range.end <= source[index].len());
            }
        }
        assert_eq!(&source[1][lines[1][0].0.clone()], "two");
    }

    #[test]
    fn the_bridge_feeds_lines_in_order_and_restarts_from_the_first_changed_line() {
        let palette = Palette::new(
            &Tokens::new(glide_sdk::theme::Appearance::Dark, glide_sdk::theme::Accent::Blue),
            Font::MONOSPACE,
        );
        let lines = Arc::new(highlight(Language::Rust, "fn a() {}\nfn b() {}\nfn c() {}"));
        let none = Arc::new(Marks::new());
        let settings = |lines, revision, marks: &Arc<Marks>, marked| Settings {
            lines,
            revision,
            palette,
            marks: Arc::clone(marks),
            marked,
        };
        let mut bridge = Bridge::new(&settings(Arc::clone(&lines), 1, &none, 0));
        for line in ["fn a() {}", "fn b() {}", "fn c() {}"] {
            assert!(bridge.highlight_line(line).next().is_some());
        }
        assert_eq!(bridge.current_line(), 3);
        let changed = Arc::new(highlight(Language::Rust, "fn a() {}\nlet b = 1;\nfn c() {}"));
        bridge.update(&settings(Arc::clone(&changed), 2, &none, 0));
        assert_eq!(bridge.current_line(), 1);
        bridge.change_line(0);
        assert_eq!(bridge.current_line(), 0);
        assert_eq!(bridge.highlight_line("fn").next().map(|(range, _)| range), Some(0..2));
        let text = "fn a() {}\nlet b = 1;\nfn c() {}";
        let found = Arc::new(marks(text, &[14..15, 24..25], Some(1)));
        assert_eq!(*found, vec![(1, 4..5, false), (2, 3..4, true)]);
        bridge.update(&settings(changed, 2, &found, 1));
        assert_eq!(bridge.current_line(), 1, "a new match restarts from its line");
        let _ = bridge.highlight_line("let b = 1;");
        let last: Vec<_> = bridge.highlight_line("fn c() {}").collect();
        assert_eq!(
            last.last().map(|(range, highlight)| (range.clone(), highlight.color)),
            Some((3..4, Some(palette.current)))
        );
    }

    #[test]
    fn ranges_from_a_stale_parse_are_clamped_to_char_boundaries() {
        assert_eq!(clamp("Глайд", &(1..3)), Some(0..4));
        assert_eq!(clamp("ab", &(5..9)), None);
    }

    #[test]
    fn shifting_keeps_the_lines_around_an_edit() {
        let lines: Lines = vec![vec![(0..1, PLAIN)], Vec::new(), vec![(0..2, PLAIN)]];
        let shifted = shift(&lines, 1, 0, 2);
        assert_eq!(shifted.len(), 5);
        assert_eq!(shifted[4], vec![(0..2, PLAIN)]);
        assert_eq!(shift(&lines, 0, 2, 0), vec![vec![(0..2, PLAIN)]]);
    }

    #[test]
    fn aliases_from_fence_info_strings_resolve() {
        for (alias, language) in [("py", Language::Python), ("sh", Language::Bash), ("JS", Language::JavaScript)] {
            assert_eq!(Language::from_key(alias), Some(language));
        }
        assert_eq!(Language::from_key("brainfuck"), None);
    }
}

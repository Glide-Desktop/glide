use std::path::Path;
use std::sync::OnceLock;

use regex::Regex;

use crate::syntax::Language;

const SAMPLE_BYTES: usize = 64 * 1024;
const SAMPLE_LINES: usize = 400;

#[must_use]
pub fn by_name(path: &Path) -> Option<Language> {
    let name = path.file_name()?.to_str()?;
    let lower = name.to_ascii_lowercase();
    let exact = match lower.as_str() {
        "cargo.lock" | "pipfile" | "poetry.lock" | "uv.lock" => Some(Language::Toml),
        ".bashrc" | ".bash_profile" | ".bash_logout" | ".profile" | ".zshrc" | ".zprofile" | ".zshenv" | "pkgbuild"
        | "apkbuild" | ".xinitrc" | ".xprofile" => Some(Language::Bash),
        ".clang-format" | ".clangd" => Some(Language::Yaml),
        _ => None,
    };
    if exact.is_some() {
        return exact;
    }
    let extension = lower.rsplit_once('.').map(|(_, extension)| extension)?;
    let language = match extension {
        "md" | "markdown" | "mkd" | "mdown" | "mdx" => Language::Markdown,
        "rs" => Language::Rust,
        "py" | "pyi" | "pyw" => Language::Python,
        "js" | "mjs" | "cjs" | "jsx" => Language::JavaScript,
        "ts" | "mts" | "cts" => Language::TypeScript,
        "tsx" => Language::Tsx,
        "json" | "jsonc" | "json5" | "geojson" | "webmanifest" | "ipynb" => Language::Json,
        "toml" => Language::Toml,
        "yaml" | "yml" => Language::Yaml,
        "sh" | "bash" | "zsh" | "ksh" | "dash" | "ebuild" | "install" => Language::Bash,
        "c" | "h" => Language::C,
        "cc" | "cpp" | "cxx" | "c++" | "hh" | "hpp" | "hxx" | "h++" | "ipp" | "tpp" | "inl" => Language::Cpp,
        "go" => Language::Go,
        "java" => Language::Java,
        "lua" => Language::Lua,
        "html" | "htm" | "xhtml" => Language::Html,
        "css" => Language::Css,
        "xml" | "svg" | "xsl" | "xslt" | "plist" | "ui" | "glade" | "xsd" | "rss" | "atom" | "kml" | "gpx"
        | "desktop.xml" => Language::Xml,
        "txt" | "text" | "log" => Language::Plain,
        _ => return None,
    };
    Some(language)
}

#[must_use]
pub(crate) fn trusts_name(language: Language) -> bool {
    language != Language::Plain
}

fn sample(text: &str) -> &str {
    let mut end = text.len().min(SAMPLE_BYTES);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let text = &text[..end];
    match text.match_indices('\n').nth(SAMPLE_LINES) {
        Some((at, _)) => &text[..at],
        None => text,
    }
}

fn interpreter(first: &str) -> Option<Language> {
    let line = first.strip_prefix("#!")?.trim();
    let mut words = line.split_whitespace();
    let program = words.next()?.rsplit('/').next()?;
    let program = if program == "env" {
        words.find(|word| !word.starts_with('-') && !word.contains('='))?.rsplit('/').next()?
    } else {
        program
    };
    let program = program.trim_end_matches(|c: char| c.is_ascii_digit() || c == '.');
    match program {
        "sh" | "bash" | "zsh" | "dash" | "ksh" | "ash" | "mksh" => Some(Language::Bash),
        "python" | "pypy" => Some(Language::Python),
        "node" | "nodejs" | "deno" | "bun" | "gjs" => Some(Language::JavaScript),
        "ts-node" | "tsx" => Some(Language::TypeScript),
        "lua" | "luajit" => Some(Language::Lua),
        "rust-script" | "cargo" => Some(Language::Rust),
        _ => None,
    }
}

fn modeline(text: &str) -> Option<Language> {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    let pattern = PATTERN.get_or_init(|| {
        Regex::new(
            r"(?:vim?:.*\b(?:ft|filetype|syntax)=([\w+-]+))|(?:-\*-.*\bmode:\s*([\w+-]+))|(?:-\*-\s*([\w+-]+)\s*-\*-)",
        )
        .expect("valid modeline pattern")
    });
    let lines: Vec<&str> = text.lines().collect();
    let edges = lines.iter().take(5).chain(lines.iter().rev().take(5));
    edges.filter_map(|line| pattern.captures(line)).find_map(|captures| {
        let name = captures.get(1).or_else(|| captures.get(2)).or_else(|| captures.get(3))?.as_str();
        Language::from_key(name.trim_end_matches("-mode"))
    })
}

fn signature(text: &str) -> Option<Language> {
    let start = text.trim_start_matches('\u{feff}').trim_start();
    let head: String = start.chars().take(256).collect::<String>().to_ascii_lowercase();
    if head.starts_with("<?xml") {
        return Some(if head.contains("<!doctype html") || head.contains("<html") {
            Language::Html
        } else {
            Language::Xml
        });
    }
    if head.starts_with("<!doctype html") || head.starts_with("<html") {
        return Some(Language::Html);
    }
    if head.starts_with("<svg") {
        return Some(Language::Xml);
    }
    None
}

struct Cue {
    language: Language,
    pattern: &'static str,
    weight: f32,
}

const fn cue(language: Language, pattern: &'static str, weight: f32) -> Cue {
    Cue { language, pattern, weight }
}

const CUES: [Cue; 64] = [
    cue(Language::Markdown, r"^#{1,6} \S", 3.0),
    cue(Language::Markdown, r"^\s{0,3}[-*+] \S", 1.0),
    cue(Language::Markdown, r"^\s{0,3}\d{1,3}[.)] \S", 1.0),
    cue(Language::Markdown, r"^\s{0,3}(```|~~~)", 2.0),
    cue(Language::Markdown, r"\[[^\]\n]+\]\([^)\s]+\)", 2.0),
    cue(Language::Markdown, r"(\*\*|__)\S[^*\n]*\S(\*\*|__)", 1.5),
    cue(Language::Markdown, r"^> ", 1.0),
    cue(Language::Markdown, r"^\|.*\|\s*$", 1.0),
    cue(Language::Markdown, r"^(=+|-+)\s*$", 1.0),
    cue(Language::Markdown, r"`[^`\n]+`", 0.5),
    cue(Language::Rust, r"^\s*(pub(\([a-z]+\))? )?(async )?fn \w+", 3.0),
    cue(Language::Rust, r"\blet (mut )?\w+(: [\w<>&]+)? =", 2.0),
    cue(Language::Rust, r"^\s*(use|mod) [\w:]+(::\{[^}]*\})?;", 2.0),
    cue(Language::Rust, r"^\s*#\[(derive|cfg|test|allow|must_use)", 3.0),
    cue(Language::Rust, r"\bimpl(<[^>]*>)? \w+", 2.0),
    cue(Language::Rust, r"&(mut |'\w+ )?self\b|\bSelf\b|-> \w", 1.0),
    cue(Language::Python, r"^\s*def \w+\(.*\)( -> [\w\[\], .]+)?:\s*$", 3.0),
    cue(Language::Python, r"^\s*(from [\w.]+ )?import [\w.]+( as \w+)?\s*$", 2.0),
    cue(Language::Python, r"^\s*class \w+(\(.*\))?:\s*$", 3.0),
    cue(Language::Python, r"^\s*(elif|except|finally)\b.*:\s*$|\bself\.\w+", 1.5),
    cue(Language::Python, r#"^if __name__ == ["']__main__["']:"#, 4.0),
    cue(Language::JavaScript, r"\b(const|let|var) \w+ = ", 1.0),
    cue(Language::JavaScript, r"\bfunction\*? ?\w*\s*\(|=> \{?", 1.5),
    cue(Language::JavaScript, r"\bconsole\.\w+\(|\brequire\(|\bmodule\.exports\b|\bdocument\.\w+", 2.0),
    cue(Language::JavaScript, r#"^\s*(import .* from ['"]|export (default |const |function |class ))"#, 1.5),
    cue(Language::TypeScript, r"^\s*(export )?(interface|type) \w+(<[^>]*>)? (=|\{|extends)", 3.0),
    cue(Language::TypeScript, r"\w\??: (string|number|boolean|void|unknown|any|never)\b", 2.0),
    cue(Language::TypeScript, r"\b(private|public|readonly) \w+:|\bas const\b|<\w+>\(", 1.0),
    cue(Language::C, r"^\s*#\s*(include|define|ifn?def|endif|pragma)\b", 2.0),
    cue(Language::C, r"^\s*#include <(stdio|stdlib|string|unistd|stdint|stdbool)\.h>", 3.0),
    cue(Language::C, r"\b(printf|malloc|free|sizeof|memcpy)\s*\(|\bstruct \w+ \{", 1.5),
    cue(Language::C, r"^(static |extern )?(int|void|char|unsigned|size_t|bool|double|float) \**\w+\(", 2.0),
    cue(Language::Cpp, r"\bstd::\w+|\bnamespace \w+|\btemplate ?<", 3.0),
    cue(Language::Cpp, r"^\s*#include <(iostream|vector|string|memory|map|algorithm)>", 3.0),
    cue(Language::Cpp, r"\b(class|public:|private:|protected:|virtual|nullptr|auto&?) ", 1.0),
    cue(Language::Go, r"^package \w+\s*$", 4.0),
    cue(Language::Go, r"^func (\(\w+ \*?\w+\) )?\w+\(", 3.0),
    cue(Language::Go, r"\w+ := |\bfmt\.\w+\(|^import \($", 2.0),
    cue(Language::Java, r"\b(public|private|protected) (static )?(final )?(class|interface|void|enum) ", 3.0),
    cue(Language::Java, r"\bSystem\.out\.|^import java\.|@Override\b", 3.0),
    cue(Language::Lua, r"^\s*local (function )?\w+", 2.0),
    cue(Language::Lua, r"\bthen\s*$|^\s*end\s*$|~=|\.\.\s*\w|\belseif\b", 1.0),
    cue(Language::Lua, r"^\s*function [\w.:]+\(", 2.0),
    cue(Language::Bash, r"^\s*(if|while) \[\[? |^\s*(fi|done|esac)\s*$|\bthen\s*$", 2.0),
    cue(Language::Bash, r"\$\{\w+[^}]*\}|\$\(|\becho \S|^\s*export \w+=", 1.5),
    cue(Language::Bash, r"^\s*\w+\(\) \{|^\s*(set -e|source |\. /)", 2.0),
    cue(Language::Toml, r"^\[\[?[\w.-]+\]\]?\s*$", 2.0),
    cue(Language::Toml, r#"^[\w.-]+ = ("|'|\d|true|false|\[|\{)"#, 2.0),
    cue(Language::Yaml, r"^[\w.-]+:( |$)", 1.0),
    cue(Language::Yaml, r"^\s+[\w.-]+:( |$)|^\s*- [\w.-]+:( |$)", 1.0),
    cue(Language::Yaml, r"^---\s*$|^\s*- \S", 0.5),
    cue(Language::Html, r"(?i)<(div|span|p|a|body|head|title|script|meta|link|ul|li|br)\b[^>]*>", 2.0),
    cue(Language::Html, r"(?i)</(div|span|p|a|body|head|html|ul|li)>", 2.0),
    cue(Language::Css, r"^\s*[.#]?[\w-]+([.:#\[][^{]*)?\s*\{\s*$", 1.5),
    cue(Language::Css, r"^\s*[\w-]+: [^;]+;\s*$", 1.5),
    cue(Language::Css, r"^\s*@(media|import|font-face|keyframes|supports)\b", 3.0),
    cue(Language::Xml, r"^\s*<[\w:.-]+(\s+[\w:.-]+=\x22[^\x22]*\x22)*\s*/?>\s*$", 1.0),
    cue(Language::Xml, r"^\s*</[\w:.-]+>\s*$", 1.0),
    cue(Language::Json, r#"^\s*"[^"]+"\s*:\s*("|\d|-|\[|\{|true|false|null)"#, 1.5),
    cue(Language::Json, r"^\s*[\[{]\s*$|^\s*[\]}],?\s*$", 0.5),
    cue(Language::Rust, r"\bprintln!|\bvec!|\bOption<|\bResult<|\bunwrap\(\)", 2.0),
    cue(Language::Python, r"\bprint\(|\bNone\b|\bTrue\b|\bFalse\b|\bself\b", 0.5),
    cue(Language::Lua, r"\brequire\s*\(?['\x22]|\bpairs\(|\bnil\b", 1.0),
    cue(Language::Go, r"\bfunc\(|\bchan \w+|\bgo func\b|\bdefer \w+", 2.0),
];

fn compiled() -> &'static [(Language, Regex, f32)] {
    static COMPILED: OnceLock<Vec<(Language, Regex, f32)>> = OnceLock::new();
    COMPILED.get_or_init(|| {
        CUES.iter()
            .map(|cue| {
                let pattern = Regex::new(&format!("(?m){}", cue.pattern)).expect("valid detection cue");
                (cue.language, pattern, cue.weight)
            })
            .collect()
    })
}

fn scores(text: &str) -> Vec<(Language, f32)> {
    let lines = text.lines().filter(|line| !line.trim().is_empty()).count().max(1) as f32;
    let mut scores: Vec<(Language, f32)> = Vec::new();
    for (language, pattern, weight) in compiled() {
        let hits = pattern.find_iter(text).count() as f32;
        if hits == 0.0 {
            continue;
        }
        let score = weight * hits.min(lines) / lines.sqrt();
        match scores.iter_mut().find(|(candidate, _)| candidate == language) {
            Some((_, total)) => *total += score,
            None => scores.push((*language, score)),
        }
    }
    scores.sort_by(|a, b| b.1.total_cmp(&a.1));
    scores
}

#[must_use]
pub(crate) fn error_ratio(language: Language, text: &str) -> Option<f32> {
    let grammar = language.grammar()?;
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&grammar).ok()?;
    let tree = parser.parse(text, None)?;
    if text.trim().is_empty() {
        return Some(0.0);
    }
    let mut errors = 0usize;
    let mut cursor = tree.walk();
    let mut descend = true;
    loop {
        let node = cursor.node();
        if descend && (node.is_error() || node.is_missing()) {
            errors += (node.end_byte() - node.start_byte()).max(1);
            descend = false;
        }
        if descend && cursor.goto_first_child() {
            continue;
        }
        descend = true;
        if cursor.goto_next_sibling() {
            continue;
        }
        loop {
            if !cursor.goto_parent() {
                return Some(errors as f32 / text.len() as f32);
            }
            if cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

fn json(text: &str) -> bool {
    let trimmed = text.trim_start_matches('\u{feff}').trim();
    (trimmed.starts_with('{') && trimmed.ends_with('}') || trimmed.starts_with('[') && trimmed.ends_with(']'))
        && error_ratio(Language::Json, trimmed).is_some_and(|ratio| ratio == 0.0)
}

#[must_use]
pub(crate) fn by_content(text: &str) -> Language {
    let text = sample(text);
    if text.trim().is_empty() {
        return Language::Plain;
    }
    if let Some(language) = text.lines().next().and_then(interpreter) {
        return language;
    }
    if let Some(language) = modeline(text).or_else(|| signature(text)) {
        return language;
    }
    if json(text) {
        return Language::Json;
    }
    let scores = scores(text);
    let Some(&(_, best)) = scores.first() else { return Language::Plain };
    if best < 1.5 {
        return Language::Plain;
    }
    let mut ranked: Vec<(Language, f32)> = scores
        .iter()
        .take(4)
        .filter(|(_, score)| *score >= best * 0.35)
        .filter_map(|&(language, score)| {
            if language == Language::Markdown {
                return Some((language, score));
            }
            let ratio = error_ratio(language, text)?;
            (ratio < 0.12).then_some((language, score * (1.0 - ratio * 6.0)))
        })
        .collect();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
    ranked.first().map_or(Language::Plain, |(language, _)| *language)
}

#[must_use]
pub fn detect(path: Option<&Path>, text: &str) -> Language {
    if let Some(language) = path.and_then(by_name).filter(|language| trusts_name(*language)) {
        return language;
    }
    by_content(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn file_names_and_extensions_decide_first() {
        let cases = [
            ("notes.md", Language::Markdown),
            ("main.rs", Language::Rust),
            ("Cargo.lock", Language::Toml),
            (".zshrc", Language::Bash),
            ("App.TSX", Language::Tsx),
            ("icon.svg", Language::Xml),
            ("vector.hpp", Language::Cpp),
        ];
        for (name, language) in cases {
            assert_eq!(by_name(&PathBuf::from(name)), Some(language), "{name}");
        }
        assert_eq!(by_name(&PathBuf::from("README")), None);
    }

    #[test]
    fn a_txt_file_is_still_examined_for_markdown() {
        let text = "# Нотатки\n\n- перший пункт\n- другий пункт\n\nДив. [сайт](https://glide.dev).\n";
        assert_eq!(detect(Some(&PathBuf::from("notes.txt")), text), Language::Markdown);
        assert_eq!(detect(Some(&PathBuf::from("main.rs")), text), Language::Rust, "a code extension wins");
    }

    #[test]
    fn shebangs_and_modelines_name_the_language() {
        assert_eq!(by_content("#!/usr/bin/env python3\nprint('hi')\n"), Language::Python);
        assert_eq!(by_content("#!/bin/bash\necho hi\n"), Language::Bash);
        assert_eq!(by_content("#!/usr/bin/env -S node --flag\n"), Language::JavaScript);
        assert_eq!(by_content("some text\n# vim: set ft=yaml :\n"), Language::Yaml);
        assert_eq!(by_content("-*- mode: lua -*-\nx = 1\n"), Language::Lua);
    }

    #[test]
    fn untitled_content_is_recognised() {
        let cases = [
            ("{\n  \"name\": \"glide\",\n  \"version\": 1\n}\n", Language::Json),
            ("<?xml version=\"1.0\"?>\n<root><a/></root>\n", Language::Xml),
            ("<!DOCTYPE html>\n<html><body><p>Hi</p></body></html>\n", Language::Html),
            (
                "use std::io;\n\nfn main() {\n    let mut line = String::new();\n    io::stdin().read_line(&mut line).unwrap();\n    println!(\"{line}\");\n}\n",
                Language::Rust,
            ),
            (
                "import os\n\n\ndef walk(path):\n    for name in os.listdir(path):\n        print(name)\n\n\nif __name__ == \"__main__\":\n    walk('.')\n",
                Language::Python,
            ),
            (
                "package main\n\nimport \"fmt\"\n\nfunc main() {\n\tname := \"glide\"\n\tfmt.Println(name)\n}\n",
                Language::Go,
            ),
            ("#include <stdio.h>\n\nint main(void) {\n    printf(\"hi\\n\");\n    return 0;\n}\n", Language::C),
            (
                "#include <iostream>\n\nint main() {\n    std::cout << \"hi\" << std::endl;\n    return 0;\n}\n",
                Language::Cpp,
            ),
            ("[package]\nname = \"glide\"\nversion = \"0.1.0\"\n\n[dependencies]\nserde = \"1\"\n", Language::Toml),
            ("name: glide\nversion: 1\nitems:\n  - one\n  - two\nnested:\n  key: value\n", Language::Yaml),
            (
                "# Глайд\n\nКороткий опис **проєкту**.\n\n## Встановлення\n\n1. Зберіть\n2. Встановіть\n\n```sh\ncargo build\n```\n",
                Language::Markdown,
            ),
            (
                "interface User {\n  name: string;\n  age: number;\n}\n\nconst user: User = { name: 'a', age: 1 };\n",
                Language::TypeScript,
            ),
            (
                "const items = require('./items');\n\nfunction total(list) {\n  return list.reduce((a, b) => a + b, 0);\n}\n\nconsole.log(total(items));\n",
                Language::JavaScript,
            ),
            (
                ".header {\n  color: red;\n  margin: 0 auto;\n}\n\n@media (max-width: 600px) {\n  .header { display: none; }\n}\n",
                Language::Css,
            ),
            (
                "if [ -z \"$HOME\" ]; then\n  echo \"no home\"\n  exit 1\nfi\nexport PATH=\"$HOME/bin:$PATH\"\n",
                Language::Bash,
            ),
            (
                "local function greet(name)\n  print(\"hi \" .. name)\nend\n\nfor _, v in pairs(list) do\n  greet(v)\nend\n",
                Language::Lua,
            ),
            (
                "public class Main {\n    public static void main(String[] args) {\n        System.out.println(\"hi\");\n    }\n}\n",
                Language::Java,
            ),
        ];
        for (text, language) in cases {
            assert_eq!(by_content(text), language, "{text}");
        }
    }

    #[test]
    fn prose_stays_plain_text() {
        for text in [
            "",
            "Привіт! Це звичайний лист без жодної розмітки.\nДо зустрічі завтра.\n",
            "Shopping list: milk, bread, eggs.\nCall mum at 6.\n",
            "Meeting notes\n\nWe agreed to ship on Friday. Anna will write the release notes.\n",
        ] {
            assert_eq!(by_content(text), Language::Plain, "{text}");
        }
    }

    #[test]
    fn error_ratio_separates_valid_from_broken_input() {
        assert_eq!(error_ratio(Language::Json, "{\"a\": [1, 2]}"), Some(0.0));
        assert!(error_ratio(Language::Json, "{\"a\": [1, 2 oops").is_some_and(|ratio| ratio > 0.0));
        assert_eq!(error_ratio(Language::Plain, "x"), None);
    }
}

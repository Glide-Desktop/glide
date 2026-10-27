use std::ops::Range;

use crate::syntax::Language;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub range: Range<usize>,
    pub text: String,
    pub select: Range<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Case {
    Upper,
    Lower,
    Title,
    Invert,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Indent {
    pub spaces: bool,
    pub width: usize,
}

impl Indent {
    #[must_use]
    pub fn unit(self) -> String {
        if self.spaces { " ".repeat(self.width.max(1)) } else { String::from("\t") }
    }
}

#[must_use]
pub(crate) fn line_span(text: &str, selection: &Range<usize>) -> Range<usize> {
    let start = text[..selection.start].rfind('\n').map_or(0, |at| at + 1);
    let last = if selection.end > selection.start && text[..selection.end].ends_with('\n') {
        selection.end - 1
    } else {
        selection.end
    };
    let end = text[last..].find('\n').map_or(text.len(), |at| last + at);
    start..end.max(start)
}

fn whole(range: Range<usize>, text: String) -> Change {
    let select = range.start..range.start + text.len();
    Change { range, text, select }
}

fn leading(line: &str) -> &str {
    &line[..line.len() - line.trim_start().len()]
}

#[must_use]
pub(crate) fn toggle_comment(text: &str, selection: &Range<usize>, language: Language) -> Option<Change> {
    let span = line_span(text, selection);
    let block = &text[span.clone()];
    if let Some(marker) = language.line_comment() {
        let lines: Vec<&str> = block.split('\n').collect();
        let filled: Vec<&&str> = lines.iter().filter(|line| !line.trim().is_empty()).collect();
        if filled.is_empty() {
            return None;
        }
        let commented = filled.iter().all(|line| line.trim_start().starts_with(marker));
        let depth = filled.iter().map(|line| leading(line).len()).min().unwrap_or(0);
        let changed: Vec<String> = lines
            .iter()
            .map(|line| {
                if line.trim().is_empty() {
                    return (*line).to_string();
                }
                if commented {
                    let indent = leading(line);
                    let rest = &line[indent.len() + marker.len()..];
                    format!("{indent}{}", rest.strip_prefix(' ').unwrap_or(rest))
                } else {
                    format!("{}{marker} {}", &line[..depth], &line[depth..])
                }
            })
            .collect();
        return Some(whole(span, changed.join("\n")));
    }
    let (open, close) = language.block_comment()?;
    let trimmed = block.trim();
    let indent = leading(block);
    let text = if let Some(inner) = trimmed.strip_prefix(open).and_then(|rest| rest.strip_suffix(close)) {
        let inner = inner.strip_prefix(' ').unwrap_or(inner);
        format!("{indent}{}", inner.strip_suffix(' ').unwrap_or(inner))
    } else if trimmed.is_empty() {
        return None;
    } else {
        format!("{indent}{open} {} {close}", &block[indent.len()..])
    };
    Some(whole(span, text))
}

#[must_use]
pub(crate) fn duplicate(text: &str, selection: &Range<usize>) -> Change {
    if selection.start < selection.end {
        let copy = &text[selection.clone()];
        let at = selection.end;
        return Change { range: at..at, text: copy.to_string(), select: at..at + copy.len() };
    }
    let span = line_span(text, selection);
    let line = &text[span.clone()];
    let offset = selection.start - span.start + line.len() + 1;
    Change { range: span.end..span.end, text: format!("\n{line}"), select: span.start + offset..span.start + offset }
}

#[must_use]
pub(crate) fn delete_lines(text: &str, selection: &Range<usize>) -> Change {
    let span = line_span(text, selection);
    let range = if span.end < text.len() {
        span.start..span.end + 1
    } else if span.start > 0 {
        span.start - 1..span.end
    } else {
        span.clone()
    };
    let caret = range.start.min(text.len() - (range.end - range.start));
    let caret = if span.end < text.len() || span.start == 0 {
        caret
    } else {
        text[..range.start].rfind('\n').map_or(0, |at| at + 1)
    };
    Change { range, text: String::new(), select: caret..caret }
}

#[must_use]
pub(crate) fn move_lines(text: &str, selection: &Range<usize>, up: bool) -> Option<Change> {
    let span = line_span(text, selection);
    let block = &text[span.clone()];
    if up {
        if span.start == 0 {
            return None;
        }
        let above = text[..span.start - 1].rfind('\n').map_or(0, |at| at + 1);
        let neighbour = &text[above..span.start - 1];
        let shift = neighbour.len() + 1;
        Some(Change {
            range: above..span.end,
            text: format!("{block}\n{neighbour}"),
            select: selection.start - shift..selection.end - shift,
        })
    } else {
        if span.end >= text.len() {
            return None;
        }
        let below_end = text[span.end + 1..].find('\n').map_or(text.len(), |at| span.end + 1 + at);
        let neighbour = &text[span.end + 1..below_end];
        let shift = neighbour.len() + 1;
        Some(Change {
            range: span.start..below_end,
            text: format!("{neighbour}\n{block}"),
            select: selection.start + shift..selection.end + shift,
        })
    }
}

#[must_use]
pub(crate) fn join_lines(text: &str, selection: &Range<usize>) -> Option<Change> {
    let span = line_span(text, selection);
    let span = if span.start == line_span(text, &(selection.end..selection.end)).start {
        let next = text[span.end..].find('\n').map(|at| span.end + at)?;
        let end = text[next + 1..].find('\n').map_or(text.len(), |at| next + 1 + at);
        span.start..end
    } else {
        span
    };
    let joined: Vec<&str> = text[span.clone()]
        .split('\n')
        .enumerate()
        .map(|(index, line)| if index == 0 { line.trim_end() } else { line.trim() })
        .collect();
    let joined = joined.into_iter().filter(|line| !line.is_empty()).collect::<Vec<_>>().join(" ");
    Some(whole(span, joined))
}

#[must_use]
pub(crate) fn sort_lines(text: &str, selection: &Range<usize>) -> Option<Change> {
    let span = line_span(text, selection);
    let mut lines: Vec<&str> = text[span.clone()].split('\n').collect();
    if lines.len() < 2 {
        return None;
    }
    lines.sort_by_key(|line| line.to_lowercase());
    Some(whole(span, lines.join("\n")))
}

#[must_use]
pub fn indent(text: &str, selection: &Range<usize>, indent: Indent) -> Change {
    let unit = indent.unit();
    if !text[selection.clone()].contains('\n') && selection.start == selection.end {
        let at = selection.start;
        let column = at - text[..at].rfind('\n').map_or(0, |line| line + 1);
        let pad = if indent.spaces { " ".repeat(indent.width - column % indent.width.max(1)) } else { unit };
        return Change { range: at..at, select: at + pad.len()..at + pad.len(), text: pad };
    }
    let span = line_span(text, selection);
    let lines: Vec<String> = text[span.clone()]
        .split('\n')
        .map(|line| if line.is_empty() { String::new() } else { format!("{unit}{line}") })
        .collect();
    whole(span, lines.join("\n"))
}

#[must_use]
pub(crate) fn unindent(text: &str, selection: &Range<usize>, indent: Indent) -> Option<Change> {
    let span = line_span(text, selection);
    let width = indent.width.max(1);
    let lines: Vec<String> = text[span.clone()]
        .split('\n')
        .map(|line| {
            if let Some(rest) = line.strip_prefix('\t') {
                return rest.to_string();
            }
            let spaces = line.chars().take(width).take_while(|c| *c == ' ').count();
            line[spaces..].to_string()
        })
        .collect();
    let joined = lines.join("\n");
    (joined != text[span.clone()]).then(|| whole(span, joined))
}

#[must_use]
pub(crate) fn change_case(text: &str, selection: &Range<usize>, case: Case) -> Option<Change> {
    let range = if selection.start == selection.end { word_at(text, selection.start)? } else { selection.clone() };
    let source = &text[range.clone()];
    let changed = match case {
        Case::Upper => source.to_uppercase(),
        Case::Lower => source.to_lowercase(),
        Case::Invert => source
            .chars()
            .flat_map(|c| -> Vec<char> {
                if c.is_uppercase() { c.to_lowercase().collect() } else { c.to_uppercase().collect() }
            })
            .collect(),
        Case::Title => {
            let mut start = true;
            source
                .chars()
                .flat_map(|c| {
                    let out: Vec<char> = if start { c.to_uppercase().collect() } else { c.to_lowercase().collect() };
                    start = !c.is_alphanumeric() && c != '\'' && c != '’';
                    out
                })
                .collect()
        }
    };
    Some(whole(range, changed))
}

#[must_use]
pub(crate) fn word_at(text: &str, at: usize) -> Option<Range<usize>> {
    let word = |c: char| c.is_alphanumeric() || c == '_';
    let start = text[..at].char_indices().rev().take_while(|(_, c)| word(*c)).last().map_or(at, |(index, _)| index);
    let end = text[at..].char_indices().find(|(_, c)| !word(*c)).map_or(text.len(), |(index, _)| at + index);
    (end > start).then_some(start..end)
}

#[must_use]
pub fn newline(text: &str, at: usize, auto_indent: bool) -> String {
    if !auto_indent {
        return String::from("\n");
    }
    let start = text[..at].rfind('\n').map_or(0, |line| line + 1);
    format!("\n{}", leading(&text[start..at]))
}

struct Item {
    indent: usize,
    bullet: char,
    number: Option<usize>,
    check: Option<bool>,
    boxed: usize,
    content: usize,
}

fn list_item(line: &str) -> Option<Item> {
    let indent = line.len() - line.trim_start_matches([' ', '\t']).len();
    let rest = &line[indent..];
    let (bullet, number, marker) = match rest.chars().next()? {
        c @ ('-' | '*' | '+') if rest[1..].starts_with(' ') => (c, None, 2),
        '0'..='9' => {
            let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
            let delimiter = rest[digits..].chars().next().filter(|c| matches!(c, '.' | ')'))?;
            if digits > 9 || !rest[digits + 1..].starts_with(' ') {
                return None;
            }
            (delimiter, Some(rest[..digits].parse().ok()?), digits + 2)
        }
        _ => return None,
    };
    let after = &rest[marker..];
    let check = ["[ ]", "[x]", "[X]"]
        .iter()
        .position(|tag| after.starts_with(tag) && (after.len() == tag.len() || after[tag.len()..].starts_with(' ')));
    let box_len = check.map_or(0, |_| if after.len() == 3 { 3 } else { 4 });
    Some(Item {
        indent,
        bullet,
        number,
        check: check.map(|tag| tag > 0),
        boxed: box_len,
        content: indent + marker + box_len,
    })
}

#[must_use]
pub(crate) fn continue_list(text: &str, at: usize) -> Option<Change> {
    let start = text[..at].rfind('\n').map_or(0, |line| line + 1);
    let end = text[at..].find('\n').map_or(text.len(), |line| at + line);
    let line = &text[start..end];
    let item = list_item(line)?;
    if at - start < item.content.min(line.len()) {
        return None;
    }
    if line[item.content.min(line.len())..].trim().is_empty() {
        return Some(Change { range: start..end, text: String::new(), select: start..start });
    }
    let marker = match item.number {
        Some(number) => format!("{}{}{} ", &line[..item.indent], number + 1, item.bullet),
        None => format!("{}{} ", &line[..item.indent], item.bullet),
    };
    let text = format!("\n{marker}{}", if item.check.is_some() { "[ ] " } else { "" });
    let caret = at + text.len();
    Some(Change { range: at..at, text, select: caret..caret })
}

#[must_use]
pub(crate) fn toggle_check(text: &str, selection: &Range<usize>) -> Option<Change> {
    let span = line_span(text, selection);
    let lines: Vec<&str> = text[span.clone()].split('\n').collect();
    let items: Vec<Option<Item>> = lines.iter().map(|line| list_item(line)).collect();
    let checks: Vec<bool> = items.iter().flatten().filter_map(|item| item.check).collect();
    let target = !checks.is_empty() && checks.contains(&false);
    let rewritten: Vec<String> = lines
        .iter()
        .zip(&items)
        .map(|(line, item)| match item {
            _ if line.trim().is_empty() => (*line).to_string(),
            Some(Item { check: Some(_), content, boxed, .. }) => {
                let box_at = content - boxed;
                format!("{}[{}]{}", &line[..box_at], if target { 'x' } else { ' ' }, &line[box_at + 3..])
            }
            Some(item) => {
                let at = item.content;
                format!("{}[ ] {}", &line[..at], &line[at..])
            }
            None => {
                let at = line.len() - line.trim_start().len();
                format!("{}- [ ] {}", &line[..at], &line[at..])
            }
        })
        .collect();
    let joined = rewritten.join("\n");
    if joined == text[span.clone()] {
        return None;
    }
    let delta = joined.len().cast_signed() - (span.end - span.start).cast_signed();
    let select = if selection.start == selection.end {
        let caret = (selection.start.cast_signed() + delta).max(span.start.cast_signed()).cast_unsigned();
        caret..caret
    } else {
        span.start..span.start + joined.len()
    };
    Some(Change { range: span, text: joined, select })
}

#[must_use]
pub(crate) fn trim_trailing(text: &str) -> Option<String> {
    let trimmed: Vec<&str> = text.split('\n').map(str::trim_end).collect();
    let joined = trimmed.join("\n");
    (joined != text).then_some(joined)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stats {
    pub lines: usize,
    pub words: usize,
    pub characters: usize,
    pub characters_without_spaces: usize,
    pub bytes: usize,
}

#[must_use]
pub fn stats(text: &str) -> Stats {
    Stats {
        lines: text.matches('\n').count() + 1,
        words: text.split_whitespace().count(),
        characters: text.chars().count(),
        characters_without_spaces: text.chars().filter(|c| !c.is_whitespace()).count(),
        bytes: text.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(text: &str, change: &Change) -> String {
        let mut text = text.to_string();
        text.replace_range(change.range.clone(), &change.text);
        text
    }

    fn enter(text: &str) -> String {
        let at = text.find('|').unwrap();
        let text = text.replacen('|', "", 1);
        continue_list(&text, at).map_or_else(|| text.clone(), |change| apply(&text, &change))
    }

    #[test]
    fn enter_continues_bullets_numbers_and_checklists() {
        assert_eq!(enter("- one|"), "- one\n- ");
        assert_eq!(enter("  * nested|"), "  * nested\n  * ");
        assert_eq!(enter("9. nine|"), "9. nine\n10. ");
        assert_eq!(enter("3) three|"), "3) three\n4) ");
        assert_eq!(enter("- [x] done|"), "- [x] done\n- [ ] ");
        assert_eq!(enter("- [ ] todo|"), "- [ ] todo\n- [ ] ");
        assert_eq!(enter("- one|two"), "- one\n- two", "text after the caret moves to the new item");
    }

    #[test]
    fn enter_on_an_empty_item_ends_the_list_and_elsewhere_is_left_alone() {
        assert_eq!(enter("- one\n- |"), "- one\n");
        assert_eq!(enter("- [ ] |"), "");
        assert_eq!(enter("plain|"), "plain");
        assert_eq!(enter("-no space|"), "-no space");
        assert_eq!(enter("1.5 apples|"), "1.5 apples");
        assert_eq!(enter("-| item"), "- item", "a caret inside the marker is a plain newline");
    }

    fn check(text: &str) -> String {
        let at = text.find('|').unwrap();
        let text = text.replacen('|', "", 1);
        toggle_check(&text, &(at..at)).map_or_else(|| text.clone(), |change| apply(&text, &change))
    }

    #[test]
    fn ctrl_enter_walks_a_line_through_plain_unchecked_and_checked() {
        assert_eq!(check("buy milk|"), "- [ ] buy milk");
        assert_eq!(check("- buy milk|"), "- [ ] buy milk");
        assert_eq!(check("- [ ] buy milk|"), "- [x] buy milk");
        assert_eq!(check("- [x] buy milk|"), "- [ ] buy milk");
        assert_eq!(check("  1. step|"), "  1. [ ] step");
        assert_eq!(check("- [ ] |"), "- [x] ");
        assert_eq!(check("|"), "");
    }

    #[test]
    fn the_caret_stays_on_its_text_when_a_box_is_added() {
        let text = "buy milk";
        let change = toggle_check(text, &(4..4)).unwrap();
        assert_eq!(change.select, 10..10);
    }

    #[test]
    fn a_selection_checks_everything_unless_all_are_already_checked() {
        let text = "- [x] a\n- [ ] b\nplain\n\n- [x] c";
        let change = toggle_check(text, &(0..text.len())).unwrap();
        assert_eq!(apply(text, &change), "- [x] a\n- [x] b\n- [ ] plain\n\n- [x] c");
        let done = "- [x] a\n- [x] c";
        let change = toggle_check(done, &(0..done.len())).unwrap();
        assert_eq!(apply(done, &change), "- [ ] a\n- [ ] c");
    }

    #[test]
    fn comments_toggle_at_the_shallowest_indent_and_back() {
        let text = "fn a() {\n    one();\n\n        two();\n}";
        let change = toggle_comment(text, &(10..30), Language::Rust).unwrap();
        let commented = apply(text, &change);
        assert_eq!(commented, "fn a() {\n    // one();\n\n    //     two();\n}");
        let back = toggle_comment(&commented, &(change.select.start..change.select.end), Language::Rust).unwrap();
        assert_eq!(apply(&commented, &back), text);
        let html = toggle_comment("<p>hi</p>", &(0..0), Language::Html).unwrap();
        assert_eq!(apply("<p>hi</p>", &html), "<!-- <p>hi</p> -->");
        let back = toggle_comment("<!-- <p>hi</p> -->", &(0..0), Language::Html).unwrap();
        assert_eq!(apply("<!-- <p>hi</p> -->", &back), "<p>hi</p>");
        assert_eq!(toggle_comment("x", &(0..0), Language::Plain), None);
    }

    #[test]
    fn lines_duplicate_move_and_delete() {
        let text = "one\ntwo\nthree";
        assert_eq!(apply(text, &duplicate(text, &(5..5))), "one\ntwo\ntwo\nthree");
        let up = move_lines(text, &(5..5), true).unwrap();
        assert_eq!(apply(text, &up), "two\none\nthree");
        assert_eq!(up.select, 1..1);
        let down = move_lines(text, &(5..5), false).unwrap();
        assert_eq!(apply(text, &down), "one\nthree\ntwo");
        assert_eq!(move_lines(text, &(0..0), true), None);
        assert_eq!(move_lines(text, &(10..10), false), None);
        assert_eq!(apply(text, &delete_lines(text, &(5..5))), "one\nthree");
        assert_eq!(apply(text, &delete_lines(text, &(10..10))), "one\ntwo");
        assert_eq!(apply("solo", &delete_lines("solo", &(1..1))), "");
    }

    #[test]
    fn join_sort_and_trim() {
        let text = "a  \n   b\nc";
        assert_eq!(apply(text, &join_lines(text, &(0..0)).unwrap()), "a b\nc");
        assert_eq!(apply("b\nA\nc", &sort_lines("b\nA\nc", &(0..5)).unwrap()), "A\nb\nc");
        assert_eq!(trim_trailing("a \nb\t\nc"), Some("a\nb\nc".into()));
        assert_eq!(trim_trailing("a\nb"), None);
    }

    #[test]
    fn indentation_uses_tabs_or_tab_stops() {
        let spaces = Indent { spaces: true, width: 4 };
        assert_eq!(apply("ab", &indent("ab", &(1..1), spaces)), "a   b");
        assert_eq!(apply("a\nb", &indent("a\nb", &(0..3), spaces)), "    a\n    b");
        let tabs = Indent { spaces: false, width: 4 };
        assert_eq!(apply("a\n\nb", &indent("a\n\nb", &(0..4), tabs)), "\ta\n\n\tb");
        assert_eq!(apply("\ta\n  b", &unindent("\ta\n  b", &(0..6), spaces).unwrap()), "a\nb");
        assert_eq!(unindent("a", &(0..0), spaces), None);
    }

    #[test]
    fn case_changes_apply_to_the_selection_or_the_word_under_the_caret() {
        let text = "привіт світе";
        assert_eq!(apply(text, &change_case(text, &(4..4), Case::Upper).unwrap()), "ПРИВІТ світе");
        assert_eq!(apply(text, &change_case(text, &(0..text.len()), Case::Title).unwrap()), "Привіт Світе");
        assert_eq!(apply("aBc", &change_case("aBc", &(0..3), Case::Invert).unwrap()), "AbC");
        assert_eq!(change_case("  ", &(1..1), Case::Upper), None);
    }

    #[test]
    fn new_lines_keep_the_indentation_of_the_line_they_split() {
        assert_eq!(newline("    let x", 9, true), "\n    ");
        assert_eq!(newline("\tif x", 2, true), "\n\t");
        assert_eq!(newline("    x", 5, false), "\n");
    }

    #[test]
    fn statistics_count_unicode_characters() {
        let stats = stats("Привіт, світе!\nДругий рядок");
        assert_eq!((stats.lines, stats.words, stats.characters), (2, 4, 27));
    }
}

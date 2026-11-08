use super::{PageText, Rect, Word};

#[derive(Debug, Clone, PartialEq)]
pub struct Glyph {
    pub text: String,
    pub x: f32,
    pub baseline: f32,
    pub size: f32,
    pub advance: Option<f32>,
}

#[must_use]
pub fn words(glyphs: &[Glyph], images: Vec<Rect>) -> PageText {
    let mut out = PageText { words: Vec::new(), images };
    let mut line = 0u32;
    let mut current: Option<(String, Rect)> = None;
    let mut last: Option<&Glyph> = None;
    for (index, glyph) in glyphs.iter().enumerate() {
        let size = glyph.size.max(1.0);
        let width = glyph
            .advance
            .or_else(|| {
                glyphs
                    .get(index + 1)
                    .filter(|next| (next.baseline - glyph.baseline).abs() < size * 0.3)
                    .map(|next| next.x - glyph.x)
            })
            .filter(|width| *width > 0.0 && *width < size * 2.0)
            .unwrap_or(size * 0.5);
        let rect = Rect::new(glyph.x, glyph.baseline - size * 0.82, width, size * 1.04);
        let new_line = last.is_some_and(|previous| {
            (previous.baseline - glyph.baseline).abs() > size * 0.45 || glyph.x + size < previous.x
        });
        let gap = !new_line && current.as_ref().is_some_and(|(_, word)| glyph.x - word.right() > size * 0.25);
        if new_line || gap || glyph.text.trim().is_empty() {
            if let Some((text, rect)) = current.take() {
                out.words.push(Word { text, rect, line });
            }
            if new_line {
                line += 1;
            }
        }
        if !glyph.text.trim().is_empty() {
            match current.as_mut() {
                Some((text, word)) => {
                    text.push_str(&glyph.text);
                    *word = word.union(&rect);
                }
                None => current = Some((glyph.text.clone(), rect)),
            }
        }
        last = Some(glyph);
    }
    if let Some((text, rect)) = current {
        out.words.push(Word { text, rect, line });
    }
    out
}

#[must_use]
pub fn fold(text: &str) -> String {
    text.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'ё' => 'е',
            'ґ' => 'г',
            '’' | 'ʼ' | '`' => '\'',
            other => other,
        })
        .collect()
}

fn folded_with_origin(text: &str, case: bool) -> (String, Vec<usize>) {
    let mut folded = String::new();
    let mut origin = Vec::new();
    let mut buffer = [0u8; 4];
    for (index, c) in text.chars().enumerate() {
        if case {
            folded.push(c);
            origin.push(index);
            continue;
        }
        for f in fold(c.encode_utf8(&mut buffer)).chars() {
            folded.push(f);
            origin.push(index);
        }
    }
    (folded, origin)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub page: usize,
    pub rects: Vec<Rect>,
    pub before: String,
    pub matched: String,
    pub after: String,
}

#[must_use]
pub fn mentions(text: &str, query: &str, whole_word: bool, case: bool) -> bool {
    let (hay, needle) =
        if case { (text.to_string(), query.trim().to_string()) } else { (fold(text), fold(query.trim())) };
    if needle.is_empty() {
        return false;
    }
    hay.match_indices(&needle).any(|(at, found)| {
        let before = hay[..at].chars().next_back();
        let after = hay[at + found.len()..].chars().next();
        !whole_word || !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Anchor {
    Start,
    End,
    Whole,
}

fn locate(text: &str, token: &str, case: bool, anchor: Anchor, whole: bool) -> Option<(usize, usize)> {
    let (hay, origin) = folded_with_origin(text, case);
    let lead = hay.len() - hay.trim_start_matches(|c: char| !c.is_alphanumeric()).len();
    let trail = hay.trim_end_matches(|c: char| !c.is_alphanumeric()).len();
    if lead >= trail || token.is_empty() {
        return None;
    }
    let core = &hay[lead..trail];
    let at = if whole || anchor == Anchor::Whole {
        (core == token).then_some(lead)?
    } else if anchor == Anchor::End {
        core.ends_with(token).then(|| trail - token.len())?
    } else {
        core.starts_with(token).then_some(lead)?
    };
    let first = hay[..at].chars().count();
    let start = *origin.get(first)?;
    let end = origin.get(first + token.chars().count() - 1).map_or(start + 1, |last| last + 1);
    Some((start, end))
}

fn span(word: &Word, start: usize, end: usize) -> Rect {
    let count = word.text.chars().count().max(1) as f32;
    let share = |n: usize| word.rect.w * n as f32 / count;
    Rect::new(word.rect.x + share(start), word.rect.y, (share(end) - share(start)).max(2.0), word.rect.h)
}

fn context(text: &PageText, first: usize, last: usize, prefix: &str, suffix: &str) -> (String, String) {
    let before_words: Vec<&str> = text.words[first.saturating_sub(5)..first].iter().map(|w| w.text.as_str()).collect();
    let after_words: Vec<&str> = text.words.iter().skip(last + 1).take(6).map(|w| w.text.as_str()).collect();
    let mut before = before_words.join(" ");
    if !before.is_empty() {
        before.push(' ');
    }
    before.push_str(prefix);
    let mut after = suffix.to_string();
    if !after_words.is_empty() {
        after.push(' ');
        after.push_str(&after_words.join(" "));
    }
    (before, after)
}

fn find_phrase(page: usize, text: &PageText, tokens: &[String], whole_word: bool, case: bool) -> Vec<Hit> {
    let count = tokens.len();
    let mut hits = Vec::new();
    for index in 0..text.words.len().saturating_sub(count - 1) {
        let words = &text.words[index..index + count];
        let Some(head) = locate(&words[0].text, &tokens[0], case, Anchor::End, whole_word) else {
            continue;
        };
        let middle = (1..count - 1).all(|i| locate(&words[i].text, &tokens[i], case, Anchor::Whole, true).is_some());
        if !middle {
            continue;
        }
        let Some(tail) = locate(&words[count - 1].text, &tokens[count - 1], case, Anchor::Start, whole_word) else {
            continue;
        };
        let mut rects: Vec<(u32, Rect)> = Vec::new();
        let mut parts: Vec<String> = Vec::new();
        for (i, word) in words.iter().enumerate() {
            let chars: Vec<char> = word.text.chars().collect();
            let (start, end) = if i == 0 {
                (head.0, chars.len())
            } else if i == count - 1 {
                (0, tail.1)
            } else {
                (0, chars.len())
            };
            parts.push(chars[start.min(chars.len())..end.min(chars.len())].iter().collect());
            let rect = span(word, start, end);
            match rects.last_mut() {
                Some((line, joined)) if *line == word.line => *joined = joined.union(&rect),
                _ => rects.push((word.line, rect)),
            }
        }
        let first: Vec<char> = words[0].text.chars().collect();
        let last: Vec<char> = words[count - 1].text.chars().collect();
        let prefix: String = first[..head.0.min(first.len())].iter().collect();
        let suffix: String = last[tail.1.min(last.len())..].iter().collect();
        let (before, after) = context(text, index, index + count - 1, &prefix, &suffix);
        hits.push(Hit {
            page,
            rects: rects.into_iter().map(|(_, rect)| rect).collect(),
            before,
            matched: parts.join(" "),
            after,
        });
    }
    hits
}

#[must_use]
pub fn find(page: usize, text: &PageText, query: &str, whole_word: bool, case: bool) -> Vec<Hit> {
    let tokens: Vec<String> =
        query.split_whitespace().map(|token| if case { token.to_string() } else { fold(token) }).collect();
    if tokens.len() > 1 {
        return find_phrase(page, text, &tokens, whole_word, case);
    }
    let Some(needle) = tokens.into_iter().next() else {
        return Vec::new();
    };
    let needle_chars = needle.chars().count();
    let mut hits = Vec::new();
    for (index, word) in text.words.iter().enumerate() {
        let (hay, origin) = folded_with_origin(&word.text, case);
        let Some(at) = hay.find(&needle) else {
            continue;
        };
        if whole_word && (at > 0 || hay.len() != needle.len()) {
            let trimmed = hay.trim_matches(|c: char| !c.is_alphanumeric());
            if trimmed != needle {
                continue;
            }
        }
        let chars: Vec<char> = word.text.chars().collect();
        let first = hay[..at].chars().count();
        let start = origin.get(first).copied().unwrap_or(0).min(chars.len());
        let end = origin.get(first + needle_chars - 1).map_or(chars.len(), |last| last + 1).clamp(start, chars.len());
        let rect = span(word, start, end);
        let prefix: String = chars[..start].iter().collect();
        let suffix: String = chars[end..].iter().collect();
        let (before, after) = context(text, index, index, &prefix, &suffix);
        hits.push(Hit { page, rects: vec![rect], before, matched: chars[start..end].iter().collect(), after });
    }
    hits
}

#[must_use]
pub fn selection(text: &PageText, from: (f32, f32), to: (f32, f32)) -> Vec<usize> {
    let index_at = |point: (f32, f32)| {
        text.words
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| distance(&a.rect, point).total_cmp(&distance(&b.rect, point)))
            .map(|(index, _)| index)
    };
    let (Some(a), Some(b)) = (index_at(from), index_at(to)) else {
        return Vec::new();
    };
    (a.min(b)..=a.max(b)).collect()
}

fn distance(rect: &Rect, (x, y): (f32, f32)) -> f32 {
    let dx = (rect.x - x).max(0.0).max(x - rect.right());
    let dy = (rect.y - y).max(0.0).max(y - rect.bottom());
    dx * dx + dy * dy * 4.0
}

#[must_use]
pub fn line_rects(text: &PageText, indices: &[usize]) -> Vec<Rect> {
    let mut out: Vec<(u32, Rect)> = Vec::new();
    for &index in indices {
        let Some(word) = text.words.get(index) else {
            continue;
        };
        match out.last_mut() {
            Some((line, rect)) if *line == word.line => *rect = rect.union(&word.rect),
            _ => out.push((word.line, word.rect)),
        }
    }
    out.into_iter().map(|(_, rect)| rect).collect()
}

#[must_use]
pub fn quote(text: &PageText, indices: &[usize]) -> String {
    indices
        .iter()
        .filter_map(|&index| text.words.get(index))
        .map(|word| word.text.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn glyph(text: &str, x: f32, baseline: f32) -> Glyph {
        Glyph { text: text.into(), x, baseline, size: 10.0, advance: None }
    }

    fn sample() -> PageText {
        let mut glyphs = Vec::new();
        let mut x = 10.0;
        for c in "Робочі простори".chars() {
            glyphs.push(glyph(&c.to_string(), x, 20.0));
            x += 6.0;
        }
        x = 10.0;
        for c in "Простір зникає".chars() {
            glyphs.push(glyph(&c.to_string(), x, 40.0));
            x += 6.0;
        }
        words(&glyphs, Vec::new())
    }

    #[test]
    fn glyphs_group_into_words_and_lines() {
        let text = sample();
        let names: Vec<&str> = text.words.iter().map(|w| w.text.as_str()).collect();
        assert_eq!(names, ["Робочі", "простори", "Простір", "зникає"]);
        assert_eq!(text.words[1].line, 0);
        assert_eq!(text.words[2].line, 1);
        assert_eq!(text.plain(), "Робочі простори\nПростір зникає");
    }

    #[test]
    fn search_ignores_case_and_gives_context() {
        let text = sample();
        let hits = find(6, &text, "простір", false, false);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].matched, "Простір");
        assert_eq!(hits[0].page, 6);
        let prefix = find(6, &text, "прост", false, false);
        assert_eq!(prefix.len(), 2);
        assert!(prefix[0].rects[0].w < text.words[1].rect.w);
        assert!(find(6, &text, "прост", true, false).is_empty());
        assert!(find(6, &text, "простір", false, true).is_empty());
    }

    #[test]
    fn a_match_after_a_letter_that_folds_into_two_maps_back_to_the_original_letters() {
        let mut glyphs = Vec::new();
        let mut x = 10.0;
        for c in "İZMİR".chars() {
            glyphs.push(glyph(&c.to_string(), x, 20.0));
            x += 6.0;
        }
        let text = words(&glyphs, Vec::new());
        let hits = find(0, &text, "r", false, false);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].matched, "R");
        assert_eq!(hits[0].before, "İZMİ");
        let whole = find(0, &text, "mi̇r", false, false);
        assert_eq!(whole[0].matched, "MİR");
    }

    #[test]
    fn a_phrase_is_found_across_words_and_lines() {
        let text = sample();
        let same_line = find(0, &text, "робочі  простори", false, false);
        assert_eq!(same_line.len(), 1);
        assert_eq!(same_line[0].matched, "Робочі простори");
        assert_eq!(same_line[0].rects.len(), 1);
        let across = find(0, &text, "простори простір", false, false);
        assert_eq!(across.len(), 1);
        assert_eq!(across[0].rects.len(), 2, "one box per line");
        assert_eq!(across[0].after, " зникає");
        let partial = find(0, &text, "очі прос", false, false);
        assert_eq!(partial[0].matched, "очі прос");
        assert_eq!(partial[0].before, "Роб");
        assert!(find(0, &text, "очі прос", true, false).is_empty());
        assert!(find(0, &text, "простори зникає", false, false).is_empty());
    }

    #[test]
    fn notes_are_searched_with_the_case_and_whole_word_switches() {
        assert!(mentions("Перевірити робочі простори", "робочі простори", false, false));
        assert!(mentions("Перевірити", "перевір", false, false));
        assert!(!mentions("Перевірити", "перевір", true, false));
        assert!(mentions("Перевірити, потім", "перевірити", true, false));
        assert!(!mentions("Перевірити", "перевірити", false, true));
    }

    #[test]
    fn a_drag_selects_every_word_between_its_ends() {
        let text = sample();
        let words = selection(&text, (12.0, 16.0), (40.0, 36.0));
        assert_eq!(words, vec![0, 1, 2]);
        assert_eq!(quote(&text, &words), "Робочі простори Простір");
        assert_eq!(line_rects(&text, &words).len(), 2);
    }
}

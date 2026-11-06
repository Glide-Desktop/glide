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

#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub page: usize,
    pub rects: Vec<Rect>,
    pub before: String,
    pub matched: String,
    pub after: String,
}

#[must_use]
pub fn find(page: usize, text: &PageText, query: &str, whole_word: bool, case: bool) -> Vec<Hit> {
    let needle = if case { query.trim().to_string() } else { fold(query.trim()) };
    if needle.is_empty() {
        return Vec::new();
    }
    let norm = |s: &str| if case { s.to_string() } else { fold(s) };
    let mut hits = Vec::new();
    for (index, word) in text.words.iter().enumerate() {
        let hay = norm(&word.text);
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
        let start = hay[..at].chars().count();
        let len = needle.chars().count().min(chars.len().saturating_sub(start));
        let share = |n: usize| word.rect.w * n as f32 / chars.len().max(1) as f32;
        let rect = Rect::new(word.rect.x + share(start), word.rect.y, share(len).max(2.0), word.rect.h);
        let before_words: Vec<&str> =
            text.words[index.saturating_sub(5)..index].iter().map(|w| w.text.as_str()).collect();
        let after_words: Vec<&str> = text.words.iter().skip(index + 1).take(6).map(|w| w.text.as_str()).collect();
        let prefix: String = chars[..start].iter().collect();
        let suffix: String = chars[start + len..].iter().collect();
        let mut before = before_words.join(" ");
        if !before.is_empty() {
            before.push(' ');
        }
        before.push_str(&prefix);
        let mut after = suffix;
        if !after_words.is_empty() {
            after.push(' ');
            after.push_str(&after_words.join(" "));
        }
        hits.push(Hit { page, rects: vec![rect], before, matched: chars[start..start + len].iter().collect(), after });
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
    fn a_drag_selects_every_word_between_its_ends() {
        let text = sample();
        let words = selection(&text, (12.0, 16.0), (40.0, 36.0));
        assert_eq!(words, vec![0, 1, 2]);
        assert_eq!(quote(&text, &words), "Робочі простори Простір");
        assert_eq!(line_rects(&text, &words).len(), 2);
    }
}

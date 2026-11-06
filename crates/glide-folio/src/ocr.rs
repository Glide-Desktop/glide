use crate::doc::{PageText, Rect, Word};

#[must_use]
pub fn parse_tsv(tsv: &str, scale: f32) -> PageText {
    let mut text = PageText::default();
    let mut lines: Vec<(u32, u32, u32)> = Vec::new();
    for row in tsv.lines().skip(1) {
        let cells: Vec<&str> = row.split('\t').collect();
        if cells.len() < 12 || cells[0] != "5" {
            continue;
        }
        let word = cells[11].trim();
        if word.is_empty() {
            continue;
        }
        let number = |i: usize| cells[i].parse::<u32>().unwrap_or(0);
        let key = (number(2), number(3), number(4));
        let line = lines.iter().position(|k| *k == key).unwrap_or_else(|| {
            lines.push(key);
            lines.len() - 1
        });
        let at = |i: usize| number(i) as f32 / scale;
        text.words.push(Word {
            text: word.to_string(),
            rect: Rect::new(at(6), at(7), at(8), at(9)),
            line: line as u32,
        });
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tesseract_words_become_positioned_text() {
        let tsv = "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext\n\
                   1\t1\t0\t0\t0\t0\t0\t0\t1000\t1400\t-1\t\n\
                   5\t1\t1\t1\t1\t1\t100\t200\t80\t30\t95\tЯк\n\
                   5\t1\t1\t1\t1\t2\t190\t200\t90\t30\t94\tумру,\n\
                   5\t1\t1\t1\t2\t1\t100\t250\t70\t30\t96\tто\n";
        let text = parse_tsv(tsv, 2.0);
        assert_eq!(text.plain(), "Як умру,\nто");
        assert_eq!(text.words[1].rect, Rect::new(95.0, 100.0, 45.0, 15.0));
    }
}

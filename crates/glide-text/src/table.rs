pub const ROWS: usize = 2000;

#[must_use]
pub fn parse(text: &str, delimiter: char) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cell = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            if c != '"' {
                cell.push(c);
            } else if chars.peek() == Some(&'"') {
                cell.push('"');
                chars.next();
            } else {
                quoted = false;
            }
            continue;
        }
        match c {
            '"' if cell.is_empty() => quoted = true,
            c if c == delimiter => row.push(std::mem::take(&mut cell)),
            '\r' => {}
            '\n' => {
                row.push(std::mem::take(&mut cell));
                push(&mut rows, std::mem::take(&mut row));
                if rows.len() >= ROWS {
                    return rows;
                }
            }
            c => cell.push(c),
        }
    }
    if !cell.is_empty() || !row.is_empty() {
        row.push(cell);
        push(&mut rows, row);
    }
    rows
}

fn push(rows: &mut Vec<Vec<String>>, row: Vec<String>) {
    if row.len() > 1 || row.first().is_some_and(|cell| !cell.is_empty()) {
        rows.push(row);
    }
}

#[must_use]
pub fn widths(rows: &[Vec<String>]) -> Vec<u16> {
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
    (0..columns)
        .map(|column| {
            let longest = rows
                .iter()
                .take(200)
                .filter_map(|row| row.get(column))
                .map(|cell| cell.chars().count())
                .max()
                .unwrap_or(0);
            u16::try_from(longest.clamp(4, 40)).unwrap_or(40)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cells(text: &str, delimiter: char) -> Vec<Vec<&'static str>> {
        parse(text, delimiter)
            .into_iter()
            .map(|row| row.into_iter().map(|cell| &*Box::leak(cell.into_boxed_str())).collect())
            .collect()
    }

    #[test]
    fn quoted_cells_keep_delimiters_quotes_and_line_breaks() {
        assert_eq!(
            cells("a,\"b,c\",\"say \"\"hi\"\"\"\r\n1,\"two\nlines\",3\n", ','),
            [vec!["a", "b,c", "say \"hi\""], vec!["1", "two\nlines", "3"]]
        );
    }

    #[test]
    fn tabs_blank_lines_and_a_missing_final_newline() {
        assert_eq!(cells("x\ty\n\n1\t2", '\t'), [vec!["x", "y"], vec!["1", "2"]]);
        assert_eq!(cells("lonely", ','), [vec!["lonely"]]);
        assert!(cells("", ',').is_empty());
        assert_eq!(cells("a,\n", ','), [vec!["a", ""]], "an empty last cell is still a cell");
    }

    #[test]
    fn widths_follow_the_longest_cell_within_bounds() {
        let rows = parse("id,name\n1,Ada Lovelace\n2,x\n", ',');
        assert_eq!(widths(&rows), [4, 12]);
        let long = parse(&format!("{}\n", "z".repeat(90)), ',');
        assert_eq!(widths(&long), [40]);
    }

    #[test]
    fn a_huge_file_is_cut_after_the_row_limit() {
        let text = "a,b\n".repeat(ROWS + 50);
        assert_eq!(parse(&text, ',').len(), ROWS);
    }
}

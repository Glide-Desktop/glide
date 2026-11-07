use std::path::Path;
use std::process::{Command, Stdio};

const WANTED: [&str; 2] = ["ukr", "eng"];

#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Lines(Vec<Line>),
    Missing,
    Failed,
}

#[must_use]
pub fn pick_languages(listed: &str) -> Option<String> {
    let available: Vec<&str> = listed.lines().skip(1).map(str::trim).collect();
    let picked: Vec<&str> = WANTED.into_iter().filter(|lang| available.contains(lang)).collect();
    (!picked.is_empty()).then(|| picked.join("+"))
}

#[must_use]
pub fn parse_tsv(tsv: &str, width: f32, height: f32) -> Vec<Line> {
    let mut keys: Vec<(u32, u32, u32)> = Vec::new();
    let mut lines: Vec<(Vec<String>, f32, f32, f32, f32)> = Vec::new();
    for row in tsv.lines().skip(1) {
        let cells: Vec<&str> = row.split('\t').collect();
        if cells.len() < 12 || cells[0] != "5" {
            continue;
        }
        let word = cells[11].trim();
        let confidence = cells[10].parse::<f32>().unwrap_or(0.0);
        if word.is_empty() || confidence < 40.0 {
            continue;
        }
        let number = |index: usize| cells[index].parse::<f32>().unwrap_or(0.0);
        let key = (number(2) as u32, number(3) as u32, number(4) as u32);
        let (left, top, right, bottom) = (number(6), number(7), number(6) + number(8), number(7) + number(9));
        if let Some(index) = keys.iter().position(|known| *known == key) {
            let line = &mut lines[index];
            line.0.push(word.to_string());
            line.1 = line.1.min(left);
            line.2 = line.2.min(top);
            line.3 = line.3.max(right);
            line.4 = line.4.max(bottom);
        } else {
            keys.push(key);
            lines.push((vec![word.to_string()], left, top, right, bottom));
        }
    }
    let (width, height) = (width.max(1.0), height.max(1.0));
    lines
        .into_iter()
        .map(|(words, left, top, right, bottom)| Line {
            text: words.join(" "),
            x: left / width,
            y: top / height,
            width: (right - left) / width,
            height: (bottom - top) / height,
        })
        .collect()
}

#[must_use]
pub fn recognise(png: &Path, width: u32, height: u32) -> Outcome {
    let Some(tool) = which("tesseract") else { return Outcome::Missing };
    let languages = Command::new(&tool)
        .arg("--list-langs")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .and_then(|output| pick_languages(&String::from_utf8_lossy(&output.stdout)));
    let mut command = Command::new(&tool);
    command.arg(png).arg("stdout");
    if let Some(languages) = languages {
        command.args(["-l", &languages]);
    }
    command.arg("tsv").stdin(Stdio::null()).stderr(Stdio::null());
    match command.output() {
        Ok(output) if output.status.success() => {
            Outcome::Lines(parse_tsv(&String::from_utf8_lossy(&output.stdout), width as f32, height as f32))
        }
        _ => Outcome::Failed,
    }
}

fn which(program: &str) -> Option<std::path::PathBuf> {
    std::env::var_os("PATH")?
        .to_str()?
        .split(':')
        .map(|dir| Path::new(dir).join(program))
        .find(|candidate| candidate.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_of_a_line_merge_into_one_box_in_frame_fractions() {
        let tsv = "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext\n\
                   5\t1\t1\t1\t1\t1\t100\t200\t80\t30\t95\tОзеро\n\
                   5\t1\t1\t1\t1\t2\t190\t205\t120\t28\t94\tНесамовите\n\
                   5\t1\t1\t1\t2\t1\t100\t250\t70\t20\t12\tшум\n\
                   5\t1\t1\t1\t3\t1\t100\t280\t60\t20\t91\t1750\n";
        let lines = parse_tsv(tsv, 1000.0, 500.0);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text, "Озеро Несамовите");
        assert!((lines[0].x - 0.1).abs() < 1e-6);
        assert!((lines[0].width - 0.21).abs() < 1e-6);
        assert!((lines[0].height - 0.066).abs() < 1e-6);
        assert_eq!(lines[1].text, "1750");
    }
}

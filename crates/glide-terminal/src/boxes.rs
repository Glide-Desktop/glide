use iced::advanced::renderer::{Quad, Renderer as _};
use iced::{Color, Rectangle, Renderer};

const LIGHT: u8 = 1;
const HEAVY: u8 = 2;
const DOUBLE: u8 = 3;

fn arms(c: char) -> Option<[u8; 4]> {
    let (l, h, d) = (LIGHT, HEAVY, DOUBLE);
    Some(match c {
        '─' | '┄' | '┈' | '╌' => [0, l, 0, l],
        '━' | '┅' | '┉' | '╍' => [0, h, 0, h],
        '│' | '┆' | '┊' | '╎' => [l, 0, l, 0],
        '┃' | '┇' | '┋' | '╏' => [h, 0, h, 0],
        '┌' | '╭' => [0, l, l, 0],
        '┐' | '╮' => [0, 0, l, l],
        '└' | '╰' => [l, l, 0, 0],
        '┘' | '╯' => [l, 0, 0, l],
        '├' => [l, l, l, 0],
        '┤' => [l, 0, l, l],
        '┬' => [0, l, l, l],
        '┴' => [l, l, 0, l],
        '┼' => [l, l, l, l],
        '┏' => [0, h, h, 0],
        '┓' => [0, 0, h, h],
        '┗' => [h, h, 0, 0],
        '┛' => [h, 0, 0, h],
        '┣' => [h, h, h, 0],
        '┫' => [h, 0, h, h],
        '┳' => [0, h, h, h],
        '┻' => [h, h, 0, h],
        '╋' => [h, h, h, h],
        '═' => [0, d, 0, d],
        '║' => [d, 0, d, 0],
        '╔' => [0, d, d, 0],
        '╗' => [0, 0, d, d],
        '╚' => [d, d, 0, 0],
        '╝' => [d, 0, 0, d],
        '╠' => [d, d, d, 0],
        '╣' => [d, 0, d, d],
        '╦' => [0, d, d, d],
        '╩' => [d, d, 0, d],
        '╬' => [d, d, d, d],
        '╴' => [0, 0, 0, l],
        '╵' => [l, 0, 0, 0],
        '╶' => [0, l, 0, 0],
        '╷' => [0, 0, l, 0],
        '╸' => [0, 0, 0, h],
        '╹' => [h, 0, 0, 0],
        '╺' => [0, h, 0, 0],
        '╻' => [0, 0, h, 0],
        _ => return None,
    })
}

fn fill(renderer: &mut Renderer, x: f32, y: f32, width: f32, height: f32, color: Color) {
    if width > 0.0 && height > 0.0 {
        renderer.fill_quad(Quad { bounds: Rectangle { x, y, width, height }, ..Quad::default() }, color);
    }
}

fn segments(arms: [u8; 4], cell: Rectangle) -> Vec<Rectangle> {
    let stroke = (cell.width / 8.0).round().max(1.0);
    let cx = (cell.x + cell.width / 2.0 - stroke / 2.0).round();
    let cy = (cell.y + cell.height / 2.0 - stroke / 2.0).round();
    let (right, bottom) = (cell.x + cell.width, cell.y + cell.height);
    let [up, east, down, west] = arms;
    let thickness = |weight: u8| if weight == HEAVY { stroke * 2.0 } else { stroke };
    let offsets = |weight: u8| match weight {
        0 => Vec::new(),
        DOUBLE => vec![-stroke, stroke],
        _ => vec![0.0],
    };
    let reach = |weight: u8| if weight == DOUBLE { stroke } else { 0.0 };
    let shift = |weight: u8| (thickness(weight) - stroke) / 2.0;
    let mut out = Vec::new();
    for offset in offsets(east) {
        let x = cx - reach(east);
        out.push(Rectangle { x, y: cy + offset - shift(east), width: right - x, height: thickness(east) });
    }
    for offset in offsets(west) {
        let width = cx - cell.x + stroke + reach(west);
        out.push(Rectangle { x: cell.x, y: cy + offset - shift(west), width, height: thickness(west) });
    }
    for offset in offsets(up) {
        let height = cy - cell.y + stroke + reach(up);
        out.push(Rectangle { x: cx + offset - shift(up), y: cell.y, width: thickness(up), height });
    }
    for offset in offsets(down) {
        let y = cy - reach(down);
        out.push(Rectangle { x: cx + offset - shift(down), y, width: thickness(down), height: bottom - y });
    }
    out
}

fn eighths(renderer: &mut Renderer, glyph: char, cell: Rectangle, color: Color) -> bool {
    let Rectangle { x, y, width, height } = cell;
    let part = |total: f32, eighths: u32| (total * eighths as f32 / 8.0).round();
    match glyph {
        '▀' => fill(renderer, x, y, width, part(height, 4), color),
        '▁'..='█' => {
            let filled = part(height, glyph as u32 - '▀' as u32);
            fill(renderer, x, y + height - filled, width, filled, color);
        }
        '▉'..='▏' => fill(renderer, x, y, part(width, '▏' as u32 - glyph as u32 + 1), height, color),
        '▐' => fill(renderer, x + part(width, 4), y, width - part(width, 4), height, color),
        '░' | '▒' | '▓' => {
            let alpha = (glyph as u32 - '░' as u32 + 1) as f32 * 0.25;
            fill(renderer, x, y, width, height, Color { a: color.a * alpha, ..color });
        }
        '▔' => fill(renderer, x, y, width, part(height, 1), color),
        '▕' => fill(renderer, x + width - part(width, 1), y, part(width, 1), height, color),
        _ => return false,
    }
    true
}

fn quadrants(renderer: &mut Renderer, c: char, cell: Rectangle, color: Color) -> bool {
    const BITS: [u8; 10] = [4, 8, 1, 13, 9, 7, 11, 2, 6, 14];
    let Some(index) = (c as u32).checked_sub('▖' as u32).filter(|index| *index < 10) else { return false };
    let half_w = (cell.width / 2.0).round();
    let half_h = (cell.height / 2.0).round();
    let bits = BITS[index as usize];
    let spans = [
        (1, cell.x, cell.y, half_w, half_h),
        (2, cell.x + half_w, cell.y, cell.width - half_w, half_h),
        (4, cell.x, cell.y + half_h, half_w, cell.height - half_h),
        (8, cell.x + half_w, cell.y + half_h, cell.width - half_w, cell.height - half_h),
    ];
    for (bit, x, y, w, h) in spans {
        if bits & bit != 0 {
            fill(renderer, x, y, w, h, color);
        }
    }
    true
}

pub fn draw(renderer: &mut Renderer, c: char, cell: Rectangle, color: Color) -> bool {
    if let Some(arms) = arms(c) {
        for segment in segments(arms, cell) {
            fill(renderer, segment.x, segment.y, segment.width, segment.height, color);
        }
        return true;
    }
    eighths(renderer, c, cell, color) || quadrants(renderer, c, cell, color)
}

#[must_use]
pub fn handles(c: char) -> bool {
    arms(c).is_some() || ('\u{2580}'..='\u{259f}').contains(&c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_block_element_is_drawn_without_the_font() {
        assert!(('\u{2580}'..='\u{259f}').all(handles));
        assert!(['─', '│', '┌', '╋', '═', '╬', '╭', '╯'].into_iter().all(handles));
        assert!(!handles('a') && !handles('╒'));
    }

    #[test]
    fn straight_lines_only_cover_their_own_axis_and_span_the_cell() {
        let cell = Rectangle { x: 10.0, y: 20.0, width: 8.0, height: 16.0 };
        let horizontal = segments(arms('─').unwrap(), cell);
        assert_eq!(horizontal.len(), 2);
        assert!(horizontal.iter().all(|segment| segment.height == 1.0));
        let left = horizontal.iter().map(|segment| segment.x).fold(f32::MAX, f32::min);
        let right = horizontal.iter().map(|segment| segment.x + segment.width).fold(f32::MIN, f32::max);
        assert_eq!((left, right), (cell.x, cell.x + cell.width));
        let vertical = segments(arms('│').unwrap(), cell);
        assert!(vertical.iter().all(|segment| segment.width == 1.0));
        assert_eq!(segments(arms('═').unwrap(), cell).len(), 4);
    }

    #[test]
    fn opposite_corners_mirror_each_other() {
        let flip = |[up, east, down, west]: [u8; 4]| [down, west, up, east];
        for (a, b) in [('┌', '┘'), ('┐', '└'), ('╔', '╝'), ('┏', '┛'), ('├', '┤'), ('┬', '┴')] {
            assert_eq!(flip(arms(a).unwrap()), arms(b).unwrap(), "{a} {b}");
        }
    }
}

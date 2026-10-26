use iced::advanced::image::Renderer as _;
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{Tree, Widget};
use iced::{Color, Element, Length, Rectangle, Renderer, Size, Theme, mouse};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Plus,
    More,
    Bell,
    Close,
    Search,
    Up,
    Down,
    Sidebar,
    Folder,
    File,
    Right,
    Menu,
    Eye,
    Refresh,
    Check,
    Warning,
    Copy,
    EyeOff,
    Link,
    Shield,
    Server,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Maximize,
    Restore,
    WindowClose,
    Close,
    Search,
    Up,
    Down,
}

const SCALE: i32 = 4;
type Cache = Vec<((Kind, u16, [u8; 4]), iced::widget::image::Handle)>;

#[must_use]
pub fn glyph<'a, Message: 'a>(icon: Icon, side: f32, tint: Color) -> Element<'a, Message> {
    let kind = match icon {
        Icon::Close => Kind::Close,
        Icon::Search => Kind::Search,
        Icon::Up => Kind::Up,
        Icon::Down => Kind::Down,
        _ => {
            let body = match icon {
                Icon::Plus => "<path d='M12 5v14M5 12h14'/>",
                Icon::More => {
                    "<circle cx='5' cy='12' r='1' fill='currentColor'/><circle cx='12' cy='12' r='1' fill='currentColor'/><circle cx='19' cy='12' r='1' fill='currentColor'/>"
                }
                Icon::Bell => "<path d='M6 16V11a6 6 0 0 1 12 0v5l1.5 2h-15z'/><path d='M10 20.5a2 2 0 0 0 4 0'/>",
                Icon::Sidebar => "<rect x='3.5' y='4.5' width='17' height='15' rx='2.5'/><path d='M9.5 4.5v15'/>",
                Icon::Folder => {
                    "<path d='M3.5 7.5a2 2 0 0 1 2-2h4l2 2h7a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2z'/>"
                }
                Icon::File => {
                    "<path d='M7 3.5h7l4.5 4.5v11a1.5 1.5 0 0 1-1.5 1.5H7A1.5 1.5 0 0 1 5.5 19V5A1.5 1.5 0 0 1 7 3.5z'/><path d='M14 3.5V8h4.5'/>"
                }
                Icon::Right => "<path d='M9.5 6l6 6-6 6'/>",
                Icon::Menu => "<path d='M4.5 7h15M4.5 12h15M4.5 17h15'/>",
                Icon::Eye => {
                    "<path d='M2.5 12s3.5-6.5 9.5-6.5S21.5 12 21.5 12s-3.5 6.5-9.5 6.5S2.5 12 2.5 12z'/><circle cx='12' cy='12' r='2.75'/>"
                }
                Icon::Refresh => "<path d='M19.5 12a7.5 7.5 0 1 1-2.2-5.3'/><path d='M19.5 4.5v4h-4'/>",
                Icon::Check => "<path d='M5 12.5l4.5 4.5L19 7.5'/>",
                Icon::Warning => "<path d='M12 4.5l8.5 15h-17z'/><path d='M12 10v4.5M12 17v.5'/>",
                Icon::Copy => "<rect x='9' y='9' width='11' height='11' rx='2'/><path d='M5 15V6a2 2 0 0 1 2-2h8'/>",
                Icon::EyeOff => {
                    "<path d='M3 3l18 18'/><path d='M10.6 5.6A9.6 9.6 0 0 1 12 5.5c6 0 9.5 6.5 9.5 6.5a16 16 0 0 1-3 3.8'/><path d='M6.6 6.9C4 8.6 2.5 12 2.5 12s3.5 6.5 9.5 6.5a9 9 0 0 0 5.1-1.5'/><path d='M9.9 9.9a2.75 2.75 0 0 0 4.2 4.2'/>"
                }
                Icon::Link => {
                    "<path d='M10 14a5 5 0 0 0 7 0l3-3a5 5 0 0 0-7-7l-1 1'/><path d='M14 10a5 5 0 0 0-7 0l-3 3a5 5 0 0 0 7 7l1-1'/>"
                }
                Icon::Shield => {
                    "<path d='M12 3l7 3v5c0 4.6-3 8.4-7 10-4-1.6-7-5.4-7-10V6z'/><path d='M9.5 12l1.8 1.8 3.4-3.6'/>"
                }
                Icon::Server => {
                    "<rect x='3.5' y='4' width='17' height='7' rx='2'/><rect x='3.5' y='13' width='17' height='7' rx='2'/><path d='M7.5 7.5h.01M7.5 16.5h.01'/>"
                }
                _ => unreachable!(),
            };
            let svg = format!(
                "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='currentColor' stroke-width='1.75' stroke-linecap='round' stroke-linejoin='round'>{body}</svg>"
            );
            return iced::widget::svg(iced::widget::svg::Handle::from_memory(svg.into_bytes()))
                .width(side)
                .height(side)
                .opacity(tint.a)
                .style(move |_, _| iced::widget::svg::Style { color: Some(Color { a: 1.0, ..tint }) })
                .into();
        }
    };
    Element::new(Glyph { kind, side, tint: Some(tint) })
}

#[must_use]
pub fn window_control<'a, Message: 'a>(maximized: bool) -> Element<'a, Message> {
    Element::new(Glyph {
        kind: if maximized { Kind::Restore } else { Kind::Maximize },
        side: glide_theme::component::SMALL_CONTROL,
        tint: None,
    })
}

#[must_use]
pub fn window_close<'a, Message: 'a>() -> Element<'a, Message> {
    Element::new(Glyph { kind: Kind::WindowClose, side: glide_theme::component::SMALL_CONTROL, tint: None })
}

fn mask(kind: Kind, side: u16) -> Vec<u8> {
    use glide_paint::Painter;
    let side = i32::from(side);
    let mut bytes = vec![0; (side * SCALE) as usize * (side * SCALE) as usize * 4];
    let white = glide_theme::Color([1.0; 4]);
    let clear = glide_theme::Color([0.0; 4]);
    match kind {
        Kind::Maximize => Painter::draw_icon_maximize(&mut bytes, side, side, SCALE, white, clear),
        Kind::Restore => {
            Painter::draw_icon_restore(&mut bytes, side, side, SCALE, white, glide_theme::Color([0.0, 0.0, 0.0, 1.0]));
            for pixel in bytes.chunks_exact_mut(4) {
                pixel.fill(pixel[0]);
            }
        }
        Kind::WindowClose => Painter::draw_icon_close(&mut bytes, side, side, SCALE, white, clear),
        Kind::Close => {
            glide_paint::draw_close_glyph_over(&mut bytes, side, side, side as f32, side as f32 * 0.075, SCALE, white);
        }
        Kind::Search => glide_paint::draw_search_glyph_over(
            &mut bytes,
            side,
            side,
            side as f32 / 2.0,
            side as f32 / 2.0,
            side as f32 * 0.8,
            side as f32 * 0.075,
            SCALE,
            white,
        ),
        Kind::Up | Kind::Down => glide_paint::draw_chevron_glyph_over(
            &mut bytes,
            side,
            side,
            side as f32,
            side as f32 * 0.075,
            kind == Kind::Up,
            SCALE,
            white,
        ),
    }
    bytes
}

fn image(kind: Kind, side: f32, color: Color) -> iced::widget::image::Handle {
    static CACHE: std::sync::Mutex<Cache> = std::sync::Mutex::new(Vec::new());
    let side = side.ceil().clamp(1.0, 256.0) as u16;
    let rgba = [color.r, color.g, color.b, color.a].map(|channel| (channel.clamp(0.0, 1.0) * 255.0).round() as u8);
    let mut cache = CACHE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some((_, handle)) = cache.iter().find(|(key, _)| *key == (kind, side, rgba)) {
        return handle.clone();
    }
    let mut bytes = mask(kind, side);
    for pixel in bytes.chunks_exact_mut(4) {
        let alpha = ((u16::from(pixel[3]) * u16::from(rgba[3]) + 127) / 255) as u8;
        pixel.copy_from_slice(&[rgba[0], rgba[1], rgba[2], alpha]);
    }
    let pixels = u32::from(side) * SCALE as u32;
    let handle = iced::widget::image::Handle::from_rgba(pixels, pixels, bytes);
    if cache.len() >= 128 {
        cache.clear();
    }
    cache.push(((kind, side, rgba), handle.clone()));
    handle
}

struct Glyph {
    kind: Kind,
    side: f32,
    tint: Option<Color>,
}

impl<Message> Widget<Message, Theme, Renderer> for Glyph {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(self.side), Length::Fixed(self.side))
    }
    fn layout(&mut self, _: &mut Tree, _: &Renderer, limits: &layout::Limits) -> layout::Node {
        layout::Node::new(limits.resolve(
            Length::Fixed(self.side),
            Length::Fixed(self.side),
            Size::new(self.side, self.side),
        ))
    }
    fn draw(
        &self, _: &Tree, renderer: &mut Renderer, _: &Theme, style: &renderer::Style, layout: Layout<'_>,
        _: mouse::Cursor, viewport: &Rectangle,
    ) {
        renderer.draw_image(
            iced::advanced::image::Image::new(image(self.kind, self.side, self.tint.unwrap_or(style.text_color))),
            layout.bounds(),
            *viewport,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn window_controls_match_the_shared_painter_exactly() {
        let side = glide_theme::component::SMALL_CONTROL as i32;
        for kind in [Kind::Maximize, Kind::Restore, Kind::WindowClose] {
            let mut expected = vec![0; (side * SCALE * side * SCALE * 4) as usize];
            let draw = match kind {
                Kind::Maximize => glide_paint::Painter::draw_icon_maximize,
                Kind::Restore => glide_paint::Painter::draw_icon_restore,
                _ => glide_paint::Painter::draw_icon_close,
            };
            let bg = if kind == Kind::Restore { [0.0, 0.0, 0.0, 1.0] } else { [0.0; 4] };
            draw(&mut expected, side, side, SCALE, glide_theme::Color([1.0; 4]), glide_theme::Color(bg));
            if kind == Kind::Restore {
                for pixel in expected.chunks_exact_mut(4) {
                    pixel.fill(pixel[0]);
                }
            }
            assert_eq!(mask(kind, side as u16), expected);
        }
        assert_ne!(mask(Kind::Maximize, 28), mask(Kind::Restore, 28));
    }

    #[test]
    fn restore_mask_hides_the_back_square_inside_the_front_square() {
        let side = glide_theme::component::SMALL_CONTROL as i32;
        let pixels = side * SCALE;
        let mut see_through = vec![0; (pixels * pixels * 4) as usize];
        glide_paint::Painter::draw_icon_restore(
            &mut see_through,
            side,
            side,
            SCALE,
            glide_theme::Color([1.0; 4]),
            glide_theme::Color([0.0; 4]),
        );
        let mask = mask(Kind::Restore, side as u16);
        let glyph = side as f32 / 2.0;
        let (near, far) = (side as f32 / 2.0 - glyph / 2.0, side as f32 / 2.0 + glyph / 2.0);
        let offset = glyph * 0.28;
        let (back_left, back_bottom) = (near + offset, far - offset);
        let middle = back_left.midpoint(back_bottom);
        let alpha = |bytes: &[u8], x: f32, y: f32| {
            let (px, py) = ((x * SCALE as f32) as i32, (y * SCALE as f32) as i32);
            bytes[((py * pixels + px) * 4 + 3) as usize]
        };
        for (x, y) in [(back_left, middle), (middle, back_bottom)] {
            assert!(alpha(&see_through, x, y) > 0);
            assert_eq!(alpha(&mask, x, y), 0);
        }
    }
}

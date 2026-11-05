use std::rc::Rc;

use glide_iced::Paint;
use glide_iced::style::{R2, R3, SP1, SP2};
use glide_theme::TextStyle;
use glide_theme::shape::R1;
use iced::widget::canvas::{self, Action, Frame, Geometry, LineCap, Path, Stroke};
use iced::widget::{Space, canvas as surface, column, container, image, mouse_area, row, stack, svg};
use iced::{
    Alignment, Background, Border, Color, ContentFit, Element, Event, Fill, Length, Point, Rectangle, Renderer, Shadow,
    Size, Theme, Vector, mouse,
};

use super::{slider, text, tooltip};

pub const GLASS: Color = Color { r: 24.0 / 255.0, g: 23.0 / 255.0, b: 27.0 / 255.0, a: 0.64 };
pub const GLASS_INK: Color = Color { r: 242.0 / 255.0, g: 240.0 / 255.0, b: 237.0 / 255.0, a: 1.0 };
pub const STAGE: Color = Color { r: 18.0 / 255.0, g: 17.0 / 255.0, b: 20.0 / 255.0, a: 1.0 };
pub const GLASS_BUTTON: f32 = 34.0;
pub const FILM_THUMB: f32 = 44.0;
pub const FILM_CURRENT: f32 = 60.0;
pub const HISTOGRAM: f32 = 52.0;
pub const DIAL_WIDTH: f32 = 460.0;
pub const DIAL_HEIGHT: f32 = 48.0;

const HEART: &[u8] = b"<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='white' stroke='white' stroke-width='2' stroke-linejoin='round'><path d='M12 20s-7-4.4-7-10a4 4 0 0 1 7-2.6A4 4 0 0 1 19 10c0 5.6-7 10-7 10z'/></svg>";
const CHECK: &[u8] = b"<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='white' stroke-width='3.2' stroke-linecap='round' stroke-linejoin='round'><path d='m5 12 5 5 9-10'/></svg>";

fn scrim() -> Color {
    Color { r: 17.0 / 255.0, g: 16.0 / 255.0, b: 18.0 / 255.0, a: 0.58 }
}

#[must_use]
pub fn glass_style() -> container::Style {
    container::Style {
        text_color: Some(GLASS_INK),
        background: Some(Background::Color(GLASS)),
        border: Border { color: Color { a: 0.12, ..Color::WHITE }, width: 1.0, radius: R3.into() },
        shadow: Shadow { color: Color { a: 0.4, ..Color::BLACK }, offset: Vector::new(0.0, 8.0), blur_radius: 24.0 },
        snap: true,
    }
}

#[must_use]
pub fn glass<'a, M: 'a>(content: impl Into<Element<'a, M>>) -> container::Container<'a, M> {
    container(content).padding(SP1).style(|_| glass_style())
}

#[must_use]
pub fn glass_circle<'a, M: Clone + 'a>(
    paint: &Paint, glyph: impl super::Glyph + 'a, label: &'a str, message: M,
) -> Element<'a, M> {
    let face = container(glyph.draw(16.0, GLASS_INK)).center(40.0).style(|_| container::Style {
        border: Border { radius: 20.0.into(), ..glass_style().border },
        ..glass_style()
    });
    tooltip(paint, mouse_area(face).on_press(message).interaction(mouse::Interaction::Pointer), label)
}

#[must_use]
pub fn glass_button<'a, M: Clone + 'a>(
    paint: &Paint, content: impl Into<Element<'a, M>>, label: &'a str, message: Option<M>,
) -> Element<'a, M> {
    let enabled = message.is_some();
    let face =
        iced::widget::button(container(content).center_y(GLASS_BUTTON).center_x(Length::Shrink).padding([0.0, 9.0]))
            .padding(0)
            .height(GLASS_BUTTON)
            .on_press_maybe(message)
            .style(move |_, status| {
                let layer = match status {
                    iced::widget::button::Status::Hovered => 0.10,
                    iced::widget::button::Status::Pressed => 0.16,
                    _ => 0.0,
                };
                iced::widget::button::Style {
                    background: (layer > 0.0).then_some(Background::Color(Color { a: layer, ..Color::WHITE })),
                    text_color: if enabled { GLASS_INK } else { Color { a: 0.42, ..GLASS_INK } },
                    border: Border { radius: R2.into(), ..Border::default() },
                    shadow: Shadow::default(),
                    snap: true,
                }
            });
    if label.is_empty() { face.into() } else { tooltip(paint, face, label) }
}

#[must_use]
pub fn glass_icon<'a, M: Clone + 'a>(
    paint: &Paint, glyph: impl super::Glyph + 'a, label: &'a str, message: Option<M>,
) -> Element<'a, M> {
    let ink = if message.is_some() { GLASS_INK } else { Color { a: 0.42, ..GLASS_INK } };
    glass_button(paint, glyph.draw(16.0, ink), label, message)
}

#[must_use]
pub fn glass_label<'a, M: 'a>(label: impl Into<String>, width: f32) -> Element<'a, M> {
    container(text(label.into(), TextStyle { size_px: 12.0, line_height_px: 16.0, ..TextStyle::NUM }).color(GLASS_INK))
        .center_x(width)
        .center_y(GLASS_BUTTON)
        .into()
}

#[must_use]
pub fn glass_divider<'a, M: 'a>() -> Element<'a, M> {
    container(Space::new().width(1).height(18))
        .padding([0.0, SP1])
        .style(|_| container::Style {
            background: Some(Background::Color(Color { a: 0.16, ..Color::WHITE })),
            ..container::Style::default()
        })
        .into()
}

#[must_use]
pub fn format_badge<'a, M: 'a>(label: &str) -> Element<'a, M> {
    container(
        text(label.to_uppercase(), TextStyle { size_px: 10.0, line_height_px: 14.0, ..TextStyle::MICRO })
            .color(Color::WHITE),
    )
    .padding([2.0, 6.0])
    .style(|_| container::Style {
        background: Some(Background::Color(scrim())),
        border: Border { radius: R1.into(), ..Border::default() },
        ..container::Style::default()
    })
    .into()
}

pub struct Tile<'a, M> {
    pub picture: Option<image::Handle>,
    pub side: f32,
    pub badge: Option<String>,
    pub favourite: bool,
    pub picked: Option<bool>,
    pub on_press: Option<M>,
    pub on_right: Option<M>,
    pub label: &'a str,
}

#[must_use]
pub fn photo_tile<'a, M: Clone + 'a>(paint: &Paint, tile: Tile<'a, M>) -> Element<'a, M> {
    let picked = tile.picked == Some(true);
    let side = tile.side;
    let inner = if picked { (side * 0.92).floor() } else { side };
    let radius = if picked { R2 } else { R1 };
    let well = paint.fill_2();
    let face: Element<'a, M> = match tile.picture {
        Some(handle) => {
            image(handle).width(inner).height(inner).content_fit(ContentFit::Cover).border_radius(radius).into()
        }
        None => container(Space::new())
            .width(inner)
            .height(inner)
            .style(move |_| glide_iced::style::rounded(well, radius))
            .into(),
    };
    let soft = paint.acc_soft();
    let ground = container(face)
        .center(side)
        .style(move |_| if picked { glide_iced::style::rounded(soft, R1) } else { container::Style::default() });
    let mut layers: Vec<Element<'a, M>> = vec![ground.into()];
    let edge = if picked { (side - inner) / 2.0 + 6.0 } else { 6.0 };
    if let Some(badge) = tile.badge {
        layers.push(container(format_badge(&badge)).padding(edge).align_left(Fill).align_bottom(Fill).into());
    }
    if tile.favourite {
        let heart = svg(svg::Handle::from_memory(HEART)).width(14).height(14);
        layers.push(container(heart).padding(edge).align_right(Fill).align_bottom(Fill).into());
    }
    if let Some(on) = tile.picked {
        let accent = paint.acc_bg();
        let mark: Element<'a, M> =
            if on { svg(svg::Handle::from_memory(CHECK)).width(12).height(12).into() } else { Space::new().into() };
        let disc = container(mark).center(22.0).style(move |_| container::Style {
            background: Some(Background::Color(if on { accent } else { Color { a: 0.25, ..scrim() } })),
            border: Border { color: Color { a: 0.95, ..Color::WHITE }, width: 1.5, radius: 11.0.into() },
            shadow: Shadow {
                color: Color { a: 0.35, ..Color::BLACK },
                offset: Vector::new(0.0, 1.0),
                blur_radius: 3.0,
            },
            ..container::Style::default()
        });
        layers.push(container(disc).padding(edge + 1.0).align_right(Fill).align_top(Fill).into());
    }
    let body = stack(layers).width(side).height(side);
    let mut area = mouse_area(body).interaction(mouse::Interaction::Pointer);
    if let Some(message) = tile.on_press {
        area = area.on_press(message);
    }
    if let Some(message) = tile.on_right {
        area = area.on_right_press(message);
    }
    let _ = tile.label;
    area.into()
}

#[must_use]
pub fn film_thumb<'a, M: Clone + 'a>(picture: Option<image::Handle>, current: bool, message: M) -> Element<'a, M> {
    let width = if current { FILM_CURRENT } else { FILM_THUMB };
    let opacity: f32 = if current { 1.0 } else { 0.6 };
    let face: Element<'a, M> = match picture {
        Some(handle) => image(handle)
            .width(width)
            .height(FILM_THUMB)
            .content_fit(ContentFit::Cover)
            .border_radius(R1)
            .opacity(opacity)
            .into(),
        None => container(Space::new())
            .width(width)
            .height(FILM_THUMB)
            .style(|_| glide_iced::style::rounded(Color { a: 0.12, ..Color::WHITE }, R1))
            .into(),
    };
    let ring = container(face).padding(if current { 2.0 } else { 0.0 }).style(move |_| container::Style {
        border: Border {
            color: if current { Color::WHITE } else { Color::TRANSPARENT },
            width: 2.0,
            radius: (R1 + 2.0).into(),
        },
        ..container::Style::default()
    });
    mouse_area(ring).on_press(message).interaction(mouse::Interaction::Pointer).into()
}

pub struct CoverCard<M> {
    pub pictures: Vec<Option<image::Handle>>,
    pub width: f32,
    pub height: f32,
    pub title: String,
    pub subtitle: String,
    pub overlay: bool,
    pub on_press: Option<M>,
    pub on_right: Option<M>,
}

fn cover_picture<'a, M: 'a>(paint: &Paint, picture: Option<image::Handle>, width: f32, height: f32) -> Element<'a, M> {
    let well = paint.fill_2();
    match picture {
        Some(handle) => image(handle).width(width).height(height).content_fit(ContentFit::Cover).into(),
        None => container(Space::new())
            .width(width)
            .height(height)
            .style(move |_| glide_iced::style::rounded(well, 0.0))
            .into(),
    }
}

#[must_use]
pub fn cover_card<'a, M: Clone + 'a>(paint: &Paint, card: CoverCard<M>) -> Element<'a, M> {
    let (width, height) = (card.width, card.height);
    let art: Element<'a, M> = if card.pictures.len() >= 4 {
        let half_w = (width - 2.0) / 2.0;
        let half_h = (height - 2.0) / 2.0;
        let mut pictures = card.pictures.into_iter();
        let mut cell = || cover_picture(paint, pictures.next().flatten(), half_w, half_h);
        column![row![cell(), cell()].spacing(2), row![cell(), cell()].spacing(2)].spacing(2).into()
    } else {
        cover_picture(paint, card.pictures.into_iter().next().flatten(), width, height)
    };
    let shadow = glide_theme::elevation::layers(glide_theme::elevation::Level::El1, paint.tokens.appearance);
    let shadow = shadow.first().map_or_else(Shadow::default, |layer| Shadow {
        color: glide_iced::style::to_iced(layer.color),
        offset: Vector::new(layer.dx, layer.dy),
        blur_radius: layer.blur,
    });
    let fill = paint.fill_2();
    let mut layers: Vec<Element<'a, M>> = vec![art];
    if card.overlay {
        let shade = container(
            row![
                text(card.title.clone(), TextStyle::T1).color(Color::WHITE),
                Space::new().width(Fill),
                text(card.subtitle.clone(), TextStyle::CALL).color(Color::WHITE),
            ]
            .align_y(Alignment::End),
        )
        .padding([14.0, 18.0])
        .width(Fill)
        .style(|_| container::Style {
            background: Some(Background::Gradient(iced::Gradient::Linear(
                iced::gradient::Linear::new(std::f32::consts::PI)
                    .add_stop(0.0, Color::TRANSPARENT)
                    .add_stop(1.0, Color { a: 0.62, ..scrim() }),
            ))),
            ..container::Style::default()
        });
        layers.push(container(shade).align_bottom(Fill).into());
    }
    let cover = container(stack(layers).width(width).height(height)).clip(true).style(move |_| container::Style {
        background: Some(Background::Color(fill)),
        border: Border { radius: R3.into(), ..Border::default() },
        shadow,
        ..container::Style::default()
    });
    let body: Element<'a, M> = if card.overlay {
        cover.into()
    } else {
        column![
            cover,
            column![
                text(card.title, TextStyle::HEAD).color(paint.fg_1()),
                text(card.subtitle, TextStyle::CAP).color(paint.fg_2())
            ]
            .padding([0.0, 2.0])
        ]
        .spacing(SP2)
        .width(width)
        .into()
    };
    let mut area = mouse_area(body).interaction(mouse::Interaction::Pointer);
    if let Some(message) = card.on_press {
        area = area.on_press(message);
    }
    if let Some(message) = card.on_right {
        area = area.on_right_press(message);
    }
    area.into()
}

#[must_use]
pub fn exif_strip<'a, M: 'a>(paint: &Paint, cells: Vec<String>) -> Element<'a, M> {
    let line = paint.hairline();
    let count = cells.len();
    let cells = cells.into_iter().enumerate().map(|(index, cell)| {
        container(text(cell, TextStyle { size_px: 12.0, line_height_px: 16.0, ..TextStyle::NUM }).color(paint.fg_1()))
            .center_x(Fill)
            .padding([7.0, 2.0])
            .style(move |_| container::Style {
                border: Border {
                    color: if index + 1 < count { line } else { Color::TRANSPARENT },
                    width: 0.0,
                    radius: 0.0.into(),
                },
                ..container::Style::default()
            })
            .into()
    });
    let view = paint.view_bg();
    container(row(cells).width(Fill))
        .style(move |_| container::Style {
            background: Some(Background::Color(view)),
            border: Border { color: line, width: 1.0, radius: R2.into() },
            ..container::Style::default()
        })
        .into()
}

#[must_use]
pub fn adjust_slider<'a, M: Clone + 'a>(
    paint: &Paint, label: &'a str, value: i32, min: i32, on_change: impl Fn(i32) -> M + 'a, reset: Option<(M, &'a str)>,
) -> Element<'a, M> {
    let changed = value != 0;
    let shown = if value > 0 && min < 0 { format!("+{value}") } else { value.to_string() };
    let mut title = row![text(label, TextStyle { weight: 500, ..TextStyle::CALL }).color(paint.fg_1())]
        .spacing(7)
        .align_y(Alignment::Center);
    if changed {
        title = title.push(super::dot(paint.acc_bg(), 5.0));
    }
    let mut head = row![container(title).width(Fill)].spacing(SP2).align_y(Alignment::Center).height(22);
    if let Some((message, word)) = reset.filter(|_| changed) {
        head = head.push(super::link(paint, word, Some(message)));
    }
    head = head.push(
        container(text(shown, TextStyle { size_px: 12.0, line_height_px: 16.0, ..TextStyle::NUM }).color(paint.fg_2()))
            .width(34)
            .align_right(34),
    );
    column![head, slider(paint, min as f32..=100.0, value as f32, move |v| on_change(v.round() as i32))]
        .spacing(SP1)
        .into()
}

struct Histogram {
    bins: [[u32; 64]; 3],
    paint: Paint,
}

impl<M> canvas::Program<M> for Histogram {
    type State = ();

    fn draw(&self, (): &(), renderer: &Renderer, _: &Theme, bounds: Rectangle, _: mouse::Cursor) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        frame.fill(&Path::rounded_rectangle(Point::ORIGIN, bounds.size(), R2.into()), self.paint.fill_1());
        let peak = self.bins.iter().flatten().copied().max().unwrap_or(1).max(1) as f32;
        let colours = [
            Color::from_rgba8(240, 121, 111, 0.30),
            Color::from_rgba8(98, 214, 155, 0.32),
            Color::from_rgba8(91, 155, 236, 0.38),
        ];
        for (channel, colour) in self.bins.iter().zip(colours) {
            let step = bounds.width / (channel.len() - 1) as f32;
            let path = Path::new(|builder| {
                builder.move_to(Point::new(0.0, bounds.height));
                for (index, &count) in channel.iter().enumerate() {
                    let level = (count as f32 / peak).sqrt();
                    builder.line_to(Point::new(index as f32 * step, bounds.height - level * (bounds.height - 4.0)));
                }
                builder.line_to(Point::new(bounds.width, bounds.height));
                builder.close();
            });
            frame.fill(&path, colour);
        }
        vec![frame.into_geometry()]
    }
}

#[must_use]
pub fn histogram<'a, M: 'a>(paint: &Paint, bins: [[u32; 64]; 3]) -> Element<'a, M> {
    surface(Histogram { bins, paint: *paint }).width(Fill).height(HISTOGRAM).into()
}

struct Checks;

impl<M> canvas::Program<M> for Checks {
    type State = ();

    fn draw(&self, (): &(), renderer: &Renderer, _: &Theme, bounds: Rectangle, _: mouse::Cursor) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let light = Color::from_rgb8(0xe8, 0xe6, 0xe2);
        let dark = Color::from_rgb8(0xcf, 0xcc, 0xc7);
        frame.fill_rectangle(Point::ORIGIN, bounds.size(), light);
        let cell = 8.0;
        let columns = (bounds.width / cell).ceil() as usize;
        let rows = (bounds.height / cell).ceil() as usize;
        for row in 0..rows {
            for column in (row % 2..columns).step_by(2) {
                frame.fill_rectangle(Point::new(column as f32 * cell, row as f32 * cell), Size::new(cell, cell), dark);
            }
        }
        vec![frame.into_geometry()]
    }
}

#[must_use]
pub fn checkerboard<'a, M: 'a>(width: f32, height: f32) -> Element<'a, M> {
    surface(Checks).width(width).height(height).into()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CropRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Grip {
    Move,
    North,
    South,
    East,
    West,
    NorthEast,
    NorthWest,
    SouthEast,
    SouthWest,
}

#[derive(Debug, Default)]
pub struct CropState {
    grip: Option<(Grip, Point, CropRect)>,
}

struct CropOverlay<'a, M> {
    rect: CropRect,
    ratio: Option<f32>,
    aspect: f32,
    on_change: Rc<dyn Fn(CropRect) -> M + 'a>,
}

const MIN_CROP: f32 = 0.05;
const GRIP: f32 = 16.0;

fn grip_at(rect: CropRect, point: Point, size: Size) -> Option<Grip> {
    let (left, top) = (rect.x * size.width, rect.y * size.height);
    let (right, bottom) = (left + rect.w * size.width, top + rect.h * size.height);
    let near = |a: f32, b: f32| (a - b).abs() <= GRIP;
    let inside_x = point.x >= left - GRIP && point.x <= right + GRIP;
    let inside_y = point.y >= top - GRIP && point.y <= bottom + GRIP;
    if !(inside_x && inside_y) {
        return None;
    }
    let (n, s, w, e) = (near(point.y, top), near(point.y, bottom), near(point.x, left), near(point.x, right));
    Some(match (n, s, w, e) {
        (true, _, true, _) => Grip::NorthWest,
        (true, _, _, true) => Grip::NorthEast,
        (_, true, true, _) => Grip::SouthWest,
        (_, true, _, true) => Grip::SouthEast,
        (true, ..) => Grip::North,
        (_, true, ..) => Grip::South,
        (_, _, true, _) => Grip::West,
        (_, _, _, true) => Grip::East,
        _ => Grip::Move,
    })
}

fn dragged(start: CropRect, grip: Grip, dx: f32, dy: f32, ratio: Option<f32>) -> CropRect {
    let (mut left, mut top, mut right, mut bottom) = (start.x, start.y, start.x + start.w, start.y + start.h);
    match grip {
        Grip::Move => {
            let x = (start.x + dx).clamp(0.0, 1.0 - start.w);
            let y = (start.y + dy).clamp(0.0, 1.0 - start.h);
            return CropRect { x, y, ..start };
        }
        Grip::North => top += dy,
        Grip::South => bottom += dy,
        Grip::West => left += dx,
        Grip::East => right += dx,
        Grip::NorthEast => (top, right) = (top + dy, right + dx),
        Grip::NorthWest => (top, left) = (top + dy, left + dx),
        Grip::SouthEast => (bottom, right) = (bottom + dy, right + dx),
        Grip::SouthWest => (bottom, left) = (bottom + dy, left + dx),
    }
    left = left.clamp(0.0, right - MIN_CROP);
    top = top.clamp(0.0, bottom - MIN_CROP);
    right = right.clamp(left + MIN_CROP, 1.0);
    bottom = bottom.clamp(top + MIN_CROP, 1.0);
    let mut rect = CropRect { x: left, y: top, w: right - left, h: bottom - top };
    if let Some(ratio) = ratio {
        let vertical = matches!(grip, Grip::North | Grip::South);
        if vertical {
            rect.w = (rect.h * ratio).min(1.0);
            rect.h = rect.w / ratio;
            rect.x = (start.x + start.w / 2.0 - rect.w / 2.0).clamp(0.0, 1.0 - rect.w);
        } else {
            rect.h = (rect.w / ratio).min(1.0);
            rect.w = rect.h * ratio;
            if matches!(grip, Grip::NorthEast | Grip::NorthWest) {
                rect.y = (bottom - rect.h).max(0.0);
            } else if matches!(grip, Grip::East | Grip::West) {
                rect.y = (start.y + start.h / 2.0 - rect.h / 2.0).clamp(0.0, 1.0 - rect.h);
            }
            if matches!(grip, Grip::NorthWest | Grip::SouthWest | Grip::West) {
                rect.x = (right - rect.w).max(0.0);
            }
        }
    }
    rect
}

impl<M> canvas::Program<M> for CropOverlay<'_, M> {
    type State = CropState;

    fn update(
        &self, state: &mut CropState, event: &Event, bounds: Rectangle, cursor: mouse::Cursor,
    ) -> Option<Action<M>> {
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let point = cursor.position_in(bounds)?;
                let grip = grip_at(self.rect, point, bounds.size())?;
                state.grip = Some((grip, point, self.rect));
                Some(Action::request_redraw().and_capture())
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let (grip, origin, start) = state.grip?;
                let point = cursor.position_from(bounds.position())?;
                let dx = (point.x - origin.x) / bounds.width;
                let dy = (point.y - origin.y) / bounds.height;
                let ratio = self.ratio.map(|ratio| ratio / self.aspect.max(0.01));
                Some(Action::publish((self.on_change)(dragged(start, grip, dx, dy, ratio))).and_capture())
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) if state.grip.is_some() => {
                state.grip = None;
                Some(Action::request_redraw())
            }
            _ => None,
        }
    }

    fn draw(
        &self, state: &CropState, renderer: &Renderer, _: &Theme, bounds: Rectangle, _: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let (width, height) = (bounds.width, bounds.height);
        let r = self.rect;
        let (left, top, w, h) = (r.x * width, r.y * height, r.w * width, r.h * height);
        let dim = Color { a: 0.62, ..STAGE };
        frame.fill_rectangle(Point::ORIGIN, Size::new(width, top), dim);
        frame.fill_rectangle(Point::new(0.0, top + h), Size::new(width, height - top - h), dim);
        frame.fill_rectangle(Point::new(0.0, top), Size::new(left, h), dim);
        frame.fill_rectangle(Point::new(left + w, top), Size::new(width - left - w, h), dim);
        let thirds = Color { a: if state.grip.is_some() { 0.6 } else { 0.4 }, ..Color::WHITE };
        for k in 1..3 {
            let fx = left + w * k as f32 / 3.0;
            let fy = top + h * k as f32 / 3.0;
            frame.stroke(
                &Path::line(Point::new(fx, top), Point::new(fx, top + h)),
                Stroke::default().with_color(thirds).with_width(1.0),
            );
            frame.stroke(
                &Path::line(Point::new(left, fy), Point::new(left + w, fy)),
                Stroke::default().with_color(thirds).with_width(1.0),
            );
        }
        let edge = Stroke::default().with_color(Color { a: 0.95, ..Color::WHITE }).with_width(1.5);
        frame.stroke(&Path::rectangle(Point::new(left, top), Size::new(w, h)), edge);
        let handle = Stroke::default().with_color(Color::WHITE).with_width(4.0).with_line_cap(LineCap::Square);
        let arm = 20.0_f32.min(w / 3.0).min(h / 3.0);
        for (x, y, sx, sy) in [
            (left, top, 1.0, 1.0),
            (left + w, top, -1.0, 1.0),
            (left, top + h, 1.0, -1.0),
            (left + w, top + h, -1.0, -1.0),
        ] {
            let path = Path::new(|builder| {
                builder.move_to(Point::new(x + sx * arm, y));
                builder.line_to(Point::new(x, y));
                builder.line_to(Point::new(x, y + sy * arm));
            });
            frame.stroke(&path, handle);
        }
        vec![frame.into_geometry()]
    }

    fn mouse_interaction(&self, state: &CropState, bounds: Rectangle, cursor: mouse::Cursor) -> mouse::Interaction {
        let grip = state
            .grip
            .map(|(grip, ..)| grip)
            .or_else(|| grip_at(self.rect, cursor.position_in(bounds)?, bounds.size()));
        match grip {
            Some(Grip::Move) => mouse::Interaction::Grab,
            Some(Grip::North | Grip::South) => mouse::Interaction::ResizingVertically,
            Some(Grip::East | Grip::West) => mouse::Interaction::ResizingHorizontally,
            Some(Grip::NorthWest | Grip::SouthEast) => mouse::Interaction::ResizingDiagonallyDown,
            Some(Grip::NorthEast | Grip::SouthWest) => mouse::Interaction::ResizingDiagonallyUp,
            None => mouse::Interaction::default(),
        }
    }
}

#[must_use]
pub fn crop_overlay<'a, M: 'a>(
    rect: CropRect, ratio: Option<f32>, aspect: f32, width: f32, height: f32, on_change: impl Fn(CropRect) -> M + 'a,
) -> Element<'a, M> {
    surface(CropOverlay { rect, ratio, aspect, on_change: Rc::new(on_change) }).width(width).height(height).into()
}

#[derive(Debug, Default)]
pub struct DialState {
    drag: Option<(f32, f32)>,
}

struct Dial<'a, M> {
    degrees: f32,
    on_change: Rc<dyn Fn(f32) -> M + 'a>,
}

const TICK: f32 = 14.0;

#[must_use]
pub fn dial_value(start: f32, dx: f32) -> f32 {
    let value = (start - dx / TICK).clamp(-45.0, 45.0);
    let value = (value * 2.0).round() / 2.0;
    if value.abs() <= 0.5 { 0.0 } else { value }
}

impl<M> canvas::Program<M> for Dial<'_, M> {
    type State = DialState;

    fn update(
        &self, state: &mut DialState, event: &Event, bounds: Rectangle, cursor: mouse::Cursor,
    ) -> Option<Action<M>> {
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let point = cursor.position_in(bounds)?;
                state.drag = Some((point.x, self.degrees));
                Some(Action::request_redraw().and_capture())
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let (origin, start) = state.drag?;
                let point = cursor.position_from(bounds.position())?;
                Some(Action::publish((self.on_change)(dial_value(start, point.x - origin))).and_capture())
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) if state.drag.is_some() => {
                state.drag = None;
                Some(Action::request_redraw())
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) if cursor.is_over(bounds) => {
                let step = match delta {
                    mouse::ScrollDelta::Lines { y, .. } => *y * 0.5,
                    mouse::ScrollDelta::Pixels { y, .. } => *y / TICK,
                };
                Some(Action::publish((self.on_change)(dial_value(self.degrees, -step * TICK))).and_capture())
            }
            _ => None,
        }
    }

    fn draw(
        &self, _: &DialState, renderer: &Renderer, _: &Theme, bounds: Rectangle, _: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let centre = bounds.width / 2.0;
        let faint = Stroke::default().with_color(Color { a: 0.45, ..GLASS_INK }).with_width(1.0);
        let first = (self.degrees - centre / TICK).floor() as i32;
        let last = (self.degrees + centre / TICK).ceil() as i32;
        for degree in first..=last {
            if degree.abs() > 45 {
                continue;
            }
            let x = centre + (degree as f32 - self.degrees) * TICK;
            let long = degree % 5 == 0;
            let (y0, y1) = if long { (10.0, 20.0) } else { (12.0, 18.0) };
            frame.stroke(&Path::line(Point::new(x, y0), Point::new(x, y1)), faint);
        }
        let zero = centre - self.degrees * TICK;
        let accent =
            Stroke::default().with_color(Color::from_rgb8(91, 155, 236)).with_width(2.5).with_line_cap(LineCap::Round);
        frame.stroke(&Path::line(Point::new(zero, 4.0), Point::new(zero, 26.0)), accent);
        let pointer = Stroke::default().with_color(Color::WHITE).with_width(2.0).with_line_cap(LineCap::Round);
        frame.stroke(&Path::line(Point::new(centre, 6.0), Point::new(centre, 24.0)), pointer);
        let label = format!("{:+.1}°", self.degrees).replace('-', "−").replace('.', ",");
        frame.fill_text(canvas::Text {
            content: if self.degrees == 0.0 { "0°".into() } else { label },
            position: Point::new(centre, 32.0),
            color: Color::WHITE,
            size: 11.0.into(),
            font: iced::Font::MONOSPACE,
            align_x: iced::widget::text::Alignment::Center,
            ..canvas::Text::default()
        });
        vec![frame.into_geometry()]
    }

    fn mouse_interaction(&self, _: &DialState, bounds: Rectangle, cursor: mouse::Cursor) -> mouse::Interaction {
        if cursor.is_over(bounds) { mouse::Interaction::ResizingHorizontally } else { mouse::Interaction::default() }
    }
}

#[must_use]
pub fn straighten_dial<'a, M: 'a>(degrees: f32, on_change: impl Fn(f32) -> M + 'a) -> Element<'a, M> {
    surface(Dial { degrees, on_change: Rc::new(on_change) }).width(DIAL_WIDTH).height(DIAL_HEIGHT).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crop_corners_stay_inside_and_keep_a_locked_ratio() {
        let start = CropRect { x: 0.1, y: 0.1, w: 0.8, h: 0.8 };
        let moved = dragged(start, Grip::Move, 0.5, -0.5, None);
        assert!((moved.x - 0.2).abs() < 1e-6 && moved.y.abs() < 1e-6);
        let grown = dragged(start, Grip::SouthEast, 0.5, 0.5, None);
        assert!((grown.w - 0.9).abs() < 1e-6 && (grown.h - 0.9).abs() < 1e-6);
        let squared = dragged(start, Grip::East, -0.4, 0.0, Some(1.0));
        assert!((squared.w - squared.h).abs() < 1e-5 && squared.w > 0.0);
        let tiny = dragged(start, Grip::NorthWest, 2.0, 2.0, None);
        assert!(tiny.w >= MIN_CROP - 1e-6 && tiny.h >= MIN_CROP - 1e-6);
    }

    #[test]
    fn the_dial_snaps_to_half_degrees_and_to_zero() {
        assert!(dial_value(0.0, -TICK * 3.0).eq(&3.0));
        assert!(dial_value(0.0, -TICK * 0.4).eq(&0.0));
        assert!(dial_value(44.0, -TICK * 10.0).eq(&45.0));
        assert!(dial_value(-3.5, 0.0).eq(&-3.5));
    }
}

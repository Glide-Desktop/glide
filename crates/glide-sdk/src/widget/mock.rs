use glide_iced::Paint;
use glide_iced::style::{self, R2, R3};
use glide_iced::widgets;
use glide_theme::TextStyle;
use glide_theme::elevation::Level;
use glide_theme::shape::{R_ROUND, R1, R4};
use iced::border::Radius;
use iced::widget::{Column, Container, Row, Space, Stack, button as control, column, container, image, row};
use iced::{
    Alignment, Background, Border, Color, ContentFit, Element, Fill, Gradient, Length, Padding, Shadow, Vector,
};

use super::{Glyph, icon, text};

fn gradient(top: Color, bottom: Color, radius: impl Into<Radius>) -> container::Style {
    container::Style {
        background: Some(Background::Gradient(Gradient::Linear(
            iced::gradient::Linear::new(std::f32::consts::PI).add_stop(0.0, top).add_stop(1.0, bottom),
        ))),
        border: Border { radius: radius.into(), ..Border::default() },
        ..container::Style::default()
    }
}

#[must_use]
pub fn block<'a, M: 'a>(
    color: Color, width: impl Into<Length>, height: impl Into<Length>, radius: impl Into<Radius>,
) -> Element<'a, M> {
    let radius = radius.into();
    container(Space::new()).width(width).height(height).style(move |_| style::rounded(color, radius)).into()
}

#[must_use]
pub fn outlined<'a, M: 'a>(
    fill: Color, stroke: Color, stroke_width: f32, width: impl Into<Length>, height: impl Into<Length>,
    radius: impl Into<Radius>,
) -> Element<'a, M> {
    outlined_surface(fill, stroke, stroke_width, radius, Space::new()).width(width).height(height).into()
}

#[must_use]
pub fn surface<'a, M: 'a>(
    background: Color, radius: impl Into<Radius>, content: impl Into<Element<'a, M>>,
) -> Container<'a, M> {
    let radius = radius.into();
    container(content).style(move |_| style::rounded(background, radius))
}

#[must_use]
pub fn outlined_surface<'a, M: 'a>(
    background: Color, stroke: Color, stroke_width: f32, radius: impl Into<Radius>, content: impl Into<Element<'a, M>>,
) -> Container<'a, M> {
    let radius = radius.into();
    container(content).style(move |_| container::Style {
        border: Border { color: stroke, width: stroke_width, radius },
        ..style::rounded(background, radius)
    })
}

#[must_use]
pub fn ring_dot<'a, M: 'a>(color: Color, side: f32) -> Element<'a, M> {
    outlined(color, Color { a: 0.35, ..Color::WHITE }, 2.0, side, side, R_ROUND)
}

#[must_use]
pub fn dots<'a, M: 'a>(color: Color, count: u32, side: f32, gap: f32) -> Element<'a, M> {
    Row::with_children((0..count.clamp(1, 5)).map(|_| block(color, side, side, R_ROUND)))
        .spacing(gap)
        .align_y(Alignment::Center)
        .into()
}

#[must_use]
pub fn dots_width(count: u32, side: f32, gap: f32) -> f32 {
    let count = count.clamp(1, 5) as f32;
    count * side + (count - 1.0) * gap
}

#[must_use]
pub fn gradient_strip<'a, M: 'a>(paint: impl Into<Paint>, from: Color, to: Color, height: f32) -> Element<'a, M> {
    let line = paint.into().line();
    container(Space::new())
        .width(Fill)
        .height(height)
        .style(move |_| container::Style {
            background: Some(Background::Gradient(Gradient::Linear(
                iced::gradient::Linear::new(std::f32::consts::FRAC_PI_2).add_stop(0.0, from).add_stop(1.0, to),
            ))),
            border: Border { color: line, width: 1.0, radius: R_ROUND.into() },
            ..container::Style::default()
        })
        .into()
}

#[must_use]
pub fn at<'a, M: 'a>(x: f32, y: f32, content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    container(content).padding(Padding::ZERO.top(y).left(x)).into()
}

#[must_use]
pub fn centered<'a, M: 'a>(content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    container(content).center_x(Fill).center_y(Fill).into()
}

#[must_use]
pub fn bottom<'a, M: 'a>(gap: f32, content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    container(content)
        .width(Fill)
        .height(Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::End)
        .padding(Padding::ZERO.bottom(gap))
        .into()
}

#[must_use]
pub fn caption<'a, M: 'a>(paint: impl Into<Paint>, label: impl Into<std::borrow::Cow<'a, str>>) -> Element<'a, M> {
    centered(text(label, TextStyle::CALL).color(paint.into().fg_2()))
}

#[must_use]
pub fn screen<'a, M: 'a>(
    paint: impl Into<Paint>, width: impl Into<Length>, height: f32, layers: Vec<Element<'a, M>>,
) -> Element<'a, M> {
    let paint = paint.into();
    let ramp = paint.tokens.ramp;
    let (top, bottom) = (style::to_iced(ramp[1]), style::to_iced(ramp[4]));
    let line = paint.line_2();
    container(Stack::with_children(layers).width(Fill).height(height))
        .width(width)
        .height(height)
        .clip(true)
        .style(move |_| container::Style {
            border: Border { color: line, width: 1.0, radius: R2.into() },
            ..gradient(top, bottom, R2)
        })
        .into()
}

#[must_use]
pub fn topbar<'a, M: 'a>(paint: impl Into<Paint>) -> Element<'a, M> {
    let paint = paint.into();
    let fg = paint.fg_3();
    surface(
        paint.bar_bg(),
        0.0,
        row![
            block(fg, 18.0, 4.0, R1),
            Space::new().width(Fill),
            block(fg, 26.0, 4.0, R1),
            Space::new().width(Fill),
            block(fg, 12.0, 4.0, R1),
        ]
        .align_y(Alignment::Center),
    )
    .padding(Padding::from([0.0, 8.0]))
    .width(Fill)
    .height(10)
    .center_y(10.0)
    .into()
}

#[must_use]
pub fn bar<'a, M: 'a>(paint: impl Into<Paint>, height: f32, content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    surface(paint.into().bar_bg(), 0.0, content)
        .padding(Padding::from([0.0, 12.0]))
        .width(Fill)
        .height(height)
        .center_y(height)
        .into()
}

#[must_use]
pub fn window<'a, M: 'a>(paint: impl Into<Paint>, width: f32, height: f32) -> Element<'a, M> {
    let paint = paint.into();
    let (bg, line) = (paint.win_bg(), paint.line_2());
    container(column![block(paint.bar_bg(), Fill, 8.0, 0.0), Space::new().height(Fill)].width(Fill).height(Fill))
        .width(width)
        .height(height)
        .clip(true)
        .style(move |_| container::Style {
            background: Some(Background::Color(bg)),
            border: Border { color: line, width: 1.0, radius: R1.into() },
            shadow: Shadow {
                color: Color::from_rgba(0.0, 0.0, 0.0, 0.25),
                offset: Vector::new(0.0, 2.0),
                blur_radius: 6.0,
            },
            ..container::Style::default()
        })
        .into()
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(clippy::struct_excessive_bools)] // Independent window-frame options a settings preview toggles one by one
pub struct TitledWindow {
    pub centered: bool,
    pub bar_height: f32,
    pub button: f32,
    pub close_left: bool,
    pub shadow: bool,
    pub border: bool,
    pub width: f32,
    pub height: f32,
}

#[must_use]
pub fn round_button<'a, M: 'a>(paint: impl Into<Paint>, glyph: impl Glyph, side: f32) -> Element<'a, M> {
    let paint = paint.into();
    surface(paint.fill_2(), R_ROUND, icon(glyph, (side * 0.55).round(), paint.fg_2()))
        .center_x(Length::Fixed(side))
        .center_y(Length::Fixed(side))
        .into()
}

#[must_use]
pub fn titled_window<'a, M: 'a>(
    paint: impl Into<Paint>, title: &'a str, maximize: impl Glyph, close: impl Glyph, spec: TitledWindow,
) -> Element<'a, M> {
    let paint = paint.into();
    let label = text(title, TextStyle { weight: 600, ..TextStyle::CAP }).color(paint.fg_1());
    let title: Element<'a, M> =
        if spec.centered { container(label).center_x(Fill).into() } else { container(label).width(Fill).into() };
    let (leading, trailing): (Element<'a, M>, Element<'a, M>) = if spec.close_left {
        (round_button(paint, close, spec.button), round_button(paint, maximize, spec.button))
    } else {
        (round_button(paint, maximize, spec.button), round_button(paint, close, spec.button))
    };
    let bar = surface(paint.bar_bg(), 0.0, row![leading, title, trailing].spacing(8).align_y(Alignment::Center))
        .padding(Padding::from([0.0, 5.0]))
        .width(Fill)
        .height(spec.bar_height)
        .center_y(spec.bar_height);
    let (body, ring) = (paint.win_bg(), paint.line());
    container(column![bar, Space::new().height(Fill)].width(Fill))
        .width(spec.width)
        .height(spec.height)
        .clip(true)
        .style(move |_| container::Style {
            background: Some(Background::Color(body)),
            border: Border { color: ring, width: if spec.border { 1.0 } else { 0.0 }, radius: R3.into() },
            shadow: if spec.shadow {
                Shadow {
                    color: Color::from_rgba(0.0, 0.0, 0.0, 0.35),
                    offset: Vector::new(0.0, 6.0),
                    blur_radius: 18.0,
                }
            } else {
                Shadow::default()
            },
            ..container::Style::default()
        })
        .into()
}

#[must_use]
pub fn mini_dock<'a, M: 'a>(paint: impl Into<Paint>) -> Element<'a, M> {
    let paint = paint.into();
    let ramp = paint.tokens.ramp;
    let icons = Row::with_children(ramp[1..=4].iter().map(|colour| block(style::to_iced(*colour), 9.0, 9.0, 3.0)));
    outlined_surface(paint.popover_bg(), paint.line_2(), 1.0, 5.0, icons.spacing(3)).padding(3).into()
}

#[must_use]
pub fn dock<'a, M: 'a>(paint: impl Into<Paint>, tiles: impl Into<Element<'a, M>>) -> Element<'a, M> {
    let paint = paint.into();
    outlined_surface(Color { a: 0.74, ..paint.popover_bg() }, paint.line_2(), 1.0, R4, tiles).padding(6).into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fit {
    Cover,
    Contain,
    Center,
    Tile,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Wallpaper {
    Solid(Color),
    Pending,
    Picture { handle: image::Handle, width: f32, height: f32 },
}

#[must_use]
pub fn wallpaper<'a, M: 'a>(
    paint: impl Into<Paint>, wallpaper: &Wallpaper, fit: Fit, backdrop: Color, width: f32, height: f32,
    reference_width: f32,
) -> Element<'a, M> {
    let paint = paint.into();
    let flat = |colour: Color| -> Element<'a, M> { block(colour, Fill, Fill, 0.0) };
    let content = match wallpaper {
        Wallpaper::Solid(colour) => flat(*colour),
        Wallpaper::Pending => flat(paint.sunken_bg()),
        Wallpaper::Picture { handle, width: natural_w, height: natural_h } => {
            let scale = width / reference_width;
            let (w, h) = ((natural_w * scale).max(2.0), (natural_h * scale).max(2.0));
            let shot = |w: f32, h: f32| image(handle.clone()).width(w).height(h).content_fit(ContentFit::Fill);
            match fit {
                Fit::Cover | Fit::Contain => image(handle.clone())
                    .width(Fill)
                    .height(Fill)
                    .content_fit(if fit == Fit::Cover { ContentFit::Cover } else { ContentFit::Contain })
                    .into(),
                Fit::Center => container(shot(w, h)).center_x(Fill).center_y(Fill).into(),
                Fit::Tile => {
                    let (cols, rows) =
                        (((width / w).ceil() as usize).clamp(1, 16), ((height / h).ceil() as usize).clamp(1, 16));
                    Column::with_children(
                        (0..rows).map(|_| Row::with_children((0..cols).map(|_| shot(w, h).into())).into()),
                    )
                    .into()
                }
            }
        }
    };
    surface(backdrop, R2, content).width(width).height(height).clip(true).into()
}

#[must_use]
pub fn document<'a, M: 'a>(
    paint: impl Into<Paint>, paper: Color, bar: Color, ink: Color, accent: Color, width: f32, height: f32,
) -> Element<'a, M> {
    let paint = paint.into();
    let line = |length: f32, color: Color| block(color, length, 5.0, R_ROUND);
    let page = container(
        column![
            block(bar, Fill, 14.0, R1),
            column![line(70.0, ink), line(96.0, ink), line(52.0, accent)]
                .spacing(6)
                .padding(Padding::from([4.0, 10.0])),
        ]
        .spacing(6),
    )
    .width(width)
    .height(height)
    .style(move |_| container::Style {
        border: Border { color: paint.line_2(), width: 1.0, radius: R2.into() },
        ..style::rounded(paper, R2)
    });
    widgets::surface(page, &paint, Level::El1, R2)
}

#[must_use]
pub fn schedule_bar<'a, M: 'a>(spans: &[(u16, u16)], total: u16, on: Color, off: Color, height: f32) -> Element<'a, M> {
    let piece = |from: u16, to: u16, color: Color| -> Option<Element<'a, M>> {
        (to > from).then(|| block(color, Length::FillPortion(to - from), Fill, R1))
    };
    let mut bar = Row::new().height(height).width(Fill);
    let mut cursor = 0;
    for &(from, to) in spans {
        bar = bar.push(piece(cursor, from, off)).push(piece(from, to, on));
        cursor = to;
    }
    bar.push(piece(cursor, total, off)).into()
}

#[must_use]
pub fn battery<'a, M: 'a>(paint: impl Into<Paint>, level: f32, color: Color) -> Element<'a, M> {
    let paint = paint.into();
    let (body_w, body_h) = (64.0, 34.0);
    let fill = block(color, ((body_w - 8.0) * level.clamp(0.0, 1.0)).max(2.0), body_h - 8.0, R1);
    let shell =
        outlined_surface(Color::TRANSPARENT, paint.fg_3(), 2.0, R2, fill).padding(4).width(body_w).height(body_h);
    row![shell, block(paint.fg_3(), 4.0, 12.0, R1)].spacing(2).align_y(Alignment::Center).into()
}

#[must_use]
pub fn glyph_well<'a, M: 'a>(
    paint: impl Into<Paint>, width: f32, height: f32, content: impl Into<Element<'a, M>>,
) -> Element<'a, M> {
    let paint = paint.into();
    outlined_surface(paint.fill_1(), paint.line_2(), 1.0, R2, content)
        .center_x(Length::Fixed(width))
        .center_y(Length::Fixed(height))
        .into()
}

#[must_use]
pub fn sample_area<'a, M: 'a>(
    paint: impl Into<Paint>, width: impl Into<Length>, height: impl Into<Length>, layers: Vec<Element<'a, M>>,
) -> Element<'a, M> {
    let paint = paint.into();
    let mut stack = vec![outlined(paint.sunken_bg(), paint.line_2(), 1.0, Fill, Fill, R3)];
    stack.extend(layers);
    container(Stack::with_children(stack).width(Fill).height(Fill)).width(width).height(height).clip(true).into()
}

#[must_use]
pub fn touchpad<'a, M: 'a>(paint: impl Into<Paint>, overlay: impl Into<Element<'a, M>>) -> Element<'a, M> {
    let paint = paint.into();
    container(iced::widget::stack![block(paint.fill_2(), 104.0, 68.0, R2), overlay.into()])
        .width(Length::Fixed(104.0))
        .height(Length::Fixed(68.0))
        .into()
}

#[must_use]
pub fn mouse<'a, M: 'a>(paint: impl Into<Paint>, left_handed: bool) -> Element<'a, M> {
    let paint = paint.into();
    let (left, right) = if left_handed { (paint.fill_3(), paint.acc_bg()) } else { (paint.acc_bg(), paint.fill_3()) };
    surface(
        paint.fill_2(),
        R4,
        column![
            row![block(left, Fill, 28.0, R2), block(right, Fill, 28.0, R2)].spacing(3),
            container(block(paint.line_2(), 4.0, 10.0, 2.0)).center_x(Fill),
        ]
        .spacing(6),
    )
    .padding(4)
    .width(Length::Fixed(48.0))
    .height(Length::Fixed(72.0))
    .into()
}

#[must_use]
pub fn monitor<'a, M: Clone + 'a>(
    paint: impl Into<Paint>, name: String, detail: String, (width, height): (f32, f32), selected: bool, message: M,
) -> Element<'a, M> {
    let paint = paint.into();
    let (name_ink, detail_ink) =
        if selected { (paint.acc_fg(), Color { a: 0.8, ..paint.acc_fg() }) } else { (paint.fg_1(), paint.fg_2()) };
    let label = column![
        text(name, TextStyle { weight: 600, ..TextStyle::CAP }).color(name_ink),
        text(detail, TextStyle::CAP).color(detail_ink),
    ]
    .spacing(2)
    .align_x(Alignment::Center);
    let ramp = paint.tokens.ramp;
    let (top, bottom) =
        if selected { (style::to_iced(ramp[2]), style::to_iced(ramp[4])) } else { (paint.fill_2(), paint.fill_1()) };
    let ring = if selected { paint.fg_1() } else { paint.line_2() };
    let face = container(label)
        .center_x(Length::Fixed(width.max(72.0)))
        .center_y(Length::Fixed(height.max(44.0)))
        .style(move |_| container::Style {
            border: Border {
                color: ring,
                width: if selected { glide_theme::component::FOCUS_RING } else { 1.0 },
                radius: R1.into(),
            },
            ..gradient(top, bottom, R1)
        });
    let face = widgets::surface(face, &paint, Level::El1, R1);
    let button = control(face).padding(0).on_press(message.clone());
    let button = widgets::button(button, move |_| style::flat(Color::TRANSPARENT, paint.fg_1(), R1));
    column![
        super::focus::activatable(&paint, button, R1, Some(message)),
        block(paint.fill_3(), (width * 0.28).max(18.0), 4.0, R1),
    ]
    .spacing(3)
    .align_x(Alignment::Center)
    .into()
}

#[must_use]
pub fn click_target<'a, M: Clone + 'a>(
    glyph: impl Glyph, tint: Color, fill: Color, ring: Color, side: f32, message: M,
) -> Element<'a, M> {
    let face = outlined_surface(fill, ring, 2.0, R_ROUND, icon(glyph, (side * 0.4).round(), tint))
        .center_x(Length::Fixed(side))
        .center_y(Length::Fixed(side));
    control(face).padding(0).on_press(message).style(|_, _| control::Style::default()).into()
}

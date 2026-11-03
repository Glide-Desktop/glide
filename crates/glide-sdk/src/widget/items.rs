use glide_iced::Paint;
use glide_iced::style::{self, R2, SP1, SP2, SP3};
use glide_theme::TextStyle;
use glide_theme::shape::R1;
use iced::widget::{Space, column, container, stack};
use iced::{Alignment, Border, Color, Element, Fill, Length, Padding, Rectangle};

use super::{Glyph, icon, mix, text};

const LIFT: f32 = 4.0;
const GROWTH: f32 = 0.06;

#[must_use]
pub fn corner_badge<'a, M: 'a>(paint: &Paint, glyph: impl Glyph, side: f32) -> Element<'a, M> {
    let background = paint.popover_bg();
    container(icon(glyph, side, paint.fg_1())).padding(2).style(move |_| style::rounded(background, R1)).into()
}

#[must_use]
#[allow(clippy::too_many_arguments)] // One tile of a grid: its parts, its size and three independent animated states
pub fn item_tile<'a, M: 'a>(
    paint: &Paint, art: Element<'a, M>, badge: Option<Element<'a, M>>, label: Element<'a, M>, icon: f32,
    (width, height): (f32, f32), selected: f32, (hover, lift): (f32, f32),
) -> Element<'a, M> {
    let paint = *paint;
    let rise = lift * LIFT;
    let art: Element<'a, M> = glide_iced::widgets::lift(art, 1.0 + lift * GROWTH, -rise);
    let art = container(art).center(Length::Fixed(icon));
    let art: Element<'a, M> = match badge {
        Some(badge) => stack![art, container(badge).align_bottom(Fill).align_left(Fill)].into(),
        None => art.into(),
    };
    let base = mix(Color::TRANSPARENT, paint.acc_soft(), selected);
    let hover = hover * (1.0 - selected);
    let body = column![art, container(label).center_x(Fill)].spacing(SP2).align_x(Alignment::Center);
    container(body)
        .padding(Padding::from([SP2, SP1]))
        .width(Length::Fixed(width))
        .height(Length::Fixed(height))
        .style(move |_| style::rounded(paint.hover(base, hover), R2))
        .into()
}

#[must_use]
pub fn item_ink(paint: &Paint, selected: f32, secondary: bool) -> Color {
    let rest = if secondary { paint.fg_2() } else { paint.fg_1() };
    mix(rest, paint.acc_soft_fg(), selected)
}

#[must_use]
pub fn table_row<'a, M: 'a>(
    paint: &Paint, content: impl Into<Element<'a, M>>, selected: f32, stripe: bool, hover: f32, height: f32,
) -> Element<'a, M> {
    let paint = *paint;
    let rest = if stripe { paint.fill_1() } else { Color::TRANSPARENT };
    let base = mix(rest, paint.acc_soft(), selected);
    let hover = hover * (1.0 - selected);
    let cells = container(content).width(Fill).height(height - 1.0).align_y(Alignment::Center);
    let rule = container(Space::new().width(Fill).height(1)).style(move |_| style::rounded(paint.line(), 0.0));
    container(column![cells, rule])
        .width(Fill)
        .height(height)
        .style(move |_| container::Style {
            text_color: Some(item_ink(&paint, selected, false)),
            ..style::rounded(paint.hover(base, hover), 0.0)
        })
        .into()
}

#[must_use]
pub fn section_title<'a, M: 'a>(paint: &Paint, title: &str, highlighted: bool) -> Element<'a, M> {
    let paint = *paint;
    let ink = if highlighted { paint.acc_soft_fg() } else { paint.fg_2() };
    container(text(title.to_string(), TextStyle::MICRO).color(ink))
        .padding(Padding::from([2.0, SP3]))
        .width(Fill)
        .style(move |_| if highlighted { style::rounded(paint.acc_soft(), R2) } else { container::Style::default() })
        .into()
}

#[must_use]
pub fn selection_band<'a, M: 'a>(paint: &Paint, rect: Rectangle) -> Element<'a, M> {
    let accent = paint.acc_bg();
    let fill = accent.scale_alpha(glide_theme::motion::ST_PRESS);
    let band = container(Space::new().width(rect.width).height(rect.height)).style(move |_| container::Style {
        border: Border { color: accent, width: 1.0, radius: R1.into() },
        ..style::rounded(fill, R1)
    });
    container(band).padding(Padding { top: rect.y, left: rect.x, ..Padding::ZERO }).width(Fill).height(Fill).into()
}

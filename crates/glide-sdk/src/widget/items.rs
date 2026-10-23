use glide_iced::Paint;
use glide_iced::style::{self, R2, SP1, SP2};
use glide_theme::TextStyle;
use glide_theme::shape::R1;
use iced::widget::{Space, column, container, float, stack};
use iced::{Alignment, Border, Color, Element, Fill, Length, Padding, Rectangle, Vector};

use super::{Glyph, icon, text};

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
    paint: &Paint, art: Element<'a, M>, badge: Option<Element<'a, M>>, label: Element<'a, M>, plate: f32,
    (width, height): (f32, f32), selected: bool, (hover, lift): (f32, f32),
) -> Element<'a, M> {
    let paint = *paint;
    let rise = lift * LIFT;
    let art: Element<'a, M> =
        float(art).scale(1.0 + lift * GROWTH).translate(move |_, _| Vector::new(0.0, -rise)).into();
    let art: Element<'a, M> = match badge {
        Some(badge) => stack![art, container(badge).align_bottom(Fill).align_left(Fill)].into(),
        None => art,
    };
    let base = if selected { paint.acc_soft() } else { Color::TRANSPARENT };
    let plate =
        container(art).center(Length::Fixed(plate)).style(move |_| style::rounded(paint.hover(base, hover), R2));
    let label = container(label)
        .padding(Padding::from([1.0, SP1]))
        .max_width(width)
        .style(move |_| if selected { style::rounded(paint.acc_soft(), R1) } else { container::Style::default() });
    container(column![plate, label].spacing(SP1).align_x(Alignment::Center))
        .center_x(Length::Fixed(width))
        .height(Length::Fixed(height))
        .clip(true)
        .into()
}

#[must_use]
pub fn item_ink(paint: &Paint, selected: bool, secondary: bool) -> Color {
    match (selected, secondary) {
        (true, _) => paint.acc_soft_fg(),
        (false, true) => paint.fg_2(),
        (false, false) => paint.fg_1(),
    }
}

#[must_use]
pub fn table_row<'a, M: 'a>(
    paint: &Paint, content: impl Into<Element<'a, M>>, selected: bool, stripe: bool, hover: f32, height: f32,
) -> Element<'a, M> {
    let paint = *paint;
    let base = match (selected, stripe) {
        (true, _) => paint.acc_soft(),
        (false, true) => paint.fill_1(),
        (false, false) => Color::TRANSPARENT,
    };
    container(content)
        .padding(Padding::from([0.0, SP2]))
        .width(Fill)
        .height(height)
        .align_y(Alignment::Center)
        .style(move |_| container::Style {
            text_color: Some(item_ink(&paint, selected, false)),
            ..style::rounded(paint.hover(base, hover), R2)
        })
        .into()
}

#[must_use]
pub fn section_title<'a, M: 'a>(paint: &Paint, title: &str, highlighted: bool) -> Element<'a, M> {
    let paint = *paint;
    let ink = if highlighted { paint.acc_soft_fg() } else { paint.fg_3() };
    container(text(title.to_string(), TextStyle::MICRO).color(ink))
        .padding(Padding::from([2.0, SP2]))
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

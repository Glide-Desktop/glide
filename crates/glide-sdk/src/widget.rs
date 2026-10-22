use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::RangeInclusive;

use glide_iced::Paint;
pub use glide_iced::glyph::Icon;
use glide_iced::style::{self, R2, R3, SP1, SP2, SP3, SP4, SP5};
pub use glide_iced::text::{Text, Wrapping, text};
use glide_iced::widgets;
use glide_theme::elevation::Level;
use glide_theme::shape::{R_ROUND, R1};
use glide_theme::{TextStyle, component};
use iced::border::Radius;
use iced::widget::{
    Space, button as control, checkbox as tick, column, container, pick_list, progress_bar, row, scrollable as scroll,
    slider as rail, text_input, toggler, tooltip as tip,
};
use iced::{Alignment, Background, Border, Color, Element, Fill, Length, Padding, Shadow};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Tone {
    Accent,
    #[default]
    Neutral,
    Ghost,
    Danger,
}

pub const ROW: f32 = 44.0;
pub const SWITCH: f32 = 24.0;

#[must_use]
pub fn measure(content: &str, style: TextStyle) -> f32 {
    use iced::advanced::graphics::text::{self as graphics, cosmic_text};
    thread_local! {
        static CACHE: RefCell<HashMap<(String, u32, u16, u64), f32>> = RefCell::new(HashMap::new());
    }
    let key = (content.to_string(), style.size_px.to_bits(), style.weight, glide_iced::font::revision());
    if let Some(width) = CACHE.with_borrow(|cache| cache.get(&key).copied()) {
        return width;
    }
    let mut system = graphics::font_system().write().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut buffer =
        cosmic_text::Buffer::new(system.raw(), cosmic_text::Metrics::new(style.size_px, style.line_height_px));
    buffer.set_size(system.raw(), None, None);
    let attributes = graphics::to_attributes(iced::Font::DEFAULT)
        .weight(cosmic_text::Weight(style.weight))
        .letter_spacing(style.tracking_em);
    buffer.set_text(system.raw(), content, &attributes, cosmic_text::Shaping::Advanced, None);
    buffer.shape_until_scroll(system.raw(), false);
    let width = graphics::measure(&buffer).0.width.ceil();
    CACHE.with_borrow_mut(|cache| {
        if cache.len() > 512 {
            cache.clear();
        }
        cache.insert(key, width);
    });
    width
}

#[must_use]
pub fn button_style(paint: &Paint, tone: Tone, status: control::Status) -> control::Style {
    match tone {
        _ if status == control::Status::Disabled => style::flat(paint.fill_1(), paint.fg_4(), R2),
        Tone::Accent => {
            let background = match status {
                control::Status::Hovered | control::Status::Pressed => paint.acc_bg_hover(),
                _ => paint.acc_bg(),
            };
            style::flat(background, paint.acc_fg(), R2)
        }
        Tone::Neutral => style::action(paint, glide_iced::Tone::Neutral, status),
        Tone::Ghost => style::action(paint, glide_iced::Tone::Ghost, status),
        Tone::Danger => style::action(paint, glide_iced::Tone::Danger, status),
    }
}

#[must_use]
pub fn button<'a, M: Clone + 'a>(paint: &Paint, label: &'a str, tone: Tone, message: Option<M>) -> Element<'a, M> {
    let paint = *paint;
    let control = control(container(text(label, TextStyle::CALL).wrapping(Wrapping::None)).center_y(Fill))
        .on_press_maybe(message)
        .height(component::CONTROL)
        .padding(Padding::from([0.0, SP4]));
    widgets::button(control, move |status| button_style(&paint, tone, status))
}

#[must_use]
pub fn primary<'a, M: Clone + 'a>(paint: &Paint, label: &'a str, message: M) -> Element<'a, M> {
    button(paint, label, Tone::Accent, Some(message))
}

#[must_use]
pub fn secondary<'a, M: Clone + 'a>(paint: &Paint, label: &'a str, message: M) -> Element<'a, M> {
    button(paint, label, Tone::Neutral, Some(message))
}

#[must_use]
pub fn ghost<'a, M: Clone + 'a>(paint: &Paint, label: &'a str, message: M) -> Element<'a, M> {
    button(paint, label, Tone::Ghost, Some(message))
}

#[must_use]
pub fn danger<'a, M: Clone + 'a>(paint: &Paint, label: &'a str, message: M) -> Element<'a, M> {
    button(paint, label, Tone::Danger, Some(message))
}

#[must_use]
pub fn tooltip<'a, M: 'a>(paint: &Paint, child: impl Into<Element<'a, M>>, label: &'a str) -> Element<'a, M> {
    let paint = *paint;
    tip(
        child,
        widgets::surface(
            container(text(label, TextStyle::CAP)).padding([SP1, SP2]).style(move |_| {
                let mut style = style::popover(&paint);
                style.border.radius = R2.into();
                style
            }),
            &paint,
            Level::El2,
            R2,
        ),
        tip::Position::Bottom,
    )
    .delay(component::TOOLTIP_DELAY)
    .gap(SP2)
    .into()
}

#[must_use]
pub fn icon<'a, M: 'a>(icon: Icon, side: f32, tint: Color) -> Element<'a, M> {
    glide_iced::glyph::glyph(icon, side, tint)
}

#[must_use]
pub fn icon_button<'a, M: Clone + 'a>(
    paint: &Paint, glyph: Icon, label: &'a str, message: Option<M>, on: bool,
) -> Element<'a, M> {
    let paint = *paint;
    let tint = if message.is_none() {
        paint.fg_4()
    } else if on {
        paint.fg_1()
    } else {
        paint.fg_2()
    };
    let control = control(container(icon(glyph, 16.0, tint)).center(Fill))
        .on_press_maybe(message)
        .width(component::SMALL_CONTROL)
        .height(component::SMALL_CONTROL)
        .padding(0);
    tooltip(
        &paint,
        widgets::button(control, move |status| {
            if on {
                style::flat(paint.layered(paint.fill_2(), status), paint.fg_1(), R2)
            } else {
                style::action(&paint, glide_iced::Tone::Ghost, status)
            }
        }),
        label,
    )
}

#[must_use]
pub fn input_style(paint: &Paint, status: text_input::Status) -> text_input::Style {
    let mut style = style::field(paint, status);
    style.background = Background::Color(Color::TRANSPARENT);
    style.border = Border::default();
    if status == text_input::Status::Disabled {
        style.value = paint.fg_4();
    }
    style
}

#[must_use]
pub fn field<'a, M: Clone + 'a>(paint: &Paint, input: text_input::TextInput<'a, M>) -> Element<'a, M> {
    let paint = *paint;
    widgets::field(
        container(
            input
                .size(TextStyle::BODY.size_px)
                .padding(Padding::from([SP1, 0.0]))
                .style(move |_, status| input_style(&paint, status)),
        )
        .height(component::CONTROL)
        .padding(Padding::from([0.0, SP3]))
        .center_y(component::CONTROL),
        &paint,
    )
}

#[must_use]
pub fn text_field<'a, M: Clone + 'a>(
    paint: &Paint, placeholder: &str, value: &str, on_input: impl Fn(String) -> M + 'a,
) -> Element<'a, M> {
    field(paint, text_input(placeholder, value).on_input(on_input))
}

#[must_use]
pub fn switch_style(paint: &Paint, status: toggler::Status) -> toggler::Style {
    let (on, hovered, disabled) = match status {
        toggler::Status::Active { is_toggled } => (is_toggled, false, false),
        toggler::Status::Hovered { is_toggled } => (is_toggled, true, false),
        toggler::Status::Disabled { is_toggled } => (is_toggled, false, true),
    };
    let track = match (disabled, on, hovered) {
        (true, _, _) => paint.fill_2(),
        (false, true, true) => paint.acc_bg_hover(),
        (false, true, false) => paint.acc_bg(),
        (false, false, true) => paint.layer(paint.fill_3(), glide_theme::motion::ST_HOVER),
        (false, false, false) => paint.fill_3(),
    };
    let thumb = if on { Color::WHITE } else { style::to_iced(paint.tokens.switch_thumb()) };
    toggler::Style {
        background: Background::Color(track),
        background_border_width: 0.0,
        background_border_color: Color::TRANSPARENT,
        foreground: Background::Color(thumb.scale_alpha(paint.alpha)),
        foreground_border_width: 0.0,
        foreground_border_color: Color::TRANSPARENT,
        text_color: Some(if disabled { paint.fg_4() } else { paint.fg_1() }),
        border_radius: Some(Radius::from(R_ROUND)),
        padding_ratio: 0.125,
    }
}

#[must_use]
pub fn switch<'a, M: 'a>(paint: &Paint, on: bool, on_toggle: Option<impl Fn(bool) -> M + 'a>) -> Element<'a, M> {
    let paint = *paint;
    toggler(on).on_toggle_maybe(on_toggle).size(SWITCH).style(move |_, status| switch_style(&paint, status)).into()
}

#[must_use]
pub fn checkbox_style(paint: &Paint, status: tick::Status) -> tick::Style {
    let (checked, hovered, disabled) = match status {
        tick::Status::Active { is_checked } => (is_checked, false, false),
        tick::Status::Hovered { is_checked } => (is_checked, true, false),
        tick::Status::Disabled { is_checked } => (is_checked, false, true),
    };
    let background = match (checked, hovered) {
        (true, true) => paint.acc_bg_hover(),
        (true, false) => paint.acc_bg(),
        (false, true) => paint.layer(paint.view_bg(), glide_theme::motion::ST_HOVER),
        (false, false) => paint.view_bg(),
    };
    tick::Style {
        background: Background::Color(if disabled { paint.fill_2() } else { background }),
        icon_color: paint.acc_fg(),
        border: Border {
            color: if checked && !disabled { Color::TRANSPARENT } else { paint.line_2() },
            width: 1.0,
            radius: R1.into(),
        },
        text_color: Some(if disabled { paint.fg_4() } else { paint.fg_1() }),
    }
}

#[must_use]
pub fn checkbox<'a, M: 'a>(
    paint: &Paint, label: &'a str, checked: bool, on_toggle: Option<impl Fn(bool) -> M + 'a>,
) -> Element<'a, M> {
    let paint = *paint;
    tick(checked)
        .label(label)
        .size(18)
        .spacing(SP2)
        .text_size(TextStyle::CALL.size_px)
        .on_toggle_maybe(on_toggle)
        .style(move |_, status| checkbox_style(&paint, status))
        .into()
}

#[must_use]
pub fn segmented<'a, T: Copy + PartialEq + 'a, M: Clone + 'a>(
    paint: &Paint, options: &[(T, &'a str)], selected: T, on_select: impl Fn(T) -> M,
) -> Element<'a, M> {
    let paint = *paint;
    let thumb = style::to_iced(paint.tokens.segment_thumb()).scale_alpha(paint.alpha);
    let shadow = glide_theme::elevation::layers(Level::El1, paint.tokens.appearance).first().map_or_else(
        Shadow::default,
        |layer| Shadow {
            color: style::to_iced(layer.color).scale_alpha(paint.alpha),
            offset: iced::Vector::new(layer.dx, layer.dy),
            blur_radius: layer.blur,
        },
    );
    let segments = options.iter().map(|&(value, label)| {
        let active = value == selected;
        let label = text(label, if active { TextStyle::MENU_ITEM } else { TextStyle::CALL }).wrapping(Wrapping::None);
        control(container(label).center_y(Fill))
            .on_press(on_select(value))
            .height(component::SMALL_CONTROL)
            .padding(Padding::from([0.0, SP3 + 2.0]))
            .style(move |_, status| {
                if active {
                    control::Style { shadow, ..style::flat(thumb, paint.fg_1(), R1 + 2.0) }
                } else {
                    style::flat(paint.layered(Color::TRANSPARENT, status), paint.fg_2(), R1 + 2.0)
                }
            })
            .into()
    });
    container(iced::widget::Row::with_children(segments).spacing(2))
        .padding((component::CONTROL - component::SMALL_CONTROL) / 2.0)
        .style(move |_| style::rounded(paint.fill_1(), R2))
        .into()
}

#[must_use]
pub fn slider_style(paint: &Paint, status: rail::Status) -> rail::Style {
    let (radius, width) = match status {
        rail::Status::Active => (8.0, 0.0),
        rail::Status::Hovered => (8.0, 4.0),
        rail::Status::Dragged => (8.0, 6.0),
    };
    rail::Style {
        rail: rail::Rail {
            backgrounds: (Background::Color(paint.acc_bg()), Background::Color(paint.fill_2())),
            width: 6.0,
            border: Border { radius: R1.into(), ..Border::default() },
        },
        handle: rail::Handle {
            shape: rail::HandleShape::Circle { radius },
            background: Background::Color(paint.acc_bg()),
            border_width: width,
            border_color: paint.acc_ring(),
        },
    }
}

#[must_use]
pub fn slider<'a, M: Clone + 'a>(
    paint: &Paint, range: RangeInclusive<f32>, value: f32, on_change: impl Fn(f32) -> M + 'a,
) -> Element<'a, M> {
    let paint = *paint;
    rail(range, value, on_change).style(move |_, status| slider_style(&paint, status)).into()
}

#[must_use]
pub fn progress<'a, M: 'a>(paint: &Paint, fraction: f32) -> Element<'a, M> {
    let paint = *paint;
    progress_bar(0.0..=1.0, fraction.clamp(0.0, 1.0))
        .girth(6)
        .style(move |_| progress_bar::Style {
            background: Background::Color(paint.fill_2()),
            bar: Background::Color(paint.acc_bg()),
            border: Border { radius: R1.into(), ..Border::default() },
        })
        .into()
}

#[must_use]
pub fn dropdown<'a, T: ToString + PartialEq + Clone + 'a, M: Clone + 'a>(
    paint: &Paint, options: Vec<T>, selected: Option<T>, on_select: impl Fn(T) -> M + 'a,
) -> Element<'a, M> {
    let paint = *paint;
    pick_list(options, selected, on_select)
        .text_size(TextStyle::CALL.size_px)
        .padding(Padding::from([SP2 - 1.0, SP3]))
        .style(move |_, status| {
            let (background, border) = match status {
                pick_list::Status::Active => (paint.fill_1(), paint.line()),
                pick_list::Status::Hovered => {
                    (paint.layer(paint.fill_1(), glide_theme::motion::ST_HOVER), paint.line_2())
                }
                pick_list::Status::Opened { .. } => {
                    (paint.layer(paint.fill_1(), glide_theme::motion::ST_PRESS), paint.acc_bg())
                }
            };
            pick_list::Style {
                text_color: paint.fg_1(),
                placeholder_color: paint.fg_3(),
                handle_color: paint.fg_2(),
                background: Background::Color(background),
                border: Border { color: border, width: 1.0, radius: R2.into() },
            }
        })
        .menu_style(move |_| iced::overlay::menu::Style {
            background: Background::Color(paint.popover_bg()),
            border: Border { color: paint.line(), width: 1.0, radius: R3.into() },
            text_color: paint.fg_1(),
            selected_text_color: paint.fg_1(),
            selected_background: Background::Color(paint.fill_2()),
            shadow: Shadow::default(),
        })
        .into()
}

#[must_use]
pub fn scrollable_style(paint: &Paint, status: scroll::Status) -> scroll::Style {
    let hovered = matches!(
        status,
        scroll::Status::Hovered { is_vertical_scrollbar_hovered: true, .. } | scroll::Status::Dragged { .. }
    );
    let rail = scroll::Rail {
        background: None,
        border: Border::default(),
        scroller: scroll::Scroller {
            background: Background::Color(if hovered { paint.fg_3() } else { paint.fill_3() }),
            border: Border { radius: R_ROUND.into(), ..Border::default() },
        },
    };
    scroll::Style {
        container: container::Style::default(),
        vertical_rail: rail,
        horizontal_rail: rail,
        gap: None,
        auto_scroll: scroll::AutoScroll {
            background: Background::Color(paint.popover_bg()),
            border: Border { color: paint.line(), width: 1.0, radius: R_ROUND.into() },
            shadow: Shadow::default(),
            icon: paint.fg_2(),
        },
    }
}

#[must_use]
pub fn scrollable<'a, M: 'a>(paint: &Paint, content: impl Into<Element<'a, M>>) -> scroll::Scrollable<'a, M> {
    let paint = *paint;
    let bar = scroll::Scrollbar::new().width(6).scroller_width(6).spacing(SP1);
    scroll::Scrollable::with_direction(content, scroll::Direction::Vertical(bar))
        .style(move |_, status| scrollable_style(&paint, status))
}

#[must_use]
pub fn separator<'a, M: 'a>(paint: &Paint) -> Element<'a, M> {
    let paint = *paint;
    container(Space::new().width(Fill).height(1)).style(move |_| style::rounded(paint.hairline(), 0.0)).into()
}

#[must_use]
pub fn vertical_separator<'a, M: 'a>(paint: &Paint) -> Element<'a, M> {
    let paint = *paint;
    container(Space::new().width(1).height(Fill)).style(move |_| style::rounded(paint.hairline(), 0.0)).into()
}

#[must_use]
pub fn card_style(paint: &Paint) -> container::Style {
    container::Style {
        text_color: Some(paint.fg_1()),
        background: Some(Background::Color(paint.card_bg())),
        border: Border { color: paint.line(), width: 1.0, radius: R3.into() },
        ..container::Style::default()
    }
}

#[must_use]
pub fn card<'a, M: 'a>(paint: &Paint, content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    let paint = *paint;
    widgets::surface(
        container(content).padding(SP4).width(Fill).style(move |_| card_style(&paint)),
        &paint,
        Level::El1,
        R3,
    )
}

#[must_use]
pub fn list<'a, M: 'a>(paint: &Paint, rows: Vec<Element<'a, M>>) -> Element<'a, M> {
    let paint = *paint;
    let mut items = column![];
    let count = rows.len();
    for (index, item) in rows.into_iter().enumerate() {
        items = items.push(item);
        if index + 1 < count {
            items = items.push(container(separator(&paint)).padding(Padding::from([0.0, SP4])));
        }
    }
    widgets::surface(container(items).width(Fill).style(move |_| card_style(&paint)), &paint, Level::El1, R3)
}

#[must_use]
pub fn list_row<'a, M: 'a>(
    paint: &Paint, title: &'a str, subtitle: Option<&'a str>, trailing: impl Into<Element<'a, M>>,
) -> Element<'a, M> {
    let mut label = column![text(title, TextStyle::CALL)].spacing(2);
    if let Some(subtitle) = subtitle {
        label = label.push(text(subtitle, TextStyle::CAP).color(paint.fg_3()));
    }
    container(row![container(label).width(Fill), trailing.into()].spacing(SP3).align_y(Alignment::Center))
        .padding(Padding::from([SP2, SP4]))
        .center_y(Length::Shrink)
        .height(Length::Shrink)
        .width(Fill)
        .into()
}

#[must_use]
pub fn section<'a, M: 'a>(paint: &Paint, title: &'a str, content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    column![
        container(text(title, TextStyle::MICRO).color(paint.fg_3())).padding(Padding::from([0.0, SP2])),
        content.into()
    ]
    .spacing(SP2)
    .into()
}

#[must_use]
pub fn badge<'a, M: 'a>(paint: &Paint, label: &'a str, tone: Tone) -> Element<'a, M> {
    let (background, color) = match tone {
        Tone::Accent => (paint.acc_bg(), paint.acc_fg()),
        Tone::Neutral | Tone::Ghost => (paint.fill_2(), paint.fg_2()),
        Tone::Danger => (paint.danger_soft(), paint.danger()),
    };
    container(text(label, TextStyle::CAP).color(color).wrapping(Wrapping::None))
        .padding(Padding::from([2.0, SP2]))
        .style(move |_| style::rounded(background, R_ROUND))
        .into()
}

#[must_use]
pub fn empty_state<'a, M: 'a>(paint: &Paint, glyph: Icon, title: &'a str, body: &'a str) -> Element<'a, M> {
    let paint = *paint;
    let well =
        container(icon(glyph, 28.0, paint.fg_3())).center(64).style(move |_| style::rounded(paint.fill_1(), R_ROUND));
    container(
        column![
            well,
            text(title, TextStyle::HEAD),
            text(body, TextStyle::CALL).color(paint.fg_3()).align_x(Alignment::Center)
        ]
        .spacing(SP3)
        .align_x(Alignment::Center)
        .max_width(360),
    )
    .center(Fill)
    .padding(SP5)
    .into()
}

#[must_use]
pub fn sidebar_item<'a, M: Clone + 'a>(
    paint: &Paint, glyph: Option<Icon>, label: &'a str, selected: bool, message: M,
) -> Element<'a, M> {
    let paint = *paint;
    let tint = if selected { paint.acc_soft_fg() } else { paint.fg_3() };
    let mut content = row![].spacing(SP2 + 2.0).align_y(Alignment::Center);
    if let Some(glyph) = glyph {
        content = content.push(icon(glyph, 16.0, tint));
    }
    let style = if selected { TextStyle::MENU_ITEM } else { TextStyle::CALL };
    content = content.push(text(label, style).wrapping(Wrapping::None));
    let control = control(container(content).center_y(Fill).clip(true))
        .on_press(message)
        .width(Fill)
        .height(component::CONTROL)
        .padding(Padding::from([0.0, SP3]));
    widgets::button(control, move |status| {
        if selected {
            style::flat(paint.acc_soft(), paint.acc_soft_fg(), R2)
        } else {
            style::flat(paint.layered(Color::TRANSPARENT, status), paint.fg_2(), R2)
        }
    })
}

#[must_use]
pub fn sidebar<'a, M: 'a>(paint: &Paint, width: f32, items: Vec<Element<'a, M>>) -> Element<'a, M> {
    let background = paint.sidebar_bg();
    container(scrollable(paint, iced::widget::Column::with_children(items).spacing(2).padding(SP2)))
        .width(width)
        .height(Fill)
        .style(move |_| style::rounded(background, 0.0))
        .into()
}

#[must_use]
pub fn surface<'a, M: 'a>(background: Color, content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    container(content).width(Fill).height(Fill).style(move |_| style::rounded(background, 0.0)).into()
}

#[must_use]
pub fn popover<'a, M: 'a>(paint: &Paint, content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    let paint = *paint;
    iced::widget::opaque(widgets::surface(
        container(content).padding(SP1).style(move |_| style::popover(&paint)),
        &paint,
        Level::El3,
        R3,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use glide_theme::{Accent, Appearance};

    #[test]
    fn disabled_buttons_read_as_disabled_in_every_tone() {
        let paint = Paint::new(Appearance::Dark, Accent::Blue);
        for tone in [Tone::Accent, Tone::Neutral, Tone::Ghost, Tone::Danger] {
            assert_eq!(button_style(&paint, tone, control::Status::Disabled).text_color, paint.fg_4());
        }
    }

    #[test]
    fn the_accent_button_lifts_on_hover_only() {
        let paint = Paint::new(Appearance::Light, Accent::Teal);
        let active = button_style(&paint, Tone::Accent, control::Status::Active);
        let hovered = button_style(&paint, Tone::Accent, control::Status::Hovered);
        assert_eq!(active.background, Some(Background::Color(paint.acc_bg())));
        assert_eq!(hovered.background, Some(Background::Color(paint.acc_bg_hover())));
    }

    #[test]
    fn a_switch_is_accent_only_while_on() {
        let paint = Paint::new(Appearance::Dark, Accent::Blue);
        let on = switch_style(&paint, toggler::Status::Active { is_toggled: true });
        let off = switch_style(&paint, toggler::Status::Active { is_toggled: false });
        assert_eq!(on.background, Background::Color(paint.acc_bg()));
        assert_ne!(off.background, Background::Color(paint.acc_bg()));
    }
}

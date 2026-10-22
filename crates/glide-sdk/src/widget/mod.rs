mod buttons;
mod controls;
mod feedback;
mod focus;
mod inputs;
mod layout;
mod popover;
mod selection;
mod strip;

use std::cell::RefCell;
use std::collections::HashMap;

use glide_iced::Paint;
pub use glide_iced::glyph::Icon;
use glide_iced::style::{self, SP1, to_iced};
pub use glide_iced::text::{Text, Wrapping, text};
use glide_theme::TextStyle;
use glide_theme::shape::R_ROUND;
use iced::widget::{Space, container, scrollable as scroll};
use iced::{Background, Border, Color, Element, Fill, Shadow};

pub use buttons::{
    Size, Tone, button, button_style, compact_button, danger, ghost, icon_button, linked, outline, primary, secondary,
    sized_button, sized_icon_button, sized_toggle_button, toggle_button, tonal,
};
pub use controls::{
    CHECK, Check, SwitchSize, check_box, chevron, progress, radio_mark, sized_switch, slider, spinner, stepped_slider,
    switch,
};
pub use feedback::{Severity, avatar, badge, banner, count_badge, empty_state, keycap, skeleton};
pub use focus::{Focus, RING, focusable};
pub use inputs::{
    Validity, field, input_style, password_field, search_field, search_input, stepper, text_area_style, text_field,
    validated_field,
};
pub use layout::{
    action_card, action_row, card, card_style, expander, facts, list, list_row, section, select_row, sidebar,
    sidebar_item, surface, switch_row, well,
};
pub use popover::{Popup, Tooltip, dropdown, menu_button, tooltip};
pub use selection::{
    DOT, DOT_ACTIVE, ICON_SEGMENT, Indicator, SWATCH, breadcrumbs, checkbox, chip, icon_segmented, page_dots,
    radio_group, segmented, sidebar_list, swatches, tabs, track,
};
pub use strip::{Mark, TAB_GAP, TAB_PAD, Tab, TabStrip, tab_slot, tab_strip};

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

pub(crate) fn mix(from: Color, to: Color, amount: f32) -> Color {
    to_iced(
        glide_theme::Color([from.r, from.g, from.b, from.a])
            .lerp_premultiplied(glide_theme::Color([to.r, to.g, to.b, to.a]), amount.clamp(0.0, 1.0)),
    )
}

pub trait Glyph: Copy {
    fn draw<'a, M: 'a>(self, side: f32, tint: Color) -> Element<'a, M>;
}

impl Glyph for Icon {
    fn draw<'a, M: 'a>(self, side: f32, tint: Color) -> Element<'a, M> {
        glide_iced::glyph::glyph(self, side, tint)
    }
}

#[must_use]
pub fn icon<'a, M: 'a>(icon: impl Glyph, side: f32, tint: Color) -> Element<'a, M> {
    icon.draw(side, tint)
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
pub fn popover<'a, M: 'a>(paint: &Paint, content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    let paint = *paint;
    iced::widget::opaque(glide_iced::widgets::surface(
        container(content).padding(SP1).style(move |_| style::popover(&paint)),
        &paint,
        glide_theme::elevation::Level::El3,
        glide_iced::style::R3,
    ))
}

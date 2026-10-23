mod buttons;
mod cards;
mod controls;
mod editor;
mod feedback;
mod find;
mod focus;
mod form;
mod inputs;
mod items;
mod layout;
mod media;
pub mod mock;
mod panes;
mod pointer;
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
    Size, Tone, button, button_style, compact_button, danger, flat_toggle_button, ghost, icon_button, link, linked,
    outline, primary, secondary, sized_button, sized_icon_button, sized_toggle_button, toggle_button, tonal,
};
pub use cards::{
    choice_card, grid, hero_card, legend, page_header, preview_card, preview_ring_style, sample_well, stat_tile,
    tinted_card, tinted_card_style, usage_bar, usage_card,
};
pub use controls::{
    CHECK, Check, SwitchSize, check_box, chevron, paused_progress, progress, radio_indicator, radio_mark,
    sized_spinner, sized_switch, slider, spinner, stepped_slider, switch,
};
pub use editor::{editor, markdown_settings, markdown_view, text_area};
pub use feedback::{
    Severity, Task, avatar, badge, banner, busy_label, count_badge, dot, empty_note, empty_state, icon_tile, initials,
    key_combo, keycap, letter_tile, noted_banner, skeleton, status_badge, task_card,
};
pub use find::{FIND_FIELD, FindBar, INFO_BAR, bar_strip, find_bar, info_bar};
pub use focus::{Focus, RING, focusable};
pub use form::{action_bar, boxed_input, form_row, hint_line, key_values, sheet};
pub use inputs::{
    Validity, bare_text_area_style, field, hinted_search_input, inline_field, inline_style, input_style,
    password_field, search_field, search_input, stepper, text_area_style, text_field, validated_field,
};
pub use items::{corner_badge, item_ink, item_tile, section_title, selection_band, table_row};
pub use layout::{
    Group, HeaderCell, ListRow, ROW_HEIGHT, STATUS_BAR, SidebarItem, TABLE_HEADER, TREE_INDENT, TREE_ROW, action_card,
    action_row, card, card_style, disclosure, expander, facts, inert, list, list_row, profile_card, section,
    select_row, sidebar, sidebar_item, status_bar, status_button, status_strip, surface, switch_row, table_header,
    tree_row, well,
};
pub use media::{
    Art, art, frame_button, level_control, level_meter, photo, photo_button, picture, range_slider, slider_readout,
    svg_glyph,
};
pub use panes::{code_block, heading_row, palette_swatch, pane, shortcut_list, split_view, stat_rows};
pub use pointer::{ContextArea, context_area, reveal};
pub use popover::{Popup, Tooltip, bubble, dropdown, menu_button, tooltip};
pub use selection::{
    Crumb, DOT, DOT_ACTIVE, ICON_SEGMENT, Indicator, SWATCH, breadcrumbs, checkbox, chip, crumb_trail, icon_segmented,
    page_dots, radio_group, segmented, sidebar_list, swatch, swatches, tabs, track,
};
pub use strip::{Mark, TAB_GAP, TAB_PAD, Tab, TabStrip, tab_slot, tab_strip};

type Measured = (String, u32, u16, u32, u64, u32);

fn shape(content: &str, style: TextStyle, wrap: Option<f32>) -> (f32, usize) {
    use iced::advanced::graphics::text::{self as graphics, cosmic_text};
    thread_local! {
        static CACHE: RefCell<HashMap<Measured, (f32, usize)>> = RefCell::new(HashMap::new());
    }
    let key = (
        content.to_string(),
        style.size_px.to_bits(),
        style.weight,
        style.tracking_em.to_bits(),
        glide_iced::font::revision(),
        wrap.map_or(u32::MAX, f32::to_bits),
    );
    if let Some(shaped) = CACHE.with_borrow(|cache| cache.get(&key).copied()) {
        return shaped;
    }
    let mut system = graphics::font_system().write().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut buffer =
        cosmic_text::Buffer::new(system.raw(), cosmic_text::Metrics::new(style.size_px, style.line_height_px));
    buffer.set_size(system.raw(), wrap, None);
    buffer.set_wrap(system.raw(), cosmic_text::Wrap::WordOrGlyph);
    let attributes = graphics::to_attributes(iced::Font::DEFAULT)
        .weight(cosmic_text::Weight(style.weight))
        .letter_spacing(style.tracking_em);
    buffer.set_text(system.raw(), content, &attributes, cosmic_text::Shaping::Advanced, None);
    buffer.shape_until_scroll(system.raw(), false);
    let shaped = (graphics::measure(&buffer).0.width.ceil(), buffer.layout_runs().count().max(1));
    CACHE.with_borrow_mut(|cache| {
        if cache.len() > 1024 {
            cache.clear();
        }
        cache.insert(key, shaped);
    });
    shaped
}

#[must_use]
pub fn measure(content: &str, style: TextStyle) -> f32 {
    shape(content, style, None).0
}

#[must_use]
pub fn measure_lines(content: &str, style: TextStyle, width: f32) -> usize {
    shape(content, style, Some(width.max(1.0))).1
}

#[must_use]
pub fn breakable(name: &str) -> String {
    match name.rfind('.').filter(|&dot| dot > 0) {
        Some(dot) => format!("{}\u{200B}{}", &name[..dot], &name[dot..]),
        None => name.to_string(),
    }
}

#[must_use]
pub fn elide_lines(name: &str, width: f32, style: TextStyle, lines: usize) -> String {
    let fits = |text: &str| measure_lines(&breakable(text), style, width) <= lines;
    let shown = if fits(name) {
        name.to_string()
    } else {
        let (mut low, mut high) = (ELIDE_FLOOR, name.chars().count());
        while high - low > 1 {
            let middle = low.midpoint(high);
            if fits(&ellipsize(name, middle)) {
                low = middle;
            } else {
                high = middle;
            }
        }
        ellipsize(name, low)
    };
    breakable(&shown)
}

const ELIDE_FLOOR: usize = 6;

#[must_use]
pub fn ellipsize(name: &str, max: usize) -> String {
    let count = name.chars().count();
    if count <= max {
        return name.to_string();
    }
    let tail =
        name.rfind('.').filter(|&dot| dot > 0).map_or(0, |dot| name[dot..].chars().count() + 4).max(3).min(max / 2);
    let head = max.saturating_sub(tail + 1);
    let start: String = name.chars().take(head).collect();
    let end: String = name.chars().skip(count - tail).collect();
    format!("{start}…{end}")
}

#[must_use]
pub fn elide(name: &str, width: f32, style: TextStyle) -> String {
    if measure(name, style) <= width {
        return name.to_string();
    }
    let (mut low, mut high) = (ELIDE_FLOOR, name.chars().count());
    while high - low > 1 {
        let middle = low.midpoint(high);
        if measure(&ellipsize(name, middle), style) <= width {
            low = middle;
        } else {
            high = middle;
        }
    }
    ellipsize(name, low)
}

#[must_use]
pub fn elide_end(sentence: &str, width: f32, style: TextStyle) -> String {
    if measure(sentence, style) <= width {
        return sentence.to_string();
    }
    let chars: Vec<char> = sentence.chars().collect();
    let shortened = |count: usize| format!("{}…", chars[..count].iter().collect::<String>().trim_end());
    let (mut low, mut high) = (0, chars.len());
    while high - low > 1 {
        let middle = low.midpoint(high);
        if measure(&shortened(middle), style) <= width {
            low = middle;
        } else {
            high = middle;
        }
    }
    if low == 0 { String::new() } else { shortened(low) }
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
pub fn scrollable_x<'a, M: 'a>(paint: &Paint, content: impl Into<Element<'a, M>>) -> scroll::Scrollable<'a, M> {
    let paint = *paint;
    let bar = scroll::Scrollbar::new().width(6).scroller_width(6).spacing(SP1);
    scroll::Scrollable::with_direction(content, scroll::Direction::Horizontal(bar))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_name_keeps_its_extension_and_fits_the_measured_width() {
        let name = "a-very-long-holiday-photo-from-the-beach.jpeg";
        let shown = elide(name, 140.0, TextStyle::CAP);
        assert!(shown.rsplit('.').next() == Some("jpeg") && shown.contains('…'), "{shown}");
        assert!(measure(&shown, TextStyle::CAP) <= 140.0);
        assert_eq!(elide("notes.txt", 400.0, TextStyle::CAP), "notes.txt");
    }

    #[test]
    fn a_sentence_is_cut_at_its_end_or_dropped_when_nothing_fits() {
        let sentence = "Items in the Trash are deleted for good when emptied";
        let shown = elide_end(sentence, 120.0, TextStyle::CAP);
        assert!(shown.ends_with('…') && sentence.starts_with(shown.trim_end_matches('…')), "{shown}");
        assert!(measure(&shown, TextStyle::CAP) <= 120.0);
        assert_eq!(elide_end(sentence, 2.0, TextStyle::CAP), "");
    }
}

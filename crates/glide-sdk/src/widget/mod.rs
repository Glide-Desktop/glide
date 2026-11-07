mod buttons;
mod cards;
pub mod cells;
mod controls;
mod document;
mod editor;
mod feedback;
mod find;
mod focus;
mod form;
mod gallery;
mod inputs;
mod insight;
mod inspector;
mod items;
mod layout;
mod media;
pub mod mock;
mod panes;
pub mod player;
mod pointer;
mod popover;
mod secrets;
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
    Size, Tone, button, button_style, compact_button, danger, flat_toggle_button, full_button, ghost, glyph_button,
    icon_button, link, linked, outline, primary, secondary, sized_button, sized_icon_button, sized_toggle_button,
    toggle_button, tonal, tone_ink,
};
pub use cards::{
    HERO_TITLE, PAGE_LEAD, PAGE_TITLE, choice_card, compare_card, grid, hero_card, legend, page_header, preview_card,
    preview_ring_style, sample_well, stat_tile, tinted_card, tinted_card_style, usage_bar, usage_card,
};
pub use controls::{
    CHECK, Check, SwitchSize, ZOOM_TRACK, check_box, chevron, fold_chevron, paused_progress, progress, radio_indicator,
    radio_mark, sized_spinner, sized_switch, slider, spinner, stepped_slider, switch, zoom_control,
};
pub use document::{
    Area, Cover, Layer, Marked, PAGE_GAP, PIN, PageEvent, PageTool, Paint2 as MarkStyle, Paper, THUMB_WIDTH, TOOL,
    ThumbMarks, Verdict, continue_card, doc_cover, external_link, floating, form_input_style, hit_row, link_preview,
    note_card, page_layer, page_sheet, page_thumb, paper_picker, progress_line, quote_row, reading_progress, sign_here,
    signature_pad, signature_status, tool_button, tool_divider, tool_label,
};
pub use editor::{
    EDITOR_BAR, EditorContent, EditorRenderer, ScrolledEditor, View as EditorView, editor, editor_track,
    markdown_settings, markdown_view, scrolled_editor, text_area,
};
pub use feedback::{
    Severity, Task, avatar, badge, banner, busy_label, count_badge, dot, empty_note, empty_state, icon_tile, initials,
    inked_tile, key_combo, keycap, letter_tile, noted_banner, skeleton, status_badge, task_card, task_row,
};
pub use find::{
    FIND_FIELD, FindBar, INFO_BAR, arriving, arriving_over, bar_strip, find_bar, info_bar, popping, rising, swapped,
};
pub use focus::{Focus, RING, focusable};
pub use form::{action_bar, boxed_input, form_row, hint_line, key_values, sheet};
pub use gallery::{
    CoverCard, CropRect, DIAL_HEIGHT, DIAL_WIDTH, FILM_CURRENT, FILM_THUMB, GLASS, GLASS_BUTTON, GLASS_INK, HISTOGRAM,
    STAGE, Tile, adjust_slider, checkerboard, cover_card, crop_overlay, dial_value, exif_strip, film_thumb,
    format_badge, glass, glass_button, glass_circle, glass_divider, glass_icon, glass_label, glass_style, histogram,
    photo_tile, straighten_dial,
};
pub use inputs::{
    Validity, bare_text_area_style, field, field_with, hinted_search_input, inline_field, inline_style, input_style,
    password_field, search_field, search_input, secret_input, secret_input_with, sized_search_input, stepper,
    text_area_style, text_field, validated_field,
};
pub use insight::{
    FACT_LABEL, Fact, Glance, INFO_BUTTON, Issue, Stage, applies_chip, choice_tiles, fact_grid, info_button, kv_grid,
    labelled_switch, reset_action, section_heading, signal_bars, stage_track, status_chips, status_text,
};
pub use inspector::{INSPECTOR_PREVIEW, INSPECTOR_WIDTH, Inspector};
pub use items::{corner_badge, item_ink, item_tile, section_title, selection_band, table_row};
pub use layout::{
    CHANGED_MARK, Group, HeaderCell, ISLAND_GAP, ISLAND_TRAVEL, ListRow, ROW_HEIGHT, ROW_NOTE, ROW_TITLE, SECTION_NOTE,
    SIDEBAR_DENSE_ROW, STATUS_BAR, SidebarItem, TABLE_CELL, TABLE_HEADER, TABLE_ROW, TREE_INDENT, TREE_ROW,
    action_card, action_row, card, card_style, detail_band, disclosure, expander, facts, inert, island_motion, list,
    list_row, profile_card, row_icon, section, select_row, sidebar, sidebar_item, split_actions, status_bar,
    status_button, status_islands, status_strip, surface, switch_row, table_header, tree_row, well,
};
pub use media::{
    Art, art, captioned_picture, frame_button, level_control, level_meter, photo, photo_button, picture, range_slider,
    slider_readout, svg_glyph,
};
pub use panes::{code_block, heading_row, palette_swatch, pane, shortcut_list, split_view, stat_rows};
pub use pointer::{ContextArea, context_area, reveal};
pub use popover::{
    MenuAction, Popup, RowMarks, Tooltip, bubble, dropdown, dropdown_compact, dropdown_decorated, dropdown_with,
    menu_button, tooltip,
};
pub use secrets::{
    CODE, CodeSize, DiffField, ENTRY_ROW, FIELD_ROW, Glyphs, HALO, MASK, QrModules, SECRET, SecretField, Strength,
    SyncLine, VaultState, caption_row, classify, code_cells, code_input, copy_button, countdown_ring, diff_card,
    entry_row, field_row, glyph_tile, grouped_code, halo, link_field, mark_tile, masked, only_digits, option_card,
    password_text, qr_code, runs, secret_field, secret_value, stat_choice, strength_line, strength_meter, sync_card,
    tinted_tile, totp, vault_chip, wide_button,
};
pub use selection::{
    Crumb, DOT, DOT_ACTIVE, ICON_SEGMENT, Indicator, SWATCH, breadcrumbs, checkbox, chip, crumb_trail, icon_segmented,
    icon_segmented_width, page_dots, radio_group, segmented, segmented_fill, sidebar_list, swatch, swatches, tabs,
    track,
};
pub use strip::{Mark, Tab, TabStrip, tab_strip};

type Measured = (String, u32, u16, u32, u64, u32, bool);

const SHAPED_GENERATION: usize = 4096;

#[derive(Debug, Default)]
struct Generations<K, V> {
    young: HashMap<K, V>,
    old: HashMap<K, V>,
}

impl<K: std::hash::Hash + Eq, V: Copy> Generations<K, V> {
    fn get(&mut self, key: &K) -> Option<V> {
        if let Some(value) = self.young.get(key) {
            return Some(*value);
        }
        let (key, value) = self.old.remove_entry(key)?;
        self.insert(key, value);
        Some(value)
    }

    fn insert(&mut self, key: K, value: V) {
        if self.young.len() >= SHAPED_GENERATION {
            self.old = std::mem::take(&mut self.young);
        }
        self.young.insert(key, value);
    }
}

fn shape(content: &str, style: TextStyle, wrap: Option<f32>) -> (f32, usize) {
    use iced::advanced::graphics::text::{self as graphics, cosmic_text};
    thread_local! {
        static CACHE: RefCell<Generations<Measured, (f32, usize)>> = RefCell::new(Generations::default());
    }
    let key = (
        content.to_string(),
        style.size_px.to_bits(),
        style.weight,
        style.tracking_em.to_bits(),
        glide_iced::font::revision(),
        wrap.map_or(u32::MAX, f32::to_bits),
        style.tabular,
    );
    if let Some(shaped) = CACHE.with_borrow_mut(|cache| cache.get(&key)) {
        return shaped;
    }
    let mut system = graphics::font_system().write().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut buffer =
        cosmic_text::Buffer::new(system.raw(), cosmic_text::Metrics::new(style.size_px, style.line_height_px));
    buffer.set_size(system.raw(), wrap, None);
    buffer.set_wrap(system.raw(), cosmic_text::Wrap::WordOrGlyph);
    let font = if style.tabular { glide_iced::font::TABULAR } else { iced::Font::DEFAULT };
    let attributes =
        graphics::to_attributes(font).weight(cosmic_text::Weight(style.weight)).letter_spacing(style.tracking_em);
    buffer.set_text(system.raw(), content, &attributes, cosmic_text::Shaping::Advanced, None);
    buffer.shape_until_scroll(system.raw(), false);
    let shaped = (graphics::measure(&buffer).0.width.ceil(), buffer.layout_runs().count().max(1));
    CACHE.with_borrow_mut(|cache| cache.insert(key, shaped));
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
    let opens = |stem: &str| stem.contains(|c: char| c.is_whitespace() || c == '-');
    match name.rfind('.').filter(|&dot| dot > 0 && !opens(&name[..dot])) {
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
        let count = name.chars().count();
        let (mut low, mut high) = (ELIDE_FLOOR.min(count), count);
        while high.saturating_sub(low) > 1 {
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
    let count = name.chars().count();
    let (mut low, mut high) = (ELIDE_FLOOR.min(count), count);
    while high.saturating_sub(low) > 1 {
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
pub fn overlay_scrollable<'a, M: 'a>(paint: &Paint, content: impl Into<Element<'a, M>>) -> scroll::Scrollable<'a, M> {
    let paint = *paint;
    let bar = scroll::Scrollbar::new().width(6).scroller_width(6).margin(1);
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
    fn a_full_shape_cache_keeps_what_was_used_recently() {
        let mut cache = Generations::default();
        cache.insert(0, 0);
        for key in 1..SHAPED_GENERATION {
            cache.insert(key, key);
        }
        assert_eq!(cache.get(&0), Some(0), "a key still in use survives the first turnover");
        for key in SHAPED_GENERATION..SHAPED_GENERATION * 2 {
            cache.insert(key, key);
        }
        assert_eq!(cache.get(&0), Some(0), "touching it moved it to the young generation");
        assert_eq!(cache.get(&1), None, "an untouched key ages out instead of every key at once");
    }

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

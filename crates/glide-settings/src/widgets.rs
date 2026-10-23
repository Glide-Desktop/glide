use glide_sdk::widget as sdk;
use iced::widget::{container, space, text};
use iced::{Color, Element, Length, Padding};

use crate::icons::Icon;
use crate::theme::{self, ThemeState, Tone};

pub use glide_sdk::widget::{Group, ListRow as Row, grid};

impl From<ThemeState> for glide_sdk::Paint {
    fn from(t: ThemeState) -> Self {
        paint(&t)
    }
}

impl sdk::Glyph for Icon {
    fn draw<'a, M: 'a>(self, side: f32, tint: Color) -> Element<'a, M> {
        sdk::svg_glyph(self.handle(), side, tint)
    }
}

#[must_use]
pub fn paint(t: &ThemeState) -> glide_sdk::Paint {
    t.paint
}

#[must_use]
pub fn icon<'a, M: 'a>(glyph: Icon, size: f32, color: Color) -> Element<'a, M> {
    sdk::icon(glyph, size, color)
}

#[must_use]
pub fn tile<'a, M: 'a>(glyph: Icon, color: Color, size: f32) -> Element<'a, M> {
    sdk::icon_tile(glyph, color, size)
}

#[must_use]
pub fn switch<'a, M: Clone + 'a>(t: ThemeState, value: bool, on_toggle: impl Fn(bool) -> M + 'a) -> Element<'a, M> {
    sdk::switch(&paint(&t), value, Some(on_toggle))
}

#[must_use]
pub fn switch_disabled<'a, M: Clone + 'a>(t: ThemeState, value: bool) -> Element<'a, M> {
    sdk::switch(&paint(&t), value, None::<fn(bool) -> M>)
}

#[must_use]
pub fn segmented<'a, M: Clone + 'a, T: Copy + PartialEq + 'a>(
    t: ThemeState, options: Vec<(T, String)>, selected: T, on_select: impl Fn(T) -> M,
) -> Element<'a, M> {
    let options: Vec<(T, std::borrow::Cow<'a, str>)> =
        options.into_iter().map(|(value, label)| (value, label.into())).collect();
    sdk::segmented(&paint(&t), &options, selected, on_select)
}

fn action_button<'a, M: Clone + 'a>(
    t: &ThemeState, label: impl text::IntoFragment<'a>, message: Option<M>, tone: sdk::Tone,
) -> Element<'a, M> {
    sdk::button(&paint(t), label.into_fragment(), tone, message)
}

#[must_use]
pub fn primary<'a, M: Clone + 'a>(
    t: ThemeState, label: impl text::IntoFragment<'a>, message: Option<M>,
) -> Element<'a, M> {
    action_button(&t, label, message, sdk::Tone::Accent)
}

#[must_use]
pub fn secondary<'a, M: Clone + 'a>(
    t: ThemeState, label: impl text::IntoFragment<'a>, message: Option<M>,
) -> Element<'a, M> {
    action_button(&t, label, message, sdk::Tone::Neutral)
}

#[must_use]
pub fn tonal<'a, M: Clone + 'a>(
    t: ThemeState, label: impl text::IntoFragment<'a>, message: Option<M>,
) -> Element<'a, M> {
    action_button(&t, label, message, sdk::Tone::Tonal)
}

#[must_use]
pub fn danger<'a, M: Clone + 'a>(
    t: ThemeState, label: impl text::IntoFragment<'a>, message: Option<M>,
) -> Element<'a, M> {
    action_button(&t, label, message, sdk::Tone::Danger)
}

#[must_use]
pub fn icon_button<'a, M: Clone + 'a>(t: ThemeState, glyph: Icon, message: Option<M>) -> Element<'a, M> {
    sdk::icon_button(&paint(&t), glyph, "", message, false)
}

#[must_use]
pub fn dropdown<'a, M: Clone + 'a, T: ToString + PartialEq + Clone + 'a>(
    t: ThemeState, options: Vec<T>, selected: Option<T>, on_select: impl Fn(T) -> M + 'a, width: f32,
) -> Element<'a, M> {
    flexible(sdk::dropdown(&paint(&t), options, selected, "", on_select), width)
}

#[must_use]
pub fn flexible<'a, M: 'a>(content: impl Into<Element<'a, M>>, max_width: f32) -> Element<'a, M> {
    container(content).width(Length::FillPortion(1)).max_width(max_width).into()
}

#[must_use]
pub fn inert<'a, M: 'a>(t: ThemeState, enabled: bool, content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    sdk::inert(&paint(&t), enabled, content)
}

#[must_use]
pub fn chooser<'a, M: Clone + 'a, T: ToString + PartialEq + Clone + 'a>(
    t: ThemeState, options: Vec<T>, placeholder: &str, on_select: impl Fn(T) -> M + 'a, width: f32,
) -> Element<'a, M> {
    flexible(sdk::dropdown(&paint(&t), options, None, placeholder, on_select), width)
}

#[must_use]
pub fn field<'a, M: Clone + 'a>(
    t: ThemeState, placeholder: &str, value: &str, on_input: impl Fn(String) -> M + 'a, on_submit: Option<M>,
    invalid: bool,
) -> iced::widget::TextInput<'a, M> {
    let validity = if invalid { sdk::Validity::Invalid } else { sdk::Validity::Neutral };
    sdk::boxed_input(&paint(&t), placeholder, value, validity).on_input(on_input).on_submit_maybe(on_submit)
}

#[must_use]
pub fn sheet<'a, M: 'a>(t: ThemeState, rows: Vec<Element<'a, M>>) -> Element<'a, M> {
    sdk::sheet(&paint(&t), rows)
}

#[must_use]
pub fn lines<'a, M: 'a>(t: ThemeState, info: Vec<(String, String)>) -> Element<'a, M> {
    sdk::key_values(&paint(&t), info)
}

#[must_use]
pub fn form_row<'a, M: 'a>(t: ThemeState, label: &str, hint: Option<&str>, control: Element<'a, M>) -> Element<'a, M> {
    sdk::form_row(&paint(&t), label, hint, control)
}

#[must_use]
pub fn note<'a, M: 'a>(t: ThemeState, glyph: Icon, message: &str) -> Element<'a, M> {
    sdk::hint_line(&paint(&t), glyph, message)
}

#[must_use]
pub fn actions<'a, M: 'a>(buttons: Vec<Element<'a, M>>) -> Element<'a, M> {
    sdk::action_bar(buttons)
}

#[must_use]
pub fn chevron<'a, M: 'a>(t: ThemeState, open: bool) -> Element<'a, M> {
    sdk::chevron(&paint(&t), open)
}

#[must_use]
pub fn value_slider<'a, M: Clone + 'a>(
    t: ThemeState, range: std::ops::RangeInclusive<f64>, value: f64, step: f64, on_change: impl Fn(f64) -> M + 'a,
    on_release: Option<M>,
) -> Element<'a, M> {
    let range = (*range.start() as f32)..=(*range.end() as f32);
    let change = move |value: f32| on_change(f64::from(value));
    sdk::stepped_slider(&paint(&t), range, value as f32, step as f32, Some(change), on_release)
}

#[must_use]
pub fn banner<'a, M: Clone + 'a>(
    t: ThemeState, tone: Tone, glyph: Icon, message: impl text::IntoFragment<'a>, action: Option<Element<'a, M>>,
) -> Element<'a, M> {
    let message = message.into_fragment();
    sdk::noted_banner(&paint(&t), tone, glyph, "", &message, action.into_iter().collect())
}

#[must_use]
pub fn empty_state<'a, M: 'a>(
    t: ThemeState, glyph: Icon, heading: impl text::IntoFragment<'a>, description: impl text::IntoFragment<'a>,
) -> Element<'a, M> {
    sdk::empty_note(&paint(&t), glyph, &heading.into_fragment(), &description.into_fragment())
}

#[must_use]
pub fn list_empty<'a, M: 'a>(
    t: ThemeState, glyph: Icon, heading: impl text::IntoFragment<'a>, description: impl text::IntoFragment<'a>,
) -> Element<'a, M> {
    empty_state(t, glyph, heading, description)
}

#[must_use]
pub fn sample_well<'a, M: 'a>(t: ThemeState, content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    container(sdk::sample_well(&paint(&t), content)).padding(Padding::from([6.0, theme::ROW_PADDING_X])).into()
}

#[must_use]
pub fn gap<'a, M: 'a>(height: f32) -> Element<'a, M> {
    space().height(height).into()
}

#[must_use]
pub fn spinner_label<'a, M: 'a>(t: ThemeState, label: impl text::IntoFragment<'a>) -> Element<'a, M> {
    sdk::busy_label(&paint(&t), label.into_fragment())
}

#[must_use]
pub fn micro_label<'a, M: 'a>(t: ThemeState, label: &str) -> Element<'a, M> {
    sdk::text(label.to_string(), glide_sdk::theme::TextStyle::MICRO).color(t.fg_3()).into()
}

#[must_use]
pub fn status_dot<'a, M: 'a>(color: Color, size: f32) -> Element<'a, M> {
    sdk::dot(color, size)
}

#[must_use]
pub fn avatar<'a, M: 'a>(t: ThemeState, name: &str, size: f32) -> Element<'a, M> {
    sdk::avatar(&paint(&t), name, size)
}

#[must_use]
pub fn pill<'a, M: 'a>(t: ThemeState, tone: Tone, label: impl text::IntoFragment<'a>) -> Element<'a, M> {
    sdk::badge(&paint(&t), label.into_fragment(), tone)
}

#[must_use]
pub fn status_pill<'a, M: 'a>(t: ThemeState, tone: Tone, label: impl text::IntoFragment<'a>) -> Element<'a, M> {
    sdk::status_badge(&paint(&t), label.into_fragment(), tone)
}

#[must_use]
pub fn chip<'a, M: Clone + 'a>(
    t: ThemeState, label: impl text::IntoFragment<'a>, selected: bool, on_press: Option<M>,
) -> Element<'a, M> {
    sdk::chip(&paint(&t), label.into_fragment(), selected, on_press)
}

#[must_use]
pub fn key_symbol(name: &str) -> String {
    let trimmed = name.trim();
    let symbol = match trimmed.to_ascii_lowercase().as_str() {
        "super" | "logo" | "mod4" | "meta" | "win" | "cmd" | "command" => "⌘",
        "shift" => "⇧",
        "ctrl" | "control" | "primary" => "Ctrl",
        "alt" | "mod1" | "option" => "Alt",
        "return" | "enter" => "↵",
        "tab" => "⇥",
        "backspace" => "⌫",
        "delete" | "del" => "Del",
        "page_up" | "pageup" | "prior" => "PgUp",
        "page_down" | "pagedown" | "next" => "PgDn",
        "plus" => "+",
        _ => return crate::services::keybinds::key_label(trimmed),
    };
    symbol.to_string()
}

#[must_use]
pub fn key_cap<'a, M: 'a>(t: ThemeState, label: &str) -> Element<'a, M> {
    sdk::keycap(&paint(&t), label.to_string())
}

#[must_use]
pub fn key_combo<'a, M: 'a>(t: ThemeState, combo: &str) -> Element<'a, M> {
    sdk::key_combo(&paint(&t), combo, key_symbol)
}

#[must_use]
pub fn colour_swatch<'a, M: Clone + 'a>(
    t: ThemeState, color: Color, selected: bool, on_press: Option<M>, size: f32,
) -> Element<'a, M> {
    sdk::swatch(&paint(&t), color, selected, on_press, size)
}

#[must_use]
pub fn preview_card<'a, M: Clone + 'a>(
    t: ThemeState, content: impl Into<Element<'a, M>>, label: impl text::IntoFragment<'a>, selected: bool,
    on_press: Option<M>,
) -> Element<'a, M> {
    sdk::preview_card(&paint(&t), content, label.into_fragment(), selected, on_press)
}

#[must_use]
pub fn radio_mark<'a, M: 'a>(t: ThemeState, selected: bool) -> Element<'a, M> {
    sdk::radio_indicator(&paint(&t), selected)
}

#[must_use]
pub fn radio_card<'a, M: Clone + 'a>(
    t: ThemeState, illustration: impl Into<Element<'a, M>>, heading: impl text::IntoFragment<'a>,
    subtitle: Option<String>, selected: bool, on_press: M,
) -> Element<'a, M> {
    sdk::choice_card(&paint(&t), illustration, heading.into_fragment(), subtitle, selected, on_press)
}

#[must_use]
pub fn stat_tile<'a, M: 'a>(
    t: ThemeState, glyph: Icon, color: Color, value: impl text::IntoFragment<'a>, label: impl text::IntoFragment<'a>,
) -> Element<'a, M> {
    sdk::stat_tile(&paint(&t), glyph, color, value.into_fragment(), label.into_fragment())
}

#[must_use]
pub fn stat_row<'a, M: 'a>(t: ThemeState, tiles: Vec<Element<'a, M>>) -> Element<'a, M> {
    let per_line = if t.compact { 2 } else { tiles.len() };
    grid(tiles, per_line, 12.0)
}

#[must_use]
pub fn progress_bar<'a, M: 'a>(t: ThemeState, fraction: f32) -> Element<'a, M> {
    sdk::progress(&paint(&t), Some(fraction))
}

#[must_use]
pub fn usage_card<'a, M: 'a>(
    t: ThemeState, heading: impl text::IntoFragment<'a>, summary: impl text::IntoFragment<'a>,
    segments: Vec<(f32, Color, String, String)>,
) -> Element<'a, M> {
    sdk::usage_card(&paint(&t), heading.into_fragment(), summary.into_fragment(), segments)
}

#[must_use]
pub fn info_grid<'a, M: Clone + 'a>(t: ThemeState, rows: Vec<(String, String)>) -> Element<'a, M> {
    sdk::list(&paint(&t), info_rows(t, rows))
}

#[must_use]
pub fn info_rows<'a, M: Clone + 'a>(t: ThemeState, rows: Vec<(String, String)>) -> Vec<Element<'a, M>> {
    let paint = paint(&t);
    rows.into_iter()
        .map(|(key, value)| {
            let value = sdk::text(value, glide_sdk::theme::TextStyle::CALL).color(paint.fg_1());
            Row::new(paint, key).suffix(value).build()
        })
        .collect()
}

#[must_use]
pub fn section_hero<'a, M: 'a>(
    t: ThemeState, glyph: Icon, color: Color, heading: impl text::IntoFragment<'a>,
    description: impl text::IntoFragment<'a>, trailing: Option<Element<'a, M>>,
) -> Element<'a, M> {
    sdk::hero_card(&paint(&t), glyph, color, heading.into_fragment(), description.into_fragment(), trailing, t.compact)
}

#[must_use]
pub fn disclosure<'a, M: 'a>(t: ThemeState, value: Option<String>) -> Element<'a, M> {
    sdk::disclosure(&paint(&t), value)
}

#[must_use]
pub fn link<'a, M: Clone + 'a>(
    t: ThemeState, label: impl text::IntoFragment<'a>, message: Option<M>,
) -> Element<'a, M> {
    sdk::link(&paint(&t), label.into_fragment(), message)
}

#[must_use]
pub fn page_title<'a, M: 'a>(
    t: ThemeState, glyph: Icon, color: Color, eyebrow: Option<&str>, heading: impl text::IntoFragment<'a>,
    description: Option<&'a str>, trailing: Option<Element<'a, M>>,
) -> Element<'a, M> {
    sdk::page_header(&paint(&t), glyph, color, eyebrow, heading.into_fragment(), description, trailing)
}

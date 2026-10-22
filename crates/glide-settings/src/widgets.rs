use glide_sdk::widget as sdk;
use iced::widget::{Column, button, column, container, opaque, row, rule, space, stack, svg, text, text_input};
use iced::{Alignment, Color, Element, Fill, Length, Padding};

use crate::icons::Icon;
use crate::theme::{self, ThemeState};

#[must_use]
pub fn icon<'a, M: 'a>(glyph: Icon, size: f32, color: Color) -> Element<'a, M> {
    let opaque = Color { a: 1.0, ..color };
    svg(glyph.handle())
        .width(size)
        .height(size)
        .opacity(color.a)
        .style(move |_: &iced::Theme, _| svg::Style { color: Some(opaque) })
        .into()
}

#[must_use]
pub fn tile<'a, M: 'a>(glyph: Icon, color: Color, size: f32) -> Element<'a, M> {
    container(icon(glyph, (size * 0.66).round(), Color::WHITE))
        .width(size)
        .height(size)
        .center_x(size)
        .center_y(size)
        .style(move |_: &iced::Theme| theme::tile_style_sized(color, size))
        .into()
}

#[must_use]
pub fn title<'a, M: 'a>(label: impl text::IntoFragment<'a>, size: f32) -> Element<'a, M> {
    text(label).size(size).font(theme::SEMIBOLD).into()
}

#[must_use]
pub fn caption<'a, M: 'a>(t: ThemeState, label: impl text::IntoFragment<'a>) -> Element<'a, M> {
    text(label).size(13).color(t.fg_2()).into()
}

#[must_use]
pub fn separator<'a, M: 'a>(t: ThemeState) -> Element<'a, M> {
    container(rule::horizontal(1).style(move |_: &iced::Theme| theme::rule_style(&t)))
        .padding(Padding::ZERO.left(theme::ROW_PADDING_X).right(theme::ROW_PADDING_X))
        .into()
}

pub use glide_sdk::widget::{Group, ListRow as Row};

impl From<ThemeState> for glide_sdk::Paint {
    fn from(t: ThemeState) -> Self {
        paint(&t)
    }
}

#[must_use]
pub fn min_height<'a, M: 'a>(content: impl Into<Element<'a, M>>, height: f32) -> Element<'a, M> {
    row![space().height(height), content.into()].align_y(Alignment::Center).into()
}

#[must_use]
pub fn paint(t: &ThemeState) -> glide_sdk::Paint {
    glide_sdk::Paint { tokens: t.tokens, alpha: 1.0 }
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

#[must_use]
pub fn badge<'a, M: 'a>(label: impl text::IntoFragment<'a>, bg: Color, fg: Color) -> Element<'a, M> {
    container(text(label).size(11.5).font(theme::SEMIBOLD))
        .padding([2, 8])
        .style(move |_: &iced::Theme| theme::badge_style(bg, fg))
        .into()
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
    let paint = paint(&t);
    let color = if message.is_some() { t.fg_2() } else { t.fg_4() };
    let control = button(
        container(icon(glyph, 16.0, color))
            .center_x(Length::Fixed(theme::SMALL_CONTROL_HEIGHT))
            .center_y(Length::Fixed(theme::SMALL_CONTROL_HEIGHT)),
    )
    .on_press_maybe(message.clone())
    .padding(0);
    let control = glide_sdk::glide_iced::widgets::button(control, move |status| {
        sdk::button_style(&paint, sdk::Tone::Ghost, status)
    });
    sdk::focusable(&paint, control, glide_sdk::theme::R2).on_activate(message).into()
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
    let content = content.into();
    if enabled {
        return content;
    }
    let veil = iced::Color { a: 0.55, ..t.card_bg() };
    stack![
        content,
        opaque(
            container(space())
                .width(Fill)
                .height(Fill)
                .style(move |_: &iced::Theme| theme::solid(veil, theme::radius_r1())),
        ),
    ]
    .into()
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
    let mut input = text_input(placeholder, value).on_input(on_input).padding([8, 11]).size(13).style(
        move |_: &iced::Theme, status| {
            let mut style = theme::text_input_style(&t, status);
            if invalid {
                style.border.color = t.danger();
                style.border.width = 1.5;
            }
            style
        },
    );
    if let Some(message) = on_submit {
        input = input.on_submit(message);
    }
    input
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
    t: ThemeState, tone: Color, glyph: Icon, message: impl text::IntoFragment<'a>, action: Option<Element<'a, M>>,
) -> Element<'a, M> {
    let mut line =
        row![icon(glyph, 18.0, tone), text(message).size(13).width(Fill),].spacing(10).align_y(Alignment::Center);
    if let Some(action) = action {
        line = line.push(action);
    }
    container(line).padding([10, 14]).width(Fill).style(move |_: &iced::Theme| theme::banner_style(&t, tone)).into()
}

#[must_use]
pub fn empty_state<'a, M: 'a>(
    t: ThemeState, glyph: Icon, heading: impl text::IntoFragment<'a>, description: impl text::IntoFragment<'a>,
) -> Element<'a, M> {
    container(
        column![
            icon(glyph, 40.0, t.fg_3()),
            text(heading).size(16).font(theme::SEMIBOLD),
            text(description).size(13).color(t.fg_2()).align_x(Alignment::Center),
        ]
        .spacing(8)
        .align_x(Alignment::Center),
    )
    .padding(32)
    .center_x(Fill)
    .into()
}

#[must_use]
pub fn sample_well<'a, M: 'a>(t: ThemeState, content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    let well = t.sunken_bg();
    container(
        container(content)
            .padding([12, 16])
            .width(Fill)
            .style(move |_: &iced::Theme| theme::solid(well, theme::radius_r2())),
    )
    .padding([6.0, theme::ROW_PADDING_X])
    .into()
}

#[must_use]
pub fn gap<'a, M: 'a>(height: f32) -> Element<'a, M> {
    space().height(height).into()
}

#[must_use]
pub fn spinner_label<'a, M: 'a>(t: ThemeState, label: impl text::IntoFragment<'a>) -> Element<'a, M> {
    row![icon(Icon::Refresh, 14.0, t.fg_3()), text(label).size(12.5).color(t.fg_3())]
        .spacing(6)
        .align_y(Alignment::Center)
        .into()
}

#[must_use]
pub fn micro_label<'a, M: 'a>(t: ThemeState, label: &str) -> Element<'a, M> {
    text(label.to_uppercase()).size(11).font(theme::SEMIBOLD).color(t.fg_3()).into()
}

#[must_use]
pub fn status_dot<'a, M: 'a>(color: Color, size: f32) -> Element<'a, M> {
    container(space())
        .width(size)
        .height(size)
        .style(move |_: &iced::Theme| theme::solid(color, theme::radius_round()))
        .into()
}

#[must_use]
pub fn avatar<'a, M: 'a>(t: ThemeState, initials: impl text::IntoFragment<'a>, size: f32) -> Element<'a, M> {
    container(text(initials).size((size * 0.38).round()).font(theme::SEMIBOLD).color(Color::WHITE))
        .center_x(Length::Fixed(size))
        .center_y(Length::Fixed(size))
        .style(move |_: &iced::Theme| theme::avatar_style(&t))
        .into()
}

#[must_use]
pub fn pill<'a, M: 'a>(t: ThemeState, tone: theme::Tone, label: impl text::IntoFragment<'a>) -> Element<'a, M> {
    let (bg, fg) = theme::tone_colors(&t, tone);
    container(text(label).size(11.5).font(theme::SEMIBOLD).color(fg))
        .padding([3, 9])
        .style(move |_: &iced::Theme| theme::badge_style(bg, fg))
        .into()
}

#[must_use]
pub fn status_pill<'a, M: 'a>(t: ThemeState, tone: theme::Tone, label: impl text::IntoFragment<'a>) -> Element<'a, M> {
    let (bg, fg) = theme::tone_colors(&t, tone);
    container(
        row![status_dot(fg, 6.0), text(label).size(11.5).font(theme::SEMIBOLD).color(fg)]
            .spacing(5)
            .align_y(Alignment::Center),
    )
    .padding([3, 9])
    .style(move |_: &iced::Theme| theme::badge_style(bg, fg))
    .into()
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
    let single = label.chars().count() == 1;
    let cap = container(text(label.to_string()).size(12).font(theme::SEMIBOLD).color(t.fg_2()))
        .center_y(Length::Fixed(KEY_CAP_SIZE))
        .style(move |_: &iced::Theme| theme::keycap_style(&t));
    if single { cap.center_x(Length::Fixed(KEY_CAP_SIZE)).into() } else { cap.padding([0, 7]).into() }
}

pub const KEY_CAP_SIZE: f32 = 22.0;

#[must_use]
pub fn key_combo<'a, M: 'a>(t: ThemeState, combo: &str) -> Element<'a, M> {
    let mut keys = row![].spacing(4).align_y(Alignment::Center);
    for part in combo.split('+').filter(|part| !part.trim().is_empty()) {
        keys = keys.push(key_cap(t, &key_symbol(part)));
    }
    keys.into()
}

fn contrast_on(color: Color) -> Color {
    if color.r * 0.299 + color.g * 0.587 + color.b * 0.114 > 0.62 {
        Color::from_rgb(0.1, 0.1, 0.1)
    } else {
        Color::WHITE
    }
}

#[must_use]
pub fn colour_swatch<'a, M: Clone + 'a>(
    t: ThemeState, color: Color, selected: bool, on_press: Option<M>, size: f32,
) -> Element<'a, M> {
    let mark: Element<'a, M> =
        if selected { icon(Icon::Check, (size * 0.5).round(), contrast_on(color)) } else { space().into() };
    let dot = container(mark)
        .center_x(Length::Fixed(size))
        .center_y(Length::Fixed(size))
        .style(move |_: &iced::Theme| theme::swatch_style(color, &t));
    let ring =
        container(dot).padding(theme::RING_GAP).style(move |_: &iced::Theme| theme::swatch_ring_style(&t, selected));
    button(ring)
        .on_press_maybe(on_press)
        .padding(0)
        .style(move |_: &iced::Theme, status| theme::plain_button_style(&t, status))
        .into()
}

#[must_use]
pub fn preview_card<'a, M: Clone + 'a>(
    t: ThemeState, content: impl Into<Element<'a, M>>, label: impl text::IntoFragment<'a>, selected: bool,
    on_press: Option<M>,
) -> Element<'a, M> {
    let framed = button(content.into())
        .on_press_maybe(on_press)
        .padding(theme::RING_GAP)
        .style(move |_: &iced::Theme, status| theme::ring_button_style(&t, selected, status));
    column![
        framed,
        text(label).size(12.5).font(if selected { theme::SEMIBOLD } else { iced::Font::DEFAULT }).color(if selected {
            t.acc_text()
        } else {
            t.fg_2()
        }),
    ]
    .spacing(6)
    .align_x(Alignment::Center)
    .into()
}

#[must_use]
pub fn radio_mark<'a, M: 'a>(t: ThemeState, selected: bool) -> Element<'a, M> {
    let mark: Element<'a, M> = if selected { icon(Icon::Check, 10.0, t.acc_fg()) } else { space().into() };
    container(mark)
        .center_x(Length::Fixed(16.0))
        .center_y(Length::Fixed(16.0))
        .style(move |_: &iced::Theme| theme::radio_mark_style(&t, selected))
        .into()
}

#[must_use]
pub fn radio_card<'a, M: Clone + 'a>(
    t: ThemeState, illustration: impl Into<Element<'a, M>>, heading: impl text::IntoFragment<'a>,
    subtitle: Option<String>, selected: bool, on_press: M,
) -> Element<'a, M> {
    let mut labels = column![text(heading).size(13.5).font(theme::SEMIBOLD)].spacing(1).width(Fill);
    if let Some(subtitle) = subtitle.filter(|text| !text.is_empty()) {
        labels = labels.push(text(subtitle).size(12).color(t.fg_2()));
    }
    let body = column![
        container(illustration.into()).center_x(Fill),
        row![radio_mark(t, selected), labels].spacing(8).align_y(Alignment::Center),
    ]
    .spacing(10);
    button(body)
        .on_press(on_press)
        .padding(10)
        .width(Fill)
        .style(move |_: &iced::Theme, status| theme::selectable_card_style(&t, selected, status))
        .into()
}

#[must_use]
pub fn stat_tile<'a, M: 'a>(
    t: ThemeState, glyph: Icon, color: Color, value: impl text::IntoFragment<'a>, label: impl text::IntoFragment<'a>,
) -> Element<'a, M> {
    container(
        column![
            tile(glyph, color, 28.0),
            column![text(value).size(24).font(theme::SEMIBOLD).color(t.fg_1()), text(label).size(12).color(t.fg_2()),],
        ]
        .spacing(10),
    )
    .padding(14)
    .width(Fill)
    .style(move |_: &iced::Theme| theme::group_style(&t))
    .into()
}

#[must_use]
pub fn stat_row<'a, M: 'a>(t: ThemeState, tiles: Vec<Element<'a, M>>) -> Element<'a, M> {
    let per_line = if t.compact { 2 } else { tiles.len() };
    grid(tiles, per_line, 12.0)
}

#[must_use]
pub fn grid<'a, M: 'a>(cells: Vec<Element<'a, M>>, per_line: usize, spacing: f32) -> Element<'a, M> {
    let per_line = per_line.max(1);
    let mut lines = Column::new().spacing(spacing);
    let mut cells = cells.into_iter().peekable();
    while cells.peek().is_some() {
        let line: Vec<Element<'a, M>> = cells.by_ref().take(per_line).collect();
        let missing = per_line - line.len();
        lines = lines.push(
            iced::widget::Row::with_children(line)
                .extend((0..missing).map(|_| space().width(Fill).into()))
                .spacing(spacing),
        );
    }
    lines.into()
}

fn portion(fraction: f32) -> u16 {
    (fraction.clamp(0.0, 1.0) * 1000.0).round() as u16
}

#[must_use]
pub fn usage_bar<'a, M: 'a>(t: ThemeState, segments: &[(f32, Color)], height: f32) -> Element<'a, M> {
    let mut bar = iced::widget::Row::new().spacing(2).width(Fill).height(height);
    let mut used: u16 = 0;
    for &(fraction, color) in segments {
        let part = portion(fraction).min(1000 - used);
        if part == 0 {
            continue;
        }
        used += part;
        bar = bar.push(
            container(space())
                .width(Length::FillPortion(part))
                .height(Fill)
                .style(move |_: &iced::Theme| theme::solid(color, theme::radius_r1())),
        );
    }
    if used < 1000 {
        bar = bar.push(
            container(space())
                .width(Length::FillPortion(1000 - used))
                .height(Fill)
                .style(move |_: &iced::Theme| theme::track_style(&t)),
        );
    }
    bar.into()
}

#[must_use]
pub fn progress_bar<'a, M: 'a>(t: ThemeState, fraction: f32) -> Element<'a, M> {
    sdk::progress(&paint(&t), Some(fraction))
}

#[must_use]
pub fn legend<'a, M: 'a>(t: ThemeState, items: Vec<(Color, String, String)>) -> Element<'a, M> {
    let mut line = iced::widget::Row::new().spacing(16).align_y(Alignment::Center);
    for (color, label, value) in items {
        let mut entry =
            row![status_dot(color, 8.0), text(label).size(12).color(t.fg_1())].spacing(6).align_y(Alignment::Center);
        if !value.is_empty() {
            entry = entry.push(text(value).size(12).color(t.fg_3()));
        }
        line = line.push(entry);
    }
    line.wrap().vertical_spacing(6).into()
}

#[must_use]
pub fn usage_card<'a, M: 'a>(
    t: ThemeState, heading: impl text::IntoFragment<'a>, summary: impl text::IntoFragment<'a>,
    segments: Vec<(f32, Color, String, String)>,
) -> Element<'a, M> {
    let bars: Vec<(f32, Color)> = segments.iter().map(|(fraction, color, _, _)| (*fraction, *color)).collect();
    let items = segments.into_iter().map(|(_, color, label, value)| (color, label, value)).collect();
    container(
        column![
            row![
                text(heading).size(14).font(theme::SEMIBOLD).color(t.fg_1()).width(Fill),
                text(summary).size(13).color(t.fg_2()),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
            usage_bar(t, &bars, 10.0),
            legend(t, items),
        ]
        .spacing(12),
    )
    .padding(16)
    .width(Fill)
    .style(move |_: &iced::Theme| theme::group_style(&t))
    .into()
}

#[must_use]
pub fn info_grid<'a, M: 'a>(t: ThemeState, rows: Vec<(String, String)>) -> Element<'a, M> {
    let rows = rows
        .into_iter()
        .map(|(key, value)| {
            container(
                row![
                    text(key).size(13).color(t.fg_2()).width(Length::FillPortion(2)),
                    text(value).size(13).color(t.fg_1()).width(Length::FillPortion(3)).align_x(Alignment::End),
                ]
                .spacing(12)
                .align_y(Alignment::Center),
            )
            .padding([10.0, theme::ROW_PADDING_X])
            .into()
        })
        .collect();
    sdk::list(&paint(&t), rows)
}

#[must_use]
pub fn list_empty<'a, M: 'a>(
    t: ThemeState, glyph: Icon, heading: impl text::IntoFragment<'a>, description: impl text::IntoFragment<'a>,
) -> Element<'a, M> {
    let well = t.fill_1();
    container(
        column![
            container(icon(glyph, 22.0, t.fg_3()))
                .center_x(Length::Fixed(44.0))
                .center_y(Length::Fixed(44.0))
                .style(move |_: &iced::Theme| theme::solid(well, theme::radius_r3())),
            text(heading).size(14).font(theme::SEMIBOLD).color(t.fg_1()),
            text(description).size(12.5).color(t.fg_2()).align_x(Alignment::Center),
        ]
        .spacing(6)
        .align_x(Alignment::Center),
    )
    .padding([20, 16])
    .center_x(Fill)
    .into()
}

#[must_use]
pub fn section_hero<'a, M: 'a>(
    t: ThemeState, glyph: Icon, color: Color, heading: impl text::IntoFragment<'a>,
    description: impl text::IntoFragment<'a>, trailing: Option<Element<'a, M>>,
) -> Element<'a, M> {
    let mut labels = column![
        text(heading).size(22).font(theme::SEMIBOLD).color(t.fg_1()).wrapping(text::Wrapping::WordOrGlyph),
        text(description).size(13).color(t.fg_2()),
    ]
    .spacing(4)
    .width(Fill);
    let mut end = None;
    match trailing {
        Some(trailing) if t.compact => labels = labels.push(container(trailing).padding(Padding::ZERO.top(4))),
        other => end = other,
    }
    let mut line = row![tile(glyph, color, 64.0), labels].spacing(18).align_y(Alignment::Center);
    if let Some(trailing) = end {
        line = line.push(trailing);
    }
    container(line).padding(20).width(Fill).style(move |_: &iced::Theme| theme::hero_card_style(&t, color)).into()
}

#[must_use]
pub fn disclosure<'a, M: 'a>(t: ThemeState, value: Option<String>) -> Element<'a, M> {
    let mut line = row![].spacing(6).align_y(Alignment::Center);
    if let Some(value) = value.filter(|value| !value.is_empty()) {
        line = line.push(text(value).size(13).color(t.fg_2()));
    }
    line.push(icon(Icon::ChevronRight, 14.0, t.fg_3())).into()
}

#[must_use]
pub fn link<'a, M: Clone + 'a>(
    t: ThemeState, label: impl text::IntoFragment<'a>, message: Option<M>,
) -> Element<'a, M> {
    button(text(label).size(13).font(theme::SEMIBOLD))
        .on_press_maybe(message)
        .padding([4, 8])
        .style(move |_: &iced::Theme, status| {
            let mut style = theme::button_ghost(&t, status);
            if !matches!(status, iced::widget::button::Status::Disabled) {
                style.text_color = t.acc_text();
            }
            style
        })
        .into()
}

#[must_use]
pub fn page_title<'a, M: 'a>(
    t: ThemeState, glyph: Icon, color: Color, eyebrow: Option<&str>, heading: impl text::IntoFragment<'a>,
    description: Option<&'a str>, trailing: Option<Element<'a, M>>,
) -> Element<'a, M> {
    let mut labels = Column::new().spacing(2).width(Fill);
    if let Some(eyebrow) = eyebrow.filter(|text| !text.is_empty()) {
        labels = labels.push(micro_label(t, eyebrow));
    }
    labels = labels.push(text(heading).size(theme::PAGE_TITLE_SIZE).font(theme::SEMIBOLD).color(t.fg_1()));
    if let Some(description) = description.filter(|text| !text.is_empty()) {
        labels = labels.push(text(description).size(14).color(t.fg_2()));
    }
    let mut line = row![tile(glyph, color, theme::PAGE_TILE_SIZE), labels].spacing(16).align_y(Alignment::Center);
    if let Some(trailing) = trailing {
        line = line.push(trailing);
    }
    line.into()
}

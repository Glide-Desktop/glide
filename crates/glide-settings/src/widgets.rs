use iced::widget::{
    Column, button, column, container, opaque, pick_list, row, rule, slider, space, stack, svg, text, text_input,
    toggler,
};
use iced::{Alignment, Color, Element, Fill, Length, Padding};

use crate::icons::Icon;
use crate::theme::{self, ThemeState};

#[must_use]
pub fn icon<'a, M: 'a>(glyph: Icon, size: f32, color: Color) -> Element<'a, M> {
    svg(glyph.handle())
        .width(size)
        .height(size)
        .style(move |_: &iced::Theme, _| svg::Style { color: Some(color) })
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

fn control_label<'a>(label: impl text::IntoFragment<'a>, font: iced::Font) -> iced::widget::Text<'a> {
    text(label).size(13).font(font).height(Fill).align_y(Alignment::Center)
}

pub struct Group<'a, M> {
    theme: ThemeState,
    title: Option<String>,
    description: Option<String>,
    suffix: Option<Element<'a, M>>,
    rows: Vec<Element<'a, M>>,
}

impl<'a, M: 'a> Group<'a, M> {
    #[must_use]
    pub fn new(theme: ThemeState) -> Self {
        Self { theme, title: None, description: None, suffix: None, rows: Vec::new() }
    }

    #[must_use]
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    #[must_use]
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    #[must_use]
    pub fn suffix(mut self, suffix: impl Into<Element<'a, M>>) -> Self {
        self.suffix = Some(suffix.into());
        self
    }

    #[must_use]
    pub fn push(mut self, row: impl Into<Element<'a, M>>) -> Self {
        self.rows.push(row.into());
        self
    }

    #[must_use]
    pub fn push_maybe(self, row: Option<impl Into<Element<'a, M>>>) -> Self {
        match row {
            Some(row) => self.push(row),
            None => self,
        }
    }

    #[must_use]
    pub fn extend(mut self, rows: impl IntoIterator<Item = Element<'a, M>>) -> Self {
        self.rows.extend(rows);
        self
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    #[must_use]
    pub fn build(self) -> Element<'a, M> {
        let t = self.theme;
        let mut outer = Column::new().spacing(8);
        if self.title.is_some() || self.description.is_some() || self.suffix.is_some() {
            let mut heading = Column::new().spacing(2).width(Fill);
            if let Some(label) = self.title {
                heading = heading.push(text(label).size(14).font(theme::SEMIBOLD).color(t.fg_1()));
            }
            if let Some(description) = self.description {
                heading = heading.push(text(description).size(13).color(t.fg_2()));
            }
            let mut header = row![heading].align_y(Alignment::Center).spacing(12);
            if let Some(suffix) = self.suffix {
                header = header.push(suffix);
            }
            outer = outer.push(container(header).padding(Padding::ZERO.left(4)));
        }
        if !self.rows.is_empty() {
            outer = outer.push(boxed_list(&t, self.rows));
        }
        outer.into()
    }
}

fn boxed_list<'a, M: 'a>(t: &ThemeState, rows: Vec<Element<'a, M>>) -> Element<'a, M> {
    let t = *t;
    let count = rows.len();
    let mut list = Column::new();
    for (index, row) in rows.into_iter().enumerate() {
        list = list.push(row);
        if index + 1 < count {
            list = list.push(separator(t));
        }
    }
    container(list).padding(theme::GROUP_INSET).width(Fill).style(move |_: &iced::Theme| theme::group_style(&t)).into()
}

pub struct Row<'a, M> {
    theme: ThemeState,
    title: String,
    subtitle: Option<String>,
    prefix: Option<Element<'a, M>>,
    suffix: Option<Element<'a, M>>,
    on_press: Option<M>,
    dimmed: bool,
}

impl<'a, M: Clone + 'a> Row<'a, M> {
    #[must_use]
    pub fn new(theme: ThemeState, title: impl Into<String>) -> Self {
        Self { theme, title: title.into(), subtitle: None, prefix: None, suffix: None, on_press: None, dimmed: false }
    }

    #[must_use]
    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
        let subtitle = subtitle.into();
        if !subtitle.is_empty() {
            self.subtitle = Some(subtitle);
        }
        self
    }

    #[must_use]
    pub fn prefix(mut self, prefix: impl Into<Element<'a, M>>) -> Self {
        self.prefix = Some(prefix.into());
        self
    }

    #[must_use]
    pub fn suffix(mut self, suffix: impl Into<Element<'a, M>>) -> Self {
        self.suffix = Some(suffix.into());
        self
    }

    #[must_use]
    pub fn on_press(mut self, message: M) -> Self {
        self.on_press = Some(message);
        self
    }

    #[must_use]
    pub fn dimmed(mut self, dimmed: bool) -> Self {
        self.dimmed = dimmed;
        self
    }

    #[must_use]
    pub fn build(self) -> Element<'a, M> {
        let t = self.theme;
        let title_color = if self.dimmed { t.fg_3() } else { t.fg_1() };
        let mut labels = column![text(self.title).size(14).color(title_color)].spacing(1).width(Fill);
        if let Some(subtitle) = self.subtitle {
            labels = labels.push(text(subtitle).size(12).color(t.fg_2()));
        }
        let mut line = row![].spacing(12).align_y(Alignment::Center);
        if let Some(prefix) = self.prefix {
            line = line.push(prefix);
        }
        line = line.push(labels);
        if let Some(suffix) = self.suffix {
            line = line.push(suffix);
        }
        let body = container(min_height(line, theme::row_content_height()))
            .padding([theme::ROW_PADDING_Y, theme::ROW_PADDING_X])
            .width(Fill);
        match self.on_press {
            Some(message) => button(body)
                .on_press(message)
                .padding(0)
                .width(Fill)
                .style(move |_: &iced::Theme, status| theme::row_button_style(&t, status))
                .into(),
            None => body.into(),
        }
    }
}

#[must_use]
pub fn min_height<'a, M: 'a>(content: impl Into<Element<'a, M>>, height: f32) -> Element<'a, M> {
    row![space().height(height), content.into()].align_y(Alignment::Center).into()
}

#[must_use]
pub fn switch<'a, M: 'a>(t: ThemeState, value: bool, on_toggle: impl Fn(bool) -> M + 'a) -> Element<'a, M> {
    styled_toggler(&t, value).on_toggle(on_toggle).into()
}

#[must_use]
pub fn switch_disabled<'a, M: 'a>(t: ThemeState, value: bool) -> Element<'a, M> {
    styled_toggler(&t, value).into()
}

fn styled_toggler<'a, M: 'a>(t: &ThemeState, value: bool) -> iced::widget::Toggler<'a, M> {
    let t = *t;
    toggler(value).size(theme::SWITCH_SIZE).style(move |_: &iced::Theme, status| theme::toggle_style(&t, status))
}

#[must_use]
pub fn segmented<'a, M: Clone + 'a, T: Copy + PartialEq + 'a>(
    t: ThemeState, options: Vec<(T, String)>, selected: T, on_select: impl Fn(T) -> M,
) -> Element<'a, M> {
    let mut segments = row![].spacing(2);
    for (value, label) in options {
        let active = value == selected;
        segments = segments.push(
            button(
                text(label)
                    .size(13)
                    .font(if active { theme::SEMIBOLD } else { iced::Font::DEFAULT })
                    .align_x(Alignment::Center)
                    .align_y(Alignment::Center)
                    .width(Fill)
                    .height(Fill),
            )
            .on_press(on_select(value))
            .padding([0, 14])
            .height(theme::SMALL_CONTROL_HEIGHT)
            .width(Length::Shrink)
            .style(move |_: &iced::Theme, status| theme::segment_style(&t, active, status)),
        );
    }
    container(segments)
        .padding((theme::CONTROL_HEIGHT - theme::SMALL_CONTROL_HEIGHT) / 2.0)
        .style(move |_: &iced::Theme| theme::segmented_track_style(&t))
        .into()
}

#[must_use]
pub fn badge<'a, M: 'a>(label: impl text::IntoFragment<'a>, bg: Color, fg: Color) -> Element<'a, M> {
    container(text(label).size(11.5).font(theme::SEMIBOLD))
        .padding([2, 8])
        .style(move |_: &iced::Theme| theme::badge_style(bg, fg))
        .into()
}

fn action_button<'a, M: Clone + 'a>(
    t: &ThemeState, label: impl text::IntoFragment<'a>, message: Option<M>,
    style: fn(&ThemeState, button::Status) -> button::Style,
) -> Element<'a, M> {
    let t = *t;
    button(control_label(label, theme::SEMIBOLD))
        .on_press_maybe(message)
        .padding([0, 16])
        .height(theme::CONTROL_HEIGHT)
        .style(move |_: &iced::Theme, status| style(&t, status))
        .into()
}

#[must_use]
pub fn primary<'a, M: Clone + 'a>(
    t: ThemeState, label: impl text::IntoFragment<'a>, message: Option<M>,
) -> Element<'a, M> {
    action_button(&t, label, message, theme::button_primary)
}

#[must_use]
pub fn secondary<'a, M: Clone + 'a>(
    t: ThemeState, label: impl text::IntoFragment<'a>, message: Option<M>,
) -> Element<'a, M> {
    action_button(&t, label, message, theme::button_secondary)
}

#[must_use]
pub fn tonal<'a, M: Clone + 'a>(
    t: ThemeState, label: impl text::IntoFragment<'a>, message: Option<M>,
) -> Element<'a, M> {
    action_button(&t, label, message, theme::button_tonal)
}

#[must_use]
pub fn danger<'a, M: Clone + 'a>(
    t: ThemeState, label: impl text::IntoFragment<'a>, message: Option<M>,
) -> Element<'a, M> {
    action_button(&t, label, message, theme::button_danger_soft)
}

#[must_use]
pub fn icon_button<'a, M: Clone + 'a>(t: ThemeState, glyph: Icon, message: Option<M>) -> Element<'a, M> {
    let color = if message.is_some() { t.fg_2() } else { t.fg_4() };
    button(
        container(icon(glyph, 16.0, color))
            .center_x(Length::Fixed(theme::SMALL_CONTROL_HEIGHT))
            .center_y(Length::Fixed(theme::SMALL_CONTROL_HEIGHT)),
    )
    .on_press_maybe(message)
    .padding(0)
    .style(move |_: &iced::Theme, status| theme::icon_button_style(&t, status))
    .into()
}

#[must_use]
pub fn dropdown<'a, M: Clone + 'a, T: ToString + PartialEq + Clone + 'a>(
    t: ThemeState, options: Vec<T>, selected: Option<T>, on_select: impl Fn(T) -> M + 'a, width: f32,
) -> Element<'a, M> {
    flexible(
        pick_list(options, selected, on_select)
            .width(Fill)
            .padding([8, 11])
            .text_size(13)
            .style(move |_: &iced::Theme, status| theme::pick_list_style(&t, status))
            .menu_style(move |_: &iced::Theme| theme::menu_style(&t)),
        width,
    )
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
    flexible(
        pick_list(options, None::<T>, on_select)
            .placeholder(placeholder)
            .width(Fill)
            .padding([8, 11])
            .text_size(13)
            .menu_height(Length::Fixed(320.0))
            .style(move |_: &iced::Theme, status| theme::pick_list_style(&t, status))
            .menu_style(move |_: &iced::Theme| theme::menu_style(&t)),
        width,
    )
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
    let mut control = slider(range, value, on_change)
        .step(step)
        .width(Fill)
        .style(move |_: &iced::Theme, status| theme::slider_style(&t, status));
    if let Some(message) = on_release {
        control = control.on_release(message);
    }
    control.into()
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
pub fn hero<'a, M: 'a>(
    t: ThemeState, glyph: Icon, color: Color, heading: impl text::IntoFragment<'a>,
    description: impl text::IntoFragment<'a>, suffix: Option<Element<'a, M>>,
) -> Element<'a, M> {
    let mut line = row![
        tile(glyph, color, 52.0),
        column![text(heading).size(19).font(theme::SEMIBOLD), text(description).size(13).color(t.fg_2()),]
            .spacing(2)
            .width(Fill),
    ]
    .spacing(16)
    .align_y(Alignment::Center);
    if let Some(suffix) = suffix {
        line = line.push(suffix);
    }
    container(line).padding(16).width(Fill).style(move |_: &iced::Theme| theme::group_style(&t)).into()
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
    let mut content = row![].spacing(6).height(Fill).align_y(Alignment::Center);
    if selected {
        content = content.push(icon(Icon::Check, 13.0, t.acc_soft_fg()));
    }
    content = content.push(text(label).size(13).font(if selected { theme::SEMIBOLD } else { iced::Font::DEFAULT }));
    button(content)
        .on_press_maybe(on_press)
        .padding([0, 12])
        .height(theme::SMALL_CONTROL_HEIGHT)
        .style(move |_: &iced::Theme, status| theme::chip_style(&t, selected, status))
        .into()
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
        "escape" | "esc" => "Esc",
        "space" => "Space",
        "backspace" => "⌫",
        "delete" | "del" => "Del",
        "left" => "←",
        "right" => "→",
        "up" => "↑",
        "down" => "↓",
        "page_up" | "pageup" | "prior" => "PgUp",
        "page_down" | "pagedown" | "next" => "PgDn",
        "print" => "PrtSc",
        "plus" => "+",
        "minus" => "−",
        "equal" => "=",
        "grave" => "`",
        "comma" => ",",
        "period" => ".",
        "slash" => "/",
        "backslash" => "\\",
        "semicolon" => ";",
        "apostrophe" => "'",
        "bracketleft" => "[",
        "bracketright" => "]",
        _ => "",
    };
    if !symbol.is_empty() {
        return symbol.to_string();
    }
    let bare = trimmed.strip_prefix("XF86").unwrap_or(trimmed);
    if bare.chars().count() == 1 { bare.to_uppercase() } else { bare.to_string() }
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
pub fn stat_row<'a, M: 'a>(tiles: impl IntoIterator<Item = Element<'a, M>>) -> Element<'a, M> {
    iced::widget::Row::with_children(tiles).spacing(12).into()
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
    let part = portion(fraction);
    let mut fill = row![].width(Fill).height(Fill);
    if part > 0 {
        let accent = t.acc_bg();
        fill = fill.push(
            container(space())
                .width(Length::FillPortion(part))
                .height(Fill)
                .style(move |_: &iced::Theme| theme::solid(accent, theme::radius_r1())),
        );
    }
    if part < 1000 {
        fill = fill.push(space().width(Length::FillPortion(1000 - part)));
    }
    container(fill).width(Fill).height(PROGRESS_HEIGHT).style(move |_: &iced::Theme| theme::track_style(&t)).into()
}

pub const PROGRESS_HEIGHT: f32 = 6.0;

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
    boxed_list(&t, rows)
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
    let mut line = row![
        tile(glyph, color, 64.0),
        column![
            text(heading).size(22).font(theme::SEMIBOLD).color(t.fg_1()),
            text(description).size(13).color(t.fg_2()),
        ]
        .spacing(4)
        .width(Fill),
    ]
    .spacing(18)
    .align_y(Alignment::Center);
    if let Some(trailing) = trailing {
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

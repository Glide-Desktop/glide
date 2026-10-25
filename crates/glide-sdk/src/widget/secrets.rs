use std::borrow::Cow;

use glide_iced::Paint;
use glide_iced::style::{self, R2, R3, SP1, SP2, SP3, SP4};
use glide_iced::widgets;
use glide_theme::shape::R1;
use glide_theme::{TextStyle, component};
use iced::widget::canvas::{self, Frame, Geometry, LineCap, Path, Stroke};
use iced::widget::text::{LineHeight, Span};
use iced::widget::{
    Column, Row, Space, button as control, canvas as surface, column, container, rich_text, row, stack,
};
use iced::widget::{text_input, text_input::TextInput};
use iced::{Alignment, Border, Color, Element, Fill, Font, Length, Padding, Point, Radians, Rectangle, Renderer};
use iced::{Size as Extent, Theme, mouse};

use super::buttons::{Size, Tone, button, sized_icon_button};
use super::feedback::{Severity, badge, dot};
use super::focus::focusable;
use super::{Icon, Wrapping, mix, text};
use crate::anim::{Speed, animate};

pub const SECRET: TextStyle =
    TextStyle { size_px: 14.5, line_height_px: 21.0, weight: 400, tracking_em: 0.02, uppercase: false, tabular: true };
pub const CODE: TextStyle =
    TextStyle { size_px: 18.0, line_height_px: 26.0, weight: 600, tracking_em: 0.06, uppercase: false, tabular: true };
pub const MASK: char = '•';
pub const MASK_LIMIT: usize = 18;
pub const METER_SEGMENT: f32 = 28.0;
pub const METER_HEIGHT: f32 = 4.0;
pub const RING: f32 = 28.0;
pub const URGENT_SECONDS: u32 = 5;
pub const DIGIT_WIDTH: f32 = 36.0;
pub const DIGIT_HEIGHT: f32 = 44.0;
pub const DIGIT_LARGE_WIDTH: f32 = 48.0;
pub const DIGIT_LARGE_HEIGHT: f32 = 56.0;

#[must_use]
pub fn masked(secret: &str) -> String {
    std::iter::repeat_n(MASK, secret.chars().count().clamp(1, MASK_LIMIT)).collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Strength {
    Weak,
    Fair,
    Strong,
    VeryStrong,
}

impl Strength {
    #[must_use]
    pub const fn level(self) -> usize {
        match self {
            Self::Weak => 1,
            Self::Fair => 2,
            Self::Strong => 3,
            Self::VeryStrong => 4,
        }
    }

    #[must_use]
    pub const fn severity(self) -> Severity {
        match self {
            Self::Weak => Severity::Danger,
            Self::Fair => Severity::Warn,
            Self::Strong | Self::VeryStrong => Severity::Ok,
        }
    }

    #[must_use]
    pub fn color(self, paint: &Paint) -> Color {
        match self.severity() {
            Severity::Danger => paint.danger(),
            Severity::Warn => paint.warn(),
            _ => paint.ok(),
        }
    }

    #[must_use]
    pub fn of_bits(bits: f64) -> Self {
        if bits < 40.0 {
            Self::Weak
        } else if bits < 60.0 {
            Self::Fair
        } else if bits < 90.0 {
            Self::Strong
        } else {
            Self::VeryStrong
        }
    }
}

#[must_use]
pub fn strength_meter<'a, M: 'a>(paint: &Paint, strength: Option<Strength>) -> Element<'a, M> {
    let paint = *paint;
    let filled = strength.map_or(0, Strength::level);
    let ink = strength.map_or(paint.fill_2(), |strength| strength.color(&paint));
    animate(filled as f32, Speed::COLOR, move |value| {
        let segments = (0..4).map(|index| {
            let value = value.clone();
            let track = paint.fill_2();
            container(Space::new().width(METER_SEGMENT).height(METER_HEIGHT))
                .style(move |_| style::rounded(mix(track, ink, (value.get() - index as f32).clamp(0.0, 1.0)), R1))
                .into()
        });
        Row::with_children(segments).spacing(SP1)
    })
    .into()
}

#[must_use]
pub fn strength_line<'a, M: 'a>(paint: &Paint, strength: Strength, label: impl Into<Cow<'a, str>>) -> Element<'a, M> {
    row![strength_meter(paint, Some(strength)), text(label, TextStyle::CAP).color(strength.color(paint))]
        .spacing(SP2)
        .align_y(Alignment::Center)
        .into()
}

#[must_use]
pub fn secret_value<'a, M: 'a>(paint: &Paint, secret: &str, revealed: bool) -> Element<'a, M> {
    let shown = if revealed { secret.to_string() } else { masked(secret) };
    text(shown, SECRET).color(paint.fg_1()).wrapping(Wrapping::None).into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Glyphs {
    Letter,
    Digit,
    Symbol,
}

#[must_use]
pub fn classify(ch: char) -> Glyphs {
    if ch.is_ascii_digit() {
        Glyphs::Digit
    } else if ch.is_alphabetic() || ch.is_whitespace() {
        Glyphs::Letter
    } else {
        Glyphs::Symbol
    }
}

#[must_use]
pub fn runs(secret: &str) -> Vec<(Glyphs, String)> {
    let mut runs: Vec<(Glyphs, String)> = Vec::new();
    for ch in secret.chars() {
        let kind = classify(ch);
        match runs.last_mut() {
            Some((last, run)) if *last == kind => run.push(ch),
            _ => runs.push((kind, ch.to_string())),
        }
    }
    runs
}

#[must_use]
pub fn password_text<'a, M: 'a>(paint: &Paint, secret: &str, style: TextStyle) -> Element<'a, M> {
    let font = glide_iced::font::TABULAR;
    let spans: Vec<Span<'a, (), Font>> = runs(secret)
        .into_iter()
        .map(|(kind, run)| {
            let color = match kind {
                Glyphs::Letter => paint.fg_1(),
                Glyphs::Digit => paint.acc_text(),
                Glyphs::Symbol => paint.warn(),
            };
            let breakable: String = run.chars().flat_map(|ch| [ch, '\u{200B}']).collect();
            Span::new(breakable).color(color).font(font)
        })
        .collect();
    rich_text::<(), M, Theme, Renderer>(spans)
        .size(style.size_px)
        .line_height(LineHeight::Absolute(style.line_height_px.into()))
        .wrapping(Wrapping::Glyph)
        .width(Fill)
        .into()
}

pub struct SecretField<'a, M> {
    paint: Paint,
    secret: String,
    revealed: bool,
    reveal: Option<(M, &'a str, &'a str)>,
    copy: Option<(M, &'a str)>,
}

impl<'a, M: Clone + 'a> SecretField<'a, M> {
    #[must_use]
    pub fn new(paint: &Paint, secret: impl Into<String>) -> Self {
        Self { paint: *paint, secret: secret.into(), revealed: false, reveal: None, copy: None }
    }

    #[must_use]
    pub fn revealed(mut self, revealed: bool) -> Self {
        self.revealed = revealed;
        self
    }

    #[must_use]
    pub fn on_reveal(mut self, message: M, show: &'a str, hide: &'a str) -> Self {
        self.reveal = Some((message, show, hide));
        self
    }

    #[must_use]
    pub fn on_copy(mut self, message: M, label: &'a str) -> Self {
        self.copy = Some((message, label));
        self
    }

    fn actions(
        paint: &Paint, revealed: bool, reveal: Option<(M, &'a str, &'a str)>, copy: Option<(M, &'a str)>,
    ) -> Row<'a, M> {
        let mut actions = row![].spacing(SP1).align_y(Alignment::Center);
        if let Some((message, show, hide)) = reveal {
            let (glyph, label) = if revealed { (Icon::EyeOff, hide) } else { (Icon::Eye, show) };
            actions = actions.push(sized_icon_button(paint, glyph, label, Some(message), false, Size::Small));
        }
        if let Some((message, label)) = copy {
            actions = actions.push(sized_icon_button(paint, Icon::Copy, label, Some(message), false, Size::Small));
        }
        actions
    }

    #[must_use]
    pub fn bare(self) -> Element<'a, M> {
        let Self { paint, secret, revealed, reveal, copy } = self;
        row![
            container(secret_value(&paint, &secret, revealed)).width(Fill).clip(true),
            Self::actions(&paint, revealed, reveal, copy)
        ]
        .spacing(SP2)
        .align_y(Alignment::Center)
        .into()
    }
}

impl<'a, M: Clone + 'a> From<SecretField<'a, M>> for Element<'a, M> {
    fn from(field: SecretField<'a, M>) -> Self {
        let paint = field.paint;
        let inner = container(field.bare())
            .height(component::CONTROL)
            .padding(Padding::from([0.0, SP1]).left(SP3))
            .center_y(component::CONTROL);
        widgets::field(inner, &paint)
    }
}

#[must_use]
pub fn secret_field<'a, M: Clone + 'a>(paint: &Paint, secret: impl Into<String>) -> SecretField<'a, M> {
    SecretField::new(paint, secret)
}

struct Ring {
    paint: Paint,
    fraction: f32,
    urgent: bool,
}

impl<M> canvas::Program<M> for Ring {
    type State = ();

    fn draw(&self, (): &(), renderer: &Renderer, _: &Theme, bounds: Rectangle, _: mouse::Cursor) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let width = 2.4;
        let center = Point::new(bounds.width / 2.0, bounds.height / 2.0);
        let radius = bounds.width.min(bounds.height) / 2.0 - width / 2.0 - 0.5;
        frame
            .stroke(&Path::circle(center, radius), Stroke::default().with_color(self.paint.fill_2()).with_width(width));
        let fraction = self.fraction.clamp(0.0, 1.0);
        if fraction > 0.0 {
            let start = -std::f32::consts::FRAC_PI_2;
            let arc = Path::new(|path| {
                path.arc(canvas::path::Arc {
                    center,
                    radius,
                    start_angle: Radians(start),
                    end_angle: Radians(start + std::f32::consts::TAU * fraction),
                });
            });
            let ink = if self.urgent { self.paint.danger() } else { self.paint.acc_bg() };
            frame.stroke(&arc, Stroke::default().with_color(ink).with_width(width).with_line_cap(LineCap::Round));
        }
        vec![frame.into_geometry()]
    }
}

#[must_use]
pub fn countdown_ring<'a, M: 'a>(paint: &Paint, seconds_left: u32, period: u32) -> Element<'a, M> {
    let period = period.max(1);
    let urgent = seconds_left <= URGENT_SECONDS;
    let fraction = seconds_left.min(period) as f32 / period as f32;
    let label = text(seconds_left.to_string(), TextStyle { size_px: 10.0, line_height_px: 12.0, ..TextStyle::NUM })
        .color(if urgent { paint.danger() } else { paint.fg_2() });
    stack![surface(Ring { paint: *paint, fraction, urgent }).width(RING).height(RING), container(label).center(RING),]
        .width(RING)
        .height(RING)
        .into()
}

#[must_use]
pub fn grouped_code(code: &str) -> String {
    let digits: Vec<char> = code.chars().filter(|ch| !ch.is_whitespace()).collect();
    let half = match digits.len() {
        6 | 8 => digits.len() / 2,
        7 => 3,
        _ => return digits.into_iter().collect(),
    };
    let (head, tail) = digits.split_at(half);
    format!("{} {}", head.iter().collect::<String>(), tail.iter().collect::<String>())
}

#[must_use]
pub fn totp<'a, M: 'a>(paint: &Paint, code: &str, seconds_left: u32, period: u32) -> Element<'a, M> {
    let urgent = seconds_left <= URGENT_SECONDS;
    let ink = if urgent { paint.danger() } else { paint.acc_text() };
    let paint = *paint;
    let code = grouped_code(code);
    let shown = animate(1.0, Speed::COLOR, move |_| text(code, CODE).color(ink).wrapping(Wrapping::None)).from(0.0);
    row![shown, countdown_ring(&paint, seconds_left, period)].spacing(SP3 - 2.0).align_y(Alignment::Center).into()
}

#[must_use]
pub fn link_field<'a, M: Clone + 'a>(
    paint: &Paint, link: &str, label: impl Into<Cow<'a, str>>, message: Option<M>,
) -> Element<'a, M> {
    let shown = text(link.to_string(), TextStyle { size_px: 13.0, line_height_px: 19.0, ..SECRET })
        .color(paint.fg_1())
        .wrapping(Wrapping::None);
    let lead = super::icon(Icon::Link, 16.0, paint.fg_3());
    let inner = container(row![lead, container(shown).width(Fill).clip(true)].spacing(SP2).align_y(Alignment::Center))
        .height(component::LARGE_CONTROL)
        .padding(Padding::from([0.0, SP3]))
        .center_y(component::LARGE_CONTROL);
    let field = widgets::tinted_field(inner, paint, paint.ok(), paint.ok_soft());
    let copy = super::buttons::sized_button(paint, label, Tone::Accent, Size::Large, message);
    row![container(field).width(Fill), copy].spacing(SP2).align_y(Alignment::Center).into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CodeSize {
    #[default]
    Compact,
    Large,
}

impl CodeSize {
    const fn extent(self) -> (f32, f32, f32) {
        match self {
            Self::Compact => (DIGIT_WIDTH, DIGIT_HEIGHT, 18.0),
            Self::Large => (DIGIT_LARGE_WIDTH, DIGIT_LARGE_HEIGHT, 22.0),
        }
    }
}

fn digit_cell<'a, M: 'a>(
    paint: &Paint, digit: Option<char>, focused: bool, invalid: bool, size: CodeSize,
) -> Element<'a, M> {
    let paint = *paint;
    let (width, height, glyph) = size.extent();
    let content: Element<'a, M> = match digit {
        Some(digit) => text(
            digit.to_string(),
            TextStyle { size_px: glyph, line_height_px: glyph * 1.3, weight: 600, ..TextStyle::NUM },
        )
        .color(paint.fg_1())
        .into(),
        None if focused => container(Space::new().width(1.5).height(glyph + 2.0))
            .style(move |_| style::rounded(paint.acc_bg(), 0.0))
            .into(),
        None => Space::new().into(),
    };
    let (border, halo_color) = if invalid {
        (paint.danger(), Some(paint.danger_soft()))
    } else if focused {
        (paint.acc_bg(), Some(paint.acc_ring()))
    } else {
        (paint.line_2(), None)
    };
    let cell = container(content).center_x(width).center_y(height).style(move |_| container::Style {
        background: Some(paint.view_bg().into()),
        border: Border { color: border, width: 1.0, radius: R2.into() },
        ..container::Style::default()
    });
    halo(cell, halo_color, R2)
}

pub const HALO: f32 = 3.0;

pub fn halo<'a, M: 'a>(child: impl Into<Element<'a, M>>, color: Option<Color>, radius: f32) -> Element<'a, M> {
    container(child)
        .padding(HALO)
        .style(move |_| container::Style {
            border: Border { color: color.unwrap_or(Color::TRANSPARENT), width: HALO, radius: (radius + HALO).into() },
            ..container::Style::default()
        })
        .into()
}

#[must_use]
pub fn code_cells<'a, M: 'a>(
    paint: &Paint, value: &str, count: usize, focused: bool, invalid: bool, size: CodeSize,
) -> Element<'a, M> {
    let digits: Vec<char> = value.chars().filter(char::is_ascii_digit).take(count).collect();
    let cursor = digits.len().min(count.saturating_sub(1));
    let cells = (0..count).map(|index| {
        let halo = focused && index == cursor && (digits.len() < count || index == count - 1);
        digit_cell(paint, digits.get(index).copied(), halo, invalid, size)
    });
    Row::with_children(cells).spacing(SP1 / 2.0).align_y(Alignment::Center).into()
}

#[must_use]
pub fn only_digits(input: &str, count: usize) -> String {
    input.chars().filter(char::is_ascii_digit).take(count).collect()
}

pub struct CodeInput<'a, M> {
    paint: Paint,
    value: String,
    count: usize,
    id: Option<iced::widget::Id>,
    invalid: bool,
    size: CodeSize,
    on_input: Box<dyn Fn(String) -> M + 'a>,
    on_submit: Option<M>,
}

impl<'a, M: Clone + 'a> CodeInput<'a, M> {
    #[must_use]
    pub fn id(mut self, id: impl Into<iced::widget::Id>) -> Self {
        self.id = Some(id.into());
        self
    }

    #[must_use]
    pub fn invalid(mut self, invalid: bool) -> Self {
        self.invalid = invalid;
        self
    }

    #[must_use]
    pub fn size(mut self, size: CodeSize) -> Self {
        self.size = size;
        self
    }

    #[must_use]
    pub fn on_submit(mut self, message: M) -> Self {
        self.on_submit = Some(message);
        self
    }
}

#[must_use]
pub fn code_input<'a, M: Clone + 'a>(
    paint: &Paint, value: &str, count: usize, on_input: impl Fn(String) -> M + 'a,
) -> CodeInput<'a, M> {
    CodeInput {
        paint: *paint,
        value: value.to_string(),
        count,
        id: None,
        invalid: false,
        size: CodeSize::Compact,
        on_input: Box::new(on_input),
        on_submit: None,
    }
}

impl<'a, M: Clone + 'a> From<CodeInput<'a, M>> for Element<'a, M> {
    fn from(input: CodeInput<'a, M>) -> Self {
        input.build()
    }
}

impl<'a, M: Clone + 'a> CodeInput<'a, M> {
    fn build(self) -> Element<'a, M> {
        let Self { paint, value, count, id, invalid, size, on_input, on_submit } = self;
        let cells = code_cells(&paint, &value, count, true, invalid, size);
        let mut capture: TextInput<'a, M> = text_input("", &value)
            .on_input(move |typed| on_input(only_digits(&typed, count)))
            .size(1)
            .padding(0)
            .width(Fill)
            .style(move |_, _| text_input::Style {
                background: Color::TRANSPARENT.into(),
                border: Border::default(),
                icon: Color::TRANSPARENT,
                placeholder: Color::TRANSPARENT,
                value: Color::TRANSPARENT,
                selection: Color::TRANSPARENT,
            });
        if let Some(id) = id {
            capture = capture.id(id);
        }
        if let Some(submit) = on_submit {
            capture = capture.on_submit(submit);
        }
        let (_, height, _) = size.extent();
        stack![cells, container(capture).height(height + 2.0 * HALO).width(Fill)].into()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VaultState {
    Synced,
    Pending,
    Local,
    Unlocked,
    Current,
}

#[must_use]
pub fn vault_chip<'a, M: 'a>(paint: &Paint, label: impl Into<Cow<'a, str>>, state: VaultState) -> Element<'a, M> {
    let paint = *paint;
    let (background, ink, weight) = match state {
        VaultState::Unlocked => (paint.ok_soft(), paint.ok(), 500),
        VaultState::Current => (paint.acc_soft(), paint.acc_soft_fg(), 600),
        _ => (paint.fill_1(), paint.fg_2(), 500),
    };
    let lead: Element<'a, M> = match state {
        VaultState::Synced => dot(paint.ok(), 6.0),
        VaultState::Pending => dot(paint.warn(), 6.0),
        VaultState::Local => dot(paint.fg_4(), 6.0),
        VaultState::Current => dot(paint.acc_bg(), 6.0),
        VaultState::Unlocked => super::icon(Icon::Check, 14.0, paint.ok()),
    };
    let label = text(label, TextStyle { weight, ..TextStyle::CAP }).color(ink).wrapping(Wrapping::None);
    container(row![lead, label].spacing(SP1 + 2.0).align_y(Alignment::Center))
        .height(22)
        .padding(Padding::from([0.0, SP2]))
        .center_y(22)
        .style(move |_| style::rounded(background, R1))
        .into()
}

pub struct DiffField {
    pub label: String,
    pub value: String,
    pub differs: bool,
    pub extra: Option<Strength>,
}

#[must_use]
pub fn diff_card<'a, M: Clone + 'a>(
    paint: &Paint, title: impl Into<Cow<'a, str>>, subtitle: impl Into<Cow<'a, str>>, mark: Option<&'a str>,
    fields: Vec<DiffField>, selected: bool, message: M,
) -> Element<'a, M> {
    let paint = *paint;
    let radio = super::controls::radio_mark(&paint, selected, Some(message.clone()));
    let mut head = row![
        radio,
        column![text(title, TextStyle::HEAD), text(subtitle, TextStyle::CAP).color(paint.fg_3())]
            .spacing(0)
            .width(Fill),
    ]
    .spacing(SP3 - 2.0)
    .align_y(Alignment::Center);
    if let Some(mark) = mark {
        head = head.push(badge(&paint, mark, Severity::Accent));
    }
    let mut body = Column::new().push(container(head).padding(Padding::from([SP3, SP3 + 2.0])).width(Fill));
    for field in fields {
        let tint = field.differs.then(|| paint.warn_soft());
        let mut lines =
            column![text(field.label, TextStyle::CAP).color(if field.differs { paint.warn() } else { paint.fg_3() })]
                .spacing(2);
        lines = lines.push(text(field.value, TextStyle::CALL));
        if let Some(strength) = field.extra {
            lines = lines.push(strength_meter(&paint, Some(strength)));
        }
        body = body.push(super::separator(&paint)).push(
            container(lines)
                .padding(Padding::from([SP2, SP3 + 2.0]))
                .width(Fill)
                .style(move |_| tint.map_or_else(container::Style::default, |tint| style::rounded(tint, 0.0))),
        );
    }
    let card = container(body).width(Fill).clip(true).style(move |_| container::Style {
        background: Some(paint.card_bg().into()),
        border: Border { color: if selected { paint.acc_bg() } else { paint.line() }, width: 1.0, radius: R3.into() },
        ..container::Style::default()
    });
    let control =
        control(card).on_press(message.clone()).padding(0).width(Fill).style(|_, _| control::Style::default());
    let framed = halo(control, selected.then(|| paint.acc_ring()), R3);
    focusable(&paint, framed, R3 + HALO).on_activate(Some(message)).into()
}

#[derive(Debug, Clone)]
pub struct QrModules {
    side: usize,
    dark: Vec<bool>,
}

impl QrModules {
    #[must_use]
    pub fn encode(data: &str) -> Option<Self> {
        let code = qrcode::QrCode::with_error_correction_level(data.as_bytes(), qrcode::EcLevel::M).ok()?;
        let side = code.width();
        let dark = code.to_colors().into_iter().map(|color| color == qrcode::Color::Dark).collect();
        Some(Self { side, dark })
    }

    #[must_use]
    pub const fn side(&self) -> usize {
        self.side
    }

    #[must_use]
    pub fn dark(&self, x: usize, y: usize) -> bool {
        x < self.side && y < self.side && self.dark[y * self.side + x]
    }
}

pub const QR_QUIET: usize = 4;

struct Qr {
    paint: Paint,
    modules: QrModules,
}

impl<M> canvas::Program<M> for Qr {
    type State = ();

    fn draw(&self, (): &(), renderer: &Renderer, _: &Theme, bounds: Rectangle, _: mouse::Cursor) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let total = self.modules.side + QR_QUIET * 2;
        let unit = (bounds.width.min(bounds.height) / total as f32).floor().max(1.0);
        let offset = ((bounds.width.min(bounds.height) - unit * total as f32) / 2.0).floor();
        frame.fill_rectangle(Point::ORIGIN, bounds.size(), self.paint.card_bg());
        let ink = self.paint.fg_1();
        for y in 0..self.modules.side {
            for x in 0..self.modules.side {
                if self.modules.dark(x, y) {
                    let at = Point::new(offset + (x + QR_QUIET) as f32 * unit, offset + (y + QR_QUIET) as f32 * unit);
                    frame.fill_rectangle(at, Extent::new(unit, unit), ink);
                }
            }
        }
        vec![frame.into_geometry()]
    }
}

#[must_use]
pub fn qr_code<'a, M: 'a>(paint: &Paint, data: &str, side: f32) -> Element<'a, M> {
    match QrModules::encode(data) {
        Some(modules) => surface(Qr { paint: *paint, modules }).width(side).height(side).into(),
        None => Space::new().width(side).height(side).into(),
    }
}

#[must_use]
pub fn stat_choice<'a, M: Clone + 'a>(
    paint: &Paint, value: impl Into<Cow<'a, str>>, ink: Color, label: impl Into<Cow<'a, str>>,
    caption: impl Into<Cow<'a, str>>, selected: bool, message: M,
) -> Element<'a, M> {
    let paint = *paint;
    let figure =
        text(value, TextStyle { size_px: 28.0, line_height_px: 34.0, weight: 600, ..TextStyle::NUM }).color(ink);
    let content =
        column![figure, text(label, TextStyle::HEAD), text(caption, TextStyle::CAP).color(paint.fg_3())].spacing(SP1);
    let control =
        control(container(content).padding(SP3 + 4.0).width(Fill)).on_press(message.clone()).padding(0).width(Fill);
    let control = widgets::button(control, move |status| {
        let mut style = style::flat(paint.layered(paint.card_bg(), status), paint.fg_1(), R3);
        style.border =
            Border { color: if selected { paint.acc_bg() } else { paint.line() }, width: 1.0, radius: R3.into() };
        style
    });
    let raised = widgets::surface(control, &paint, glide_theme::elevation::Level::El1, R3);
    let framed = halo(raised, selected.then(|| paint.acc_ring()), R3);
    focusable(&paint, framed, R3 + HALO).on_activate(Some(message)).into()
}

#[must_use]
pub fn entry_row<'a, M: Clone + 'a>(
    paint: &Paint, lead: impl Into<Element<'a, M>>, title: impl Into<Cow<'a, str>>, subtitle: impl Into<Cow<'a, str>>,
    trailing: Vec<Element<'a, M>>, selected: bool, message: M,
) -> Element<'a, M> {
    let paint = *paint;
    let labels = column![
        text(title, TextStyle::HEAD).wrapping(Wrapping::None),
        text(subtitle, TextStyle::CAP)
            .color(if selected { paint.fg_2() } else { paint.fg_3() })
            .wrapping(Wrapping::None),
    ]
    .width(Fill)
    .clip(true);
    let content = row![lead.into(), labels, Row::with_children(trailing).spacing(SP1 + 2.0).align_y(Alignment::Center)]
        .spacing(SP3)
        .align_y(Alignment::Center);
    let picked = message.clone();
    let row = animate(if selected { 1.0 } else { 0.0 }, Speed::COLOR, move |value| {
        let value = value.clone();
        let control = control(container(content).center_y(Fill))
            .on_press(picked)
            .width(Fill)
            .height(ENTRY_ROW)
            .padding(Padding::from([0.0, SP3]));
        widgets::button(control, move |status| {
            let background =
                mix(paint.layered(Color::TRANSPARENT, status), paint.acc_soft(), value.get().clamp(0.0, 1.0));
            style::flat(background, paint.fg_1(), R2)
        })
    });
    focusable(&paint, row, R2).inset().on_activate(Some(message)).into()
}

pub const ENTRY_ROW: f32 = 56.0;

#[must_use]
pub fn mark_tile<'a, M: 'a>(paint: &Paint, name: &str, side: f32, highlighted: bool) -> Element<'a, M> {
    let letter: String =
        name.chars().find(|c| c.is_alphanumeric()).map_or_else(|| String::from("?"), |c| c.to_uppercase().collect());
    let glyph = (side * 0.42).round();
    let (well, ink) =
        if highlighted { (paint.acc_soft(), paint.acc_soft_fg()) } else { (paint.fill_2(), paint.fg_2()) };
    let radius = if side >= 48.0 {
        R3
    } else if side <= 24.0 {
        R1
    } else {
        R2
    };
    container(
        text(letter, TextStyle { size_px: glyph, line_height_px: glyph * 1.2, weight: 630, ..TextStyle::HEAD })
            .color(ink),
    )
    .center(side)
    .style(move |_| style::rounded(well, radius))
    .into()
}

#[must_use]
pub fn glyph_tile<'a, M: 'a>(paint: &Paint, glyph: impl super::Glyph, side: f32, highlighted: bool) -> Element<'a, M> {
    let (well, ink) =
        if highlighted { (paint.acc_soft(), paint.acc_soft_fg()) } else { (paint.fill_2(), paint.fg_2()) };
    let radius = if side >= 48.0 {
        R3
    } else if side <= 24.0 {
        R1
    } else {
        R2
    };
    container(super::icon(glyph, (side * 0.5).round().clamp(14.0, 24.0), ink))
        .center(side)
        .style(move |_| style::rounded(well, radius))
        .into()
}

#[must_use]
pub fn tinted_tile<'a, M: 'a>(glyph: impl super::Glyph, side: f32, well: Color, ink: Color) -> Element<'a, M> {
    let radius = if side >= 48.0 {
        R3
    } else if side <= 24.0 {
        R1
    } else {
        R2
    };
    container(super::icon(glyph, (side * 0.5).round().clamp(14.0, 24.0), ink))
        .center(side)
        .style(move |_| style::rounded(well, radius))
        .into()
}

#[must_use]
pub fn copy_button<'a, M: Clone + 'a>(paint: &Paint, label: &'a str, message: M) -> Element<'a, M> {
    sized_icon_button(paint, Icon::Copy, label, Some(message), false, Size::Small)
}

#[must_use]
pub fn wide_button<'a, M: Clone + 'a>(
    paint: &Paint, label: impl Into<Cow<'a, str>>, tone: Tone, message: Option<M>,
) -> Element<'a, M> {
    container(button(paint, label, tone, message)).width(Length::Fill).center_x(Fill).into()
}

#[must_use]
pub fn option_card<'a, M: Clone + 'a>(
    paint: &Paint, lead: impl Into<Element<'a, M>>, title: impl Into<Cow<'a, str>>, body: impl Into<Cow<'a, str>>,
    selected: bool, message: Option<M>,
) -> Element<'a, M> {
    let enabled = message.is_some();
    let paint = if enabled { *paint } else { paint.faded(0.55) };
    let content = column![lead.into(), text(title, TextStyle::HEAD), text(body, TextStyle::CAP).color(paint.fg_3())]
        .spacing(SP2)
        .width(Fill);
    let card =
        control(container(content).padding(SP4).width(Fill)).on_press_maybe(message.clone()).padding(0).width(Fill);
    let card = widgets::button(card, move |status| {
        let mut style = style::flat(paint.layered(paint.card_bg(), status), paint.fg_1(), R3);
        style.border =
            Border { color: if selected { paint.acc_bg() } else { paint.line() }, width: 1.0, radius: R3.into() };
        style
    });
    let framed = halo(card, selected.then(|| paint.acc_ring()), R3);
    focusable(&paint, framed, R3 + HALO).on_activate(message).into()
}

pub const FIELD_ROW: f32 = 44.0;

#[must_use]
pub fn field_row<'a, M: 'a>(
    paint: &Paint, label: impl Into<Cow<'a, str>>, value: impl Into<Element<'a, M>>, trailing: Vec<Element<'a, M>>,
) -> Element<'a, M> {
    let lines = column![text(label, TextStyle::CAP).color(paint.fg_3()), value.into()].spacing(2).width(Fill);
    let content = row![lines, Row::with_children(trailing).spacing(SP1).align_y(Alignment::Center)]
        .spacing(SP3)
        .align_y(Alignment::Center);
    container(row![Space::new().height(FIELD_ROW), content].align_y(Alignment::Center))
        .padding(Padding::from([SP1, SP3]))
        .width(Fill)
        .into()
}

pub struct SyncLine<'a> {
    pub label: Cow<'a, str>,
    pub status: Cow<'a, str>,
    pub state: VaultState,
}

#[must_use]
pub fn sync_card<'a, M: 'a>(paint: &Paint, lines: Vec<SyncLine<'a>>, note: Option<Cow<'a, str>>) -> Element<'a, M> {
    let paint = *paint;
    let mut content = Column::new().spacing(SP2);
    for line in lines {
        let (lead, ink) = match line.state {
            VaultState::Synced => (paint.ok(), paint.fg_3()),
            VaultState::Pending => (paint.warn(), paint.warn()),
            VaultState::Local | VaultState::Unlocked => (paint.fg_4(), paint.fg_3()),
            VaultState::Current => (paint.acc_bg(), paint.fg_3()),
        };
        content = content.push(
            row![
                dot(lead, 8.0),
                text(line.label, TextStyle::CAP).width(Fill).wrapping(Wrapping::None),
                text(line.status, TextStyle::CAP).color(ink).wrapping(Wrapping::None),
            ]
            .spacing(SP2)
            .align_y(Alignment::Center),
        );
    }
    if let Some(note) = note {
        content =
            content.push(container(text(note, TextStyle::CAP).color(paint.fg_3())).padding(Padding::ZERO.left(SP4)));
    }
    container(content).padding(SP3).width(Fill).style(move |_| style::rounded(paint.fill_1(), R3)).into()
}

#[must_use]
pub fn caption_row<'a, M: 'a>(
    paint: &Paint, left: impl Into<Cow<'a, str>>, right: impl Into<Cow<'a, str>>,
) -> Element<'a, M> {
    row![
        text(left, TextStyle::MICRO).color(paint.fg_3()).width(Fill),
        text(right, TextStyle::MICRO).color(paint.fg_3()),
    ]
    .padding(Padding::from([SP2, SP3]))
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mask_hides_the_length_of_long_secrets() {
        assert_eq!(masked("abc"), "•••");
        assert_eq!(masked(&"x".repeat(40)).chars().count(), MASK_LIMIT);
        assert_eq!(masked(""), "•", "an empty secret still shows one bullet");
    }

    #[test]
    fn runs_split_letters_digits_and_symbols() {
        let kinds: Vec<(Glyphs, String)> = runs("c7#Vq2!");
        assert_eq!(
            kinds,
            [
                (Glyphs::Letter, "c".into()),
                (Glyphs::Digit, "7".into()),
                (Glyphs::Symbol, "#".into()),
                (Glyphs::Letter, "Vq".into()),
                (Glyphs::Digit, "2".into()),
                (Glyphs::Symbol, "!".into()),
            ]
        );
    }

    #[test]
    fn codes_are_grouped_in_halves() {
        assert_eq!(grouped_code("482913"), "482 913");
        assert_eq!(grouped_code("12345678"), "1234 5678");
        assert_eq!(grouped_code("1234567"), "123 4567");
        assert_eq!(grouped_code("12345"), "12345");
        assert_eq!(grouped_code("482 913"), "482 913");
    }

    #[test]
    fn strength_follows_the_bit_thresholds() {
        assert_eq!(Strength::of_bits(20.0), Strength::Weak);
        assert_eq!(Strength::of_bits(50.0), Strength::Fair);
        assert_eq!(Strength::of_bits(80.0), Strength::Strong);
        assert_eq!(Strength::of_bits(120.0), Strength::VeryStrong);
        assert_eq!(Strength::Fair.severity(), Severity::Warn);
    }

    #[test]
    fn a_code_input_keeps_only_digits_up_to_its_length() {
        assert_eq!(only_digits("4a8 2-9135", 6), "482913");
    }

    #[test]
    fn a_qr_code_has_its_finder_patterns() {
        let modules = QrModules::encode("https://send.bitwarden.com/#/abc/def").unwrap();
        assert!(modules.side() >= 21);
        for (x, y) in [(0, 0), (6, 0), (0, 6), (modules.side() - 1, 0), (0, modules.side() - 1)] {
            assert!(modules.dark(x, y), "finder corner {x},{y}");
        }
        assert!(!modules.dark(7, 7), "the separator around a finder is light");
    }
}

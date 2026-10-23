use std::path::Path;

use glide_iced::Paint;
use glide_iced::style::{self, SP2};
use glide_iced::widgets;
use glide_theme::TextStyle;
use glide_theme::shape::R_ROUND;
use iced::widget::{Row, Space, button as control, container, image, row, svg};
use iced::{Alignment, Color, ContentFit, Element, Fill, Length};

use super::cards::preview_ring_style;
use super::{Glyph, Severity, icon_button, text};

#[derive(Debug, Clone, PartialEq)]
pub enum Art {
    Vector(svg::Handle),
    Raster(image::Handle),
}

impl Art {
    #[must_use]
    pub fn from_path(path: &Path) -> Self {
        if path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("svg")) {
            Self::Vector(svg::Handle::from_path(path))
        } else {
            Self::Raster(image::Handle::from_path(path))
        }
    }
}

#[must_use]
pub fn svg_glyph<'a, M: 'a>(handle: svg::Handle, side: f32, tint: Color) -> Element<'a, M> {
    let opaque = Color { a: 1.0, ..tint };
    svg(handle).width(side).height(side).opacity(tint.a).style(move |_, _| svg::Style { color: Some(opaque) }).into()
}

#[must_use]
pub fn art<'a, M: 'a>(art: Art, side: f32, opacity: f32) -> Element<'a, M> {
    match art {
        Art::Vector(handle) => svg(handle).width(side).height(side).opacity(opacity).into(),
        Art::Raster(handle) => {
            image(handle).width(side).height(side).content_fit(ContentFit::Contain).opacity(opacity).into()
        }
    }
}

#[must_use]
pub fn photo<'a, M: 'a>(path: &Path, side: f32, radius: f32) -> Element<'a, M> {
    match Art::from_path(path) {
        Art::Vector(handle) => svg(handle).width(side).height(side).into(),
        Art::Raster(handle) => {
            image(handle).width(side).height(side).content_fit(ContentFit::Cover).border_radius(radius).into()
        }
    }
}

#[must_use]
pub fn photo_button<'a, M: Clone + 'a>(
    paint: &Paint, path: &Path, side: f32, selected: bool, message: Option<M>,
) -> Element<'a, M> {
    let paint = *paint;
    let face = control(photo(path, side, side / 2.0)).on_press_maybe(message).padding(super::RING + 2.0);
    widgets::button(face, move |status| preview_ring_style(&paint, selected, status))
}

#[must_use]
pub fn picture<'a, M: 'a>(
    image_handle: image::Handle, width: impl Into<Length>, height: impl Into<Length>, fit: ContentFit, radius: f32,
) -> Element<'a, M> {
    image(image_handle).width(width).height(height).content_fit(fit).border_radius(radius).into()
}

#[must_use]
pub fn level_meter<'a, M: 'a>(paint: &Paint, level: f32, peak: f32, severity: Severity) -> Element<'a, M> {
    let tone = match severity {
        Severity::Danger => paint.danger(),
        Severity::Warn => paint.warn(),
        Severity::Ok => paint.ok(),
        Severity::Accent => paint.acc_bg(),
        Severity::Neutral => paint.fg_3(),
    };
    let segment = |portion: u16, color: Option<Color>| -> Element<'a, M> {
        let piece = container(Space::new()).width(Length::FillPortion(portion)).height(METER);
        match color {
            Some(color) => piece.style(move |_| style::rounded(color, R_ROUND)).into(),
            None => piece.into(),
        }
    };
    let filled = (level.clamp(0.0, 1.0) * 1000.0).round() as u16;
    let peak_at = (peak.clamp(0.0, 1.0) * 1000.0).round() as u16;
    let rest = 1000 - filled;
    let mut bar = Row::new().align_y(Alignment::Center);
    if filled > 0 {
        bar = bar.push(segment(filled, Some(tone)));
    }
    if peak_at > filled + PEAK {
        let gap = peak_at - filled - PEAK;
        let after = rest - gap - PEAK;
        bar = bar.push((gap > 0).then(|| segment(gap, None))).push(segment(PEAK, Some(tone)));
        bar = bar.push((after > 0).then(|| segment(after, None)));
    } else if rest > 0 {
        bar = bar.push(segment(rest, None));
    }
    let track = paint.fill_2();
    container(bar).width(Fill).height(METER).style(move |_| style::rounded(track, R_ROUND)).into()
}

const METER: f32 = 6.0;
const PEAK: u16 = 8;

#[must_use]
pub fn level_control<'a, M: Clone + 'a>(
    paint: &Paint, glyph: impl Glyph, mute: M, slider: impl Into<Element<'a, M>>, level: f32, muted: bool,
) -> Element<'a, M> {
    let leading = icon_button(paint, glyph, "", Some(mute), false);
    slider_readout(paint, Some(leading), slider, format!("{:.0}%", level * 100.0), muted)
}

#[must_use]
pub fn slider_readout<'a, M: 'a>(
    paint: &Paint, leading: Option<Element<'a, M>>, slider: impl Into<Element<'a, M>>, readout: String, dimmed: bool,
) -> Element<'a, M> {
    let readout = text(readout, TextStyle::NUM)
        .color(if dimmed { paint.fg_3() } else { paint.fg_2() })
        .width(48.0)
        .align_x(Alignment::End);
    row![].push(leading).push(slider.into()).push(readout).spacing(SP2 + 2.0).align_y(Alignment::Center).into()
}

#[must_use]
pub fn range_slider<'a, M: 'a>(
    paint: &Paint, low: impl Into<std::borrow::Cow<'a, str>>, slider: impl Into<Element<'a, M>>,
    high: impl Into<std::borrow::Cow<'a, str>>,
) -> Element<'a, M> {
    row![text(low, TextStyle::CAP).color(paint.fg_3()), slider.into(), text(high, TextStyle::CAP).color(paint.fg_3()),]
        .spacing(SP2 + 2.0)
        .align_y(Alignment::Center)
        .into()
}

#[must_use]
pub fn frame_button<'a, M: Clone + 'a>(
    paint: &Paint, content: impl Into<Element<'a, M>>, selected: bool, message: M,
) -> Element<'a, M> {
    let paint = *paint;
    let framed = control(content.into()).on_press(message.clone()).padding(super::RING + 2.0).width(Fill);
    let framed = widgets::button(framed, move |status| preview_ring_style(&paint, selected, status));
    super::focus::activatable(&paint, framed, glide_iced::style::R3, Some(message))
}

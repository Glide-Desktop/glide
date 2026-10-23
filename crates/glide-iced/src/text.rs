use std::borrow::Cow;
use std::sync::Arc;

use glide_theme::TextStyle;
use iced::advanced::graphics::text::{self, Renderer as _, cosmic_text};
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{Tree, Widget, tree};
pub use iced::widget::text::Wrapping;
use iced::{Color, Element, Font, Length, Rectangle, Renderer, Size, Theme, alignment, mouse};

pub struct Text<'a> {
    content: Cow<'a, str>,
    style: TextStyle,
    font: Font,
    color: Option<Color>,
    width: Length,
    wrapping: Wrapping,
    align: alignment::Horizontal,
}

pub fn text<'a>(content: impl Into<Cow<'a, str>>, style: TextStyle) -> Text<'a> {
    let content = content.into();
    let content = if style.uppercase { Cow::Owned(content.to_uppercase()) } else { content };
    Text {
        content,
        style,
        font: Font {
            family: if style.tabular {
                iced::font::Family::Name(glide_paint::TABULAR_FAMILY)
            } else {
                iced::font::Family::SansSerif
            },
            ..Font::DEFAULT
        },
        color: None,
        width: Length::Shrink,
        wrapping: Wrapping::Word,
        align: alignment::Horizontal::Left,
    }
}

impl Text<'_> {
    #[must_use]
    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }
    #[must_use]
    pub fn font(mut self, font: Font) -> Self {
        self.font = font;
        self
    }
    #[must_use]
    pub fn wrapping(mut self, wrapping: Wrapping) -> Self {
        self.wrapping = wrapping;
        self
    }
    #[must_use]
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }
    #[must_use]
    pub fn align_x(mut self, align: impl Into<alignment::Horizontal>) -> Self {
        self.align = align.into();
        self
    }
    fn attributes(&self) -> cosmic_text::Attrs<'static> {
        text::to_attributes(self.font)
            .weight(cosmic_text::Weight(self.style.weight))
            .letter_spacing(self.style.tracking_em)
    }
}

#[derive(PartialEq)]
struct Key {
    content: String,
    style: TextStyle,
    font: Font,
    width: f32,
    height: f32,
    wrap: Wrapping,
    revision: u64,
}

#[derive(Default)]
struct State {
    key: Option<Key>,
    buffer: Option<Arc<cosmic_text::Buffer>>,
    measured: Size,
}

impl<Message> Widget<Message, Theme, Renderer> for Text<'_> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }
    fn size(&self) -> Size<Length> {
        Size::new(self.width, Length::Shrink)
    }
    fn layout(&mut self, tree: &mut Tree, _: &Renderer, limits: &layout::Limits) -> layout::Node {
        let state = tree.state.downcast_mut::<State>();
        let key = Key {
            content: self.content.to_string(),
            style: self.style,
            font: self.font,
            width: limits.max().width,
            height: limits.max().height,
            wrap: self.wrapping,
            revision: crate::font::revision(),
        };
        if state.key.as_ref() != Some(&key) {
            let mut system = text::font_system().write().unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut buffer = cosmic_text::Buffer::new(
                system.raw(),
                cosmic_text::Metrics::new(self.style.size_px, self.style.line_height_px),
            );
            let width = if self.wrapping == Wrapping::None { None } else { Some(limits.max().width) };
            buffer.set_size(system.raw(), width, None);
            buffer.set_wrap(system.raw(), text::to_wrap(self.wrapping));
            let attributes = self.attributes();
            buffer.set_text(system.raw(), &self.content, &attributes, cosmic_text::Shaping::Advanced, None);
            buffer.shape_until_scroll(system.raw(), false);
            state.measured = text::measure(&buffer).0;
            let align = match self.align {
                alignment::Horizontal::Left => None,
                alignment::Horizontal::Center => Some(cosmic_text::Align::Center),
                alignment::Horizontal::Right => Some(cosmic_text::Align::Right),
            };
            if align.is_some() && buffer.layout_runs().nth(1).is_some() {
                buffer.set_size(system.raw(), Some(state.measured.width.ceil()), None);
                for line in &mut buffer.lines {
                    line.set_align(align);
                }
                buffer.shape_until_scroll(system.raw(), false);
            }
            state.buffer = Some(Arc::new(buffer));
            state.key = Some(key);
        }
        layout::Node::new(limits.resolve(self.width, Length::Shrink, state.measured))
    }
    fn draw(
        &self, tree: &Tree, renderer: &mut Renderer, _: &Theme, style: &renderer::Style, layout: Layout<'_>,
        _: mouse::Cursor, viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        let Some(buffer) = &state.buffer else { return };
        let bounds = layout.bounds();
        let offset = (bounds.width - state.measured.width).max(0.0)
            * match self.align {
                alignment::Horizontal::Left => 0.0,
                alignment::Horizontal::Center => 0.5,
                alignment::Horizontal::Right => 1.0,
            };
        if let Some(clip_bounds) = bounds.intersection(viewport) {
            renderer.fill_raw(text::Raw {
                buffer: Arc::downgrade(buffer),
                position: iced::Point::new(bounds.x + offset, bounds.y),
                color: self.color.unwrap_or(style.text_color),
                clip_bounds,
            });
        }
    }
}

impl<'a, Message: 'a> From<Text<'a>> for Element<'a, Message> {
    fn from(label: Text<'a>) -> Self {
        Self::new(label)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn labels_apply_every_typographic_input() {
        let style = TextStyle::WIN_TITLE;
        let label = text("Window", style);
        assert_eq!(label.attributes().weight.0, style.weight);
        assert_eq!(label.style.line_height_px, 20.0);
        assert_eq!(label.style.tracking_em, -0.012);
        assert_eq!(text("section", TextStyle::MICRO).content, "SECTION");
        assert_eq!(text("section", TextStyle::BODY).content, "section");
        assert_eq!(text("42", TextStyle::NUM).font.family, iced::font::Family::Name(glide_paint::TABULAR_FAMILY));
    }

    #[test]
    fn layout_uses_the_styles_tracking_and_line_height() {
        fn measured(style: TextStyle) -> Size {
            let mut label = text("Meridian window title", style);
            let mut tree = Tree::new(&label as &dyn Widget<(), Theme, Renderer>);
            let renderer = Renderer::Secondary(iced_tiny_skia::Renderer::new(Font::DEFAULT, iced::Pixels(14.0)));
            <Text<'_> as Widget<(), Theme, Renderer>>::layout(
                &mut label,
                &mut tree,
                &renderer,
                &layout::Limits::new(Size::ZERO, Size::new(1000.0, 200.0)),
            );
            tree.state.downcast_ref::<State>().measured
        }
        let tight = measured(TextStyle::WIN_TITLE);
        let normal = measured(TextStyle { tracking_em: 0.0, ..TextStyle::WIN_TITLE });
        assert!(tight.width < normal.width, "tracking must affect shaped text width");
        assert_eq!(tight.height, TextStyle::WIN_TITLE.line_height_px);
    }
}

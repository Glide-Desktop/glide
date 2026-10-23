use glide_iced::Paint;
use iced::advanced::text::Highlight;
use iced::advanced::text::highlighter::PlainText;
use iced::widget::markdown;
use iced::widget::text::{LineHeight, Wrapping};
use iced::widget::text_editor::{Content, TextEditor};
use iced::{Background, Border, Element, Fill, Font, Padding};

use super::bare_text_area_style;

#[must_use]
pub fn editor<'a, M: Clone + 'a>(paint: &Paint, content: &'a Content) -> TextEditor<'a, PlainText, M> {
    let paint = *paint;
    iced::widget::text_editor(content)
        .line_height(LineHeight::Relative(1.5))
        .wrapping(Wrapping::WordOrGlyph)
        .height(Fill)
        .style(move |_, status| bare_text_area_style(&paint, status))
}

#[must_use]
pub fn markdown_settings(paint: &Paint, size: f32, mono: Font) -> markdown::Settings {
    let mut settings = markdown::Settings::with_text_size(
        size,
        markdown::Style {
            font: Font::DEFAULT,
            inline_code_highlight: Highlight {
                background: Background::Color(paint.fill_2()),
                border: Border { radius: 4.0.into(), ..Border::default() },
            },
            inline_code_padding: Padding::from([0.0, 3.0]),
            inline_code_color: paint.fg_1(),
            inline_code_font: mono,
            code_block_font: mono,
            link_color: paint.acc_bg(),
        },
    );
    settings.code_size = (size * 0.9).into();
    settings
}

#[must_use]
pub fn markdown_view<'a, M: 'a>(
    paint: &Paint, items: &'a [markdown::Item], settings: markdown::Settings, viewer: &impl markdown::Viewer<'a, M>,
) -> Element<'a, M> {
    use glide_iced::style::{SP4, SP5};
    super::scrollable(
        paint,
        iced::widget::container(markdown::view_with(items, settings, viewer))
            .padding(Padding::from([SP4, SP5]))
            .width(Fill),
    )
    .height(Fill)
    .into()
}

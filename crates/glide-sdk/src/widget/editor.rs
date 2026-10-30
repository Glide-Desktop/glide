use std::cell::Cell;

use glide_iced::Paint;
use glide_iced::style::SP1;
use glide_theme::shape::R_ROUND;
use iced::advanced::graphics::text::cosmic_text;
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad};
use iced::advanced::text::Highlight;
use iced::advanced::text::highlighter::PlainText;
use iced::advanced::widget::{Operation, Tree, tree};
use iced::advanced::{Clipboard, Shell, Widget, image, mouse, text as core_text};
use iced::widget::markdown;
use iced::widget::text::{LineHeight, Wrapping};
use iced::widget::text_editor::{Content, TextEditor};
use iced::{
    Background, Border, Color, Element, Event, Fill, Font, Length, Padding, Pixels, Point, Rectangle, Size, Theme,
    Transformation,
};

use super::bare_text_area_style;

type Inner = iced::Renderer;

#[repr(transparent)]
pub struct EditorRenderer(Inner);

pub type EditorContent = Content<EditorRenderer>;

pub const EDITOR_BAR: f32 = 6.0;
const MIN_THUMB: f32 = 24.0;

thread_local! {
    static DRAWN: Cell<Option<View>> = const { Cell::new(None) };
}

fn gauge(renderer: &Inner) -> &EditorRenderer {
    // SAFETY: `EditorRenderer` is `repr(transparent)` over `iced::Renderer`, so the two share one layout and the
    // cast reference keeps the borrow and lifetime of the original.
    unsafe { &*std::ptr::from_ref(renderer).cast::<EditorRenderer>() }
}

fn gauge_mut(renderer: &mut Inner) -> &mut EditorRenderer {
    // SAFETY: `EditorRenderer` is `repr(transparent)` over `iced::Renderer`, so the two share one layout and the
    // cast reference keeps the exclusive borrow and lifetime of the original.
    unsafe { &mut *std::ptr::from_mut(renderer).cast::<EditorRenderer>() }
}

impl renderer::Renderer for EditorRenderer {
    fn start_layer(&mut self, bounds: Rectangle) {
        renderer::Renderer::start_layer(&mut self.0, bounds);
    }

    fn end_layer(&mut self) {
        renderer::Renderer::end_layer(&mut self.0);
    }

    fn start_transformation(&mut self, transformation: Transformation) {
        renderer::Renderer::start_transformation(&mut self.0, transformation);
    }

    fn end_transformation(&mut self) {
        renderer::Renderer::end_transformation(&mut self.0);
    }

    fn fill_quad(&mut self, quad: Quad, background: impl Into<Background>) {
        renderer::Renderer::fill_quad(&mut self.0, quad, background);
    }

    fn reset(&mut self, new_bounds: Rectangle) {
        renderer::Renderer::reset(&mut self.0, new_bounds);
    }

    fn allocate_image(
        &mut self, handle: &image::Handle,
        callback: impl FnOnce(Result<image::Allocation, image::Error>) + Send + 'static,
    ) {
        renderer::Renderer::allocate_image(&mut self.0, handle, callback);
    }
}

impl core_text::Renderer for EditorRenderer {
    type Font = <Inner as core_text::Renderer>::Font;
    type Paragraph = <Inner as core_text::Renderer>::Paragraph;
    type Editor = <Inner as core_text::Renderer>::Editor;

    const ICON_FONT: Self::Font = <Inner as core_text::Renderer>::ICON_FONT;
    const CHECKMARK_ICON: char = <Inner as core_text::Renderer>::CHECKMARK_ICON;
    const ARROW_DOWN_ICON: char = <Inner as core_text::Renderer>::ARROW_DOWN_ICON;
    const SCROLL_UP_ICON: char = <Inner as core_text::Renderer>::SCROLL_UP_ICON;
    const SCROLL_DOWN_ICON: char = <Inner as core_text::Renderer>::SCROLL_DOWN_ICON;
    const SCROLL_LEFT_ICON: char = <Inner as core_text::Renderer>::SCROLL_LEFT_ICON;
    const SCROLL_RIGHT_ICON: char = <Inner as core_text::Renderer>::SCROLL_RIGHT_ICON;
    const ICED_LOGO: char = <Inner as core_text::Renderer>::ICED_LOGO;

    fn default_font(&self) -> Self::Font {
        core_text::Renderer::default_font(&self.0)
    }

    fn default_size(&self) -> Pixels {
        core_text::Renderer::default_size(&self.0)
    }

    fn fill_paragraph(&mut self, text: &Self::Paragraph, position: Point, color: Color, clip_bounds: Rectangle) {
        core_text::Renderer::fill_paragraph(&mut self.0, text, position, color, clip_bounds);
    }

    fn fill_editor(&mut self, editor: &Self::Editor, position: Point, color: Color, clip_bounds: Rectangle) {
        DRAWN.set(Some(View::of(editor.buffer())));
        core_text::Renderer::fill_editor(&mut self.0, editor, position, color, clip_bounds);
    }

    fn fill_text(
        &mut self, text: core_text::Text<String, Self::Font>, position: Point, color: Color, clip_bounds: Rectangle,
    ) {
        core_text::Renderer::fill_text(&mut self.0, text, position, color, clip_bounds);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    pub top: f32,
    pub visible: f32,
    pub total: f32,
}

impl View {
    #[must_use]
    pub fn of(buffer: &cosmic_text::Buffer) -> Self {
        let line_height = buffer.metrics().line_height.max(1.0);
        let (mut shaped, mut rows) = (0usize, 0usize);
        for line in &buffer.lines {
            if let Some(layout) = line.layout_opt() {
                shaped += 1;
                rows += layout.len().max(1);
            }
        }
        let guess = if shaped == 0 { 1.0 } else { rows as f32 / shaped as f32 };
        let height =
            |line: &cosmic_text::BufferLine| line.layout_opt().map_or(guess, |layout| layout.len().max(1) as f32);
        let scroll = buffer.scroll();
        let above: f32 = buffer.lines[..scroll.line.min(buffer.lines.len())].iter().map(height).sum();
        Self {
            top: (above + scroll.vertical / line_height).max(0.0),
            visible: buffer.size().1.unwrap_or(0.0).max(0.0) / line_height,
            total: rows as f32 + (buffer.lines.len() - shaped) as f32 * guess,
        }
    }

    #[must_use]
    pub fn overflows(self) -> bool {
        self.total > self.visible + 0.5
    }

    fn span(self) -> f32 {
        self.total.max(self.top + self.visible)
    }

    #[must_use]
    pub fn thumb(self, track: Rectangle) -> Rectangle {
        let span = self.span().max(1.0);
        let height = (track.height * (self.visible / span).min(1.0)).max(MIN_THUMB).min(track.height);
        let room = span - self.visible;
        let share = if room > 0.0 { (self.top / room).clamp(0.0, 1.0) } else { 0.0 };
        Rectangle { y: track.y + (track.height - height) * share, height, ..track }
    }

    #[must_use]
    pub fn top_at(self, track: Rectangle, thumb_y: f32) -> f32 {
        let thumb = self.thumb(track);
        let travel = track.height - thumb.height;
        let room = (self.span() - self.visible).max(0.0);
        if travel <= 0.0 {
            return self.top;
        }
        ((thumb_y - track.y) / travel).clamp(0.0, 1.0) * room
    }
}

#[must_use]
pub fn editor_track(bounds: Rectangle) -> Rectangle {
    Rectangle {
        x: bounds.x + bounds.width - SP1 - EDITOR_BAR,
        y: bounds.y + SP1,
        width: EDITOR_BAR,
        height: (bounds.height - 2.0 * SP1).max(0.0),
    }
}

fn strip(bounds: Rectangle) -> Rectangle {
    let width = EDITOR_BAR + 2.0 * SP1;
    Rectangle { x: bounds.x + bounds.width - width, width, ..bounds }
}

#[derive(Debug, Default)]
struct Bar {
    view: Cell<Option<View>>,
    grab: Option<f32>,
    hovered: bool,
}

pub struct ScrolledEditor<'a, H: core_text::Highlighter, M> {
    editor: TextEditor<'a, H, M, Theme, EditorRenderer>,
    idle: Color,
    active: Color,
    on_scroll: Box<dyn Fn(i32) -> M + 'a>,
}

fn inner(layout: Layout<'_>) -> Layout<'_> {
    layout.children().next().unwrap_or(layout)
}

impl<H: core_text::Highlighter, M> ScrolledEditor<'_, H, M> {
    fn drag(&self, bar: &Bar, view: View, track: Rectangle, thumb_y: f32, shell: &mut Shell<'_, M>) {
        let lines = (view.top_at(track, thumb_y) - view.top).round();
        if lines != 0.0 {
            shell.publish((self.on_scroll)(lines as i32));
            bar.view.set(Some(View { top: view.top + lines, ..view }));
        }
        shell.request_redraw();
    }
}

impl<H: core_text::Highlighter, M> Widget<M, Theme, Inner> for ScrolledEditor<'_, H, M> {
    fn size(&self) -> Size<Length> {
        Widget::<M, Theme, EditorRenderer>::size(&self.editor)
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Inner, limits: &layout::Limits) -> layout::Node {
        let node = Widget::<M, Theme, EditorRenderer>::layout(
            &mut self.editor,
            &mut tree.children[0],
            gauge(renderer),
            limits,
        );
        layout::Node::with_children(node.size(), vec![node])
    }

    fn draw(
        &self, tree: &Tree, renderer: &mut Inner, theme: &Theme, style: &renderer::Style, layout: Layout<'_>,
        cursor: mouse::Cursor, viewport: &Rectangle,
    ) {
        DRAWN.set(None);
        Widget::<M, Theme, EditorRenderer>::draw(
            &self.editor,
            &tree.children[0],
            gauge_mut(renderer),
            theme,
            style,
            inner(layout),
            cursor,
            viewport,
        );
        let Some(view) = DRAWN.take() else { return };
        let bar = tree.state.downcast_ref::<Bar>();
        bar.view.set(Some(view));
        if !view.overflows() {
            return;
        }
        let bounds = layout.bounds();
        let active = bar.grab.is_some() || cursor.is_over(strip(bounds));
        let color = if active { self.active } else { self.idle };
        let quad = Quad {
            bounds: view.thumb(editor_track(bounds)),
            border: Border { radius: R_ROUND.into(), ..Border::default() },
            ..Quad::default()
        };
        renderer::Renderer::fill_quad(renderer, quad, color);
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Bar>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(Bar::default())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.editor as &dyn Widget<M, Theme, EditorRenderer>)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&[&self.editor as &dyn Widget<M, Theme, EditorRenderer>]);
    }

    fn operate(&mut self, tree: &mut Tree, layout: Layout<'_>, renderer: &Inner, operation: &mut dyn Operation) {
        Widget::<M, Theme, EditorRenderer>::operate(
            &mut self.editor,
            &mut tree.children[0],
            inner(layout),
            gauge(renderer),
            operation,
        );
    }

    fn update(
        &mut self, tree: &mut Tree, event: &Event, layout: Layout<'_>, cursor: mouse::Cursor, renderer: &Inner,
        clipboard: &mut dyn Clipboard, shell: &mut Shell<'_, M>, viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let track = editor_track(bounds);
        {
            let bar = tree.state.downcast_mut::<Bar>();
            let view = bar.view.get().filter(|view| view.overflows());
            match (event, view, bar.grab) {
                (Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)), _, Some(_)) => {
                    bar.grab = None;
                    shell.capture_event();
                    shell.request_redraw();
                    return;
                }
                (Event::Mouse(mouse::Event::CursorMoved { position }), Some(view), Some(grab)) => {
                    self.drag(bar, view, track, position.y - grab, shell);
                    shell.capture_event();
                    return;
                }
                (Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)), Some(view), None) => {
                    if let Some(at) = cursor.position_over(strip(bounds)) {
                        let thumb = view.thumb(track);
                        let grab = if at.y >= thumb.y && at.y <= thumb.y + thumb.height {
                            at.y - thumb.y
                        } else {
                            thumb.height / 2.0
                        };
                        bar.grab = Some(grab);
                        self.drag(bar, view, track, at.y - grab, shell);
                        shell.capture_event();
                        return;
                    }
                }
                _ => {}
            }
            let hovered = view.is_some() && cursor.is_over(strip(bounds));
            if hovered != bar.hovered {
                bar.hovered = hovered;
                shell.request_redraw();
            }
        }
        Widget::<M, Theme, EditorRenderer>::update(
            &mut self.editor,
            &mut tree.children[0],
            event,
            inner(layout),
            cursor,
            gauge(renderer),
            clipboard,
            shell,
            viewport,
        );
    }

    fn mouse_interaction(
        &self, tree: &Tree, layout: Layout<'_>, cursor: mouse::Cursor, viewport: &Rectangle, renderer: &Inner,
    ) -> mouse::Interaction {
        let bar = tree.state.downcast_ref::<Bar>();
        if bar.grab.is_some() {
            return mouse::Interaction::Grabbing;
        }
        if bar.hovered {
            return mouse::Interaction::Idle;
        }
        Widget::<M, Theme, EditorRenderer>::mouse_interaction(
            &self.editor,
            &tree.children[0],
            inner(layout),
            cursor,
            viewport,
            gauge(renderer),
        )
    }
}

impl<'a, H: core_text::Highlighter + 'a, M: 'a> From<ScrolledEditor<'a, H, M>> for Element<'a, M> {
    fn from(editor: ScrolledEditor<'a, H, M>) -> Self {
        Element::new(editor)
    }
}

#[must_use]
pub fn editor<'a, M: Clone + 'a>(
    paint: &Paint, content: &'a EditorContent,
) -> TextEditor<'a, PlainText, M, Theme, EditorRenderer> {
    let paint = *paint;
    iced::widget::text_editor(content)
        .line_height(LineHeight::Relative(1.5))
        .wrapping(Wrapping::WordOrGlyph)
        .height(Fill)
        .style(move |_, status| bare_text_area_style(&paint, status))
}

#[must_use]
pub fn scrolled_editor<'a, H: core_text::Highlighter, M>(
    paint: &Paint, editor: TextEditor<'a, H, M, Theme, EditorRenderer>, on_scroll: impl Fn(i32) -> M + 'a,
) -> ScrolledEditor<'a, H, M> {
    ScrolledEditor { editor, idle: paint.fill_3(), active: paint.fg_3(), on_scroll: Box::new(on_scroll) }
}

#[must_use]
pub fn text_area<'a, M: Clone + 'a>(paint: &Paint, content: &'a Content) -> TextEditor<'a, PlainText, M> {
    let paint = *paint;
    iced::widget::text_editor(content).style(move |_, status| super::text_area_style(&paint, status))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn track() -> Rectangle {
        Rectangle { x: 0.0, y: 10.0, width: EDITOR_BAR, height: 200.0 }
    }

    fn buffer(text: &str, width: f32, rows: f32, scroll_line: usize) -> cosmic_text::Buffer {
        let mut system = iced::advanced::graphics::text::font_system().write().unwrap();
        let mut buffer = cosmic_text::Buffer::new(system.raw(), cosmic_text::Metrics::new(10.0, 10.0));
        buffer.set_size(system.raw(), Some(width), Some(rows * 10.0));
        buffer.set_wrap(system.raw(), cosmic_text::Wrap::WordOrGlyph);
        buffer.set_text(system.raw(), text, &cosmic_text::Attrs::new(), cosmic_text::Shaping::Advanced, None);
        buffer.set_scroll(cosmic_text::Scroll::new(scroll_line, 0.0, 0.0));
        buffer.shape_until_scroll(system.raw(), false);
        buffer
    }

    #[test]
    fn a_short_document_has_no_thumb_to_draw() {
        assert!(!View { top: 0.0, visible: 20.0, total: 12.0 }.overflows());
        assert!(View { top: 0.0, visible: 20.0, total: 100.0 }.overflows());
    }

    #[test]
    fn the_thumb_shows_the_visible_share_and_sits_where_the_view_is() {
        let top = View { top: 0.0, visible: 20.0, total: 100.0 }.thumb(track());
        assert_eq!((top.y, top.height), (10.0, 40.0));
        let end = View { top: 80.0, visible: 20.0, total: 100.0 }.thumb(track());
        assert_eq!(end.y + end.height, 210.0);
        let tiny = View { top: 0.0, visible: 20.0, total: 100_000.0 }.thumb(track());
        assert_eq!(tiny.height, MIN_THUMB, "a huge document still leaves a thumb that can be grabbed");
    }

    #[test]
    fn dragging_the_thumb_maps_back_to_the_same_first_line() {
        let view = View { top: 30.0, visible: 20.0, total: 100.0 };
        let thumb = view.thumb(track());
        assert!((view.top_at(track(), thumb.y) - 30.0).abs() < 0.01);
        assert_eq!(view.top_at(track(), -500.0), 0.0);
        assert_eq!(view.top_at(track(), 5000.0), 80.0);
    }

    #[test]
    fn the_view_is_read_from_the_editor_buffer_scroll() {
        let text = (0..100).map(|line| format!("line {line}")).collect::<Vec<_>>().join("\n");
        let view = View::of(&buffer(&text, 400.0, 20.0, 10));
        assert!((view.top - 10.0).abs() < 0.01, "{view:?}");
        assert!((view.visible - 20.0).abs() < 0.01, "{view:?}");
        assert!((view.total - 100.0).abs() < 0.01, "{view:?}");
    }

    #[test]
    fn wrapped_lines_count_as_the_rows_they_take() {
        let long = "word ".repeat(60);
        let text = (0..10).map(|_| long.as_str()).collect::<Vec<_>>().join("\n");
        let view = View::of(&buffer(&text, 100.0, 1000.0, 0));
        assert!(view.total > 20.0, "ten wrapped paragraphs are taller than ten rows: {view:?}");
    }

    #[test]
    fn the_scrollbar_reads_the_drawn_editor_and_drags_it_by_lines() {
        let paint = Paint::new(glide_theme::Appearance::Dark, glide_theme::Accent::Blue);
        let text = (0..300).map(|line| format!("line {line}")).collect::<Vec<_>>().join("\n");
        let content = EditorContent::with_text(&text);
        let mut element: Element<'_, i32> = scrolled_editor(&paint, editor(&paint, &content), |lines| lines).into();
        let mut tree = Tree::new(&element);
        let mut renderer = Inner::Secondary(iced_tiny_skia::Renderer::new(Font::DEFAULT, Pixels(14.0)));
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(400.0, 200.0));
        let node =
            element.as_widget_mut().layout(&mut tree, &renderer, &layout::Limits::new(Size::ZERO, bounds.size()));
        let layout = Layout::new(&node);
        let style = renderer::Style { text_color: Color::WHITE };
        let away = mouse::Cursor::Unavailable;
        element.as_widget().draw(&tree, &mut renderer, &Theme::Dark, &style, layout, away, &bounds);
        let view = tree.state.downcast_ref::<Bar>().view.get().expect("drawing the editor reports its view");
        assert!(view.overflows() && view.top.abs() < 0.01, "{view:?}");
        let thumb = view.thumb(editor_track(bounds));
        let grab = Point::new(thumb.center_x(), thumb.y + 2.0);
        let mut messages = Vec::new();
        let mut shell = Shell::new(&mut messages);
        let mut clipboard = iced::advanced::clipboard::Null;
        let events = [
            (Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)), grab),
            (Event::Mouse(mouse::Event::CursorMoved { position: grab + iced::Vector::new(0.0, 60.0) }), grab),
            (Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)), grab),
        ];
        for (event, at) in &events {
            let cursor = mouse::Cursor::Available(*at);
            element.as_widget_mut().update(
                &mut tree,
                event,
                layout,
                cursor,
                &renderer,
                &mut clipboard,
                &mut shell,
                &bounds,
            );
        }
        assert!(shell.is_event_captured(), "the editor under the bar never sees the drag");
        drop(shell);
        assert!(messages.iter().sum::<i32>() > 0, "dragging the thumb down scrolls down: {messages:?}");
        assert!(tree.state.downcast_ref::<Bar>().grab.is_none());
    }
}

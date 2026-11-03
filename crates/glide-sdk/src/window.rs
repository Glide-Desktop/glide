use glide_iced::Paint;
use glide_iced::style::{self, R3, SP2, SP4};
use glide_iced::widgets;
use glide_theme::{TextStyle, component};
use iced::border::Radius;
use iced::widget::{Space, button, column, container, mouse_area, row};
use iced::{Alignment, Element, Fill, Length, Padding, Size, Task, mouse, window};

use crate::widget::{Wrapping, text, tooltip};

pub use glide_app::menubar::Handles;
pub use iced::window::Direction;

#[must_use]
pub fn wayland_handles(window: &dyn iced::window::Window) -> Option<Handles> {
    use raw_window_handle::{RawDisplayHandle, RawWindowHandle};
    let display = window.display_handle().ok()?.as_raw();
    let surface = window.window_handle().ok()?.as_raw();
    match (display, surface) {
        (RawDisplayHandle::Wayland(display), RawWindowHandle::Wayland(surface)) => {
            // SAFETY: both pointers come from this window's own raw handles, and the window lives for the rest of
            // the process.
            Some(unsafe { Handles::new(display.display.as_ptr() as usize, surface.surface.as_ptr() as usize) })
        }
        _ => None,
    }
}

pub fn handles() -> Task<Option<Handles>> {
    window::latest().and_then(|id| window::run(id, wayland_handles))
}

pub const HEADER: f32 = component::WINDOW_HEADER;
pub const CONTROL: f32 = component::WINDOW_CONTROL as f32;
const EDGE: f32 = 5.0;
const CORNER: f32 = 10.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Frame {
    #[default]
    Floating,
    Maximized,
    Fullscreen,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Window {
    pub size: Size,
    pub focused: bool,
    pub frame: Frame,
}

impl Default for Window {
    fn default() -> Self {
        Self { size: Size::new(960.0, 640.0), focused: true, frame: Frame::Floating }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
    Top,
    TopRight,
    BottomLeft,
    BottomRight,
    Bottom,
}

#[must_use]
pub fn side_corners(side: Side, corner: f32) -> Radius {
    let none = Radius::default();
    match side {
        Side::Left => none.top_left(corner).bottom_left(corner),
        Side::Right => none.top_right(corner).bottom_right(corner),
        Side::Top => none.top_left(corner).top_right(corner),
        Side::TopRight => none.top_right(corner),
        Side::BottomLeft => none.bottom_left(corner),
        Side::BottomRight => none.bottom_right(corner),
        Side::Bottom => none.bottom_left(corner).bottom_right(corner),
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Edge(pub Direction);

impl PartialEq for Edge {
    fn eq(&self, other: &Self) -> bool {
        std::mem::discriminant(&self.0) == std::mem::discriminant(&other.0)
    }
}

impl Eq for Edge {}

#[derive(Debug, Clone, Copy)]
pub enum Action {
    Drag,
    ToggleMaximize,
    ToggleFullscreen,
    Minimize,
    Activate,
    Attention,
    Resize(Direction),
    Resized(Size),
    Focused(bool),
    Framed(Frame),
}

impl PartialEq for Action {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Resize(a), Self::Resize(b)) => Edge(*a) == Edge(*b),
            (Self::Resized(a), Self::Resized(b)) => a == b,
            (Self::Focused(a), Self::Focused(b)) => a == b,
            (Self::Framed(a), Self::Framed(b)) => a == b,
            _ => std::mem::discriminant(self) == std::mem::discriminant(other),
        }
    }
}

impl Window {
    #[must_use]
    pub fn edge_to_edge(&self) -> bool {
        self.frame != Frame::Floating
    }

    #[must_use]
    pub fn corner(&self) -> f32 {
        if self.edge_to_edge() { 0.0 } else { R3 }
    }

    pub fn update(&mut self, action: Action) -> Task<Action> {
        match action {
            Action::Drag => window::latest().and_then(window::drag),
            Action::ToggleMaximize => window::latest().and_then(window::toggle_maximize),
            Action::ToggleFullscreen => {
                let mode =
                    if self.frame == Frame::Fullscreen { window::Mode::Windowed } else { window::Mode::Fullscreen };
                window::latest().and_then(move |id| window::set_mode(id, mode))
            }
            Action::Minimize => window::latest().and_then(|id| window::minimize(id, true)),
            Action::Activate => window::latest().and_then(window::gain_focus),
            Action::Attention => {
                if self.focused {
                    Task::none()
                } else {
                    window::latest()
                        .and_then(|id| window::request_user_attention(id, Some(window::UserAttention::Informational)))
                }
            }
            Action::Resize(direction) => window::latest().and_then(move |id| window::drag_resize(id, direction)),
            Action::Resized(size) => {
                self.size = size;
                window::latest().and_then(|id| {
                    window::is_maximized(id).then(move |maximized| {
                        window::mode(id).map(move |mode| {
                            Action::Framed(if mode == window::Mode::Fullscreen {
                                Frame::Fullscreen
                            } else if maximized {
                                Frame::Maximized
                            } else {
                                Frame::Floating
                            })
                        })
                    })
                })
            }
            Action::Focused(focused) => {
                self.focused = focused;
                Task::none()
            }
            Action::Framed(frame) => {
                self.frame = frame;
                Task::none()
            }
        }
    }

    #[must_use]
    pub fn controls_paint(&self, paint: &Paint) -> Paint {
        if self.focused { *paint } else { paint.faded(component::IDLE_CONTROLS) }
    }
}

pub struct Header<'a, M> {
    pub title: String,
    pub subtitle: Option<String>,
    pub leading: Vec<Element<'a, M>>,
    pub trailing: Vec<Element<'a, M>>,
    pub labels: (&'a str, &'a str),
    pub height: f32,
    pub context: Option<M>,
    pub center: Option<Element<'a, M>>,
    pub maximize: bool,
    pub close: bool,
    pub corners: Option<Radius>,
}

impl<'a, M> Header<'a, M> {
    #[must_use]
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            subtitle: None,
            leading: Vec::new(),
            trailing: Vec::new(),
            labels: ("", ""),
            height: HEADER,
            context: None,
            center: None,
            maximize: true,
            close: true,
            corners: None,
        }
    }

    #[must_use]
    pub fn without_close(mut self) -> Self {
        self.close = false;
        self
    }

    #[must_use]
    pub fn center(mut self, element: impl Into<Element<'a, M>>) -> Self {
        self.center = Some(element.into());
        self
    }

    #[must_use]
    pub fn without_maximize(mut self) -> Self {
        self.maximize = false;
        self
    }

    #[must_use]
    pub fn corners(mut self, corners: Radius) -> Self {
        self.corners = Some(corners);
        self
    }

    #[must_use]
    pub fn context(mut self, message: M) -> Self {
        self.context = Some(message);
        self
    }

    #[must_use]
    pub fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    #[must_use]
    pub fn subtitle(mut self, subtitle: Option<String>) -> Self {
        self.subtitle = subtitle;
        self
    }

    #[must_use]
    pub fn leading(mut self, element: impl Into<Element<'a, M>>) -> Self {
        self.leading.push(element.into());
        self
    }

    #[must_use]
    pub fn trailing(mut self, element: impl Into<Element<'a, M>>) -> Self {
        self.trailing.push(element.into());
        self
    }

    #[must_use]
    pub fn labels(mut self, maximize: &'a str, close: &'a str) -> Self {
        self.labels = (maximize, close);
        self
    }
}

#[must_use]
pub fn maximize_button<'a, M: Clone + 'a>(
    paint: &Paint, window: &Window, label: &'a str, message: M,
) -> Element<'a, M> {
    let controls = window.controls_paint(paint);
    let control = button(glide_iced::glyph::window_control(window.edge_to_edge()))
        .on_press(message)
        .width(CONTROL)
        .height(CONTROL)
        .padding(0);
    tooltip(paint, widgets::button(control, move |status| style::maximize_control(&controls, status)), label)
}

#[must_use]
pub fn close_button<'a, M: Clone + 'a>(paint: &Paint, window: &Window, label: &'a str, message: M) -> Element<'a, M> {
    let controls = window.controls_paint(paint);
    let control = button(glide_iced::glyph::window_close()).on_press(message).width(CONTROL).height(CONTROL).padding(0);
    tooltip(paint, widgets::button(control, move |status| style::window_control(&controls, status)), label)
}

#[must_use]
pub fn header<'a, M: Clone + 'a>(
    paint: &Paint, window: &Window, header: Header<'a, M>, on_window: impl Fn(Action) -> M, on_close: M,
    room_taken: f32,
) -> Element<'a, M> {
    let maximize = maximize_button(paint, window, header.labels.0, on_window(Action::ToggleMaximize));
    let close = close_button(paint, window, header.labels.1, on_close);
    let height = header.height;
    let context = header.context;
    let leading_count = header.leading.len();
    let trailing_count = header.trailing.len();
    let first = if header.maximize { row![maximize] } else { row![] };
    let left = header.leading.into_iter().fold(first.spacing(SP2).align_y(Alignment::Center), row::Row::push);
    let mut right = header.trailing.into_iter().fold(row![].spacing(SP2).align_y(Alignment::Center), row::Row::push);
    if header.close {
        right = right.push(Space::new().width(2)).push(close);
    }
    let middle: Element<'a, M> = match header.center {
        Some(center) => container(center).width(Fill).clip(true).into(),
        None => Space::new().width(Fill).into(),
    };
    let bar = row![left, middle, right].spacing(SP2).align_y(Alignment::Center).padding(Padding::from([0.0, SP2]));
    let side = (leading_count.max(trailing_count) + 1) as f32 * (CONTROL + SP2) + SP2 + room_taken;
    let room = window.size.width - 2.0 * side - SP4;
    let mut layers: Vec<Element<'a, M>> = Vec::new();
    if room > 80.0 && !header.title.is_empty() {
        let mut lines =
            column![text(header.title, TextStyle::WIN_TITLE).wrapping(Wrapping::None)].align_x(Alignment::Center);
        if let Some(subtitle) = header.subtitle.filter(|_| window.size.height > 360.0) {
            lines = lines.push(text(subtitle, TextStyle::CAP).color(paint.fg_3()).wrapping(Wrapping::None));
        }
        let color = if window.focused { paint.fg_1() } else { paint.fg_3() };
        layers.push(
            container(
                container(lines)
                    .max_width(room)
                    .clip(true)
                    .style(move |_| container::Style { text_color: Some(color), ..container::Style::default() }),
            )
            .center(Fill)
            .into(),
        );
    }
    layers.push(bar.height(Fill).into());
    let corners = match header.corners {
        _ if window.edge_to_edge() => Radius::default(),
        Some(corners) => corners,
        None => Radius::default().top_left(R3).top_right(R3),
    };
    let (busy, idle) = (paint.bar_bg(), paint.bar_bg_idle());
    let focus = if window.focused { 1.0 } else { 0.0 };
    let bar = crate::anim::animate(focus, crate::anim::Speed::COLOR, move |value| {
        let value = value.clone();
        container(iced::widget::stack(layers))
            .width(Fill)
            .height(height)
            .style(move |_| style::rounded(crate::widget::mix(idle, busy, value.get()), corners))
    });
    let area = mouse_area(bar).on_press(on_window(Action::Drag)).on_double_click(on_window(Action::ToggleMaximize));
    match context {
        Some(message) => area.on_right_press(message).into(),
        None => area.into(),
    }
}

#[must_use]
pub fn resize_handles<'a, M: Clone + 'a>(on_window: impl Fn(Action) -> M) -> Element<'a, M> {
    resize_edges(move |direction| on_window(Action::Resize(direction)))
}

#[must_use]
pub fn resize_edges<'a, M: Clone + 'a>(on_resize: impl Fn(Direction) -> M) -> Element<'a, M> {
    let handle = |width: Length, height: Length, direction: Direction, cursor: mouse::Interaction| -> Element<'a, M> {
        mouse_area(Space::new().width(width).height(height)).on_press(on_resize(direction)).interaction(cursor).into()
    };
    let fixed = Length::Fixed;
    let down = mouse::Interaction::ResizingDiagonallyDown;
    let up = mouse::Interaction::ResizingDiagonallyUp;
    let horizontal = mouse::Interaction::ResizingHorizontally;
    let vertical = mouse::Interaction::ResizingVertically;
    column![
        row![
            handle(fixed(CORNER), fixed(EDGE), Direction::NorthWest, down),
            handle(Fill, fixed(EDGE), Direction::North, vertical),
            handle(fixed(CORNER), fixed(EDGE), Direction::NorthEast, up),
        ],
        row![
            handle(fixed(EDGE), Fill, Direction::West, horizontal),
            Space::new().width(Fill).height(Fill),
            handle(fixed(EDGE), Fill, Direction::East, horizontal),
        ]
        .height(Fill),
        row![
            handle(fixed(CORNER), fixed(EDGE), Direction::SouthWest, up),
            handle(Fill, fixed(EDGE), Direction::South, vertical),
            handle(fixed(CORNER), fixed(EDGE), Direction::SouthEast, down),
        ],
    ]
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edges_compare_by_direction() {
        assert_eq!(Edge(Direction::North), Edge(Direction::North));
        assert_ne!(Edge(Direction::North), Edge(Direction::SouthEast));
    }

    #[test]
    fn only_a_floating_window_is_rounded() {
        let mut window = Window::default();
        assert_eq!(window.corner(), R3);
        for frame in [Frame::Maximized, Frame::Fullscreen] {
            window.frame = frame;
            assert!(window.edge_to_edge());
            assert_eq!(window.corner(), 0.0);
        }
    }

    #[test]
    fn idle_windows_fade_their_controls() {
        let paint = Paint::new(glide_theme::Appearance::Dark, glide_theme::Accent::Blue);
        let mut window = Window::default();
        assert_eq!(window.controls_paint(&paint).alpha, 1.0);
        let _ = window.update(Action::Focused(false));
        assert_eq!(window.controls_paint(&paint).alpha, component::IDLE_CONTROLS);
    }
}

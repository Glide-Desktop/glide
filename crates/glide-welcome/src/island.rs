use glide_sdk::anim::{self, Speed, Value};
use glide_sdk::iced::widget::canvas as surface;
use glide_sdk::iced::widget::canvas::{self, Frame, Geometry, Path, Stroke};
use glide_sdk::iced::{Color, Element, Point, Rectangle, Renderer, Size, Theme, mouse};

use crate::{Island, ui::Message};

struct Backdrop {
    width: Value,
}

impl canvas::Program<Message> for Backdrop {
    type State = ();

    fn draw(&self, (): &(), renderer: &Renderer, _: &Theme, bounds: Rectangle, _: mouse::Cursor) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let width = self.width.get().clamp(0.0, bounds.width);
        let path = Path::rounded_rectangle(
            Point::new((bounds.width - width) / 2.0 + 0.5, 0.5),
            Size::new((width - 1.0).max(0.0), (bounds.height - 1.0).max(0.0)),
            (bounds.height / 2.0).into(),
        );
        frame.fill(&path, Color::BLACK);
        frame.stroke(&path, Stroke::default().with_color(Color::from_rgba(1.0, 1.0, 1.0, 0.2)).with_width(1.0));
        vec![frame.into_geometry()]
    }
}

pub fn backdrop(mode: Island) -> Element<'static, Message> {
    let width = match mode {
        Island::Music => 210.0,
        Island::Notification => 240.0,
        Island::Call => 200.0,
    };
    anim::animate(width, Speed::EXPAND, |value| surface(Backdrop { width: value.clone() }).width(240).height(26)).into()
}

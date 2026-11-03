use glide_sdk::anim::{self, Speed, Value};
use glide_sdk::iced::widget::canvas as surface;
use glide_sdk::iced::widget::canvas::{self, Frame, Geometry, Path};
use glide_sdk::iced::{Color, Element, Fill, Point, Rectangle, Renderer, Theme, mouse};

use crate::ui::Message;

struct Thumb {
    position: Value,
    color: Color,
}

impl canvas::Program<Message> for Thumb {
    type State = ();

    fn draw(&self, (): &(), renderer: &Renderer, _: &Theme, bounds: Rectangle, _: mouse::Cursor) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let circle = Path::circle(Point::new(self.position.get() * 28.0 + 12.0, 12.0), 12.0);
        frame.fill(&circle, self.color);
        vec![frame.into_geometry()]
    }
}

pub fn thumb(index: usize, color: Color) -> Element<'static, Message> {
    anim::animate(index as f32, Speed::EXPAND, move |position| {
        surface(Thumb { position: position.clone(), color }).width(Fill).height(24)
    })
    .into()
}

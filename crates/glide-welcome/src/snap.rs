use glide_sdk::Paint;
use glide_sdk::anim::{self, Speed, Value};
use glide_sdk::iced::widget::canvas as surface;
use glide_sdk::iced::widget::canvas::{self, Frame, Geometry, Path, Stroke};
use glide_sdk::iced::{Element, Fill, Point, Rectangle, Renderer, Size, Theme, mouse};

use crate::{Snap, ui::Message};

#[derive(Debug, Clone, Copy, PartialEq)]
struct Place {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

impl From<Snap> for Place {
    fn from(snap: Snap) -> Self {
        match snap {
            Snap::Floating => Self { x: 0.24, y: 0.24, width: 0.52, height: 0.50 },
            Snap::Left => Self { x: 0.01, y: 0.02, width: 0.48, height: 0.96 },
            Snap::Right => Self { x: 0.51, y: 0.02, width: 0.48, height: 0.96 },
            Snap::Maximized => Self { x: 0.01, y: 0.02, width: 0.98, height: 0.96 },
        }
    }
}

struct Stage {
    paint: Paint,
    x: Value,
    y: Value,
    width: Value,
    height: Value,
    ghost: Option<Place>,
}

fn path(place: Place, bounds: Size) -> Path {
    Path::rounded_rectangle(
        Point::new(place.x * bounds.width, place.y * bounds.height),
        Size::new(place.width * bounds.width, place.height * bounds.height),
        8.0.into(),
    )
}

impl canvas::Program<Message> for Stage {
    type State = ();

    fn draw(&self, (): &(), renderer: &Renderer, _: &Theme, bounds: Rectangle, _: mouse::Cursor) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let background = Path::rounded_rectangle(Point::ORIGIN, bounds.size(), 12.0.into());
        frame.fill(&background, self.paint.sunken_bg());
        if let Some(ghost) = self.ghost {
            let outline = path(ghost, bounds.size());
            frame.fill(&outline, self.paint.acc_soft());
            frame.stroke(&outline, Stroke::default().with_color(self.paint.acc_bg()).with_width(2.0));
        }
        let place = Place { x: self.x.get(), y: self.y.get(), width: self.width.get(), height: self.height.get() };
        let actor = path(place, bounds.size());
        frame.fill(&actor, self.paint.card_bg());
        frame.stroke(&actor, Stroke::default().with_color(self.paint.line_2()).with_width(1.0));
        frame.fill_text(canvas::Text {
            content: "Вікно".into(),
            position: Point::new(place.x * bounds.width + 12.0, place.y * bounds.height + 12.0),
            color: self.paint.fg_1(),
            size: 13.0.into(),
            ..canvas::Text::default()
        });
        vec![frame.into_geometry()]
    }
}

pub fn view(paint: &Paint, snap: Snap, pending: Option<Snap>) -> Element<'static, Message> {
    let paint = *paint;
    let target = Place::from(snap);
    let ghost = pending.map(Place::from);
    anim::animate(target.x, Speed::EXPAND, move |x| {
        let x = x.clone();
        anim::animate(target.y, Speed::EXPAND, move |y| {
            let y = y.clone();
            anim::animate(target.width, Speed::EXPAND, move |width| {
                let width = width.clone();
                anim::animate(target.height, Speed::EXPAND, move |height| {
                    surface(Stage { paint, x, y, width, height: height.clone(), ghost }).width(Fill).height(290)
                })
            })
        })
    })
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snap_targets_stay_within_the_demonstration_stage() {
        for snap in [Snap::Floating, Snap::Left, Snap::Right, Snap::Maximized] {
            let place = Place::from(snap);
            assert!(place.x >= 0.0 && place.y >= 0.0);
            assert!(place.width > 0.0 && place.height > 0.0);
            assert!(place.x + place.width <= 1.0 && place.y + place.height <= 1.0);
        }
        assert!(Place::from(Snap::Left).x < Place::from(Snap::Right).x);
    }
}

use smithay::{
    backend::renderer::element::memory::MemoryRenderBuffer,
    utils::{Logical, Point, Rectangle, Size, Transform},
};

use crate::config::Color;

pub const EDGE: i32 = 12;
pub const TOP_EDGE: i32 = 6;
const STROKE: f32 = 2.0;
const RADIUS: f32 = glide_theme::shape::R3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapZone {
    Left,
    Right,
    Maximize,
}

#[must_use]
pub fn zone_at(
    pointer: Point<f64, Logical>,
    work_area: Rectangle<i32, Logical>,
) -> Option<SnapZone> {
    let left = f64::from(work_area.loc.x);
    let right = f64::from(work_area.loc.x + work_area.size.w);
    let top = f64::from(work_area.loc.y);
    let bottom = f64::from(work_area.loc.y + work_area.size.h);
    if pointer.y < top - f64::from(TOP_EDGE) || pointer.y > bottom {
        return None;
    }
    if pointer.y <= top + f64::from(TOP_EDGE) {
        return Some(SnapZone::Maximize);
    }
    if pointer.x <= left + f64::from(EDGE) {
        return Some(SnapZone::Left);
    }
    if pointer.x >= right - f64::from(EDGE) {
        return Some(SnapZone::Right);
    }
    None
}

#[must_use]
pub fn zone_rect(zone: SnapZone, work_area: Rectangle<i32, Logical>) -> Rectangle<i32, Logical> {
    let half = work_area.size.w / 2;
    match zone {
        SnapZone::Maximize => work_area,
        SnapZone::Left => Rectangle::new(work_area.loc, Size::from((half, work_area.size.h))),
        SnapZone::Right => Rectangle::new(
            Point::from((work_area.loc.x + half, work_area.loc.y)),
            Size::from((work_area.size.w - half, work_area.size.h)),
        ),
    }
}

#[derive(Debug)]
pub struct SnapPreview {
    pub zone: SnapZone,
    pub rect: Rectangle<i32, Logical>,
    reveal: glide_ui::Reveal,
    buffer: Option<(i32, MemoryRenderBuffer)>,
}

impl SnapPreview {
    #[must_use]
    pub fn new(zone: SnapZone, rect: Rectangle<i32, Logical>) -> Self {
        let mut reveal = glide_ui::Reveal::closed();
        reveal.open(glide_theme::motion::t2());
        Self {
            zone,
            rect,
            reveal,
            buffer: None,
        }
    }

    pub fn retarget(&mut self, zone: SnapZone, rect: Rectangle<i32, Logical>) {
        if self.rect.size != rect.size {
            self.buffer = None;
        }
        self.zone = zone;
        self.rect = rect;
        self.reveal.open(glide_theme::motion::t2());
    }

    pub fn dismiss(&mut self) {
        self.reveal.close(glide_theme::motion::t2());
    }

    #[must_use]
    pub const fn is_armed(&self) -> bool {
        self.reveal.is_open()
    }

    #[must_use]
    pub fn is_visible(&self) -> bool {
        self.reveal.is_visible(glide_theme::motion::t2())
    }

    #[must_use]
    pub fn is_animating(&self) -> bool {
        self.reveal.is_animating(glide_theme::motion::t2())
    }

    #[must_use]
    pub fn alpha(&self) -> f32 {
        let duration = glide_theme::motion::t2();
        let progress = if self.reveal.is_open() {
            self.reveal
                .progress(duration, |t| glide_theme::motion::E_OUT.ease_f64(t))
        } else {
            self.reveal
                .progress(duration, |t| glide_theme::motion::E_IO.ease_f64(t))
        };
        #[allow(clippy::cast_possible_truncation)]
        {
            progress.clamp(0.0, 1.0) as f32
        }
    }

    pub fn buffer(
        &mut self,
        fill: Color,
        stroke: Color,
        scale: i32,
    ) -> Option<&MemoryRenderBuffer> {
        if self
            .buffer
            .as_ref()
            .is_none_or(|(cached, _)| *cached != scale)
        {
            let buffer = draw(self.rect.size, fill, stroke, scale)?;
            self.buffer = Some((scale, buffer));
        }
        self.buffer.as_ref().map(|(_, buffer)| buffer)
    }
}

fn draw(
    size: Size<i32, Logical>,
    fill: Color,
    stroke: Color,
    scale: i32,
) -> Option<MemoryRenderBuffer> {
    let scale = scale.max(1);
    let (w, h) = (size.w, size.h);
    let pixels = usize::try_from(
        w.checked_mul(scale)?
            .checked_mul(h.checked_mul(scale)?)?
            .checked_mul(4)?,
    )
    .ok()?;
    let mut buf = vec![0u8; pixels];
    crate::paint::Painter::fill_panel_background(&mut buf, w, h, scale, fill, RADIUS, [true; 4]);
    crate::paint::stroke_rounded_rect_over(
        &mut buf, w, h, 0, 0, w, h, scale, STROKE, RADIUS, stroke,
    );
    Some(MemoryRenderBuffer::from_slice(
        &buf,
        smithay::backend::allocator::Fourcc::Argb8888,
        (w * scale, h * scale),
        scale,
        Transform::Normal,
        None,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area() -> Rectangle<i32, Logical> {
        Rectangle::new(Point::from((0, 34)), Size::from((1920, 1046)))
    }

    #[test]
    fn the_edges_map_to_their_zones_and_the_middle_to_none() {
        assert_eq!(
            zone_at(Point::from((3.0, 500.0)), area()),
            Some(SnapZone::Left)
        );
        assert_eq!(
            zone_at(Point::from((1918.0, 500.0)), area()),
            Some(SnapZone::Right)
        );
        assert_eq!(
            zone_at(Point::from((900.0, 36.0)), area()),
            Some(SnapZone::Maximize)
        );
        assert_eq!(zone_at(Point::from((900.0, 500.0)), area()), None);
        assert_eq!(
            zone_at(Point::from((3.0, 10.0)), area()),
            None,
            "above the work area is the topbar"
        );
    }

    #[test]
    fn the_top_edge_wins_over_the_side_edges() {
        assert_eq!(
            zone_at(Point::from((2.0, 35.0)), area()),
            Some(SnapZone::Maximize)
        );
    }

    #[test]
    fn a_dismissed_preview_stays_visible_while_it_fades_but_is_no_longer_armed() {
        let mut preview = SnapPreview::new(SnapZone::Left, zone_rect(SnapZone::Left, area()));
        assert!(preview.is_armed());
        preview.retarget(SnapZone::Right, zone_rect(SnapZone::Right, area()));
        assert_eq!(preview.zone, SnapZone::Right);
        assert!(preview.is_armed(), "retargeting keeps the preview armed");
        preview.dismiss();
        assert!(!preview.is_armed());
        assert!(
            preview.is_visible() || glide_theme::motion::t2().is_zero(),
            "the fade-out keeps drawing until it settles"
        );
    }

    #[test]
    fn halves_tile_the_work_area_exactly() {
        let left = zone_rect(SnapZone::Left, area());
        let right = zone_rect(SnapZone::Right, area());
        assert_eq!(left.loc, area().loc);
        assert_eq!(left.size.w + right.size.w, area().size.w);
        assert_eq!(right.loc.x, left.loc.x + left.size.w);
        assert_eq!(zone_rect(SnapZone::Maximize, area()), area());
    }
}

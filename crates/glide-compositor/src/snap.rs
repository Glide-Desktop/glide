use std::time::{Duration, Instant};

use smithay::{
    backend::renderer::element::memory::MemoryRenderBuffer,
    utils::{Logical, Point, Rectangle, Size},
};

use crate::config::Color;

pub const EDGE: i32 = 12;
pub const TOP_EDGE: i32 = 6;
const STROKE: f32 = 2.0;
const FILL_ALPHA_MAX: f32 = 0.3;
const RADIUS: f32 = glide_theme::shape::R3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapZone {
    Left,
    Right,
    Maximize,
}

#[must_use]
pub fn zone_at(pointer: Point<f64, Logical>, work_area: Rectangle<i32, Logical>) -> Option<SnapZone> {
    let left = f64::from(work_area.loc.x);
    let right = f64::from(work_area.loc.x + work_area.size.w);
    let top = f64::from(work_area.loc.y);
    let bottom = f64::from(work_area.loc.y + work_area.size.h);
    if pointer.y > bottom {
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
pub fn zone_at_on(
    pointer: Point<f64, Logical>, work_area: Rectangle<i32, Logical>, output: Rectangle<i32, Logical>,
    outputs: &[Rectangle<i32, Logical>],
) -> Option<SnapZone> {
    let zone = zone_at(pointer, work_area)?;
    (!edge_is_shared(zone, pointer, output, outputs)).then_some(zone)
}

fn edge_is_shared(
    zone: SnapZone, pointer: Point<f64, Logical>, output: Rectangle<i32, Logical>, outputs: &[Rectangle<i32, Logical>],
) -> bool {
    let spans = |start: i32, len: i32, at: f64| f64::from(start) <= at && at < f64::from(start + len);
    outputs.iter().filter(|other| **other != output).any(|other| match zone {
        SnapZone::Left => other.loc.x + other.size.w == output.loc.x && spans(other.loc.y, other.size.h, pointer.y),
        SnapZone::Right => other.loc.x == output.loc.x + output.size.w && spans(other.loc.y, other.size.h, pointer.y),
        SnapZone::Maximize => other.loc.y + other.size.h == output.loc.y && spans(other.loc.x, other.size.w, pointer.x),
    })
}

pub const DWELL: Duration = Duration::from_millis(150);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dwell {
    Idle,
    Started,
    Pending,
    Armed(SnapZone),
}

#[derive(Debug, Default)]
pub struct EdgeDwell {
    pending: Option<(SnapZone, Instant)>,
    pointer: Option<Point<f64, Logical>>,
}

impl EdgeDwell {
    pub fn update(&mut self, zone: Option<SnapZone>, pointer: Point<f64, Logical>, now: Instant) -> Dwell {
        self.pointer = Some(pointer);
        let Some(zone) = zone else {
            self.pending = None;
            return Dwell::Idle;
        };
        match self.pending {
            Some((pending, since)) if pending == zone => {
                if now.saturating_duration_since(since) >= DWELL {
                    Dwell::Armed(zone)
                } else {
                    Dwell::Pending
                }
            }
            _ => {
                self.pending = Some((zone, now));
                Dwell::Started
            }
        }
    }

    pub fn reset(&mut self) {
        self.pending = None;
        self.pointer = None;
    }

    #[must_use]
    pub const fn pointer(&self) -> Option<Point<f64, Logical>> {
        self.pointer
    }
}

pub const HOT_CORNER: f64 = 2.0;

#[must_use]
pub fn in_hot_corner(pointer: Point<f64, Logical>, outputs: &[Rectangle<i32, Logical>]) -> bool {
    let covered = |x: i32, y: i32| outputs.iter().any(|o| o.contains((x, y)));
    outputs.iter().any(|o| {
        let dx = pointer.x - f64::from(o.loc.x);
        let dy = pointer.y - f64::from(o.loc.y);
        (0.0..HOT_CORNER).contains(&dx)
            && (0.0..HOT_CORNER).contains(&dy)
            && !covered(o.loc.x - 1, o.loc.y)
            && !covered(o.loc.x, o.loc.y - 1)
    })
}

#[derive(Debug, Default)]
pub struct HotCorner {
    inside: bool,
}

impl HotCorner {
    pub fn update(&mut self, pointer: Point<f64, Logical>, outputs: &[Rectangle<i32, Logical>]) -> bool {
        let inside = in_hot_corner(pointer, outputs);
        let entered = inside && !self.inside;
        self.inside = inside;
        entered
    }
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
    from: Rectangle<i32, Logical>,
    glide: glide_ui::Tween,
    buffer: Option<((i32, Color, Color), MemoryRenderBuffer)>,
}

impl SnapPreview {
    #[must_use]
    pub fn new(zone: SnapZone, rect: Rectangle<i32, Logical>) -> Self {
        let mut reveal = glide_ui::Reveal::closed().with_ease(glide_theme::motion::ease_out);
        reveal.open(glide_theme::motion::t2());
        Self { zone, rect, reveal, from: rect, glide: glide_ui::Tween::at(1.0), buffer: None }
    }

    pub fn retarget(&mut self, zone: SnapZone, rect: Rectangle<i32, Logical>) {
        if self.rect != rect {
            self.from = self.shown();
            self.glide = glide_ui::Tween::at(0.0);
            self.glide.retarget(1.0, glide_theme::motion::t2(), glide_theme::motion::ease_out);
        }
        if self.rect.size != rect.size {
            self.buffer = None;
        }
        self.zone = zone;
        self.rect = rect;
        self.reveal.open(glide_theme::motion::t2());
    }

    #[must_use]
    pub fn shown(&self) -> Rectangle<i32, Logical> {
        let t = self.glide.value(glide_theme::motion::t2(), glide_theme::motion::ease_out);
        let mix = |from: i32, to: i32| (f64::from(from) + (f64::from(to) - f64::from(from)) * t).round() as i32;
        Rectangle::new(
            Point::from((mix(self.from.loc.x, self.rect.loc.x), mix(self.from.loc.y, self.rect.loc.y))),
            Size::from((
                mix(self.from.size.w, self.rect.size.w).max(1),
                mix(self.from.size.h, self.rect.size.h).max(1),
            )),
        )
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
        self.reveal.is_animating(glide_theme::motion::t2()) || self.glide.is_animating(glide_theme::motion::t2())
    }

    #[must_use]
    pub fn alpha(&self) -> f32 {
        let progress = self.reveal.value(glide_theme::motion::t2());
        ((progress / 0.4).min(1.0)).clamp(0.0, 1.0) as f32
    }

    pub fn buffer(&mut self, fill: Color, stroke: Color, scale: i32) -> Option<&MemoryRenderBuffer> {
        let key = (scale, translucent(fill), stroke);
        if self.buffer.as_ref().is_none_or(|(cached, _)| *cached != key) {
            let buffer = draw(self.rect.size, key.1, stroke, scale)?;
            self.buffer = Some((key, buffer));
        }
        self.buffer.as_ref().map(|(_, buffer)| buffer)
    }
}

#[must_use]
fn translucent(fill: Color) -> Color {
    let [r, g, b, a] = fill.0;
    Color([r, g, b, a.min(FILL_ALPHA_MAX)])
}

fn draw(size: Size<i32, Logical>, fill: Color, stroke: Color, scale: i32) -> Option<MemoryRenderBuffer> {
    let mut c = crate::raster::Canvas::new(size.w, size.h, scale.max(1))?;
    c.panel(fill, RADIUS);
    c.stroke(c.full(), STROKE, RADIUS, stroke);
    Some(c.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::raster::rect;

    fn area() -> Rectangle<i32, Logical> {
        rect(0, 34, 1920, 1046)
    }

    #[test]
    fn the_hot_corner_fires_once_per_entry_into_the_top_left_corner() {
        let outputs = [rect(0, 0, 1920, 1080)];
        let mut corner = HotCorner::default();
        assert!(!corner.update(Point::from((400.0, 300.0)), &outputs));
        assert!(corner.update(Point::from((0.0, 0.0)), &outputs));
        assert!(
            !corner.update(Point::from((1.0, 0.5)), &outputs),
            "staying in the corner must not toggle the overview again"
        );
        assert!(!corner.update(Point::from((40.0, 40.0)), &outputs));
        assert!(corner.update(Point::from((0.0, 1.0)), &outputs));
    }

    #[test]
    fn a_retargeted_preview_glides_from_where_it_was() {
        let mut preview = SnapPreview::new(SnapZone::Left, rect(0, 34, 960, 1046));
        assert_eq!(preview.shown(), rect(0, 34, 960, 1046));
        preview.retarget(SnapZone::Right, rect(960, 34, 960, 1046));
        assert_eq!(preview.rect, rect(960, 34, 960, 1046));
        if !glide_theme::motion::is_disabled() {
            assert_eq!(preview.shown().loc.x, 0, "it starts at the old zone");
            assert!(preview.is_animating());
        }
    }

    #[test]
    fn a_corner_shared_with_a_neighbouring_output_is_not_hot() {
        let outputs = [rect(0, 0, 1920, 1080), rect(1920, 0, 1920, 1080)];
        assert!(in_hot_corner(Point::from((0.0, 0.0)), &outputs));
        assert!(
            !in_hot_corner(Point::from((1920.0, 0.0)), &outputs),
            "the pointer crosses into the right output here, it is not a screen corner"
        );
    }

    #[test]
    fn the_edges_map_to_their_zones_and_the_middle_to_none() {
        assert_eq!(zone_at(Point::from((3.0, 500.0)), area()), Some(SnapZone::Left));
        assert_eq!(zone_at(Point::from((1918.0, 500.0)), area()), Some(SnapZone::Right));
        assert_eq!(zone_at(Point::from((900.0, 36.0)), area()), Some(SnapZone::Maximize));
        assert_eq!(zone_at(Point::from((900.0, 500.0)), area()), None);
    }

    #[test]
    fn the_topbar_strip_above_the_work_area_arms_maximize() {
        for pointer in [(900.0, 0.0), (3.0, 10.0), (1919.0, 33.0)] {
            assert_eq!(
                zone_at(Point::from(pointer), area()),
                Some(SnapZone::Maximize),
                "the pointer is pinned at the output's top edge, which the topbar covers"
            );
        }
    }

    #[test]
    fn the_top_edge_wins_over_the_side_edges() {
        assert_eq!(zone_at(Point::from((2.0, 35.0)), area()), Some(SnapZone::Maximize));
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
    fn the_preview_fill_stays_translucent_even_for_an_opaque_token() {
        let opaque = Color([0.2, 0.4, 0.9, 1.0]);
        let fill = translucent(opaque);
        assert!(fill.0[3] <= FILL_ALPHA_MAX);
        assert_eq!(fill.0[..3], opaque.0[..3]);
        let soft = Color([0.2, 0.4, 0.9, 0.12]);
        assert_eq!(translucent(soft), soft);
    }

    #[test]
    fn an_edge_shared_with_a_neighbouring_output_never_arms_a_half() {
        let left_output = rect(0, 0, 1920, 1080);
        let right_output = rect(1920, 0, 1920, 1080);
        let outputs = [left_output, right_output];
        let left_area = area();
        let right_area = rect(1920, 34, 1920, 1046);

        assert_eq!(
            zone_at_on(Point::from((1918.0, 500.0)), left_area, left_output, &outputs),
            None,
            "the right edge of the left monitor leads onto the right monitor"
        );
        assert_eq!(
            zone_at_on(Point::from((1921.0, 500.0)), right_area, right_output, &outputs),
            None,
            "the left edge of the right monitor leads back onto the left one"
        );
        assert_eq!(
            zone_at_on(Point::from((3.0, 500.0)), left_area, left_output, &outputs),
            Some(SnapZone::Left),
            "the outer edge still snaps"
        );
        assert_eq!(zone_at_on(Point::from((3838.0, 500.0)), right_area, right_output, &outputs), Some(SnapZone::Right));
    }

    #[test]
    fn a_shared_edge_only_suppresses_where_the_neighbour_actually_is() {
        let tall = rect(0, 0, 1920, 1080);
        let short = rect(1920, 0, 1280, 500);
        let outputs = [tall, short];
        assert_eq!(zone_at_on(Point::from((1918.0, 300.0)), area(), tall, &outputs), None);
        assert_eq!(
            zone_at_on(Point::from((1918.0, 800.0)), area(), tall, &outputs),
            Some(SnapZone::Right),
            "below the short neighbour the edge is a wall again"
        );
    }

    #[test]
    fn an_output_stacked_above_suppresses_the_maximize_strip() {
        let lower = rect(0, 1080, 1920, 1080);
        let upper = rect(0, 0, 1920, 1080);
        let lower_area = rect(0, 1114, 1920, 1046);
        assert_eq!(zone_at_on(Point::from((900.0, 1081.0)), lower_area, lower, &[upper, lower]), None);
        assert_eq!(zone_at_on(Point::from((900.0, 1081.0)), lower_area, lower, &[lower]), Some(SnapZone::Maximize));
    }

    #[test]
    fn a_zone_arms_only_after_the_pointer_dwells() {
        let start = Instant::now();
        let at = Point::from((3.0, 500.0));
        let mut dwell = EdgeDwell::default();

        assert_eq!(dwell.update(Some(SnapZone::Left), at, start), Dwell::Started);
        assert_eq!(dwell.update(Some(SnapZone::Left), at, start + DWELL / 2), Dwell::Pending);
        assert_eq!(dwell.update(Some(SnapZone::Left), at, start + DWELL), Dwell::Armed(SnapZone::Left));
        assert_eq!(dwell.pointer(), Some(at));
    }

    #[test]
    fn leaving_or_switching_the_edge_restarts_the_dwell() {
        let start = Instant::now();
        let at = Point::from((3.0, 500.0));
        let mut dwell = EdgeDwell::default();

        dwell.update(Some(SnapZone::Left), at, start);
        assert_eq!(dwell.update(None, at, start + DWELL / 2), Dwell::Idle);
        assert_eq!(
            dwell.update(Some(SnapZone::Left), at, start + DWELL),
            Dwell::Started,
            "a pass through the middle forgets the earlier dwell"
        );
        assert_eq!(dwell.update(Some(SnapZone::Maximize), at, start + DWELL * 3), Dwell::Started);
        dwell.reset();
        assert_eq!(dwell.pointer(), None);
    }

    #[test]
    fn the_dwell_is_long_enough_to_cross_an_edge_but_short_enough_to_feel_instant() {
        assert!(DWELL >= Duration::from_millis(100));
        assert!(DWELL <= Duration::from_millis(250));
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

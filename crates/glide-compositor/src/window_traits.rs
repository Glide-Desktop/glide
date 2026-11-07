use std::collections::HashMap;

use glide_layout::{Rectangle, WindowId};

pub const MAX_BLUR_REGIONS: usize = 16;
pub const PIP_WIDTH: i32 = 384;
pub const PIP_MARGIN: i32 = 24;
const MAX_BLUR_RADIUS: i32 = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlurRegion {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub radius: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    Aspect(Option<f64>),
    KeepAbove(bool),
    PictureInPicture(bool),
    Blur(Vec<BlurRegion>),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Saved {
    pub frame: Rectangle,
    pub maximized: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Traits {
    pub aspect: Option<f64>,
    pub keep_above: bool,
    pub pip: bool,
    pub blur: Vec<BlurRegion>,
    pub saved: HashMap<WindowId, Saved>,
}

impl Traits {
    #[must_use]
    pub const fn above(&self) -> bool {
        self.keep_above || self.pip
    }
}

#[must_use]
pub fn aspect(width: u32, height: u32) -> Option<f64> {
    (width > 0 && height > 0).then(|| f64::from(width) / f64::from(height))
}

#[must_use]
pub fn blur_regions(raw: &[(i32, i32, i32, i32, i32)]) -> Vec<BlurRegion> {
    raw.iter()
        .take(MAX_BLUR_REGIONS)
        .filter(|(_, _, width, height, _)| *width > 0 && *height > 0)
        .map(|&(x, y, width, height, radius)| BlurRegion {
            x,
            y,
            width,
            height,
            radius: radius.clamp(0, MAX_BLUR_RADIUS).min(width / 2).min(height / 2),
        })
        .collect()
}

#[must_use]
pub fn pip_frame(work_area: Rectangle, aspect: Option<f64>, content_aspect: f64) -> Rectangle {
    let aspect = aspect.unwrap_or(content_aspect).clamp(0.25, 4.0);
    let width = PIP_WIDTH.min(work_area.size.width - 2 * PIP_MARGIN).max(1);
    let height = ((f64::from(width) / aspect).round() as i32).clamp(1, (work_area.size.height - 2 * PIP_MARGIN).max(1));
    let width = if height < (f64::from(width) / aspect).round() as i32 {
        (f64::from(height) * aspect).round() as i32
    } else {
        width
    };
    let x = work_area.loc.x + work_area.size.width - PIP_MARGIN - width;
    let y = work_area.loc.y + work_area.size.height - PIP_MARGIN - height;
    Rectangle::from_loc_and_size(x.max(work_area.loc.x), y.max(work_area.loc.y), width, height)
}

#[derive(Debug, Default)]
pub struct WindowTraits {
    by_pid: HashMap<i32, Traits>,
}

impl WindowTraits {
    pub fn change(&mut self, pid: i32, change: &Change) -> &mut Traits {
        self.by_pid.retain(|known, _| *known == pid || crate::framed::process_alive(*known));
        let traits = self.by_pid.entry(pid).or_default();
        match change {
            Change::Aspect(aspect) => traits.aspect = *aspect,
            Change::KeepAbove(keep) => traits.keep_above = *keep,
            Change::PictureInPicture(on) => traits.pip = *on,
            Change::Blur(regions) => traits.blur.clone_from(regions),
        }
        traits
    }

    #[must_use]
    pub fn of(&self, pid: i32) -> Option<&Traits> {
        self.by_pid.get(&pid)
    }

    pub fn of_mut(&mut self, pid: i32) -> Option<&mut Traits> {
        self.by_pid.get_mut(&pid)
    }

    #[must_use]
    pub fn blurs(&self) -> bool {
        self.by_pid.values().any(|traits| !traits.blur.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_zero_side_clears_the_aspect() {
        assert_eq!(aspect(1920, 1080), Some(1920.0 / 1080.0));
        assert_eq!(aspect(0, 1080), None);
        assert_eq!(aspect(1920, 0), None);
    }

    #[test]
    fn blur_regions_drop_empty_ones_and_keep_radii_inside_the_region() {
        let regions = blur_regions(&[(10, 20, 640, 82, 12), (0, 0, 0, 10, 4), (5, 5, 20, 10, 500)]);
        assert_eq!(regions.len(), 2);
        assert_eq!(regions[0], BlurRegion { x: 10, y: 20, width: 640, height: 82, radius: 12 });
        assert_eq!(regions[1].radius, 5, "a radius never exceeds half the shorter side");
        let many: Vec<_> = (0..40).map(|index| (index, 0, 10, 10, 0)).collect();
        assert_eq!(blur_regions(&many).len(), MAX_BLUR_REGIONS);
    }

    #[test]
    fn picture_in_picture_sits_in_the_bottom_right_corner_of_the_work_area() {
        let area = Rectangle::from_loc_and_size(0, 32, 1280, 688);
        let frame = pip_frame(area, Some(16.0 / 9.0), 1.0);
        assert_eq!(frame, Rectangle::from_loc_and_size(1280 - 24 - 384, 32 + 688 - 24 - 216, 384, 216));
        let portrait = pip_frame(Rectangle::from_loc_and_size(0, 0, 1280, 300), Some(9.0 / 16.0), 1.0);
        assert!(portrait.size.height <= 300 - 48, "{portrait:?}");
        assert!(portrait.loc.y >= 0 && portrait.loc.x + portrait.size.width <= 1280 - 24);
        assert_eq!(pip_frame(area, None, 4.0 / 3.0).size.height, 288, "without an aspect the window's own shape");
    }

    #[test]
    fn traits_are_kept_per_process_and_picture_in_picture_counts_as_above() {
        let mut traits = WindowTraits::default();
        let pid = i32::try_from(std::process::id()).unwrap();
        assert!(!traits.change(pid, &Change::PictureInPicture(true)).keep_above);
        assert!(traits.of(pid).unwrap().above());
        traits.change(pid, &Change::Blur(vec![BlurRegion { x: 0, y: 0, width: 4, height: 4, radius: 0 }]));
        assert!(traits.blurs());
        traits.change(pid, &Change::Blur(Vec::new()));
        assert!(!traits.blurs());
    }
}

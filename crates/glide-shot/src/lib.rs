pub mod compose;
pub mod model;
pub mod preview;
pub mod render;

use glide_paint as paint;
use glide_shell::{i18n, raster};

#[must_use]
pub fn crop_region(target: model::Area, scale: f64, size: (u32, u32)) -> Option<(i32, i32, i32, i32)> {
    raster::physical_crop(target, scale, (size.0 as i32, size.1 as i32))
}

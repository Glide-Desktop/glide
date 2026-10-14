use std::collections::HashMap;

use glide_layout::WorkspaceId;
use smithay::{
    backend::renderer::element::memory::MemoryRenderBuffer,
    utils::{Logical, Point, Rectangle, Scale, Size},
};

use super::{OverviewRenderElement, PerScale, RasterKey, hover_anim, place};
use crate::{clip::ClipCapableRenderer, config::OverviewTheme, raster::ShellRenderer};

const TOP_GAP: i32 = 10;
const THUMB_W: i32 = 92;
const THUMB_H: i32 = 52;
const GAP: i32 = 8;
const BORDER: f32 = 2.0;
const LIFT: f64 = 2.0;
const RISE: f64 = 14.0;
const SHADOW_DY: f32 = 2.0;
const SHADOW_BLUR: f32 = 8.0;
const SHADOW_ALPHA: f32 = 0.28;
const HOVER_BUCKETS: f32 = 12.0;

type CachedThumb = HashMap<(bool, u8), (RasterKey, MemoryRenderBuffer)>;

#[derive(Debug, Default)]
pub(super) struct Strip {
    workspaces: Vec<WorkspaceId>,
    active: Option<WorkspaceId>,
    hover: glide_ui::HoverAnimation<WorkspaceId>,
    thumbs: PerScale<CachedThumb>,
}

impl Strip {
    pub(super) fn set(&mut self, workspaces: Vec<WorkspaceId>, active: WorkspaceId) -> bool {
        let shown = self.shown();
        if self.workspaces != workspaces {
            self.workspaces = workspaces;
        }
        self.active = Some(active);
        if self.hover.target().is_some_and(|id| !self.workspaces.contains(&id)) {
            self.hover.retarget_without_restarting(None);
        }
        shown != self.shown()
    }

    pub(super) fn shown(&self) -> bool {
        self.workspaces.len() > 1
    }

    pub(super) fn band(&self) -> i32 {
        if self.shown() { TOP_GAP + THUMB_H + glide_theme::spacing::SP4 } else { 0 }
    }

    pub(super) fn set_hover(&mut self, hover: Option<WorkspaceId>) -> bool {
        self.hover.set_with(hover, hover_anim())
    }

    pub(super) fn is_animating(&self) -> bool {
        self.hover.progress(hover_anim()) < 1.0
    }

    pub(super) fn rects(
        &self, area: Rectangle<i32, Logical>,
    ) -> impl Iterator<Item = (WorkspaceId, Rectangle<i32, Logical>)> + '_ {
        let count = i32::try_from(self.workspaces.len()).unwrap_or(0);
        let total = count * THUMB_W + (count - 1).max(0) * GAP;
        let left = area.loc.x + (area.size.w - total) / 2;
        let y = area.loc.y + TOP_GAP;
        self.workspaces.iter().enumerate().map(move |(index, id)| {
            let x = left + i32::try_from(index).unwrap_or(0) * (THUMB_W + GAP);
            (*id, Rectangle::new(Point::from((x, y)), Size::from((THUMB_W, THUMB_H))))
        })
    }

    pub(super) fn workspace_at(&self, area: Rectangle<i32, Logical>, pos: Point<f64, Logical>) -> Option<WorkspaceId> {
        self.shown().then(|| self.rects(area).find(|(_, rect)| rect.to_f64().contains(pos)).map(|(id, _)| id)).flatten()
    }

    pub(super) fn render<R>(
        &mut self, renderer: &mut R, theme: &OverviewTheme, (area, t): (Rectangle<i32, Logical>, f64),
        (scale, output_scale): (Scale<f64>, i32), elements: &mut Vec<OverviewRenderElement<R>>,
    ) where
        R: ShellRenderer + ClipCapableRenderer,
    {
        if !self.shown() || t <= 0.0 {
            return;
        }
        let key = RasterKey::new(theme, output_scale);
        let margin = margin();
        let alpha = t.clamp(0.0, 1.0) as f32;
        let rise = (1.0 - t) * RISE;
        let placements: Vec<(WorkspaceId, Rectangle<i32, Logical>)> = self.rects(area).collect();
        for (id, rect) in placements {
            let level = self.hover.level(id, hover_anim());
            let border_t = glide_theme::motion::E_OUT.ease_f64(level) as f32;
            let lift = glide_theme::motion::E_OVER.ease_f64(level) * LIFT;
            let bucket = (border_t * HOVER_BUCKETS).round() as u8;
            let active = self.active == Some(id);
            let thumbs = self.thumbs.entry(output_scale).or_default();
            let needs_redraw = thumbs.get(&(active, bucket)).is_none_or(|(cached, _)| *cached != key);
            if needs_redraw {
                let hover_t = f32::from(bucket) / HOVER_BUCKETS;
                if let Some(memory) = draw_thumb(theme, active, hover_t, output_scale) {
                    thumbs.insert((active, bucket), (key, memory));
                }
            }
            let Some((_, memory)) = thumbs.get(&(active, bucket)) else {
                continue;
            };
            let pos = Point::<f64, Logical>::from((
                f64::from(rect.loc.x - margin),
                f64::from(rect.loc.y - margin) - rise - lift,
            ))
            .to_physical(scale);
            elements.extend(place(renderer, pos, memory, alpha).map(OverviewRenderElement::Buffer));
        }
    }
}

fn margin() -> i32 {
    crate::paint::shadow_fade_margin(SHADOW_BLUR.ceil() as i32)
}

fn draw_thumb(theme: &OverviewTheme, active: bool, hover_t: f32, output_scale: i32) -> Option<MemoryRenderBuffer> {
    let (margin, radius) = (margin(), glide_theme::shape::R2);
    let color = crate::config::Color([0.0, 0.0, 0.0, SHADOW_ALPHA]);
    let shadow = glide_theme::elevation::ShadowLayer { dx: 0.0, dy: SHADOW_DY, blur: SHADOW_BLUR, spread: 0.0, color };
    let mut c = crate::raster::shadow((THUMB_W, THUMB_H), margin, radius, &[shadow], output_scale)?;
    let thumb = crate::raster::rect(margin, margin, THUMB_W, THUMB_H);
    c.gradient(thumb, radius, theme.strip_from, theme.strip_to);
    let border = if active { theme.highlight } else { theme.strip_pick.with_alpha(theme.strip_pick.0[3] * hover_t) };
    if border.0[3] > 0.0 {
        c.stroke(thumb, BORDER, radius, border);
    }
    Some(c.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area() -> Rectangle<i32, Logical> {
        Rectangle::new(Point::from((0, 34)), Size::from((1280, 766)))
    }

    #[test]
    fn a_single_workspace_shows_no_strip_and_reserves_nothing() {
        let mut strip = Strip::default();
        strip.set(vec![WorkspaceId(1)], WorkspaceId(1));
        assert!(!strip.shown());
        assert_eq!(strip.band(), 0);
        assert_eq!(strip.workspace_at(area(), Point::from((640.0, 60.0))), None);
    }

    #[test]
    fn thumbs_are_centred_under_the_topbar_and_hit_test_by_workspace() {
        let mut strip = Strip::default();
        assert!(strip.set(vec![WorkspaceId(1), WorkspaceId(2)], WorkspaceId(2)));
        let rects: Vec<_> = strip.rects(area()).collect();
        assert_eq!(rects.len(), 2);
        let (first, second) = (rects[0].1, rects[1].1);
        assert_eq!(first.loc.y, 34 + TOP_GAP);
        assert_eq!(second.loc.x - (first.loc.x + first.size.w), GAP);
        let total = first.size.w * 2 + GAP;
        assert_eq!(first.loc.x, (1280 - total) / 2);
        let centre = |rect: Rectangle<i32, Logical>| {
            Point::from((
                f64::from(rect.loc.x) + f64::from(rect.size.w) / 2.0,
                f64::from(rect.loc.y) + f64::from(rect.size.h) / 2.0,
            ))
        };
        assert_eq!(strip.workspace_at(area(), centre(second)), Some(WorkspaceId(2)));
        assert_eq!(strip.workspace_at(area(), Point::from((10.0, 10.0))), None);
        assert!(strip.band() > THUMB_H);
    }

    #[test]
    fn a_hover_on_a_vanished_workspace_is_dropped_on_the_next_set() {
        let mut strip = Strip::default();
        strip.set(vec![WorkspaceId(1), WorkspaceId(2)], WorkspaceId(1));
        assert!(strip.set_hover(Some(WorkspaceId(2))));
        assert!(!strip.set(vec![WorkspaceId(1), WorkspaceId(3)], WorkspaceId(1)));
        assert_eq!(strip.hover.target(), None);
    }
}

use glide_geom::Rectangle;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Track {
    Fixed(i32),
    Flex(i32),
}

#[must_use]
pub fn layout_row(bounds: Rectangle, tracks: &[Track]) -> Vec<Rectangle> {
    layout_line(bounds, tracks, true)
}

#[must_use]
pub fn layout_column(bounds: Rectangle, tracks: &[Track]) -> Vec<Rectangle> {
    layout_line(bounds, tracks, false)
}

fn layout_line(bounds: Rectangle, tracks: &[Track], horizontal: bool) -> Vec<Rectangle> {
    let total = if horizontal {
        bounds.size.width
    } else {
        bounds.size.height
    };

    let fixed_sum: i32 = tracks
        .iter()
        .map(|t| match t {
            Track::Fixed(n) => *n,
            Track::Flex(_) => 0,
        })
        .sum();
    let flex_weight_sum: i32 = tracks
        .iter()
        .map(|t| match t {
            Track::Flex(w) => *w,
            Track::Fixed(_) => 0,
        })
        .sum();
    let remaining = (total - fixed_sum).max(0);
    let flex_count = tracks
        .iter()
        .filter(|t| matches!(t, Track::Flex(_)))
        .count();

    let mut out = Vec::with_capacity(tracks.len());
    let mut cursor = if horizontal {
        bounds.loc.x
    } else {
        bounds.loc.y
    };
    let mut flex_used = 0;
    let mut flex_seen = 0;

    for track in tracks {
        let length = match track {
            Track::Fixed(n) => *n,
            Track::Flex(w) => {
                flex_seen += 1;
                if flex_weight_sum <= 0 {
                    0
                } else if flex_seen == flex_count {
                    // Last flex track absorbs the rounding remainder so the
                    // tracks always tile the bounds exactly, with no gap.
                    remaining - flex_used
                } else {
                    let length = remaining * w / flex_weight_sum;
                    flex_used += length;
                    length
                }
            }
        };

        let rect = if horizontal {
            Rectangle::from_loc_and_size(cursor, bounds.loc.y, length, bounds.size.height)
        } else {
            Rectangle::from_loc_and_size(bounds.loc.x, cursor, bounds.size.width, length)
        };
        out.push(rect);
        cursor += length;
    }

    out
}

/// Finds the last entry (by draw order - later entries are treated as on
/// top) whose rectangle contains `(x, y)`. Coordinates are `f64` because
/// pointer input carries sub-pixel precision even though layout tracks are
/// laid out on integer bounds.
#[must_use]
pub fn hit_test<T: Copy>(entries: &[(T, Rectangle)], x: f64, y: f64) -> Option<T> {
    entries
        .iter()
        .rev()
        .find(|(_, r)| {
            x >= f64::from(r.loc.x)
                && x < f64::from(r.loc.x + r.size.width)
                && y >= f64::from(r.loc.y)
                && y < f64::from(r.loc.y + r.size.height)
        })
        .map(|(id, _)| *id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: i32, y: i32, w: i32, h: i32) -> Rectangle {
        Rectangle::from_loc_and_size(x, y, w, h)
    }

    #[test]
    fn fixed_tracks_tile_left_to_right_with_no_gap_or_overlap() {
        let rects = layout_row(rect(0, 0, 300, 40), &[Track::Fixed(50), Track::Fixed(80)]);
        assert_eq!(rects[0], rect(0, 0, 50, 40));
        assert_eq!(rects[1], rect(50, 0, 80, 40));
    }

    #[test]
    fn a_single_flex_track_absorbs_all_remaining_space() {
        let rects = layout_row(
            rect(0, 0, 300, 40),
            &[Track::Fixed(32), Track::Flex(1), Track::Fixed(64)],
        );
        assert_eq!(rects[0], rect(0, 0, 32, 40));
        assert_eq!(rects[1], rect(32, 0, 204, 40), "300 - 32 - 64 = 204");
        assert_eq!(rects[2], rect(236, 0, 64, 40));
    }

    #[test]
    fn matches_the_titlebar_button_cluster_geometry_exactly() {
        // The same four zones HeaderBar::zone_at used to compute by hand:
        // maximize [0, bw), drag [bw, w-2bw), chevron [w-2bw, w-bw), close [w-bw, w).
        let (w, h, bw) = (800, 32, 32);
        let rects = layout_row(
            rect(0, 0, w, h),
            &[
                Track::Fixed(bw),
                Track::Flex(1),
                Track::Fixed(bw),
                Track::Fixed(bw),
            ],
        );
        assert_eq!(rects[0], rect(0, 0, bw, h), "maximize");
        assert_eq!(rects[1], rect(bw, 0, w - 3 * bw, h), "drag");
        assert_eq!(rects[2], rect(w - 2 * bw, 0, bw, h), "chevron");
        assert_eq!(rects[3], rect(w - bw, 0, bw, h), "close");
    }

    #[test]
    fn multiple_flex_tracks_split_proportionally_to_their_weight() {
        let rects = layout_row(rect(0, 0, 300, 40), &[Track::Flex(1), Track::Flex(2)]);
        assert_eq!(rects[0].size.width, 100, "weight 1 of 3 shares -> 100");
        assert_eq!(rects[1].size.width, 200, "weight 2 of 3 shares -> 200");
    }

    #[test]
    fn flex_tracks_never_go_negative_when_fixed_tracks_overflow_the_bounds() {
        let rects = layout_row(rect(0, 0, 50, 40), &[Track::Fixed(80), Track::Flex(1)]);
        assert_eq!(
            rects[1].size.width, 0,
            "no space left to give the flex track"
        );
    }

    #[test]
    fn columns_stack_top_to_bottom() {
        let rects = layout_column(rect(10, 20, 100, 96), &[Track::Fixed(32); 3]);
        assert_eq!(rects[0], rect(10, 20, 100, 32));
        assert_eq!(rects[1], rect(10, 52, 100, 32));
        assert_eq!(rects[2], rect(10, 84, 100, 32));
    }

    #[test]
    fn hit_test_finds_the_containing_rectangle() {
        let entries = [("a", rect(0, 0, 10, 10)), ("b", rect(10, 0, 10, 10))];
        assert_eq!(hit_test(&entries, 5.0, 5.0), Some("a"));
        assert_eq!(hit_test(&entries, 15.0, 5.0), Some("b"));
    }

    #[test]
    fn hit_test_is_exclusive_on_the_far_edge() {
        let entries = [("a", rect(0, 0, 10, 10))];
        assert_eq!(hit_test(&entries, 9.999, 9.999), Some("a"));
        assert_eq!(hit_test(&entries, 10.0, 5.0), None);
        assert_eq!(hit_test(&entries, 5.0, 10.0), None);
    }

    #[test]
    fn hit_test_outside_every_rectangle_is_none() {
        let entries = [("a", rect(0, 0, 10, 10))];
        assert_eq!(hit_test(&entries, -1.0, 5.0), None);
        assert_eq!(hit_test(&entries, 5.0, -1.0), None);
    }

    #[test]
    fn hit_test_picks_the_later_entry_when_rectangles_overlap() {
        let entries = [("under", rect(0, 0, 10, 10)), ("over", rect(0, 0, 10, 10))];
        assert_eq!(hit_test(&entries, 5.0, 5.0), Some("over"));
    }

    #[test]
    fn empty_input_produces_no_rectangles_and_hit_tests_to_nothing() {
        let rects = layout_row(rect(0, 0, 300, 40), &[]);
        assert!(rects.is_empty());
        let entries: [(u8, Rectangle); 0] = [];
        assert_eq!(hit_test(&entries, 5.0, 5.0), None);
    }
}

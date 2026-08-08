//! Overview layout: arranging window previews into centred rows.
//!
//! Pure geometry, like the rest of this crate — it takes frame sizes in and
//! hands target rectangles back, and knows nothing about rendering,
//! animation or input. The compositor turns a [`OverviewSlot`] into a
//! scaled render element; nothing here is aware that happens.
//!
//! # Why rows rather than a grid
//!
//! A fixed `ceil(sqrt(n))` grid gives every window an identical cell, which
//! forces wildly different aspect ratios into the same box — a portrait
//! window and a wide terminal end up scaled to wildly different sizes to
//! fit one shared cell, and half of each cell is empty. The row model used
//! here (the same shape GNOME's overview uses) instead:
//!
//! - picks a row count from the area's aspect ratio, not from `n` alone,
//! - gives every window in a row the same *height* budget and lets its
//!   width follow its own aspect ratio,
//! - centres each row horizontally and the block of rows vertically.
//!
//! The result is that previews keep their proportions and the arrangement
//! stays visually balanced when windows differ in shape.

use crate::{
    geometry::{Point, Rectangle, Size},
    ids::WindowId,
};

/// Tunables for [`compute_overview_layout`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OverviewOptions {
    /// Space between adjacent previews, and between rows.
    pub gap: i32,
    /// Vertical space reserved under each preview for its label.
    pub label_height: i32,
    /// Upper bound on a preview's scale.
    ///
    /// Without a cap, a lone small window would be blown up past its native
    /// size and look blurry; overview previews should never be *larger*
    /// than the window itself.
    pub max_scale: f64,
}

impl Default for OverviewOptions {
    fn default() -> Self {
        Self {
            gap: 32,
            label_height: 24,
            max_scale: 1.0,
        }
    }
}

/// Where one window's preview should be drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OverviewSlot {
    pub id: WindowId,
    /// Target rectangle for the preview, in the same global logical
    /// coordinates the rest of the layout uses. Already scaled: this is
    /// exactly where and how big the preview goes.
    pub target: Rectangle,
    /// Scale applied to the window's frame to reach `target`. Uniform on
    /// both axes, so the preview keeps the window's aspect ratio.
    pub scale: f64,
    /// Which row this slot landed in, top to bottom. Used for keyboard
    /// navigation, which moves between rows rather than through a flat
    /// list.
    pub row: usize,
}

fn distribute(n: usize, rows: usize) -> Vec<usize> {
    let base = n / rows;
    let remainder = n % rows;
    (0..rows)
        .map(|i| base + usize::from(i < remainder))
        .collect()
}

fn layout_in_rows(
    windows: &[(WindowId, Size)],
    area: Rectangle,
    opts: &OverviewOptions,
    rows: usize,
) -> Option<(Vec<OverviewSlot>, i64)> {
    let counts = distribute(windows.len(), rows);

    let rows_i32 = i32::try_from(rows).unwrap_or(i32::MAX);
    let total_gap = opts.gap * (rows_i32 - 1);
    let row_height = ((area.size.height - total_gap) / rows_i32) - opts.label_height;
    if row_height <= 0 {
        return None;
    }

    let mut slots = Vec::with_capacity(windows.len());
    let mut covered: i64 = 0;
    let mut index = 0usize;
    let block_height = rows_i32 * (row_height + opts.label_height) + total_gap;
    let mut y = area.loc.y + (area.size.height - block_height) / 2;

    for (row, &count) in counts.iter().enumerate() {
        let row_windows = &windows[index..index + count];
        index += count;

        let count_i32 = i32::try_from(count).unwrap_or(i32::MAX);
        let row_gaps = opts.gap * (count_i32 - 1);
        let width_budget = area.size.width - row_gaps;

        let mut scales: Vec<f64> = row_windows
            .iter()
            .map(|(_, size)| {
                #[allow(clippy::cast_precision_loss)]
                let by_height = f64::from(row_height) / f64::from(size.height.max(1));
                by_height.min(opts.max_scale)
            })
            .collect();

        #[allow(clippy::cast_precision_loss)]
        let row_width: f64 = row_windows
            .iter()
            .zip(&scales)
            .map(|((_, size), scale)| f64::from(size.width) * scale)
            .sum();

        #[allow(clippy::cast_precision_loss)]
        let budget = f64::from(width_budget.max(1));
        if row_width > budget {
            let shrink = budget / row_width;
            for scale in &mut scales {
                *scale *= shrink;
            }
        }

        let sizes: Vec<Size> = row_windows
            .iter()
            .zip(&scales)
            .map(|((_, size), scale)| {
                #[allow(
                    clippy::cast_precision_loss,
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss
                )]
                Size::new(
                    ((f64::from(size.width) * scale) as i32).max(1),
                    ((f64::from(size.height) * scale) as i32).max(1),
                )
            })
            .collect();

        let used: i32 = sizes.iter().map(|s| s.width).sum::<i32>() + row_gaps;
        if used > area.size.width {
            return None;
        }
        let mut x = area.loc.x + (area.size.width - used) / 2;

        for ((&(id, _), &scale), &size) in row_windows.iter().zip(&scales).zip(&sizes) {
            let slot_y = y + (row_height - size.height);

            covered += i64::from(size.width) * i64::from(size.height);
            slots.push(OverviewSlot {
                id,
                target: Rectangle::new(Point::new(x, slot_y), size),
                scale,
                row,
            });

            x += size.width + opts.gap;
        }

        y += row_height + opts.label_height + opts.gap;
    }

    Some((slots, covered))
}

#[must_use]
pub fn compute_overview_layout(
    windows: &[(WindowId, Size)],
    area: Rectangle,
    opts: &OverviewOptions,
) -> Vec<OverviewSlot> {
    let windows: Vec<(WindowId, Size)> = windows
        .iter()
        .copied()
        .filter(|(_, size)| !size.is_empty())
        .collect();

    if windows.is_empty() || area.size.is_empty() {
        return Vec::new();
    }

    let mut best: Option<(Vec<OverviewSlot>, i64)> = None;
    for rows in 1..=windows.len() {
        let Some(candidate) = layout_in_rows(&windows, area, opts, rows) else {
            continue;
        };
        if best
            .as_ref()
            .is_none_or(|(_, best_covered)| candidate.1 > *best_covered)
        {
            best = Some(candidate);
        }
    }

    best.map(|(slots, _)| slots).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area() -> Rectangle {
        Rectangle::from_loc_and_size(0, 0, 1920, 1080)
    }

    /// Options without a label strip, so expected numbers stay readable.
    fn opts() -> OverviewOptions {
        OverviewOptions {
            gap: 20,
            label_height: 0,
            max_scale: 1.0,
        }
    }

    fn win(w: i32, h: i32) -> (WindowId, Size) {
        (WindowId::next(), Size::new(w, h))
    }

    #[test]
    fn empty_input_produces_no_slots() {
        assert!(compute_overview_layout(&[], area(), &opts()).is_empty());
    }

    #[test]
    fn zero_sized_area_produces_no_slots() {
        let windows = [win(800, 600)];
        let empty = Rectangle::from_loc_and_size(0, 0, 0, 0);
        assert!(compute_overview_layout(&windows, empty, &opts()).is_empty());
    }

    #[test]
    fn zero_sized_windows_are_skipped() {
        let windows = [(WindowId::next(), Size::ZERO), win(800, 600)];
        let slots = compute_overview_layout(&windows, area(), &opts());
        assert_eq!(slots.len(), 1, "the empty window has nothing to scale");
        assert_eq!(slots[0].id, windows[1].0);
    }

    #[test]
    fn single_window_is_centred() {
        let windows = [win(800, 600)];
        let slots = compute_overview_layout(&windows, area(), &opts());

        assert_eq!(slots.len(), 1);
        let target = slots[0].target;
        // Horizontally centred within the area.
        let left_margin = target.loc.x - area().loc.x;
        let right_margin = (area().loc.x + area().size.width) - (target.loc.x + target.size.width);
        assert!(
            (left_margin - right_margin).abs() <= 1,
            "expected centring, got margins {left_margin} / {right_margin}"
        );
    }

    #[test]
    fn preview_never_exceeds_max_scale() {
        // A tiny window in a huge area would otherwise be blown up and look
        // blurry; overview previews are never larger than the window.
        let windows = [win(320, 240)];
        let slots = compute_overview_layout(&windows, area(), &opts());

        assert!(slots[0].scale <= 1.0);
        assert_eq!(slots[0].target.size, Size::new(320, 240));
    }

    #[test]
    fn max_scale_is_configurable() {
        let windows = [win(320, 240)];
        let capped = OverviewOptions {
            max_scale: 0.5,
            ..opts()
        };
        let slots = compute_overview_layout(&windows, area(), &capped);

        assert!(slots[0].scale <= 0.5 + f64::EPSILON);
        assert_eq!(slots[0].target.size, Size::new(160, 120));
    }

    #[test]
    fn aspect_ratio_is_preserved() {
        // The whole point of the row model: a preview is the window,
        // smaller — never stretched to fill a cell.
        let windows = [win(1600, 400), win(400, 1200)];
        let slots = compute_overview_layout(&windows, area(), &opts());

        for (slot, (_, original)) in slots.iter().zip(&windows) {
            #[allow(clippy::cast_precision_loss)]
            let want = f64::from(original.width) / f64::from(original.height);
            #[allow(clippy::cast_precision_loss)]
            let got = f64::from(slot.target.size.width) / f64::from(slot.target.size.height);
            assert!(
                (want - got).abs() < 0.02,
                "aspect drifted: wanted {want}, got {got}"
            );
        }
    }

    #[test]
    fn slots_never_overlap() {
        for n in 1..=9usize {
            let windows: Vec<_> = (0..n).map(|_| win(800, 600)).collect();
            let slots = compute_overview_layout(&windows, area(), &opts());
            assert_eq!(slots.len(), n);

            for (i, a) in slots.iter().enumerate() {
                for b in slots.iter().skip(i + 1) {
                    assert!(
                        !a.target.overlaps(b.target),
                        "n={n}: {:?} overlaps {:?}",
                        a.target,
                        b.target
                    );
                }
            }
        }
    }

    #[test]
    fn slots_stay_inside_the_area() {
        for n in 1..=9usize {
            let windows: Vec<_> = (0..n).map(|_| win(800, 600)).collect();
            let slots = compute_overview_layout(&windows, area(), &opts());

            for slot in &slots {
                let t = slot.target;
                assert!(t.loc.x >= area().loc.x, "n={n}: {t:?} escapes left");
                assert!(t.loc.y >= area().loc.y, "n={n}: {t:?} escapes top");
                assert!(
                    t.loc.x + t.size.width <= area().loc.x + area().size.width,
                    "n={n}: {t:?} escapes right"
                );
                assert!(
                    t.loc.y + t.size.height <= area().loc.y + area().size.height,
                    "n={n}: {t:?} escapes bottom"
                );
            }
        }
    }

    #[test]
    fn rows_are_horizontally_centred() {
        // A row that does not fill the width must be centred, not
        // left-aligned — the previous grid implementation left the last
        // row hugging the left edge.
        let windows: Vec<_> = (0..3).map(|_| win(800, 600)).collect();
        let slots = compute_overview_layout(&windows, area(), &opts());

        for row in 0..=slots.iter().map(|s| s.row).max().unwrap() {
            let in_row: Vec<_> = slots.iter().filter(|s| s.row == row).collect();
            let first = in_row.first().unwrap().target;
            let last = in_row.last().unwrap().target;

            let left = first.loc.x - area().loc.x;
            let right = (area().loc.x + area().size.width) - (last.loc.x + last.size.width);
            assert!(
                (left - right).abs() <= 1,
                "row {row} not centred: {left} vs {right}"
            );
        }
    }

    #[test]
    fn block_of_rows_is_vertically_centred() {
        let windows: Vec<_> = (0..4).map(|_| win(800, 600)).collect();
        let slots = compute_overview_layout(&windows, area(), &opts());

        let top = slots.iter().map(|s| s.target.loc.y).min().unwrap();
        let bottom = slots
            .iter()
            .map(|s| s.target.loc.y + s.target.size.height)
            .max()
            .unwrap();

        let above = top - area().loc.y;
        let below = (area().loc.y + area().size.height) - bottom;
        assert!(
            (above - below).abs() <= area().size.height / 10,
            "block not vertically centred: {above} above, {below} below"
        );
    }

    #[test]
    fn area_offset_is_respected() {
        // The work area does not start at the origin once a panel reserves
        // space; slots must follow it rather than assuming (0, 0).
        let windows = [win(800, 600)];
        let offset = Rectangle::from_loc_and_size(100, 40, 1920, 1000);
        let slots = compute_overview_layout(&windows, offset, &opts());

        let t = slots[0].target;
        assert!(t.loc.x >= 100 && t.loc.y >= 40, "{t:?} ignores the offset");
    }

    #[test]
    fn order_is_preserved() {
        let windows: Vec<_> = (0..5).map(|_| win(800, 600)).collect();
        let slots = compute_overview_layout(&windows, area(), &opts());

        let got: Vec<_> = slots.iter().map(|s| s.id).collect();
        let want: Vec<_> = windows.iter().map(|(id, _)| *id).collect();
        assert_eq!(got, want);
    }

    #[test]
    fn rows_are_assigned_top_to_bottom() {
        let windows: Vec<_> = (0..6).map(|_| win(800, 600)).collect();
        let slots = compute_overview_layout(&windows, area(), &opts());

        // Row indices must be non-decreasing through the slot list, and a
        // later row must sit lower on screen.
        for pair in slots.windows(2) {
            assert!(pair[0].row <= pair[1].row);
            if pair[0].row < pair[1].row {
                assert!(pair[0].target.loc.y < pair[1].target.loc.y);
            }
        }
    }

    #[test]
    fn earlier_rows_take_the_remainder() {
        assert_eq!(distribute(5, 2), vec![3, 2]);
        assert_eq!(distribute(7, 3), vec![3, 2, 2]);
        assert_eq!(distribute(4, 2), vec![2, 2]);
        assert_eq!(distribute(1, 1), vec![1]);
    }

    fn rows_used(slots: &[OverviewSlot]) -> usize {
        slots.iter().map(|s| s.row).max().map_or(0, |max| max + 1)
    }

    #[test]
    fn row_count_follows_the_area_aspect() {
        let windows: Vec<_> = (0..8).map(|_| win(800, 600)).collect();
        let wide = compute_overview_layout(
            &windows,
            Rectangle::from_loc_and_size(0, 0, 3840, 1080),
            &opts(),
        );
        let tall = compute_overview_layout(
            &windows,
            Rectangle::from_loc_and_size(0, 0, 1080, 3840),
            &opts(),
        );
        assert!(rows_used(&wide) < rows_used(&tall));
    }

    #[test]
    fn row_count_is_never_more_than_the_window_count() {
        for n in 1..=4usize {
            let windows: Vec<_> = (0..n).map(|_| win(800, 600)).collect();
            let slots = compute_overview_layout(
                &windows,
                Rectangle::from_loc_and_size(0, 0, 100, 4000),
                &opts(),
            );
            assert!(
                rows_used(&slots) <= n,
                "n={n} produced {} rows",
                rows_used(&slots)
            );
        }
    }

    #[test]
    fn multiple_windows_use_more_than_one_row_when_that_makes_them_bigger() {
        // Three 800x600 windows on a 1080p screen fit in one row only by
        // shrinking hard to clear the width budget; two rows leaves each
        // preview visibly larger, so that is what must be chosen.
        let windows: Vec<_> = (0..3).map(|_| win(800, 600)).collect();
        let slots = compute_overview_layout(&windows, area(), &opts());
        assert!(rows_used(&slots) > 1, "expected more than one row");
    }

    #[test]
    fn chosen_layout_covers_at_least_as_much_area_as_any_other_row_count() {
        for n in 1..=9usize {
            let windows: Vec<_> = (0..n).map(|_| win(800, 600)).collect();
            let chosen = compute_overview_layout(&windows, area(), &opts());
            let chosen_area: i64 = chosen
                .iter()
                .map(|s| i64::from(s.target.size.width) * i64::from(s.target.size.height))
                .sum();

            for rows in 1..=n {
                if let Some((_, covered)) = layout_in_rows(&windows, area(), &opts(), rows) {
                    assert!(
                        chosen_area >= covered,
                        "n={n}: {rows} rows covers {covered} > chosen {chosen_area}"
                    );
                }
            }
        }
    }

    #[test]
    fn area_too_short_for_a_row_produces_no_slots() {
        // Better to draw nothing than to emit inverted rectangles.
        let windows = [win(800, 600)];
        let sliver = Rectangle::from_loc_and_size(0, 0, 1920, 4);
        let with_label = OverviewOptions {
            label_height: 24,
            ..opts()
        };
        assert!(compute_overview_layout(&windows, sliver, &with_label).is_empty());
    }

    #[test]
    fn a_wide_row_shrinks_to_fit() {
        // Eight wide windows in one row cannot all be scaled to the row
        // height; they must shrink together, keeping relative sizes.
        let windows: Vec<_> = (0..8).map(|_| win(1600, 400)).collect();
        let slots = compute_overview_layout(&windows, area(), &opts());

        for slot in &slots {
            assert!(slot.scale < 1.0, "expected shrink, got {}", slot.scale);
        }
        // All identical windows keep identical sizes after the shared
        // shrink factor.
        let first = slots[0].target.size;
        for slot in &slots {
            assert_eq!(slot.target.size, first);
        }
    }

    #[test]
    fn label_height_reserves_vertical_space() {
        // Compared with an uncapped scale so the previews are actually
        // height-limited: at max_scale 1.0 a single 800x600 window never
        // grows to the row height, so the label strip would not show.
        let windows = [win(800, 600)];
        let uncapped = OverviewOptions {
            max_scale: 4.0,
            ..opts()
        };
        let with_label = OverviewOptions {
            label_height: 200,
            ..uncapped
        };

        let without = compute_overview_layout(&windows, area(), &uncapped);
        let with = compute_overview_layout(&windows, area(), &with_label);

        assert!(
            with[0].target.size.height < without[0].target.size.height,
            "label strip did not reduce the preview height"
        );
    }
}

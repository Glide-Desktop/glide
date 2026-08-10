use crate::ids::WindowId;
use glide_geom::{Point, Rectangle, Size};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OverviewOptions {
    pub gap: i32,
    pub label_height: i32,
    /// Signed offset from a preview's bottom edge to the top edge of its
    /// label pill. **Normally negative**: per the design spec the pill
    /// rides up *onto* the preview rather than hanging below it, so only
    /// `label_height + label_offset` of vertical space is actually
    /// consumed underneath each row.
    ///
    /// Must match whatever offset the renderer actually draws the label
    /// at (see `glide-compositor`'s `overview::label_offset`), or the
    /// packed layout will disagree with what's on screen and a
    /// bottom-row pill can spill past the edge of the layout area.
    pub label_offset: i32,
    /// Minimum breathing room between the packed block of previews and
    /// every edge of the layout area, in logical px. Without it a block
    /// that happens to consume the full available height (easy to hit —
    /// the row-height budget is derived *from* that height) ends up
    /// flush against the screen edges, with the top row's titlebar and
    /// the bottom row's label pill touching them.
    pub padding: i32,
    pub max_scale: f64,
}

impl Default for OverviewOptions {
    fn default() -> Self {
        Self {
            gap: 32,
            label_height: 24,
            label_offset: 0,
            padding: 0,
            max_scale: 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OverviewSlot {
    pub id: WindowId,
    pub target: Rectangle,
    pub scale: f64,
    pub row: usize,
}

fn distribute(n: usize, rows: usize) -> Vec<usize> {
    let base = n / rows;
    let remainder = n % rows;
    (0..rows)
        .map(|i| base + usize::from(i < remainder))
        .collect()
}

/// One row's sized-but-not-yet-positioned previews, produced by
/// `layout_in_rows`' first pass.
struct RowPlan {
    scales: Vec<f64>,
    sizes: Vec<Size>,
    /// Tallest preview in the row — the row's real height, which is
    /// generally less than the row-height *budget*.
    height: i32,
    /// Total width including inter-preview gaps.
    used_width: i32,
}

fn layout_in_rows(
    windows: &[(WindowId, Size)],
    area: Rectangle,
    opts: &OverviewOptions,
    rows: usize,
) -> Option<(Vec<OverviewSlot>, i64)> {
    let counts = distribute(windows.len(), rows);

    // Vertical space a row's label actually consumes *below* the preview:
    // the pill's height minus however much of it overlaps back onto the
    // preview (`label_offset` is normally negative — see its doc comment).
    // Clamped at zero: a pill fully inside the preview costs nothing extra.
    let label_strip = (opts.label_height + opts.label_offset).max(0);

    let rows_i32 = i32::try_from(rows).unwrap_or(i32::MAX);
    let total_gap = opts.gap * (rows_i32 - 1);
    let row_height = ((area.size.height - total_gap) / rows_i32) - label_strip;
    if row_height <= 0 {
        return None;
    }

    // ---- Pass 1: size every row, without committing to any position ----
    //
    // Positions can't be assigned yet: a row's *actual* height is the
    // tallest preview in it, which is generally less than the `row_height`
    // budget (previews are capped by `max_scale` and by the row's width).
    // Centring against the budget instead of the real height is what used
    // to leave the whole block hugging one edge of the screen.
    let mut plans: Vec<RowPlan> = Vec::with_capacity(rows);
    let mut index = 0usize;

    for &count in &counts {
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

        let used_width: i32 = sizes.iter().map(|s| s.width).sum::<i32>() + row_gaps;
        if used_width > area.size.width {
            return None;
        }
        let height = sizes.iter().map(|s| s.height).max().unwrap_or(0);

        plans.push(RowPlan {
            scales,
            sizes,
            height,
            used_width,
        });
    }

    // ---- Pass 2: centre the block, then centre within each row ----
    let block_height: i32 = plans
        .iter()
        .map(|plan| plan.height + label_strip)
        .sum::<i32>()
        + total_gap;
    let mut y = area.loc.y + (area.size.height - block_height) / 2;

    let mut slots = Vec::with_capacity(windows.len());
    let mut covered: i64 = 0;
    let mut index = 0usize;

    for (row, (&count, plan)) in counts.iter().zip(&plans).enumerate() {
        let row_windows = &windows[index..index + count];
        index += count;

        let mut x = area.loc.x + (area.size.width - plan.used_width) / 2;

        for ((&(id, _), &scale), &size) in row_windows.iter().zip(&plan.scales).zip(&plan.sizes) {
            // Centred against the row's real height, so a short preview
            // sits mid-row rather than being pinned to its bottom edge.
            let slot_y = y + (plan.height - size.height) / 2;

            covered += i64::from(size.width) * i64::from(size.height);
            slots.push(OverviewSlot {
                id,
                target: Rectangle::new(Point::new(x, slot_y), size),
                scale,
                row,
            });

            x += size.width + opts.gap;
        }

        y += plan.height + label_strip + opts.gap;
    }

    Some((slots, covered))
}

/// Below this many windows the overview always uses a single row: one or
/// two previews sharing the full width, vertically centred. Wrapping only
/// starts once there are enough previews for a second row to actually buy
/// meaningful size back.
const MIN_WINDOWS_FOR_MULTIPLE_ROWS: usize = 3;

/// Shrinks `area` by `padding` on every side, refusing to eat more than a
/// quarter of either dimension so an oversized configured value degrades
/// gracefully on small outputs instead of collapsing the area to nothing.
fn apply_padding(area: Rectangle, padding: i32) -> Rectangle {
    let padding = padding.max(0);
    let pad_x = padding.min(area.size.width / 4);
    let pad_y = padding.min(area.size.height / 4);
    area.inset(glide_geom::Insets {
        top: pad_y,
        bottom: pad_y,
        left: pad_x,
        right: pad_x,
    })
}

/// The most rows worth considering for `count` windows.
fn max_rows_for(count: usize) -> usize {
    if count < MIN_WINDOWS_FOR_MULTIPLE_ROWS {
        1
    } else {
        count
    }
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

    let area = apply_padding(area, opts.padding);
    if area.size.is_empty() {
        return Vec::new();
    }

    let mut best: Option<(Vec<OverviewSlot>, i64)> = None;
    for rows in 1..=max_rows_for(windows.len()) {
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

    fn opts() -> OverviewOptions {
        OverviewOptions {
            gap: 20,
            label_height: 0,
            label_offset: 0,
            // Most geometry tests assert exact edge-to-edge centring, so
            // they opt out of the outer padding; it gets its own tests.
            padding: 0,
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
        let left_margin = target.loc.x - area().loc.x;
        let right_margin = (area().loc.x + area().size.width) - (target.loc.x + target.size.width);
        assert!(
            (left_margin - right_margin).abs() <= 1,
            "expected centring, got margins {left_margin} / {right_margin}"
        );
    }

    #[test]
    fn preview_never_exceeds_max_scale() {
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

    /// Vertical margins above and below the whole block of previews.
    fn vertical_margins(
        slots: &[OverviewSlot],
        area: Rectangle,
        opts: &OverviewOptions,
    ) -> (i32, i32) {
        let top = slots.iter().map(|s| s.target.loc.y).min().unwrap();
        let bottom = slots
            .iter()
            .map(|s| s.target.loc.y + s.target.size.height)
            .max()
            .unwrap();
        // The label strip hangs below the bottom row and is part of the
        // centred block, so it counts against the lower margin.
        let label_strip = (opts.label_height + opts.label_offset).max(0);
        (
            top - area.loc.y,
            (area.loc.y + area.size.height) - (bottom + label_strip),
        )
    }

    #[test]
    fn block_of_rows_is_vertically_centred() {
        let windows: Vec<_> = (0..4).map(|_| win(800, 600)).collect();
        let slots = compute_overview_layout(&windows, area(), &opts());

        let (above, below) = vertical_margins(&slots, area(), &opts());
        assert!(
            (above - below).abs() <= 2,
            "block not vertically centred: {above} above, {below} below"
        );
    }

    /// Regression test for the real-world bug: with a single row the row
    /// budget spans the entire area, and previews used to be pinned to the
    /// *bottom* of that budget — so a lone window sat at the very bottom of
    /// the screen with all the slack above it.
    #[test]
    fn a_single_row_is_vertically_centred_not_bottom_aligned() {
        for n in 1..=2usize {
            let windows: Vec<_> = (0..n).map(|_| win(800, 600)).collect();
            let slots = compute_overview_layout(&windows, area(), &opts());

            let (above, below) = vertical_margins(&slots, area(), &opts());
            assert!(
                (above - below).abs() <= 2,
                "n={n}: single row not centred: {above} above, {below} below"
            );
            assert!(
                above > 0,
                "n={n}: expected real slack above the row, got {above}"
            );
        }
    }

    /// Previews shorter than their row must sit mid-row, not on its floor.
    #[test]
    fn previews_are_centred_within_their_row() {
        // A short window beside a tall one: same row, different heights.
        let short = win(400, 200);
        let tall = win(400, 800);
        let windows = [short, tall];
        let slots = compute_overview_layout(&windows, area(), &opts());

        let short_slot = slots.iter().find(|s| s.id == short.0).unwrap().target;
        let tall_slot = slots.iter().find(|s| s.id == tall.0).unwrap().target;

        let short_centre = short_slot.loc.y + short_slot.size.height / 2;
        let tall_centre = tall_slot.loc.y + tall_slot.size.height / 2;
        assert!(
            (short_centre - tall_centre).abs() <= 2,
            "previews in a row must share a centre line: {short_centre} vs {tall_centre}"
        );
    }

    #[test]
    fn fewer_than_three_windows_always_use_a_single_row() {
        for n in 1..MIN_WINDOWS_FOR_MULTIPLE_ROWS {
            let windows: Vec<_> = (0..n).map(|_| win(800, 600)).collect();
            // Even on a tall, narrow area where wrapping would otherwise
            // win on coverage.
            let tall = Rectangle::from_loc_and_size(0, 0, 900, 2400);
            let slots = compute_overview_layout(&windows, tall, &opts());
            assert_eq!(rows_used(&slots), 1, "n={n} should stay on one row");
        }
    }

    /// Regression test: the row-height budget is derived from the area
    /// height, so a packed block routinely consumes *all* of it — leaving
    /// the top row's titlebar and the bottom row's label pill flush
    /// against the screen edges. `padding` must guarantee breathing room
    /// on every side.
    #[test]
    fn padding_keeps_the_block_clear_of_every_edge() {
        let pad = 64;
        let padded = OverviewOptions {
            padding: pad,
            label_height: 28,
            label_offset: -14,
            ..opts()
        };

        // Several counts, so both the single-row and the wrapped cases are
        // covered — three windows is what reproduced this on screen.
        for n in 1..=5usize {
            let windows: Vec<_> = (0..n).map(|_| win(800, 600)).collect();
            let slots = compute_overview_layout(&windows, area(), &padded);
            assert!(!slots.is_empty(), "n={n} produced no slots");

            let label_strip = (padded.label_height + padded.label_offset).max(0);
            let top = slots.iter().map(|s| s.target.loc.y).min().unwrap();
            let bottom = slots
                .iter()
                .map(|s| s.target.loc.y + s.target.size.height)
                .max()
                .unwrap()
                + label_strip;
            let left = slots.iter().map(|s| s.target.loc.x).min().unwrap();
            let right = slots
                .iter()
                .map(|s| s.target.loc.x + s.target.size.width)
                .max()
                .unwrap();

            let a = area();
            assert!(
                top - a.loc.y >= pad,
                "n={n}: only {} px above the block",
                top - a.loc.y
            );
            assert!(
                (a.loc.y + a.size.height) - bottom >= pad,
                "n={n}: only {} px below the block (label included)",
                (a.loc.y + a.size.height) - bottom
            );
            assert!(
                left - a.loc.x >= pad,
                "n={n}: only {} px left of the block",
                left - a.loc.x
            );
            assert!(
                (a.loc.x + a.size.width) - right >= pad,
                "n={n}: only {} px right of the block",
                (a.loc.x + a.size.width) - right
            );
        }
    }

    #[test]
    fn padding_still_leaves_the_block_centred() {
        let padded = OverviewOptions {
            padding: 64,
            ..opts()
        };
        let windows: Vec<_> = (0..4).map(|_| win(800, 600)).collect();
        let slots = compute_overview_layout(&windows, area(), &padded);

        let (above, below) = vertical_margins(&slots, area(), &padded);
        assert!(
            (above - below).abs() <= 2,
            "padding broke centring: {above} above, {below} below"
        );
    }

    #[test]
    fn oversized_padding_degrades_instead_of_collapsing_the_area() {
        let absurd = OverviewOptions {
            padding: 10_000,
            ..opts()
        };
        let windows = [win(800, 600)];
        let slots = compute_overview_layout(&windows, area(), &absurd);
        assert_eq!(
            slots.len(),
            1,
            "an absurd padding must still lay the window out somewhere"
        );

        // Clamped to a quarter per side, i.e. half the area survives.
        let t = slots[0].target;
        assert!(t.size.width > 0 && t.size.height > 0);
        assert!(t.loc.x >= area().size.width / 4);
        assert!(t.loc.y >= area().size.height / 4);
    }

    #[test]
    fn negative_padding_is_treated_as_none() {
        let negative = OverviewOptions {
            padding: -50,
            ..opts()
        };
        let windows = [win(800, 600)];
        let with_negative = compute_overview_layout(&windows, area(), &negative);
        let without = compute_overview_layout(&windows, area(), &opts());
        assert_eq!(with_negative[0].target, without[0].target);
    }

    #[test]
    fn three_or_more_windows_may_wrap_onto_several_rows() {
        let windows: Vec<_> = (0..MIN_WINDOWS_FOR_MULTIPLE_ROWS)
            .map(|_| win(800, 600))
            .collect();
        let slots = compute_overview_layout(&windows, area(), &opts());
        assert!(
            rows_used(&slots) > 1,
            "three windows should be allowed to wrap"
        );
    }

    #[test]
    fn area_offset_is_respected() {
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

            // Only among the row counts the layout is actually allowed to
            // pick from — under `MIN_WINDOWS_FOR_MULTIPLE_ROWS` a single
            // row is mandated even if wrapping would cover more.
            for rows in 1..=max_rows_for(n) {
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
        let windows: Vec<_> = (0..8).map(|_| win(1600, 400)).collect();
        let slots = compute_overview_layout(&windows, area(), &opts());

        for slot in &slots {
            assert!(slot.scale < 1.0, "expected shrink, got {}", slot.scale);
        }
        let first = slots[0].target.size;
        for slot in &slots {
            assert_eq!(slot.target.size, first);
        }
    }

    #[test]
    fn label_height_reserves_vertical_space() {
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

    /// Regression test: `label_offset` used to have no counterpart at all
    /// in the layout budget — only `label_height` was reserved — so a
    /// bottom-row preview's label pill could spill past the bottom of the
    /// layout area whenever the block's natural centring margin was
    /// smaller than the renderer's actual offset.
    #[test]
    fn a_positive_label_offset_reserves_space_on_top_of_label_height() {
        let windows = [win(800, 600)];
        let uncapped = OverviewOptions {
            max_scale: 4.0,
            ..opts()
        };
        let pushed_down = OverviewOptions {
            label_offset: 200,
            ..uncapped
        };

        let without = compute_overview_layout(&windows, area(), &uncapped);
        let with = compute_overview_layout(&windows, area(), &pushed_down);

        assert!(
            with[0].target.size.height < without[0].target.size.height,
            "label_offset did not reduce the preview height"
        );
    }

    /// The design case: a negative offset overlaps the pill back onto the
    /// preview, so it consumes *less* room below the row than its full
    /// height — previews get correspondingly larger.
    #[test]
    fn a_negative_label_offset_gives_the_row_height_back() {
        let windows = [win(800, 600)];
        let base = OverviewOptions {
            max_scale: 4.0,
            label_height: 200,
            label_offset: 0,
            ..opts()
        };
        let overlapping = OverviewOptions {
            label_offset: -100,
            ..base
        };

        let flush = compute_overview_layout(&windows, area(), &base);
        let overlapped = compute_overview_layout(&windows, area(), &overlapping);

        assert!(
            overlapped[0].target.size.height > flush[0].target.size.height,
            "overlapping the pill onto the preview must free up row height"
        );
    }

    /// A pill fully inside the preview costs no extra vertical space —
    /// and must never be counted as *negative* space either.
    #[test]
    fn an_offset_that_fully_overlaps_the_pill_costs_no_extra_space() {
        let windows = [win(800, 600)];
        let none = OverviewOptions {
            max_scale: 4.0,
            label_height: 0,
            label_offset: 0,
            ..opts()
        };
        let fully_inside = OverviewOptions {
            label_height: 40,
            label_offset: -40,
            ..none
        };
        let past_inside = OverviewOptions {
            label_offset: -100,
            ..fully_inside
        };

        let baseline = compute_overview_layout(&windows, area(), &none);
        let inside = compute_overview_layout(&windows, area(), &fully_inside);
        let past = compute_overview_layout(&windows, area(), &past_inside);

        assert_eq!(inside[0].target.size, baseline[0].target.size);
        assert_eq!(
            past[0].target.size, baseline[0].target.size,
            "over-overlapping must clamp, not hand back phantom height"
        );
    }
}

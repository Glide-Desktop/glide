use crate::ids::WindowId;
use glide_geom::{Point, Rectangle, Size};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OverviewOptions {
    pub gap: i32,
    pub label_height: i32,
    pub label_offset: i32,
    pub padding: i32,
    pub max_scale: f64,
}

impl Default for OverviewOptions {
    fn default() -> Self {
        Self { gap: 32, label_height: 24, label_offset: 0, padding: 0, max_scale: 1.0 }
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
    (0..rows).map(|i| base + usize::from(i < remainder)).collect()
}

struct RowPlan {
    scales: Vec<f64>,
    sizes: Vec<Size>,
    height: i32,
    used_width: i32,
}

fn layout_in_rows(
    windows: &[(WindowId, Size)], area: Rectangle, opts: &OverviewOptions, rows: usize,
) -> Option<(Vec<OverviewSlot>, i64)> {
    let counts = distribute(windows.len(), rows);

    let label_strip = opts.label_height.checked_add(opts.label_offset)?.max(0);

    let rows_i32 = i32::try_from(rows).unwrap_or(i32::MAX);
    let total_gap = opts.gap.checked_mul(rows_i32 - 1)?;
    let row_height = (area.size.height.checked_sub(total_gap)? / rows_i32).checked_sub(label_strip)?;
    if row_height <= 0 {
        return None;
    }

    let mut plans: Vec<RowPlan> = Vec::with_capacity(rows);
    let mut index = 0usize;

    for &count in &counts {
        let row_windows = &windows[index..index + count];
        index += count;

        let count_i32 = i32::try_from(count).unwrap_or(i32::MAX);
        let row_gaps = opts.gap.checked_mul(count_i32 - 1)?;
        let width_budget = area.size.width.checked_sub(row_gaps)?;
        if width_budget <= 0 {
            return None;
        }

        let mut scales: Vec<f64> = row_windows
            .iter()
            .map(|(_, size)| {
                let by_height = f64::from(row_height) / f64::from(size.height.max(1));
                by_height.min(opts.max_scale)
            })
            .collect();

        let row_width: f64 =
            row_windows.iter().zip(&scales).map(|((_, size), scale)| f64::from(size.width) * scale).sum();

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
                Size::new(
                    ((f64::from(size.width) * scale) as i32).max(1),
                    ((f64::from(size.height) * scale) as i32).max(1),
                )
            })
            .collect();

        let used_width = sizes.iter().try_fold(row_gaps, |total, size| total.checked_add(size.width))?;
        if used_width > area.size.width {
            return None;
        }
        let height = sizes.iter().map(|s| s.height).max().unwrap_or(0);

        plans.push(RowPlan { scales, sizes, height, used_width });
    }

    let block_height =
        plans.iter().try_fold(total_gap, |total, plan| total.checked_add(plan.height)?.checked_add(label_strip))?;
    let mut y = area.loc.y.saturating_add((area.size.height - block_height) / 2);

    let mut slots = Vec::with_capacity(windows.len());
    let mut covered: i64 = 0;
    let mut index = 0usize;

    for (row, (&count, plan)) in counts.iter().zip(&plans).enumerate() {
        let row_windows = &windows[index..index + count];
        index += count;

        let mut x = area.loc.x.saturating_add((area.size.width - plan.used_width) / 2);

        for ((&(id, _), &scale), &size) in row_windows.iter().zip(&plan.scales).zip(&plan.sizes) {
            let slot_y = y.saturating_add((plan.height - size.height) / 2);

            covered += i64::from(size.width) * i64::from(size.height);
            slots.push(OverviewSlot { id, target: Rectangle::new(Point::new(x, slot_y), size), scale, row });

            x = x.saturating_add(size.width).saturating_add(opts.gap);
        }

        y = y.saturating_add(plan.height).saturating_add(label_strip).saturating_add(opts.gap);
    }

    Some((slots, covered))
}

const MIN_WINDOWS_FOR_MULTIPLE_ROWS: usize = 3;

fn apply_padding(area: Rectangle, padding: i32) -> Rectangle {
    let padding = padding.max(0);
    let pad_x = padding.min(area.size.width / 4);
    let pad_y = padding.min(area.size.height / 4);
    area.inset(glide_geom::Insets { top: pad_y, bottom: pad_y, left: pad_x, right: pad_x })
}

fn max_rows_for(count: usize, area: Rectangle) -> usize {
    let portrait = area.size.height > area.size.width;
    if count < MIN_WINDOWS_FOR_MULTIPLE_ROWS && !portrait { 1 } else { count }
}

#[must_use]
pub fn compute_overview_layout(
    windows: &[(WindowId, Size)], area: Rectangle, opts: &OverviewOptions,
) -> Vec<OverviewSlot> {
    let windows: Vec<(WindowId, Size)> = windows.iter().copied().filter(|(_, size)| !size.is_empty()).collect();

    if windows.is_empty() || area.size.is_empty() || !opts.max_scale.is_finite() || opts.max_scale <= 0.0 {
        return Vec::new();
    }

    let area = apply_padding(area, opts.padding);
    if area.size.is_empty() {
        return Vec::new();
    }

    let opts = OverviewOptions { gap: opts.gap.max(0), ..*opts };
    let mut best: Option<(Vec<OverviewSlot>, i64)> = None;
    for rows in 1..=max_rows_for(windows.len(), area) {
        let Some(candidate) = layout_in_rows(&windows, area, &opts, rows) else {
            continue;
        };
        if best.as_ref().is_none_or(|(_, best_covered)| candidate.1 > *best_covered) {
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
        OverviewOptions { gap: 20, label_height: 0, label_offset: 0, padding: 0, max_scale: 1.0 }
    }

    fn win(w: i32, h: i32) -> (WindowId, Size) {
        (WindowId::next(), Size::new(w, h))
    }

    fn same(n: usize) -> Vec<(WindowId, Size)> {
        (0..n).map(|_| win(800, 600)).collect()
    }

    fn rows_used(slots: &[OverviewSlot]) -> usize {
        slots.iter().map(|s| s.row).max().map_or(0, |max| max + 1)
    }

    fn bounds(slots: &[OverviewSlot], label_strip: i32) -> (i32, i32, i32, i32) {
        let top = slots.iter().map(|s| s.target.loc.y).min().unwrap();
        let bottom = slots.iter().map(|s| s.target.loc.y + s.target.size.height).max().unwrap() + label_strip;
        let left = slots.iter().map(|s| s.target.loc.x).min().unwrap();
        let right = slots.iter().map(|s| s.target.loc.x + s.target.size.width).max().unwrap();
        (top, bottom, left, right)
    }

    fn height_with(opts: OverviewOptions) -> i32 {
        compute_overview_layout(&[win(800, 600)], area(), &opts)[0].target.size.height
    }

    #[test]
    fn degenerate_inputs_produce_no_slots() {
        assert!(compute_overview_layout(&[], area(), &opts()).is_empty());
        let empty = Rectangle::from_loc_and_size(0, 0, 0, 0);
        assert!(compute_overview_layout(&same(1), empty, &opts()).is_empty());
        let sliver = Rectangle::from_loc_and_size(0, 0, 1920, 4);
        let labelled = OverviewOptions { label_height: 24, ..opts() };
        assert!(compute_overview_layout(&same(1), sliver, &labelled).is_empty());

        let windows = [(WindowId::next(), Size::ZERO), win(800, 600)];
        let slots = compute_overview_layout(&windows, area(), &opts());
        assert_eq!(slots.len(), 1);
        assert_eq!(slots[0].id, windows[1].0);
    }

    #[test]
    fn invalid_scales_and_overflowing_label_strips_produce_no_slots() {
        for max_scale in [f64::NAN, f64::INFINITY, 0.0, -1.0] {
            assert!(compute_overview_layout(&same(2), area(), &OverviewOptions { max_scale, ..opts() }).is_empty());
        }
        let extreme = OverviewOptions { label_height: i32::MAX, label_offset: 1, ..opts() };
        assert!(compute_overview_layout(&same(2), area(), &extreme).is_empty());
        assert!(compute_overview_layout(&same(2), area(), &OverviewOptions { gap: i32::MAX, ..opts() }).is_empty());
        let negative_gap = OverviewOptions { gap: -100, ..opts() };
        let slots = compute_overview_layout(&same(2), area(), &negative_gap);
        assert_eq!(slots.len(), 2);
        assert!(!slots[0].target.overlaps(slots[1].target));
    }

    #[test]
    fn slots_keep_order_stay_inside_never_overlap_and_are_centred() {
        for n in 1..=9usize {
            let windows = same(n);
            let slots = compute_overview_layout(&windows, area(), &opts());
            let ids: Vec<_> = slots.iter().map(|s| s.id).collect();
            let want: Vec<_> = windows.iter().map(|(id, _)| *id).collect();
            assert_eq!(ids, want, "n={n}");

            for (i, a) in slots.iter().enumerate() {
                let t = a.target;
                assert!(t.loc.x >= 0 && t.loc.y >= 0, "n={n}: {t:?}");
                assert!(t.loc.x + t.size.width <= 1920 && t.loc.y + t.size.height <= 1080);
                for b in slots.iter().skip(i + 1) {
                    assert!(!t.overlaps(b.target), "n={n}");
                    assert!(a.row <= b.row);
                    if a.row < b.row {
                        assert!(t.loc.y < b.target.loc.y);
                    }
                }
            }

            let (top, bottom, _, _) = bounds(&slots, 0);
            assert!((top - (1080 - bottom)).abs() <= 2, "n={n}: vertical");
            for row in 0..rows_used(&slots) {
                let in_row: Vec<_> = slots.iter().filter(|s| s.row == row).collect();
                let (first, last) = (in_row[0].target, in_row[in_row.len() - 1].target);
                let right = 1920 - (last.loc.x + last.size.width);
                assert!((first.loc.x - right).abs() <= 1, "n={n}: row {row}");
            }
        }
    }

    #[test]
    fn previews_keep_their_aspect_and_share_a_row_centre_line() {
        let windows = [win(1600, 400), win(400, 1200)];
        let slots = compute_overview_layout(&windows, area(), &opts());
        for (slot, (_, original)) in slots.iter().zip(&windows) {
            let want = f64::from(original.width) / f64::from(original.height);
            let got = f64::from(slot.target.size.width) / f64::from(slot.target.size.height);
            assert!((want - got).abs() < 0.02);
        }
        let centre = |s: &OverviewSlot| s.target.loc.y + s.target.size.height / 2;
        assert!((centre(&slots[0]) - centre(&slots[1])).abs() <= 2);

        let offset = Rectangle::from_loc_and_size(100, 40, 1920, 1000);
        let t = compute_overview_layout(&same(1), offset, &opts())[0].target;
        assert!(t.loc.x >= 100 && t.loc.y >= 40);
    }

    #[test]
    fn previews_never_exceed_max_scale_and_wide_rows_shrink_evenly() {
        let small = [win(320, 240)];
        let slots = compute_overview_layout(&small, area(), &opts());
        assert_eq!(slots[0].target.size, Size::new(320, 240));
        let capped = OverviewOptions { max_scale: 0.5, ..opts() };
        let slots = compute_overview_layout(&small, area(), &capped);
        assert_eq!(slots[0].target.size, Size::new(160, 120));

        let wide: Vec<_> = (0..8).map(|_| win(1600, 400)).collect();
        let slots = compute_overview_layout(&wide, area(), &opts());
        assert!(slots.iter().all(|s| s.scale < 1.0));
        assert!(slots.iter().all(|s| s.target.size == slots[0].target.size));
    }

    #[test]
    fn row_count_follows_window_count_and_area_aspect() {
        assert_eq!(distribute(7, 3), vec![3, 2, 2]);
        for n in 1..MIN_WINDOWS_FOR_MULTIPLE_ROWS {
            assert_eq!(rows_used(&compute_overview_layout(&same(n), area(), &opts())), 1, "n={n}");
        }
        let portrait = Rectangle::from_loc_and_size(0, 0, 900, 2400);
        assert_eq!(rows_used(&compute_overview_layout(&same(1), portrait, &opts())), 1);
        assert_eq!(
            rows_used(&compute_overview_layout(&same(2), portrait, &opts())),
            2,
            "two windows on a portrait output stack instead of shrinking side by side"
        );
        let three = compute_overview_layout(&same(MIN_WINDOWS_FOR_MULTIPLE_ROWS), area(), &opts());
        assert!(rows_used(&three) > 1);

        let windows = same(8);
        let wide = Rectangle::from_loc_and_size(0, 0, 3840, 1080);
        let tall = Rectangle::from_loc_and_size(0, 0, 1080, 3840);
        assert!(
            rows_used(&compute_overview_layout(&windows, wide, &opts()))
                < rows_used(&compute_overview_layout(&windows, tall, &opts()))
        );

        for n in 1..=9usize {
            let windows = same(n);
            let chosen: i64 = compute_overview_layout(&windows, area(), &opts())
                .iter()
                .map(|s| i64::from(s.target.size.width) * i64::from(s.target.size.height))
                .sum();
            for rows in 1..=max_rows_for(n, area()) {
                if let Some((_, covered)) = layout_in_rows(&windows, area(), &opts(), rows) {
                    assert!(chosen >= covered, "n={n}, rows={rows}");
                }
            }
        }
    }

    #[test]
    fn padding_clears_every_edge_and_degrades_gracefully() {
        let padded = OverviewOptions { padding: 64, label_height: 28, label_offset: -14, ..opts() };
        for n in 1..=5usize {
            let slots = compute_overview_layout(&same(n), area(), &padded);
            let (top, bottom, left, right) = bounds(&slots, 14);
            assert!(top >= 64 && 1080 - bottom >= 64 && left >= 64 && 1920 - right >= 64);
        }

        let absurd = OverviewOptions { padding: 10_000, ..opts() };
        let t = compute_overview_layout(&same(1), area(), &absurd)[0].target;
        assert!(t.loc.x >= 1920 / 4 && t.loc.y >= 1080 / 4 && !t.size.is_empty());

        let negative = OverviewOptions { padding: -50, ..opts() };
        assert_eq!(
            compute_overview_layout(&same(1), area(), &negative)[0].target.size,
            compute_overview_layout(&same(1), area(), &opts())[0].target.size
        );
    }

    #[test]
    fn the_label_strip_reserves_height_and_overlap_gives_it_back() {
        let base = OverviewOptions { max_scale: 4.0, ..opts() };
        let plain = height_with(base);
        let labelled = OverviewOptions { label_height: 200, ..base };
        assert!(height_with(labelled) < plain);
        assert!(height_with(OverviewOptions { label_offset: 200, ..base }) < plain);
        assert!(height_with(OverviewOptions { label_offset: -100, ..labelled }) > height_with(labelled));
        for offset in [-40, -100] {
            let inside = OverviewOptions { label_height: 40, label_offset: offset, ..base };
            assert_eq!(height_with(inside), plain);
        }
    }
}

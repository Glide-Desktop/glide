use super::*;

fn theme_with_duration(ms: u64) -> OverviewTheme {
    OverviewTheme { duration_ms: ms, ..OverviewTheme::default() }
}

use crate::raster::rect;

fn centre(rect: Rectangle<i32, Logical>) -> Point<f64, Logical> {
    Point::from((
        f64::from(rect.loc.x) + f64::from(rect.size.w) / 2.0,
        f64::from(rect.loc.y) + f64::from(rect.size.h) / 2.0,
    ))
}

fn ago(ms: u64) -> Instant {
    Instant::now()
        .checked_sub(Duration::from_millis(ms))
        .expect("the process has been up for longer than the test's offset")
}

fn closing_since(ms: u64) -> Phase {
    Phase::Closing { started: ago(ms), select: None, from: SceneFrom { primary: 1.0, backdrop: 1.0, release: None } }
}

const ONE: WorkspaceId = WorkspaceId(1);
const TWO: WorkspaceId = WorkspaceId(2);

fn place() -> Place {
    Place { output: rect(0, 0, 1920, 1080), area: rect(0, 34, 1920, 1046) }
}

fn entry(id: WindowId, frame: Rectangle<i32, Logical>, workspace: WorkspaceId) -> Entry {
    Entry { id, frame, workspace }
}

fn ui_with(entries: &[Entry], ms: u64, focused: Option<WindowId>, workspaces: &[WorkspaceId]) -> OverviewUi {
    OverviewUi::new(entries, place(), &theme_with_duration(ms), focused, (workspaces.to_vec(), workspaces[0], true))
}

fn ui_with_one_window(duration_ms: u64) -> (OverviewUi, WindowId) {
    let id = WindowId::next();
    let ui = ui_with(&[entry(id, rect(0, 0, 800, 600), ONE)], duration_ms, Some(id), &[ONE]);
    (ui, id)
}

fn order(ui: &OverviewUi) -> Vec<WindowId> {
    ui.paint_order().into_iter().map(|index| ui.previews[index].id).collect()
}

#[test]
fn the_focused_window_paints_above_neighbours_and_other_workspaces_paint_last() {
    let (focused, above, elsewhere) = (WindowId::next(), WindowId::next(), WindowId::next());
    let entries = [
        entry(focused, rect(100, 100, 800, 600), ONE),
        entry(above, rect(300, 200, 800, 600), ONE),
        entry(elsewhere, rect(100, 100, 800, 600), TWO),
    ];
    let mut ui = ui_with(&entries, 200, Some(focused), &[ONE, TWO]);
    assert_eq!(order(&ui), vec![focused, above, elsewhere]);
    ui.begin_close(Some(above));
    assert_eq!(order(&ui)[0], above);
}

mod scene_easing {
    use super::*;

    #[test]
    fn the_exit_is_front_loaded_just_like_the_entry() {
        let (mut ui, _id) = ui_with_one_window(400);
        ui.phase = closing_since(80);
        let remaining = ui.progress();
        assert!(remaining < 0.6, "the exit stalled: still {remaining} open a fifth of the way through");
    }

    #[test]
    fn closing_mirrors_opening_over_the_same_elapsed_time() {
        for millis in [40_u64, 120, 200, 320] {
            let (mut opening, _) = ui_with_one_window(400);
            opening.phase = Phase::opening(ago(millis));
            let (mut closing, _) = ui_with_one_window(400);
            closing.phase = closing_since(millis);
            let sum = opening.progress() + closing.progress();
            assert!(
                (sum - 1.0).abs() < 0.05,
                "at {millis}ms opening {} and closing {}",
                opening.progress(),
                closing.progress()
            );
        }
    }

    #[test]
    fn a_settled_overview_is_fully_progressed_on_both_curves() {
        let (ui, _id) = ui_with_one_window(0);
        assert!((ui.progress() - 1.0).abs() < f64::EPSILON);
        assert!((ui.backdrop_progress() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn previews_lead_the_backdrop_mid_transition() {
        let (mut ui, _id) = ui_with_one_window(400);
        ui.phase = Phase::opening(ago(80));
        let (previews, backdrop) = (ui.progress(), ui.backdrop_progress());
        assert!(previews > backdrop, "previews ({previews}) must lead the backdrop ({backdrop})");
        assert!((0.0..1.0).contains(&backdrop), "backdrop: {backdrop}");
    }
}

#[test]
fn the_grid_starts_under_a_strip_that_is_always_there() {
    let (ui, _id) = ui_with_one_window(0);
    let grid = ui.grid_area();
    assert!(grid.loc.y > place().area.loc.y + 72, "one workspace still reserves the strip: {grid:?}");
    let tile = ui.preview_rect(ui.previews[0].id).unwrap();
    assert!(tile.loc.y >= grid.loc.y);
}

#[test]
fn a_neighbour_workspace_is_a_page_one_stride_to_the_side() {
    let (here, there) = (WindowId::next(), WindowId::next());
    let entries = [entry(here, rect(100, 100, 800, 600), ONE), entry(there, rect(100, 100, 800, 600), TWO)];
    let ui = ui_with(&entries, 0, Some(here), &[ONE, TWO]);
    let (a, b) = (ui.preview_rect(here).unwrap(), ui.preview_rect(there).unwrap());
    assert_eq!(b.loc.x - a.loc.x, ui.stride().round() as i32, "same slot, one page over");
    let grid = ui.grid_area();
    let page_two = f64::from(grid.loc.x + ui.theme.padding) + ui.stride();
    assert_eq!(page_two.round() as i32, place().output.size.w - PEEK, "the next page starts {PEEK} px inside the edge");
}

#[test]
fn the_card_of_the_next_page_always_peeks_in_even_when_its_windows_do_not() {
    let (here, there) = (WindowId::next(), WindowId::next());
    let entries = [entry(here, rect(100, 100, 400, 300), ONE), entry(there, rect(100, 100, 400, 300), TWO)];
    let ui = ui_with(&entries, 0, Some(here), &[ONE, TWO]);
    let next = moved_by(ui.page_card(), ui.page_dx(1));
    let edge = f64::from(place().output.size.w);
    assert!(next.loc.x < edge - f64::from(PEEK) + 1.0 && next.loc.x > edge - 2.0 * f64::from(PEEK), "{next:?}");
    assert!(ui.preview_rect(there).unwrap().loc.x > 1920, "its single small window is off screen");
}

#[test]
fn switching_the_active_workspace_slides_the_pages_instead_of_jumping() {
    let (here, there) = (WindowId::next(), WindowId::next());
    let entries = [entry(here, rect(100, 100, 800, 600), ONE), entry(there, rect(100, 100, 800, 600), TWO)];
    let mut ui = ui_with(&entries, 0, Some(here), &[ONE, TWO]);
    let before = ui.preview_rect(there).unwrap();
    ui.set_workspaces(vec![ONE, TWO], TWO, true);
    if glide_theme::motion::is_disabled() {
        assert_eq!(ui.preview_rect(there).unwrap().loc.x, before.loc.x - ui.stride().round() as i32);
    } else {
        assert!(ui.is_animating(), "the scroll runs");
        assert_eq!(ui.preview_rect(there).unwrap(), before, "it starts where it was drawn");
    }
}

#[test]
fn a_sideways_swipe_moves_the_pages_with_the_finger_and_bands_at_the_ends() {
    let (here, there) = (WindowId::next(), WindowId::next());
    let entries = [entry(here, rect(100, 100, 800, 600), ONE), entry(there, rect(100, 100, 800, 600), TWO)];
    let mut ui = ui_with(&entries, 0, Some(here), &[ONE, TWO]);
    let start = ui.preview_rect(there).unwrap().loc.x;
    assert!(ui.scrub_pages(0.5));
    let moved = start - ui.preview_rect(there).unwrap().loc.x;
    assert!((f64::from(moved) - ui.stride() * 0.5).abs() <= 1.0, "half a swipe, half a page: {moved}");
    assert!(ui.scrub_pages(-0.5));
    let banded = ui.preview_rect(there).unwrap().loc.x - start;
    assert!(f64::from(banded) <= ui.stride() * RUBBER_BAND * 0.5 + 1.0, "nothing before the first page: {banded}");

    ui.release_scrub(0.0);
    ui.settle_scroll();
    if glide_theme::motion::is_disabled() {
        assert_eq!(ui.preview_rect(there).unwrap().loc.x, start, "a short swipe settles back");
    } else {
        assert!(ui.is_animating(), "it travels back instead of jumping");
    }
}

#[test]
fn a_released_swipe_that_switches_carries_on_from_where_the_finger_left() {
    let (here, there) = (WindowId::next(), WindowId::next());
    let entries = [entry(here, rect(100, 100, 800, 600), ONE), entry(there, rect(100, 100, 800, 600), TWO)];
    let mut ui = ui_with(&entries, 400, Some(here), &[ONE, TWO]);
    ui.scrub_pages(0.6);
    let held = ui.preview_rect(there).unwrap();
    ui.release_scrub(2.0);
    assert_eq!(ui.preview_rect(there).unwrap(), held, "letting go does not jump");
    ui.set_workspaces(vec![ONE, TWO], TWO, true);
    ui.settle_scroll();
    assert_eq!(ui.scroll.target(), 1.0);
    assert!(ui.scroll_release.is_some() || glide_theme::motion::is_disabled(), "the release keeps the finger's speed");
}

#[test]
fn only_the_active_page_takes_hover_presses_and_navigation() {
    let (here, there) = (WindowId::next(), WindowId::next());
    let entries = [entry(here, rect(100, 100, 800, 600), ONE), entry(there, rect(100, 100, 800, 600), TWO)];
    let mut ui = ui_with(&entries, 0, None, &[ONE, TWO]);
    assert!(!ui.set_hover(Some(there)));
    assert_eq!(ui.hovered(), None);
    assert!(!ui.press(there, Point::from((0.0, 0.0))));
    assert!(ui.navigate(Direction::Right));
    assert_eq!(ui.hovered(), Some(here));
    assert!(!ui.navigate(Direction::Right), "navigation never leaves the page");
}

#[test]
fn the_close_button_shows_on_the_hovered_tile_only_and_clicks_only_when_settled() {
    let preview = rect(100, 200, 400, 300);
    let button = close_button_rect(preview);
    assert_eq!(button.size, Size::from((OV_CLOSE_SIZE, OV_CLOSE_SIZE)));
    assert_eq!(button.loc.y, preview.loc.y - OV_CLOSE_OUTSET);
    assert_eq!(button.loc.x + button.size.w, preview.loc.x + preview.size.w + OV_CLOSE_OUTSET);

    let (mut ui, id) = ui_with_one_window(0);
    let settled = ui.preview_rect(id).unwrap();
    ui.set_hover(None);
    assert_eq!(ui.close_button_at(centre(close_button_rect(settled))), None, "no button without a hover");
    ui.set_hover(Some(id));
    assert_eq!(ui.close_button_at(centre(close_button_rect(settled))), Some(id));
    assert_eq!(ui.close_button_at(centre(settled)), None);
    let (mut moving, id) = ui_with_one_window(400);
    moving.phase = Phase::opening(ago(40));
    let button = close_button_rect(moving.preview_rect(id).unwrap());
    assert_eq!(moving.close_button_at(centre(button)), None);
}

#[test]
fn the_label_pill_makes_room_for_the_app_icon_and_ellipsises_inside_its_padding() {
    assert_eq!(label_icon_side(28), 18);
    assert_eq!(label_metrics(100, 28, false, 1000), LabelMetrics { width: 124, text: (12, 112) });
    assert_eq!(label_metrics(100, 28, true, 1000), LabelMetrics { width: 141, text: (29, 129) });
    assert_eq!(label_metrics(400, 28, true, 200), LabelMetrics { width: 200, text: (29, 188) });
    assert_eq!(label_metrics(400, 28, true, 20).text, (29, 29));
    assert!((LABEL_RISE - 4.0).abs() < f64::EPSILON);
    assert_eq!((label_offset(28), label_offset(40)), (-14, -20));
}

#[test]
fn the_close_cross_and_shadow_follow_the_design() {
    assert!((OV_CLOSE_GLYPH - 6.0 * 11.0 / 12.0).abs() < 0.01);
    assert!((OV_CLOSE_GLYPH_STROKE - 1.8 * 11.0 / 12.0).abs() < 0.01);
    let layers = close_shadow_layers(glide_theme::Appearance::Dark);
    assert_eq!(layers.len(), 2, "--el-2 is a two-layer shadow");
    let widest = layers.iter().map(|layer| layer.blur).fold(0.0_f32, f32::max);
    assert!(close_shadow_margin() > 0 && close_shadow_margin() as f32 >= widest);
}

#[test]
fn relayout_with_identical_inputs_is_skipped_and_a_changed_frame_relayouts() {
    let (mut ui, id) = ui_with_one_window(0);
    ui.previews[0].row = 99;
    ui.relayout(&[entry(id, rect(0, 0, 800, 600), ONE)], place());
    assert_eq!(ui.previews[0].row, 99, "identical inputs leave the previews untouched");
    ui.relayout(&[entry(id, rect(0, 0, 400, 300), ONE)], place());
    assert_eq!(ui.previews[0].row, 0);
    assert_eq!(ui.previews[0].frame, rect(0, 0, 400, 300));
}

#[test]
fn a_tile_whose_slot_moves_glides_there_instead_of_jumping() {
    let (a, b) = (WindowId::next(), WindowId::next());
    let entries = [entry(a, rect(0, 0, 800, 600), ONE), entry(b, rect(0, 0, 800, 600), ONE)];
    let mut ui = ui_with(&entries, 0, Some(a), &[ONE]);
    let before = ui.preview_rect(b).unwrap();
    ui.relayout(&[entry(b, rect(0, 0, 800, 600), ONE)], place());
    let after = ui.previews.iter().find(|p| p.id == b).unwrap();
    assert_ne!(round_rect(after.target.to_f64()), before);
    if !glide_theme::motion::is_disabled() {
        assert!(after.is_gliding());
        assert_eq!(ui.preview_rect(b).unwrap(), before, "it starts where it was drawn");
    }
}

#[test]
fn relayout_moves_hover_to_the_adjacent_neighbour_not_first() {
    let (a, b, c) = (WindowId::next(), WindowId::next(), WindowId::next());
    let all =
        [entry(a, rect(0, 0, 800, 600), ONE), entry(b, rect(0, 0, 800, 600), ONE), entry(c, rect(0, 0, 800, 600), ONE)];
    let mut ui = ui_with(&all, 0, Some(b), &[ONE]);
    ui.relayout(&[all[0], all[2]], place());
    assert_eq!(ui.hovered(), Some(c));
}

#[test]
fn navigate_without_hover_respects_direction() {
    let (a, b) = (WindowId::next(), WindowId::next());
    let entries = [entry(a, rect(0, 0, 800, 600), ONE), entry(b, rect(0, 0, 800, 600), ONE)];
    let mut ui = ui_with(&entries, 0, None, &[ONE]);
    ui.navigate(Direction::Right);
    assert_eq!(ui.hovered(), Some(a));
    ui.set_hover(None);
    ui.navigate(Direction::Left);
    assert_eq!(ui.hovered(), Some(b));
}

#[test]
fn a_flicked_commit_carries_the_finger_speed_into_the_animation() {
    let (mut ui, _id) = ui_with_one_window(400);
    ui.set_interactive_progress(0.4);
    ui.commit_to_opening(3.0);
    let Phase::Opening { from, .. } = ui.phase else {
        panic!("expected Phase::Opening");
    };
    let release = from.release.expect("a gesture commit uses a release curve");
    let speed = release.slope * 0.6 / release.duration.as_secs_f64();
    assert!((speed - 3.0).abs() < 0.05, "left the finger at {speed}");
}

#[test]
fn interactive_scrubbing_is_linear_and_clamped() {
    let (mut ui, _id) = ui_with_one_window(400);
    for step in 0..=20 {
        let q = f64::from(step) / 20.0;
        ui.set_interactive_progress(q);
        assert!((ui.progress() - q).abs() < 1e-9);
        assert!((ui.backdrop_progress() - q).abs() < 1e-9);
    }
    ui.set_interactive_progress(-100.0);
    assert!(ui.progress().abs() < f64::EPSILON);
    ui.revert_to_closing(0.0);
    assert!(ui.is_closing());
}

#[test]
fn handing_over_to_the_launcher_lifts_and_shrinks_the_tiles_instead_of_fading_them_in_place() {
    let tile = rect(200, 300, 500, 300).to_f64();
    assert_eq!(handoff_rect(tile, 0.0), tile);
    let gone = handoff_rect(tile, 1.0);
    assert!((gone.size.w - 460.0).abs() < 1e-9, "×.92");
    assert!(tile.loc.y - gone.loc.y >= 260.0, "the tile travels up with the axis: {gone:?}");
    const { assert!(HANDOFF_LIFT >= 24.0 && HANDOFF_STRIP >= 24.0) };
    let (mut ui, _id) = ui_with_one_window(400);
    assert!(ui.set_handoff(0.5));
    assert!(ui.handoff_strip() >= ui.handoff_lift(), "the strip leaves before the tiles");
}

#[test]
fn a_window_drawn_in_a_thumbnail_stays_inside_it_even_when_it_hangs_off_screen() {
    let thumb = Rectangle::new(Point::from((100.0, 50.0)), Size::from((192.0, 108.0)));
    let output = rect(0, 0, 1920, 1080);
    let inside = mini_rect(rect(100, 100, 960, 540), thumb, output);
    assert!((inside.size.w - 96.0).abs() < 1e-9 && (inside.loc.x - 110.0).abs() < 1e-9, "{inside:?}");
    let hanging = mini_rect(rect(1500, -200, 800, 3000), thumb, output);
    assert!(hanging.loc.x >= thumb.loc.x && hanging.loc.y >= thumb.loc.y);
    assert!(hanging.loc.x + hanging.size.w <= thumb.loc.x + thumb.size.w + 1e-9);
    assert!(hanging.loc.y + hanging.size.h <= thumb.loc.y + thumb.size.h + 1e-9);
}

mod dragging {
    use super::*;

    fn two_pages() -> (OverviewUi, WindowId, WindowId) {
        let (here, there) = (WindowId::next(), WindowId::next());
        let entries = [entry(here, rect(100, 100, 800, 600), ONE), entry(there, rect(100, 100, 800, 600), TWO)];
        (ui_with(&entries, 0, Some(here), &[ONE, TWO]), here, there)
    }

    fn thumb_centre(ui: &OverviewUi, workspace: WorkspaceId) -> Point<f64, Logical> {
        let thumb = ui.strip.thumb_rect(ui.place.area, ui.place.output, workspace, 0.0).unwrap();
        Point::from((thumb.loc.x + thumb.size.w / 2.0, thumb.loc.y + thumb.size.h / 2.0))
    }

    #[test]
    fn a_press_released_in_place_is_a_click_on_the_window() {
        let (mut ui, here, _) = two_pages();
        let at = centre(ui.preview_rect(here).unwrap());
        assert!(ui.press(here, at));
        assert!(!ui.drag_motion(Point::from((at.x + 3.0, at.y))));
        assert_eq!(ui.release(), Some(Release::Click(here)));
        assert!(!ui.is_pressed());
    }

    #[test]
    fn dropping_on_another_workspace_thumbnail_moves_the_window_there() {
        let (mut ui, here, _) = two_pages();
        let at = centre(ui.preview_rect(here).unwrap());
        ui.press(here, at);
        assert!(ui.drag_motion(thumb_centre(&ui, TWO)));
        assert!(ui.is_dragging());
        assert_eq!(ui.preview_at(at), None, "the carried tile is out of the grid");
        assert_eq!(ui.release(), Some(Release::Move { id: here, to: MoveTo::Workspace(TWO) }));
    }

    #[test]
    fn dropping_on_its_own_workspace_or_nowhere_sends_the_tile_back() {
        let (mut ui, here, _) = two_pages();
        let at = centre(ui.preview_rect(here).unwrap());
        ui.press(here, at);
        ui.drag_motion(thumb_centre(&ui, ONE));
        assert_eq!(ui.release(), Some(Release::Settled));
        ui.press(here, at);
        ui.drag_motion(Point::from((at.x + 40.0, at.y + 40.0)));
        assert_eq!(ui.release(), Some(Release::Settled));
        if !glide_theme::motion::is_disabled() {
            assert!(ui.previews.iter().find(|p| p.id == here).unwrap().is_gliding(), "it travels back to its slot");
        }
    }

    #[test]
    fn the_add_tile_and_the_far_edge_create_a_new_workspace() {
        let (mut ui, here, _) = two_pages();
        let at = centre(ui.preview_rect(here).unwrap());
        ui.press(here, at);
        ui.drag_motion(Point::from((at.x + 20.0, at.y)));
        let add =
            ui.strip.slots(ui.place.area, ui.place.output).into_iter().find(|(slot, _)| *slot == Slot::Add).unwrap().1;
        ui.drag_motion(Point::from((add.loc.x + add.size.w / 2.0, add.loc.y + add.size.h / 2.0)));
        assert_eq!(ui.release(), Some(Release::Move { id: here, to: MoveTo::New }));

        let (mut ui, here, _) = two_pages();
        ui.set_workspaces(vec![ONE, TWO], ONE, true);
        ui.press(here, at);
        ui.drag_motion(Point::from((1910.0, f64::from(ui.grid_area().loc.y) + 100.0)));
        assert_eq!(
            ui.release(),
            Some(Release::Move { id: here, to: MoveTo::Workspace(TWO) }),
            "the right edge is the next page"
        );
    }

    #[test]
    fn a_window_moved_to_another_page_lands_in_its_thumbnail() {
        let (mut ui, here, there) = two_pages();
        let at = centre(ui.preview_rect(here).unwrap());
        ui.press(here, at);
        ui.drag_motion(thumb_centre(&ui, TWO));
        ui.release();
        ui.relayout(
            &[entry(there, rect(100, 100, 800, 600), TWO), entry(here, rect(100, 100, 800, 600), TWO)],
            place(),
        );
        assert_eq!(ui.workspace_of(here), Some(TWO));
        assert_eq!(ui.landings.len(), 1);
        assert!(ui.is_animating());
    }

    #[test]
    fn escape_puts_a_carried_tile_back() {
        let (mut ui, here, _) = two_pages();
        let at = centre(ui.preview_rect(here).unwrap());
        ui.press(here, at);
        ui.drag_motion(Point::from((at.x + 100.0, at.y)));
        assert!(ui.cancel_drag());
        assert!(!ui.is_pressed());
        assert!(!ui.cancel_drag());
    }
}

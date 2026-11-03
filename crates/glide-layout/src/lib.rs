pub mod ids;
pub mod layout;
pub mod overview;

pub use glide_geom::{Insets, Point, Rectangle, Size};
pub use ids::{OutputId, WindowId, WorkspaceId};
pub use layout::{Layout, NewWindowPolicy, OutputInfo, WindowState, Workspace, WorkspaceTarget};
pub use overview::{OverviewOptions, OverviewSlot, compute_overview_layout};

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: Size = Size::new(800, 600);
    const SMALL: Size = Size::new(400, 300);

    fn output(id: OutputId, geometry: Rectangle) -> OutputInfo {
        OutputInfo { id, geometry, work_area: geometry }
    }

    fn rect(x: i32, y: i32, w: i32, h: i32) -> Rectangle {
        Rectangle::from_loc_and_size(x, y, w, h)
    }

    fn with_output(geometry: Rectangle) -> Layout {
        let mut layout = Layout::new();
        layout.outputs_changed(&[output(OutputId::next(), geometry)]);
        layout
    }

    fn with_panel() -> Layout {
        let mut layout = Layout::new();
        layout.outputs_changed(&[output_with_panel(OutputId::next())]);
        layout
    }

    fn output_with_panel(id: OutputId) -> OutputInfo {
        OutputInfo { id, geometry: rect(0, 0, 1920, 1080), work_area: rect(0, 40, 1920, 1040) }
    }

    fn two_outputs() -> [OutputInfo; 2] {
        [output(OutputId::next(), rect(0, 0, 1920, 1080)), output(OutputId::next(), rect(1920, 0, 2560, 1440))]
    }

    fn windows(layout: &mut Layout, count: usize, size: Size) -> Vec<WindowId> {
        (0..count)
            .map(|_| {
                let id = WindowId::next();
                layout.add_window(id, size);
                id
            })
            .collect()
    }

    fn shared(count: usize) -> (Layout, Vec<WindowId>) {
        let mut layout = Layout::new();
        layout.set_policy(NewWindowPolicy::ActiveWorkspace);
        let ids = windows(&mut layout, count, SIZE);
        (layout, ids)
    }

    fn state(layout: &Layout, id: WindowId) -> &WindowState {
        layout.window_state(id).unwrap()
    }

    fn geometry(layout: &Layout, id: WindowId) -> Rectangle {
        state(layout, id).geometry
    }

    fn visible(layout: &Layout) -> Vec<WindowId> {
        layout.visible_windows().map(|s| s.id).collect()
    }

    fn overview_ids(layout: &Layout) -> Vec<WindowId> {
        layout.overview_windows().iter().map(|s| s.id).collect()
    }

    fn titlebar_insets() -> Insets {
        Insets { top: 32, left: 0, right: 0, bottom: 0 }
    }

    fn inset_window() -> (Layout, WindowId) {
        let mut layout = Layout::new();
        let w = WindowId::next();
        layout.add_window(w, Size::ZERO);
        layout.set_window_insets(w, titlebar_insets());
        layout.update_window_size(w, SIZE);
        (layout, w)
    }

    #[test]
    fn test_add_window_new_workspace_policy() {
        let mut layout = Layout::new();
        assert_eq!(layout.policy(), NewWindowPolicy::NewWorkspace);
        let (w1, w2) = (WindowId::next(), WindowId::next());
        let ws1 = layout.add_window(w1, Size::new(1024, 768));
        assert_eq!((layout.window_count(), layout.workspace_count()), (1, 1));
        assert_eq!(layout.focused_window(), Some(w1));
        assert!(layout.is_focused(w1) && layout.is_visible(w1));

        let ws2 = layout.add_window(w2, Size::new(1024, 768));
        assert_ne!(ws1, ws2);
        assert_eq!((layout.window_count(), layout.workspace_count()), (2, 2));
        assert_eq!(layout.focused_window(), Some(w2));
        assert!(layout.is_visible(w2) && !layout.is_visible(w1));
    }

    #[test]
    fn test_add_window_active_workspace_policy() {
        let mut layout = Layout::new();
        layout.set_policy(NewWindowPolicy::ActiveWorkspace);
        let (w1, w2) = (WindowId::next(), WindowId::next());
        assert_eq!(layout.add_window(w1, SIZE), layout.add_window(w2, SIZE));
        assert_eq!((layout.window_count(), layout.workspace_count()), (2, 1));
        assert!(layout.is_visible(w1) && layout.is_visible(w2));
    }

    #[test]
    fn test_delayed_placement_zero_size() {
        let mut layout = Layout::new();
        let w1 = WindowId::next();
        layout.add_window(w1, Size::ZERO);
        assert!(!state(&layout, w1).is_placed);
        assert_eq!(geometry(&layout, w1).loc, Point::ZERO);

        assert!(layout.update_window_size(w1, SIZE));
        assert!(state(&layout, w1).is_placed);
        assert_eq!(geometry(&layout, w1).loc, Point::new(560, 240));
    }

    mod maximized_before_first_commit {
        use super::*;

        fn layout_with_output() -> (Layout, WindowId) {
            let mut layout = with_output(rect(0, 0, 1920, 1080));
            let w = WindowId::next();
            layout.add_window(w, Size::ZERO);
            (layout, w)
        }

        fn placed() -> (Layout, WindowId, Rectangle) {
            let mut layout = with_output(rect(0, 0, 1920, 1080));
            let w = WindowId::next();
            layout.add_window(w, SIZE);
            let natural = geometry(&layout, w);
            (layout, w, natural)
        }

        #[test]
        fn maximizing_an_unplaced_window_marks_it_placed() {
            let (mut layout, w) = layout_with_output();
            assert!(!state(&layout, w).is_placed);
            assert!(layout.set_maximized(w, true));
            assert!(
                state(&layout, w).is_placed,
                "without this the configure carries no size, so the client picks its own default and maps tiny"
            );
            assert_eq!(geometry(&layout, w), layout.primary_output_work_area());
        }

        #[test]
        fn a_committed_size_does_not_shrink_a_maximized_window() {
            let (mut layout, w) = layout_with_output();
            layout.set_maximized(w, true);
            let maximized = geometry(&layout, w);
            assert!(!layout.update_window_size(w, Size::new(500, 120)));
            assert_eq!(geometry(&layout, w), maximized, "the compositor owns a maximized window's geometry");
        }

        #[test]
        fn fullscreen_before_placement_behaves_the_same() {
            let (mut layout, w) = layout_with_output();
            assert!(layout.set_fullscreen(w, true));
            assert!(state(&layout, w).is_placed);
            assert_eq!(geometry(&layout, w), layout.primary_output_geometry());
            assert!(!layout.update_window_size(w, Size::new(500, 120)));
            assert_eq!(geometry(&layout, w), layout.primary_output_geometry());
        }

        #[test]
        fn unmaximizing_a_never_placed_window_returns_it_to_delayed_placement() {
            let (mut layout, w) = layout_with_output();
            layout.set_maximized(w, true);
            layout.set_maximized(w, false);
            assert!(
                !state(&layout, w).is_placed,
                "it never had a natural geometry to restore, so it must be centred from the size it eventually commits"
            );
            assert!(layout.update_window_size(w, SIZE));
            assert_eq!(geometry(&layout, w).size, SIZE);
        }

        #[test]
        fn unmaximizing_a_placed_window_restores_its_saved_geometry() {
            let (mut layout, w, natural) = placed();
            layout.set_maximized(w, true);
            layout.set_maximized(w, false);
            assert_eq!(geometry(&layout, w), natural);
        }

        #[test]
        fn adding_a_titlebar_keeps_maximized_and_fullscreen_windows_in_place() {
            for fullscreen in [false, true] {
                let (mut layout, w) = layout_with_output();
                let expected = if fullscreen {
                    layout.set_fullscreen(w, true);
                    layout.primary_output_geometry()
                } else {
                    layout.set_maximized(w, true);
                    layout.primary_output_work_area()
                };
                layout.set_window_insets(w, Insets { top: 48, ..Insets::NONE });
                assert_eq!(geometry(&layout, w), expected);
            }
        }

        #[test]
        fn a_titlebar_added_while_maximized_still_applies_on_restore() {
            let (mut layout, w, natural) = placed();
            layout.set_maximized(w, true);
            layout.set_window_insets(w, Insets { top: 48, ..Insets::NONE });
            layout.set_maximized(w, false);
            assert_eq!(
                geometry(&layout, w).size,
                Size::new(natural.size.width, natural.size.height + 48),
                "the restored frame has to make room for the titlebar it gained"
            );
        }

        #[test]
        fn a_floating_window_still_grows_to_fit_a_new_titlebar() {
            let (mut layout, w, before) = placed();
            layout.set_window_insets(w, Insets { top: 48, ..Insets::NONE });
            let after = geometry(&layout, w);
            assert_eq!(after.size.height, before.size.height + 48);
            assert_eq!(after.loc.y, before.loc.y - 48);
        }

        #[test]
        fn maximizing_with_no_output_is_refused_rather_than_collapsing_to_zero() {
            let mut layout = Layout::new();
            layout.outputs_changed(&[]);
            let w = WindowId::next();
            layout.add_window(w, Size::ZERO);
            if layout.primary_output_work_area().size.is_empty() {
                assert!(!layout.set_maximized(w, true));
                assert!(!state(&layout, w).maximized);
            }
        }
    }

    #[test]
    fn test_centering_on_custom_output() {
        let mut layout = with_output(rect(1000, 0, 2560, 1440));
        let w1 = WindowId::next();
        layout.add_window(w1, Size::new(1000, 800));
        assert_eq!(geometry(&layout, w1).loc, Point::new(1780, 320));
    }

    #[test]
    fn test_cascade_offset_for_multiple_windows_same_workspace() {
        let (layout, ids) = shared(2);
        assert_eq!(geometry(&layout, ids[0]).loc, Point::new(560, 240));
        assert_eq!(geometry(&layout, ids[1]).loc, Point::new(560 + 32, 240 + 32));
    }

    #[test]
    fn test_outputs_changed_keeps_on_screen_windows_and_rescues_lost_ones() {
        for (dragged, screen, expected) in [
            (rect(40, 50, 800, 600), rect(0, 0, 3840, 2160), Point::new(40, 50)),
            (rect(5000, 5000, 800, 600), rect(0, 0, 1920, 1080), Point::new(560, 240)),
        ] {
            let mut layout = Layout::new();
            let w1 = WindowId::next();
            layout.add_window(w1, SIZE);
            layout.set_window_geometry(w1, dragged);
            layout.outputs_changed(&[output(OutputId::next(), screen)]);
            assert_eq!(geometry(&layout, w1), Rectangle { loc: expected, ..dragged });
        }
    }

    #[test]
    fn test_outputs_changed_resizes_maximized_windows() {
        let mut layout = Layout::new();
        let w1 = WindowId::next();
        layout.add_window(w1, SIZE);
        layout.set_maximized(w1, true);
        layout.outputs_changed(&[output(OutputId::next(), rect(0, 0, 3840, 2160))]);
        assert_eq!(geometry(&layout, w1), rect(0, 0, 3840, 2160));
    }

    #[test]
    fn test_visible_windows_are_ordered_bottom_to_top_and_raise_moves_to_the_top() {
        let mut layout = Layout::new();
        layout.set_policy(NewWindowPolicy::ActiveWorkspace);
        let ids = windows(&mut layout, 8, SMALL);
        for _ in 0..5 {
            assert_eq!(visible(&layout), ids, "stacking order must not vary between calls");
        }
        assert!(layout.raise_window(ids[0]));
        assert_eq!(visible(&layout), [&ids[1..], &ids[..1]].concat());
    }

    #[test]
    fn test_workspace_remembers_its_own_focus() {
        let (mut layout, ids) = shared(2);
        let ws1 = layout.active_workspace();
        layout.focus_window(ids[0]);
        layout.set_policy(NewWindowPolicy::NewWorkspace);
        layout.add_window(WindowId::next(), SMALL);
        assert!(layout.switch_to_workspace(WorkspaceTarget::Id(ws1)));
        assert_eq!(layout.focused_window(), Some(ids[0]));
    }

    #[test]
    fn test_focus_window_raises_and_switches_workspace() {
        let mut layout = Layout::new();
        let (w1, w2) = (WindowId::next(), WindowId::next());
        let ws1 = layout.add_window(w1, SMALL);
        layout.add_window(w2, SMALL);
        assert!(layout.focus_window(w1));
        assert_eq!(layout.active_workspace(), ws1);
        assert_eq!(layout.focused_window(), Some(w1));
        assert!(layout.is_visible(w1));
        assert!(!layout.focus_window(WindowId::next()));
    }

    #[test]
    fn test_dialog_on_inactive_workspace_does_not_steal_focus() {
        let mut layout = Layout::new();
        let (parent, other, dialog) = (WindowId::next(), WindowId::next(), WindowId::next());
        let ws_parent = layout.add_window(parent, SIZE);
        layout.add_window(other, SIZE);
        let ws_other = layout.active_workspace();
        assert_ne!(ws_parent, ws_other);
        assert_eq!(layout.add_window_with_parent(dialog, SMALL, parent), ws_parent);
        assert_eq!(layout.active_workspace(), ws_other);
        assert_eq!(layout.focused_window(), Some(other));
        assert!(layout.switch_to_workspace(WorkspaceTarget::Id(ws_parent)));
        assert_eq!(layout.focused_window(), Some(dialog));
    }

    #[test]
    fn a_dialog_opens_centred_over_its_parent() {
        let mut layout = Layout::new();
        let (parent, dialog, late) = (WindowId::next(), WindowId::next(), WindowId::next());
        layout.add_window(parent, SIZE);
        layout.add_window_with_parent(dialog, SMALL, parent);
        layout.add_window_with_parent(late, Size::new(0, 0), parent);
        layout.update_window_size(late, SMALL);
        let center = |id| {
            let frame = layout.window_state(id).expect("window").frame_geometry();
            (frame.loc.x + frame.size.width / 2, frame.loc.y + frame.size.height / 2)
        };
        assert_eq!(center(dialog), center(parent));
        assert_eq!(center(late), center(parent), "placed once its size is known");
    }

    #[test]
    fn test_dynamic_workspace_destruction_on_remove() {
        let mut layout = Layout::new();
        let ids = windows(&mut layout, 2, SIZE);
        assert_eq!(layout.workspace_count(), 2);
        assert!(layout.remove_window(ids[1]));
        assert!(!layout.remove_window(ids[1]));
        assert_eq!((layout.window_count(), layout.workspace_count()), (1, 1));
        assert_eq!(layout.focused_window(), Some(ids[0]));
        assert!(layout.is_visible(ids[0]));

        layout.remove_window(ids[0]);
        assert_eq!((layout.window_count(), layout.workspace_count()), (0, 1), "the last workspace survives");
    }

    #[test]
    fn closing_a_window_returns_to_the_workspace_it_was_opened_from() {
        let mut layout = Layout::new();
        let (terminal, browser, files) = (WindowId::next(), WindowId::next(), WindowId::next());
        let home = layout.add_window(terminal, SIZE);
        layout.add_window(browser, SIZE);
        assert!(layout.focus_window(terminal));
        layout.add_window(files, SIZE);
        assert_eq!(layout.workspace_ids()[1], layout.active_workspace(), "opened right after the terminal's");
        assert!(layout.remove_window(files));
        assert_eq!(layout.active_workspace(), home, "not the neighbour that took the index");
        assert_eq!(layout.focused_window(), Some(terminal));
    }

    #[test]
    fn closing_windows_one_after_another_walks_back_in_the_order_they_were_visited() {
        let mut layout = Layout::new();
        let ids = windows(&mut layout, 3, SIZE);
        let workspaces = layout.workspace_ids();
        assert!(layout.focus_window(ids[2]));
        assert!(layout.focus_window(ids[0]));
        assert!(layout.focus_window(ids[1]));
        assert!(layout.remove_window(ids[1]));
        assert_eq!(layout.active_workspace(), workspaces[0]);
        assert_eq!(layout.focused_window(), Some(ids[0]));
        assert!(layout.remove_window(ids[0]));
        assert_eq!(layout.active_workspace(), workspaces[2]);
        assert_eq!(layout.focused_window(), Some(ids[2]));
    }

    #[test]
    fn test_switching_to_a_new_workspace_appends_one_empty_workspace() {
        let mut layout = Layout::new();
        let ws1 = layout.add_window(WindowId::next(), SIZE);
        assert!(!layout.last_workspace_is_empty());
        assert!(layout.switch_to_workspace(WorkspaceTarget::New));
        let fresh = layout.active_workspace();
        assert_ne!(fresh, ws1);
        assert_eq!(layout.workspace_ids(), vec![ws1, fresh]);
        assert_eq!(layout.focused_window(), None);
        assert!(layout.last_workspace_is_empty());
        assert!(layout.switch_to_workspace(WorkspaceTarget::New));
        assert_eq!(layout.active_workspace(), fresh);
        assert_eq!(layout.workspace_count(), 2);

        assert_eq!(layout.add_window(WindowId::next(), SIZE), fresh, "a new window fills the empty workspace");
        assert_eq!(layout.workspace_count(), 2);
    }

    #[test]
    fn test_leaving_an_empty_workspace_destroys_it() {
        let mut layout = Layout::new();
        let w1 = WindowId::next();
        let ws1 = layout.add_window(w1, SIZE);
        layout.switch_to_workspace(WorkspaceTarget::New);
        assert!(layout.switch_to_workspace(WorkspaceTarget::Previous));
        assert_eq!(layout.workspace_ids(), vec![ws1]);
        assert_eq!(layout.focused_window(), Some(w1));
        layout.switch_to_workspace(WorkspaceTarget::New);
        assert!(layout.focus_window(w1));
        assert_eq!(layout.workspace_ids(), vec![ws1]);
    }

    #[test]
    fn test_moving_a_window_to_a_new_workspace() {
        let (mut layout, ids) = shared(2);
        let ws1 = layout.active_workspace();
        assert!(layout.move_window_to_workspace(ids[1], WorkspaceTarget::New));
        let workspaces = layout.workspace_ids();
        assert_eq!(workspaces.len(), 2);
        assert_eq!(workspaces[0], ws1);
        assert_eq!(state(&layout, ids[1]).workspace_id, workspaces[1]);
        assert!(!layout.move_window_to_workspace(WindowId::next(), WorkspaceTarget::New));
        assert_eq!(layout.workspace_count(), 2);
    }

    #[test]
    fn test_switch_workspace_by_index_id_next_and_previous() {
        let mut layout = Layout::new();
        let (w1, w2) = (WindowId::next(), WindowId::next());
        let ws1 = layout.add_window(w1, SIZE);
        let ws2 = layout.add_window(w2, SIZE);
        assert_eq!(layout.active_workspace(), ws2);
        assert!(layout.switch_to_workspace(WorkspaceTarget::Index(0)));
        assert_eq!((layout.active_workspace(), layout.focused_window()), (ws1, Some(w1)));
        assert!(layout.switch_to_workspace(WorkspaceTarget::Id(ws2)));
        assert_eq!((layout.active_workspace(), layout.focused_window()), (ws2, Some(w2)));

        assert!(layout.switch_to_workspace(WorkspaceTarget::Previous));
        assert_eq!(layout.active_workspace(), ws1);
        assert!(!layout.switch_to_workspace(WorkspaceTarget::Previous));
        assert!(layout.switch_to_workspace(WorkspaceTarget::Next));
        assert_eq!(layout.active_workspace(), ws2);
        assert!(!layout.switch_to_workspace(WorkspaceTarget::Next));
    }

    #[test]
    fn test_move_window_to_workspace() {
        let (mut layout, ids) = shared(2);
        let ws1 = layout.active_workspace();
        layout.set_policy(NewWindowPolicy::NewWorkspace);
        let ws2 = layout.add_window(WindowId::next(), SIZE);
        assert!(layout.move_window_to_workspace(ids[0], WorkspaceTarget::Id(ws2)));
        assert_eq!(state(&layout, ids[0]).workspace_id, ws2);
        assert!(layout.is_visible(ids[0]));
        assert!(layout.move_window_to_workspace(ids[0], WorkspaceTarget::Id(ws1)));
        assert_eq!(state(&layout, ids[0]).workspace_id, ws1);
        assert!(!layout.is_visible(ids[0]));
    }

    #[test]
    fn test_maximize_and_fullscreen_restore_and_report_their_duration() {
        let mut layout = Layout::new();
        let w = WindowId::next();
        layout.add_window(w, SIZE);
        let initial = geometry(&layout, w);
        let locked = |layout: &Layout| state(layout, w).maximized || state(layout, w).fullscreen;
        assert!(!locked(&layout), "a fresh window is freely resizable");

        assert!(layout.set_maximized(w, true));
        assert!(locked(&layout) && state(&layout, w).maximized);
        assert_eq!(geometry(&layout, w).size, Size::new(1920, 1080));
        assert!(layout.set_maximized(w, false));
        assert!(!locked(&layout), "unmaximizing must unlock the size again");
        assert_eq!(geometry(&layout, w), initial);

        assert!(layout.toggle_fullscreen(w));
        assert!(locked(&layout) && state(&layout, w).fullscreen);
        assert_eq!(geometry(&layout, w).size, Size::new(1920, 1080));
        assert!(layout.toggle_fullscreen(w));
        assert!(!locked(&layout), "leaving fullscreen must unlock too");
        assert_eq!(geometry(&layout, w).size, SIZE);
    }

    #[test]
    fn fullscreen_goes_to_the_output_the_client_names_and_can_move_there_later() {
        let mut layout = Layout::new();
        layout.outputs_changed(&two_outputs());
        let w = WindowId::next();
        layout.add_window(w, SIZE);
        let external = Point::new(1920 + 1280, 720);
        assert!(layout.set_fullscreen_on(w, true, Some(external)));
        assert_eq!(geometry(&layout, w), rect(1920, 0, 2560, 1440));
        assert!(layout.set_fullscreen_on(w, true, Some(Point::new(10, 10))), "a second request moves it");
        assert_eq!(geometry(&layout, w), rect(0, 0, 1920, 1080));
        assert!(!layout.set_fullscreen_on(w, true, Some(Point::new(10, 10))), "already there");
        assert!(!layout.set_fullscreen_on(w, true, None), "no output named, nothing to do");
        assert!(layout.set_fullscreen(w, false));
        assert_eq!(geometry(&layout, w).size, SIZE, "it comes back to its floating size");
    }

    #[test]
    fn test_set_fullscreen_and_restore_when_maximized() {
        let mut layout = Layout::new();
        let w = WindowId::next();
        layout.add_window(w, SIZE);
        assert!(layout.set_maximized(w, true));
        let work_area = layout.primary_output_work_area();
        assert_eq!(geometry(&layout, w), work_area);

        assert!(layout.set_fullscreen(w, true));
        assert_eq!(geometry(&layout, w), layout.primary_output_geometry());
        assert!(state(&layout, w).fullscreen && state(&layout, w).maximized);

        assert!(layout.set_fullscreen(w, false));
        assert!(!state(&layout, w).fullscreen && state(&layout, w).maximized);
        assert_eq!(geometry(&layout, w), work_area);

        assert!(layout.set_maximized(w, false));
        assert_eq!(geometry(&layout, w).size, SIZE);
    }

    #[test]
    fn test_cascade_offset_wrapping_for_many_windows() {
        let mut layout = Layout::new();
        let ids = windows(&mut layout, 9, SMALL);
        assert_eq!(geometry(&layout, ids[0]).loc, geometry(&layout, ids[8]).loc);
    }

    #[test]
    fn a_window_taller_than_the_work_area_opens_below_the_panel() {
        let mut layout = with_panel();
        let w = WindowId::next();
        layout.add_window(w, Size::new(1920, 1080));
        assert_eq!(geometry(&layout, w).loc, Point::new(0, 40), "the titlebar must stay reachable");
    }

    #[test]
    fn a_late_titlebar_never_pushes_the_frame_under_the_panel() {
        let mut layout = with_panel();
        let w = WindowId::next();
        layout.add_window(w, Size::new(1200, 1030));
        assert!(geometry(&layout, w).loc.y >= 40);
        layout.set_window_insets(w, Insets { top: 48, left: 0, right: 0, bottom: 0 });
        assert_eq!(geometry(&layout, w).loc.y, 40, "the frame top must stay inside the work area");
        assert_eq!(geometry(&layout, w).size.height, 1030 + 48);
    }

    #[test]
    fn keep_reachable_leaves_windows_already_inside_alone() {
        let area = rect(0, 40, 1920, 1040);
        let inside = rect(100, 200, 800, 600);
        assert_eq!(crate::layout::keep_reachable(inside, area), inside);
        assert_eq!(crate::layout::keep_reachable(rect(-50, 10, 800, 600), area).loc, Point::new(0, 40));
    }

    #[test]
    fn keep_grip_reachable_rescues_titlebars_but_allows_partial_overhangs() {
        use crate::layout::{MOVE_MIN_VISIBLE_WIDTH, keep_grip_reachable};
        let area = rect(0, 40, 1920, 1040);
        let frame = rect(300, 5, 800, 600);
        let settled = keep_grip_reachable(frame, 36, area);
        assert_eq!((settled.loc, settled.size), (Point::new(300, 40), frame.size));

        let overhang = rect(-500, 300, 800, 600);
        assert_eq!(keep_grip_reachable(overhang, 36, area), overhang, "a partial overhang is a legitimate placement");
        let settled = keep_grip_reachable(rect(-5000, 300, 800, 600), 36, area);
        assert_eq!(settled.loc.x + settled.size.width, MOVE_MIN_VISIBLE_WIDTH);
        assert_eq!(keep_grip_reachable(rect(5000, 300, 800, 600), 36, area).loc.x, 1920 - MOVE_MIN_VISIBLE_WIDTH);
        assert_eq!(keep_grip_reachable(rect(300, 3000, 800, 600), 36, area).loc.y, 40 + 1040 - 36);
    }

    #[test]
    fn settled_after_move_uses_the_output_under_the_anchor_and_skips_locked_windows() {
        let mut layout = Layout::new();
        let left =
            OutputInfo { id: OutputId::next(), geometry: rect(0, 0, 1920, 1080), work_area: rect(0, 40, 1920, 1040) };
        let right = OutputInfo {
            id: OutputId::next(),
            geometry: rect(1920, 0, 1920, 1080),
            work_area: rect(1920, 40, 1920, 1040),
        };
        layout.outputs_changed(&[left, right]);
        let w = WindowId::next();
        layout.add_window(w, SIZE);

        layout.set_window_geometry(w, rect(2100, 0, 800, 600));
        assert_eq!(layout.settled_after_move(w, Point::new(2500, 10)), Some(rect(2100, 40, 800, 600)));
        layout.set_window_geometry(w, rect(2100, 200, 800, 600));
        assert_eq!(layout.settled_after_move(w, Point::new(2500, 210)), None);
        layout.set_window_geometry(w, rect(2100, 0, 800, 600));
        assert!(layout.set_maximized(w, true));
        assert_eq!(layout.settled_after_move(w, Point::new(2500, 10)), None);
    }

    #[test]
    fn workspace_windows_at_lists_neighbours_without_minimized() {
        let mut layout = Layout::new();
        windows(&mut layout, 2, SMALL);
        let count = layout.workspace_count();
        let listed: usize = (0..count).map(|index| layout.workspace_windows_at(index).len()).sum();
        assert_eq!(listed, 2);
        assert!(layout.workspace_windows_at(count + 3).is_empty());
    }

    #[test]
    fn test_work_area_bounds_placement_and_maximize_but_not_fullscreen() {
        let mut layout = with_panel();
        let w1 = WindowId::next();
        layout.add_window(w1, SIZE);
        assert_eq!(geometry(&layout, w1).loc, Point::new(560, 260));
        assert!(layout.set_maximized(w1, true));
        assert_eq!(geometry(&layout, w1), rect(0, 40, 1920, 1040), "maximized must not tuck itself under the panel");
        assert!(layout.toggle_fullscreen(w1));
        assert_eq!(geometry(&layout, w1), rect(0, 0, 1920, 1080), "fullscreen must cover the panel, unlike maximize");
    }

    #[test]
    fn test_minimizing_hides_unfocuses_and_focusing_restores() {
        let (mut layout, ids) = shared(2);
        assert_eq!(layout.focused_window(), Some(ids[1]));
        assert!(layout.set_minimized(ids[1], true));
        assert!(!layout.is_visible(ids[1]) && layout.is_visible(ids[0]));
        assert_eq!(layout.focused_window(), Some(ids[0]));
        assert_eq!(visible(&layout), vec![ids[0]]);

        assert!(layout.set_minimized(ids[0], true));
        assert_eq!(layout.focused_window(), None, "minimizing the last visible window clears focus");
        assert_eq!(layout.visible_windows().count(), 0);

        assert!(layout.focus_window(ids[1]));
        assert!(!state(&layout, ids[1]).minimized && layout.is_visible(ids[1]));
        assert_eq!(layout.focused_window(), Some(ids[1]));
    }

    #[test]
    fn test_initial_placement_accounts_for_insets() {
        let mut layout = Layout::new();
        let w1 = WindowId::next();
        layout.add_window(w1, Size::ZERO);
        assert!(!layout.set_window_insets(WindowId::next(), titlebar_insets()));
        assert!(layout.set_window_insets(w1, titlebar_insets()));
        layout.update_window_size(w1, SIZE);
        assert_eq!(state(&layout, w1).frame_geometry(), rect(560, 224, 800, 632));
    }

    #[test]
    fn test_content_geometry_is_frame_shrunk_by_insets_and_follows_resizes() {
        let (mut layout, w1) = inset_window();
        let (frame, content) = (state(&layout, w1).frame_geometry(), state(&layout, w1).content_geometry());
        assert_eq!(content.size, SIZE);
        assert_eq!(content.loc, Point::new(frame.loc.x, frame.loc.y + 32));

        layout.update_window_size(w1, Size::new(1000, 700));
        assert_eq!(state(&layout, w1).content_geometry().size, Size::new(1000, 700));
        assert_eq!(state(&layout, w1).frame_geometry().size, Size::new(1000, 732));
    }

    #[test]
    fn test_fullscreen_ignores_insets_for_content_geometry() {
        let (mut layout, w1) = inset_window();
        layout.toggle_fullscreen(w1);
        let state = state(&layout, w1);
        assert_eq!(state.content_geometry(), state.frame_geometry());
        assert_eq!(state.content_geometry(), rect(0, 0, 1920, 1080));
    }

    #[test]
    fn test_changing_insets_after_placement_keeps_content_fixed() {
        let (mut layout, w1) = inset_window();
        let content_before = state(&layout, w1).content_geometry();
        layout.set_window_insets(w1, Insets::NONE);
        assert_eq!(state(&layout, w1).content_geometry(), content_before);
        assert_eq!(state(&layout, w1).frame_geometry(), content_before, "with no insets the frame IS the content");
        layout.set_window_insets(w1, titlebar_insets());
        assert_eq!(state(&layout, w1).content_geometry(), content_before);
        assert_eq!(state(&layout, w1).frame_geometry().size, Size::new(800, 632));
    }

    #[test]
    fn test_overview_toggles_without_touching_focus_workspace_or_visibility() {
        let mut layout = Layout::new();
        assert!(!layout.overview_active());
        assert!(layout.open_overview());
        assert!(layout.overview_active());
        assert!(!layout.open_overview(), "opening twice reports no change");
        assert!(layout.close_overview());
        assert!(!layout.overview_active());
        assert!(!layout.close_overview(), "closing twice reports no change");

        windows(&mut layout, 2, SIZE);
        let (workspace, focus, before) = (layout.active_workspace(), layout.focused_window(), visible(&layout));
        layout.open_overview();
        assert_eq!((layout.active_workspace(), layout.focused_window()), (workspace, focus));
        assert_eq!(visible(&layout), before);
        assert_eq!(before.len(), 1, "only the active workspace is 'visible'");
    }

    #[test]
    fn test_overview_windows_span_every_workspace_and_skip_minimized() {
        let mut layout = Layout::new();
        let ids = windows(&mut layout, 3, SIZE);
        assert_eq!(layout.workspace_count(), 3);
        assert_eq!(overview_ids(&layout), ids, "workspace order, then stacking");
        layout.set_minimized(ids[0], true);
        assert_eq!(overview_ids(&layout), ids[1..]);
    }

    #[test]
    fn a_workspace_moves_to_a_new_place_and_keeps_its_windows_and_the_view() {
        let mut layout = Layout::new();
        let ids = windows(&mut layout, 3, SIZE);
        let order = layout.workspace_ids();
        let active = layout.active_workspace();
        assert!(layout.move_workspace(order[2], 0));
        assert_eq!(layout.workspace_ids(), vec![order[2], order[0], order[1]]);
        assert_eq!(layout.active_workspace(), active);
        assert_eq!(state(&layout, ids[2]).workspace_id, order[2]);
        assert!(!layout.move_workspace(order[2], 0), "already there");
        assert!(layout.move_workspace(order[2], 99), "a far index is the end");
        assert_eq!(layout.workspace_ids().last(), Some(&order[2]));
        assert!(!layout.move_workspace(WorkspaceId(9_999), 0));
    }

    #[test]
    fn the_overview_keeps_an_emptied_active_workspace_until_it_closes() {
        let mut layout = Layout::new();
        let ids = windows(&mut layout, 2, SIZE);
        let (first, second) = (state(&layout, ids[0]).workspace_id, layout.active_workspace());
        layout.open_overview();
        assert!(layout.move_window_to_workspace(ids[1], WorkspaceTarget::Id(first)));
        assert_eq!(layout.active_workspace(), second, "the view stays where the user is looking");
        assert_eq!(layout.workspace_ids(), vec![first, second]);
        assert_eq!(layout.focused_window(), None);

        assert!(layout.close_overview());
        assert_eq!(layout.workspace_ids(), vec![first], "the empty workspace goes once the overview does");
        assert_eq!(layout.active_workspace(), first);
        assert!(layout.focused_window().is_some());
    }

    #[test]
    fn dialog_reparented_after_creation_returns_to_parent_workspace_focused() {
        let mut layout = Layout::new();
        let (editor, term, dialog) = (WindowId::next(), WindowId::next(), WindowId::next());
        let ws_editor = layout.add_window(editor, SIZE);
        let ws_term = layout.add_window(term, SIZE);
        assert!(layout.switch_to_workspace(WorkspaceTarget::Id(ws_editor)));
        assert_eq!(layout.workspace_ids(), vec![ws_editor, ws_term]);
        layout.add_window(dialog, SMALL);
        assert_eq!(layout.workspace_count(), 3);

        assert!(layout.move_window_to_workspace(dialog, WorkspaceTarget::Id(ws_editor)));
        assert_eq!(layout.workspace_ids(), vec![ws_editor, ws_term]);
        assert_eq!((layout.active_workspace(), layout.focused_window()), (ws_editor, Some(dialog)));
        assert!(layout.is_visible(dialog) && layout.is_visible(editor));
    }

    #[test]
    fn a_vanishing_unfocused_active_workspace_refocuses_on_move_or_remove() {
        for remove in [false, true] {
            let mut layout = Layout::new();
            let (w1, w2) = (WindowId::next(), WindowId::next());
            let ws1 = layout.add_window(w1, SIZE);
            layout.add_window(w2, SIZE);
            layout.clear_focus();
            if remove {
                assert!(layout.remove_window(w2));
                assert_eq!(layout.focused_window(), Some(w1));
            } else {
                assert!(layout.move_window_to_workspace(w2, WorkspaceTarget::Id(ws1)));
                assert!(layout.focused_window().is_some());
            }
            assert_eq!(layout.active_workspace(), ws1);
        }
    }

    #[test]
    fn maximizing_a_fullscreen_window_only_records_the_flag() {
        let (screen, work_area) = (rect(0, 0, 1920, 1080), rect(0, 40, 1920, 1040));
        let mut layout = Layout::new();
        layout.outputs_changed(&[OutputInfo { id: OutputId::next(), geometry: screen, work_area }]);
        let w = WindowId::next();
        layout.add_window(w, SIZE);
        let floating = geometry(&layout, w);

        assert!(layout.set_fullscreen(w, true));
        assert!(layout.set_maximized(w, true));
        let current = state(&layout, w);
        assert!(current.fullscreen && current.maximized);
        assert_eq!((current.geometry, current.saved_geometry), (screen, Some(floating)));

        assert!(layout.set_fullscreen(w, false));
        assert_eq!((geometry(&layout, w), state(&layout, w).saved_geometry), (work_area, Some(floating)));
        assert!(layout.set_maximized(w, false));
        assert_eq!(geometry(&layout, w), floating);
    }

    #[test]
    fn unmaximizing_a_fullscreen_window_restores_floating_on_exit() {
        let mut layout = Layout::new();
        layout.outputs_changed(&two_outputs()[..1]);
        let w = WindowId::next();
        layout.add_window(w, SIZE);
        let floating = geometry(&layout, w);
        assert!(layout.set_maximized(w, true));
        assert!(layout.set_fullscreen(w, true));
        assert!(layout.set_maximized(w, false));
        assert!(layout.set_fullscreen(w, false));
        assert!(!state(&layout, w).maximized);
        assert_eq!(geometry(&layout, w), floating);
    }

    #[test]
    fn a_maximized_window_on_the_second_output_keeps_its_output() {
        let outputs = two_outputs();
        let maximized_on_second = || {
            let mut layout = Layout::new();
            layout.outputs_changed(&outputs);
            let w = WindowId::next();
            layout.add_window(w, SIZE);
            layout.set_window_geometry(w, rect(2200, 100, 800, 600));
            assert!(layout.set_maximized(w, true));
            (layout, w)
        };

        let (mut layout, w) = maximized_on_second();
        layout.set_window_insets(w, titlebar_insets());
        assert_eq!(geometry(&layout, w), outputs[1].work_area, "insets re-derive from the window's own output");

        let (mut layout, w) = maximized_on_second();
        layout.outputs_changed(&outputs[..1]);
        assert!(layout.set_maximized(w, false));
        let restored = geometry(&layout, w);
        assert_eq!(restored.size, SIZE);
        assert!(outputs[0].geometry.overlaps(restored), "saved geometry is rescued when its output goes away");
    }

    #[test]
    fn a_window_maximized_before_its_first_commit_opens_on_the_output_under_the_pointer() {
        let outputs = two_outputs();
        for fullscreen in [false, true] {
            let mut layout = Layout::new();
            layout.outputs_changed(&outputs);
            layout.set_placement_anchor(Point::new(2500, 700));
            let w = WindowId::next();
            layout.add_window(w, Size::ZERO);
            if fullscreen {
                assert!(layout.set_fullscreen(w, true));
                assert_eq!(geometry(&layout, w), outputs[1].geometry);
            } else {
                assert!(layout.set_maximized(w, true));
                assert_eq!(geometry(&layout, w), outputs[1].work_area);
            }
        }
    }

    #[test]
    fn a_dialog_stays_on_its_parents_output_when_the_pointer_is_elsewhere() {
        let outputs = two_outputs();
        let mut layout = Layout::new();
        layout.outputs_changed(&outputs);
        let parent = WindowId::next();
        layout.add_window(parent, SIZE);
        layout.set_window_geometry(parent, rect(2400, 300, 800, 600));
        layout.set_placement_anchor(Point::new(100, 100));
        let dialog = WindowId::next();
        layout.add_window_with_parent(dialog, SMALL, parent);
        assert!(outputs[1].geometry.contains(geometry(&layout, dialog).center()), "{:?}", geometry(&layout, dialog));
    }

    #[test]
    fn fixed_workspaces_exist_up_front_and_never_vanish() {
        let mut layout = Layout::new();
        assert!(layout.set_fixed_workspaces(Some(4)));
        assert!(!layout.set_fixed_workspaces(Some(4)));
        assert_eq!(layout.workspace_count(), 4);

        let first = windows(&mut layout, 2, SIZE);
        assert_eq!(layout.workspace_count(), 4, "new windows do not make new workspaces");
        assert!(
            first
                .iter()
                .all(|id| layout.window_state(*id).is_some_and(|s| s.workspace_id == layout.active_workspace()))
        );

        assert!(layout.switch_to_workspace(WorkspaceTarget::Index(2)));
        assert!(layout.switch_to_workspace(WorkspaceTarget::Index(3)));
        assert_eq!(layout.workspace_count(), 4, "leaving an empty workspace keeps it");
        let parked = windows(&mut layout, 1, SIZE)[0];
        assert!(!layout.switch_to_workspace(WorkspaceTarget::New), "no workspace is created past a busy last one");
        assert_eq!(layout.workspace_count(), 4);
        assert!(layout.move_window_to_workspace(parked, WorkspaceTarget::Index(0)));

        assert!(layout.switch_to_workspace(WorkspaceTarget::Index(0)));
        assert!(layout.move_window_to_workspace(first[0], WorkspaceTarget::Index(1)));
        assert!(layout.move_window_to_workspace(first[1], WorkspaceTarget::Index(1)));
        assert_eq!(layout.workspace_count(), 4, "an emptied workspace stays");
        assert_eq!(layout.active_workspace(), layout.workspace_ids()[0], "and so does the view");
        layout.remove_window(first[0]);
        assert_eq!(layout.workspace_count(), 4);
    }

    #[test]
    fn shrinking_the_fixed_count_folds_windows_into_the_last_one_kept() {
        let mut layout = Layout::new();
        layout.set_fixed_workspaces(Some(3));
        assert!(layout.switch_to_workspace(WorkspaceTarget::Index(2)));
        let late = windows(&mut layout, 1, SIZE)[0];
        layout.set_fixed_workspaces(Some(2));
        assert_eq!(layout.workspace_count(), 2);
        let kept = layout.workspace_ids()[1];
        assert_eq!(layout.window_state(late).map(|s| s.workspace_id), Some(kept));
        assert_eq!(layout.active_workspace(), kept, "the view follows the folded workspace");

        assert!(layout.set_fixed_workspaces(None));
        assert_eq!(layout.workspace_count(), 1, "back to dynamic: empty workspaces other than the active one go");
    }

    #[test]
    fn folding_the_active_workspace_keeps_the_focused_window_focused() {
        let mut layout = Layout::new();
        layout.set_fixed_workspaces(Some(3));
        assert!(layout.switch_to_workspace(WorkspaceTarget::Index(1)));
        let below = windows(&mut layout, 2, SIZE);
        layout.focus_window(below[0]);
        assert!(layout.switch_to_workspace(WorkspaceTarget::Index(2)));
        let top = windows(&mut layout, 1, SIZE)[0];
        layout.focus_window(top);
        layout.set_fixed_workspaces(Some(2));
        assert_eq!(layout.focused_window(), Some(top), "the folded windows land on top, so their focus stays");
    }

    #[test]
    fn losing_every_output_leaves_windows_where_they_were() {
        let screen = rect(0, 0, 2560, 1440);
        let mut layout = with_output(screen);
        let w = WindowId::next();
        layout.add_window(w, SIZE);
        layout.set_window_geometry(w, rect(2000, 1100, 400, 300));
        layout.outputs_changed(&[]);
        layout.outputs_changed(&[output(OutputId::next(), screen)]);
        assert_eq!(geometry(&layout, w), rect(2000, 1100, 400, 300));
    }

    #[test]
    fn a_titlebar_gained_while_maximized_or_fullscreen_never_restores_under_the_panel() {
        for fullscreen in [false, true] {
            let mut layout = with_panel();
            let w = WindowId::next();
            layout.add_window(w, SIZE);
            layout.set_window_geometry(w, rect(560, 40, 800, 600));
            if fullscreen {
                layout.set_fullscreen(w, true);
            } else {
                layout.set_maximized(w, true);
            }
            layout.set_window_insets(w, Insets { top: 48, ..Insets::NONE });
            if fullscreen {
                layout.set_fullscreen(w, false);
            } else {
                layout.set_maximized(w, false);
            }
            assert_eq!(geometry(&layout, w), rect(560, 40, 800, 648), "fullscreen {fullscreen}");
        }
    }

    fn stacked_outputs() -> (OutputInfo, OutputInfo) {
        (output(OutputId::next(), rect(0, 0, 1920, 1080)), output(OutputId::next(), rect(0, -1080, 1920, 1080)))
    }

    #[test]
    fn unplugging_the_output_a_titlebar_was_on_pulls_a_window_that_still_overlaps_another_into_reach() {
        let (lower, upper) = stacked_outputs();
        let mut layout = Layout::new();
        layout.outputs_changed(&[lower.clone(), upper]);
        let w = WindowId::next();
        layout.add_window(w, SIZE);
        layout.set_window_insets(w, titlebar_insets());
        layout.set_window_geometry(w, rect(100, -900, 800, 1000));
        layout.outputs_changed(std::slice::from_ref(&lower));
        assert_eq!(geometry(&layout, w), rect(100, 0, 800, 1000), "the titlebar must not stay above the screen");
    }

    #[test]
    fn unplugging_an_output_also_pulls_the_saved_frame_of_a_maximized_window_into_reach() {
        let (lower, upper) = stacked_outputs();
        let mut layout = Layout::new();
        layout.outputs_changed(&[lower.clone(), upper]);
        let w = WindowId::next();
        layout.add_window(w, SIZE);
        layout.set_window_geometry(w, rect(100, -900, 800, 1000));
        assert!(layout.set_maximized(w, true));
        assert_eq!(state(&layout, w).saved_geometry, Some(rect(100, -900, 800, 1000)));
        layout.outputs_changed(std::slice::from_ref(&lower));
        assert_eq!(state(&layout, w).saved_geometry, Some(rect(100, 0, 800, 1000)));
        assert_eq!(geometry(&layout, w), lower.work_area, "the maximized frame itself follows the remaining output");
    }

    #[test]
    fn a_window_hanging_off_the_bottom_with_its_titlebar_in_reach_is_left_alone_when_another_output_goes() {
        let (lower, upper) = stacked_outputs();
        let mut layout = Layout::new();
        layout.outputs_changed(&[lower.clone(), upper]);
        let w = WindowId::next();
        layout.add_window(w, SIZE);
        layout.set_window_insets(w, titlebar_insets());
        layout.set_window_geometry(w, rect(100, 900, 800, 600));
        layout.outputs_changed(std::slice::from_ref(&lower));
        assert_eq!(geometry(&layout, w), rect(100, 900, 800, 600));
    }

    #[test]
    fn a_window_that_overlaps_no_remaining_output_is_still_centred_on_the_primary_one() {
        let (lower, upper) = stacked_outputs();
        let mut layout = Layout::new();
        layout.outputs_changed(&[lower, upper.clone()]);
        let w = WindowId::next();
        layout.add_window(w, SIZE);
        layout.set_window_geometry(w, rect(100, 300, 800, 600));
        layout.outputs_changed(&[upper]);
        assert_eq!(geometry(&layout, w), rect(560, -840, 800, 600));
    }

    #[test]
    fn windows_follow_their_output_when_its_origin_moves() {
        let [a, b] = two_outputs();
        let mut layout = Layout::new();
        layout.outputs_changed(&[a.clone(), b.clone()]);
        let floating = WindowId::next();
        layout.add_window(floating, SIZE);
        layout.set_window_geometry(floating, rect(2000, 100, 800, 600));
        let maximized = WindowId::next();
        layout.add_window(maximized, SIZE);
        layout.set_window_geometry(maximized, rect(2200, 200, 800, 600));
        assert!(layout.set_maximized(maximized, true));
        let staying = WindowId::next();
        layout.add_window(staying, SIZE);
        layout.set_window_geometry(staying, rect(300, 300, 800, 600));

        let wider = OutputInfo { geometry: rect(0, 0, 3840, 1080), work_area: rect(0, 0, 3840, 1080), ..a };
        let moved = OutputInfo { geometry: rect(3840, 0, 2560, 1440), work_area: rect(3840, 0, 2560, 1440), ..b };
        layout.outputs_changed(&[wider, moved.clone()]);

        assert_eq!(geometry(&layout, floating), rect(3920, 100, 800, 600));
        assert_eq!(geometry(&layout, maximized), moved.work_area, "a maximized window stays on its own output");
        assert_eq!(state(&layout, maximized).saved_geometry, Some(rect(4120, 200, 800, 600)));
        assert_eq!(geometry(&layout, staying), rect(300, 300, 800, 600));
    }

    #[test]
    fn a_new_output_mapped_on_top_of_another_does_not_drag_that_outputs_windows_along() {
        let first = output(OutputId::next(), rect(0, 0, 1920, 1080));
        let newcomer = output(OutputId::next(), rect(0, 0, 1920, 1080));
        let mut layout = Layout::new();
        layout.outputs_changed(&[first.clone(), newcomer.clone()]);
        let w = WindowId::next();
        layout.add_window(w, SIZE);
        layout.set_window_geometry(w, rect(300, 300, 800, 600));
        let placed =
            OutputInfo { geometry: rect(1920, 0, 1920, 1080), work_area: rect(1920, 0, 1920, 1080), ..newcomer };
        layout.outputs_changed(&[first, placed]);
        assert_eq!(geometry(&layout, w), rect(300, 300, 800, 600));
    }

    #[test]
    fn a_decoration_height_change_keeps_a_tiled_frame_and_only_resizes_its_content() {
        for tile in [rect(0, 40, 960, 1040), rect(960, 40, 960, 1040)] {
            let mut layout = with_panel();
            let w = WindowId::next();
            layout.add_window(w, SIZE);
            layout.set_window_insets(w, Insets { top: 48, ..Insets::NONE });
            layout.set_window_geometry(w, tile);
            for top in [38, 48, 38] {
                layout.set_window_insets(w, Insets { top, ..Insets::NONE });
                assert_eq!(geometry(&layout, w), tile, "top inset {top}");
                assert_eq!(state(&layout, w).content_geometry().size.height, 1040 - top);
            }
        }
    }

    #[test]
    fn a_frame_filling_the_whole_work_area_is_not_a_tile_and_still_grows_with_its_titlebar() {
        let mut layout = with_panel();
        let w = WindowId::next();
        layout.add_window(w, SIZE);
        layout.set_window_geometry(w, rect(0, 40, 1920, 1040));
        layout.set_window_insets(w, Insets { top: 48, ..Insets::NONE });
        assert_eq!(geometry(&layout, w), rect(0, 40, 1920, 1088));
    }

    #[test]
    fn a_window_stays_on_its_output_when_that_output_shrinks_under_it() {
        let big = output(OutputId::next(), rect(0, 0, 3840, 2160));
        let right = output(OutputId::next(), rect(3840, 0, 1920, 1080));
        let mut layout = Layout::new();
        layout.outputs_changed(&[big.clone(), right.clone()]);
        let floating = WindowId::next();
        layout.add_window(floating, SIZE);
        layout.set_window_geometry(floating, rect(3000, 1500, 800, 600));
        let maximized = WindowId::next();
        layout.add_window(maximized, SIZE);
        layout.set_window_geometry(maximized, rect(2900, 1400, 800, 600));
        assert!(layout.set_maximized(maximized, true));

        let scaled = OutputInfo { geometry: rect(0, 0, 1920, 1080), work_area: rect(0, 40, 1920, 1040), ..big };
        let moved = OutputInfo { geometry: rect(1920, 0, 1920, 1080), work_area: rect(1920, 0, 1920, 1080), ..right };
        layout.outputs_changed(&[scaled.clone(), moved]);

        assert_eq!(geometry(&layout, floating), rect(1120, 480, 800, 600), "not carried onto the neighbour");
        assert_eq!(geometry(&layout, maximized), scaled.work_area);
        assert_eq!(state(&layout, maximized).saved_geometry, Some(rect(1120, 480, 800, 600)));
    }

    #[test]
    fn a_window_whose_titlebar_is_reachable_on_another_output_does_not_jump_after_a_move() {
        let lower =
            OutputInfo { id: OutputId::next(), geometry: rect(0, 0, 1920, 1080), work_area: rect(0, 40, 1920, 1040) };
        let upper = output(OutputId::next(), rect(0, -1080, 1920, 1080));
        let mut layout = Layout::new();
        layout.outputs_changed(&[lower, upper]);
        let w = WindowId::next();
        layout.add_window(w, SIZE);
        layout.set_window_insets(w, titlebar_insets());
        layout.set_window_geometry(w, rect(100, -300, 800, 632));
        assert_eq!(layout.settled_after_move(w, Point::new(500, 10)), None);
        layout.set_window_geometry(w, rect(100, 10, 800, 632));
        assert_eq!(layout.settled_after_move(w, Point::new(500, 20)), Some(rect(100, 40, 800, 632)));
    }

    #[test]
    fn a_window_placed_late_opens_where_the_pointer_was_when_it_was_created() {
        let outputs = two_outputs();
        let mut layout = Layout::new();
        layout.outputs_changed(&outputs);
        layout.set_placement_anchor(Point::new(2500, 700));
        let late = WindowId::next();
        layout.add_window(late, Size::ZERO);
        layout.set_placement_anchor(Point::new(100, 100));
        layout.add_window(WindowId::next(), SIZE);
        assert_eq!(layout.output_geometry_for_window(late), outputs[1].geometry);
        assert!(layout.update_window_size(late, SIZE));
        assert!(outputs[1].geometry.contains(geometry(&layout, late).center()), "{:?}", geometry(&layout, late));
    }

    #[test]
    fn windows_orphaned_by_an_unplug_cascade_instead_of_piling_up() {
        let (lower, upper) = stacked_outputs();
        let mut layout = Layout::new();
        layout.outputs_changed(&[lower, upper.clone()]);
        let ids = windows(&mut layout, 3, SIZE);
        for (index, &id) in ids.iter().enumerate() {
            let step = i32::try_from(index).unwrap_or(0) * 100;
            layout.set_window_geometry(id, rect(100 + step, 300, 800, 600));
        }
        layout.outputs_changed(&[upper]);
        let mut spots: Vec<Point> = ids.iter().map(|&id| geometry(&layout, id).loc).collect();
        spots.sort_unstable_by_key(|spot| (spot.x, spot.y));
        spots.dedup();
        assert_eq!(spots.len(), ids.len(), "{spots:?}");
    }

    #[test]
    fn a_titlebar_under_the_panel_is_pulled_into_the_work_area_when_outputs_change() {
        let mut layout = with_panel();
        let w = WindowId::next();
        layout.add_window(w, SIZE);
        layout.set_window_insets(w, titlebar_insets());
        layout.set_window_geometry(w, rect(300, 10, 800, 632));
        layout.outputs_changed(&[output_with_panel(OutputId::next())]);
        assert_eq!(geometry(&layout, w), rect(300, 40, 800, 632));
    }

    #[test]
    fn folding_a_workspace_into_another_keeps_the_focus_it_remembered() {
        let mut layout = Layout::new();
        layout.set_fixed_workspaces(Some(3));
        assert!(layout.switch_to_workspace(WorkspaceTarget::Index(2)));
        let folded = windows(&mut layout, 2, SIZE);
        layout.focus_window(folded[0]);
        assert!(layout.raise_window(folded[1]));
        assert!(layout.switch_to_workspace(WorkspaceTarget::Index(0)));
        layout.set_fixed_workspaces(Some(2));
        assert!(layout.switch_to_workspace(WorkspaceTarget::Index(1)));
        assert_eq!(layout.focused_window(), Some(folded[0]), "not whichever folded window lies on top");
    }

    mod focus {
        use super::*;

        fn app_window(layout: &mut Layout, app: &str) -> WindowId {
            let id = WindowId::next();
            layout.add_window(id, SMALL);
            layout.set_app(id, Some(app.to_string()));
            id
        }

        #[test]
        fn focusing_a_window_on_another_workspace_records_no_window_in_between() {
            let mut layout = Layout::new();
            let (a, b, c) = (WindowId::next(), WindowId::next(), WindowId::next());
            layout.add_window(a, SIZE);
            layout.add_window(b, SIZE);
            layout.add_window(c, SIZE);
            assert!(layout.focus_window(b));
            assert!(layout.focus_window(a));
            assert_eq!(layout.recent_windows(), [a, b, c]);
            assert!(layout.focus_window(c));
            assert_eq!(layout.recent_windows(), [c, a, b], "one step: the previous window comes second");
        }

        #[test]
        fn closing_a_dialog_returns_to_its_parent() {
            let (mut layout, ids) = shared(2);
            let dialog = WindowId::next();
            layout.focus_window(ids[0]);
            layout.focus_window(ids[1]);
            layout.add_window_with_parent(dialog, SMALL, ids[0]);
            assert_eq!(layout.focused_window(), Some(dialog));
            assert!(layout.remove_window(dialog));
            assert_eq!(layout.focused_window(), Some(ids[0]), "the parent, not the most recent other window");
        }

        #[test]
        fn closing_a_window_prefers_the_same_app_then_the_most_recent_one() {
            let mut layout = Layout::new();
            layout.set_policy(NewWindowPolicy::ActiveWorkspace);
            let editor = app_window(&mut layout, "editor");
            let browser = app_window(&mut layout, "browser");
            let terminal = app_window(&mut layout, "terminal");
            let second_editor = app_window(&mut layout, "editor");
            layout.focus_window(editor);
            layout.focus_window(browser);
            layout.focus_window(terminal);
            layout.focus_window(second_editor);
            assert!(layout.remove_window(second_editor));
            assert_eq!(layout.focused_window(), Some(editor), "another window of the same app first");
            assert!(layout.remove_window(editor));
            assert_eq!(layout.focused_window(), Some(terminal), "then the most recently used one");
        }

        #[test]
        fn minimizing_moves_to_the_same_app_and_sends_the_window_to_the_back() {
            let mut layout = Layout::new();
            layout.set_policy(NewWindowPolicy::ActiveWorkspace);
            let first = app_window(&mut layout, "files");
            let other = app_window(&mut layout, "music");
            let second = app_window(&mut layout, "files");
            layout.focus_window(first);
            layout.focus_window(other);
            layout.focus_window(second);
            assert!(layout.set_minimized(second, true));
            assert_eq!(layout.focused_window(), Some(first));
            assert_eq!(layout.recent_windows().last(), Some(&second));
        }

        #[test]
        fn a_modal_child_takes_every_fallback_and_every_focus() {
            let (mut layout, ids) = shared(2);
            let modal = WindowId::next();
            layout.add_window_with_parent(modal, SMALL, ids[0]);
            layout.set_modal(modal, true);
            layout.focus_window(ids[1]);
            assert!(layout.focus_window(ids[0]));
            assert_eq!(layout.focused_window(), Some(modal), "a click on the parent lands on its modal dialog");
            assert_eq!(visible(&layout).last(), Some(&modal), "and the dialog stays above the parent");
            layout.focus_window(ids[1]);
            assert!(layout.remove_window(ids[1]));
            assert_eq!(layout.focused_window(), Some(modal));
        }

        #[test]
        fn an_unmapped_window_gives_up_focus_and_comes_back_when_mapped() {
            let (mut layout, ids) = shared(2);
            layout.focus_window(ids[0]);
            layout.focus_window(ids[1]);
            assert!(layout.unmap_window(ids[1]));
            assert_eq!(layout.focused_window(), Some(ids[0]));
            assert!(!visible(&layout).contains(&ids[1]));
            assert!(!layout.focus_window(ids[1]), "an unmapped window cannot be focused");
            assert!(layout.map_window(ids[1], true));
            assert_eq!(layout.focused_window(), Some(ids[1]));
        }

        #[test]
        fn a_window_mapped_in_the_background_opens_under_the_focused_one() {
            let (mut layout, ids) = shared(2);
            layout.focus_window(ids[0]);
            let late = WindowId::next();
            let ws = layout.admit_window(late, SMALL, None);
            assert_eq!(ws, layout.active_workspace());
            assert_eq!(layout.focused_window(), Some(ids[0]), "admitting is not focusing");
            assert!(layout.map_window(late, false));
            assert_eq!(layout.focused_window(), Some(ids[0]));
            assert_eq!(visible(&layout), [ids[1], late, ids[0]]);
            assert_eq!(layout.recent_windows()[..2], [ids[0], late], "next in line for Alt+Tab");
        }

        #[test]
        fn a_background_window_never_switches_to_the_workspace_made_for_it() {
            let mut layout = Layout::new();
            let first = WindowId::next();
            let home = layout.add_window(first, SIZE);
            let late = WindowId::next();
            let made = layout.admit_window(late, SIZE, None);
            assert_ne!(made, home);
            layout.map_window(late, false);
            assert_eq!(layout.active_workspace(), home);
            assert_eq!(layout.focused_window(), Some(first));
            assert!(layout.focus_window(late));
            assert_eq!(layout.active_workspace(), made);
        }

        #[test]
        fn browsing_workspaces_leaves_the_history_alone_until_committed() {
            let mut layout = Layout::new();
            let (a, b) = (WindowId::next(), WindowId::next());
            layout.add_window(a, SIZE);
            let ws_b = layout.add_window(b, SIZE);
            layout.focus_window(a);
            assert_eq!(layout.recent_windows(), [a, b]);
            layout.open_overview();
            assert!(layout.browse_workspace(WorkspaceTarget::Id(ws_b)));
            assert_eq!(layout.focused_window(), Some(b));
            assert_eq!(layout.recent_windows(), [a, b], "browsing is not choosing");
            layout.close_overview();
            assert_eq!(layout.recent_windows(), [a, b], "a window picked next is the one recorded");
            layout.commit_focus();
            assert_eq!(layout.recent_windows(), [b, a], "closing on the page commits it");
        }

        #[test]
        fn switching_to_a_workspace_focuses_its_most_recent_window() {
            let mut layout = Layout::new();
            layout.set_policy(NewWindowPolicy::ActiveWorkspace);
            let ids = windows(&mut layout, 3, SMALL);
            let home = layout.active_workspace();
            layout.focus_window(ids[1]);
            layout.focus_window(ids[0]);
            assert!(layout.switch_to_workspace(WorkspaceTarget::New));
            assert!(layout.switch_to_workspace(WorkspaceTarget::Id(home)));
            assert_eq!(layout.focused_window(), Some(ids[0]), "not the window on top of the stack");
        }

        #[test]
        fn a_window_removed_from_the_middle_of_the_history_is_forgotten() {
            let (mut layout, ids) = shared(3);
            layout.remove_window(ids[1]);
            assert!(!layout.recent_windows().contains(&ids[1]));
            assert_eq!(layout.recent_windows().len(), 2);
        }
    }
}

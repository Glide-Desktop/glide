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

    fn output(id: OutputId, geometry: Rectangle) -> OutputInfo {
        OutputInfo {
            id,
            geometry,
            work_area: geometry,
        }
    }

    #[test]
    fn test_point_operations() {
        let p = Point::new(10, 20);
        assert_eq!(p.x, 10);
        assert_eq!(p.y, 20);
        assert_eq!(Point::ZERO, Point::new(0, 0));
    }

    #[test]
    fn test_size_operations() {
        let s = Size::new(800, 600);
        assert_eq!(s.width, 800);
        assert_eq!(s.height, 600);
        assert!(!s.is_empty());

        let empty = Size::new(0, 100);
        assert!(empty.is_empty());
    }

    #[test]
    fn test_rectangle_operations() {
        let rect = Rectangle::from_loc_and_size(100, 100, 400, 300);
        assert_eq!(rect.loc, Point::new(100, 100));
        assert_eq!(rect.size, Size::new(400, 300));
        assert_eq!(rect.center(), Point::new(300, 250));

        assert!(rect.contains(Point::new(150, 150)));
        assert!(rect.contains(Point::new(100, 100)));
        assert!(!rect.contains(Point::new(500, 400)));
        assert!(!rect.contains(Point::new(50, 50)));
    }

    #[test]
    fn test_id_uniqueness() {
        let w1 = WindowId::next();
        let w2 = WindowId::next();
        assert_ne!(w1, w2);

        let ws1 = WorkspaceId::next();
        let ws2 = WorkspaceId::next();
        assert_ne!(ws1, ws2);

        let o1 = OutputId::next();
        let o2 = OutputId::next();
        assert_ne!(o1, o2);
    }

    #[test]
    fn test_layout_initial_state() {
        let layout = Layout::new();
        assert_eq!(layout.window_count(), 0);
        assert_eq!(layout.workspace_count(), 1);
        assert_eq!(layout.focused_window(), None);
        assert_eq!(layout.policy(), NewWindowPolicy::NewWorkspace);
    }

    #[test]
    fn test_add_window_new_workspace_policy() {
        let mut layout = Layout::new();
        let w1 = WindowId::next();
        let size = Size::new(1024, 768);

        let ws1 = layout.add_window(w1, size);
        assert_eq!(layout.window_count(), 1);
        assert_eq!(layout.workspace_count(), 1);
        assert_eq!(layout.focused_window(), Some(w1));
        assert!(layout.is_focused(w1));
        assert!(layout.is_visible(w1));

        let w2 = WindowId::next();
        let ws2 = layout.add_window(w2, size);
        assert_ne!(ws1, ws2);
        assert_eq!(layout.window_count(), 2);
        assert_eq!(layout.workspace_count(), 2);
        assert_eq!(layout.focused_window(), Some(w2));
        assert!(layout.is_visible(w2));
        assert!(!layout.is_visible(w1));
    }

    #[test]
    fn test_add_window_active_workspace_policy() {
        let mut layout = Layout::new();
        layout.set_policy(NewWindowPolicy::ActiveWorkspace);

        let w1 = WindowId::next();
        let w2 = WindowId::next();
        let size = Size::new(800, 600);

        let ws1 = layout.add_window(w1, size);
        let ws2 = layout.add_window(w2, size);

        assert_eq!(ws1, ws2);
        assert_eq!(layout.window_count(), 2);
        assert_eq!(layout.workspace_count(), 1);
        assert!(layout.is_visible(w1));
        assert!(layout.is_visible(w2));
    }

    #[test]
    fn test_delayed_placement_zero_size() {
        let mut layout = Layout::new();
        let w1 = WindowId::next();
        layout.add_window(w1, Size::ZERO);

        let state = layout.window_state(w1).unwrap();
        assert!(!state.is_placed);
        assert_eq!(state.geometry.loc, Point::ZERO);

        assert!(layout.update_window_size(w1, Size::new(800, 600)));
        let state = layout.window_state(w1).unwrap();
        assert!(state.is_placed);
        assert_eq!(state.geometry.loc, Point::new(560, 240));
    }

    #[test]
    fn test_centering_on_custom_output() {
        let mut layout = Layout::new();
        let o1 = OutputId::next();
        layout.outputs_changed(&[output(
            o1,
            Rectangle::from_loc_and_size(1000, 0, 2560, 1440),
        )]);

        let w1 = WindowId::next();
        layout.add_window(w1, Size::new(1000, 800));

        let state = layout.window_state(w1).unwrap();
        assert_eq!(state.geometry.loc, Point::new(1780, 320));
    }

    #[test]
    fn test_cascade_offset_for_multiple_windows_same_workspace() {
        let mut layout = Layout::new();
        layout.set_policy(NewWindowPolicy::ActiveWorkspace);

        let w1 = WindowId::next();
        let w2 = WindowId::next();
        let size = Size::new(800, 600);

        layout.add_window(w1, size);
        layout.add_window(w2, size);

        let s1 = layout.window_state(w1).unwrap();
        let s2 = layout.window_state(w2).unwrap();

        assert_eq!(s1.geometry.loc, Point::new(560, 240));
        assert_eq!(s2.geometry.loc, Point::new(560 + 32, 240 + 32));
    }

    #[test]
    fn test_outputs_changed_keeps_windows_that_are_still_on_screen() {
        let mut layout = Layout::new();
        let w1 = WindowId::next();
        layout.add_window(w1, Size::new(800, 600));

        let dragged = Rectangle::from_loc_and_size(40, 50, 800, 600);
        layout.set_window_geometry(w1, dragged);

        let o1 = OutputId::next();
        layout.outputs_changed(&[output(o1, Rectangle::from_loc_and_size(0, 0, 3840, 2160))]);

        assert_eq!(layout.window_state(w1).unwrap().geometry, dragged);
    }

    #[test]
    fn test_outputs_changed_rescues_windows_left_off_screen() {
        let mut layout = Layout::new();
        let w1 = WindowId::next();
        layout.add_window(w1, Size::new(800, 600));

        layout.set_window_geometry(w1, Rectangle::from_loc_and_size(5000, 5000, 800, 600));

        let o1 = OutputId::next();
        layout.outputs_changed(&[output(o1, Rectangle::from_loc_and_size(0, 0, 1920, 1080))]);

        assert_eq!(
            layout.window_state(w1).unwrap().geometry.loc,
            Point::new(560, 240)
        );
    }

    #[test]
    fn test_outputs_changed_resizes_maximized_windows() {
        let mut layout = Layout::new();
        let w1 = WindowId::next();
        layout.add_window(w1, Size::new(800, 600));
        layout.set_maximized(w1, true);

        let o1 = OutputId::next();
        layout.outputs_changed(&[output(o1, Rectangle::from_loc_and_size(0, 0, 3840, 2160))]);

        assert_eq!(
            layout.window_state(w1).unwrap().geometry,
            Rectangle::from_loc_and_size(0, 0, 3840, 2160)
        );
    }

    #[test]
    fn test_visible_windows_are_ordered_bottom_to_top() {
        let mut layout = Layout::new();
        layout.set_policy(NewWindowPolicy::ActiveWorkspace);

        let ids: Vec<WindowId> = (0..8).map(|_| WindowId::next()).collect();
        for &id in &ids {
            layout.add_window(id, Size::new(400, 300));
        }

        for _ in 0..5 {
            let order: Vec<WindowId> = layout.visible_windows().map(|s| s.id).collect();
            assert_eq!(order, ids, "stacking order must not vary between calls");
        }
    }

    #[test]
    fn test_raise_window_moves_it_to_the_top() {
        let mut layout = Layout::new();
        layout.set_policy(NewWindowPolicy::ActiveWorkspace);

        let w1 = WindowId::next();
        let w2 = WindowId::next();
        let w3 = WindowId::next();
        for id in [w1, w2, w3] {
            layout.add_window(id, Size::new(400, 300));
        }

        assert!(layout.raise_window(w1));

        let order: Vec<WindowId> = layout.visible_windows().map(|s| s.id).collect();
        assert_eq!(order, vec![w2, w3, w1]);
    }

    #[test]
    fn test_workspace_remembers_its_own_focus() {
        let mut layout = Layout::new();
        layout.set_policy(NewWindowPolicy::ActiveWorkspace);

        let w1 = WindowId::next();
        let w2 = WindowId::next();
        layout.add_window(w1, Size::new(400, 300));
        layout.add_window(w2, Size::new(400, 300));
        let ws1 = layout.active_workspace();

        layout.focus_window(w1);
        layout.set_policy(NewWindowPolicy::NewWorkspace);
        let w3 = WindowId::next();
        layout.add_window(w3, Size::new(400, 300));

        assert!(layout.switch_to_workspace(WorkspaceTarget::Id(ws1)));

        assert_eq!(layout.focused_window(), Some(w1));
    }

    #[test]
    fn test_focus_window_raises_and_switches_workspace() {
        let mut layout = Layout::new();
        let w1 = WindowId::next();
        let w2 = WindowId::next();

        let ws1 = layout.add_window(w1, Size::new(400, 300));
        layout.add_window(w2, Size::new(400, 300));

        assert!(layout.focus_window(w1));
        assert_eq!(layout.active_workspace(), ws1);
        assert_eq!(layout.focused_window(), Some(w1));
        assert!(layout.is_visible(w1));
    }

    #[test]
    fn test_dialog_on_inactive_workspace_does_not_steal_focus() {
        let mut layout = Layout::new();
        let parent = WindowId::next();
        let ws_parent = layout.add_window(parent, Size::new(800, 600));

        let other = WindowId::next();
        layout.add_window(other, Size::new(800, 600));
        let ws_other = layout.active_workspace();
        assert_ne!(ws_parent, ws_other);

        let dialog = WindowId::next();
        layout.add_window_with_parent(dialog, Size::new(400, 300), parent);

        assert_eq!(layout.active_workspace(), ws_other);
        assert_eq!(layout.focused_window(), Some(other));

        assert!(layout.switch_to_workspace(WorkspaceTarget::Id(ws_parent)));
        assert_eq!(layout.focused_window(), Some(dialog));
    }

    #[test]
    fn test_remove_window() {
        let mut layout = Layout::new();
        let w1 = WindowId::next();
        let w2 = WindowId::next();
        let size = Size::new(800, 600);

        layout.add_window(w1, size);
        layout.add_window(w2, size);
        assert_eq!(layout.window_count(), 2);

        assert!(layout.remove_window(w2));
        assert_eq!(layout.window_count(), 1);
        assert!(!layout.remove_window(w2));
    }

    #[test]
    fn test_dynamic_workspace_destruction_on_remove() {
        let mut layout = Layout::new();
        let w1 = WindowId::next();
        let w2 = WindowId::next();
        let size = Size::new(800, 600);

        layout.add_window(w1, size);
        layout.add_window(w2, size);
        assert_eq!(layout.workspace_count(), 2);

        layout.remove_window(w2);
        assert_eq!(layout.workspace_count(), 1);
        assert_eq!(layout.focused_window(), Some(w1));
        assert!(layout.is_visible(w1));
    }

    #[test]
    fn test_last_workspace_not_destroyed_when_empty() {
        let mut layout = Layout::new();
        let w1 = WindowId::next();
        layout.add_window(w1, Size::new(800, 600));

        assert_eq!(layout.workspace_count(), 1);
        layout.remove_window(w1);
        assert_eq!(layout.window_count(), 0);
        assert_eq!(layout.workspace_count(), 1);
    }

    #[test]
    fn test_switch_workspace_by_index_and_id() {
        let mut layout = Layout::new();
        let w1 = WindowId::next();
        let w2 = WindowId::next();

        let ws1 = layout.add_window(w1, Size::new(800, 600));
        let ws2 = layout.add_window(w2, Size::new(800, 600));

        assert_eq!(layout.active_workspace(), ws2);
        assert!(layout.switch_to_workspace(WorkspaceTarget::Index(0)));
        assert_eq!(layout.active_workspace(), ws1);
        assert_eq!(layout.focused_window(), Some(w1));

        assert!(layout.switch_to_workspace(WorkspaceTarget::Id(ws2)));
        assert_eq!(layout.active_workspace(), ws2);
        assert_eq!(layout.focused_window(), Some(w2));
    }

    #[test]
    fn test_switch_workspace_next_previous() {
        let mut layout = Layout::new();
        let w1 = WindowId::next();
        let w2 = WindowId::next();

        let ws1 = layout.add_window(w1, Size::new(800, 600));
        let ws2 = layout.add_window(w2, Size::new(800, 600));

        assert_eq!(layout.active_workspace(), ws2);
        assert!(layout.switch_to_workspace(WorkspaceTarget::Previous));
        assert_eq!(layout.active_workspace(), ws1);

        assert!(!layout.switch_to_workspace(WorkspaceTarget::Previous));
        assert!(layout.switch_to_workspace(WorkspaceTarget::Next));
        assert_eq!(layout.active_workspace(), ws2);
        assert!(!layout.switch_to_workspace(WorkspaceTarget::Next));
    }

    #[test]
    fn test_move_window_to_workspace() {
        let mut layout = Layout::new();
        layout.set_policy(NewWindowPolicy::ActiveWorkspace);

        let w1 = WindowId::next();
        let w2 = WindowId::next();
        let ws1 = layout.add_window(w1, Size::new(800, 600));
        layout.add_window(w2, Size::new(800, 600));

        layout.set_policy(NewWindowPolicy::NewWorkspace);
        let w3 = WindowId::next();
        let ws2 = layout.add_window(w3, Size::new(800, 600));

        assert!(layout.move_window_to_workspace(w1, WorkspaceTarget::Id(ws2)));
        assert_eq!(layout.window_state(w1).unwrap().workspace_id, ws2);
        assert!(layout.is_visible(w1));

        assert!(layout.move_window_to_workspace(w1, WorkspaceTarget::Id(ws1)));
        assert_eq!(layout.window_state(w1).unwrap().workspace_id, ws1);
        assert!(!layout.is_visible(w1));
    }

    #[test]
    fn test_focus_window() {
        let mut layout = Layout::new();
        let w1 = WindowId::next();
        let w2 = WindowId::next();
        let size = Size::new(800, 600);

        layout.add_window(w1, size);
        layout.add_window(w2, size);
        assert_eq!(layout.focused_window(), Some(w2));

        assert!(layout.focus_window(w1));
        assert_eq!(layout.focused_window(), Some(w1));
        assert!(layout.is_visible(w1));
        assert_eq!(
            layout.active_workspace(),
            layout.window_state(w1).unwrap().workspace_id
        );
    }

    #[test]
    fn test_focus_nonexistent_window() {
        let mut layout = Layout::new();
        let w1 = WindowId::next();
        assert!(!layout.focus_window(w1));
    }

    #[test]
    fn test_visible_windows_iterator() {
        let mut layout = Layout::new();
        layout.set_policy(NewWindowPolicy::ActiveWorkspace);

        let w1 = WindowId::next();
        let w2 = WindowId::next();
        let size = Size::new(800, 600);

        layout.add_window(w1, size);
        layout.add_window(w2, size);

        let visible: Vec<WindowId> = layout.visible_windows().map(|s| s.id).collect();
        assert_eq!(visible.len(), 2);
        assert!(visible.contains(&w1));
        assert!(visible.contains(&w2));
    }

    #[test]
    fn test_add_window_with_parent_same_workspace() {
        let mut layout = Layout::new();
        let parent = WindowId::next();
        let child = WindowId::next();
        let size = Size::new(800, 600);

        let ws_parent = layout.add_window(parent, size);

        let other = WindowId::next();
        let ws_other = layout.add_window(other, size);
        assert_ne!(ws_parent, ws_other);

        let ws_child = layout.add_window_with_parent(child, size, parent);
        assert_eq!(ws_child, ws_parent);
        assert_eq!(layout.window_state(child).unwrap().workspace_id, ws_parent);
    }

    #[test]
    fn test_set_maximized_and_restore() {
        let mut layout = Layout::new();
        let w = WindowId::next();
        let size = Size::new(800, 600);
        layout.add_window(w, size);

        let initial_loc = layout.window_state(w).unwrap().geometry.loc;

        assert!(layout.set_maximized(w, true));
        assert!(layout.window_state(w).unwrap().maximized);
        assert_eq!(
            layout.window_state(w).unwrap().geometry.size,
            Size::new(1920, 1080)
        );

        assert!(layout.set_maximized(w, false));
        assert!(!layout.window_state(w).unwrap().maximized);
        assert_eq!(layout.window_state(w).unwrap().geometry.loc, initial_loc);
        assert_eq!(layout.window_state(w).unwrap().geometry.size, size);
    }

    /// The compositor refuses drag-resizes while a window is maximized or
    /// fullscreen (`GlideState::is_size_locked`). This pins the state those
    /// checks read: a window must report the flag for exactly as long as it
    /// is in that mode, so a resize can never land on a window that still
    /// believes it is maximized — clients draw themselves differently when
    /// they do (GTK squares its corners and drops its shadow).
    #[test]
    fn maximized_and_fullscreen_are_reported_for_exactly_their_duration() {
        let mut layout = Layout::new();
        let w = WindowId::next();
        layout.add_window(w, Size::new(800, 600));

        let locked = |layout: &Layout| {
            let state = layout.window_state(w).unwrap();
            state.maximized || state.fullscreen
        };

        assert!(!locked(&layout), "a fresh window is freely resizable");

        layout.set_maximized(w, true);
        assert!(locked(&layout));
        layout.set_maximized(w, false);
        assert!(!locked(&layout), "unmaximizing must unlock the size again");

        layout.set_fullscreen(w, true);
        assert!(locked(&layout));
        layout.set_fullscreen(w, false);
        assert!(!locked(&layout), "leaving fullscreen must unlock too");
    }

    #[test]
    fn test_set_fullscreen_and_restore() {
        let mut layout = Layout::new();
        let w = WindowId::next();
        let size = Size::new(800, 600);
        layout.add_window(w, size);

        assert!(layout.toggle_fullscreen(w));
        assert!(layout.window_state(w).unwrap().fullscreen);
        assert_eq!(
            layout.window_state(w).unwrap().geometry.size,
            Size::new(1920, 1080)
        );

        assert!(layout.toggle_fullscreen(w));
        assert!(!layout.window_state(w).unwrap().fullscreen);
        assert_eq!(layout.window_state(w).unwrap().geometry.size, size);
    }

    #[test]
    fn test_set_fullscreen_and_restore_when_maximized() {
        let mut layout = Layout::new();
        let w = WindowId::next();
        let size = Size::new(800, 600);
        layout.add_window(w, size);

        assert!(layout.set_maximized(w, true));
        let work_area = layout.primary_output_work_area();
        assert_eq!(layout.window_state(w).unwrap().geometry, work_area);

        assert!(layout.set_fullscreen(w, true));
        let output_geom = layout.primary_output_geometry();
        assert_eq!(layout.window_state(w).unwrap().geometry, output_geom);
        assert!(layout.window_state(w).unwrap().fullscreen);
        assert!(layout.window_state(w).unwrap().maximized);

        assert!(layout.set_fullscreen(w, false));
        assert!(!layout.window_state(w).unwrap().fullscreen);
        assert!(layout.window_state(w).unwrap().maximized);
        assert_eq!(layout.window_state(w).unwrap().geometry, work_area);

        assert!(layout.set_maximized(w, false));
        assert_eq!(layout.window_state(w).unwrap().geometry.size, size);
    }

    #[test]
    fn test_cascade_offset_wrapping_for_many_windows() {
        let mut layout = Layout::new();
        let size = Size::new(400, 300);

        let mut windows = Vec::new();
        for _ in 0..9 {
            let w = WindowId::next();
            layout.add_window(w, size);
            windows.push(w);
        }

        let loc0 = layout.window_state(windows[0]).unwrap().geometry.loc;
        let loc8 = layout.window_state(windows[8]).unwrap().geometry.loc;
        assert_eq!(loc0, loc8);
    }

    #[test]
    fn test_set_window_geometry() {
        let mut layout = Layout::new();
        let w1 = WindowId::next();
        layout.add_window(w1, Size::new(100, 100));

        let new_geom = Rectangle::from_loc_and_size(50, 50, 600, 400);
        assert!(layout.set_window_geometry(w1, new_geom));

        let state = layout.window_state(w1).unwrap();
        assert_eq!(state.geometry, new_geom);
    }

    fn output_with_panel(id: OutputId) -> OutputInfo {
        OutputInfo {
            id,
            geometry: Rectangle::from_loc_and_size(0, 0, 1920, 1080),
            work_area: Rectangle::from_loc_and_size(0, 40, 1920, 1040),
        }
    }

    #[test]
    fn test_new_windows_centre_in_the_work_area_not_the_full_output() {
        let mut layout = Layout::new();
        layout.outputs_changed(&[output_with_panel(OutputId::next())]);

        let w1 = WindowId::next();
        layout.add_window(w1, Size::new(800, 600));

        assert_eq!(
            layout.window_state(w1).unwrap().geometry.loc,
            Point::new(560, 260)
        );
    }

    #[test]
    fn test_maximize_covers_the_work_area_but_fullscreen_covers_the_whole_output() {
        let mut layout = Layout::new();
        layout.outputs_changed(&[output_with_panel(OutputId::next())]);

        let w1 = WindowId::next();
        layout.add_window(w1, Size::new(800, 600));

        assert!(layout.set_maximized(w1, true));
        assert_eq!(
            layout.window_state(w1).unwrap().geometry,
            Rectangle::from_loc_and_size(0, 40, 1920, 1040),
            "maximized must not tuck itself under the panel"
        );

        assert!(layout.toggle_fullscreen(w1));
        assert_eq!(
            layout.window_state(w1).unwrap().geometry,
            Rectangle::from_loc_and_size(0, 0, 1920, 1080),
            "fullscreen must cover the panel, unlike maximize"
        );
    }

    #[test]
    fn test_minimized_window_is_not_visible_and_not_focused() {
        let mut layout = Layout::new();
        layout.set_policy(NewWindowPolicy::ActiveWorkspace);

        let w1 = WindowId::next();
        let w2 = WindowId::next();
        layout.add_window(w1, Size::new(800, 600));
        layout.add_window(w2, Size::new(800, 600));
        assert_eq!(layout.focused_window(), Some(w2));

        assert!(layout.set_minimized(w2, true));

        assert!(!layout.is_visible(w2));
        assert!(layout.is_visible(w1));
        assert_eq!(layout.focused_window(), Some(w1));

        let visible: Vec<WindowId> = layout.visible_windows().map(|s| s.id).collect();
        assert_eq!(visible, vec![w1]);
    }

    #[test]
    fn test_focusing_a_minimized_window_restores_it() {
        let mut layout = Layout::new();
        let w1 = WindowId::next();
        layout.add_window(w1, Size::new(800, 600));
        layout.set_minimized(w1, true);
        assert!(layout.window_state(w1).unwrap().minimized);

        assert!(layout.focus_window(w1));

        assert!(!layout.window_state(w1).unwrap().minimized);
        assert!(layout.is_visible(w1));
        assert_eq!(layout.focused_window(), Some(w1));
    }

    #[test]
    fn test_minimizing_last_visible_window_clears_focus() {
        let mut layout = Layout::new();
        let w1 = WindowId::next();
        layout.add_window(w1, Size::new(800, 600));

        assert!(layout.set_minimized(w1, true));

        assert_eq!(layout.focused_window(), None);
        assert_eq!(layout.visible_windows().count(), 0);
    }

    fn titlebar_insets() -> Insets {
        Insets {
            top: 32,
            left: 0,
            right: 0,
            bottom: 0,
        }
    }

    #[test]
    fn test_undecorated_window_has_identical_frame_and_content() {
        let mut layout = Layout::new();
        let w1 = WindowId::next();
        layout.add_window(w1, Size::ZERO);
        layout.update_window_size(w1, Size::new(800, 600));

        let state = layout.window_state(w1).unwrap();
        assert_eq!(state.frame_geometry(), state.content_geometry());
    }

    #[test]
    fn test_initial_placement_accounts_for_insets() {
        let mut layout = Layout::new();
        let w1 = WindowId::next();
        layout.add_window(w1, Size::ZERO);
        assert!(layout.set_window_insets(w1, titlebar_insets()));
        layout.update_window_size(w1, Size::new(800, 600));

        let state = layout.window_state(w1).unwrap();
        assert_eq!(state.frame_geometry().size, Size::new(800, 632));
        assert_eq!(state.frame_geometry().loc, Point::new(560, 224));
    }

    #[test]
    fn test_content_geometry_is_frame_shrunk_by_insets() {
        let mut layout = Layout::new();
        let w1 = WindowId::next();
        layout.add_window(w1, Size::ZERO);
        layout.set_window_insets(w1, titlebar_insets());
        layout.update_window_size(w1, Size::new(800, 600));

        let state = layout.window_state(w1).unwrap();
        let frame = state.frame_geometry();
        let content = state.content_geometry();

        assert_eq!(content.size, Size::new(800, 600));
        assert_eq!(content.loc, Point::new(frame.loc.x, frame.loc.y + 32));
    }

    #[test]
    fn test_resize_after_placement_keeps_frame_in_sync_with_content() {
        let mut layout = Layout::new();
        let w1 = WindowId::next();
        layout.add_window(w1, Size::ZERO);
        layout.set_window_insets(w1, titlebar_insets());
        layout.update_window_size(w1, Size::new(800, 600));

        layout.update_window_size(w1, Size::new(1000, 700));

        let state = layout.window_state(w1).unwrap();
        assert_eq!(state.content_geometry().size, Size::new(1000, 700));
        assert_eq!(state.frame_geometry().size, Size::new(1000, 732));
    }

    #[test]
    fn test_fullscreen_ignores_insets_for_content_geometry() {
        let mut layout = Layout::new();
        let w1 = WindowId::next();
        layout.add_window(w1, Size::ZERO);
        layout.set_window_insets(w1, titlebar_insets());
        layout.update_window_size(w1, Size::new(800, 600));

        layout.toggle_fullscreen(w1);

        let state = layout.window_state(w1).unwrap();
        assert_eq!(state.content_geometry(), state.frame_geometry());
        assert_eq!(
            state.content_geometry(),
            Rectangle::from_loc_and_size(0, 0, 1920, 1080)
        );
    }

    #[test]
    fn test_set_window_insets_on_nonexistent_window_fails() {
        let mut layout = Layout::new();
        assert!(!layout.set_window_insets(WindowId::next(), titlebar_insets()));
    }

    #[test]
    fn test_changing_insets_after_placement_keeps_content_fixed() {
        let mut layout = Layout::new();
        let w1 = WindowId::next();
        layout.add_window(w1, Size::ZERO);
        layout.set_window_insets(w1, titlebar_insets());
        layout.update_window_size(w1, Size::new(800, 600));

        let content_before = layout.window_state(w1).unwrap().content_geometry();

        layout.set_window_insets(w1, Insets::NONE);
        let state = layout.window_state(w1).unwrap();
        assert_eq!(state.content_geometry(), content_before);
        assert_eq!(
            state.frame_geometry(),
            content_before,
            "with no insets the frame IS the content"
        );

        layout.set_window_insets(w1, titlebar_insets());
        let state = layout.window_state(w1).unwrap();
        assert_eq!(state.content_geometry(), content_before);
        assert_eq!(state.frame_geometry().size, Size::new(800, 632));
    }

    #[test]
    fn test_overview_starts_closed_and_toggles() {
        let mut layout = Layout::new();
        assert!(!layout.overview_active());

        assert!(layout.open_overview());
        assert!(layout.overview_active());
        assert!(!layout.open_overview(), "opening twice reports no change");

        assert!(layout.close_overview());
        assert!(!layout.overview_active());
        assert!(!layout.close_overview(), "closing twice reports no change");
    }

    #[test]
    fn test_opening_overview_does_not_change_focus_or_workspace() {
        let mut layout = Layout::new();
        let w1 = WindowId::next();
        let w2 = WindowId::next();
        layout.add_window(w1, Size::new(800, 600));
        layout.add_window(w2, Size::new(800, 600));

        let workspace_before = layout.active_workspace();
        let focus_before = layout.focused_window();

        layout.open_overview();

        assert_eq!(layout.active_workspace(), workspace_before);
        assert_eq!(layout.focused_window(), focus_before);
    }

    #[test]
    fn test_visible_windows_ignores_the_overview() {
        let mut layout = Layout::new();
        let w1 = WindowId::next();
        let w2 = WindowId::next();
        layout.add_window(w1, Size::new(800, 600));
        layout.add_window(w2, Size::new(800, 600));

        let before: Vec<WindowId> = layout.visible_windows().map(|s| s.id).collect();
        layout.open_overview();
        let during: Vec<WindowId> = layout.visible_windows().map(|s| s.id).collect();

        assert_eq!(before, during);
        assert_eq!(during.len(), 1, "only the active workspace is 'visible'");
    }

    #[test]
    fn test_overview_windows_spans_every_workspace() {
        let mut layout = Layout::new();
        let w1 = WindowId::next();
        let w2 = WindowId::next();
        let w3 = WindowId::next();
        layout.add_window(w1, Size::new(800, 600));
        layout.add_window(w2, Size::new(800, 600));
        layout.add_window(w3, Size::new(800, 600));
        assert_eq!(layout.workspace_count(), 3);

        let ids: Vec<WindowId> = layout.overview_windows().iter().map(|s| s.id).collect();
        assert_eq!(ids, vec![w1, w2, w3], "workspace order, then stacking");
    }

    #[test]
    fn test_overview_windows_skips_minimized() {
        let mut layout = Layout::new();
        let w1 = WindowId::next();
        let w2 = WindowId::next();
        layout.add_window(w1, Size::new(800, 600));
        layout.add_window(w2, Size::new(800, 600));

        layout.set_minimized(w1, true);

        let ids: Vec<WindowId> = layout.overview_windows().iter().map(|s| s.id).collect();
        assert_eq!(ids, vec![w2]);
    }

    #[test]
    fn test_overview_windows_is_empty_with_no_windows() {
        let layout = Layout::new();
        assert!(layout.overview_windows().is_empty());
    }
}

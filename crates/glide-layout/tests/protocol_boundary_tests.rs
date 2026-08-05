use glide_layout::{
    Insets, Layout, NewWindowPolicy, OutputId, OutputInfo, Rectangle, Size, WindowId, WorkspaceId,
    WorkspaceTarget,
};
use std::collections::HashSet;

fn sample_output(id: u64, x: i32, y: i32, w: i32, h: i32) -> OutputInfo {
    OutputInfo {
        id: OutputId::from_u64(id),
        geometry: Rectangle::from_loc_and_size(x, y, w, h),
        work_area: Rectangle::from_loc_and_size(x, y + 32, w, h - 32),
    }
}

#[test]
fn test_rapid_workspace_switching_and_window_movement_stress() {
    let mut layout = Layout::new();
    layout.set_policy(NewWindowPolicy::ActiveWorkspace);
    layout.outputs_changed(&[sample_output(1, 0, 0, 1920, 1080)]);

    // Create 50 windows across multiple workspaces
    let mut windows = Vec::new();
    for _ in 0..50 {
        let w = WindowId::next();
        layout.add_window(w, Size::new(600, 400));
        windows.push(w);
    }

    let workspace_ids = layout.workspace_ids();
    assert!(!workspace_ids.is_empty());

    // Execute 10,000 rapid workspace switches and window moves
    for i in 0..10_000 {
        let w = windows[i % windows.len()];
        let target_ws_idx = (i * 7) % 10;
        let target = WorkspaceTarget::Index(target_ws_idx);

        // Move window to target workspace
        let _ = layout.move_window_to_workspace(w, target);

        // Switch workspace
        match i % 4 {
            0 => {
                let _ = layout.switch_to_workspace(WorkspaceTarget::Index(target_ws_idx));
            }
            1 => {
                let _ = layout.switch_to_workspace(WorkspaceTarget::Next);
            }
            2 => {
                let _ = layout.switch_to_workspace(WorkspaceTarget::Previous);
            }
            3 => {
                let _ =
                    layout.switch_to_workspace(WorkspaceTarget::Id(WorkspaceId::from_u64(999_999)));
            }
            _ => unreachable!(),
        }

        // Verify focus and visible window query invariant on active workspace
        let active = layout.active_workspace();
        assert!(layout.workspace_ids().contains(&active));
        let _ = layout.focused_window();
        let _ = layout.visible_windows().count();
    }
}

#[test]
fn test_empty_workspace_and_window_lifecycle() {
    let mut layout = Layout::new();
    layout.outputs_changed(&[sample_output(1, 0, 0, 1920, 1080)]);

    // Check initial state
    assert_eq!(layout.window_count(), 0);
    assert_eq!(layout.workspace_count(), 1);
    assert_eq!(layout.focused_window(), None);
    assert_eq!(layout.visible_windows().count(), 0);

    // Try resolving invalid targets
    assert_eq!(layout.resolve_target(WorkspaceTarget::Index(999)), None);
    assert_eq!(
        layout.resolve_target(WorkspaceTarget::Id(WorkspaceId::from_u64(99999))),
        None
    );
    assert_eq!(layout.resolve_target(WorkspaceTarget::Next), None);
    assert_eq!(layout.resolve_target(WorkspaceTarget::Previous), None);

    // Add and remove a window
    let w1 = WindowId::next();
    layout.add_window(w1, Size::new(400, 300));
    assert_eq!(layout.window_count(), 1);

    // Minimize and focus interactions
    assert!(layout.set_minimized(w1, true));
    assert_eq!(layout.focused_window(), None);
    assert_eq!(layout.visible_windows().count(), 0);

    // Unminimize via focus_window
    assert!(layout.focus_window(w1));
    assert_eq!(layout.focused_window(), Some(w1));
    assert_eq!(layout.visible_windows().count(), 1);

    // Remove window
    assert!(layout.remove_window(w1));
    assert_eq!(layout.window_count(), 0);
    assert_eq!(layout.workspace_count(), 1); // Last workspace is retained
    assert_eq!(layout.focused_window(), None);
}

#[test]
fn test_outputs_changed_boundary_conditions() {
    let mut layout = Layout::new();
    let w1 = WindowId::next();
    layout.add_window(w1, Size::new(800, 600));
    layout.set_window_insets(
        w1,
        Insets {
            top: 30,
            left: 5,
            right: 5,
            bottom: 5,
        },
    );

    // 1. Pass empty outputs
    layout.outputs_changed(&[]);
    assert_eq!(
        layout.primary_output_geometry(),
        glide_layout::layout::FALLBACK_OUTPUT
    );

    // 2. Pass output that leaves window off-screen to trigger rescue
    let offscreen_output = sample_output(2, 5000, 5000, 1920, 1080);
    layout.outputs_changed(std::slice::from_ref(&offscreen_output));

    let state = layout.window_state(w1).unwrap();
    // Window should be rescued to center of the offscreen output
    assert!(offscreen_output.geometry.overlaps(state.geometry));

    // 3. Maximized & Fullscreen tracking across output changes
    assert!(layout.set_maximized(w1, true));
    let new_output = sample_output(3, 0, 0, 2560, 1440);
    layout.outputs_changed(std::slice::from_ref(&new_output));
    let state_max = layout.window_state(w1).unwrap();
    assert_eq!(state_max.geometry, new_output.work_area);

    assert!(layout.set_fullscreen(w1, true));
    let new_output2 = sample_output(4, 0, 0, 3840, 2160);
    layout.outputs_changed(std::slice::from_ref(&new_output2));
    let state_full = layout.window_state(w1).unwrap();
    assert_eq!(state_full.geometry, new_output2.geometry);
}

#[test]
fn test_multi_surface_idle_inhibit_simulation() {
    struct MockIdleTracker {
        inhibited: bool,
        surfaces: HashSet<u64>,
    }

    impl MockIdleTracker {
        fn new() -> Self {
            Self {
                inhibited: false,
                surfaces: HashSet::new(),
            }
        }

        fn inhibit(&mut self, surface_id: u64) {
            self.surfaces.insert(surface_id);
            self.inhibited = true;
        }

        fn uninhibit(&mut self, surface_id: u64) {
            self.surfaces.remove(&surface_id);
            if self.surfaces.is_empty() {
                self.inhibited = false;
            }
        }
    }

    let mut tracker = MockIdleTracker::new();
    assert!(!tracker.inhibited);

    // 100 surfaces inhibiting
    for id in 1..=100 {
        tracker.inhibit(id);
        assert!(tracker.inhibited);
    }

    // Duplicate inhibit calls on surface 50
    for _ in 0..10 {
        tracker.inhibit(50);
        assert!(tracker.inhibited);
    }

    // Spurious uninhibit calls on surface 999
    for _ in 0..10 {
        tracker.uninhibit(999);
        assert!(tracker.inhibited);
    }

    // Uninhibit 99 surfaces
    for id in 1..100 {
        tracker.uninhibit(id);
        assert!(
            tracker.inhibited,
            "Should remain inhibited until last surface (100) uninhibits"
        );
    }

    // Uninhibit final surface
    tracker.uninhibit(100);
    assert!(
        !tracker.inhibited,
        "Should be uninhibited when surface set becomes empty"
    );
    assert!(tracker.surfaces.is_empty());
}

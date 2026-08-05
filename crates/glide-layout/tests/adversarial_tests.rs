use glide_layout::{
    Layout, NewWindowPolicy, OutputId, OutputInfo, Point, Rectangle, Size, WindowId,
};

fn sample_output() -> OutputInfo {
    OutputInfo {
        id: OutputId::next(),
        geometry: Rectangle::from_loc_and_size(0, 0, 1920, 1080),
        work_area: Rectangle::from_loc_and_size(0, 32, 1920, 1048), // Top bar 32px
    }
}

#[test]
fn test_fullscreen_untoggle_when_maximized_normal_sequence() {
    let mut layout = Layout::new();
    layout.outputs_changed(&[sample_output()]);

    let w1 = WindowId::next();
    layout.add_window(w1, Size::new(800, 600));

    let initial_geom = layout.window_state(w1).unwrap().geometry;

    // 1. Maximize window
    assert!(layout.set_maximized(w1, true));
    let state_max = layout.window_state(w1).unwrap();
    assert!(state_max.maximized);
    assert!(!state_max.fullscreen);
    assert_eq!(
        state_max.geometry,
        Rectangle::from_loc_and_size(0, 32, 1920, 1048)
    );

    // 2. Fullscreen while maximized
    assert!(layout.set_fullscreen(w1, true));
    let state_full = layout.window_state(w1).unwrap();
    assert!(state_full.maximized);
    assert!(state_full.fullscreen);
    assert_eq!(
        state_full.geometry,
        Rectangle::from_loc_and_size(0, 0, 1920, 1080)
    );

    // 3. Un-fullscreen while still maximized
    assert!(layout.set_fullscreen(w1, false));
    let state_unfull = layout.window_state(w1).unwrap();
    assert!(state_unfull.maximized, "Should still be maximized");
    assert!(!state_unfull.fullscreen, "Should no longer be fullscreen");
    assert_eq!(
        state_unfull.geometry,
        Rectangle::from_loc_and_size(0, 32, 1920, 1048),
        "Un-fullscreening a maximized window must return geometry to work_area, not initial_geom"
    );

    // 4. Un-maximize
    assert!(layout.set_maximized(w1, false));
    let state_restored = layout.window_state(w1).unwrap();
    assert!(!state_restored.maximized);
    assert!(!state_restored.fullscreen);
    assert_eq!(
        state_restored.geometry, initial_geom,
        "Un-maximizing must restore to pre-maximized initial geometry"
    );
}

#[test]
fn test_unmaximize_while_fullscreen_then_unfullscreen() {
    let mut layout = Layout::new();
    layout.outputs_changed(&[sample_output()]);

    let w1 = WindowId::next();
    layout.add_window(w1, Size::new(800, 600));

    let initial_geom = layout.window_state(w1).unwrap().geometry;

    // Maximize then Fullscreen
    layout.set_maximized(w1, true);
    layout.set_fullscreen(w1, true);

    // Now un-maximize while in fullscreen mode
    layout.set_maximized(w1, false);
    let state_unmax = layout.window_state(w1).unwrap();
    assert!(!state_unmax.maximized);
    assert!(state_unmax.fullscreen);

    // Now un-fullscreen
    layout.set_fullscreen(w1, false);
    let state_final = layout.window_state(w1).unwrap();
    assert!(!state_final.maximized);
    assert!(!state_final.fullscreen);
    assert_eq!(
        state_final.geometry, initial_geom,
        "Un-fullscreening after un-maximizing while fullscreen must restore to initial geometry"
    );
}

#[test]
fn test_100_plus_window_cascade_wrapping() {
    let mut layout = Layout::new();
    layout.set_policy(NewWindowPolicy::ActiveWorkspace);
    layout.outputs_changed(&[sample_output()]);

    let window_count = 150;
    let mut windows = Vec::with_capacity(window_count);

    for _ in 0..window_count {
        let w = WindowId::next();
        layout.add_window(w, Size::new(400, 300));
        windows.push(w);
    }

    assert_eq!(layout.window_count(), window_count);

    // Check cascade stepping % 8
    // Work area is loc (0, 32), size (1920, 1048)
    // frame_size is (400, 300)
    // base x = 0 + (1920 - 400) / 2 = 760
    // base y = 32 + (1048 - 300) / 2 = 32 + 374 = 406
    for (idx, &w) in windows.iter().enumerate() {
        let state = layout.window_state(w).unwrap();
        let expected_step = i32::try_from(idx % 8).unwrap();
        let expected_offset = expected_step * 32;
        let expected_loc = Point::new(760 + expected_offset, 406 + expected_offset);

        assert_eq!(
            state.geometry.loc, expected_loc,
            "Window index {idx} offset mismatch"
        );
    }

    // Verify visible windows iterator handles 150 windows correctly
    let visible_count = layout.visible_windows().count();
    assert_eq!(visible_count, window_count);

    // Remove windows and ensure no panics/leaks
    for w in windows {
        assert!(layout.remove_window(w));
    }
    assert_eq!(layout.window_count(), 0);
}

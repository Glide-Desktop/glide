#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    dead_code,
    clippy::float_cmp,
    clippy::uninlined_format_args,
    clippy::unchecked_time_subtraction,
    clippy::similar_names,
    clippy::unreadable_literal
)]

#[path = "../src/gestures.rs"]
mod gestures;

use gestures::*;
use glide_layout::WorkspaceTarget;
use std::time::{Duration, Instant};

#[test]
fn test_challenge1_rubber_banding_strictly_bounded_below_quarter_screen() {
    let screen_widths = [640.0, 800.0, 1024.0, 1280.0, 1366.0, 1920.0, 2560.0, 3840.0, 5120.0, 7680.0];

    for &sw in &screen_widths {
        let max_bound = 0.25 * sw;
        let mut prev_eff = 0.0;

        let mut dx = 0.0;
        while dx <= 100_000.0 {
            let eff = calculate_rubber_band(dx, sw);
            assert!(eff < max_bound, "eff {eff} must be strictly < max_bound {max_bound}");
            assert!(eff >= 0.0);
            if dx > 0.0 {
                assert!(eff > prev_eff, "rubber banding must be strictly monotonic");
            }
            let p = normalized_progress(eff, sw);
            assert!(p.abs() < 0.25, "normalized progress |p| {} must be strictly < 0.25", p.abs());
            assert!(0.35 - p.abs() > 0.10, "safety margin to 0.35 commit threshold must be > 0.10");

            let neg_eff = calculate_rubber_band(-dx, sw);
            assert!(neg_eff > -max_bound);
            assert!(neg_eff <= 0.0);
            let neg_p = normalized_progress(neg_eff, sw);
            assert!(neg_p.abs() < 0.25);

            prev_eff = eff;
            if dx == 0.0 {
                dx = 0.1;
            } else if dx < 1.0 {
                dx = 1.0;
            } else if dx < 1000.0 {
                dx += 10.0;
            } else {
                dx += 1000.0;
            }
        }

        for &extreme_dx in &[1_000_000.0, 10_000_000.0, 1_000_000_000.0] {
            let eff = calculate_rubber_band(extreme_dx, sw);
            assert!(eff < max_bound);
            assert!(max_bound - eff > 0.0);
            let p = normalized_progress(eff, sw);
            assert!(p.abs() < 0.25);
        }
    }
}

#[test]
fn test_challenge1_boundary_resistance_derivative_diminishes() {
    let sw = 1920.0;
    let step = 1.0;
    let mut prev_slope = 1.0;

    let mut dx = 0.0;
    while dx < 10_000.0 {
        let eff1 = calculate_rubber_band(dx, sw);
        let eff2 = calculate_rubber_band(dx + step, sw);
        let slope = (eff2 - eff1) / step;

        assert!(slope > 0.0, "slope must be positive");
        assert!(slope <= prev_slope + 1e-9, "resistance must increase as dx increases");
        prev_slope = slope;
        dx += 50.0;
    }
}

#[test]
fn test_challenge1_accidental_boundary_commit_impossible_at_extremes() {
    let sw = 1920.0;
    let total_ws = 4;

    for &massive_dx in &[500.0, 1000.0, 5000.0, 50000.0, 100000.0] {
        let eff_left = effective_displacement(0, total_ws, massive_dx, sw);
        let p_left = normalized_progress(eff_left, sw);
        let settle_left = decide_workspace_settlement(0, total_ws, p_left, 0.0, false);
        assert_eq!(settle_left.target, None);
        assert_eq!(settle_left.target_progress, 0.0);

        let eff_right = effective_displacement(total_ws - 1, total_ws, -massive_dx, sw);
        let p_right = normalized_progress(eff_right, sw);
        let settle_right = decide_workspace_settlement(total_ws - 1, total_ws, p_right, 0.0, false);
        assert_eq!(settle_right.target, None);
        assert_eq!(settle_right.target_progress, 0.0);

        let settle_left_flick = decide_workspace_settlement(0, total_ws, p_left, -2.0, false);
        assert_eq!(settle_left_flick.target, None);

        let settle_right_flick = decide_workspace_settlement(total_ws - 1, total_ws, p_right, 2.0, false);
        assert_eq!(settle_right_flick.target, None);
    }
}

#[test]
fn test_challenge1_zero_and_negative_screen_width_safeguard() {
    assert_eq!(calculate_rubber_band(100.0, 0.0), 0.0);
    assert_eq!(calculate_rubber_band(100.0, -1920.0), 0.0);
    assert_eq!(normalized_progress(100.0, 0.0), 0.0);
    assert_eq!(normalized_progress(100.0, -1920.0), 0.0);
    assert_eq!(normalized_velocity_x(500.0, 0.0), 0.0);
    assert_eq!(normalized_velocity_x(500.0, -1920.0), 0.0);
}

#[test]
fn test_challenge2_threshold_arbitration_critical_34_vs_36() {
    let total_ws = 5;
    let active_ws = 2;

    let s_forward_below = decide_workspace_settlement(active_ws, total_ws, 0.34, 0.0, false);
    assert_eq!(s_forward_below.target, None);
    assert_eq!(s_forward_below.target_progress, 0.0);

    let s_forward_above = decide_workspace_settlement(active_ws, total_ws, 0.36, 0.0, false);
    assert_eq!(s_forward_above.target, Some(WorkspaceTarget::Next));
    assert_eq!(s_forward_above.target_progress, 1.0);

    let s_backward_below = decide_workspace_settlement(active_ws, total_ws, -0.34, 0.0, false);
    assert_eq!(s_backward_below.target, None);
    assert_eq!(s_backward_below.target_progress, 0.0);

    let s_backward_above = decide_workspace_settlement(active_ws, total_ws, -0.36, 0.0, false);
    assert_eq!(s_backward_above.target, Some(WorkspaceTarget::Previous));
    assert_eq!(s_backward_above.target_progress, -1.0);

    let s_exact_threshold = decide_workspace_settlement(active_ws, total_ws, 0.35, 0.0, false);
    assert_eq!(s_exact_threshold.target, None);

    let s_exact_neg_threshold = decide_workspace_settlement(active_ws, total_ws, -0.35, 0.0, false);
    assert_eq!(s_exact_neg_threshold.target, None);

    let s_just_above = decide_workspace_settlement(active_ws, total_ws, 0.35001, 0.0, false);
    assert_eq!(s_just_above.target, Some(WorkspaceTarget::Next));

    let s_just_below = decide_workspace_settlement(active_ws, total_ws, 0.34999, 0.0, false);
    assert_eq!(s_just_below.target, None);
}

#[test]
fn test_challenge2_threshold_arbitration_cancelled_flag_overrides() {
    let s_cancelled_fwd = decide_workspace_settlement(2, 5, 0.80, 2.0, true);
    assert_eq!(s_cancelled_fwd.target, None);
    assert_eq!(s_cancelled_fwd.target_progress, 0.0);

    let s_cancelled_bwd = decide_workspace_settlement(2, 5, -0.80, -2.0, true);
    assert_eq!(s_cancelled_bwd.target, None);
    assert_eq!(s_cancelled_bwd.target_progress, 0.0);
}

#[test]
fn test_challenge2_threshold_arbitration_outer_boundary_clamping() {
    let s_start_bound = decide_workspace_settlement(0, 4, -0.50, -1.0, false);
    assert_eq!(s_start_bound.target, None);

    let s_end_bound = decide_workspace_settlement(3, 4, 0.50, 1.0, false);
    assert_eq!(s_end_bound.target, None);

    let s_single_ws_fwd = decide_workspace_settlement(0, 1, 0.50, 1.0, false);
    assert_eq!(s_single_ws_fwd.target, None);

    let s_single_ws_bwd = decide_workspace_settlement(0, 1, -0.50, -1.0, false);
    assert_eq!(s_single_ws_bwd.target, None);

    let s_zero_ws = decide_workspace_settlement(0, 0, 0.50, 1.0, false);
    assert_eq!(s_zero_ws.target, None);
}

#[test]
fn test_challenge3_velocity_tracking_direction_agreement_truth_table() {
    let total_ws = 4;
    let active_ws = 1;

    let s_fwd_agree = decide_workspace_settlement(active_ws, total_ws, 0.15, 0.55, false);
    assert_eq!(s_fwd_agree.target, Some(WorkspaceTarget::Next));

    let s_fwd_opp = decide_workspace_settlement(active_ws, total_ws, 0.15, -0.55, false);
    assert_eq!(s_fwd_opp.target, None);

    let s_bwd_agree = decide_workspace_settlement(active_ws, total_ws, -0.15, -0.55, false);
    assert_eq!(s_bwd_agree.target, Some(WorkspaceTarget::Previous));

    let s_bwd_opp = decide_workspace_settlement(active_ws, total_ws, -0.15, 0.55, false);
    assert_eq!(s_bwd_opp.target, None);

    let s_zero_p_pos_v = decide_workspace_settlement(active_ws, total_ws, 0.0, 1.5, false);
    assert_eq!(s_zero_p_pos_v.target, None);

    let s_zero_p_neg_v = decide_workspace_settlement(active_ws, total_ws, 0.0, -1.5, false);
    assert_eq!(s_zero_p_neg_v.target, None);

    let s_vel_boundary_exact = decide_workspace_settlement(active_ws, total_ws, 0.15, 0.50, false);
    assert_eq!(s_vel_boundary_exact.target, None);

    let s_vel_boundary_just_above = decide_workspace_settlement(active_ws, total_ws, 0.15, 0.50001, false);
    assert_eq!(s_vel_boundary_just_above.target, Some(WorkspaceTarget::Next));

    let s_low_vel = decide_workspace_settlement(active_ws, total_ws, 0.15, 0.49, false);
    assert_eq!(s_low_vel.target, None);
}

#[test]
fn test_challenge3_velocity_tracker_sliding_window_and_eviction() {
    let mut tracker = VelocityTracker::new(Duration::from_millis(100));
    let t0 = Instant::now();

    assert_eq!(tracker.velocity_x(), 0.0);

    tracker.push(t0, -10.0, 0.0);
    assert_eq!(tracker.velocity_x(), 0.0);

    let t1 = t0 + Duration::from_millis(10);
    tracker.push(t1, -10.0, 0.0);
    assert_eq!(tracker.velocity_x(), 0.0);

    let t2 = t0 + Duration::from_millis(20);
    tracker.push(t2, -20.0, 0.0);
    let vx = tracker.velocity_x();
    assert!(vx < 0.0);

    let t3 = t0 + Duration::from_millis(150);
    tracker.push(t3, 50.0, 0.0);
    let t4 = t0 + Duration::from_millis(170);
    tracker.push(t4, 50.0, 0.0);

    let vx_after = tracker.velocity_x();
    assert!(vx_after > 0.0, "velocity must reflect recent window, not expired past");

    tracker.reset();
    assert_eq!(tracker.velocity_x(), 0.0);
}

#[test]
fn test_challenge4_workspace_swipe_state_machine_full_lifecycle() {
    let sw = 1920.0;
    let mut state = WorkspaceSwipeState::new(1, 3, sw);

    state.update(-0.2 * SWIPE_DISTANCE);
    assert!((state.current_progress() - 0.20).abs() < 1e-6);

    state.update(-0.2 * SWIPE_DISTANCE);
    assert!((state.current_progress() - 0.40).abs() < 1e-6);

    let settle = decide_workspace_settlement(
        state.active_workspace,
        state.total_workspaces,
        state.current_progress(),
        0.0,
        false,
    );
    assert_eq!(settle.target, Some(WorkspaceTarget::Next));

    state.phase = WorkspaceSwipePhase::Settling {
        started: Instant::now().checked_sub(Duration::from_millis(500)).unwrap(),
        release: glide_theme::motion::Release { duration: Duration::from_millis(200), slope: 0.0 },
        from_progress: state.current_progress(),
        target_progress: settle.target_progress,
        target_workspace: settle.target,
    };

    assert!((state.current_progress() - 1.0).abs() < 1e-6);
}

#[test]
fn test_challenge4_workspace_swipe_rubber_band_direction_reversal() {
    let sw = 1920.0;
    let mut state = WorkspaceSwipeState::new(0, 3, sw);

    state.update(-300.0);
    assert!(!is_at_boundary(0, 3, -300.0));
    assert!((state.effective_x() + 300.0 / SWIPE_DISTANCE * sw).abs() < 1e-6);

    state.update(800.0);
    assert!(state.raw_delta_x > 0.0);
    assert!(is_at_boundary(0, 3, state.raw_delta_x));
    assert!(state.effective_x() < 0.25 * sw);
    assert!(state.current_progress().abs() < 0.25);
}

#[test]
fn test_challenge4_axis_hysteresis_lock_in() {
    let mut tracker = TouchpadGestureTracker::new(SHELL_DESKTOP);
    let t = Instant::now();

    tracker.update(t, 6.0, 0.0);
    assert_eq!(tracker.kind, GestureKind::Undecided);

    tracker.update(t, 7.0, 0.0);
    assert_eq!(tracker.kind, GestureKind::Workspace);

    tracker.update(t, 0.0, 100.0);
    assert_eq!(tracker.kind, GestureKind::Workspace);
}

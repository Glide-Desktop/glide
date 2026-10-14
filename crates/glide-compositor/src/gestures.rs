use std::collections::VecDeque;
use std::time::{Duration, Instant};

use glide_ipc::{GestureAction, GesturePhase, GestureSample};
use glide_layout::WorkspaceTarget;
use glide_theme::motion::Release;
use serde::Deserialize;

#[allow(clippy::struct_excessive_bools)] // Independent [gestures] switches mirrored from glide.toml
#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct GestureConfig {
    pub enabled: bool,
    pub fingers: u32,
    pub workspace_swipe: bool,
    pub overview_swipe: bool,
    pub app_swipe: bool,
    pub pinch: bool,
    pub touch_edges: bool,
    pub natural: bool,
    pub sensitivity: f64,
}

pub const GESTURE_FINGERS: std::ops::RangeInclusive<u32> = 3..=4;
pub const GESTURE_SENSITIVITY: std::ops::RangeInclusive<f64> = 0.5..=2.0;
pub const SWIPE_DISTANCE: f64 = 400.0;
pub const OVERVIEW_DISTANCE: f64 = 300.0;
pub const APP_STEP: f64 = 100.0;
pub const PINCH_FINGERS: u32 = 3;

impl Default for GestureConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            fingers: 3,
            workspace_swipe: true,
            overview_swipe: true,
            app_swipe: true,
            pinch: true,
            touch_edges: true,
            natural: true,
            sensitivity: 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwipeRole {
    Shell,
    Windows,
}

impl GestureConfig {
    #[must_use]
    pub fn fingers(&self) -> u32 {
        self.fingers.clamp(*GESTURE_FINGERS.start(), *GESTURE_FINGERS.end())
    }

    #[must_use]
    pub fn window_fingers(&self) -> u32 {
        if self.fingers() == 3 { 4 } else { 3 }
    }

    #[must_use]
    pub fn role(&self, fingers: u32) -> Option<SwipeRole> {
        if !self.enabled {
            None
        } else if fingers == self.fingers() && (self.workspace_swipe || self.overview_swipe) {
            Some(SwipeRole::Shell)
        } else if fingers == self.window_fingers() && self.app_swipe {
            Some(SwipeRole::Windows)
        } else {
            None
        }
    }

    #[must_use]
    pub fn claims(&self, fingers: u32) -> bool {
        self.role(fingers).is_some()
    }

    #[must_use]
    pub fn claims_pinch(&self, fingers: u32) -> bool {
        self.enabled && self.pinch && fingers >= PINCH_FINGERS
    }

    #[must_use]
    pub fn delta(&self, dx: f64, dy: f64) -> (f64, f64) {
        let sensitivity = if self.sensitivity.is_finite() {
            self.sensitivity.clamp(*GESTURE_SENSITIVITY.start(), *GESTURE_SENSITIVITY.end())
        } else {
            1.0
        };
        let factor = if self.natural { sensitivity } else { -sensitivity };
        (dx * factor, dy * factor)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SwipeTarget {
    #[default]
    None,
    Compositor,
    Client,
    Practice,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GestureKind {
    Undecided,
    Workspace,
    Overview,
}

#[derive(Debug, Clone, Copy)]
pub struct VelocitySample {
    pub timestamp: Instant,
    pub delta_x: f64,
    pub delta_y: f64,
}

#[derive(Debug)]
pub struct VelocityTracker {
    samples: VecDeque<VelocitySample>,
    window_duration: Duration,
}

impl Default for VelocityTracker {
    fn default() -> Self {
        Self::new(Duration::from_millis(100))
    }
}

impl VelocityTracker {
    #[must_use]
    pub fn new(window_duration: Duration) -> Self {
        Self { samples: VecDeque::with_capacity(16), window_duration }
    }

    pub fn push(&mut self, now: Instant, delta_x: f64, delta_y: f64) {
        self.samples.push_back(VelocitySample { timestamp: now, delta_x, delta_y });
        while self.samples.front().is_some_and(|front| now.duration_since(front.timestamp) > self.window_duration) {
            self.samples.pop_front();
        }
    }

    fn velocity(&self, delta: impl Fn(&VelocitySample) -> f64) -> f64 {
        let (Some(oldest), Some(newest)) = (self.samples.front(), self.samples.back()) else {
            return 0.0;
        };
        let dt = newest.timestamp.duration_since(oldest.timestamp).as_secs_f64();
        if self.samples.len() < 2 || dt < 0.015 {
            return 0.0;
        }
        self.samples.iter().map(delta).sum::<f64>() / dt
    }

    #[must_use]
    pub fn velocity_x(&self) -> f64 {
        self.velocity(|sample| sample.delta_x)
    }

    #[must_use]
    pub fn velocity_y(&self) -> f64 {
        self.velocity(|sample| sample.delta_y)
    }

    pub fn reset(&mut self) {
        self.samples.clear();
    }
}

#[must_use]
pub fn calculate_rubber_band(delta_x: f64, screen_width: f64) -> f64 {
    let limit = 0.25 * screen_width;
    if limit <= 0.0 {
        return 0.0;
    }
    delta_x / (1.0 + delta_x.abs() / limit)
}

#[must_use]
pub fn is_at_boundary(active_workspace: usize, total_workspaces: usize, delta_x: f64) -> bool {
    (active_workspace == 0 && delta_x > 0.0) || (active_workspace + 1 >= total_workspaces && delta_x < 0.0)
}

#[must_use]
pub fn effective_displacement(
    active_workspace: usize, total_workspaces: usize, delta_x: f64, screen_width: f64,
) -> f64 {
    if is_at_boundary(active_workspace, total_workspaces, delta_x) {
        calculate_rubber_band(delta_x, screen_width)
    } else {
        delta_x
    }
}

#[must_use]
pub fn raw_displacement(active_workspace: usize, total_workspaces: usize, effective_x: f64, span: f64) -> f64 {
    let limit = 0.25 * span;
    if limit > 0.0 && is_at_boundary(active_workspace, total_workspaces, effective_x) {
        effective_x / (1.0 - (effective_x.abs() / limit).min(0.99))
    } else {
        effective_x
    }
}

#[must_use]
pub fn normalized_progress(effective_x: f64, screen_width: f64) -> f64 {
    if screen_width <= 0.0 { 0.0 } else { -effective_x / screen_width }
}

#[must_use]
pub fn normalized_velocity_x(velocity_x: f64, screen_width: f64) -> f64 {
    normalized_progress(velocity_x, screen_width)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorkspaceSettlement {
    pub target: Option<WorkspaceTarget>,
    pub target_progress: f64,
    pub remaining_distance: f64,
    pub release: Release,
}

#[must_use]
pub fn decide_workspace_settlement(
    active_workspace: usize, total_workspaces: usize, progress: f64, velocity_p: f64, cancelled: bool,
) -> WorkspaceSettlement {
    let has_target = if progress > 0.0 {
        active_workspace + 1 < total_workspaces
    } else if progress < 0.0 {
        active_workspace > 0
    } else {
        false
    };

    let same_sign = (velocity_p > 0.0 && progress > 0.0) || (velocity_p < 0.0 && progress < 0.0);
    let commit = !cancelled && has_target && (progress.abs() > 0.35 || (velocity_p.abs() > 0.5 && same_sign));

    let (target, target_progress) = if commit {
        if progress > 0.0 { (Some(WorkspaceTarget::Next), 1.0) } else { (Some(WorkspaceTarget::Previous), -1.0) }
    } else {
        (None, 0.0)
    };

    let remaining_distance = (target_progress - progress).abs().clamp(0.0, 1.0);
    WorkspaceSettlement {
        target,
        target_progress,
        remaining_distance,
        release: Release::new(target_progress - progress, velocity_p, glide_theme::motion::t3()),
    }
}

#[must_use]
pub fn swipe_workspace_target(
    target: WorkspaceTarget, active_workspace: usize, existing_workspaces: usize,
) -> WorkspaceTarget {
    if target == WorkspaceTarget::Next && active_workspace + 1 >= existing_workspaces {
        WorkspaceTarget::New
    } else {
        target
    }
}

pub const SHELL_DESKTOP: f64 = 0.0;
pub const SHELL_OVERVIEW: f64 = 1.0;
pub const SHELL_LAUNCHER: f64 = 2.0;

#[must_use]
pub fn shell_position(base: f64, delta_y: f64) -> f64 {
    (base - delta_y / OVERVIEW_DISTANCE).clamp(SHELL_DESKTOP, SHELL_LAUNCHER)
}

#[must_use]
pub fn decide_shell_settlement(base: f64, position: f64, velocity_up: f64, cancelled: bool) -> f64 {
    if cancelled {
        return base.round().clamp(SHELL_DESKTOP, SHELL_LAUNCHER);
    }
    let floor = position.floor().min(SHELL_LAUNCHER - 1.0);
    let fraction = position - floor;
    let up = if velocity_up > 0.5 {
        true
    } else if velocity_up < -0.5 {
        false
    } else if base <= floor {
        fraction > 0.35
    } else if base >= floor + 1.0 {
        fraction >= 0.65
    } else {
        fraction >= 0.5
    };
    if up { floor + 1.0 } else { floor }
}

#[must_use]
pub fn overview_velocity_up(velocity_y: f64) -> f64 {
    -velocity_y / OVERVIEW_DISTANCE
}

#[must_use]
pub fn app_switch_offset(accum_dx: f64) -> i32 {
    (accum_dx / APP_STEP).round() as i32
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowSwipe {
    Maximize,
    Restore,
    Minimize,
}

#[must_use]
pub fn decide_window_swipe(accum_dy: f64, velocity_up: f64, maximized: bool, cancelled: bool) -> Option<WindowSwipe> {
    let travel = -accum_dy / OVERVIEW_DISTANCE;
    if cancelled {
        return None;
    }
    let up = travel > 0.35 || (travel > 0.0 && velocity_up > 0.5);
    let down = travel < -0.35 || (travel < 0.0 && velocity_up < -0.5);
    match (up, down, maximized) {
        (true, _, false) => Some(WindowSwipe::Maximize),
        (_, true, true) => Some(WindowSwipe::Restore),
        (_, true, false) => Some(WindowSwipe::Minimize),
        _ => None,
    }
}

pub const TOUCH_EDGE: f64 = 16.0;
pub const TOUCH_PULL: f64 = 48.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TouchEdge {
    Top,
    Bottom,
    Left,
    Right,
}

#[must_use]
pub fn touch_edge(origin: (f64, f64), size: (f64, f64), at: (f64, f64)) -> Option<TouchEdge> {
    let (x, y) = (at.0 - origin.0, at.1 - origin.1);
    let (w, h) = size;
    if !(0.0..=w).contains(&x) || !(0.0..=h).contains(&y) {
        None
    } else if h - y <= TOUCH_EDGE {
        Some(TouchEdge::Bottom)
    } else if y <= TOUCH_EDGE {
        Some(TouchEdge::Top)
    } else if x <= TOUCH_EDGE {
        Some(TouchEdge::Left)
    } else if w - x <= TOUCH_EDGE {
        Some(TouchEdge::Right)
    } else {
        None
    }
}

#[must_use]
pub fn touch_units(edge: TouchEdge, dx: f64, dy: f64, size: (f64, f64)) -> (f64, f64) {
    match edge {
        TouchEdge::Left | TouchEdge::Right if size.0 > 0.0 => (dx * SWIPE_DISTANCE / size.0, 0.0),
        TouchEdge::Bottom if size.1 > 0.0 => (0.0, dy * OVERVIEW_DISTANCE / (0.5 * size.1)),
        _ => (0.0, 0.0),
    }
}

#[derive(Debug)]
pub struct EdgeSwipe {
    pub slot: smithay::backend::input::TouchSlot,
    pub edge: TouchEdge,
    pub origin: (f64, f64),
    pub size: (f64, f64),
    pub start: (f64, f64),
    pub last: (f64, f64),
    pub claimed: bool,
}

impl EdgeSwipe {
    #[must_use]
    pub fn pulled_by(&self, distance: f64) -> bool {
        let (dx, dy) = (self.last.0 - self.start.0, self.last.1 - self.start.1);
        match self.edge {
            TouchEdge::Top => dy >= distance,
            TouchEdge::Bottom => dy <= -distance,
            TouchEdge::Left => dx >= distance,
            TouchEdge::Right => dx <= -distance,
        }
    }

    #[must_use]
    pub fn pulled(&self) -> bool {
        self.pulled_by(TOUCH_PULL)
    }

    #[must_use]
    pub fn left_half(&self) -> bool {
        self.start.0 - self.origin.0 < self.size.0 / 2.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinchAction {
    Launcher,
    Desktop,
}

#[must_use]
pub fn pinch_progress(scale: f64) -> f64 {
    ((1.0 - scale) / 0.4).clamp(-1.0, 1.0)
}

#[must_use]
pub fn decide_pinch(scale: f64, cancelled: bool) -> Option<PinchAction> {
    let progress = pinch_progress(scale);
    if cancelled {
        None
    } else if progress > 0.35 {
        Some(PinchAction::Launcher)
    } else if progress < -0.35 {
        Some(PinchAction::Desktop)
    } else {
        None
    }
}

#[derive(Debug)]
pub struct TouchpadGestureTracker {
    pub kind: GestureKind,
    pub role: SwipeRole,
    pub shell_base: f64,
    pub accum_dx: f64,
    pub accum_dy: f64,
    pub velocity: VelocityTracker,
    epoch: Option<(u64, Instant)>,
}

impl TouchpadGestureTracker {
    #[must_use]
    pub fn new(shell_base: f64) -> Self {
        Self {
            kind: GestureKind::Undecided,
            role: SwipeRole::Shell,
            shell_base: shell_base.clamp(SHELL_DESKTOP, SHELL_LAUNCHER),
            accum_dx: 0.0,
            accum_dy: 0.0,
            velocity: VelocityTracker::default(),
            epoch: None,
        }
    }

    #[must_use]
    pub fn shell_position(&self) -> f64 {
        shell_position(self.shell_base, self.accum_dy)
    }

    #[must_use]
    pub fn shell_target(&self, cancelled: bool) -> f64 {
        decide_shell_settlement(self.shell_base, self.shell_position(), self.overview_velocity(), cancelled)
    }

    #[must_use]
    pub fn workspace_velocity(&self) -> f64 {
        normalized_velocity_x(self.velocity.velocity_x(), SWIPE_DISTANCE)
    }

    #[must_use]
    pub fn overview_velocity(&self) -> f64 {
        overview_velocity_up(self.velocity.velocity_y())
    }

    pub fn instant_for(&mut self, event_micros: u64) -> Instant {
        let (base_micros, base) = *self.epoch.get_or_insert((event_micros, Instant::now()));
        base + Duration::from_micros(event_micros.saturating_sub(base_micros))
    }

    pub fn update(&mut self, now: Instant, dx: f64, dy: f64) {
        self.accum_dx += dx;
        self.accum_dy += dy;
        self.velocity.push(now, dx, dy);

        if self.kind == GestureKind::Undecided {
            let dist = (self.accum_dx * self.accum_dx + self.accum_dy * self.accum_dy).sqrt();
            if dist >= 12.0 {
                self.kind = if self.accum_dx.abs() > self.accum_dy.abs() {
                    GestureKind::Workspace
                } else {
                    GestureKind::Overview
                };
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WorkspaceSwipePhase {
    Tracking {
        current_progress: f64,
        effective_x: f64,
    },
    Settling {
        started: Instant,
        release: Release,
        from_progress: f64,
        target_progress: f64,
        target_workspace: Option<WorkspaceTarget>,
    },
}

#[derive(Debug)]
pub struct WorkspaceSwipeState {
    pub active_workspace: usize,
    pub total_workspaces: usize,
    pub screen_width: f64,
    pub raw_delta_x: f64,
    pub phase: WorkspaceSwipePhase,
}

impl WorkspaceSwipeState {
    #[must_use]
    pub fn new(active_workspace: usize, total_workspaces: usize, screen_width: f64) -> Self {
        Self {
            active_workspace,
            total_workspaces,
            screen_width,
            raw_delta_x: 0.0,
            phase: WorkspaceSwipePhase::Tracking { current_progress: 0.0, effective_x: 0.0 },
        }
    }

    pub fn update(&mut self, dx: f64) {
        self.raw_delta_x = (self.raw_delta_x + dx).clamp(-4.0 * SWIPE_DISTANCE, 4.0 * SWIPE_DISTANCE);
        if !is_at_boundary(self.active_workspace, self.total_workspaces, self.raw_delta_x) {
            self.raw_delta_x = self.raw_delta_x.clamp(-SWIPE_DISTANCE, SWIPE_DISTANCE);
        }
        let eff =
            effective_displacement(self.active_workspace, self.total_workspaces, self.raw_delta_x, SWIPE_DISTANCE);
        let progress = normalized_progress(eff, SWIPE_DISTANCE);
        self.phase =
            WorkspaceSwipePhase::Tracking { current_progress: progress, effective_x: -progress * self.screen_width };
    }

    pub fn settle(&mut self, velocity: f64, cancelled: bool) -> WorkspaceSettlement {
        let progress = self.current_progress();
        let settlement =
            decide_workspace_settlement(self.active_workspace, self.total_workspaces, progress, velocity, cancelled);
        self.phase = WorkspaceSwipePhase::Settling {
            started: Instant::now(),
            release: settlement.release,
            from_progress: progress,
            target_progress: settlement.target_progress,
            target_workspace: settlement.target,
        };
        settlement
    }

    pub fn catch(&mut self) -> bool {
        if matches!(self.phase, WorkspaceSwipePhase::Tracking { .. }) {
            return false;
        }
        let effective = -self.current_progress() * SWIPE_DISTANCE;
        self.raw_delta_x = raw_displacement(self.active_workspace, self.total_workspaces, effective, SWIPE_DISTANCE);
        self.update(0.0);
        true
    }

    #[must_use]
    pub fn effective_x(&self) -> f64 {
        match self.phase {
            WorkspaceSwipePhase::Tracking { effective_x, .. } => effective_x,
            WorkspaceSwipePhase::Settling { .. } => -self.current_progress() * self.screen_width,
        }
    }

    #[must_use]
    pub fn current_progress(&self) -> f64 {
        match self.phase {
            WorkspaceSwipePhase::Tracking { current_progress, .. } => current_progress,
            WorkspaceSwipePhase::Settling { started, release, from_progress, target_progress, .. } => {
                if release.duration.is_zero() {
                    target_progress
                } else {
                    let elapsed = started.elapsed().as_secs_f64();
                    let frac = elapsed / release.duration.as_secs_f64();
                    glide_theme::motion::lerp(from_progress, target_progress, release.at(frac))
                }
            }
        }
    }
}

const PRACTICE_WORKSPACE: usize = 1;
const PRACTICE_WORKSPACES: usize = 3;

#[derive(Debug)]
pub struct Practice {
    kind: glide_ipc::GestureKind,
    fingers: u32,
    role: Option<SwipeRole>,
    shell_pinch: bool,
    tracker: TouchpadGestureTracker,
    scale: f64,
}

impl Practice {
    fn new(kind: glide_ipc::GestureKind, fingers: u32, role: Option<SwipeRole>) -> Self {
        Self {
            kind,
            fingers,
            role,
            shell_pinch: false,
            tracker: TouchpadGestureTracker::new(SHELL_DESKTOP),
            scale: 1.0,
        }
    }

    #[must_use]
    pub fn swipe(fingers: u32, config: &GestureConfig) -> (Self, GestureSample) {
        let practice = Self::new(glide_ipc::GestureKind::Swipe, fingers, config.role(fingers));
        let action = if practice.role.is_some() { GestureAction::None } else { GestureAction::App };
        let sample = practice.sample(GesturePhase::Begin, action, 0.0, false);
        (practice, sample)
    }

    #[must_use]
    pub fn pinch(fingers: u32, config: &GestureConfig) -> (Self, GestureSample) {
        let mut practice = Self::new(glide_ipc::GestureKind::Pinch, fingers, None);
        practice.shell_pinch = config.claims_pinch(fingers);
        let action = if practice.shell_pinch { GestureAction::None } else { GestureAction::App };
        let sample = practice.sample(GesturePhase::Begin, action, 0.0, false);
        (practice, sample)
    }

    #[must_use]
    pub fn hold(fingers: u32) -> (Self, GestureSample) {
        let practice = Self::new(glide_ipc::GestureKind::Hold, fingers, None);
        let sample = practice.sample(GesturePhase::Begin, GestureAction::App, 0.0, false);
        (practice, sample)
    }

    #[must_use]
    pub const fn kind(&self) -> glide_ipc::GestureKind {
        self.kind
    }

    pub fn swipe_update(&mut self, event_micros: u64, dx: f64, dy: f64, config: &GestureConfig) -> GestureSample {
        let now = self.tracker.instant_for(event_micros);
        self.tracker.update(now, dx, dy);
        let (action, progress) = self.reading(config);
        self.sample(GesturePhase::Update, action, progress.abs(), false)
    }

    pub fn pinch_update(&mut self, dx: f64, dy: f64, scale: f64) -> GestureSample {
        self.tracker.accum_dx += dx;
        self.tracker.accum_dy += dy;
        self.scale = scale;
        let (action, progress) = self.reading(&GestureConfig::default());
        self.sample(GesturePhase::Update, action, progress, false)
    }

    #[must_use]
    pub fn end(&self, cancelled: bool, config: &GestureConfig) -> GestureSample {
        let (action, progress) = self.reading(config);
        let committed = !cancelled && self.commits(action, progress);
        self.sample(GesturePhase::End, action, progress.abs(), committed)
    }

    fn commits(&self, action: GestureAction, progress: f64) -> bool {
        let tracker = &self.tracker;
        match (self.kind, action) {
            (glide_ipc::GestureKind::Swipe, GestureAction::NextWorkspace | GestureAction::PreviousWorkspace) => {
                decide_workspace_settlement(
                    PRACTICE_WORKSPACE,
                    PRACTICE_WORKSPACES,
                    progress,
                    tracker.workspace_velocity(),
                    false,
                )
                .target
                .is_some()
            }
            (glide_ipc::GestureKind::Swipe, GestureAction::OpenOverview) => {
                tracker.shell_target(false) >= SHELL_OVERVIEW
            }
            (glide_ipc::GestureKind::Swipe, GestureAction::OpenLauncher) => {
                tracker.shell_target(false) >= SHELL_LAUNCHER
            }
            (glide_ipc::GestureKind::Swipe, GestureAction::SwitchApp) => app_switch_offset(tracker.accum_dx) != 0,
            (glide_ipc::GestureKind::Swipe, GestureAction::MaximizeWindow | GestureAction::MinimizeWindow) => {
                decide_window_swipe(tracker.accum_dy, tracker.overview_velocity(), false, false).is_some()
            }
            (glide_ipc::GestureKind::Pinch, GestureAction::OpenLauncher | GestureAction::ShowDesktop) => {
                decide_pinch(self.scale, false).is_some()
            }
            (glide_ipc::GestureKind::Swipe, _) | (glide_ipc::GestureKind::Pinch, GestureAction::None) => false,
            _ => true,
        }
    }

    fn reading(&self, config: &GestureConfig) -> (GestureAction, f64) {
        let tracker = &self.tracker;
        match (self.kind, self.role) {
            (glide_ipc::GestureKind::Swipe, Some(SwipeRole::Shell)) => {}
            (glide_ipc::GestureKind::Swipe, Some(SwipeRole::Windows)) => {
                return match tracker.kind {
                    GestureKind::Workspace => (GestureAction::SwitchApp, (tracker.accum_dx.abs() / APP_STEP).min(1.0)),
                    GestureKind::Overview => {
                        let travel = (tracker.accum_dy / OVERVIEW_DISTANCE).abs();
                        if tracker.accum_dy < 0.0 {
                            (GestureAction::MaximizeWindow, travel)
                        } else {
                            (GestureAction::MinimizeWindow, travel)
                        }
                    }
                    GestureKind::Undecided => (GestureAction::None, 0.0),
                };
            }
            (glide_ipc::GestureKind::Pinch, _) if self.shell_pinch => {
                let progress = pinch_progress(self.scale);
                return if progress > 0.0 {
                    (GestureAction::OpenLauncher, progress)
                } else if progress < 0.0 {
                    (GestureAction::ShowDesktop, -progress)
                } else {
                    (GestureAction::None, 0.0)
                };
            }
            (glide_ipc::GestureKind::Pinch, _) => {
                return (GestureAction::App, (self.scale - 1.0).abs().min(1.0));
            }
            _ => return (GestureAction::App, 0.0),
        }
        match tracker.kind {
            GestureKind::Workspace if !config.workspace_swipe => (GestureAction::Disabled, 0.0),
            GestureKind::Overview if !config.overview_swipe => (GestureAction::Disabled, 0.0),
            GestureKind::Workspace => {
                let progress = normalized_progress(self.tracker.accum_dx, SWIPE_DISTANCE).clamp(-1.0, 1.0);
                let action =
                    if progress >= 0.0 { GestureAction::NextWorkspace } else { GestureAction::PreviousWorkspace };
                (action, progress)
            }
            GestureKind::Overview if tracker.shell_position() > SHELL_OVERVIEW => {
                (GestureAction::OpenLauncher, tracker.shell_position() - SHELL_OVERVIEW)
            }
            GestureKind::Overview if tracker.accum_dy < 0.0 => (GestureAction::OpenOverview, tracker.shell_position()),
            GestureKind::Undecided | GestureKind::Overview => (GestureAction::None, 0.0),
        }
    }

    fn sample(&self, phase: GesturePhase, action: GestureAction, progress: f64, committed: bool) -> GestureSample {
        GestureSample {
            kind: self.kind,
            phase,
            fingers: self.fingers,
            dx: self.tracker.accum_dx,
            dy: self.tracker.accum_dy,
            scale: self.scale,
            progress: progress.clamp(0.0, 1.0),
            action,
            committed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn practise_swipe(fingers: u32, config: &GestureConfig, dx: f64, dy: f64) -> GestureSample {
        let (mut practice, begin) = Practice::swipe(fingers, config);
        assert_eq!(begin.phase, GesturePhase::Begin);
        for step in 0..10u64 {
            practice.swipe_update(1_000_000 + step * 10_000, dx / 10.0, dy / 10.0, config);
        }
        practice.end(false, config)
    }

    #[test]
    fn practice_reports_what_a_swipe_would_do_without_doing_it() {
        let config = GestureConfig::default();
        let left = practise_swipe(3, &config, -0.5 * SWIPE_DISTANCE, 10.0);
        assert_eq!(left.action, GestureAction::NextWorkspace);
        assert!(left.committed);
        assert!((left.progress - 0.5).abs() < 1e-6);

        let right = practise_swipe(3, &config, 0.5 * SWIPE_DISTANCE, 0.0);
        assert_eq!(right.action, GestureAction::PreviousWorkspace);

        let up = practise_swipe(3, &config, 5.0, -250.0);
        assert_eq!(up.action, GestureAction::OpenOverview);
        assert!(up.committed);

        let nudge = practise_swipe(3, &config, -20.0, 0.0);
        assert_eq!(nudge.action, GestureAction::NextWorkspace);
        assert!(nudge.progress < 0.1);
    }

    #[test]
    fn practice_follows_the_gesture_settings() {
        let config = GestureConfig { workspace_swipe: false, ..GestureConfig::default() };
        let sideways = practise_swipe(3, &config, -500.0, 0.0);
        assert_eq!(sideways.action, GestureAction::Disabled);
        assert!(!sideways.committed);

        let four = GestureConfig { fingers: 4, ..GestureConfig::default() };
        let three = practise_swipe(3, &four, -500.0, 0.0);
        assert_eq!(three.action, GestureAction::SwitchApp);
        assert_eq!(practise_swipe(4, &four, -500.0, 0.0).fingers, 4);
        let shell_only = GestureConfig { app_swipe: false, ..GestureConfig::default() };
        let app = practise_swipe(4, &shell_only, -500.0, 0.0);
        assert_eq!(app.action, GestureAction::App, "four fingers go to the app");
    }

    #[test]
    fn practice_reports_the_new_shell_actions() {
        let config = GestureConfig::default();
        let launcher = practise_swipe(3, &config, 0.0, -1.6 * OVERVIEW_DISTANCE);
        assert_eq!(launcher.action, GestureAction::OpenLauncher);
        assert!(launcher.committed);
        let apps = practise_swipe(4, &config, 2.0 * APP_STEP, 0.0);
        assert_eq!(apps.action, GestureAction::SwitchApp);
        assert!(apps.committed);
        let up = practise_swipe(4, &config, 0.0, -0.5 * OVERVIEW_DISTANCE);
        assert_eq!(up.action, GestureAction::MaximizeWindow);
        let down = practise_swipe(4, &config, 0.0, 0.5 * OVERVIEW_DISTANCE);
        assert_eq!(down.action, GestureAction::MinimizeWindow);
        assert!(down.committed);
        let (mut pinch, _) = Practice::pinch(3, &config);
        let closing = pinch.pinch_update(0.0, 0.0, 0.7);
        assert_eq!(closing.action, GestureAction::OpenLauncher);
        assert!(pinch.end(false, &config).committed);
        let (mut spread, _) = Practice::pinch(4, &config);
        assert_eq!(spread.pinch_update(0.0, 0.0, 1.3).action, GestureAction::ShowDesktop);
    }

    #[test]
    fn practice_reports_pinch_scale_and_hold() {
        let (mut pinch, _) = Practice::pinch(2, &GestureConfig::default());
        let update = pinch.pinch_update(0.0, 0.0, 1.4);
        assert_eq!(update.kind, glide_ipc::GestureKind::Pinch);
        assert!((update.scale - 1.4).abs() < 1e-9);
        assert!((update.progress - 0.4).abs() < 1e-9);
        let end = pinch.end(false, &GestureConfig::default());
        assert!(end.committed);
        assert_eq!(end.action, GestureAction::App);
        let (hold, _) = Practice::hold(3);
        assert!(!hold.end(true, &GestureConfig::default()).committed);
    }

    #[test]
    fn swipe_effort_does_not_depend_on_the_output_width() {
        for width in [1280.0, 1920.0, 3440.0, 7680.0] {
            let mut swipe = WorkspaceSwipeState::new(1, 3, width);
            swipe.update(-0.25 * SWIPE_DISTANCE);
            assert!((swipe.current_progress() - 0.25).abs() < 1e-9);
            assert!((swipe.effective_x() + 0.25 * width).abs() < 1e-6);
        }
    }

    #[test]
    fn a_swipe_never_travels_past_the_neighbouring_workspace() {
        let mut swipe = WorkspaceSwipeState::new(1, 3, 1920.0);
        swipe.update(-10.0 * SWIPE_DISTANCE);
        assert!((swipe.current_progress() - 1.0).abs() < 1e-9);
        swipe.update(0.5 * SWIPE_DISTANCE);
        assert!((swipe.current_progress() - 0.5).abs() < 1e-9);
    }

    #[test]
    fn catching_a_settle_continues_from_where_it_is_drawn() {
        for (active, dx) in [(1, -0.5), (0, 0.6), (2, -0.6), (1, 0.2)] {
            let mut swipe = WorkspaceSwipeState::new(active, 3, 1920.0);
            swipe.update(dx * SWIPE_DISTANCE);
            swipe.settle(3.0 * dx.signum(), false);
            swipe.phase = match swipe.phase {
                WorkspaceSwipePhase::Settling { release, from_progress, target_progress, target_workspace, .. } => {
                    WorkspaceSwipePhase::Settling {
                        started: Instant::now().checked_sub(release.duration / 3).expect("monotonic clock"),
                        release,
                        from_progress,
                        target_progress,
                        target_workspace,
                    }
                }
                tracking @ WorkspaceSwipePhase::Tracking { .. } => tracking,
            };
            let drawn = swipe.current_progress();
            assert!(swipe.catch());
            assert!(
                (swipe.current_progress() - drawn).abs() < 0.02,
                "workspace {active}: caught at {} but drawn at {drawn}",
                swipe.current_progress()
            );
            assert!(!swipe.catch(), "a tracking swipe has nothing to catch");
        }
    }

    #[test]
    fn a_workspace_release_starts_at_the_finger_speed() {
        let mut swipe = WorkspaceSwipeState::new(1, 3, 1920.0);
        swipe.update(-0.5 * SWIPE_DISTANCE);
        let settlement = swipe.settle(3.0, false);
        assert_eq!(settlement.target, Some(WorkspaceTarget::Next));
        assert!(settlement.release.slope > 0.0);
        let speed = settlement.release.slope * 0.5 / settlement.release.duration.as_secs_f64();
        assert!((speed - 3.0).abs() < 0.05, "left the finger at {speed}");
    }

    #[test]
    fn the_other_finger_count_drives_windows_and_apps() {
        let three = GestureConfig::default();
        assert_eq!(three.role(3), Some(SwipeRole::Shell));
        assert_eq!(three.role(4), Some(SwipeRole::Windows));
        assert_eq!(three.role(2), None);
        let four = GestureConfig { fingers: 4, ..GestureConfig::default() };
        assert_eq!(four.role(3), Some(SwipeRole::Windows));
        let no_apps = GestureConfig { app_swipe: false, ..GestureConfig::default() };
        assert_eq!(no_apps.role(4), None);
        assert!(three.claims_pinch(3) && three.claims_pinch(5));
        assert!(!three.claims_pinch(2), "two-finger pinch is zoom and stays with the app");
    }

    #[test]
    fn app_switching_moves_one_app_per_step_under_the_finger() {
        assert_eq!(app_switch_offset(0.0), 0);
        assert_eq!(app_switch_offset(0.4 * APP_STEP), 0);
        assert_eq!(app_switch_offset(0.6 * APP_STEP), 1);
        assert_eq!(app_switch_offset(3.2 * APP_STEP), 3);
        assert_eq!(app_switch_offset(-1.6 * APP_STEP), -2);
    }

    #[test]
    fn a_vertical_window_swipe_maximizes_restores_or_minimizes() {
        let (up, down) = (-0.5 * OVERVIEW_DISTANCE, 0.5 * OVERVIEW_DISTANCE);
        for (dy, velocity, maximized, cancelled, want) in [
            (up, 0.0, false, false, Some(WindowSwipe::Maximize)),
            (up, 0.0, true, false, None),
            (down, 0.0, true, false, Some(WindowSwipe::Restore)),
            (down, 0.0, false, false, Some(WindowSwipe::Minimize)),
            (-10.0, 0.0, false, false, None),
            (-10.0, 0.8, false, false, Some(WindowSwipe::Maximize)),
            (up, 0.0, false, true, None),
        ] {
            assert_eq!(decide_window_swipe(dy, velocity, maximized, cancelled), want, "{dy} {velocity}");
        }
    }

    #[test]
    fn pinching_in_opens_the_launcher_and_out_shows_the_desktop() {
        assert_eq!(decide_pinch(0.7, false), Some(PinchAction::Launcher));
        assert_eq!(decide_pinch(1.3, false), Some(PinchAction::Desktop));
        assert_eq!(decide_pinch(0.95, false), None);
        assert_eq!(decide_pinch(0.5, true), None);
        assert!((pinch_progress(0.8) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn touch_edges_are_claimed_only_near_the_border() {
        let (origin, size) = ((100.0, 0.0), (1000.0, 600.0));
        for (point, edge) in [
            ((500.0, 595.0), Some(TouchEdge::Bottom)),
            ((500.0, 4.0), Some(TouchEdge::Top)),
            ((105.0, 300.0), Some(TouchEdge::Left)),
            ((1095.0, 300.0), Some(TouchEdge::Right)),
            ((500.0, 300.0), None),
            ((50.0, 300.0), None),
        ] {
            assert_eq!(touch_edge(origin, size, point), edge, "{point:?}");
        }
        let (dx, _) = touch_units(TouchEdge::Left, 500.0, 40.0, size);
        assert!((dx - 0.5 * SWIPE_DISTANCE).abs() < 1e-9, "a finger drags the desktop 1:1");
        let (_, dy) = touch_units(TouchEdge::Bottom, 9.0, -300.0, size);
        assert!((dy + OVERVIEW_DISTANCE).abs() < 1e-9);
    }

    #[test]
    fn one_upward_axis_runs_from_desktop_through_overview_to_the_launcher() {
        let tracker = TouchpadGestureTracker::new(0.4);
        assert!((tracker.shell_position() - 0.4).abs() < 1e-9, "starts where it is drawn");
        for (base, dy, position) in [
            (0.4, -0.3 * OVERVIEW_DISTANCE, 0.7),
            (0.4, OVERVIEW_DISTANCE, 0.0),
            (SHELL_DESKTOP, -150.0, 0.5),
            (SHELL_OVERVIEW, 150.0, 0.5),
            (SHELL_OVERVIEW, -0.5 * OVERVIEW_DISTANCE, 1.5),
            (SHELL_DESKTOP, -9.0 * OVERVIEW_DISTANCE, SHELL_LAUNCHER),
        ] {
            assert!((shell_position(base, dy) - position).abs() < 1e-6, "{base} {dy}");
        }
        for (base, position, velocity, cancelled, rest) in [
            (SHELL_DESKTOP, 0.36, 0.0, false, SHELL_OVERVIEW),
            (SHELL_DESKTOP, 0.34, 0.0, false, SHELL_DESKTOP),
            (SHELL_OVERVIEW, 1.4, 0.0, false, SHELL_LAUNCHER),
            (SHELL_OVERVIEW, 1.2, 0.0, false, SHELL_OVERVIEW),
            (SHELL_OVERVIEW, 1.0, 0.0, false, SHELL_OVERVIEW),
            (SHELL_OVERVIEW, 0.6, 0.0, false, SHELL_DESKTOP),
            (SHELL_OVERVIEW, 0.7, 0.0, false, SHELL_OVERVIEW),
            (SHELL_LAUNCHER, 1.7, 0.0, false, SHELL_LAUNCHER),
            (SHELL_LAUNCHER, 1.5, 0.0, false, SHELL_OVERVIEW),
            (SHELL_LAUNCHER, SHELL_LAUNCHER, 0.0, false, SHELL_LAUNCHER),
            (SHELL_DESKTOP, 0.1, 0.6, false, SHELL_OVERVIEW),
            (SHELL_OVERVIEW, 0.9, -0.6, false, SHELL_DESKTOP),
            (SHELL_OVERVIEW, 1.9, 5.0, true, SHELL_OVERVIEW),
        ] {
            let settled = decide_shell_settlement(base, position, velocity, cancelled);
            assert_eq!(settled, rest, "{base} {position} {velocity}");
        }
    }

    #[test]
    fn velocity_follows_event_timestamps_not_arrival_time() {
        let mut tracker = TouchpadGestureTracker::new(SHELL_DESKTOP);
        for step in 0..10u64 {
            let now = tracker.instant_for(5_000_000 + step * 10_000);
            tracker.update(now, -20.0, 0.0);
        }
        let velocity = tracker.velocity.velocity_x();
        assert!(
            (velocity + 2000.0).abs() < 250.0,
            "20 px per 10 ms is about -2000 px/s regardless of batching, got {velocity}"
        );
    }

    #[test]
    fn a_flick_against_the_target_does_not_speed_up_the_settle() {
        let long = Duration::from_millis(320);
        assert_eq!(Release::new(-0.4, 0.0, long), Release::new(-0.4, 5.0, long));
    }

    #[test]
    fn test_boundary_rubber_banding_asymptote() {
        let sw = 1920.0;
        for dx in [10.0, 100.0, 500.0, 1000.0, 5000.0, 100_000.0] {
            for signed in [dx, -dx] {
                let eff = calculate_rubber_band(signed, sw);
                assert!(eff.abs() < 0.25 * sw && eff.signum() == signed.signum());
                assert!(normalized_progress(eff, sw).abs() < 0.25);
            }
        }
        assert!((calculate_rubber_band(1.0, sw) - 1.0).abs() < 0.01);
        for (active, dx, edge) in [
            (0, 50.0, true),
            (0, -50.0, false),
            (3, -50.0, true),
            (3, 50.0, false),
            (1, 50.0, false),
            (1, -50.0, false),
        ] {
            assert_eq!(is_at_boundary(active, 4, dx), edge, "{active} {dx}");
        }
    }

    #[test]
    fn test_rolling_velocity_tracker() {
        let mut tracker = VelocityTracker::new(Duration::from_millis(100));
        let t0 = Instant::now();
        tracker.push(t0, 0.0, 0.0);
        let t1 = t0 + Duration::from_millis(20);
        tracker.push(t1, -20.0, 0.0);
        let t2 = t0 + Duration::from_millis(40);
        tracker.push(t2, -20.0, 0.0);

        let vx = tracker.velocity_x();
        assert!(vx < -500.0);
        let vp = normalized_velocity_x(vx, 1000.0);
        assert!(vp > 0.5);
    }

    #[test]
    fn test_workspace_settlement_decision() {
        let settle_a = decide_workspace_settlement(1, 3, 0.36, 0.1, false);
        assert_eq!(settle_a.target, Some(WorkspaceTarget::Next));
        assert!((settle_a.target_progress - 1.0).abs() < f64::EPSILON);

        let settle_b = decide_workspace_settlement(1, 3, 0.20, 0.6, false);
        assert_eq!(settle_b.target, Some(WorkspaceTarget::Next));

        let settle_c = decide_workspace_settlement(1, 3, 0.20, 0.1, false);
        assert_eq!(settle_c.target, None);
        assert!(settle_c.target_progress.abs() < f64::EPSILON);

        let settle_d = decide_workspace_settlement(1, 3, 0.20, -0.6, false);
        assert_eq!(settle_d.target, None);

        let settle_e = decide_workspace_settlement(1, 3, -0.20, -0.6, false);
        assert_eq!(settle_e.target, Some(WorkspaceTarget::Previous));
        assert!((settle_e.target_progress - (-1.0)).abs() < f64::EPSILON);

        let settle_boundary = decide_workspace_settlement(0, 3, -0.20, -1.5, false);
        assert_eq!(settle_boundary.target, None);

        let settle_cancelled = decide_workspace_settlement(1, 3, 0.50, 1.0, true);
        assert_eq!(settle_cancelled.target, None);
    }

    #[test]
    fn swiping_past_the_last_workspace_targets_a_new_one() {
        let past_last = decide_workspace_settlement(2, 3 + 1, 0.6, 0.0, false);
        assert_eq!(past_last.target, Some(WorkspaceTarget::Next));
        assert_eq!(swipe_workspace_target(WorkspaceTarget::Next, 2, 3), WorkspaceTarget::New);
        assert_eq!(swipe_workspace_target(WorkspaceTarget::Next, 1, 3), WorkspaceTarget::Next);
        assert_eq!(swipe_workspace_target(WorkspaceTarget::Previous, 2, 3), WorkspaceTarget::Previous);
        assert!(!is_at_boundary(2, 3 + 1, -50.0));
    }

    #[test]
    fn test_duration_scaled_bezier_settlement() {
        let settle_half = decide_workspace_settlement(1, 3, 0.5, 0.0, false);
        assert!((settle_half.remaining_distance - 0.5).abs() < 1e-6);

        let settle_small = decide_workspace_settlement(1, 3, 0.95, 0.0, false);
        assert!((settle_small.remaining_distance - 0.05).abs() < 1e-6);
        assert!(settle_small.release.duration >= glide_theme::motion::t3().mul_f64(0.25));
    }

    #[test]
    fn test_axis_disambiguation() {
        let mut tracker = TouchpadGestureTracker::new(SHELL_DESKTOP);
        let t = Instant::now();
        tracker.update(t, 4.0, 2.0);
        assert_eq!(tracker.kind, GestureKind::Undecided);

        tracker.update(t, 10.0, 1.0);
        assert_eq!(tracker.kind, GestureKind::Workspace);

        let mut tracker_v = TouchpadGestureTracker::new(SHELL_DESKTOP);
        tracker_v.update(t, 2.0, -15.0);
        assert_eq!(tracker_v.kind, GestureKind::Overview);
    }

    #[test]
    fn empirical_challenge_overview_flick_velocity_and_arbitration() {
        let settle = |base: f64, q: f64, v: f64, cancelled: bool| decide_shell_settlement(base, q, v, cancelled);
        assert_eq!(settle(SHELL_DESKTOP, 0.35, 0.0, false), SHELL_DESKTOP);
        assert_eq!(settle(SHELL_DESKTOP, 0.35 + f64::EPSILON, 0.0, false), SHELL_OVERVIEW);
        for q in [0.001, 0.01, 0.05, 0.10, 0.20, 0.30, 0.34, 0.3499] {
            assert_eq!(settle(SHELL_DESKTOP, q, 0.50001, false), SHELL_OVERVIEW);
            assert_eq!(settle(SHELL_DESKTOP, q, 0.49999, false), SHELL_DESKTOP);
            assert_eq!(settle(SHELL_DESKTOP, q, -1.0, false), SHELL_DESKTOP);
        }
        assert_eq!(settle(SHELL_DESKTOP, 0.99, 10.0, true), SHELL_DESKTOP);
        assert_eq!(settle(SHELL_OVERVIEW, 0.01, -10.0, true), SHELL_OVERVIEW);
        assert_eq!(settle(SHELL_OVERVIEW, 0.65001, 0.0, false), SHELL_OVERVIEW);
        assert_eq!(settle(SHELL_OVERVIEW, 0.64999, 0.0, false), SHELL_DESKTOP);
        assert_eq!(settle(SHELL_OVERVIEW, 0.90, -0.50001, false), SHELL_DESKTOP);
        assert_eq!(settle(SHELL_OVERVIEW, 0.90, -0.49999, false), SHELL_OVERVIEW);

        let mut tracker = VelocityTracker::new(Duration::from_millis(100));
        let t0 = Instant::now();
        tracker.push(t0, 0.0, 0.0);
        let t1 = t0 + Duration::from_millis(40);
        tracker.push(t1, 0.0, -40.0);
        let vy = tracker.velocity_y();
        assert!(vy <= -500.0);
        let v_up = overview_velocity_up(vy);
        assert!(v_up > 0.5);
        assert_eq!(settle(SHELL_DESKTOP, 0.10, v_up, false), SHELL_OVERVIEW);
    }
}

use std::{
    borrow::Cow,
    time::{Duration, Instant},
};

use glide_theme::motion;
use smithay::{
    backend::input::{Axis, AxisSource, InputTime},
    desktop::find_popup_root_surface,
    input::pointer::AxisFrame,
    reexports::wayland_server::{Resource, protocol::wl_surface::WlSurface},
    wayland::{compositor::get_parent, pointer_constraints::with_pointer_constraint, seat::WaylandFocus},
};

use crate::GlideState;

const NOTCH: f64 = 120.0;
const AXES: [Axis; 2] = [Axis::Horizontal, Axis::Vertical];
const FRACTIONAL_WHEEL_VERSION: u32 = 8;

const SELF_ANIMATED: &[&str] = &[
    "firefox",
    "firefox-esr",
    "firefox-developer-edition",
    "firefoxdeveloperedition",
    "librewolf",
    "floorp",
    "waterfox",
    "zen",
    "zen-browser",
    "thunderbird",
    "betterbird",
    "chromium",
    "chromium-browser",
    "chrome",
    "google-chrome",
    "google-chrome-beta",
    "google-chrome-unstable",
    "chrome-*",
    "brave",
    "brave-browser",
    "brave-*",
    "com.brave.browser",
    "microsoft-edge",
    "microsoft-edge-*",
    "msedge-*",
    "com.microsoft.edge",
    "vivaldi",
    "vivaldi-stable",
    "vivaldi-*",
    "opera",
    "yandex-browser",
    "electron",
    "electron*",
    "code",
    "code-oss",
    "code-url-handler",
    "codium",
    "vscodium",
    "cursor",
    "discord",
    "vesktop",
    "webcord",
    "slack",
    "obsidian",
    "signal",
    "signal-desktop",
    "element",
    "riot",
    "spotify",
    "com.spotify.client",
    "teams-for-linux",
    "mattermost",
    "joplin",
    "bitwarden",
    "1password",
    "notion-app",
    "figma-linux",
    "logseq",
    "steam",
    "steamwebhelper",
    "beeper",
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Delta {
    pub v120: i32,
    pub value: f64,
}

#[derive(Debug, Clone, Copy, Default)]
struct Track {
    from: f64,
    to: f64,
    sent: i32,
    ratio: f64,
    began: Option<Instant>,
    duration: Duration,
}

impl Track {
    fn progress(&self, now: Instant) -> f64 {
        match self.began {
            Some(began) if !self.duration.is_zero() => {
                now.saturating_duration_since(began).as_secs_f64() / self.duration.as_secs_f64()
            }
            _ => 1.0,
        }
    }

    fn position(&self, now: Instant) -> f64 {
        let progress = self.progress(now);
        if progress >= 1.0 { self.to } else { motion::lerp(self.from, self.to, motion::ease_out(progress)) }
    }

    fn push(&mut self, v120: f64, value: f64, now: Instant, duration: Duration) {
        if v120 == 0.0 || !v120.is_finite() || !value.is_finite() {
            return;
        }
        let here = self.position(now);
        let reversed = (self.to - here) * v120 < 0.0;
        let base = if reversed { here } else { self.to };
        self.from = here;
        self.to = base + v120;
        self.ratio = value / v120;
        self.began = Some(now);
        self.duration = duration;
    }

    fn step_to(&mut self, goal: f64) -> Option<Delta> {
        let goal = goal.round() as i32;
        let v120 = goal - self.sent;
        if v120 == 0 {
            return None;
        }
        self.sent = goal;
        Some(Delta { v120, value: f64::from(v120) * self.ratio })
    }

    fn tick(&mut self, now: Instant) -> Option<Delta> {
        self.began?;
        let delta = self.step_to(self.position(now));
        if self.progress(now) >= 1.0 {
            *self = Self::default();
        }
        delta
    }

    fn flush(&mut self) -> Option<Delta> {
        self.began?;
        let delta = self.step_to(self.to);
        *self = Self::default();
        delta
    }
}

#[derive(Debug, Default)]
pub struct Smoother {
    tracks: [Track; 2],
}

impl Smoother {
    pub fn push(&mut self, notches: [(f64, f64); 2], now: Instant, duration: Duration) {
        for (track, (v120, value)) in self.tracks.iter_mut().zip(notches) {
            track.push(v120, value, now, duration);
        }
    }

    pub fn tick(&mut self, now: Instant) -> [Option<Delta>; 2] {
        self.tracks.each_mut().map(|track| track.tick(now))
    }

    pub fn flush(&mut self) -> [Option<Delta>; 2] {
        self.tracks.each_mut().map(Track::flush)
    }

    pub fn clear(&mut self) {
        self.tracks = [Track::default(); 2];
    }

    #[must_use]
    pub fn is_animating(&self) -> bool {
        self.tracks.iter().any(|track| track.began.is_some())
    }
}

#[derive(Debug, Default)]
pub struct SmoothWheel {
    smoother: Smoother,
    target: Option<WlSurface>,
}

impl SmoothWheel {
    fn clear(&mut self) {
        self.smoother.clear();
        self.target = None;
    }
}

#[must_use]
pub fn is_notched(notches: [f64; 2]) -> bool {
    notches.iter().any(|v120| *v120 != 0.0) && notches.iter().all(|v120| v120.is_finite() && *v120 % NOTCH == 0.0)
}

fn matches_entry(id: &str, last: &str, entry: &str) -> bool {
    let entry = entry.trim();
    let head_matches =
        |text: &str, prefix: &str| text.get(..prefix.len()).is_some_and(|head| head.eq_ignore_ascii_case(prefix));
    match entry.strip_suffix('*') {
        Some(prefix) => !prefix.is_empty() && (head_matches(id, prefix) || head_matches(last, prefix)),
        None => !entry.is_empty() && (id.eq_ignore_ascii_case(entry) || last.eq_ignore_ascii_case(entry)),
    }
}

#[must_use]
pub fn animates_itself(app_id: &str, extra: &[String]) -> bool {
    let id = app_id.trim();
    if id.is_empty() {
        return false;
    }
    let last = id.rsplit('.').next().unwrap_or(id);
    SELF_ANIMATED.iter().copied().chain(extra.iter().map(String::as_str)).any(|entry| matches_entry(id, last, entry))
}

fn constraint_active(surface: &WlSurface, pointer: &smithay::input::pointer::PointerHandle<GlideState>) -> bool {
    with_pointer_constraint(surface, pointer, |constraint| constraint.is_some_and(|c| c.is_active()))
}

fn root_of(surface: &WlSurface) -> WlSurface {
    let mut root = surface.clone();
    while let Some(parent) = get_parent(&root) {
        root = parent;
    }
    root
}

impl GlideState {
    fn pointer_surface(&self) -> Option<WlSurface> {
        let focus = self.seat.get_pointer()?.current_focus()?;
        focus.wl_surface().map(Cow::into_owned)
    }

    pub(crate) fn pointer_constrained(&self) -> bool {
        let (Some(pointer), Some(surface)) = (self.seat.get_pointer(), self.pointer_surface()) else {
            return false;
        };
        constraint_active(&surface, &pointer)
    }

    pub(crate) fn surface_app_id(&self, surface: &WlSurface) -> Option<String> {
        let mut root = root_of(surface);
        if let Some(popup) = self.popups.find_popup(&root)
            && let Ok(toplevel) = find_popup_root_surface(&popup)
        {
            root = root_of(&toplevel);
        }
        self.mapped_window(&root)?.identity().0
    }

    fn reads_fractional_wheel(&self, surface: &WlSurface) -> bool {
        let (Some(pointer), Some(client)) = (self.seat.get_pointer(), surface.client()) else {
            return false;
        };
        let modern = {
            let mut pointers = pointer.client_pointers(&client).peekable();
            pointers.peek().is_some() && pointers.all(|wl_pointer| wl_pointer.version() >= FRACTIONAL_WHEEL_VERSION)
        };
        modern || self.app_menus.is_gtk_client(&client)
    }

    fn wants_smooth_wheel(&self, surface: &WlSurface) -> bool {
        let Some(pointer) = self.seat.get_pointer() else {
            return false;
        };
        if !self.reads_fractional_wheel(surface) || constraint_active(surface, &pointer) {
            return false;
        }
        !self.surface_app_id(surface).is_some_and(|id| animates_itself(&id, &self.input_config.smooth_wheel_exclude))
    }

    pub fn smooth_wheel_takes(&mut self, source: AxisSource, notches: [(f64, f64); 2], factor: f64) -> bool {
        let focus = self.pointer_surface();
        if self.smooth_wheel.target != focus {
            self.smooth_wheel.clear();
        }
        let duration = motion::t2();
        let take = self.input_config.smooth_wheel
            && source == AxisSource::Wheel
            && !duration.is_zero()
            && is_notched(notches.map(|(v120, _)| v120))
            && focus.as_ref().is_some_and(|surface| self.wants_smooth_wheel(surface));
        if !take {
            self.flush_smooth_wheel();
            return false;
        }
        self.smooth_wheel.target = focus;
        self.smooth_wheel.smoother.push(
            notches.map(|(v120, value)| (v120 * factor, value * factor)),
            Instant::now(),
            duration,
        );
        self.queue_animation_redraw();
        true
    }

    pub fn smooth_wheel_pending(&mut self) -> bool {
        if !self.smooth_wheel.smoother.is_animating() {
            return false;
        }
        if self.pointer_surface() != self.smooth_wheel.target {
            self.smooth_wheel.clear();
            return false;
        }
        true
    }

    pub fn tick_smooth_wheel_on(&mut self, output: &smithay::output::Output) {
        if !self.smooth_wheel.smoother.is_animating() {
            return;
        }
        let pointer = self.seat.get_pointer().map(|pointer| pointer.current_location()).unwrap_or_default();
        if !self.space.output_geometry(output).is_some_and(|area| area.to_f64().contains(pointer)) {
            return;
        }
        if !self.smooth_wheel_pending() {
            return;
        }
        let deltas = self.smooth_wheel.smoother.tick(glide_ui::clock::now());
        self.send_wheel(deltas);
    }

    pub fn flush_smooth_wheel_before(&mut self, next: Option<&WlSurface>) {
        if self.smooth_wheel.smoother.is_animating() && self.smooth_wheel.target.as_ref() != next {
            self.flush_smooth_wheel();
        }
    }

    fn flush_smooth_wheel(&mut self) {
        if !self.smooth_wheel.smoother.is_animating() {
            return;
        }
        if self.pointer_surface() != self.smooth_wheel.target {
            self.smooth_wheel.clear();
            return;
        }
        let deltas = self.smooth_wheel.smoother.flush();
        self.smooth_wheel.target = None;
        self.send_wheel(deltas);
    }

    fn send_wheel(&mut self, deltas: [Option<Delta>; 2]) {
        if deltas.iter().all(Option::is_none) {
            return;
        }
        let mut frame = AxisFrame::new(InputTime::now()).source(AxisSource::Wheel);
        for (axis, delta) in AXES.into_iter().zip(deltas) {
            if let Some(delta) = delta {
                frame = frame.value(axis, delta.value).v120(axis, delta.v120);
            }
        }
        let pointer = self.seat.get_pointer().expect("a pointer was added in GlideState::new");
        pointer.axis(self, frame);
        pointer.frame(self);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DURATION: Duration = Duration::from_millis(200);
    const FRAME: Duration = Duration::from_millis(16);

    fn vertical(v120: f64) -> [(f64, f64); 2] {
        [(0.0, 0.0), (v120, v120 / 8.0)]
    }

    fn drain(smoother: &mut Smoother, mut now: Instant) -> (Vec<Delta>, Instant) {
        let mut out = Vec::new();
        while smoother.is_animating() {
            now += FRAME;
            if let [_, Some(delta)] = smoother.tick(now) {
                out.push(delta);
            }
            assert!(out.len() < 1000, "the animation never settled");
        }
        (out, now)
    }

    fn totals(deltas: &[Delta]) -> (i32, f64) {
        (deltas.iter().map(|delta| delta.v120).sum(), deltas.iter().map(|delta| delta.value).sum())
    }

    #[test]
    fn one_notch_is_spread_over_several_frames_and_adds_up_exactly() {
        let start = Instant::now();
        let mut smoother = Smoother::default();
        smoother.push(vertical(120.0), start, DURATION);
        let (deltas, end) = drain(&mut smoother, start);
        assert!(deltas.len() >= 5, "a notch should take several frames, got {deltas:?}");
        let (v120, value) = totals(&deltas);
        assert_eq!(v120, 120);
        assert!((value - 15.0).abs() < 1e-9);
        assert!(end.duration_since(start) >= DURATION);
        assert!(deltas.iter().all(|delta| delta.v120 > 0 && delta.value > 0.0));
    }

    #[test]
    fn two_frames_in_a_third_of_the_notch_has_already_moved() {
        let start = Instant::now();
        let mut smoother = Smoother::default();
        smoother.push(vertical(120.0), start, DURATION);
        let [_, Some(first)] = smoother.tick(start + FRAME) else { panic!("nothing after the first frame") };
        let [_, Some(second)] = smoother.tick(start + FRAME * 2) else { panic!("nothing after the second frame") };
        assert!(first.v120 + second.v120 >= 40, "E_OUT front-loads the motion, got {first:?} {second:?}");
    }

    #[test]
    fn notches_that_arrive_mid_flight_extend_the_same_motion() {
        let start = Instant::now();
        let mut smoother = Smoother::default();
        let mut sent = Vec::new();
        let mut now = start;
        for _ in 0..4 {
            smoother.push(vertical(-120.0), now, DURATION);
            for _ in 0..3 {
                now += FRAME;
                if let [_, Some(delta)] = smoother.tick(now) {
                    sent.push(delta);
                }
            }
        }
        sent.extend(drain(&mut smoother, now).0);
        assert_eq!(totals(&sent).0, -480);
        assert!(sent.iter().all(|delta| delta.v120 < 0), "never runs backwards: {sent:?}");
    }

    #[test]
    fn a_scaled_wheel_keeps_its_factor_in_both_value_and_v120() {
        let start = Instant::now();
        let mut smoother = Smoother::default();
        for _ in 0..3 {
            smoother.push(vertical(180.0), start, DURATION);
        }
        let (v120, value) = totals(&drain(&mut smoother, start).0);
        assert_eq!(v120, 540);
        assert!((value - 67.5).abs() < 1e-9);
    }

    #[test]
    fn turning_back_drops_what_was_left_of_the_old_direction() {
        let start = Instant::now();
        let mut smoother = Smoother::default();
        smoother.push(vertical(120.0), start, DURATION);
        let [_, Some(forward)] = smoother.tick(start + FRAME) else { panic!("no first frame") };
        smoother.push(vertical(-120.0), start + FRAME, DURATION);
        let (back, _) = drain(&mut smoother, start + FRAME);
        assert!(back.iter().all(|delta| delta.v120 < 0), "{back:?}");
        assert_eq!(forward.v120 + totals(&back).0, forward.v120 - 120);
    }

    #[test]
    fn a_flush_sends_exactly_the_remainder_in_one_step() {
        let start = Instant::now();
        let mut smoother = Smoother::default();
        smoother.push([(120.0, 15.0), (240.0, 30.0)], start, DURATION);
        let [Some(h), Some(v)] = smoother.tick(start + FRAME) else { panic!("both axes move") };
        let [Some(rest_h), Some(rest_v)] = smoother.flush() else { panic!("both axes have a remainder") };
        assert_eq!(h.v120 + rest_h.v120, 120);
        assert_eq!(v.v120 + rest_v.v120, 240);
        assert!(((v.value + rest_v.value) - 30.0).abs() < 1e-9);
        assert!(!smoother.is_animating());
        assert_eq!(smoother.flush(), [None, None]);
    }

    #[test]
    fn a_zero_duration_lands_on_the_first_tick() {
        let start = Instant::now();
        let mut smoother = Smoother::default();
        smoother.push(vertical(120.0), start, Duration::ZERO);
        assert_eq!(smoother.tick(start), [None, Some(Delta { v120: 120, value: 15.0 })]);
        assert!(!smoother.is_animating());
    }

    #[test]
    fn clearing_forgets_the_motion() {
        let start = Instant::now();
        let mut smoother = Smoother::default();
        smoother.push(vertical(120.0), start, DURATION);
        smoother.clear();
        assert!(!smoother.is_animating());
        assert_eq!(smoother.tick(start + DURATION), [None, None]);
    }

    #[test]
    fn only_whole_notches_are_smoothed() {
        assert!(is_notched([0.0, 120.0]));
        assert!(is_notched([-240.0, 0.0]));
        assert!(!is_notched([0.0, 0.0]), "an empty frame is nothing to smooth");
        assert!(!is_notched([0.0, 30.0]), "a high-resolution wheel is already smooth");
        assert!(!is_notched([120.0, 15.0]));
        assert!(!is_notched([f64::NAN, 120.0]));
    }

    #[test]
    fn browsers_and_electron_apps_keep_their_own_animation() {
        for id in [
            "firefox",
            "org.mozilla.firefox",
            "google-chrome",
            "com.google.Chrome",
            "chrome-abcdef-Default",
            "Brave-browser",
            "code",
            "com.visualstudio.code",
            "com.discordapp.Discord",
            "md.obsidian.Obsidian",
            "com.spotify.Client",
        ] {
            assert!(animates_itself(id, &[]), "{id}");
        }
        for id in ["org.gnome.Nautilus", "org.kde.dolphin", "glide-settings", "", "  ", "codeblocks"] {
            assert!(!animates_itself(id, &[]), "{id}");
        }
    }

    #[test]
    fn the_config_list_extends_the_built_in_one() {
        let extra = vec![String::from("org.kde.dolphin"), String::from("my-*"), String::from("*"), String::new()];
        assert!(animates_itself("org.kde.dolphin", &extra));
        assert!(animates_itself("dolphin", &[String::from("Dolphin")]));
        assert!(animates_itself("my-electron-app", &extra));
        assert!(!animates_itself("org.gnome.Nautilus", &extra), "a bare star or an empty entry matches nothing");
    }
}

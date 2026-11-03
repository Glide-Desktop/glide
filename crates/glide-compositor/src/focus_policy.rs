use std::time::{Duration, Instant};

pub const LAUNCH_WINDOW: Duration = Duration::from_secs(15);
pub const TYPING_GUARD: Duration = Duration::from_secs(1);
const KEPT_INTENTS: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchIntent {
    pub app: Option<String>,
    pub pid: Option<u32>,
    pub at: Instant,
}

impl LaunchIntent {
    fn names(&self, app: Option<&str>) -> bool {
        self.app.as_deref().zip(app).is_some_and(|(intent, app)| intent.eq_ignore_ascii_case(app))
    }

    const fn anonymous(&self) -> bool {
        self.app.is_none() && self.pid.is_none()
    }
}

#[derive(Debug, Default)]
pub struct LaunchIntents(Vec<LaunchIntent>);

impl LaunchIntents {
    pub fn record(&mut self, app: Option<String>, now: Instant) {
        let app = app.filter(|app| !app.trim().is_empty());
        self.0.push(LaunchIntent { app, pid: None, at: now });
        if self.0.len() > KEPT_INTENTS {
            self.0.remove(0);
        }
    }

    pub fn attach_pid(&mut self, pid: u32) {
        if let Some(intent) = self.0.iter_mut().rev().find(|intent| intent.pid.is_none()) {
            intent.pid = Some(pid);
        }
    }

    fn position(
        &self, app: Option<&str>, spawned: impl Fn(u32) -> bool, anonymous: bool, max_age: Duration, now: Instant,
    ) -> Option<usize> {
        self.0.iter().rposition(|intent| {
            now.saturating_duration_since(intent.at) <= max_age
                && (intent.names(app) || intent.pid.is_some_and(&spawned) || (anonymous && intent.anonymous()))
        })
    }

    pub fn take_for_activation(&mut self, app: Option<&str>, spawned: impl Fn(u32) -> bool, now: Instant) -> bool {
        self.prune(now);
        let found = self
            .position(app, spawned, true, crate::handlers::xdg_activation::LAUNCH_INTENT, now)
            .map(|at| self.0.remove(at));
        found.is_some()
    }

    pub fn take_for_new_window(&mut self, app: Option<&str>, spawned: impl Fn(u32) -> bool, now: Instant) -> bool {
        self.prune(now);
        let found = self.position(app, spawned, false, LAUNCH_WINDOW, now).map(|at| self.0.remove(at));
        found.is_some()
    }

    fn prune(&mut self, now: Instant) {
        self.0.retain(|intent| now.saturating_duration_since(intent.at) <= LAUNCH_WINDOW);
    }
}

#[expect(clippy::struct_excessive_bools, reason = "each flag is one independent reason to grant or refuse focus")]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NewWindow {
    pub anything_focused: bool,
    pub child_of_focused: bool,
    pub same_client_as_focused: bool,
    pub launched: bool,
    pub typing_elsewhere: bool,
    pub game_in_front: bool,
}

#[must_use]
pub const fn takes_focus(window: NewWindow) -> bool {
    !window.anything_focused
        || window.child_of_focused
        || window.same_client_as_focused
        || window.launched
        || !(window.typing_elsewhere || window.game_in_front)
}

#[must_use]
pub fn typing_recently(last_key: Option<Instant>, now: Instant) -> bool {
    last_key.is_some_and(|at| now.saturating_duration_since(at) < TYPING_GUARD)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_window_takes_focus_unless_the_user_is_busy_elsewhere() {
        let idle = NewWindow { anything_focused: true, ..NewWindow::default() };
        assert!(takes_focus(NewWindow::default()), "nothing to take it from");
        assert!(takes_focus(idle), "the user is not typing anywhere");
        let typing = NewWindow { typing_elsewhere: true, ..idle };
        assert!(!takes_focus(typing), "opens behind while the user types in another app");
        assert!(!takes_focus(NewWindow { game_in_front: true, ..idle }), "never over a game");
        assert!(takes_focus(NewWindow { child_of_focused: true, ..typing }), "a dialog of the focused window");
        assert!(takes_focus(NewWindow { same_client_as_focused: true, ..typing }), "another window of that app");
        assert!(takes_focus(NewWindow { launched: true, ..typing }), "the app the user just launched");
    }

    #[test]
    fn typing_counts_only_for_a_moment() {
        let now = Instant::now();
        assert!(!typing_recently(None, now));
        assert!(typing_recently(Some(now), now + TYPING_GUARD / 2));
        assert!(!typing_recently(Some(now), now + TYPING_GUARD));
    }

    #[test]
    fn a_launch_is_claimed_by_its_own_app_or_process_only() {
        let now = Instant::now();
        let mut intents = LaunchIntents::default();
        intents.record(Some("org.gnome.Nautilus".into()), now);
        assert!(!intents.take_for_new_window(Some("firefox"), |_| false, now), "another app's window");
        assert!(intents.take_for_new_window(Some("org.gnome.nautilus"), |_| false, now));
        assert!(!intents.take_for_new_window(Some("org.gnome.nautilus"), |_| false, now), "claimed once");

        intents.record(None, now);
        intents.attach_pid(4242);
        assert!(!intents.take_for_new_window(Some("anything"), |pid| pid == 7, now));
        assert!(intents.take_for_new_window(None, |pid| pid == 4242, now), "a window of the spawned process");
    }

    #[test]
    fn an_anonymous_launch_only_backs_an_activation_and_only_briefly() {
        let now = Instant::now();
        let mut intents = LaunchIntents::default();
        intents.record(None, now);
        assert!(!intents.take_for_new_window(Some("app"), |_| false, now), "a new window needs a matching launch");
        assert!(!intents.take_for_activation(Some("app"), |_| false, now + Duration::from_secs(6)));
        intents.record(None, now);
        assert!(intents.take_for_activation(Some("app"), |_| false, now + Duration::from_secs(1)));
    }

    #[test]
    fn a_new_window_may_appear_long_after_its_launch() {
        let now = Instant::now();
        let mut intents = LaunchIntents::default();
        intents.record(Some("steam".into()), now);
        assert!(intents.take_for_new_window(Some("steam"), |_| false, now + Duration::from_secs(12)));
        intents.record(Some("steam".into()), now);
        assert!(!intents.take_for_new_window(Some("steam"), |_| false, now + LAUNCH_WINDOW + Duration::from_secs(1)));
    }
}

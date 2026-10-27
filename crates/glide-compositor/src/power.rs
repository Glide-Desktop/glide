use smithay::{
    reexports::calloop::timer::{TimeoutAction, Timer},
    utils::{Logical, Point},
};

use crate::{
    power_dialog::{COUNTDOWN_TICK, Geometry, Outcome, PowerDialog},
    state::GlideState,
};

pub const SESSION_NOTICE_ID: u32 = u32::MAX - 2;
pub const END_ANYWAY: &str = "end-anyway";
const SESSION_NOTICE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(12);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PowerAction {
    Lock,
    LogOut,
    Sleep,
    Restart,
    ShutDown,
}

impl GlideState {
    pub fn perform_power_action(&mut self, action: PowerAction) {
        tracing::info!(?action, "power action");
        match action {
            PowerAction::Lock => self.lock_screen_gently(),
            PowerAction::Sleep => crate::system_info::suspend(),
            PowerAction::LogOut | PowerAction::Restart | PowerAction::ShutDown => self.request_session_end(action),
        }
    }

    pub fn request_session_end(&mut self, action: PowerAction) {
        use crate::session_end::Request;
        let windows = self.window_map.len();
        match self.session.end.request(action, std::time::Instant::now(), windows) {
            Request::Perform => self.end_session(action),
            Request::CloseWindows => {
                tracing::info!(?action, windows, "asking every window to close before the session ends");
                for window in self.window_map.values() {
                    if let Some(toplevel) = window.toplevel() {
                        toplevel.send_close();
                    }
                }
            }
        }
    }

    fn end_session(&mut self, action: PowerAction) {
        tracing::info!(?action, "ending the session");
        match action {
            PowerAction::LogOut => self.loop_signal.stop(),
            PowerAction::Restart => crate::system_info::reboot(),
            PowerAction::ShutDown => crate::system_info::power_off(),
            PowerAction::Lock | PowerAction::Sleep => {}
        }
    }

    pub fn tick_session_end(&mut self) {
        use crate::{
            i18n::{Key, t},
            session_end::Outcome,
        };
        if !self.session.end.is_pending() {
            return;
        }
        let windows = self.window_map.len();
        match self.session.end.poll(std::time::Instant::now(), windows) {
            None => {}
            Some(Outcome::Perform(action)) => self.end_session(action),
            Some(Outcome::Refused(action)) => {
                tracing::warn!(?action, windows, "apps kept their windows open; the session stays");
                if let Some(greeter) = self.greeter.as_mut().filter(|greeter| greeter.is_lock()) {
                    greeter.on_notice(t(Key::SessionEndRefusedLocked).to_string(), true);
                } else {
                    let notice = glide_notify::Notification {
                        id: SESSION_NOTICE_ID,
                        app_name: "Glide".to_string(),
                        summary: t(Key::SessionEndRefused).to_string(),
                        body: t(Key::SessionEndRefusedHint).replace("{apps}", &self.open_app_names().join(", ")),
                        actions: vec![glide_notify::Action {
                            key: END_ANYWAY.to_string(),
                            label: t(Key::SessionEndAnyway).to_string(),
                        }],
                        transient: true,
                        timeout: Some(SESSION_NOTICE_TIMEOUT),
                        ..glide_notify::Notification::default()
                    };
                    self.notifications.push(&mut self.dock.icons, notice, true);
                    self.refresh_topbar();
                }
                self.queue_redraw();
            }
        }
    }

    pub fn end_session_anyway(&mut self) {
        if let Some(action) = self.session.end.refused_action() {
            self.notifications.dismiss(SESSION_NOTICE_ID);
            self.request_session_end(action);
        }
    }

    fn open_app_names(&self) -> Vec<String> {
        let index = self.app_directory.index();
        let mut names: Vec<String> = self
            .window_map
            .values()
            .filter_map(|window| {
                let (title, app_id) = window
                    .toplevel()
                    .map(|toplevel| crate::state::toplevel_title_and_app_id(toplevel.wl_surface()))
                    .unwrap_or_default();
                let app_id = app_id.filter(|id| !id.trim().is_empty());
                let named =
                    app_id.as_deref().and_then(|id| index.and_then(|index| index.get(id))).map(|app| app.name.clone());
                named.or(app_id).or(title).filter(|name| !name.trim().is_empty())
            })
            .collect();
        names.sort();
        names.dedup();
        names
    }

    pub fn open_power_dialog(&mut self, focus: Option<PowerAction>) {
        if self.screen_locked() {
            return;
        }
        let now = std::time::Instant::now();
        if let Some(dialog) = self.live_power_dialog() {
            dialog.refocus(focus, now);
        } else {
            if self.switcher_open() {
                self.cancel_switcher();
            }
            let user = self.power_dialog_user();
            self.power_dialog = Some(PowerDialog::new(user, focus, now));
        }
        self.arm_power_countdown();
        self.queue_redraw();
    }

    fn power_dialog_user(&self) -> String {
        let name = self.topbar.system_status().user_name.trim().to_string();
        if name.is_empty() { crate::i18n::t(crate::i18n::Key::User).to_string() } else { name }
    }

    #[must_use]
    pub fn power_dialog_open(&self) -> bool {
        self.power_dialog.as_ref().is_some_and(|dialog| !dialog.is_closing())
    }

    pub fn cancel_power_dialog(&mut self) {
        self.disarm_power_countdown();
        if let Some(dialog) = self.power_dialog.as_mut() {
            dialog.begin_close();
            self.queue_redraw();
        }
    }

    fn finish_power_dialog(&mut self, action: PowerAction) {
        self.disarm_power_countdown();
        self.power_dialog = None;
        self.queue_redraw();
        self.perform_power_action(action);
    }

    fn apply_power_outcome(&mut self, outcome: Outcome) {
        match outcome {
            Outcome::Nothing => {}
            Outcome::Armed => {
                self.arm_power_countdown();
                self.queue_redraw();
            }
            Outcome::Cancel => self.cancel_power_dialog(),
            Outcome::Perform(action) => self.finish_power_dialog(action),
        }
    }

    fn live_power_dialog(&mut self) -> Option<&mut PowerDialog> {
        self.power_dialog.as_mut().filter(|d| !d.is_closing())
    }

    pub fn step_power_dialog(&mut self, delta: i32) {
        if let Some(dialog) = self.live_power_dialog()
            && dialog.step(delta)
        {
            self.queue_redraw();
        }
    }

    pub fn activate_power_dialog(&mut self) {
        let now = std::time::Instant::now();
        let outcome = self.live_power_dialog().map_or(Outcome::Nothing, |dialog| dialog.activate(now));
        self.apply_power_outcome(outcome);
    }

    pub fn power_dialog_motion(&mut self, pos: Point<f64, Logical>) -> bool {
        let area = self.output_rect();
        let Some(dialog) = self.live_power_dialog() else {
            return false;
        };
        let target = Geometry::new(area).target_at(pos, dialog.is_armed());
        if dialog.hover(target) {
            self.queue_redraw();
        }
        true
    }

    pub fn power_dialog_button(&mut self, pos: Point<f64, Logical>) -> bool {
        let area = self.output_rect();
        let now = std::time::Instant::now();
        let Some(dialog) = self.live_power_dialog() else {
            return false;
        };
        let outcome = dialog.press(Geometry::new(area), pos, now);
        self.apply_power_outcome(outcome);
        true
    }

    pub fn tick_power_dialog(&mut self) -> bool {
        let Some(dialog) = self.power_dialog.as_mut() else {
            return false;
        };
        if !dialog.tick() {
            self.power_dialog = None;
            return true;
        }
        dialog.is_animating()
    }

    fn disarm_power_countdown(&mut self) {
        if let Some(token) = self.power_countdown.take() {
            self.loop_handle.remove(token);
        }
    }

    fn arm_power_countdown(&mut self) {
        self.disarm_power_countdown();
        if !self.power_dialog.as_ref().is_some_and(PowerDialog::counting) {
            return;
        }
        let timer = Timer::from_duration(COUNTDOWN_TICK);
        let result = self.loop_handle.insert_source(timer, |_, (), state| {
            if state.power_countdown_tick() { TimeoutAction::ToDuration(COUNTDOWN_TICK) } else { TimeoutAction::Drop }
        });
        match result {
            Ok(token) => self.power_countdown = Some(token),
            Err(err) => tracing::error!(?err, "failed to arm the power dialog countdown"),
        }
    }

    fn power_countdown_tick(&mut self) -> bool {
        let now = std::time::Instant::now();
        let Some(dialog) = self.power_dialog.as_ref() else {
            self.power_countdown = None;
            return false;
        };
        if let Some(action) = dialog.due(now) {
            self.power_countdown = None;
            self.finish_power_dialog(action);
            return false;
        }
        let counting = dialog.counting();
        if !counting {
            self.power_countdown = None;
        }
        self.queue_redraw();
        counting
    }
}

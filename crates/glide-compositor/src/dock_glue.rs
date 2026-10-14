use std::collections::{HashMap, HashSet};

use smithay::{
    reexports::{
        calloop::timer::{TimeoutAction, Timer},
        wayland_server::Resource,
    },
    wayland::xdg_activation::XdgActivationTokenData,
};

use crate::{
    dock::{ATTENTION_REVEAL, DockAction, DockMenuItem, LAUNCH_TIMEOUT, MenuContext, MenuWindow},
    state::GlideState,
    window::GlideWindow,
};
use glide_layout::WindowId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockClick {
    Launch,
    Activate(WindowId),
}

#[must_use]
pub fn dock_click(candidates: &[(WindowId, bool)], focused: Option<WindowId>) -> DockClick {
    if let Some(focused) = focused.filter(|id| candidates.iter().any(|(candidate, _)| candidate == id)) {
        return DockClick::Activate(focused);
    }
    candidates
        .iter()
        .find(|(_, minimized)| !minimized)
        .or_else(|| candidates.first())
        .map_or(DockClick::Launch, |(id, _)| DockClick::Activate(*id))
}

#[must_use]
pub fn most_recent_first(windows: &[WindowId], history: &[WindowId]) -> Vec<WindowId> {
    let mut ordered: Vec<WindowId> = Vec::with_capacity(windows.len());
    for id in history {
        if windows.contains(id) && !ordered.contains(id) {
            ordered.push(*id);
        }
    }
    let mut rest: Vec<WindowId> = windows.iter().copied().filter(|id| !ordered.contains(id)).collect();
    rest.sort_by(|a, b| b.cmp(a));
    ordered.extend(rest);
    ordered
}

#[must_use]
pub fn fallback_key(id: WindowId) -> String {
    format!("{}{}", crate::dock::WINDOW_KEY_PREFIX, id.as_u64())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CachedKey {
    app_id: Option<String>,
    key: String,
    name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PidLookup {
    Unknown,
    Known(Option<i32>),
}

#[derive(Debug, Default)]
pub struct DockKeyCache {
    pids: HashMap<WindowId, Option<i32>>,
    keys: HashMap<WindowId, CachedKey>,
}

impl DockKeyCache {
    #[must_use]
    pub fn pid(&self, id: WindowId) -> PidLookup {
        match self.pids.get(&id) {
            None => PidLookup::Unknown,
            Some(pid) => PidLookup::Known(*pid),
        }
    }

    pub fn remember_pid(&mut self, id: WindowId, pid: Option<i32>) {
        self.pids.insert(id, pid);
    }

    #[must_use]
    pub fn key(&self, id: WindowId, app_id: Option<&str>) -> Option<(&str, Option<&str>)> {
        self.keys
            .get(&id)
            .filter(|cached| cached.app_id.as_deref() == app_id)
            .map(|cached| (cached.key.as_str(), cached.name.as_deref()))
    }

    pub fn remember_key(&mut self, id: WindowId, app_id: Option<&str>, key: String, name: Option<String>) {
        self.keys.insert(id, CachedKey { app_id: app_id.map(str::to_string), key, name });
    }

    pub fn retain_windows(&mut self, alive: impl Fn(WindowId) -> bool) {
        self.pids.retain(|id, _| alive(*id));
        self.keys.retain(|id, _| alive(*id));
    }

    pub fn forget_keys(&mut self) {
        self.keys.clear();
    }
}

fn own_app_id(window: &GlideWindow) -> Option<String> {
    window.identity().0.filter(|app_id| !app_id.trim().is_empty())
}

impl GlideState {
    pub(crate) fn window_pid(&self, window: &GlideWindow) -> Option<i32> {
        let toplevel = window.toplevel()?;
        let client = toplevel.wl_surface().client()?;
        client.get_credentials(&self.display_handle).ok().map(|credentials| credentials.pid)
    }

    fn cached_pid(&mut self, id: WindowId) -> Option<i32> {
        if let PidLookup::Known(pid) = self.dock_keys.pid(id) {
            return pid;
        }
        let pid = self.window_map.get(&id).and_then(|window| self.window_pid(window));
        self.dock_keys.remember_pid(id, pid);
        pid
    }

    fn resolve_dock_key(&self, id: WindowId, app_id: Option<&str>, pid: Option<i32>) -> String {
        if let Some(app_id) = app_id {
            return app_id.to_string();
        }
        pid.and_then(|pid| self.app_directory.process_app(pid)).map_or_else(|| fallback_key(id), str::to_string)
    }

    #[must_use]
    pub fn dock_key(&self, id: WindowId) -> Option<String> {
        let window = self.window_map.get(&id)?;
        let app_id = own_app_id(window);
        if let Some((key, _)) = self.dock_keys.key(id, app_id.as_deref()) {
            return Some(key.to_string());
        }
        let pid = match (&app_id, self.dock_keys.pid(id)) {
            (Some(_), _) => None,
            (None, PidLookup::Known(pid)) => pid,
            (None, PidLookup::Unknown) => self.window_pid(window),
        };
        Some(self.resolve_dock_key(id, app_id.as_deref(), pid))
    }

    fn cached_dock_key(&mut self, id: WindowId) -> Option<(String, Option<String>)> {
        let app_id = own_app_id(self.window_map.get(&id)?);
        if let Some((key, name)) = self.dock_keys.key(id, app_id.as_deref()) {
            return Some((key.to_string(), name.map(str::to_string)));
        }
        let pid = if app_id.is_some() { None } else { self.cached_pid(id) };
        let key = self.resolve_dock_key(id, app_id.as_deref(), pid);
        let name = self.app_directory.index().and_then(|index| index.name(&key)).map(str::to_string);
        self.dock_keys.remember_key(id, app_id.as_deref(), key.clone(), name.clone());
        Some((key, name))
    }

    fn resolve_anonymous_windows(&mut self) {
        let anonymous: Vec<WindowId> =
            self.window_map.iter().filter(|(_, window)| own_app_id(window).is_none()).map(|(id, _)| *id).collect();
        let pids: HashSet<i32> = anonymous.into_iter().filter_map(|id| self.cached_pid(id)).collect();
        for pid in &pids {
            self.app_directory.resolve(*pid);
        }
        self.app_directory.forget_processes_except(&pids);
    }

    pub fn refresh_dock(&mut self) -> bool {
        let window_map = &self.window_map;
        self.dock_keys.retain_windows(|id| window_map.contains_key(&id));
        self.resolve_anonymous_windows();
        let focused = self.layout.focused_window();
        let mut ids: Vec<WindowId> = self.window_map.keys().copied().collect();
        ids.sort();

        let mut apps: Vec<crate::dock::RunningApp> = Vec::new();
        for id in ids {
            let Some((key, index_name)) = self.cached_dock_key(id) else {
                continue;
            };
            let is_focused = focused == Some(id);
            if let Some(existing) = apps.iter_mut().find(|app| app.app_id.eq_ignore_ascii_case(&key)) {
                existing.focused |= is_focused;
                continue;
            }
            let title = self.window_title(id).filter(|title| !title.trim().is_empty());
            let name = index_name.or(title).unwrap_or_else(|| key.clone());
            apps.push(crate::dock::RunningApp { name, app_id: key, focused: is_focused });
        }

        apps.sort_by_key(|app| app.app_id.to_lowercase());
        self.dock.set_running(&apps)
    }

    #[must_use]
    pub fn windows_for_app(&self, key: &str) -> Vec<WindowId> {
        let mut windows: Vec<WindowId> = self
            .window_map
            .keys()
            .copied()
            .filter(|id| self.dock_key(*id).is_some_and(|candidate| candidate.eq_ignore_ascii_case(key)))
            .collect();
        windows.sort();
        windows
    }

    #[must_use]
    pub fn pin_identity(&self, app_id: &str) -> Option<(String, String)> {
        match self.app_directory.index() {
            Some(index) => index.get(app_id).map(|app| (app.name.clone(), app.command.clone())),
            None => crate::config::pin_identity_for_app_id(app_id),
        }
    }

    fn window_title(&self, id: WindowId) -> Option<String> {
        self.window_map.get(&id)?.identity().1
    }

    fn is_minimized(&self, id: WindowId) -> bool {
        self.layout.window_state(id).is_some_and(|state| state.minimized)
    }

    #[must_use]
    pub fn dock_menu_context(&self, index: usize) -> MenuContext {
        let Some(item) = self.dock.items().get(index).filter(|item| item.action == DockAction::Activate) else {
            return MenuContext::default();
        };
        let windows = most_recent_first(&self.windows_for_app(&item.app_id), &self.focus_history)
            .into_iter()
            .map(|id| MenuWindow {
                id: id.as_u64(),
                title: self.window_title(id).unwrap_or_default(),
                minimized: self.is_minimized(id),
            })
            .collect();
        let actions = self
            .app_directory
            .index()
            .and_then(|index| index.get(&item.app_id))
            .map(|app| app.actions.iter().map(|action| action.name.clone()).collect())
            .unwrap_or_default();
        MenuContext { windows, actions }
    }

    pub(crate) fn run_shell_focus_action(&mut self, action: &crate::input::KeyAction) {
        use crate::input::KeyAction;
        let changed = match action {
            KeyAction::FocusDock => {
                !self.layout.overview_active() && !self.dock_suppressed() && self.dock.begin_keyboard()
            }
            KeyAction::FocusTopBar => self.focus_top_bar(),
            KeyAction::DockNav(delta) => self.dock.keyboard_step(*delta),
            KeyAction::DockLeave => self.dock.end_keyboard(),
            KeyAction::DockOpenMenu => {
                let Some(index) = self.dock.keyboard_index() else {
                    return;
                };
                let (area, context) = (self.dock_area(), self.dock_menu_context(index));
                self.dock.open_menu(index, area, &context) && self.dock.menu_step(1)
            }
            KeyAction::DockMenuNav(delta) => self.dock.menu_step(*delta),
            KeyAction::DockActivate => {
                let Some(index) = self.dock.keyboard_index() else {
                    return;
                };
                self.dock.end_keyboard();
                if self.dock.action(index) == Some(crate::dock::DockAction::OpenLauncher) {
                    self.toggle_launcher();
                } else {
                    self.activate_dock_item(index);
                }
                true
            }
            KeyAction::DockMenuActivate => {
                let Some((index, item)) = self.dock.menu_selected() else {
                    return;
                };
                self.dock.end_keyboard();
                self.run_dock_menu_item(index, item);
                true
            }
            _ => false,
        };
        if changed {
            self.queue_redraw();
        }
    }

    fn focus_top_bar(&mut self) -> bool {
        use crate::topbar::{OpenPopup, TopbarAction};
        if !self.topbar.app_menu().is_empty() && self.topbar.open_popup() != Some(OpenPopup::AppMenu) {
            self.open_app_menu(0);
            if self.topbar.open_popup() == Some(OpenPopup::AppMenu) {
                return true;
            }
        }
        if self.topbar.open_popup() != Some(OpenPopup::QuickSettings) {
            self.run_topbar_action(TopbarAction::ToggleQuickSettings);
        }
        self.topbar.focus_nav(1, 0)
    }

    pub fn activate_dock_item(&mut self, index: usize) {
        let Some(item) = self.dock.items().get(index).cloned() else {
            return;
        };
        let ordered = most_recent_first(&self.windows_for_app(&item.app_id), &self.focus_history);
        let candidates: Vec<(WindowId, bool)> = ordered.iter().map(|id| (*id, self.is_minimized(*id))).collect();
        match dock_click(&candidates, self.layout.focused_window()) {
            DockClick::Activate(id) => {
                self.activate_window_animated(id);
                self.refresh_dock();
                self.queue_redraw();
            }
            DockClick::Launch => self.launch_app(&item.app_id, &item.name, &item.command),
        }
    }

    pub fn run_dock_menu_item(&mut self, index: usize, item: DockMenuItem) {
        let Some(tile) = self.dock.items().get(index).cloned() else {
            return;
        };
        match item {
            DockMenuItem::Window(raw) => {
                let id = WindowId::from_u64(raw);
                if self.window_map.contains_key(&id) {
                    self.activate_window_animated(id);
                }
            }
            DockMenuItem::ShowAllWindows => self.open_overview(),
            DockMenuItem::Open | DockMenuItem::NewWindow => {
                self.launch_app(&tile.app_id, &tile.name, &tile.command);
            }
            DockMenuItem::Action(slot) => {
                let command = self
                    .app_directory
                    .index()
                    .and_then(|index| index.get(&tile.app_id))
                    .and_then(|app| app.actions.get(slot))
                    .map(|action| action.command.clone());
                if let Some(command) = command {
                    self.launch_app(&tile.app_id, &tile.name, &command);
                }
            }
            DockMenuItem::Pin | DockMenuItem::Unpin => self.toggle_dock_pin(index),
            DockMenuItem::Hide => {
                for id in self.windows_for_app(&tile.app_id) {
                    if !self.is_minimized(id) {
                        self.minimize_with_flight(id);
                    }
                }
            }
            DockMenuItem::Quit => {
                for id in self.windows_for_app(&tile.app_id) {
                    if let Some(toplevel) = self.window_map.get(&id).and_then(|w| w.toplevel()) {
                        toplevel.send_close();
                    }
                }
                self.reconcile_space();
            }
        }
        self.refresh_dock();
        self.queue_redraw();
    }

    pub fn launch_app(&mut self, app_id: &str, name: &str, command: &str) {
        if command.trim().is_empty() {
            tracing::debug!(%app_id, "nothing to launch: the entry has no command");
            return;
        }
        let max_age = crate::handlers::xdg_activation::MAX_TOKEN_AGE;
        let activation = &mut self.protocols.xdg_activation_state;
        activation.retain_tokens(|_, data| data.timestamp.elapsed() <= max_age);
        let data = XdgActivationTokenData {
            app_id: (!app_id.trim().is_empty()).then(|| app_id.to_string()),
            ..XdgActivationTokenData::default()
        };
        let (token, data) = activation.create_external_token(data);
        crate::handlers::xdg_activation::grant(data);
        let token = token.as_str().to_string();
        let env = [("XDG_ACTIVATION_TOKEN", token.as_str()), ("DESKTOP_STARTUP_ID", token.as_str())];
        if let Err(err) = crate::spawn_shell_command_with_env(command, &env) {
            tracing::error!(%command, ?err, "failed to launch an app");
            return;
        }
        let running = !self.windows_for_app(app_id).is_empty();
        if !running && self.dock.begin_launch(app_id, name, command) {
            self.dock.reveal_for(ATTENTION_REVEAL);
            self.refresh_dock();
            self.arm_launch_timeout();
        }
        self.queue_redraw();
    }

    fn arm_launch_timeout(&self) {
        let timer = Timer::from_duration(LAUNCH_TIMEOUT + std::time::Duration::from_millis(50));
        let result = self.loop_handle.insert_source(timer, |_, (), state| {
            if state.refresh_dock() {
                state.queue_redraw();
            }
            TimeoutAction::Drop
        });
        if let Err(err) = result {
            tracing::warn!(?err, "failed to arm the launch feedback timeout");
        }
    }

    fn focused_dock_key(&self) -> Option<String> {
        self.layout.focused_window().and_then(|id| self.dock_key(id))
    }

    fn set_app_attention(&mut self, key: &str, urgent: bool) -> bool {
        if !urgent {
            return self.dock.set_urgent(key, false);
        }
        if self.focused_dock_key().is_some_and(|focused| focused.eq_ignore_ascii_case(key)) {
            return false;
        }
        if !self.dock.set_urgent(key, true) {
            return false;
        }
        self.dock.reveal_for(ATTENTION_REVEAL);
        true
    }

    pub fn demand_attention(&mut self, id: WindowId) {
        let Some(key) = self.dock_key(id) else {
            return;
        };
        if self.set_app_attention(&key, true) {
            tracing::debug!(%key, "an app asked for attention");
            self.refresh_dock();
            self.queue_redraw();
        }
    }

    fn dock_key_for_desktop_id(&self, desktop_id: &str) -> String {
        let index = self.app_directory.index();
        self.dock
            .items()
            .iter()
            .filter(|item| item.action == DockAction::Activate)
            .find(|item| {
                item.app_id.eq_ignore_ascii_case(desktop_id)
                    || index
                        .and_then(|index| index.get(&item.app_id))
                        .is_some_and(|app| app.id.eq_ignore_ascii_case(desktop_id))
            })
            .map_or_else(|| desktop_id.to_string(), |item| item.app_id.clone())
    }

    pub fn handle_launcher_entry(&mut self, desktop_id: &str, update: &crate::launcher_entry::EntryUpdate) {
        let entry = self.launcher_entries.entry(desktop_id.to_lowercase()).or_default();
        entry.merge(update);
        let state = *entry;
        let key = self.dock_key_for_desktop_id(desktop_id);
        let mut changed = self.dock.set_badge(&key, state.badge());
        if update.urgent.is_some() {
            changed |= self.set_app_attention(&key, state.urgent);
        }
        if changed {
            self.refresh_dock();
            self.queue_redraw();
        }
    }

    pub fn handle_app_index(&mut self, reply: crate::app_index::Reply) {
        if matches!(reply, crate::app_index::Reply::Index(_))
            && let Some(handoff) = self.handoff.as_mut()
        {
            handoff.mark_apps();
        }
        self.app_directory.apply(reply);
        self.dock_keys.forget_keys();
        if self.refresh_dock() {
            self.queue_redraw();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u64) -> WindowId {
        WindowId::from_u64(n)
    }

    #[test]
    fn an_app_with_no_windows_launches() {
        assert_eq!(dock_click(&[], None), DockClick::Launch);
        assert_eq!(dock_click(&[], Some(id(3))), DockClick::Launch);
    }

    #[test]
    fn clicking_the_focused_app_keeps_it_focused_and_never_minimizes() {
        let candidates = [(id(1), false)];
        assert_eq!(dock_click(&candidates, Some(id(1))), DockClick::Activate(id(1)));
        let several = [(id(2), false), (id(1), false)];
        assert_eq!(dock_click(&several, Some(id(1))), DockClick::Activate(id(1)), "no cycling to the next window");
    }

    #[test]
    fn clicking_another_app_raises_its_most_recent_visible_window() {
        let candidates = [(id(5), true), (id(3), false), (id(1), false)];
        assert_eq!(dock_click(&candidates, Some(id(9))), DockClick::Activate(id(3)));
    }

    #[test]
    fn when_every_window_is_minimized_the_most_recent_is_restored() {
        let candidates = [(id(4), true), (id(2), true)];
        assert_eq!(dock_click(&candidates, None), DockClick::Activate(id(4)));
    }

    #[test]
    fn recency_follows_focus_history_then_the_newest_untouched_window() {
        let windows = [id(1), id(2), id(3), id(4)];
        let history = [id(9), id(2), id(1), id(2)];
        assert_eq!(most_recent_first(&windows, &history), vec![id(2), id(1), id(4), id(3)]);
    }

    #[test]
    fn a_cached_key_is_reused_while_the_app_id_is_unchanged() {
        let mut cache = DockKeyCache::default();
        cache.remember_key(id(1), Some("firefox"), "firefox".into(), Some("Firefox".into()));
        assert_eq!(cache.key(id(1), Some("firefox")), Some(("firefox", Some("Firefox"))));
        assert_eq!(cache.key(id(2), Some("firefox")), None);
    }

    #[test]
    fn an_app_id_change_misses_the_cache() {
        let mut cache = DockKeyCache::default();
        cache.remember_key(id(1), None, fallback_key(id(1)), None);
        assert!(cache.key(id(1), None).is_some());
        assert_eq!(cache.key(id(1), Some("org.gnome.Nautilus")), None);
        cache.remember_key(id(3), Some("a"), "a".into(), None);
        assert_eq!(cache.key(id(3), Some("b")), None);
        assert_eq!(cache.key(id(3), None), None);
    }

    #[test]
    fn closing_a_window_drops_its_key_and_pid() {
        let mut cache = DockKeyCache::default();
        cache.remember_pid(id(1), Some(100));
        cache.remember_pid(id(2), None);
        cache.remember_key(id(1), None, "app".into(), None);
        cache.remember_key(id(2), Some("b"), "b".into(), None);
        cache.retain_windows(|window| window == id(2));
        assert_eq!(cache.pid(id(1)), PidLookup::Unknown);
        assert_eq!(cache.key(id(1), None), None);
        assert_eq!(cache.pid(id(2)), PidLookup::Known(None));
        assert!(cache.key(id(2), Some("b")).is_some());
    }

    #[test]
    fn an_index_reload_forgets_keys_but_keeps_pids() {
        let mut cache = DockKeyCache::default();
        cache.remember_pid(id(1), Some(42));
        cache.remember_key(id(1), None, fallback_key(id(1)), None);
        cache.forget_keys();
        assert_eq!(cache.key(id(1), None), None);
        assert_eq!(cache.pid(id(1)), PidLookup::Known(Some(42)));
    }

    #[test]
    fn anonymous_windows_get_a_stable_key_that_is_not_their_title() {
        assert_eq!(fallback_key(id(12)), "window-12");
        assert_eq!(fallback_key(id(12)), fallback_key(id(12)));
    }
}

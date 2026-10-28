use std::{io::Read, sync::Arc};

use smithay::{
    reexports::calloop::timer::{TimeoutAction, Timer},
    wayland::selection::data_device::{
        current_data_device_selection_userdata, request_data_device_client_selection, set_data_device_selection,
    },
};

use super::search::{Entry, SystemCommand};
use crate::{power::PowerAction, state::GlideState};

const TEXT_MIMES: [&str; 5] = ["text/plain;charset=utf-8", "text/plain", "UTF8_STRING", "TEXT", "STRING"];
const PASTE_LIMIT: u64 = 64 * 1024;

impl GlideState {
    pub fn arm_launcher_caret(&mut self) {
        let Some(launcher) = self.launcher.as_mut() else {
            return;
        };
        let Some(first) = launcher.next_caret_toggle() else {
            return;
        };
        if !launcher.arm_caret_timer() {
            return;
        }
        let result = self.loop_handle.insert_source(Timer::from_deadline(first), |_, (), state| {
            let next = state.launcher.as_ref().and_then(super::Launcher::next_caret_toggle);
            if let Some(next) = next {
                state.queue_redraw();
                return TimeoutAction::ToInstant(next);
            }
            if let Some(launcher) = state.launcher.as_mut() {
                launcher.disarm_caret_timer();
            }
            TimeoutAction::Drop
        });
        if let Err(err) = result {
            tracing::warn!(?err, "failed to arm the launcher caret timer");
            if let Some(launcher) = self.launcher.as_mut() {
                launcher.disarm_caret_timer();
            }
        }
    }

    fn close_launcher(&mut self) {
        if let Some(launcher) = self.launcher.as_mut() {
            launcher.begin_close();
        }
        self.queue_redraw();
    }

    fn launch_command(&mut self, app_id: &str, name: &str, command: &str) {
        self.launch_app(app_id, name, command);
        if let Some(launcher) = self.launcher.as_mut() {
            launcher.record_launch(app_id).save_in_background();
        }
    }

    pub fn search_providers(&mut self) -> Option<super::providers::Handle> {
        if std::mem::replace(&mut self.session.search_providers_started, true) {
            return self.session.search_providers.clone();
        }
        let (sender, receiver) = smithay::reexports::calloop::channel::channel::<(u64, Vec<super::providers::Found>)>();
        let handle = super::providers::start(move |generation, found| {
            let _ = sender.send((generation, found));
        });
        if handle.is_some() {
            let inserted = self.loop_handle.insert_source(receiver, |event, (), state| {
                if let smithay::reexports::calloop::channel::Event::Msg((generation, found)) = event
                    && let Some(launcher) = state.launcher.as_mut()
                {
                    launcher.set_provided(generation, found);
                    state.queue_redraw();
                }
            });
            if let Err(err) = inserted {
                tracing::warn!(?err, "cannot watch the search providers");
            }
        }
        self.session.search_providers.clone_from(&handle);
        handle
    }

    pub fn activate_launcher_entry(&mut self, entry: Entry) {
        let Some(launcher) = self.launcher.as_ref() else {
            return;
        };
        let apps = launcher.apps();
        let target = match &entry {
            Entry::App(index) => apps.get(*index).map(|app| (app, &app.command)),
            Entry::Action { app, action } => {
                apps.get(*app).and_then(|owner| Some((owner, &owner.actions.get(*action)?.command)))
            }
            _ => None,
        }
        .map(|(app, command)| (app.id.clone(), app.name.clone(), command.clone()));
        let origin = launcher.launch_rect(&entry);
        match entry {
            Entry::App(_) | Entry::Action { .. } => {
                let Some((id, name, command)) = target else {
                    return;
                };
                self.session.launch_origin =
                    origin.map(|rect| super::LaunchOrigin { key: id.clone(), rect, at: std::time::Instant::now() });
                self.launch_command(&id, &name, &command);
            }
            Entry::Calculator { value, .. } => self.copy_to_clipboard(&value),
            Entry::Command(command) => {
                self.close_launcher();
                self.run_system_command(command);
                return;
            }
            Entry::Settings(panel) => {
                let command = format!("glide-settings {}", panel.slug());
                if let Err(err) = self.spawn_user_command(&command) {
                    tracing::error!(%command, ?err, "failed to open a settings panel");
                }
            }
            Entry::Run(command) => {
                tracing::info!(%command, "running a command from the run prompt");
                match self.spawn_user_command(&command) {
                    Ok(_) => crate::launcher::history::remember_run(&command),
                    Err(err) => tracing::warn!(%command, ?err, "failed to run a command"),
                }
            }
            Entry::File(index) => {
                if let Some(path) = launcher.recent_file(index).map(|file| file.path.clone()) {
                    let mut command = std::process::Command::new("xdg-open");
                    command.arg(&path);
                    if let Err(err) = self.spawn_user_process(command) {
                        tracing::warn!(path = %path.display(), ?err, "failed to open a recent file");
                    }
                }
            }
            Entry::Provided { index, .. } => {
                if let Some((found, handle, terms)) = launcher.provided(index) {
                    handle.activate(found, terms);
                }
            }
            Entry::Window { index, .. } => {
                let id = launcher.window(index).map(|window| glide_layout::WindowId(window.id));
                self.close_launcher();
                if let Some(id) = id.filter(|id| self.window_map.contains_key(id)) {
                    self.activate_window_animated(id);
                }
                return;
            }
        }
        self.close_launcher();
    }

    pub fn open_run_prompt(&mut self) {
        if self.launcher.as_ref().is_none_or(super::Launcher::is_closing) {
            self.toggle_launcher();
        }
        if let Some(launcher) = self.launcher.as_mut() {
            launcher.begin_run_mode();
        }
        self.queue_redraw();
    }

    #[must_use]
    pub fn open_windows_for_search(&self) -> Vec<super::search::OpenWindow> {
        let mut order: Vec<glide_layout::WindowId> =
            self.focus_history.iter().copied().filter(|id| self.window_map.contains_key(id)).collect();
        let mut rest: Vec<glide_layout::WindowId> =
            self.window_map.keys().copied().filter(|id| !order.contains(id)).collect();
        rest.sort();
        order.extend(rest);
        order
            .into_iter()
            .filter_map(|id| {
                let window = self.window_map.get(&id)?;
                let (title, app_id) = crate::state::toplevel_title_and_app_id(crate::state::window_surface(window));
                Some(super::search::OpenWindow {
                    id: id.as_u64(),
                    title: title.unwrap_or_default(),
                    app_id: app_id.unwrap_or_default(),
                })
            })
            .filter(|window| !window.title.is_empty() || !window.app_id.is_empty())
            .collect()
    }

    fn run_system_command(&mut self, command: SystemCommand) {
        tracing::info!(?command, "system command from the launcher");
        match command {
            SystemCommand::Lock => self.perform_power_action(PowerAction::Lock),
            SystemCommand::Sleep => self.perform_power_action(PowerAction::Sleep),
            SystemCommand::LogOut => self.open_power_dialog(Some(PowerAction::LogOut)),
            SystemCommand::Restart => self.open_power_dialog(Some(PowerAction::Restart)),
            SystemCommand::ShutDown => self.open_power_dialog(Some(PowerAction::ShutDown)),
            SystemCommand::ScreenMirroring => self.toggle_screen_mirroring(),
        }
    }

    pub fn copy_to_clipboard(&mut self, text: &str) {
        set_data_device_selection(
            &self.display_handle,
            &self.seat,
            TEXT_MIMES.iter().map(ToString::to_string).collect(),
            crate::clipboard::Owned::Text(Arc::<str>::from(text)),
        );
    }

    pub fn copy_secret_to_clipboard(&mut self, secret: &str, private: bool) {
        if !private {
            self.copy_to_clipboard(secret);
            return;
        }
        let mimes = TEXT_MIMES.iter().chain([&crate::clipboard::PASSWORD_HINT]).map(ToString::to_string).collect();
        set_data_device_selection(
            &self.display_handle,
            &self.seat,
            mimes,
            crate::clipboard::Owned::Secret(Arc::<str>::from(secret)),
        );
    }

    pub fn paste_into_launcher(&mut self) {
        let own = current_data_device_selection_userdata(&self.seat).map(|own| own.text().map(str::to_string));
        if let Some(own) = own {
            if let Some(text) = own
                && let Some(launcher) = self.launcher.as_mut()
                && launcher.insert_text(&text)
            {
                self.queue_redraw();
            }
            return;
        }

        for mime in TEXT_MIMES {
            let Ok((reader, writer)) = std::io::pipe() else {
                return;
            };
            match request_data_device_client_selection(&self.seat, mime.to_string(), writer.into()) {
                Ok(()) => {
                    self.read_paste(reader);
                    return;
                }
                Err(smithay::wayland::selection::data_device::SelectionRequestError::InvalidMimetype) => {}
                Err(err) => {
                    tracing::debug!(?err, "nothing to paste into the launcher");
                    return;
                }
            }
        }
    }

    fn read_paste(&mut self, reader: std::io::PipeReader) {
        let (sender, receiver) = smithay::reexports::calloop::channel::channel::<String>();
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            if reader.take(PASTE_LIMIT).read_to_end(&mut bytes).is_ok() {
                let _ = sender.send(String::from_utf8_lossy(&bytes).into_owned());
            }
        });
        let result = self.loop_handle.insert_source(receiver, |event, (), state| {
            let smithay::reexports::calloop::channel::Event::Msg(text) = event else {
                return;
            };
            if let Some(launcher) = state.launcher.as_mut().filter(|l| !l.is_closing())
                && launcher.insert_text(&text)
            {
                state.queue_redraw();
            }
        });
        if let Err(err) = result {
            tracing::warn!(?err, "failed to register the launcher paste reader");
        }
    }
}

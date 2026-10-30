use std::collections::HashMap;
use std::io::Read;
use std::sync::Arc;
use std::time::{Duration, Instant};

use glide_config::passwords::Prefs;
use glide_ipc::keychain::{Candidate, ItemKind, Request, Response, SecretKind, Summary, SyncState, Vault};
use smithay::input::keyboard::{Keysym, ModifiersState};
use smithay::reexports::calloop::RegistrationToken;
use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay::reexports::wayland_server::{Resource, protocol::wl_surface::WlSurface};
use smithay::wayland::selection::data_device::{clear_data_device_selection, current_data_device_selection_userdata};

use crate::focused_field::Delivery;
use crate::i18n::{Key, t};
use crate::launcher::field::CaretMove;
use crate::state::{ClientState, GlideState};
use crate::topbar::passwords_view::{Applet, Dot, LIMIT, MenuAction, Phase, Row};
use crate::topbar::{FieldKey, OpenPopup, TopbarAction};

pub const NOTICE_ID: u32 = u32::MAX - 12;
const APP: &str = "glide-passwords";
const GENERATED_LEN: usize = 20;
const ALPHABET: &[u8] = b"abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789-_!@#%*+=?";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PwKey {
    Field(FieldKey),
    Step(i32),
    Left,
    Right,
    Activate,
    Escape,
}

#[derive(Debug, Clone)]
struct Target {
    surface: WlSurface,
    program: Option<String>,
    app_id: Option<String>,
}

#[derive(Debug, Default)]
pub struct Session {
    target: Option<Target>,
    generation: u64,
    recent: Vec<Row>,
    vaults: HashMap<String, (String, Dot)>,
    clear: Option<RegistrationToken>,
    idle: IdleLock,
}

const IDLE_FLOOR: Duration = Duration::from_mins(1);
const PREFS_STALE: Duration = Duration::from_secs(30);

#[derive(Debug, Default)]
struct IdleLock {
    read: Option<(Instant, u32)>,
    locked_for: Option<Instant>,
}

impl IdleLock {
    fn due(&mut self, now: Instant, last_input: Option<Instant>, read: impl FnOnce() -> u32) -> bool {
        let Some(last) = last_input else {
            return false;
        };
        let idle = now.saturating_duration_since(last);
        if idle < IDLE_FLOOR || self.locked_for == Some(last) {
            return false;
        }
        let minutes = match self.read {
            Some((at, minutes)) if now.saturating_duration_since(at) < PREFS_STALE => minutes,
            _ => {
                let minutes = read();
                self.read = Some((now, minutes));
                minutes
            }
        };
        if minutes == 0 || idle < Duration::from_mins(u64::from(minutes)) {
            return false;
        }
        self.locked_for = Some(last);
        true
    }
}

#[must_use]
pub fn shortcut(modifiers: &ModifiersState, raw_syms: &[Keysym], modified_sym: Keysym) -> Option<PwKey> {
    let matches = |want: Keysym| modified_sym == want || raw_syms.contains(&want);
    let plain = !modifiers.ctrl && !modifiers.alt && !modifiers.logo;
    let ctrl_only = modifiers.ctrl && !modifiers.alt && !modifiers.logo;
    if matches(Keysym::Escape) {
        return Some(PwKey::Escape);
    }
    if matches(Keysym::Return) || matches(Keysym::KP_Enter) {
        return Some(PwKey::Activate);
    }
    if matches(Keysym::Up) {
        return Some(PwKey::Step(-1));
    }
    if matches(Keysym::Down) {
        return Some(PwKey::Step(1));
    }
    if matches(Keysym::ISO_Left_Tab) {
        return Some(PwKey::Step(-1));
    }
    if matches(Keysym::Tab) {
        return Some(PwKey::Step(if modifiers.shift { -1 } else { 1 }));
    }
    if plain && matches(Keysym::Right) {
        return Some(PwKey::Right);
    }
    if plain && matches(Keysym::Left) {
        return Some(PwKey::Left);
    }
    crate::topbar::field_key(ctrl_only, plain, modified_sym.key_char(), matches).map(PwKey::Field)
}

const fn dot(sync: &SyncState) -> Dot {
    match sync {
        SyncState::Local => Dot::Local,
        SyncState::Syncing | SyncState::Synced { .. } => Dot::Synced,
        SyncState::Offline { .. } | SyncState::Failed { .. } => Dot::Pending,
    }
}

fn sub_line(username: &str, vault: &str) -> String {
    [username, vault].into_iter().filter(|part| !part.is_empty()).collect::<Vec<_>>().join(" · ")
}

fn candidate_row(candidate: Candidate, vaults: &HashMap<String, (String, Dot)>) -> Row {
    let dot = vaults.get(&candidate.item.vault).map_or(Dot::Local, |(_, dot)| *dot);
    Row {
        sub: sub_line(&candidate.username, &candidate.vault_name),
        name: candidate.label,
        dot,
        username: candidate.username,
        has_totp: candidate.has_totp,
        item: candidate.item,
    }
}

fn recent_rows(mut list: Vec<Summary>, fitting: &[Row], vaults: &HashMap<String, (String, Dot)>) -> Vec<Row> {
    list.retain(|summary| {
        !summary.deleted
            && summary.kind == ItemKind::Login
            && summary.has_password
            && !fitting.iter().any(|row| row.item == summary.item)
    });
    list.sort_by_key(|summary| std::cmp::Reverse(summary.modified));
    list.into_iter()
        .take(LIMIT)
        .map(|summary| {
            let (vault, dot) = vaults.get(&summary.item.vault).cloned().unwrap_or((String::new(), Dot::Local));
            Row {
                sub: sub_line(&summary.username, &vault),
                name: summary.name,
                dot,
                username: summary.username,
                has_totp: summary.has_totp,
                item: summary.item,
            }
        })
        .collect()
}

fn vault_map(list: &[Vault]) -> HashMap<String, (String, Dot)> {
    list.iter().map(|vault| (vault.id.clone(), (vault.name.clone(), dot(&vault.sync)))).collect()
}

fn has_every_class(password: &str) -> bool {
    let classes: [fn(&char) -> bool; 4] =
        [char::is_ascii_lowercase, char::is_ascii_uppercase, char::is_ascii_digit, char::is_ascii_punctuation];
    classes.iter().all(|class| password.chars().any(|ch| class(&ch)))
}

fn generate(mut random: impl FnMut() -> Option<u8>) -> Option<String> {
    let limit = u8::MAX - (u8::MAX % ALPHABET.len() as u8);
    loop {
        let mut password = String::with_capacity(GENERATED_LEN);
        while password.len() < GENERATED_LEN {
            let byte = random()?;
            if byte < limit {
                password.push(char::from(ALPHABET[usize::from(byte) % ALPHABET.len()]));
            }
        }
        if has_every_class(&password) {
            return Some(password);
        }
    }
}

fn system_password() -> Option<String> {
    let mut source = std::fs::File::open("/dev/urandom").ok()?;
    generate(|| {
        let mut byte = [0u8];
        source.read_exact(&mut byte).ok().map(|()| byte[0])
    })
}

impl GlideState {
    pub(crate) fn run_passwords_action(&mut self, action: TopbarAction) {
        match action {
            TopbarAction::TogglePasswords => {
                let target = self.focused_surface();
                self.topbar.toggle_popup(OpenPopup::Passwords);
                if self.topbar.open_popup() == Some(OpenPopup::Passwords) {
                    self.open_passwords(target);
                }
            }
            TopbarAction::PasswordsField => {
                self.topbar.passwords_mut().menu = None;
            }
            TopbarAction::PasswordsRow(index) => {
                let index = usize::from(index);
                let applet = self.topbar.passwords_mut();
                if applet.menu == Some(index) {
                    applet.menu = None;
                    return;
                }
                applet.selected = index;
                let click = MenuAction::of(applet.click);
                self.passwords_perform(index, click);
            }
            TopbarAction::PasswordsMenu(position) => {
                let applet = self.topbar.passwords();
                let Some(index) = applet.menu else {
                    return;
                };
                if let Some(action) = applet.menu_actions(index).get(usize::from(position)).copied() {
                    self.passwords_perform(index, action);
                }
            }
            TopbarAction::PasswordsGenerate => self.generate_password(),
            TopbarAction::PasswordsOpen | TopbarAction::PasswordsCreate | TopbarAction::PasswordsConnect => {
                self.topbar.close_popup();
                self.open_passwords_app();
            }
            TopbarAction::PasswordsUnlock => {
                let target = self.session.passwords.target.as_ref().map(|target| target.surface.clone());
                self.topbar.close_popup();
                self.unlock_keychain(None, move |state| {
                    if state.topbar.open_popup() != Some(OpenPopup::Passwords) {
                        state.topbar.toggle_popup(OpenPopup::Passwords);
                    }
                    state.open_passwords(target);
                });
            }
            _ => {}
        }
        self.queue_redraw();
    }

    pub(crate) fn run_passwords_key(&mut self, key: PwKey) {
        let phase = self.topbar.passwords().phase;
        match key {
            PwKey::Escape => {
                let applet = self.topbar.passwords_mut();
                if applet.menu.take().is_none() {
                    self.topbar.close_popup();
                }
            }
            PwKey::Activate => match phase {
                Phase::Locked => self.run_passwords_action(TopbarAction::PasswordsUnlock),
                Phase::Empty => self.run_passwords_action(TopbarAction::PasswordsCreate),
                Phase::Loading | Phase::Open => {
                    let applet = self.topbar.passwords();
                    if let Some(index) = applet.menu {
                        if let Some(action) = applet.menu_actions(index).get(applet.menu_selected).copied() {
                            self.passwords_perform(index, action);
                        }
                    } else if applet.row(applet.selected).is_some() {
                        let (index, click) = (applet.selected, MenuAction::of(applet.click));
                        self.passwords_perform(index, click);
                    }
                }
            },
            PwKey::Step(delta) => {
                let applet = self.topbar.passwords_mut();
                applet.step(delta);
            }
            PwKey::Right => {
                let applet = self.topbar.passwords_mut();
                let at_end = applet.field.caret() >= applet.field.text().len();
                if applet.menu.is_none() && at_end && applet.row(applet.selected).is_some() {
                    applet.menu = Some(applet.selected);
                    applet.menu_selected = 0;
                    applet.code = None;
                    applet.touch();
                    self.load_menu_code();
                } else {
                    if applet.menu.is_none() {
                        applet.field.move_caret(CaretMove::Right);
                    }
                    applet.touch();
                }
            }
            PwKey::Left => {
                let applet = self.topbar.passwords_mut();
                if applet.menu.take().is_none() {
                    applet.field.move_caret(CaretMove::Left);
                }
                applet.touch();
            }
            PwKey::Field(key) => self.passwords_field_key(key),
        }
        self.queue_redraw();
    }

    fn load_menu_code(&mut self) {
        let applet = self.topbar.passwords();
        let Some(row) = applet.menu.and_then(|index| applet.row(index)).filter(|row| row.has_totp) else {
            return;
        };
        let item = row.item.clone();
        let generation = self.session.passwords.generation;
        self.keychain(Request::Totp { item: item.clone() }, move |state, response| {
            let applet = state.topbar.passwords();
            let still_open = applet.menu.and_then(|index| applet.row(index)).is_some_and(|row| row.item == item);
            if !state.passwords_current(generation) || !still_open {
                return;
            }
            if let Response::Code { code } = response {
                let applet = state.topbar.passwords_mut();
                applet.code = Some(code.code);
                applet.touch();
                state.queue_redraw();
            }
        });
    }

    fn passwords_field_key(&mut self, key: FieldKey) {
        if matches!(self.topbar.passwords().phase, Phase::Locked | Phase::Empty) {
            return;
        }
        let applet = self.topbar.passwords_mut();
        let before = applet.field.text().to_string();
        match key {
            FieldKey::Type(ch) => {
                applet.field.insert(ch);
            }
            FieldKey::Backspace => {
                applet.field.backspace();
            }
            FieldKey::DeleteWord => {
                applet.field.delete_word_back();
            }
            FieldKey::Delete => {
                applet.field.delete();
            }
            FieldKey::Caret(movement) => {
                applet.field.move_caret(movement);
            }
            FieldKey::SelectAll => {
                applet.field.select_all();
            }
            FieldKey::Submit | FieldKey::Cancel => {}
        }
        applet.touch();
        if applet.field.text() != before {
            applet.menu = None;
            applet.selected = 0;
            self.search_passwords();
        }
    }

    pub(crate) fn open_passwords_popup(&mut self, surface: WlSurface) {
        if self.topbar.open_popup() != Some(OpenPopup::Passwords) {
            self.topbar.toggle_popup(OpenPopup::Passwords);
        }
        self.open_passwords(Some(surface));
        self.queue_redraw();
    }

    fn open_passwords(&mut self, surface: Option<WlSurface>) {
        let target = surface.map(|surface| {
            let program = surface
                .client()
                .and_then(|client| client.get_data::<ClientState>().and_then(|state| state.identity.program.clone()))
                .map(|path| path.display().to_string());
            let (_, app_id) = crate::state::toplevel_title_and_app_id(&surface);
            Target { surface, program, app_id }
        });
        let app = target
            .as_ref()
            .map(|target| {
                let (title, app_id) = crate::state::toplevel_title_and_app_id(&target.surface);
                crate::topbar::app_display_name(
                    app_id.as_deref().unwrap_or_default(),
                    title.as_deref().unwrap_or_default(),
                )
            })
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| t(Key::PwThisApp).to_string());
        self.session.passwords.target = target;
        self.session.passwords.generation += 1;
        self.session.passwords.recent.clear();
        let click = Prefs::load().click;
        let locked = self.topbar.passwords().locked;
        let phase = if locked { Phase::Locked } else { Phase::Loading };
        let rev = self.topbar.passwords().rev.wrapping_add(1);
        *self.topbar.passwords_mut() = Applet { app, click, locked, phase, rev, ..Applet::default() };
        self.load_passwords();
    }

    fn passwords_current(&self, generation: u64) -> bool {
        self.session.passwords.generation == generation && self.topbar.open_popup() == Some(OpenPopup::Passwords)
    }

    fn load_passwords(&mut self) {
        let generation = self.session.passwords.generation;
        self.keychain(Request::Status, move |state, response| {
            let Response::Status { exists, unlocked } = response else {
                return;
            };
            state.note_keychain_locked(exists && !unlocked);
            if !state.passwords_current(generation) {
                return;
            }
            let phase = match (exists, unlocked) {
                (false, _) => Phase::Empty,
                (true, false) => Phase::Locked,
                (true, true) => Phase::Loading,
            };
            state.topbar.passwords_mut().phase = phase;
            state.queue_redraw();
            if phase == Phase::Loading {
                state.load_password_rows(generation);
            }
        });
    }

    fn load_password_rows(&mut self, generation: u64) {
        self.keychain(Request::Vaults, move |state, response| {
            if let Response::Vaults { list, .. } = response {
                state.session.passwords.vaults = vault_map(&list);
            }
            if !state.passwords_current(generation) {
                return;
            }
            let target = state.session.passwords.target.clone();
            let request = Request::Candidates {
                program: target.as_ref().and_then(|target| target.program.clone()),
                app_id: target.as_ref().and_then(|target| target.app_id.clone()),
            };
            state.keychain(request, move |state, response| {
                if !state.passwords_current(generation) {
                    return;
                }
                let fitting: Vec<Row> = match response {
                    Response::Candidates { list } => list
                        .into_iter()
                        .map(|candidate| candidate_row(candidate, &state.session.passwords.vaults))
                        .collect(),
                    Response::Locked => {
                        state.topbar.passwords_mut().phase = Phase::Locked;
                        state.queue_redraw();
                        return;
                    }
                    _ => Vec::new(),
                };
                state.keychain(Request::Items, move |state, response| {
                    if !state.passwords_current(generation) {
                        return;
                    }
                    let recent = match response {
                        Response::Items { list } => recent_rows(list, &fitting, &state.session.passwords.vaults),
                        _ => Vec::new(),
                    };
                    state.session.passwords.recent.clone_from(&recent);
                    let applet = state.topbar.passwords_mut();
                    applet.fitting = fitting;
                    if !applet.searching() {
                        applet.found = recent;
                    }
                    applet.phase = Phase::Open;
                    applet.touch();
                    state.queue_redraw();
                });
            });
        });
    }

    fn search_passwords(&mut self) {
        let query = self.topbar.passwords().field.text().trim().to_string();
        if query.is_empty() {
            let recent = self.session.passwords.recent.clone();
            let applet = self.topbar.passwords_mut();
            applet.found = recent;
            applet.touch();
            return;
        }
        let generation = self.session.passwords.generation;
        let request = Request::Search { query: query.clone(), limit: LIMIT as u32 };
        self.keychain(request, move |state, response| {
            if !state.passwords_current(generation) || state.topbar.passwords().field.text().trim() != query {
                return;
            }
            let Response::Candidates { list } = response else {
                return;
            };
            let rows =
                list.into_iter().map(|candidate| candidate_row(candidate, &state.session.passwords.vaults)).collect();
            let applet = state.topbar.passwords_mut();
            applet.found = rows;
            applet.touch();
            state.queue_redraw();
        });
    }

    fn passwords_perform(&mut self, index: usize, action: MenuAction) {
        let Some(row) = self.topbar.passwords().row(index).cloned() else {
            return;
        };
        let target = self.session.passwords.target.as_ref().map(|target| target.surface.clone());
        self.topbar.close_popup();
        let item = row.item.clone();
        let may_type = Prefs::load().type_fallback;
        match action {
            MenuAction::Open => self.open_passwords_item(&item),
            MenuAction::Copy => {
                self.keychain(Request::Secret { item, secret: SecretKind::Password }, |state, response| {
                    if let Response::Secret { secret } = response {
                        state.copy_secret(secret.expose(), Key::PwCopied);
                    }
                });
            }
            MenuAction::Username => {
                if let Some(target) = target {
                    let delivered = self.deliver_secret(&target, &row.username, may_type);
                    self.report_fill(delivered, "user name");
                }
            }
            MenuAction::Password => {
                let Some(target) = target else {
                    return;
                };
                self.keychain(Request::Secret { item, secret: SecretKind::Password }, move |state, response| {
                    if let Response::Secret { secret } = response {
                        let delivered = state.deliver_secret(&target, secret.expose(), may_type);
                        state.report_fill(delivered, "password");
                    }
                });
            }
            MenuAction::Both => {
                let Some(target) = target else {
                    return;
                };
                let username = row.username;
                self.keychain(Request::Secret { item, secret: SecretKind::Password }, move |state, response| {
                    if let Response::Secret { secret } = response {
                        let name = state.deliver_secret(&target, &username, may_type);
                        if name == Delivery::Refused {
                            state.report_fill(name, "login");
                            return;
                        }
                        let _ = state.type_into(&target, "\t");
                        let password = state.deliver_secret(&target, secret.expose(), may_type);
                        state.report_fill(password, "login");
                    }
                });
            }
            MenuAction::Code => {
                let Some(target) = target else {
                    return;
                };
                self.keychain(Request::Totp { item }, move |state, response| {
                    if let Response::Code { code } = response {
                        let delivered = state.deliver_secret(&target, &code.code, may_type);
                        state.report_fill(delivered, "one-time code");
                    }
                });
            }
        }
    }

    pub(crate) fn report_fill(&mut self, delivered: Delivery, what: &str) {
        tracing::info!(?delivered, what, "filled a saved secret");
        if delivered == Delivery::Refused {
            self.passwords_notice(Key::PwNotFilled);
        }
    }

    fn passwords_notice(&mut self, notice: Key) {
        let toast = glide_notify::Notification {
            id: NOTICE_ID,
            app_name: "Glide".to_string(),
            app_icon: "dialog-password".to_string(),
            summary: t(notice).to_string(),
            transient: true,
            ..glide_notify::Notification::default()
        };
        self.notifications.push(&mut self.dock.icons, toast, true);
        self.queue_redraw();
    }

    fn generate_password(&mut self) {
        self.topbar.close_popup();
        if let Some(password) = system_password() {
            self.copy_secret(&password, Key::PwGenerated);
        } else {
            tracing::warn!("no randomness to generate a password from");
        }
    }

    fn copy_secret(&mut self, secret: &str, notice: Key) {
        let prefs = Prefs::load();
        self.place_secret(secret, prefs.private_clipboard, prefs.clear_after);
        self.passwords_notice(notice);
    }

    pub(crate) fn place_secret(&mut self, secret: &str, private: bool, clear_after: u32) {
        self.copy_secret_to_clipboard(secret, private);
        if let Some(token) = self.session.passwords.clear.take() {
            self.loop_handle.remove(token);
        }
        if clear_after > 0 {
            let copied: Arc<str> = Arc::from(secret);
            let timer = Timer::from_duration(Duration::from_secs(u64::from(clear_after)));
            let result = self.loop_handle.insert_source(timer, move |_, (), state| {
                state.session.passwords.clear = None;
                let ours = current_data_device_selection_userdata(&state.seat)
                    .is_some_and(|owned| owned.text().is_some_and(|text| *text == *copied));
                if ours {
                    clear_data_device_selection(&state.display_handle, &state.seat);
                }
                TimeoutAction::Drop
            });
            match result {
                Ok(token) => self.session.passwords.clear = Some(token),
                Err(err) => tracing::error!(?err, "failed to arm the clipboard clearing timer"),
            }
        }
    }

    fn open_passwords_item(&mut self, item: &glide_ipc::keychain::ItemRef) {
        let command = format!("{APP} --item {}", crate::dock_drop::shell_quote(&format!("{}/{}", item.vault, item.id)));
        if let Err(err) = self.spawn_user_command(&command) {
            tracing::warn!(%err, "failed to start Passwords");
        }
    }

    fn open_passwords_app(&mut self) {
        if let Err(err) = self.spawn_user_command(APP) {
            tracing::warn!(%err, "failed to start Passwords");
        }
    }

    pub(crate) fn tick_keychain_idle(&mut self, now: Instant) {
        let last = self.session.last_user_input;
        if self.session.passwords.idle.due(now, last, || Prefs::load().lock_after) {
            tracing::info!("locking the keychain after the idle time set in Passwords");
            self.lock_keychain();
        }
    }

    pub(crate) fn note_keychain_locked(&mut self, locked: bool) {
        if self.topbar.passwords().locked != locked {
            let applet = self.topbar.passwords_mut();
            applet.locked = locked;
            applet.touch();
            self.queue_redraw();
        }
    }

    pub(crate) fn note_login_offer(&mut self, offer: bool) {
        if self.topbar.passwords().offer != offer {
            let applet = self.topbar.passwords_mut();
            applet.offer = offer;
            applet.touch();
            self.queue_redraw();
        }
    }

    pub(crate) fn sync_passwords_applet(&mut self) {
        if self.topbar.open_popup() == Some(OpenPopup::Passwords) {
            self.session.passwords.generation += 1;
            self.load_passwords();
            return;
        }
        self.keychain(Request::Status, |state, response| {
            if let Response::Status { exists, unlocked } = response {
                state.note_keychain_locked(exists && !unlocked);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use glide_ipc::keychain::ItemRef;

    use super::*;

    fn summary(id: &str, modified: u64) -> Summary {
        Summary {
            item: ItemRef::new("local", id),
            kind: ItemKind::Login,
            name: id.to_string(),
            username: format!("{id}-user"),
            folder: String::new(),
            sites: Vec::new(),
            apps: Vec::new(),
            has_password: true,
            has_totp: false,
            has_passkey: false,
            favorite: false,
            created: 0,
            modified,
            deleted: false,
            strength: 0,
            reused: 0,
        }
    }

    #[test]
    fn the_keychain_locks_once_per_idle_stretch_after_the_chosen_minutes() {
        let start = Instant::now();
        let mut idle = IdleLock::default();
        let at = |minutes: u64| start + Duration::from_mins(minutes);
        assert!(!idle.due(at(4), Some(start), || 5), "not before the chosen time");
        assert!(!idle.due(at(5), Some(start), || 0), "a choice changed to never is read again and holds");
        assert!(idle.due(at(6), Some(start), || 5));
        assert!(!idle.due(at(7), Some(start), || 5), "one lock per idle stretch");
        let touched = at(8);
        assert!(!idle.due(at(10), Some(touched), || 5));
        assert!(idle.due(at(14), Some(touched), || 5), "input starts the count again");
        let mut never = IdleLock::default();
        assert!(!never.due(at(90), Some(start), || 0), "0 keeps the keychain open");
        assert!(!never.due(at(90), None, || 5), "no input yet, no idle time");
    }

    #[test]
    fn the_notice_id_is_its_own() {
        assert!(crate::notifications::is_local(NOTICE_ID));
        for taken in [crate::autofill::FILL_ID, crate::otp::OFFER_ID, crate::cast::NOTICE_ID, crate::welcome::NOTICE_ID]
        {
            assert_ne!(NOTICE_ID, taken);
        }
    }

    #[test]
    fn recent_rows_are_newest_first_without_the_fitting_ones_or_the_trash() {
        let vaults = HashMap::from([("local".to_string(), ("Особисте".to_string(), Dot::Local))]);
        let mut trashed = summary("trashed", 9);
        trashed.deleted = true;
        let mut note = summary("note", 8);
        note.kind = ItemKind::Note;
        let fitting = vec![candidate_row(
            Candidate {
                item: ItemRef::new("local", "telegram"),
                label: "Telegram".into(),
                username: "@artem".into(),
                vault_name: "Особисте".into(),
                has_totp: false,
            },
            &vaults,
        )];
        let list = vec![summary("old", 1), trashed, note, summary("telegram", 7), summary("new", 5)];
        let rows = recent_rows(list, &fitting, &vaults);
        let names: Vec<&str> = rows.iter().map(|row| row.name.as_str()).collect();
        assert_eq!(names, ["new", "old"]);
        assert_eq!(rows[0].sub, "new-user · Особисте");
        assert_eq!(fitting[0].sub, "@artem · Особисте");
    }

    #[test]
    fn recent_rows_stop_at_the_limit() {
        let list = (0..20).map(|n| summary(&n.to_string(), n)).collect();
        assert_eq!(recent_rows(list, &[], &HashMap::new()).len(), LIMIT);
    }

    #[test]
    fn a_sub_line_skips_empty_parts() {
        assert_eq!(sub_line("", "Bitwarden"), "Bitwarden");
        assert_eq!(sub_line("artem", ""), "artem");
    }

    #[test]
    fn generated_passwords_have_every_class_and_only_the_alphabet() {
        let mut state = 7u32;
        let password = generate(|| {
            state = state.wrapping_mul(1_103_515_245).wrapping_add(12345);
            Some((state >> 16) as u8)
        })
        .expect("an endless source never runs dry");
        assert_eq!(password.len(), GENERATED_LEN);
        assert!(has_every_class(&password));
        assert!(password.bytes().all(|byte| ALPHABET.contains(&byte)));
    }

    #[test]
    fn a_dry_source_generates_nothing() {
        assert_eq!(generate(|| None), None);
    }

    #[test]
    fn the_system_source_generates_a_password() {
        assert!(system_password().is_some_and(|password| has_every_class(&password)));
    }

    #[test]
    fn keys_drive_the_list_and_the_field() {
        let none = ModifiersState::default();
        let shift = ModifiersState { shift: true, ..ModifiersState::default() };
        assert_eq!(shortcut(&none, &[Keysym::Down], Keysym::Down), Some(PwKey::Step(1)));
        assert_eq!(shortcut(&shift, &[Keysym::Tab], Keysym::ISO_Left_Tab), Some(PwKey::Step(-1)));
        assert_eq!(shortcut(&none, &[Keysym::Right], Keysym::Right), Some(PwKey::Right));
        assert_eq!(shortcut(&none, &[Keysym::Return], Keysym::Return), Some(PwKey::Activate));
        assert_eq!(shortcut(&none, &[Keysym::Escape], Keysym::Escape), Some(PwKey::Escape));
        assert_eq!(shortcut(&none, &[Keysym::space], Keysym::space), Some(PwKey::Field(FieldKey::Type(' '))));
        assert_eq!(shortcut(&none, &[Keysym::g], Keysym::g), Some(PwKey::Field(FieldKey::Type('g'))));
    }
}

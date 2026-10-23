use glide_ipc::keychain::{Candidate, Request, Response};
use smithay::reexports::wayland_server::{Resource, protocol::wl_surface::WlSurface};

use crate::focused_field::Kind;
use crate::i18n::{Key, t};
use crate::state::{ClientState, GlideState};

pub const FILL_ID: u32 = u32::MAX - 10;
pub const UNLOCK: &str = "unlock";
const FILL_PREFIX: &str = "fill:";
const MAX_CHOICES: usize = 3;

#[derive(Debug, Clone)]
pub struct Offer {
    surface: WlSurface,
    program: Option<String>,
    app_id: Option<String>,
    app: String,
    asked: bool,
}

#[must_use]
pub fn fill_key(id: u64) -> String {
    format!("{FILL_PREFIX}{id}")
}

#[must_use]
pub fn parse_fill_key(key: &str) -> Option<u64> {
    key.strip_prefix(FILL_PREFIX)?.parse().ok()
}

impl GlideState {
    pub fn offer_passwords(&mut self) {
        if !self.session.text_field.is_some_and(|field| field.kind == Kind::Password) {
            return;
        }
        let Some(surface) = self.focused_surface() else {
            return;
        };
        if self.session.autofill.as_ref().is_some_and(|offer| offer.surface == surface) {
            return;
        }
        self.begin_offer(surface, false);
    }

    pub fn offer_passwords_now(&mut self) {
        if self.screen_locked() {
            return;
        }
        if let Some(surface) = self.focused_surface() {
            self.begin_offer(surface, true);
        }
    }

    fn begin_offer(&mut self, surface: WlSurface, asked: bool) {
        let program = surface
            .client()
            .and_then(|client| client.get_data::<ClientState>().and_then(|state| state.identity.program.clone()))
            .map(|path| path.display().to_string());
        let (title, app_id) = crate::state::toplevel_title_and_app_id(&surface);
        let app = app_id.clone().or(title).unwrap_or_default();
        self.session.autofill = Some(Offer { surface, program, app_id, app, asked });
        self.keychain(Request::Status, move |state, response| match response {
            Response::Status { exists: true, unlocked: true } => state.list_candidates(),
            Response::Status { exists: true, unlocked: false } => state.show_fill_offer(None),
            Response::Status { exists: false, .. } if asked => state.show_fill_notice(Key::PasswordNoKeychain),
            _ => {}
        });
    }

    fn list_candidates(&mut self) {
        let Some(offer) = self.session.autofill.clone() else {
            return;
        };
        let request = Request::Candidates { program: offer.program, app_id: offer.app_id };
        self.keychain(request, move |state, response| match response {
            Response::Candidates { list } if !list.is_empty() => state.show_fill_offer(Some(list)),
            Response::Candidates { .. } if offer.asked => state.show_fill_notice(Key::PasswordNoneSaved),
            Response::Locked => state.show_fill_offer(None),
            _ => {}
        });
    }

    fn show_fill_notice(&mut self, body: Key) {
        let Some(offer) = self.session.autofill.as_ref() else {
            return;
        };
        let notice = glide_notify::Notification {
            id: FILL_ID,
            app_name: "Glide".to_string(),
            app_icon: "dialog-password".to_string(),
            summary: t(Key::PasswordOffer).replace("{app}", &offer.app),
            body: t(body).to_string(),
            transient: true,
            ..glide_notify::Notification::default()
        };
        self.notifications.push(&mut self.dock.icons, notice, true);
        self.queue_redraw();
    }

    fn show_fill_offer(&mut self, choices: Option<Vec<Candidate>>) {
        let Some(offer) = self.session.autofill.as_ref() else {
            return;
        };
        if self.focused_surface().as_ref() != Some(&offer.surface) {
            return;
        }
        let (summary, body, actions) = match choices {
            None => (
                t(Key::PasswordOfferLocked).to_string(),
                t(Key::PasswordOfferLockedBody).replace("{app}", &offer.app),
                vec![glide_notify::Action { key: UNLOCK.into(), label: t(Key::KeychainUnlockAction).into() }],
            ),
            Some(list) => (
                t(Key::PasswordOffer).replace("{app}", &offer.app),
                list.iter().take(MAX_CHOICES).map(|choice| choice.label.as_str()).collect::<Vec<_>>().join(" · "),
                list.iter()
                    .take(MAX_CHOICES)
                    .map(|choice| {
                        let name = if choice.username.is_empty() { &choice.label } else { &choice.username };
                        glide_notify::Action {
                            key: fill_key(choice.id),
                            label: t(Key::PasswordFill).replace("{name}", name),
                        }
                    })
                    .collect(),
            ),
        };
        let notice = glide_notify::Notification {
            id: FILL_ID,
            app_name: "Glide".to_string(),
            app_icon: "dialog-password".to_string(),
            summary,
            body,
            actions,
            transient: true,
            ..glide_notify::Notification::default()
        };
        self.notifications.push(&mut self.dock.icons, notice, true);
        self.queue_redraw();
    }

    pub fn autofill_action(&mut self, key: &str) {
        self.notifications.dismiss(FILL_ID);
        self.queue_redraw();
        if key == UNLOCK {
            self.unlock_keychain(Self::list_candidates);
            return;
        }
        let (Some(id), Some(offer)) = (parse_fill_key(key), self.session.autofill.clone()) else {
            return;
        };
        self.keychain(Request::Secret { id }, move |state, response| {
            if let Response::Secret { secret } = response {
                let delivered = state.deliver_text(&offer.surface, secret.expose());
                tracing::info!(?delivered, app = offer.app, "filled a saved password");
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fill_key_names_the_item_it_fills() {
        assert_eq!(parse_fill_key(&fill_key(42)), Some(42));
        assert_eq!(parse_fill_key(UNLOCK), None);
        assert_eq!(parse_fill_key("fill:x"), None);
    }

    #[test]
    fn the_fill_toast_id_is_its_own() {
        assert!(crate::notifications::is_local(FILL_ID));
        assert_ne!(FILL_ID, crate::otp::OFFER_ID);
        assert_ne!(FILL_ID, crate::welcome::NOTICE_ID);
    }
}

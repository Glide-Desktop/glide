use glide_ipc::keychain::{Candidate, ItemRef, Request, Response, SecretKind};
use smithay::reexports::wayland_server::{Resource, protocol::wl_surface::WlSurface};

use crate::focused_field::Kind;
use crate::i18n::{Key, t};
use crate::state::{ClientState, GlideState};

pub const FILL_ID: u32 = u32::MAX - 10;
pub const UNLOCK: &str = "unlock";
const FILL_PREFIX: &str = "fill:";
const LATER: &str = "later";
const OTHER: &str = "other";

#[must_use]
fn more_logins(lang: crate::i18n::Lang, n: usize) -> String {
    let key = match lang.plural(n as u64) {
        glide_app::i18n::Plural::One => Key::FillMoreOne,
        glide_app::i18n::Plural::Few => Key::FillMoreFew,
        glide_app::i18n::Plural::Many => Key::FillMoreMany,
    };
    key.text(lang).replace("{n}", &n.to_string())
}

#[derive(Debug, Clone)]
pub struct Offer {
    surface: WlSurface,
    program: Option<String>,
    app_id: Option<String>,
    app: String,
    asked: bool,
}

#[must_use]
pub fn fill_key(item: &ItemRef) -> String {
    format!("{FILL_PREFIX}{}/{}", item.vault, item.id)
}

#[must_use]
pub fn parse_fill_key(key: &str) -> Option<ItemRef> {
    let (vault, id) = key.strip_prefix(FILL_PREFIX)?.split_once('/')?;
    (!vault.is_empty() && !id.is_empty()).then(|| ItemRef::new(vault, id))
}

impl GlideState {
    pub fn offer_passwords(&mut self) {
        if !self.session.text_field.is_some_and(|field| field.kind == Kind::Password) {
            self.note_login_offer(false);
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
        let app = crate::topbar::app_display_name(
            app_id.as_deref().unwrap_or_default(),
            title.as_deref().unwrap_or_default(),
        );
        let app = if app.trim().is_empty() { t(Key::PwThisApp).to_string() } else { app };
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
            Response::Candidates { list } if !list.is_empty() => {
                let same = state.focused_surface().as_ref() == Some(&offer.surface);
                state.note_login_offer(same);
                state.show_fill_offer(Some(list));
            }
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
        let later = glide_notify::Action { key: LATER.into(), label: t(Key::FillLater).into() };
        let (summary, body, actions) = match choices {
            None => (
                t(Key::PwLockedTitle).to_string(),
                t(Key::PwLockedBody).replace("{app}", &offer.app),
                vec![glide_notify::Action { key: UNLOCK.into(), label: t(Key::PwUnlock).into() }, later],
            ),
            Some(list) => {
                let Some(first) = list.first() else {
                    return;
                };
                let name = if first.username.is_empty() { &first.label } else { &first.username };
                let title =
                    if first.item.vault == glide_ipc::keychain::LOCAL { Key::FillTitle } else { Key::FillCloudTitle };
                let mut body = t(Key::FillFound).replace("{name}", name).replace("{vault}", &first.vault_name);
                if list.len() > 1 {
                    body.push(' ');
                    body.push_str(&more_logins(crate::i18n::language(), list.len() - 1));
                }
                let mut actions = vec![glide_notify::Action {
                    key: fill_key(&first.item),
                    label: t(Key::PasswordFill).replace("{name}", name),
                }];
                if list.len() > 1 {
                    actions.push(glide_notify::Action { key: OTHER.into(), label: t(Key::FillOther).into() });
                }
                actions.push(later);
                (t(title).to_string(), body, actions)
            }
        };
        let notice = glide_notify::Notification {
            id: FILL_ID,
            app_name: t(Key::FillSource).replace("{app}", &offer.app),
            app_icon: "dialog-password".to_string(),
            desktop_entry: offer.app_id.clone().unwrap_or_default(),
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
            self.unlock_keychain(None, Self::list_candidates);
            return;
        }
        if key == LATER {
            return;
        }
        if key == OTHER {
            if let Some(offer) = self.session.autofill.clone() {
                self.open_passwords_popup(offer.surface);
            }
            return;
        }
        let (Some(item), Some(offer)) = (parse_fill_key(key), self.session.autofill.clone()) else {
            return;
        };
        self.keychain(Request::Secret { item, secret: SecretKind::Password }, move |state, response| {
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
        let item = ItemRef::new("bw-1", "5f3c-9a");
        assert_eq!(parse_fill_key(&fill_key(&item)), Some(item));
        assert_eq!(parse_fill_key(UNLOCK), None);
        assert_eq!(parse_fill_key("fill:x"), None);
        assert_eq!(parse_fill_key("fill:/x"), None);
    }

    #[test]
    fn the_other_logins_are_counted_with_the_right_plural() {
        use crate::i18n::Lang;
        assert_eq!(more_logins(Lang::Uk, 1), "Ще 1 логін підходить цьому застосунку.");
        assert_eq!(more_logins(Lang::Uk, 3), "Ще 3 логіни підходять цьому застосунку.");
        assert_eq!(more_logins(Lang::Uk, 11), "Ще 11 логінів підходять цьому застосунку.");
        assert_eq!(more_logins(Lang::En, 1), "1 more login fits this app.");
        assert_eq!(more_logins(Lang::En, 2), "2 more logins fit this app.");
    }

    #[test]
    fn the_fill_toast_id_is_its_own() {
        assert!(crate::notifications::is_local(FILL_ID));
        assert_ne!(FILL_ID, crate::otp::OFFER_ID);
        assert_ne!(FILL_ID, crate::welcome::NOTICE_ID);
    }
}

use std::fmt;

use glide_ipc::keychain::{AppLink, Conflict, Draft, Hidden, ItemKind, Keep, Passkey, Send, SendDraft, Site, Vault};

use crate::vault::{PastSecret, Secret, StoredCard, StoredField};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fault {
    Locked,
    WrongPassword,
    NotFound,
    Unsupported,
    Failed(String),
}

impl fmt::Display for Fault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Locked => f.write_str("the vault is locked"),
            Self::WrongPassword => f.write_str("the password is wrong"),
            Self::NotFound => f.write_str("no such item"),
            Self::Unsupported => f.write_str("this vault cannot do that"),
            Self::Failed(message) => f.write_str(message),
        }
    }
}

impl From<crate::vault::Error> for Fault {
    fn from(error: crate::vault::Error) -> Self {
        match error {
            crate::vault::Error::WrongPassword => Self::WrongPassword,
            other => Self::Failed(other.to_string()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Record {
    pub id: String,
    pub kind: ItemKind,
    pub name: String,
    pub username: String,
    pub password: Secret,
    pub totp: Secret,
    pub notes: String,
    pub sites: Vec<Site>,
    pub apps: Vec<AppLink>,
    pub fields: Vec<StoredField>,
    pub folder: String,
    pub favorite: bool,
    pub created: u64,
    pub modified: u64,
    pub deleted: Option<u64>,
    pub history: Vec<PastSecret>,
    pub card: Option<StoredCard>,
    pub passkeys: Vec<Passkey>,
}

impl Record {
    #[must_use]
    pub fn password_text(&self) -> String {
        String::from_utf8_lossy(self.password.expose()).into_owned()
    }

    pub fn apply(&mut self, draft: &Draft, now: u64) {
        self.kind = draft.kind;
        self.name = draft.name.trim().to_string();
        self.username = draft.username.trim().to_string();
        if self.history.is_empty() {
            self.history = draft
                .history
                .iter()
                .map(|(secret, until)| PastSecret {
                    secret: Secret::new(secret.expose().as_bytes().to_vec()),
                    until: *until,
                })
                .collect();
        }
        if let Some(password) = &draft.password
            && password.expose().as_bytes() != self.password.expose()
        {
            if !self.password.expose().is_empty() {
                self.history.push(PastSecret { secret: self.password.clone(), until: now });
            }
            self.password = Secret::new(password.expose().as_bytes().to_vec());
        }
        if let Some(totp) = &draft.totp {
            self.totp = Secret::new(totp.expose().trim().as_bytes().to_vec());
        }
        self.notes.clone_from(&draft.notes);
        self.sites = draft.sites.iter().filter(|site| !site.uri.trim().is_empty()).cloned().collect();
        self.apps =
            draft.apps.iter().filter(|app| !app.app_id.trim().is_empty() || !app.program.is_empty()).cloned().collect();
        let previous = std::mem::take(&mut self.fields);
        self.fields = draft
            .fields
            .iter()
            .filter(|field| !field.name.trim().is_empty())
            .map(|field| {
                let name = field.name.trim().to_string();
                let unseen = field.hidden && field.value.is_empty();
                let kept = previous.iter().find(|old| unseen && old.hidden && old.name == name);
                StoredField {
                    value: kept.map_or_else(|| secret_of(&field.value), |old| old.value.clone()),
                    name,
                    hidden: field.hidden,
                }
            })
            .collect();
        self.folder = draft.folder.trim().to_string();
        self.favorite = draft.favorite;
        self.card = draft.card.as_ref().map(|card| {
            let previous = self.card.clone().unwrap_or_default();
            StoredCard {
                holder: card.holder.clone(),
                brand: card.brand.clone(),
                number: if card.number.is_empty() { previous.number } else { secret_of(&card.number) },
                code: if card.code.is_empty() { previous.code } else { secret_of(&card.code) },
                expiry: card.expiry.clone(),
            }
        });
        self.modified = now;
    }
}

fn secret_of(hidden: &Hidden) -> Secret {
    Secret::new(hidden.expose().as_bytes().to_vec())
}

pub trait Provider: std::marker::Send {
    fn vault(&self) -> Vault;

    fn exists(&self) -> bool;

    fn unlocked(&self) -> bool;

    fn create(&mut self, _password: &Hidden) -> Result<(), Fault> {
        Err(Fault::Unsupported)
    }

    fn unlock(&mut self, password: &Hidden) -> Result<(), Fault>;

    fn lock(&mut self);

    fn change_password(&mut self, _old: &Hidden, _new: &Hidden) -> Result<(), Fault> {
        Err(Fault::Unsupported)
    }

    fn records(&self) -> Result<&[Record], Fault>;

    fn folders(&self) -> Result<Vec<String>, Fault>;

    fn save(&mut self, draft: &Draft, now: u64) -> Result<String, Fault>;

    fn trash(&mut self, id: &str, now: u64) -> Result<(), Fault>;

    fn restore(&mut self, id: &str) -> Result<(), Fault>;

    fn purge(&mut self, id: &str) -> Result<(), Fault>;

    fn sync(&mut self) -> Result<(), Fault> {
        Ok(())
    }

    fn conflicts(&self) -> Vec<Conflict> {
        Vec::new()
    }

    fn resolve(&mut self, _id: &str, _keep: Keep) -> Result<(), Fault> {
        Err(Fault::Unsupported)
    }

    fn sends(&self) -> Result<Vec<Send>, Fault> {
        Err(Fault::Unsupported)
    }

    fn create_send(&mut self, _draft: &SendDraft) -> Result<Send, Fault> {
        Err(Fault::Unsupported)
    }

    fn send_text(&self, _id: &str) -> Result<Hidden, Fault> {
        Err(Fault::Unsupported)
    }

    fn edit_send(&mut self, _id: &str, _draft: &SendDraft, _keep_password: bool) -> Result<Send, Fault> {
        Err(Fault::Unsupported)
    }

    fn disable_send(&mut self, _id: &str) -> Result<(), Fault> {
        Err(Fault::Unsupported)
    }

    fn delete_send(&mut self, _id: &str) -> Result<(), Fault> {
        Err(Fault::Unsupported)
    }

    fn rename(&mut self, _name: &str) {}

    fn take_offline(&mut self) -> Option<serde_json::Value> {
        None
    }

    fn registration(&self) -> Option<crate::kdbx::Registered> {
        None
    }

    fn forget(&mut self) -> Result<(), Fault> {
        Err(Fault::Unsupported)
    }

    fn app_secrets(&self) -> Result<&[crate::vault::Item], Fault> {
        Err(Fault::Unsupported)
    }

    fn edit_app_secrets(&mut self, _edit: &mut dyn FnMut(&mut Vec<crate::vault::Item>, &mut u64)) -> Result<(), Fault> {
        Err(Fault::Unsupported)
    }

    fn begin_sync(&mut self, _force: bool, _now: u64) -> Option<crate::bitwarden::SyncJob> {
        None
    }

    fn finish_sync(&mut self, _done: crate::bitwarden::SyncDone, _now: u64) {}
}

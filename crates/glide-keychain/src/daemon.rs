use std::collections::HashMap;
use std::path::PathBuf;

use glide_ipc::keychain::{
    Candidate, Card, Code, Detail, Field, Folder, Hidden, ItemKind, ItemRef, Keep, LOCAL, LOGIN, Report, Request,
    Response, SecretKind, Summary, Vault, strength,
};

use crate::bitwarden::{Bitwarden, LoginError, SyncDone, SyncJob};
use crate::local::Local;
use crate::login::LoginKeyring;
use crate::provider::{Fault, Provider, Record};
use crate::totp::Totp;
use crate::vault::Params;

const OLD_AFTER: u64 = 365 * 24 * 60 * 60;
pub const LOCAL_NAME: &str = "Особисте";

#[must_use]
pub fn default_path() -> Option<PathBuf> {
    glide_config::data_home().map(|dir| dir.join("glide").join("keychain"))
}

pub struct Daemon {
    providers: Vec<Box<dyn Provider>>,
    default: String,
    clock: fn() -> u64,
    accounts: Option<PathBuf>,
    leaks: crate::leaks::Leaks,
    login: Option<Login>,
}

struct Login {
    keyring: LoginKeyring,
    user: String,
    uid: u32,
    runtime: Option<PathBuf>,
    handed_off: bool,
}

fn take_file(path: &std::path::Path) -> Option<zeroize::Zeroizing<Vec<u8>>> {
    let bytes = zeroize::Zeroizing::new(std::fs::read(path).ok()?);
    let _ = std::fs::remove_file(path);
    Some(bytes)
}

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
struct Settings {
    #[serde(default)]
    default: String,
    #[serde(default)]
    local_name: String,
    #[serde(default)]
    keepass: Vec<crate::kdbx::Registered>,
}

fn settings_path(accounts: &std::path::Path) -> PathBuf {
    accounts.with_file_name("keychain-settings.json")
}

impl std::fmt::Debug for Daemon {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Daemon").field("vaults", &self.providers.len()).finish_non_exhaustive()
    }
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.as_secs())
}

fn failed(fault: Fault) -> Response {
    match fault {
        Fault::Locked => Response::Locked,
        Fault::WrongPassword => Response::WrongPassword,
        Fault::Unsupported => Response::Unsupported,
        other => Response::Error { message: other.to_string() },
    }
}

fn done(result: Result<(), Fault>) -> Response {
    result.map_or_else(failed, |()| Response::Done)
}

fn text(secret: &crate::vault::Secret) -> String {
    String::from_utf8_lossy(secret.expose()).into_owned()
}

#[must_use]
pub fn mutates(request: &Request) -> bool {
    matches!(
        request,
        Request::Create { .. }
            | Request::Unlock { .. }
            | Request::Lock { .. }
            | Request::ChangePassword { .. }
            | Request::Save { .. }
            | Request::Trash { .. }
            | Request::Restore { .. }
            | Request::Purge { .. }
            | Request::AddBitwarden { .. }
            | Request::AddKeePass { .. }
            | Request::CheckLeaks
            | Request::Import { .. }
            | Request::RemoveVault { .. }
            | Request::RenameVault { .. }
            | Request::SetDefault { .. }
            | Request::Sync { .. }
            | Request::Resolve { .. }
            | Request::CreateSend { .. }
            | Request::EditSend { .. }
            | Request::DisableSend { .. }
            | Request::DeleteSend { .. }
    )
}

impl Daemon {
    #[must_use]
    pub fn new(path: PathBuf, params: Params) -> Self {
        let accounts = path.with_file_name("bitwarden");
        Self::with_accounts(path, params, accounts)
    }

    #[must_use]
    pub fn with_accounts(path: PathBuf, params: Params, accounts: PathBuf) -> Self {
        let settings: Settings = std::fs::read(settings_path(&accounts))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        let name = if settings.local_name.trim().is_empty() { LOCAL_NAME } else { settings.local_name.as_str() };
        let login = path.parent().zip(crate::login::current_user()).map(|(dir, (user, uid))| Login {
            keyring: LoginKeyring::load(dir),
            user,
            uid,
            runtime: None,
            handed_off: false,
        });
        let mut providers: Vec<Box<dyn Provider>> = vec![Box::new(Local::new(path, params, name))];
        for account in Bitwarden::load_all(&accounts) {
            providers.push(Box::new(account));
        }
        for registered in settings.keepass.iter().cloned() {
            providers.push(Box::new(crate::kdbx::KeePass::new(registered)));
        }
        let mut daemon = Self::with_providers(providers);
        daemon.accounts = Some(accounts);
        daemon.login = login;
        if daemon.index(&settings.default).is_ok() {
            daemon.default = settings.default;
        }
        daemon
    }

    #[must_use]
    pub fn with_providers(providers: Vec<Box<dyn Provider>>) -> Self {
        Self {
            providers,
            default: LOCAL.into(),
            clock: now,
            accounts: None,
            leaks: crate::leaks::Leaks::default(),
            login: None,
        }
    }

    fn store_settings(&self) {
        let Some(accounts) = &self.accounts else {
            return;
        };
        let local_name = self.provider(LOCAL).map(|local| local.vault().name).unwrap_or_default();
        let keepass = self.providers.iter().filter_map(|provider| provider.registration()).collect();
        let settings = Settings { default: self.default.clone(), local_name, keepass };
        if let Ok(bytes) = serde_json::to_vec_pretty(&settings) {
            if let Some(dir) = accounts.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = crate::vault::write_atomically(&settings_path(accounts), &bytes);
        }
    }

    #[must_use]
    pub fn accounts_dir(&self) -> Option<PathBuf> {
        self.accounts.clone()
    }

    #[must_use]
    pub fn now(&self) -> u64 {
        (self.clock)()
    }

    pub fn adopt_bitwarden(&mut self, mut account: Bitwarden) -> Response {
        let vault = account.vault().id;
        if let Some(existing) = self.providers.iter().position(|provider| {
            let known = provider.vault();
            known.kind == glide_ipc::keychain::VaultKind::Bitwarden
                && known.account == account.account().email
                && known.server == account.vault().server
        }) {
            if let Some(offline) = self.providers[existing].take_offline() {
                account.inherit_offline(offline);
            }
            let _ = self.providers[existing].forget();
            self.providers.remove(existing);
        }
        self.providers.push(Box::new(account));
        Response::VaultAdded { vault }
    }

    fn add_keepass(&mut self, path: &str, password: &Hidden, name: &str) -> Response {
        let path = if path.starts_with("file://") {
            let Some(path) = glide_config::uri::local_path(path) else {
                return Response::Error { message: "the KeePass file is not on this computer".into() };
            };
            path
        } else {
            PathBuf::from(path.trim())
        };
        if !path.is_absolute() {
            return Response::Error { message: "the KeePass file needs a full path".into() };
        }
        if let Some(known) = self
            .providers
            .iter()
            .position(|provider| provider.registration().is_some_and(|registered| registered.path == path))
        {
            let vault = self.providers[known].vault().id;
            return self.providers[known].unlock(password).map_or_else(failed, |()| Response::VaultAdded { vault });
        }
        let number = (1..=self.providers.len() + 1)
            .find(|number| self.index(&format!("keepass-{number}")).is_err())
            .unwrap_or(self.providers.len() + 1);
        let name = match name.trim() {
            "" => path.file_stem().map_or_else(|| "KeePass".to_string(), |stem| stem.to_string_lossy().into_owned()),
            name => name.to_string(),
        };
        let id = format!("keepass-{number}");
        let mut vault = crate::kdbx::KeePass::new(crate::kdbx::Registered { id: id.clone(), name, path });
        if let Err(fault) = vault.unlock(password) {
            return failed(fault);
        }
        self.providers.push(Box::new(vault));
        self.store_settings();
        Response::VaultAdded { vault: id }
    }

    fn add_bitwarden(&mut self, login: &glide_ipc::keychain::Login) -> Response {
        let Some(dir) = self.accounts.clone() else {
            return Response::Unsupported;
        };
        match Bitwarden::login(&dir, login, (self.clock)()) {
            Ok(account) => self.adopt_bitwarden(account),
            Err(error) => login_failed(error),
        }
    }

    #[must_use]
    pub fn leak_hashes(&self) -> Vec<String> {
        let hashes: Vec<String> = self
            .open_records()
            .filter(|(_, record)| record.kind == ItemKind::Login && !record.password.expose().is_empty())
            .map(|(_, record)| crate::leaks::sha1_hex(&record.password_text()))
            .collect();
        self.leaks.stale(hashes.iter(), (self.clock)())
    }

    pub fn store_leaks(&mut self, found: HashMap<String, bool>) {
        let now = (self.clock)();
        self.leaks.store(found, now);
    }

    #[must_use]
    pub fn app_secrets_unlocked(&self) -> bool {
        self.login.as_ref().is_some_and(|login| login.keyring.unlocked())
    }

    pub fn app_secrets(&self) -> Result<Vec<crate::vault::Item>, Fault> {
        Ok(self.login.as_ref().ok_or(Fault::Unsupported)?.keyring.items()?.to_vec())
    }

    pub fn edit_app_secrets(
        &mut self, edit: &mut dyn FnMut(&mut Vec<crate::vault::Item>, &mut u64),
    ) -> Result<(), Fault> {
        self.login.as_mut().ok_or(Fault::Unsupported)?.keyring.edit(edit)
    }

    fn unlock_login(&mut self, password: &Hidden, verified: bool) -> Response {
        let Some(login) = self.login.as_mut() else {
            return Response::Unsupported;
        };
        if verified && !login.handed_off && login.keyring.guard() != crate::login::Guard::Login {
            return Response::Done;
        }
        let Some(wrapping) = glide_login_key::wrapping_key(password.expose().as_bytes(), &login.user, login.uid) else {
            return Response::Error { message: "the login password could not be stretched".into() };
        };
        let result = login.keyring.offer(&wrapping, verified);
        self.migrate_app_secrets();
        done(result)
    }

    pub fn watch_login_handoff(&mut self, runtime: PathBuf) {
        if let Some(login) = self.login.as_mut() {
            login.runtime = Some(runtime);
        }
        self.take_login_handoff();
    }

    pub fn take_login_handoff(&mut self) {
        let Some(login) = self.login.as_mut() else {
            return;
        };
        let Some(runtime) = login.runtime.clone() else {
            return;
        };
        if !login.handed_off
            && let Ok(bytes) = std::fs::read(runtime.join(glide_login_key::KEY_FILE)).map(zeroize::Zeroizing::new)
            && let Some(wrapping) = glide_login_key::key_from(&bytes)
        {
            login.handed_off = true;
            if let Err(fault) = login.keyring.offer(&wrapping, true) {
                eprintln!(
                    "glide-keychain: the login password from this session did not open the login keyring ({fault})"
                );
            }
        }
        if let Some(bytes) = take_file(&runtime.join(glide_login_key::REKEY_FILE))
            && let Some((old, new)) = glide_login_key::rekey_from(&bytes)
        {
            match login.keyring.rekey(&old, &new) {
                Ok(()) => {
                    let _ = std::fs::remove_file(runtime.join(glide_login_key::KEY_FILE));
                }
                Err(fault) => {
                    eprintln!("glide-keychain: the login keyring did not follow the password change ({fault})");
                }
            }
        }
        self.migrate_app_secrets();
    }

    fn migrate_app_secrets(&mut self) {
        let Ok(old) = self.provider(LOCAL).and_then(|local| local.app_secrets().map(<[_]>::to_vec)) else {
            return;
        };
        if old.is_empty() || !self.app_secrets_unlocked() {
            return;
        }
        let mut skipped: Vec<u64> = Vec::new();
        let moved = self.edit_app_secrets(&mut |items, next| {
            for item in &old {
                if items.iter().any(|known| known.attributes == item.attributes) {
                    skipped.push(item.id);
                    continue;
                }
                *next += 1;
                items.push(crate::vault::Item { id: *next, ..item.clone() });
            }
        });
        match moved {
            Ok(()) => {
                if !skipped.is_empty() {
                    eprintln!(
                        "glide-keychain: {} program secrets stay in the local vault, the keyring holds others with the same attributes",
                        skipped.len()
                    );
                }
                let cleared = self.provider_mut(LOCAL).and_then(|local| {
                    local.edit_app_secrets(&mut |items, _| items.retain(|item| skipped.contains(&item.id)))
                });
                if let Err(fault) = cleared {
                    eprintln!("glide-keychain: program secrets were copied but stay in the local vault too ({fault})");
                }
            }
            Err(fault) => eprintln!("glide-keychain: program secrets stay in the local vault ({fault})"),
        }
    }

    #[must_use]
    pub fn export_records(&self, vault: Option<&str>) -> Vec<Record> {
        self.open_records()
            .filter(|(provider, record)| {
                record.deleted.is_none() && vault.is_none_or(|vault| provider.vault().id == vault)
            })
            .map(|(_, record)| record.clone())
            .collect()
    }

    pub fn import_one(&mut self, vault: &str, mut draft: glide_ipc::keychain::Draft) -> bool {
        draft.vault = vault.to_string();
        draft.item = None;
        matches!(self.handle(Request::Save { draft: Box::new(draft) }), Response::Saved { .. })
    }

    fn check_leaks(&mut self) -> Response {
        let hashes = self.leak_hashes();
        match crate::leaks::check(&hashes, crate::leaks::fetch_range) {
            Ok(found) => {
                self.store_leaks(found);
                Response::Done
            }
            Err(message) => Response::Error { message },
        }
    }

    pub fn sync_jobs(&mut self, force: bool) -> Vec<(String, SyncJob)> {
        let now = (self.clock)();
        self.providers
            .iter_mut()
            .filter_map(|provider| provider.begin_sync(force, now).map(|job| (provider.vault().id, job)))
            .collect()
    }

    pub fn finish_sync(&mut self, vault: &str, done: SyncDone) {
        let now = (self.clock)();
        if let Ok(provider) = self.provider_mut(vault) {
            provider.finish_sync(done, now);
        }
    }

    #[must_use]
    pub fn with_clock(mut self, clock: fn() -> u64) -> Self {
        self.clock = clock;
        self
    }

    pub fn add(&mut self, provider: Box<dyn Provider>) {
        self.providers.push(provider);
    }

    fn index(&self, vault: &str) -> Result<usize, Fault> {
        self.providers.iter().position(|provider| provider.vault().id == vault).ok_or(Fault::NotFound)
    }

    fn provider(&self, vault: &str) -> Result<&dyn Provider, Fault> {
        Ok(self.providers[self.index(vault)?].as_ref())
    }

    fn provider_mut(&mut self, vault: &str) -> Result<&mut Box<dyn Provider>, Fault> {
        let index = self.index(vault)?;
        Ok(&mut self.providers[index])
    }

    fn record(&self, item: &ItemRef) -> Result<&Record, Fault> {
        self.provider(&item.vault)?.records()?.iter().find(|record| record.id == item.id).ok_or(Fault::NotFound)
    }

    fn open_records(&self) -> impl Iterator<Item = (&dyn Provider, &Record)> {
        self.providers
            .iter()
            .filter_map(|provider| provider.records().ok().map(|records| (provider.as_ref(), records)))
            .flat_map(|(provider, records)| records.iter().map(move |record| (provider, record)))
    }

    fn reuse(&self) -> HashMap<String, Vec<ItemRef>> {
        let mut seen: HashMap<String, Vec<ItemRef>> = HashMap::new();
        for (provider, record) in self.open_records() {
            if record.deleted.is_none() && record.kind == ItemKind::Login && !record.password.expose().is_empty() {
                seen.entry(record.password_text()).or_default().push(ItemRef::new(provider.vault().id, &record.id));
            }
        }
        seen
    }

    fn summary(vault: &str, record: &Record, reuse: &HashMap<String, Vec<ItemRef>>) -> Summary {
        let password = record.password_text();
        let reused = reuse.get(&password).map_or(0, |group| group.len().saturating_sub(1)) as u32;
        Summary {
            item: ItemRef::new(vault, &record.id),
            kind: record.kind,
            name: record.name.clone(),
            username: record.username.clone(),
            folder: record.folder.clone(),
            sites: record.sites.iter().map(|site| site.uri.clone()).collect(),
            apps: record.apps.iter().map(|app| app.app_id.clone()).collect(),
            has_password: !password.is_empty(),
            has_totp: !record.totp.expose().is_empty(),
            has_passkey: !record.passkeys.is_empty(),
            favorite: record.favorite,
            created: record.created,
            modified: record.modified,
            deleted: record.deleted.is_some(),
            strength: strength(&password),
            reused,
        }
    }

    fn items(&self) -> Vec<Summary> {
        let reuse = self.reuse();
        let mut list: Vec<Summary> =
            self.open_records().map(|(provider, record)| Self::summary(&provider.vault().id, record, &reuse)).collect();
        list.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then(a.item.cmp(&b.item)));
        list
    }

    fn detail(&self, item: &ItemRef) -> Result<Detail, Fault> {
        let record = self.record(item)?;
        Ok(record_detail(&item.vault, record, &self.reuse()))
    }
}

#[must_use]
pub fn import_failed(error: &crate::transfer::ImportError) -> Response {
    match error {
        crate::transfer::ImportError::NeedsPassword => Response::NeedsFilePassword,
        crate::transfer::ImportError::WrongPassword => Response::WrongPassword,
        other => Response::Error { message: other.to_string() },
    }
}

#[must_use]
pub fn export_response(records: &[Record], password: &Hidden) -> Response {
    if password.is_empty() {
        return Response::Error { message: "an export needs a file password".into() };
    }
    let borrowed: Vec<&Record> = records.iter().collect();
    match crate::transfer::export(&borrowed, password.expose()) {
        Ok(data) => Response::Exported { data, count: records.len() as u32 },
        Err(error) => Response::Error { message: error.to_string() },
    }
}

#[must_use]
pub fn login_failed(error: LoginError) -> Response {
    match error {
        LoginError::TwoFactor(methods) => Response::NeedsTwoFactor { methods },
        LoginError::DeviceCode => Response::NeedsDeviceCode,
        LoginError::WrongPassword => Response::WrongPassword,
        LoginError::Failed(message) => Response::Error { message },
    }
}

#[must_use]
pub(crate) fn record_detail(vault: &str, record: &Record, reuse: &HashMap<String, Vec<ItemRef>>) -> Detail {
    let card = record.card.as_ref().map(|card| {
        let number = text(&card.number);
        let digits: String = number.chars().filter(char::is_ascii_digit).collect();
        Card {
            holder: card.holder.clone(),
            brand: card.brand.clone(),
            last_digits: digits.chars().skip(digits.len().saturating_sub(4)).collect(),
            expiry: card.expiry.clone(),
        }
    });
    Detail {
        summary: Daemon::summary(vault, record, reuse),
        notes: record.notes.clone(),
        sites: record.sites.clone(),
        apps: record.apps.clone(),
        fields: record
            .fields
            .iter()
            .map(|field| Field {
                name: field.name.clone(),
                value: if field.hidden { String::new() } else { text(&field.value) },
                hidden: field.hidden,
            })
            .collect(),
        passkeys: record.passkeys.clone(),
        history: record.history.len() as u32,
        card,
        password_length: record.password_text().chars().count() as u32,
    }
}

impl Daemon {
    fn secret(&self, item: &ItemRef, kind: &SecretKind) -> Result<Hidden, Fault> {
        let record = self.record(item)?;
        let secret = match kind {
            SecretKind::Password => text(&record.password),
            SecretKind::Totp => text(&record.totp),
            SecretKind::Field(name) => record
                .fields
                .iter()
                .find(|field| &field.name == name)
                .map(|field| text(&field.value))
                .ok_or(Fault::NotFound)?,
            SecretKind::FieldAt(index) => {
                record.fields.get(*index).map(|field| text(&field.value)).ok_or(Fault::NotFound)?
            }
            SecretKind::CardNumber => record.card.as_ref().map(|card| text(&card.number)).ok_or(Fault::NotFound)?,
            SecretKind::CardCode => record.card.as_ref().map(|card| text(&card.code)).ok_or(Fault::NotFound)?,
        };
        Ok(Hidden::new(secret))
    }

    fn code(&self, item: &ItemRef) -> Result<Code, Fault> {
        let record = self.record(item)?;
        let totp = Totp::parse(&text(&record.totp)).ok_or(Fault::NotFound)?;
        let at = (self.clock)();
        Ok(Code { code: totp.code_at(at), seconds_left: totp.seconds_left(at), period: totp.period })
    }

    fn candidate(provider: &dyn Provider, record: &Record) -> Candidate {
        Candidate {
            item: ItemRef::new(provider.vault().id, &record.id),
            label: record.name.clone(),
            username: record.username.clone(),
            vault_name: provider.vault().name,
            has_totp: !record.totp.expose().is_empty(),
        }
    }

    fn candidates(&self, program: Option<&str>, app_id: Option<&str>) -> Vec<Candidate> {
        let same = |value: &str, wanted: Option<&str>| {
            wanted.filter(|wanted| !wanted.is_empty()).is_some_and(|wanted| value.eq_ignore_ascii_case(wanted))
        };
        let mut found: Vec<(u64, Candidate)> = self
            .open_records()
            .filter(|(_, record)| {
                record.deleted.is_none()
                    && record.apps.iter().any(|app| same(&app.program, program) || same(&app.app_id, app_id))
            })
            .map(|(provider, record)| (record.modified, Self::candidate(provider, record)))
            .collect();
        found.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.item.cmp(&b.1.item)));
        found.into_iter().map(|(_, candidate)| candidate).collect()
    }

    fn search(&self, query: &str, limit: u32) -> Vec<Candidate> {
        let query = query.trim().to_lowercase();
        let mut found: Vec<(u8, u64, Candidate)> = self
            .open_records()
            .filter(|(_, record)| record.deleted.is_none() && record.kind == ItemKind::Login)
            .filter_map(|(provider, record)| {
                let name = record.name.to_lowercase();
                let rank = if query.is_empty() {
                    2
                } else if name.starts_with(&query) {
                    0
                } else if name.contains(&query)
                    || record.username.to_lowercase().contains(&query)
                    || record.sites.iter().any(|site| site.uri.to_lowercase().contains(&query))
                {
                    1
                } else {
                    return None;
                };
                Some((rank, record.modified, Self::candidate(provider, record)))
            })
            .collect();
        found.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)).then(a.2.label.cmp(&b.2.label)));
        found.into_iter().take(limit as usize).map(|(_, _, candidate)| candidate).collect()
    }

    fn report(&self) -> Report {
        let at = (self.clock)();
        let mut report = Report::default();
        for (provider, record) in self.open_records() {
            if record.deleted.is_some() || record.kind != ItemKind::Login || record.password.expose().is_empty() {
                continue;
            }
            report.checked += 1;
            let item = ItemRef::new(provider.vault().id, &record.id);
            if strength(&record.password_text()) <= 1 {
                report.weak.push(item.clone());
            }
            if self.leaks.leaked(&crate::leaks::sha1_hex(&record.password_text())) {
                report.leaked.push(item.clone());
            }
            if at.saturating_sub(record.modified) > OLD_AFTER {
                report.old.push(item);
            }
        }
        let mut groups: Vec<Vec<ItemRef>> = self.reuse().into_values().filter(|group| group.len() > 1).collect();
        for group in &mut groups {
            group.sort();
        }
        groups.sort_by(|a, b| b.len().cmp(&a.len()).then(a.cmp(b)));
        report.reused = groups;
        report
    }

    fn save(&mut self, draft: &glide_ipc::keychain::Draft) -> Result<ItemRef, Fault> {
        let at = (self.clock)();
        let target = if draft.vault.is_empty() { self.default.clone() } else { draft.vault.clone() };
        match &draft.item {
            Some(existing) if existing.vault != target => {
                let mut record = self.record(existing)?.clone();
                if !record.passkeys.is_empty() {
                    return Err(Fault::Unsupported);
                }
                record.apply(draft, at);
                let mut moved = crate::kdbx::draft_of(&record);
                moved.vault.clone_from(&target);
                let id = self.provider_mut(&target)?.save(&moved, at)?;
                self.provider_mut(&existing.vault)?.trash(&existing.id, at)?;
                Ok(ItemRef::new(target, id))
            }
            _ => {
                let id = self.provider_mut(&target)?.save(draft, at)?;
                Ok(ItemRef::new(target, id))
            }
        }
    }

    #[must_use]
    pub fn vaults(&self) -> Vec<Vault> {
        self.providers.iter().map(|provider| provider.vault()).collect()
    }

    pub fn handle(&mut self, request: Request) -> Response {
        match request {
            Request::Status => Response::Status {
                exists: self.providers.iter().any(|provider| provider.exists()),
                unlocked: self.providers.iter().any(|provider| provider.unlocked()),
            },
            Request::Vaults => Response::Vaults { list: self.vaults(), default: self.default.clone() },
            Request::Create { password } => done(self.provider_mut(LOCAL).and_then(|local| local.create(&password))),
            Request::Unlock { vault, password } if vault == LOGIN => self.unlock_login(&password, false),
            Request::Unlock { vault, password } => {
                let response = done(self.provider_mut(&vault).and_then(|p| p.unlock(&password)));
                if vault == LOCAL && response == Response::Done {
                    self.migrate_app_secrets();
                }
                response
            }
            Request::LoginPassword { password } => self.unlock_login(&password, true),
            Request::Lock { vault: Some(vault) } => done(self.provider_mut(&vault).map(|provider| provider.lock())),
            Request::Lock { vault: None } => {
                for provider in &mut self.providers {
                    provider.lock();
                }
                Response::Done
            }
            Request::RequestUnlock { .. } | Request::PromptClosed => Response::Done,
            Request::ChangePassword { vault, old, new } => {
                done(self.provider_mut(&vault).and_then(|provider| provider.change_password(&old, &new)))
            }
            Request::Items => Response::Items { list: self.items() },
            Request::Item { item } => {
                self.detail(&item).map_or_else(failed, |detail| Response::Item { detail: Box::new(detail) })
            }
            Request::Secret { item, secret } => {
                self.secret(&item, &secret).map_or_else(failed, |secret| Response::Secret { secret })
            }
            Request::Totp { item } => self.code(&item).map_or_else(failed, |code| Response::Code { code }),
            Request::Save { draft } => self.save(&draft).map_or_else(failed, |item| Response::Saved { item }),
            Request::Trash { item } => {
                let at = (self.clock)();
                done(self.provider_mut(&item.vault).and_then(|provider| provider.trash(&item.id, at)))
            }
            Request::Restore { item } => {
                done(self.provider_mut(&item.vault).and_then(|provider| provider.restore(&item.id)))
            }
            Request::Purge { item } => {
                done(self.provider_mut(&item.vault).and_then(|provider| provider.purge(&item.id)))
            }
            Request::Folders => Response::Folders {
                list: self
                    .providers
                    .iter()
                    .filter_map(|provider| {
                        let vault = provider.vault().id;
                        provider.folders().ok().map(|folders| (vault, folders))
                    })
                    .flat_map(|(vault, folders)| {
                        folders.into_iter().map(move |name| Folder { vault: vault.clone(), id: name.clone(), name })
                    })
                    .collect(),
            },
            Request::Candidates { program, app_id } => {
                Response::Candidates { list: self.candidates(program.as_deref(), app_id.as_deref()) }
            }
            Request::Search { query, limit } => Response::Candidates { list: self.search(&query, limit) },
            Request::Report => Response::Report { report: self.report() },
            other => self.accounts(other),
        }
    }

    fn accounts(&mut self, request: Request) -> Response {
        match request {
            Request::AddBitwarden { login } => self.add_bitwarden(&login),
            Request::AddKeePass { path, password, name } => self.add_keepass(&path, &password, &name),
            Request::CheckLeaks => self.check_leaks(),
            Request::Import { vault, contents, password, binary } => {
                match crate::transfer::parse_file(contents.expose(), binary, password.as_ref().map(Hidden::expose)) {
                    Ok(drafts) => {
                        let total = drafts.len() as u32;
                        let mut count = 0;
                        for draft in drafts {
                            count += u32::from(self.import_one(&vault, draft));
                        }
                        Response::Imported { count, skipped: total - count }
                    }
                    Err(error) => import_failed(&error),
                }
            }
            Request::Export { vault, password } => {
                let records = self.export_records(vault.as_deref());
                export_response(&records, &password)
            }
            Request::SendLoginEmail { login } => match self.accounts.clone() {
                Some(dir) => Bitwarden::send_login_email(&dir, &login).map_or_else(login_failed, |()| Response::Done),
                None => Response::Unsupported,
            },
            Request::RenameVault { vault, name } => {
                let result = self.provider_mut(&vault).map(|provider| provider.rename(name.trim()));
                self.store_settings();
                done(result)
            }
            Request::SetDefault { vault } => match self.index(&vault) {
                Ok(_) => {
                    self.default = vault;
                    self.store_settings();
                    Response::Done
                }
                Err(fault) => failed(fault),
            },
            Request::Sync { vault } => {
                let mut result = Ok(());
                for provider in &mut self.providers {
                    if vault.as_ref().is_none_or(|vault| &provider.vault().id == vault) && provider.unlocked() {
                        result = result.and(provider.sync());
                    }
                }
                done(result)
            }
            Request::Conflicts => {
                Response::Conflicts { list: self.providers.iter().flat_map(|provider| provider.conflicts()).collect() }
            }
            Request::Resolve { item, keep } => done(self.resolve(&item, keep)),
            Request::Sends => {
                let mut list = Vec::new();
                for provider in &self.providers {
                    if let Ok(sends) = provider.sends() {
                        list.extend(sends);
                    }
                }
                Response::Sends { list }
            }
            Request::CreateSend { draft } => self
                .provider_mut(&draft.vault)
                .and_then(|provider| provider.create_send(&draft))
                .map_or_else(failed, |send| Response::SendCreated { send: Box::new(send) }),
            Request::SendText { vault, id } => self
                .provider_mut(&vault)
                .and_then(|provider| provider.send_text(&id))
                .map_or_else(failed, |secret| Response::Secret { secret }),
            Request::EditSend { id, draft, keep_password } => self
                .provider_mut(&draft.vault)
                .and_then(|provider| provider.edit_send(&id, &draft, keep_password))
                .map_or_else(failed, |send| Response::SendCreated { send: Box::new(send) }),
            Request::DisableSend { vault, id } => {
                done(self.provider_mut(&vault).and_then(|provider| provider.disable_send(&id)))
            }
            Request::DeleteSend { vault, id } => {
                done(self.provider_mut(&vault).and_then(|provider| provider.delete_send(&id)))
            }
            Request::RemoveVault { vault } => {
                if vault == LOCAL {
                    return Response::Unsupported;
                }
                match self.index(&vault) {
                    Ok(index) => {
                        let result = self.providers[index].forget();
                        self.providers.remove(index);
                        if self.default == vault {
                            self.default = LOCAL.into();
                        }
                        self.store_settings();
                        done(result)
                    }
                    Err(fault) => failed(fault),
                }
            }
            _ => Response::Unsupported,
        }
    }

    fn resolve(&mut self, item: &ItemRef, keep: Keep) -> Result<(), Fault> {
        self.provider_mut(&item.vault)?.resolve(&item.id, keep)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glide_ipc::keychain::{AppLink, CardDraft, Draft, FieldDraft, Site};

    const FAST: Params = Params { memory_kib: 64, passes: 1, lanes: 1 };
    const NOW: u64 = 2_000_000_000;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("glide-keychain-daemon-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("keychain")
    }

    fn hidden(text: &str) -> Hidden {
        Hidden::new(text.into())
    }

    fn daemon(name: &str) -> (Daemon, PathBuf) {
        let path = temp(name);
        let mut daemon = Daemon::new(path.clone(), FAST).with_clock(|| NOW);
        assert_eq!(daemon.handle(Request::Create { password: hidden("pw") }), Response::Done);
        (daemon, path)
    }

    #[test]
    fn program_secrets_kept_in_the_local_vault_move_to_the_login_keyring_when_it_opens() {
        let path = temp("migrate");
        let mut local = Local::new(path.clone(), FAST, LOCAL_NAME);
        local.create(&hidden("pw")).unwrap();
        local
            .edit_app_secrets(&mut |items, next| {
                *next += 1;
                let mut item = crate::vault::Item::new(crate::local::SERVICE_COLLECTION, "Chrome Safe Storage", 1);
                item.id = *next;
                item.attributes.insert("application".into(), "chrome".into());
                items.push(item);
            })
            .unwrap();
        drop(local);
        let mut daemon = Daemon::new(path.clone(), FAST);
        assert!(daemon.app_secrets_unlocked(), "program secrets need no password of their own");
        assert!(daemon.app_secrets().unwrap().is_empty());
        assert_eq!(daemon.handle(Request::Unlock { vault: LOCAL.into(), password: hidden("pw") }), Response::Done);
        let moved = daemon.app_secrets().unwrap();
        assert_eq!(moved.iter().map(|item| item.label.as_str()).collect::<Vec<_>>(), ["Chrome Safe Storage"]);
        assert!(daemon.provider(LOCAL).unwrap().app_secrets().unwrap().is_empty(), "the local vault lets them go");
        assert_eq!(daemon.handle(Request::Lock { vault: None }), Response::Done);
        assert!(daemon.app_secrets_unlocked(), "locking every vault leaves program secrets open");
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn without_the_login_module_the_lock_screen_never_turns_the_key_file_into_a_password_prompt() {
        let path = temp("no-module");
        let mut daemon = Daemon::new(path.clone(), FAST);
        daemon
            .edit_app_secrets(&mut |items, next| {
                *next += 1;
                items.push(crate::vault::Item { id: *next, ..crate::vault::Item::new("", "Chrome Safe Storage", 1) });
            })
            .unwrap();
        assert_eq!(daemon.handle(Request::LoginPassword { password: hidden("login") }), Response::Done);
        assert!(Daemon::new(path.clone(), FAST).app_secrets_unlocked(), "the next login opens it without a word");
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn the_key_left_by_the_login_opens_the_app_keys_and_a_password_change_follows() {
        let path = temp("handoff");
        let runtime = path.with_file_name("runtime");
        std::fs::create_dir_all(&runtime).unwrap();
        let (user, uid) = crate::login::current_user().unwrap();
        let key = |password: &str| glide_login_key::wrapping_key(password.as_bytes(), &user, uid).unwrap();
        let store = |daemon: &mut Daemon| {
            daemon
                .edit_app_secrets(&mut |items, next| {
                    *next += 1;
                    items.push(crate::vault::Item {
                        id: *next,
                        ..crate::vault::Item::new(crate::local::SERVICE_COLLECTION, "Chrome Safe Storage", 1)
                    });
                })
                .unwrap();
        };
        std::fs::write(runtime.join(glide_login_key::KEY_FILE), key("first").as_ref()).unwrap();
        let mut daemon = Daemon::new(path.clone(), FAST);
        daemon.watch_login_handoff(runtime.clone());
        store(&mut daemon);
        let mut restarted = Daemon::new(path.clone(), FAST);
        assert!(!restarted.app_secrets_unlocked(), "the keyring was wrapped by the login password");
        restarted.watch_login_handoff(runtime.clone());
        assert!(restarted.app_secrets_unlocked(), "a daemon restarted in the same session still finds the key");

        let mut next_login = Daemon::new(path.clone(), FAST);
        next_login.watch_login_handoff(runtime.clone());
        assert_eq!(next_login.app_secrets().unwrap().len(), 1);

        let rekey = glide_login_key::rekey_bytes(&key("first"), &key("second"));
        std::fs::write(runtime.join(glide_login_key::REKEY_FILE), &*rekey).unwrap();
        next_login.take_login_handoff();
        assert!(!runtime.join(glide_login_key::KEY_FILE).exists(), "the key the login left is stale after a change");
        let mut restarted_again = Daemon::new(path.clone(), FAST);
        restarted_again.watch_login_handoff(runtime);
        assert!(!restarted_again.app_secrets_unlocked(), "a restart in the same session never restores the old wrap");
        let mut after = Daemon::new(path.clone(), FAST);
        let first = Request::Unlock { vault: LOGIN.into(), password: hidden("first") };
        assert_eq!(after.handle(first), Response::WrongPassword);
        assert_eq!(after.handle(Request::Unlock { vault: LOGIN.into(), password: hidden("second") }), Response::Done);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    fn login(name: &str, password: &str, app: &str) -> Draft {
        Draft {
            vault: LOCAL.into(),
            name: name.into(),
            username: "me".into(),
            password: Some(hidden(password)),
            apps: if app.is_empty() {
                Vec::new()
            } else {
                vec![AppLink { name: String::new(), app_id: app.into(), program: String::new() }]
            },
            sites: vec![Site { uri: format!("{}.example", name.to_lowercase()), ..Site::default() }],
            ..Draft::default()
        }
    }

    fn save(daemon: &mut Daemon, draft: Draft) -> ItemRef {
        match daemon.handle(Request::Save { draft: Box::new(draft) }) {
            Response::Saved { item } => item,
            other => panic!("{other:?}"),
        }
    }

    fn cleanup(path: &std::path::Path) {
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn a_vault_is_created_locked_and_unlocked_again() {
        let (mut daemon, path) = daemon("cycle");
        assert_eq!(daemon.handle(Request::Status), Response::Status { exists: true, unlocked: true });
        assert_eq!(daemon.handle(Request::Lock { vault: None }), Response::Done);
        assert_eq!(daemon.handle(Request::Items), Response::Items { list: Vec::new() });
        assert_eq!(
            daemon.handle(Request::Unlock { vault: LOCAL.into(), password: hidden("nope") }),
            Response::WrongPassword
        );
        assert_eq!(daemon.handle(Request::Unlock { vault: LOCAL.into(), password: hidden("pw") }), Response::Done);
        cleanup(&path);
    }

    #[test]
    fn candidates_match_the_app_newest_first_and_skip_the_trash() {
        let (mut daemon, path) = daemon("candidates");
        let old = save(&mut daemon, login("Old", "a", "org.Telegram.desktop"));
        save(&mut daemon, login("Other", "b", "firefox"));
        let Response::Candidates { list } =
            daemon.handle(Request::Candidates { program: None, app_id: Some("org.telegram.desktop".into()) })
        else {
            panic!()
        };
        assert_eq!(list.iter().map(|c| c.label.as_str()).collect::<Vec<_>>(), ["Old"]);
        assert_eq!(list[0].vault_name, LOCAL_NAME);
        daemon.handle(Request::Trash { item: old });
        assert_eq!(
            daemon.handle(Request::Candidates { program: None, app_id: Some("org.telegram.desktop".into()) }),
            Response::Candidates { list: Vec::new() }
        );
        cleanup(&path);
    }

    #[test]
    fn summaries_flag_weak_and_reused_passwords() {
        let (mut daemon, path) = daemon("report");
        save(&mut daemon, login("Router", "admin123", ""));
        let a = save(&mut daemon, login("Hetzner", "Hetzner2024", ""));
        let b = save(&mut daemon, login("GitLab", "Hetzner2024", ""));
        save(&mut daemon, login("GitHub", "c7#Vq2!mWx9tLp4Rz8", ""));
        let Response::Items { list } = daemon.handle(Request::Items) else { panic!() };
        let by_name = |name: &str| list.iter().find(|item| item.name == name).unwrap();
        assert_eq!((by_name("Router").strength, by_name("GitHub").strength), (1, 4));
        assert_eq!(by_name("Hetzner").reused, 1);
        let Response::Report { report } = daemon.handle(Request::Report) else { panic!() };
        assert_eq!(report.weak.len(), 1);
        let mut group = vec![a, b];
        group.sort();
        assert_eq!(report.reused, [group]);
        assert_eq!(report.checked, 4);
        assert!(report.old.is_empty());
        cleanup(&path);
    }

    #[test]
    fn a_totp_code_comes_from_the_stored_key() {
        let (mut daemon, path) = daemon("totp");
        let mut draft = login("GitHub", "x", "");
        draft.totp = Some(hidden("GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ"));
        let item = save(&mut daemon, draft);
        let Response::Code { code } = daemon.handle(Request::Totp { item: item.clone() }) else { panic!() };
        assert_eq!(code.code, "279037");
        assert_eq!((code.seconds_left, code.period), (10, 30));
        let Response::Item { detail } = daemon.handle(Request::Item { item }) else { panic!() };
        assert!(detail.summary.has_totp);
        cleanup(&path);
    }

    #[test]
    fn secrets_are_only_given_on_request() {
        let (mut daemon, path) = daemon("secrets");
        let item = save(&mut daemon, login("GitHub", "hunter2", ""));
        let Response::Item { detail } = daemon.handle(Request::Item { item: item.clone() }) else { panic!() };
        assert!(!format!("{detail:?}").contains("hunter2"));
        assert_eq!(detail.password_length, 7);
        assert_eq!(
            daemon.handle(Request::Secret { item, secret: SecretKind::Password }),
            Response::Secret { secret: hidden("hunter2") }
        );
        cleanup(&path);
    }

    #[test]
    fn search_ranks_prefixes_first() {
        let (mut daemon, path) = daemon("search");
        save(&mut daemon, login("GitHub", "a", ""));
        save(&mut daemon, login("MyGit", "b", ""));
        save(&mut daemon, login("Steam", "c", ""));
        let Response::Candidates { list } = daemon.handle(Request::Search { query: "git".into(), limit: 5 }) else {
            panic!()
        };
        assert_eq!(list.iter().map(|c| c.label.as_str()).collect::<Vec<_>>(), ["GitHub", "MyGit"]);
        cleanup(&path);
    }

    #[test]
    fn a_leaked_password_shows_in_the_report_and_is_not_asked_again_for_a_day() {
        let (mut daemon, path) = daemon("leaks");
        let item = save(&mut daemon, login("Router", "password", ""));
        save(&mut daemon, login("GitHub", "c7#Vq2!mWx9tLp4Rz8", ""));
        let hashes = daemon.leak_hashes();
        assert_eq!(hashes.len(), 2);
        let found = crate::leaks::check(&hashes, |_| Ok("1E4C9B93F3F0682250B6CF8331B7EE68FD8:12\n".into())).unwrap();
        daemon.store_leaks(found);
        let Response::Report { report } = daemon.handle(Request::Report) else { panic!() };
        assert_eq!(report.leaked, [item]);
        assert!(daemon.leak_hashes().is_empty(), "checked passwords wait a day");
        cleanup(&path);
    }

    #[test]
    fn a_keepass_database_joins_as_a_vault_and_is_remembered() {
        let (mut daemon, path) = daemon("keepass");
        let file = path.with_file_name("Family.kdbx");
        crate::kdbx::vault_tests_sample(&file, "door");
        let add = |password: &str| Request::AddKeePass {
            path: file.display().to_string(),
            password: hidden(password),
            name: String::new(),
        };
        assert_eq!(daemon.handle(add("window")), Response::WrongPassword);
        assert_eq!(daemon.handle(add("door")), Response::VaultAdded { vault: "keepass-1".into() });
        assert_eq!(daemon.handle(add("door")), Response::VaultAdded { vault: "keepass-1".into() }, "added once");
        let Response::Items { list } = daemon.handle(Request::Items) else { panic!() };
        assert!(list.iter().any(|item| item.item.vault == "keepass-1" && item.name == "Jira"));

        let mut again = Daemon::new(path.clone(), FAST).with_clock(|| NOW);
        let Response::Vaults { list, .. } = again.handle(Request::Vaults) else { panic!() };
        let keepass = list.iter().find(|vault| vault.id == "keepass-1").expect("remembered");
        assert_eq!((keepass.name.as_str(), keepass.unlocked), ("Family", false));
        assert_eq!(again.handle(Request::RemoveVault { vault: "keepass-1".into() }), Response::Done);
        assert!(file.exists(), "removing the vault leaves the file alone");
        let again = Daemon::new(path.clone(), FAST).with_clock(|| NOW);
        assert!(again.provider("keepass-1").is_err());
        cleanup(&path);
    }

    fn card_with_a_hidden_field() -> Draft {
        Draft {
            vault: LOCAL.into(),
            kind: ItemKind::Card,
            name: "Visa".into(),
            fields: vec![FieldDraft { name: "PIN".into(), value: hidden("4321"), hidden: true }],
            card: Some(CardDraft {
                holder: "Artem".into(),
                number: hidden("4111111111111111"),
                code: hidden("123"),
                ..CardDraft::default()
            }),
            ..Draft::default()
        }
    }

    fn as_the_editor_sends_it(daemon: &mut Daemon, item: &ItemRef) -> Draft {
        let Response::Item { detail } = daemon.handle(Request::Item { item: item.clone() }) else { panic!() };
        Draft {
            item: Some(item.clone()),
            vault: item.vault.clone(),
            kind: detail.summary.kind,
            name: detail.summary.name.clone(),
            fields: detail
                .fields
                .iter()
                .map(|field| FieldDraft { name: field.name.clone(), value: hidden(&field.value), hidden: field.hidden })
                .collect(),
            notes: "edited".into(),
            card: detail.card.as_ref().map(|card| CardDraft { holder: card.holder.clone(), ..CardDraft::default() }),
            ..Draft::default()
        }
    }

    fn secret_of(daemon: &mut Daemon, item: &ItemRef, secret: SecretKind) -> String {
        let Response::Secret { secret } = daemon.handle(Request::Secret { item: item.clone(), secret }) else {
            panic!()
        };
        secret.expose().to_string()
    }

    #[test]
    fn an_edit_that_never_saw_the_hidden_values_keeps_them() {
        let (mut daemon, path) = daemon("hidden-edit");
        let Response::Saved { item } = daemon.handle(Request::Save { draft: Box::new(card_with_a_hidden_field()) })
        else {
            panic!()
        };
        let edit = as_the_editor_sends_it(&mut daemon, &item);
        assert!(edit.fields[0].value.is_empty(), "the detail never carries a hidden value");
        assert!(matches!(daemon.handle(Request::Save { draft: Box::new(edit) }), Response::Saved { .. }));
        assert_eq!(secret_of(&mut daemon, &item, SecretKind::Field("PIN".into())), "4321");
        assert_eq!(secret_of(&mut daemon, &item, SecretKind::CardNumber), "4111111111111111");
        cleanup(&path);
    }

    #[test]
    fn hidden_fields_with_the_same_name_are_read_by_position() {
        let (mut daemon, path) = daemon("same-name-fields");
        let draft = Draft {
            vault: LOCAL.into(),
            kind: ItemKind::Login,
            name: "Bank".into(),
            fields: vec![
                FieldDraft { name: "Security answer".into(), value: hidden("Kyiv"), hidden: true },
                FieldDraft { name: "Security answer".into(), value: hidden("Rex"), hidden: true },
            ],
            ..Draft::default()
        };
        let Response::Saved { item } = daemon.handle(Request::Save { draft: Box::new(draft) }) else { panic!() };
        assert_eq!(secret_of(&mut daemon, &item, SecretKind::FieldAt(0)), "Kyiv");
        assert_eq!(secret_of(&mut daemon, &item, SecretKind::FieldAt(1)), "Rex");
        assert_eq!(secret_of(&mut daemon, &item, SecretKind::Field("Security answer".into())), "Kyiv");
        assert!(matches!(
            daemon.handle(Request::Secret { item: item.clone(), secret: SecretKind::FieldAt(2) }),
            Response::Error { .. }
        ));
        cleanup(&path);
    }

    #[test]
    fn moving_to_another_vault_carries_every_secret_and_keeps_the_original_in_the_trash() {
        let (mut daemon, path) = daemon("move");
        let file = path.with_file_name("Family.kdbx");
        crate::kdbx::vault_tests_sample(&file, "door");
        let add =
            Request::AddKeePass { path: file.display().to_string(), password: hidden("door"), name: String::new() };
        assert_eq!(daemon.handle(add), Response::VaultAdded { vault: "keepass-1".into() });
        let Response::Saved { item } = daemon.handle(Request::Save { draft: Box::new(card_with_a_hidden_field()) })
        else {
            panic!()
        };
        let mut edit = as_the_editor_sends_it(&mut daemon, &item);
        edit.vault = "keepass-1".into();
        let Response::Saved { item: moved } = daemon.handle(Request::Save { draft: Box::new(edit) }) else { panic!() };
        assert_eq!(moved.vault, "keepass-1");
        assert_eq!(secret_of(&mut daemon, &moved, SecretKind::Field("PIN".into())), "4321");
        assert_eq!(secret_of(&mut daemon, &moved, SecretKind::CardNumber), "4111111111111111");
        assert_eq!(secret_of(&mut daemon, &moved, SecretKind::CardCode), "123");
        assert!(daemon.record(&item).is_ok_and(|record| record.deleted.is_some()), "the original waits in the Trash");
        cleanup(&path);
    }

    #[test]
    fn an_import_fills_the_chosen_vault_and_an_export_carries_it_back() {
        let (mut daemon, path) = daemon("transfer");
        let csv = "name,url,username,password\nGitHub,https://github.com,artem,hunter2\n,,,\nRouter,,admin,admin123\n";
        let contents = hidden(csv);
        assert_eq!(
            daemon.handle(Request::Import { vault: LOCAL.into(), contents, password: None, binary: false }),
            Response::Imported { count: 2, skipped: 0 }
        );
        let Response::Exported { data, count } =
            daemon.handle(Request::Export { vault: None, password: hidden("file pw") })
        else {
            panic!()
        };
        assert_eq!(count, 2);
        assert!(!data.contains("hunter2"));
        assert_eq!(
            daemon.handle(Request::Import {
                vault: LOCAL.into(),
                contents: hidden(&data),
                password: None,
                binary: false
            }),
            Response::NeedsFilePassword
        );
        assert_eq!(
            daemon.handle(Request::Import {
                vault: LOCAL.into(),
                contents: hidden(&data),
                password: Some(hidden("file pw")),
                binary: false
            }),
            Response::Imported { count: 2, skipped: 0 }
        );
        let Response::Items { list } = daemon.handle(Request::Items) else { panic!() };
        assert_eq!(list.len(), 4);
        cleanup(&path);
    }

    #[test]
    fn mutating_requests_are_known() {
        assert!(mutates(&Request::Lock { vault: None }));
        assert!(!mutates(&Request::Items));
    }
}

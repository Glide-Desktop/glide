pub mod api;
pub mod cipher;
pub mod crypto;
pub mod dates;
pub mod session;
pub mod sync;

#[cfg(test)]
mod fake;
#[cfg(test)]
mod live;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use glide_ipc::keychain::{
    Conflict, Draft, Hidden, ItemRef, Keep, Login, Send, SendDraft, SyncState, TwoFactorMethod, Vault, VaultKind,
};
use serde_json::{Value, json};
use zeroize::Zeroizing;

use self::api::{ApiError, Client, Endpoints, Tokens, field, list, number, text};
use self::cipher::Keys;
use self::crypto::{CryptoError, Kdf, PrivateKey, SymmetricKey};
use self::session::{Session, device_form};
use self::sync::{NEW_PREFIX, is_new, normalize};
pub use self::sync::{Pending, StoredClash, SyncDone, SyncJob};
use crate::provider::{Fault, Provider, Record};

const SYNC_EVERY: u64 = 300;
const RETRY_AFTER: u64 = 120;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Account {
    pub id: String,
    pub name: String,
    pub server: String,
    pub email: String,
    pub kdf: Kdf,
    pub device: String,
    pub protected_key: String,
    #[serde(default)]
    pub refresh: Option<String>,
    #[serde(default)]
    pub remember: Option<String>,
    #[serde(default)]
    pub api_client: Option<String>,
    #[serde(default)]
    pub api_secret: Option<String>,
    #[serde(default)]
    pub cache: Option<Value>,
    #[serde(default)]
    pub synced_at: Option<u64>,
    #[serde(default)]
    pub queue: Vec<Pending>,
    #[serde(default)]
    pub clashes: Vec<StoredClash>,
}

struct Open {
    session: Session,
    keys: Keys,
    records: Vec<Record>,
    raw: HashMap<String, Value>,
    folders: HashMap<String, String>,
}

pub struct Bitwarden {
    path: PathBuf,
    account: Account,
    endpoints: Endpoints,
    open: Option<Open>,
    offline: bool,
    failure: Option<String>,
    syncing: bool,
    wanted: bool,
    attempted: u64,
}

impl std::fmt::Debug for Bitwarden {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Bitwarden")
            .field("id", &self.account.id)
            .field("server", &self.account.server)
            .field("unlocked", &self.open.is_some())
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoginError {
    TwoFactor(Vec<TwoFactorMethod>),
    DeviceCode,
    WrongPassword,
    Failed(String),
}

impl From<CryptoError> for Fault {
    fn from(error: CryptoError) -> Self {
        Self::Failed(error.to_string())
    }
}

fn method_of(code: u8) -> Option<TwoFactorMethod> {
    match code {
        0 => Some(TwoFactorMethod::Authenticator),
        1 => Some(TwoFactorMethod::Email),
        2 | 6 => Some(TwoFactorMethod::Duo),
        3 => Some(TwoFactorMethod::Yubikey),
        7 => Some(TwoFactorMethod::WebAuthn),
        _ => None,
    }
}

const fn code_of(method: TwoFactorMethod) -> u8 {
    match method {
        TwoFactorMethod::Authenticator => 0,
        TwoFactorMethod::Email => 1,
        TwoFactorMethod::Duo => 2,
        TwoFactorMethod::Yubikey => 3,
        TwoFactorMethod::WebAuthn => 7,
    }
}

fn host_of(server: &str) -> String {
    let rest = server.split_once("://").map_or(server, |(_, rest)| rest);
    rest.trim_end_matches('/').to_string()
}

fn login_error(error: ApiError) -> LoginError {
    match error {
        ApiError::TwoFactor(codes) => {
            let mut methods: Vec<TwoFactorMethod> = codes.into_iter().filter_map(method_of).collect();
            methods.dedup();
            LoginError::TwoFactor(methods)
        }
        ApiError::DeviceVerification => LoginError::DeviceCode,
        ApiError::WrongPassword => LoginError::WrongPassword,
        other => LoginError::Failed(other.to_string()),
    }
}

fn decode(cache: &Value, user: &SymmetricKey) -> (Keys, HashMap<String, String>, Vec<Record>, HashMap<String, Value>) {
    let profile = field(cache, "profile").cloned().unwrap_or(Value::Null);
    let mut orgs = HashMap::new();
    if let Some(private) = text(&profile, "privateKey").and_then(|value| PrivateKey::from_encrypted(user, value).ok()) {
        for org in list(&profile, "organizations") {
            if let (Some(id), Some(key)) = (text(org, "id"), text(org, "key"))
                && let Ok(key) = private.decrypt_key(key)
            {
                orgs.insert(id.to_string(), key);
            }
        }
    }
    let keys = Keys { user: user.clone(), orgs, owner: text(&profile, "id").map(str::to_string) };
    let mut folders = HashMap::new();
    for folder in list(cache, "folders") {
        if let (Some(id), Some(name)) = (text(folder, "id"), text(folder, "name"))
            && let Ok(name) = user.decrypt_text(name)
        {
            folders.insert(id.to_string(), name);
        }
    }
    let mut records = Vec::new();
    let mut raw = HashMap::new();
    for item in list(cache, "ciphers") {
        match cipher::record(item, &keys, &folders) {
            Ok(record) => {
                raw.insert(record.id.clone(), item.clone());
                records.push(record);
            }
            Err(err) => eprintln!("glide-keychain: skipped a Bitwarden item that does not decrypt ({err})"),
        }
    }
    (keys, folders, records, raw)
}

fn with_id(body: &Value, id: &str) -> Value {
    let mut body = body.clone();
    if let Some(object) = body.as_object_mut() {
        object.insert("id".into(), json!(id));
    }
    body
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.as_secs())
}

impl Bitwarden {
    #[must_use]
    pub fn directory() -> Option<PathBuf> {
        glide_config::data_home().map(|dir| dir.join("glide").join("bitwarden"))
    }

    #[must_use]
    pub fn load_all(dir: &Path) -> Vec<Self> {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Vec::new();
        };
        let mut accounts: Vec<Self> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
            .filter_map(|path| {
                let account: Account = serde_json::from_slice(&std::fs::read(&path).ok()?).ok()?;
                Some(Self::from_account(path, account))
            })
            .collect();
        accounts.sort_by(|a, b| a.account.id.cmp(&b.account.id));
        accounts
    }

    fn from_account(path: PathBuf, account: Account) -> Self {
        let endpoints = Endpoints::for_server(&account.server);
        Self {
            path,
            account,
            endpoints,
            open: None,
            offline: false,
            failure: None,
            syncing: false,
            wanted: false,
            attempted: 0,
        }
    }

    #[must_use]
    pub const fn account(&self) -> &Account {
        &self.account
    }

    fn device_id(dir: &Path) -> Result<String, LoginError> {
        let path = dir.join("device-id");
        if let Ok(known) = std::fs::read_to_string(&path)
            && known.trim().len() == 36
        {
            return Ok(known.trim().to_string());
        }
        let failed = |err: &dyn std::fmt::Display| LoginError::Failed(err.to_string());
        let device = crypto::uuid_v4().map_err(|err| failed(&err))?;
        std::fs::create_dir_all(dir).map_err(|err| failed(&err))?;
        crate::vault::write_atomically(&path, device.as_bytes()).map_err(|err| failed(&err))?;
        Ok(device)
    }

    fn master(client: &Client, login: &Login) -> Result<(String, Kdf, Zeroizing<[u8; 32]>), LoginError> {
        let email = crypto::normalize_email(&login.email);
        let prelogin = client.prelogin(&email).map_err(login_error)?;
        let kdf = Kdf::from_prelogin(prelogin.kdf, prelogin.iterations, prelogin.memory, prelogin.parallelism)
            .map_err(|err| LoginError::Failed(err.to_string()))?;
        let master = crypto::master_key(login.password.expose().as_bytes(), &email, kdf)
            .map_err(|err| LoginError::Failed(err.to_string()))?;
        Ok((email, kdf, master))
    }

    pub fn send_login_email(dir: &Path, login: &Login) -> Result<(), LoginError> {
        let client = Client::new(Endpoints::for_server(&login.server));
        let (email, _, master) = Self::master(&client, login)?;
        let hash = Zeroizing::new(crypto::master_password_hash(&master, login.password.expose().as_bytes()));
        let body = json!({ "email": email, "masterPasswordHash": *hash, "deviceIdentifier": Self::device_id(dir)? });
        client.post_public("/two-factor/send-email-login", &body).map(drop).map_err(login_error)
    }

    pub fn login(dir: &Path, login: &Login, now: u64) -> Result<Self, LoginError> {
        let client = Client::new(Endpoints::for_server(&login.server));
        let (email, kdf, master) = Self::master(&client, login)?;
        let hash = Zeroizing::new(crypto::master_password_hash(&master, login.password.expose().as_bytes()));
        let fresh_id = || crypto::uuid_v4().map_err(|err| LoginError::Failed(err.to_string()));
        let device = Self::device_id(dir)?;
        let body = Self::grant(&client, &email, &hash, &device, login).map_err(login_error)?;
        let protected =
            text(&body, "Key").ok_or_else(|| LoginError::Failed("the server sent no account key".into()))?;
        let user = SymmetricKey::stretch(&master).decrypt_key(protected).map_err(|_| LoginError::WrongPassword)?;
        let tokens = Tokens::from_response(&body, now).ok_or_else(|| LoginError::Failed("no access token".into()))?;
        let seal = |value: &str| user.encrypt_text(value).ok();
        let id = format!("bw-{}", &fresh_id()?[..8]);
        let account = Account {
            id: id.clone(),
            name: if login.name.trim().is_empty() { "Bitwarden".into() } else { login.name.trim().to_string() },
            server: login.server.trim().trim_end_matches('/').to_string(),
            email,
            kdf,
            device,
            protected_key: protected.to_string(),
            refresh: Some(tokens.refresh.as_str()).filter(|token| !token.is_empty()).and_then(seal),
            remember: text(&body, "TwoFactorToken").and_then(seal),
            api_client: login.api_key.as_ref().map(|key| key.client_id.clone()),
            api_secret: login.api_key.as_ref().and_then(|key| seal(key.client_secret.expose())),
            cache: None,
            synced_at: None,
            queue: Vec::new(),
            clashes: Vec::new(),
        };
        let mut provider = Self::from_account(dir.join(format!("{id}.json")), account);
        provider.open_with(user, hash, Some(tokens));
        let job =
            provider.prepare_sync(true, now).ok_or_else(|| LoginError::Failed("the vault did not open".into()))?;
        provider.apply_sync(job.run(now), now);
        if provider.account.cache.is_none() {
            return Err(LoginError::Failed(provider.failure.take().unwrap_or_else(|| "the vault did not sync".into())));
        }
        Ok(provider)
    }

    fn grant(client: &Client, email: &str, hash: &str, device: &str, login: &Login) -> Result<Value, ApiError> {
        let mut form = device_form(device);
        let provider;
        if let Some(key) = &login.api_key {
            form.extend([
                ("grant_type", "client_credentials"),
                ("scope", "api"),
                ("client_id", key.client_id.as_str()),
                ("client_secret", key.client_secret.expose()),
            ]);
            return client.token(&form, None);
        }
        form.extend([
            ("grant_type", "password"),
            ("scope", "api offline_access"),
            ("client_id", api::CLIENT_ID),
            ("username", email),
            ("password", hash),
        ]);
        if let Some(two) = &login.two_factor {
            provider = code_of(two.method).to_string();
            form.extend([
                ("twoFactorToken", two.code.as_str()),
                ("twoFactorProvider", provider.as_str()),
                ("twoFactorRemember", if two.remember { "1" } else { "0" }),
            ]);
        }
        if let Some(code) = &login.device_code {
            form.push(("newDeviceOtp", code.as_str()));
        }
        client.token(&form, Some(email))
    }

    fn open_with(&mut self, user: SymmetricKey, hash: Zeroizing<String>, tokens: Option<Tokens>) {
        let session = Session::new(&self.account, user.clone(), hash, tokens);
        self.open = Some(Open {
            session,
            keys: Keys { user, orgs: HashMap::new(), owner: None },
            records: Vec::new(),
            raw: HashMap::new(),
            folders: HashMap::new(),
        });
    }

    fn save_account(&self) -> Result<(), Fault> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|err| Fault::Failed(err.to_string()))?;
        }
        let bytes = serde_json::to_vec_pretty(&self.account).map_err(|err| Fault::Failed(err.to_string()))?;
        crate::vault::write_atomically(&self.path, &bytes).map_err(|err| Fault::Failed(err.to_string()))
    }

    fn opened(&self) -> Result<&Open, Fault> {
        self.open.as_ref().ok_or(Fault::Locked)
    }

    fn opened_mut(&mut self) -> Result<&mut Open, Fault> {
        self.open.as_mut().ok_or(Fault::Locked)
    }

    fn with_session<T>(&mut self, work: impl FnOnce(&mut Session) -> Result<T, ApiError>) -> Result<T, ApiError> {
        let open = self.open.as_mut().ok_or(ApiError::Unauthorized)?;
        let result = work(&mut open.session);
        let refresh = open.session.sealed_refresh();
        if refresh != self.account.refresh {
            self.account.refresh = refresh;
            let _ = self.save_account();
        }
        result
    }

    fn call<T>(&mut self, now: u64, action: impl Fn(&Client, &str) -> Result<T, ApiError>) -> Result<T, ApiError> {
        self.with_session(|session| session.call(now, action))
    }

    fn note(&mut self, error: &ApiError) {
        match error {
            ApiError::Offline(_) => self.offline = true,
            other => self.failure = Some(other.to_string()),
        }
    }

    fn adopt(&mut self, cache: &Value) -> Result<(), Fault> {
        let open = self.opened_mut()?;
        let (keys, folders, records, raw) = decode(cache, &open.keys.user);
        open.keys = keys;
        open.folders = folders;
        open.records = records;
        open.raw = raw;
        Ok(())
    }

    fn apply_locally(&mut self, pending: &Pending, at: u64) {
        let Some(open) = self.open.as_mut() else {
            return;
        };
        match pending {
            Pending::Save { id, body, .. } => {
                let body = with_id(body, id);
                if let Ok(record) = cipher::record(&body, &open.keys, &open.folders) {
                    open.records.retain(|known| known.id != *id);
                    open.raw.insert(id.clone(), body);
                    open.records.push(record);
                }
            }
            Pending::Trash { id } => {
                if let Some(record) = open.records.iter_mut().find(|record| record.id == *id) {
                    record.deleted = Some(at.max(1));
                }
            }
            Pending::Restore { id } => {
                if let Some(record) = open.records.iter_mut().find(|record| record.id == *id) {
                    record.deleted = None;
                }
            }
            Pending::Purge { id } => {
                open.records.retain(|record| record.id != *id);
                open.raw.remove(id);
            }
        }
    }

    fn apply_queue_locally(&mut self) {
        let at = now();
        for pending in self.account.queue.clone() {
            self.apply_locally(&pending, at);
        }
    }

    fn enqueue(&mut self, pending: Pending) -> Result<(), Fault> {
        self.account.queue.push(pending);
        if !self.syncing {
            self.account.queue = normalize(std::mem::take(&mut self.account.queue));
        }
        self.wanted = true;
        self.save_account()
    }

    fn cache_put(&mut self, kind: &str, item: &Value) {
        let Some(cache) = self.account.cache.as_mut().and_then(Value::as_object_mut) else {
            return;
        };
        let entries = cache.entry(kind.to_string()).or_insert_with(|| Value::Array(Vec::new()));
        if let Some(entries) = entries.as_array_mut() {
            let id = text(item, "id");
            entries.retain(|known| text(known, "id") != id);
            entries.push(item.clone());
        }
    }

    fn cache_drop(&mut self, kind: &str, id: &str) {
        if let Some(entries) = self
            .account
            .cache
            .as_mut()
            .and_then(Value::as_object_mut)
            .and_then(|cache| cache.get_mut(kind))
            .and_then(Value::as_array_mut)
        {
            entries.retain(|known| text(known, "id") != Some(id));
        }
    }

    fn adopt_cipher(&mut self, item: &Value) -> Result<String, Fault> {
        let open = self.opened_mut()?;
        let record = cipher::record(item, &open.keys, &open.folders)?;
        let id = record.id.clone();
        open.records.retain(|known| known.id != id);
        open.raw.insert(id.clone(), item.clone());
        open.records.push(record);
        self.cache_put("ciphers", item);
        self.save_account()?;
        Ok(id)
    }

    fn known_folder(&self, name: &str) -> Option<String> {
        let name = name.trim();
        let open = self.open.as_ref()?;
        open.folders.iter().find(|(_, known)| known.as_str() == name).map(|(id, _)| id.clone())
    }

    fn folder_id(&mut self, name: &str, now: u64) -> Option<String> {
        let name = name.trim();
        if name.is_empty() {
            return None;
        }
        if let Some(id) = self.known_folder(name) {
            return Some(id);
        }
        let sealed = self.open.as_ref()?.keys.user.encrypt_text(name).ok()?;
        let created =
            self.call(now, |client, token| client.post("/folders", token, &json!({ "name": sealed }))).ok()?;
        let id = text(&created, "id")?.to_string();
        if let Some(open) = self.open.as_mut() {
            open.folders.insert(id.clone(), name.to_string());
        }
        self.cache_put("folders", &created);
        Some(id)
    }

    fn change(&mut self, pending: Pending, at: u64) -> Result<(), Fault> {
        if self.syncing || is_new(pending.id()) {
            self.apply_locally(&pending, at);
            return self.enqueue(pending);
        }
        match self.with_session(|session| sync::push(session, &pending, at)) {
            Ok(_) => {
                self.apply_locally(&pending, at);
                if let Pending::Purge { id } = &pending {
                    self.cache_drop("ciphers", id);
                }
                self.save_account()
            }
            Err(ApiError::Offline(_)) => {
                self.offline = true;
                self.apply_locally(&pending, at);
                self.enqueue(pending)
            }
            Err(error) => {
                self.note(&error);
                Err(Fault::Failed(error.to_string()))
            }
        }
    }

    fn send_of(&self, raw: &Value) -> Option<Send> {
        let open = self.open.as_ref()?;
        let secret = open.keys.user.decrypt(text(raw, "key")?).ok()?;
        let key = SymmetricKey::send_key(&secret);
        let name = key.decrypt_text(text(raw, "name")?).ok()?;
        let access = text(raw, "accessId")?;
        Some(Send {
            vault: self.account.id.clone(),
            id: text(raw, "id")?.to_string(),
            name,
            link: self.endpoints.send_link(access, &secret),
            created: text(raw, "revisionDate").and_then(dates::parse).unwrap_or(0),
            expires: text(raw, "expirationDate").and_then(dates::parse),
            deletion: text(raw, "deletionDate").and_then(dates::parse).unwrap_or(0),
            max_access: number(raw, "maxAccessCount").map(|count| count as u32),
            access_count: number(raw, "accessCount").unwrap_or(0) as u32,
            disabled: field(raw, "disabled").and_then(Value::as_bool).unwrap_or(false),
            has_password: text(raw, "password").is_some_and(|hash| !hash.is_empty()),
            hide_text: field(raw, "text")
                .and_then(|text| field(text, "hidden"))
                .and_then(Value::as_bool)
                .unwrap_or(false),
            hide_email: field(raw, "hideEmail").and_then(Value::as_bool).unwrap_or(false),
        })
    }

    fn raw_sends(&self) -> Vec<Value> {
        self.account.cache.as_ref().map(|cache| list(cache, "sends").to_vec()).unwrap_or_default()
    }

    #[must_use]
    pub fn prepare_sync(&mut self, force: bool, now: u64) -> Option<SyncJob> {
        let open = self.open.as_ref()?;
        if self.syncing {
            return None;
        }
        let rested = now.saturating_sub(self.attempted) >= RETRY_AFTER;
        let stale = self.account.synced_at.is_none_or(|at| now.saturating_sub(at) >= SYNC_EVERY);
        if !(force || self.wanted || (rested && stale)) {
            return None;
        }
        let job = SyncJob { session: open.session.clone(), queue: self.account.queue.clone() };
        self.syncing = true;
        self.wanted = false;
        self.attempted = now;
        Some(job)
    }

    pub fn apply_sync(&mut self, done: SyncDone, now: u64) {
        self.syncing = false;
        let SyncDone { session, consumed, flushed, fresh } = done;
        self.account.refresh = session.sealed_refresh();
        if let Some(open) = self.open.as_mut() {
            open.session.adopt_tokens(&session);
        }
        match fresh {
            Err(ApiError::Offline(reason)) => {
                eprintln!("glide-keychain: {} stays on its cached copy ({reason})", self.account.id);
                self.offline = true;
            }
            Err(error) => {
                eprintln!("glide-keychain: {} did not sync ({error})", self.account.id);
                self.failure = Some(error.to_string());
            }
            Ok(fresh) => {
                let later = self.account.queue.split_off(consumed.min(self.account.queue.len()));
                let mut queue = flushed.kept;
                queue.extend(later.into_iter().map(|pending| pending.renamed(&flushed.renamed)));
                self.account.queue = normalize(queue);
                for clash in flushed.clashes {
                    self.account.clashes.retain(|known| known.id != clash.id);
                    self.account.clashes.push(clash);
                }
                self.offline = flushed.offline;
                self.failure = flushed.failure;
                self.account.synced_at = Some(now);
                if self.open.is_some() {
                    if let Err(fault) = self.adopt(&fresh) {
                        eprintln!("glide-keychain: {} could not read its sync ({fault})", self.account.id);
                    }
                    self.apply_queue_locally();
                }
                self.account.cache = Some(fresh);
                if !self.account.queue.is_empty() && !self.offline {
                    self.wanted = true;
                }
            }
        }
        if let Err(fault) = self.save_account() {
            eprintln!("glide-keychain: {} could not be saved ({fault})", self.account.id);
        }
    }

    fn clash_records(&self, clash: &StoredClash) -> Option<(Record, Record)> {
        let open = self.open.as_ref()?;
        let local = cipher::record(&with_id(&clash.body, &clash.id), &open.keys, &open.folders).ok()?;
        let remote = cipher::record(&clash.server, &open.keys, &open.folders).ok()?;
        Some((local, remote))
    }
}

impl Provider for Bitwarden {
    fn vault(&self) -> Vault {
        let sync = if self.syncing {
            SyncState::Syncing
        } else if let Some(message) = &self.failure {
            SyncState::Failed { message: message.clone() }
        } else if self.offline || !self.account.queue.is_empty() {
            SyncState::Offline { pending: self.account.queue.len() as u32 }
        } else if let Some(at) = self.account.synced_at {
            SyncState::Synced { at }
        } else {
            SyncState::Syncing
        };
        Vault {
            id: self.account.id.clone(),
            name: self.account.name.clone(),
            kind: VaultKind::Bitwarden,
            account: self.account.email.clone(),
            server: host_of(&self.account.server),
            exists: true,
            unlocked: self.open.is_some(),
            sync,
            count: self.open.as_ref().map_or(0, |open| open.records.iter().filter(|r| r.deleted.is_none()).count())
                as u32,
        }
    }

    fn exists(&self) -> bool {
        true
    }

    fn unlocked(&self) -> bool {
        self.open.is_some()
    }

    fn unlock(&mut self, password: &Hidden) -> Result<(), Fault> {
        let master = crypto::master_key(password.expose().as_bytes(), &self.account.email, self.account.kdf)?;
        let user = SymmetricKey::stretch(&master)
            .decrypt_key(&self.account.protected_key)
            .map_err(|_| Fault::WrongPassword)?;
        let hash = Zeroizing::new(crypto::master_password_hash(&master, password.expose().as_bytes()));
        self.open_with(user, hash, None);
        if let Some(cache) = self.account.cache.clone() {
            self.adopt(&cache)?;
            self.apply_queue_locally();
        }
        self.wanted = true;
        Ok(())
    }

    fn lock(&mut self) {
        self.open = None;
    }

    fn records(&self) -> Result<&[Record], Fault> {
        Ok(&self.opened()?.records)
    }

    fn folders(&self) -> Result<Vec<String>, Fault> {
        let mut names: Vec<String> = self.opened()?.folders.values().cloned().collect();
        names.sort();
        Ok(names)
    }

    fn save(&mut self, draft: &Draft, now: u64) -> Result<String, Fault> {
        let open = self.opened()?;
        let existing = draft.item.as_ref().map(|item| item.id.clone());
        let mut record = match &existing {
            Some(id) => open.records.iter().find(|record| record.id == *id).cloned().ok_or(Fault::NotFound)?,
            None => Record { created: now, ..Record::default() },
        };
        record.apply(draft, now);
        let raw = existing.as_ref().and_then(|id| open.raw.get(id).cloned());
        let queued = self.syncing || existing.as_deref().is_some_and(is_new);
        let folder =
            if queued { self.known_folder(&record.folder) } else { self.folder_id(&record.folder.clone(), now) };
        let body = cipher::request(&record, raw.as_ref(), &self.opened()?.keys, folder.as_deref())?;
        if !queued {
            let attempt = match &existing {
                Some(id) => self.call(now, |client, token| client.put(&format!("/ciphers/{id}"), token, Some(&body))),
                None => self.call(now, |client, token| client.post("/ciphers", token, &body)),
            };
            match attempt {
                Ok(item) => {
                    self.offline = false;
                    return self.adopt_cipher(&item);
                }
                Err(ApiError::Offline(_)) => self.offline = true,
                Err(error) => {
                    self.note(&error);
                    return Err(Fault::Failed(error.to_string()));
                }
            }
        }
        let id = match existing {
            Some(id) => id,
            None => format!("{NEW_PREFIX}{}", crypto::uuid_v4()?),
        };
        let base =
            if is_new(&id) { None } else { raw.as_ref().and_then(|raw| text(raw, "revisionDate")).map(str::to_string) };
        let pending = Pending::Save { id: id.clone(), body, base };
        self.apply_locally(&pending, now);
        self.enqueue(pending)?;
        Ok(id)
    }

    fn trash(&mut self, id: &str, now: u64) -> Result<(), Fault> {
        self.opened()?.records.iter().find(|record| record.id == id).ok_or(Fault::NotFound)?;
        self.change(Pending::Trash { id: id.into() }, now)
    }

    fn restore(&mut self, id: &str) -> Result<(), Fault> {
        self.opened()?.records.iter().find(|record| record.id == id).ok_or(Fault::NotFound)?;
        self.change(Pending::Restore { id: id.into() }, now())
    }

    fn purge(&mut self, id: &str) -> Result<(), Fault> {
        self.opened()?.records.iter().find(|record| record.id == id).ok_or(Fault::NotFound)?;
        self.change(Pending::Purge { id: id.into() }, now())
    }

    fn sync(&mut self) -> Result<(), Fault> {
        self.opened()?;
        self.wanted = true;
        Ok(())
    }

    fn conflicts(&self) -> Vec<Conflict> {
        let reuse = HashMap::new();
        self.account
            .clashes
            .iter()
            .filter_map(|clash| {
                let (local, remote) = self.clash_records(clash)?;
                Some(Conflict {
                    item: ItemRef::new(&self.account.id, &clash.id),
                    local: crate::daemon::record_detail(&self.account.id, &local, &reuse),
                    local_modified: local.modified,
                    remote: crate::daemon::record_detail(&self.account.id, &remote, &reuse),
                    remote_modified: remote.modified,
                    local_password: Hidden::new(local.password_text()),
                    remote_password: Hidden::new(remote.password_text()),
                })
            })
            .collect()
    }

    fn resolve(&mut self, id: &str, keep: Keep) -> Result<(), Fault> {
        let at = now();
        self.opened()?;
        let clash = self.account.clashes.iter().find(|clash| clash.id == id).cloned().ok_or(Fault::NotFound)?;
        let mut body = clash.body.clone();
        if let Some(object) = body.as_object_mut() {
            object.insert(
                "lastKnownRevisionDate".into(),
                field(&clash.server, "revisionDate").cloned().unwrap_or(Value::Null),
            );
            object.remove("id");
        }
        let outcome = match keep {
            Keep::Remote => Ok(None),
            Keep::Local => {
                self.call(at, |client, token| client.put(&format!("/ciphers/{id}"), token, Some(&body))).map(Some)
            }
            Keep::Both => self.call(at, |client, token| client.post("/ciphers", token, &body)).map(Some),
        };
        let item = outcome.map_err(|error| Fault::Failed(error.to_string()))?;
        self.account.clashes.retain(|known| known.id != id);
        if let Some(item) = item {
            self.adopt_cipher(&item)?;
        }
        self.wanted = true;
        self.save_account()
    }

    fn sends(&self) -> Result<Vec<Send>, Fault> {
        self.opened()?;
        Ok(self.raw_sends().iter().filter_map(|raw| self.send_of(raw)).collect())
    }

    fn create_send(&mut self, draft: &SendDraft) -> Result<Send, Fault> {
        let at = now();
        let open = self.opened()?;
        let secret: [u8; 16] = crypto::random_bytes()?;
        let key = SymmetricKey::send_key(&secret);
        let ends = dates::format(at + u64::from(draft.lifetime_hours.max(1)) * 3600);
        let password = draft
            .password
            .as_ref()
            .filter(|password| !password.is_empty())
            .map(|password| crypto::send_password_hash(password.expose(), &secret));
        let body = json!({
            "type": 0,
            "name": key.encrypt_text(draft.name.trim())?,
            "notes": Value::Null,
            "key": open.keys.user.encrypt(&secret)?,
            "maxAccessCount": draft.max_access,
            "expirationDate": ends,
            "deletionDate": ends,
            "text": { "text": key.encrypt_text(draft.text.expose())?, "hidden": draft.hide_text },
            "file": Value::Null,
            "password": password,
            "disabled": false,
            "hideEmail": draft.hide_email,
        });
        let created = self.call(at, |client, token| client.post("/sends", token, &body)).map_err(|error| {
            if let ApiError::Offline(_) = error {
                Fault::Failed("sending needs the server; you are offline".into())
            } else {
                Fault::Failed(error.to_string())
            }
        })?;
        self.cache_put("sends", &created);
        self.save_account()?;
        self.send_of(&created).ok_or_else(|| Fault::Failed("the server returned an unreadable send".into()))
    }

    fn disable_send(&mut self, id: &str) -> Result<(), Fault> {
        let at = now();
        let mut raw = self.raw_sends().into_iter().find(|raw| text(raw, "id") == Some(id)).ok_or(Fault::NotFound)?;
        if let Some(object) = raw.as_object_mut() {
            object.insert("disabled".into(), json!(true));
            object.remove("password");
        }
        let updated = self
            .call(at, |client, token| client.put(&format!("/sends/{id}"), token, Some(&raw)))
            .map_err(|error| Fault::Failed(error.to_string()))?;
        self.cache_put("sends", &updated);
        self.save_account()
    }

    fn delete_send(&mut self, id: &str) -> Result<(), Fault> {
        let at = now();
        self.call(at, |client, token| client.delete(&format!("/sends/{id}"), token))
            .map_err(|error| Fault::Failed(error.to_string()))?;
        self.cache_drop("sends", id);
        self.save_account()
    }

    fn rename(&mut self, name: &str) {
        if !name.is_empty() {
            self.account.name = name.to_string();
            let _ = self.save_account();
        }
    }

    fn forget(&mut self) -> Result<(), Fault> {
        self.open = None;
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(Fault::Failed(err.to_string())),
        }
    }

    fn begin_sync(&mut self, force: bool, now: u64) -> Option<SyncJob> {
        self.prepare_sync(force, now)
    }

    fn finish_sync(&mut self, done: SyncDone, now: u64) {
        self.apply_sync(done, now);
    }
}

pub mod index;
pub mod session;
pub mod sessions;

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use glide_ipc::keychain::{LOGIN, Response};
use zbus::fdo;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};

use self::index::Index;
use self::session::SessionKey;
use self::sessions::Sessions;
use crate::service::{Events, Service};
use crate::vault::{Item, Secret};

pub const BUS_NAME: &str = "org.freedesktop.secrets";
pub const ROOT: &str = "/org/freedesktop/secrets";
pub const COLLECTION: &str = "/org/freedesktop/secrets/collection/login";
pub const DEFAULT_ALIAS: &str = "/org/freedesktop/secrets/aliases/default";
const NONE: &str = "/";
const LABEL: &str = "org.freedesktop.Secret.Item.Label";
const ATTRIBUTES: &str = "org.freedesktop.Secret.Item.Attributes";
const GENERIC: &str = "org.freedesktop.Secret.Generic";
const PROMPT_WAIT: Duration = Duration::from_secs(90);
const PROMPT_POLL: Duration = Duration::from_millis(250);

pub type SecretValue = (OwnedObjectPath, Vec<u8>, Vec<u8>, String);

#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.freedesktop.Secret.Error")]
pub enum Error {
    #[zbus(error)]
    ZBus(zbus::Error),
    IsLocked(String),
    NoSession(String),
    NoSuchObject(String),
}

fn not_supported(what: &str) -> Error {
    Error::ZBus(zbus::Error::FDO(Box::new(fdo::Error::NotSupported(what.into()))))
}

fn path(text: &str) -> OwnedObjectPath {
    OwnedObjectPath::try_from(text.to_string()).expect("Secret Service paths are valid object paths")
}

#[must_use]
pub fn item_path(id: u64) -> OwnedObjectPath {
    path(&format!("{COLLECTION}/{id}"))
}

#[must_use]
pub fn item_id(object: &str) -> Option<u64> {
    let rest = object.strip_prefix(COLLECTION).or_else(|| object.strip_prefix(DEFAULT_ALIAS))?;
    rest.strip_prefix('/')?.parse().ok()
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.as_secs())
}

#[derive(Debug, Clone)]
enum Pending {
    Unlock(Vec<OwnedObjectPath>),
    Create { label: String, attributes: BTreeMap<String, String>, secret: Secret, content_type: String, replace: bool },
}

pub struct Secrets {
    service: Arc<Service>,
    events: Events,
    index_path: PathBuf,
    index: Mutex<Index>,
    sessions: Mutex<Sessions>,
    prompts: Mutex<HashMap<String, Pending>>,
    registered: Mutex<BTreeSet<u64>>,
    next: AtomicU64,
}

impl std::fmt::Debug for Secrets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Secrets").field("index", &self.index_path).finish_non_exhaustive()
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Secrets {
    #[must_use]
    pub fn new(service: Arc<Service>, events: Events, index_path: PathBuf) -> Arc<Self> {
        let index = Index::load(&index_path);
        Arc::new(Self {
            service,
            events,
            index_path,
            index: Mutex::new(index),
            sessions: Mutex::new(Sessions::default()),
            prompts: Mutex::new(HashMap::new()),
            registered: Mutex::new(BTreeSet::new()),
            next: AtomicU64::new(1),
        })
    }

    async fn off<T: Send + 'static>(self: &Arc<Self>, work: impl FnOnce(&Self) -> T + Send + 'static) -> T {
        let secrets = Arc::clone(self);
        blocking::unblock(move || work(&secrets)).await
    }

    fn unlocked(&self) -> bool {
        self.service.daemon().app_secrets_unlocked()
    }

    fn items(&self) -> Option<Vec<Item>> {
        let items = self.service.daemon().app_secrets().ok()?;
        self.refresh_index(&items);
        Some(items)
    }

    fn refresh_index(&self, items: &[Item]) {
        let mut index = lock(&self.index);
        let ids: Vec<u64> = items.iter().map(|item| item.id).collect();
        let fresh = Index::build(items, index.salt());
        if index.ids() != ids || *index != fresh {
            fresh.save(&self.index_path);
            *index = fresh;
        }
    }

    fn known_ids(&self) -> Vec<u64> {
        match self.items() {
            Some(items) => items.iter().map(|item| item.id).collect(),
            None => lock(&self.index).ids(),
        }
    }

    fn search(&self, wanted: &BTreeMap<String, String>) -> (Vec<OwnedObjectPath>, Vec<OwnedObjectPath>) {
        match self.items() {
            Some(items) => {
                let found = items
                    .iter()
                    .filter(|item| wanted.iter().all(|(key, value)| item.attributes.get(key) == Some(value)))
                    .map(|item| item_path(item.id))
                    .collect();
                (found, Vec::new())
            }
            None => (Vec::new(), lock(&self.index).matches(wanted).into_iter().map(item_path).collect()),
        }
    }

    fn item(&self, id: u64) -> Result<Item, Error> {
        let items = self.items().ok_or_else(|| Error::IsLocked("the keychain is locked".into()))?;
        items.into_iter().find(|item| item.id == id).ok_or_else(|| Error::NoSuchObject(format!("no item {id}")))
    }

    fn check_session(&self, session: &ObjectPath<'_>) -> Result<Option<SessionKey>, Error> {
        lock(&self.sessions).key(session.as_str()).ok_or_else(|| Error::NoSession(format!("no session {session}")))
    }

    pub fn forget_owner(&self, server: &zbus::blocking::ObjectServer, owner: &str) {
        let paths = lock(&self.sessions).drop_owner(owner);
        for path in paths {
            let _ = server.remove::<SessionObject, _>(path.as_str());
        }
    }

    fn secret_of(item: &Item, session: &ObjectPath<'_>, key: Option<&SessionKey>) -> Result<SecretValue, Error> {
        let content_type =
            if item.content_type.is_empty() { "text/plain".to_string() } else { item.content_type.clone() };
        let (parameters, value) = match key {
            Some(key) => key
                .seal(item.secret.expose())
                .ok_or_else(|| Error::ZBus(zbus::Error::Failure("the secret could not be sealed".into())))?,
            None => (Vec::new(), item.secret.expose().to_vec()),
        };
        Ok((session.to_owned().into(), parameters, value, content_type))
    }

    fn incoming(&self, secret: &SecretValue) -> Result<Secret, Error> {
        match self.check_session(&secret.0.as_ref())? {
            Some(key) => key
                .open(&secret.1, &secret.2)
                .map(|plain| Secret::new(plain.to_vec()))
                .ok_or_else(|| Error::ZBus(zbus::Error::Failure("the secret does not decrypt".into()))),
            None => Ok(Secret::new(secret.2.clone())),
        }
    }

    fn edit(&self, mut change: impl FnMut(&mut Vec<Item>, &mut u64)) -> Result<(), Error> {
        let result = self.service.daemon().edit_app_secrets(&mut change);
        result.map_err(|fault| match fault {
            crate::provider::Fault::Locked => Error::IsLocked("the keychain is locked".into()),
            other => Error::ZBus(zbus::Error::Failure(other.to_string())),
        })?;
        let _ = self.items();
        Ok(())
    }

    fn create(&self, pending: &Pending) -> Result<(u64, bool), Error> {
        let Pending::Create { label, attributes, secret, content_type, replace } = pending else {
            return Err(not_supported("not a create request"));
        };
        let at = now();
        let mut outcome = (0, false);
        self.edit(|items, next| {
            if *replace && let Some(existing) = items.iter_mut().find(|item| item.attributes == *attributes) {
                existing.label.clone_from(label);
                existing.secret = secret.clone();
                existing.content_type.clone_from(content_type);
                existing.modified = at;
                outcome = (existing.id, false);
                return;
            }
            *next += 1;
            let mut item = Item::new(crate::local::SERVICE_COLLECTION, label, at);
            item.id = *next;
            item.attributes = attributes.clone();
            item.secret = secret.clone();
            item.content_type.clone_from(content_type);
            outcome = (item.id, true);
            items.push(item);
        })?;
        Ok(outcome)
    }

    fn new_path(&self, kind: &str) -> String {
        format!("{ROOT}/{kind}/{}", self.next.fetch_add(1, Ordering::Relaxed))
    }

    async fn sync_objects(self: &Arc<Self>, server: &zbus::ObjectServer) {
        let wanted: BTreeSet<u64> = self.off(Self::known_ids).await.into_iter().collect();
        let (added, removed) = {
            let registered = lock(&self.registered);
            let added: Vec<u64> = wanted.difference(&registered).copied().collect();
            let removed: Vec<u64> = registered.difference(&wanted).copied().collect();
            (added, removed)
        };
        for id in added {
            let _ = server.at(item_path(id), ItemObject { secrets: Arc::clone(self), id }).await;
        }
        for id in removed {
            let _ = server.remove::<ItemObject, _>(item_path(id)).await;
        }
        *lock(&self.registered) = wanted;
    }

    fn after_unlock(self: &Arc<Self>, connection: &zbus::Connection, pending: &Pending) -> (bool, OwnedValue) {
        match pending {
            Pending::Unlock(objects) => (false, Value::from(objects.clone()).try_into().expect("ao is owned")),
            Pending::Create { .. } => match self.create(pending) {
                Ok((id, created)) => {
                    zbus::block_on(self.sync_objects(connection.object_server()));
                    if created {
                        let _ = zbus::block_on(collection_signal(connection, Change::Created, id));
                    }
                    (false, Value::from(item_path(id)).try_into().expect("o is owned"))
                }
                Err(_) => (true, Value::from(NONE.to_string()).try_into().expect("s is owned")),
            },
        }
    }

    fn prompt(self: &Arc<Self>, pending: Pending) -> OwnedObjectPath {
        let prompt = self.new_path("prompt");
        lock(&self.prompts).insert(prompt.clone(), pending);
        path(&prompt)
    }
}

#[derive(Debug, Clone, Copy)]
enum Change {
    Created,
    Changed,
    Deleted,
}

async fn collection_signal(connection: &zbus::Connection, change: Change, id: u64) -> zbus::Result<()> {
    let item = item_path(id);
    for at in [COLLECTION, DEFAULT_ALIAS] {
        let emitter = SignalEmitter::new(connection, at)?;
        match change {
            Change::Created => CollectionObject::item_created(&emitter, item.as_ref()).await?,
            Change::Changed => CollectionObject::item_changed(&emitter, item.as_ref()).await?,
            Change::Deleted => CollectionObject::item_deleted(&emitter, item.as_ref()).await?,
        }
    }
    Ok(())
}

fn attributes_of(properties: &HashMap<String, OwnedValue>) -> BTreeMap<String, String> {
    properties
        .get(ATTRIBUTES)
        .and_then(|value| value.try_clone().ok())
        .and_then(|value| HashMap::<String, String>::try_from(value).ok())
        .map(|map| map.into_iter().collect())
        .unwrap_or_default()
}

fn label_of(properties: &HashMap<String, OwnedValue>) -> String {
    properties
        .get(LABEL)
        .and_then(|value| value.try_clone().ok())
        .and_then(|value| String::try_from(value).ok())
        .unwrap_or_default()
}

pub struct ServiceObject {
    secrets: Arc<Secrets>,
}

#[allow(clippy::unused_self, clippy::needless_pass_by_value)] // The Secret Service spec fixes these D-Bus signatures, and zbus methods take &self and owned arguments
#[zbus::interface(name = "org.freedesktop.Secret.Service")]
impl ServiceObject {
    async fn open_session(
        &self, algorithm: &str, input: OwnedValue, #[zbus(object_server)] server: &zbus::ObjectServer,
        #[zbus(header)] header: zbus::message::Header<'_>, #[zbus(connection)] connection: &zbus::Connection,
    ) -> Result<(OwnedValue, OwnedObjectPath), Error> {
        let (output, key): (OwnedValue, Option<SessionKey>) = match algorithm {
            session::PLAIN => (Value::from("").try_into().expect("s is owned"), None),
            session::DH => {
                let peer = Vec::<u8>::try_from(input).map_err(|_| {
                    Error::ZBus(zbus::Error::FDO(Box::new(fdo::Error::InvalidArgs("expected ay".into()))))
                })?;
                let (public, key) = session::agree(&peer).ok_or_else(|| {
                    Error::ZBus(zbus::Error::FDO(Box::new(fdo::Error::InvalidArgs("bad public key".into()))))
                })?;
                (Value::from(public).try_into().expect("ay is owned"), Some(key))
            }
            _ => return Err(not_supported("the algorithm is not offered")),
        };
        let owner = header.sender().map(ToString::to_string);
        let session = self.secrets.new_path("session");
        let crowded = {
            let mut sessions = lock(&self.secrets.sessions);
            sessions.insert(session.clone(), key, owner);
            sessions.over_limit()
        };
        let evicted = if crowded {
            let names = live_names(connection).await;
            let alive = |owner: &str| names.as_ref().is_none_or(|names| names.contains(owner));
            lock(&self.secrets.sessions).evict(alive)
        } else {
            None
        };
        if let Some(gone) = &evicted {
            let _ = server.remove::<SessionObject, _>(gone.as_str()).await;
        }
        if evicted.as_deref() != Some(session.as_str()) {
            let _ = server
                .at(session.as_str(), SessionObject { secrets: Arc::clone(&self.secrets), path: session.clone() })
                .await;
        }
        Ok((output, path(&session)))
    }

    fn create_collection(
        &self, properties: HashMap<String, OwnedValue>, alias: &str,
    ) -> (OwnedObjectPath, OwnedObjectPath) {
        let _ = (properties, alias);
        (path(COLLECTION), path(NONE))
    }

    async fn search_items(
        &self, attributes: HashMap<String, String>, #[zbus(object_server)] server: &zbus::ObjectServer,
    ) -> (Vec<OwnedObjectPath>, Vec<OwnedObjectPath>) {
        self.secrets.sync_objects(server).await;
        let wanted = attributes.into_iter().collect();
        self.secrets.off(move |secrets| secrets.search(&wanted)).await
    }

    async fn unlock(
        &self, objects: Vec<OwnedObjectPath>, #[zbus(object_server)] server: &zbus::ObjectServer,
    ) -> fdo::Result<(Vec<OwnedObjectPath>, OwnedObjectPath)> {
        if self.secrets.off(Secrets::unlocked).await {
            return Ok((objects, path(NONE)));
        }
        let prompt = self.secrets.prompt(Pending::Unlock(objects));
        let object = PromptObject { secrets: Arc::clone(&self.secrets), path: prompt.to_string() };
        let _ = server.at(prompt.as_str(), object).await;
        Ok((Vec::new(), prompt))
    }

    fn lock(&self, objects: Vec<OwnedObjectPath>) -> (Vec<OwnedObjectPath>, OwnedObjectPath) {
        let _ = objects;
        (Vec::new(), path(NONE))
    }

    async fn get_secrets(
        &self, items: Vec<OwnedObjectPath>, session: ObjectPath<'_>,
    ) -> Result<HashMap<OwnedObjectPath, SecretValue>, Error> {
        let key = self.secrets.check_session(&session)?;
        let all =
            self.secrets.off(Secrets::items).await.ok_or_else(|| Error::IsLocked("the keychain is locked".into()))?;
        let mut found = HashMap::new();
        for object in items {
            let Some(item) = item_id(object.as_str()).and_then(|id| all.iter().find(|item| item.id == id)) else {
                continue;
            };
            let value = Secrets::secret_of(item, &session, key.as_ref())?;
            found.insert(object, value);
        }
        Ok(found)
    }

    fn read_alias(&self, name: &str) -> OwnedObjectPath {
        if matches!(name, "default" | "login") { path(COLLECTION) } else { path(NONE) }
    }

    fn set_alias(&self, name: &str, collection: ObjectPath<'_>) {
        let _ = (name, collection);
    }

    #[zbus(property)]
    fn collections(&self) -> Vec<OwnedObjectPath> {
        vec![path(COLLECTION)]
    }
}

pub struct CollectionObject {
    secrets: Arc<Secrets>,
}

#[allow(clippy::unused_self, clippy::needless_pass_by_value)] // The Secret Service spec fixes these D-Bus signatures, and zbus methods take &self and owned arguments
#[zbus::interface(name = "org.freedesktop.Secret.Collection")]
impl CollectionObject {
    fn delete(&self) -> Result<OwnedObjectPath, Error> {
        Err(not_supported("the login collection cannot be deleted"))
    }

    async fn search_items(
        &self, attributes: HashMap<String, String>, #[zbus(object_server)] server: &zbus::ObjectServer,
    ) -> Vec<OwnedObjectPath> {
        self.secrets.sync_objects(server).await;
        let wanted = attributes.into_iter().collect();
        let (unlocked, locked) = self.secrets.off(move |secrets| secrets.search(&wanted)).await;
        unlocked.into_iter().chain(locked).collect()
    }

    async fn create_item(
        &self, properties: HashMap<String, OwnedValue>, secret: SecretValue, replace: bool,
        #[zbus(object_server)] server: &zbus::ObjectServer, #[zbus(connection)] connection: &zbus::Connection,
    ) -> Result<(OwnedObjectPath, OwnedObjectPath), Error> {
        let value = self.secrets.incoming(&secret)?;
        let pending = Pending::Create {
            label: label_of(&properties),
            attributes: attributes_of(&properties),
            secret: value,
            content_type: secret.3,
            replace,
        };
        if !self.secrets.off(Secrets::unlocked).await {
            let prompt = self.secrets.prompt(pending);
            let object = PromptObject { secrets: Arc::clone(&self.secrets), path: prompt.to_string() };
            let _ = server.at(prompt.as_str(), object).await;
            return Ok((path(NONE), prompt));
        }
        let (id, created) = self.secrets.off(move |secrets| secrets.create(&pending)).await?;
        self.secrets.sync_objects(server).await;
        let change = if created { Change::Created } else { Change::Changed };
        let _ = collection_signal(connection, change, id).await;
        Ok((item_path(id), path(NONE)))
    }

    #[zbus(property)]
    async fn items(&self, #[zbus(object_server)] server: &zbus::ObjectServer) -> Vec<OwnedObjectPath> {
        self.secrets.sync_objects(server).await;
        self.secrets.off(Secrets::known_ids).await.into_iter().map(item_path).collect()
    }

    #[zbus(property)]
    fn label(&self) -> String {
        "Glide".to_string()
    }

    #[zbus(property)]
    async fn locked(&self) -> bool {
        !self.secrets.off(Secrets::unlocked).await
    }

    #[zbus(property)]
    fn created(&self) -> u64 {
        0
    }

    #[zbus(property)]
    async fn modified(&self) -> u64 {
        let items = self.secrets.off(Secrets::items).await;
        items.map_or(0, |items| items.iter().map(|item| item.modified).max().unwrap_or(0))
    }

    #[zbus(signal)]
    async fn item_created(emitter: &SignalEmitter<'_>, item: ObjectPath<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn item_deleted(emitter: &SignalEmitter<'_>, item: ObjectPath<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn item_changed(emitter: &SignalEmitter<'_>, item: ObjectPath<'_>) -> zbus::Result<()>;
}

pub struct ItemObject {
    secrets: Arc<Secrets>,
    id: u64,
}

#[allow(clippy::unused_self, clippy::needless_pass_by_value)] // The Secret Service spec fixes these D-Bus signatures, and zbus methods take &self and owned arguments
#[zbus::interface(name = "org.freedesktop.Secret.Item")]
impl ItemObject {
    async fn delete(
        &self, #[zbus(object_server)] server: &zbus::ObjectServer, #[zbus(connection)] connection: &zbus::Connection,
    ) -> Result<OwnedObjectPath, Error> {
        let id = self.id;
        self.secrets
            .off(move |secrets| {
                secrets.item(id)?;
                secrets.edit(|items, _| items.retain(|item| item.id != id))
            })
            .await?;
        self.secrets.sync_objects(server).await;
        let _ = collection_signal(connection, Change::Deleted, id).await;
        Ok(path(NONE))
    }

    async fn get_secret(&self, session: ObjectPath<'_>) -> Result<SecretValue, Error> {
        let key = self.secrets.check_session(&session)?;
        let item = self.current().await?;
        Secrets::secret_of(&item, &session, key.as_ref())
    }

    async fn set_secret(
        &self, secret: SecretValue, #[zbus(connection)] connection: &zbus::Connection,
    ) -> Result<(), Error> {
        let value = self.secrets.incoming(&secret)?;
        let (id, at) = (self.id, now());
        let content_type = secret.3;
        let found = self
            .secrets
            .off(move |secrets| {
                let mut found = false;
                secrets
                    .edit(|items, _| {
                        if let Some(item) = items.iter_mut().find(|item| item.id == id) {
                            item.secret = value.clone();
                            item.content_type.clone_from(&content_type);
                            item.modified = at;
                            found = true;
                        }
                    })
                    .map(|()| found)
            })
            .await?;
        if !found {
            return Err(Error::NoSuchObject(format!("no item {id}")));
        }
        let _ = collection_signal(connection, Change::Changed, id).await;
        Ok(())
    }

    #[zbus(property)]
    async fn locked(&self) -> bool {
        !self.secrets.off(Secrets::unlocked).await
    }

    #[zbus(property)]
    async fn attributes(&self) -> HashMap<String, String> {
        self.current().await.map(|item| item.attributes.into_iter().collect()).unwrap_or_default()
    }

    #[zbus(property)]
    async fn set_attributes(&mut self, attributes: HashMap<String, String>) -> fdo::Result<()> {
        let (id, at) = (self.id, now());
        let attributes: BTreeMap<String, String> = attributes.into_iter().collect();
        self.secrets
            .off(move |secrets| {
                secrets.edit(|items, _| {
                    if let Some(item) = items.iter_mut().find(|item| item.id == id) {
                        item.attributes.clone_from(&attributes);
                        item.modified = at;
                    }
                })
            })
            .await
            .map_err(|err| fdo::Error::Failed(err.to_string()))
    }

    #[zbus(property)]
    async fn label(&self) -> String {
        self.current().await.map(|item| item.label).unwrap_or_default()
    }

    #[zbus(property)]
    async fn set_label(&mut self, label: String) -> fdo::Result<()> {
        let (id, at) = (self.id, now());
        let found = self
            .secrets
            .off(move |secrets| {
                let mut found = false;
                secrets
                    .edit(|items, _| {
                        if let Some(item) = items.iter_mut().find(|item| item.id == id) {
                            item.label.clone_from(&label);
                            item.modified = at;
                            found = true;
                        }
                    })
                    .map(|()| found)
            })
            .await
            .map_err(|err| fdo::Error::Failed(err.to_string()))?;
        if found { Ok(()) } else { Err(fdo::Error::UnknownObject(format!("no item {id}"))) }
    }

    #[zbus(property, name = "Type")]
    async fn kind(&self) -> String {
        self.current()
            .await
            .ok()
            .and_then(|item| item.attributes.get("xdg:schema").cloned())
            .unwrap_or_else(|| GENERIC.to_string())
    }

    #[zbus(property)]
    async fn created(&self) -> u64 {
        self.current().await.map_or(0, |item| item.created)
    }

    #[zbus(property)]
    async fn modified(&self) -> u64 {
        self.current().await.map_or(0, |item| item.modified)
    }
}

impl ItemObject {
    async fn current(&self) -> Result<Item, Error> {
        let id = self.id;
        self.secrets.off(move |secrets| secrets.item(id)).await
    }
}

pub struct SessionObject {
    secrets: Arc<Secrets>,
    path: String,
}

#[zbus::interface(name = "org.freedesktop.Secret.Session")]
impl SessionObject {
    async fn close(&self, #[zbus(object_server)] server: &zbus::ObjectServer) {
        lock(&self.secrets.sessions).remove(&self.path);
        let _ = server.remove::<Self, _>(self.path.as_str()).await;
    }
}

pub struct PromptObject {
    secrets: Arc<Secrets>,
    path: String,
}

#[zbus::interface(name = "org.freedesktop.Secret.Prompt")]
impl PromptObject {
    async fn prompt(
        &self, window_id: &str, #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(connection)] connection: &zbus::Connection,
    ) {
        let _ = window_id;
        let requester = caller_program(&header, connection).await;
        let closed = self.secrets.service.prompts_closed();
        let events = Arc::clone(&self.secrets.events);
        blocking::unblock(move || events(Response::UnlockRequested { vault: Some(LOGIN.into()), requester })).await;
        let (secrets, path, connection) = (Arc::clone(&self.secrets), self.path.clone(), connection.clone());
        let spawned = std::thread::Builder::new().name("glide-secrets-prompt".into()).spawn(move || {
            let started = Instant::now();
            while !secrets.unlocked() && started.elapsed() < PROMPT_WAIT {
                if !lock(&secrets.prompts).contains_key(&path) {
                    return;
                }
                std::thread::sleep(PROMPT_POLL);
                if secrets.service.prompts_closed() != closed && !secrets.unlocked() {
                    break;
                }
            }
            let Some(pending) = lock(&secrets.prompts).remove(&path) else {
                return;
            };
            let (dismissed, result) = if secrets.unlocked() {
                secrets.after_unlock(&connection, &pending)
            } else {
                (true, Value::from(Vec::<OwnedObjectPath>::new()).try_into().expect("ao is owned"))
            };
            let _ = zbus::block_on(complete(&connection, &path, dismissed, result));
        });
        if let Err(err) = spawned {
            eprintln!("glide-keychain: a Secret Service prompt could not wait for the unlock ({err})");
        }
    }

    async fn dismiss(&self, #[zbus(connection)] connection: &zbus::Connection) {
        if lock(&self.secrets.prompts).remove(&self.path).is_some() {
            let empty: OwnedValue = Value::from(Vec::<OwnedObjectPath>::new()).try_into().expect("ao is owned");
            let _ = complete(connection, &self.path, true, empty).await;
        }
    }

    #[zbus(signal)]
    async fn completed(emitter: &SignalEmitter<'_>, dismissed: bool, result: Value<'_>) -> zbus::Result<()>;
}

async fn caller_program(header: &zbus::message::Header<'_>, connection: &zbus::Connection) -> Option<String> {
    let sender = header.sender()?.to_owned();
    let proxy = fdo::DBusProxy::new(connection).await.ok()?;
    let pid = proxy.get_connection_unix_process_id(sender.into()).await.ok()?;
    std::fs::read_link(format!("/proc/{pid}/exe")).ok().map(|path| path.display().to_string())
}

async fn complete(connection: &zbus::Connection, at: &str, dismissed: bool, result: OwnedValue) -> zbus::Result<()> {
    let emitter = SignalEmitter::new(connection, at.to_string())?;
    PromptObject::completed(&emitter, dismissed, Value::from(result)).await?;
    let _ = connection.object_server().remove::<PromptObject, _>(at).await;
    Ok(())
}

async fn live_names(connection: &zbus::Connection) -> Option<HashSet<String>> {
    let bus = fdo::DBusProxy::new(connection).await.ok()?;
    let names = bus.list_names().await.ok()?;
    Some(names.into_iter().map(|name| name.to_string()).collect())
}

fn watch_clients(connection: &zbus::blocking::Connection, secrets: &Arc<Secrets>) -> zbus::Result<()> {
    let bus = zbus::blocking::fdo::DBusProxy::new(connection)?;
    let changes = bus.receive_name_owner_changed()?;
    let (connection, secrets) = (connection.clone(), Arc::clone(secrets));
    let spawned = std::thread::Builder::new().name("glide-secrets-clients".into()).spawn(move || {
        let _bus = bus;
        for change in changes {
            let Ok(args) = change.args() else {
                continue;
            };
            if args.new_owner().is_some() {
                continue;
            }
            if let zbus::names::BusName::Unique(owner) = args.name() {
                secrets.forget_owner(&connection.object_server(), owner.as_str());
            }
        }
    });
    if let Err(err) = spawned {
        eprintln!("glide-keychain: the Secret Service cannot tell when a client leaves ({err})");
    }
    Ok(())
}

pub fn serve(connection: &zbus::blocking::Connection, secrets: &Arc<Secrets>) -> zbus::Result<()> {
    let server = connection.object_server();
    server.at(ROOT, ServiceObject { secrets: Arc::clone(secrets) })?;
    server.at(COLLECTION, CollectionObject { secrets: Arc::clone(secrets) })?;
    server.at(DEFAULT_ALIAS, CollectionObject { secrets: Arc::clone(secrets) })?;
    zbus::block_on(secrets.sync_objects(connection.inner().object_server()));
    if let Err(err) = watch_clients(connection, secrets) {
        eprintln!("glide-keychain: the Secret Service cannot tell when a client leaves ({err})");
    }
    Ok(())
}

pub fn claim(connection: &zbus::blocking::Connection, enabled: bool) {
    if enabled {
        let flags = zbus::fdo::RequestNameFlags::DoNotQueue.into();
        match connection.request_name_with_flags(BUS_NAME, flags) {
            Ok(zbus::fdo::RequestNameReply::PrimaryOwner | zbus::fdo::RequestNameReply::AlreadyOwner) => {}
            Ok(_) => eprintln!("glide-keychain: another program already serves {BUS_NAME}; leaving it be"),
            Err(err) => eprintln!("glide-keychain: {BUS_NAME} was not claimed ({err})"),
        }
    } else {
        let _ = connection.release_name(BUS_NAME);
    }
}

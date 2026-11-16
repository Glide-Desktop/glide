use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde_json::Value;
use zbus::fdo;
use zbus::message::Header;
use zbus::object_server::SignalEmitter;

pub use glide_ipc::config::{BUS_INTERFACE as INTERFACE, BUS_NAME, BUS_PATH as PATH};
use glide_ipc::config::{GUARDED, TRUSTED_WRITERS};

use crate::store::{Error, Store, Values};

pub struct Config {
    store: Arc<Mutex<Store>>,
    trusted: Vec<PathBuf>,
}

fn failed(err: &Error) -> fdo::Error {
    match err {
        Error::Db(_) => fdo::Error::Failed(err.to_string()),
        _ => fdo::Error::InvalidArgs(err.to_string()),
    }
}

fn encode(values: Values) -> HashMap<String, String> {
    values.into_iter().map(|(key, value)| (key, value.to_string())).collect()
}

fn decode(values: HashMap<String, String>) -> fdo::Result<Values> {
    values
        .into_iter()
        .map(|(key, text)| {
            let value = serde_json::from_str::<Value>(&text)
                .map_err(|err| fdo::Error::InvalidArgs(format!("{key} is not JSON: {err}")))?;
            Ok((key, value))
        })
        .collect()
}

#[must_use]
pub fn trusted_next_to(exe: Option<&std::path::Path>) -> Vec<PathBuf> {
    let Some(dir) = exe.and_then(std::path::Path::parent) else {
        return Vec::new();
    };
    TRUSTED_WRITERS.iter().filter_map(|name| dir.join(name).canonicalize().ok()).collect()
}

impl Config {
    #[must_use]
    pub fn new(store: Arc<Mutex<Store>>, trusted: Vec<PathBuf>) -> Self {
        Self { store, trusted }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Store> {
        self.store.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    async fn writer(&self, namespace: &str, header: &Header<'_>, connection: &zbus::Connection) -> fdo::Result<String> {
        let sender = header.sender().ok_or_else(|| fdo::Error::AccessDenied("no sender".into()))?.to_owned();
        let pid = fdo::DBusProxy::new(connection).await?.get_connection_unix_process_id(sender.into()).await?;
        let program = std::fs::read_link(format!("/proc/{pid}/exe")).ok();
        if GUARDED.contains(&namespace) && !program.as_ref().is_some_and(|program| self.trusted.contains(program)) {
            return Err(fdo::Error::AccessDenied(format!("only Glide itself may change {namespace}")));
        }
        Ok(program.map_or_else(|| format!("pid {pid}"), |program| program.display().to_string()))
    }

    async fn announce(emitter: &SignalEmitter<'_>, namespace: &str, changed: Values) -> fdo::Result<()> {
        if !changed.is_empty() {
            Self::changed(emitter, namespace, encode(changed)).await?;
        }
        Ok(())
    }
}

#[zbus::interface(name = "org.glide.Config1")]
impl Config {
    fn get(&self, namespace: &str, key: &str) -> fdo::Result<String> {
        self.lock().get(namespace, key).map(|value| value.to_string()).map_err(|err| failed(&err))
    }

    fn get_all(&self, namespace: &str) -> fdo::Result<HashMap<String, String>> {
        self.lock().get_all(namespace).map(encode).map_err(|err| failed(&err))
    }

    async fn set_many(
        &self, namespace: String, values: HashMap<String, String>, #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &zbus::Connection, #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<()> {
        let origin = self.writer(&namespace, &header, connection).await?;
        let values = decode(values)?;
        let changed = self.lock().set_many(&namespace, values, &origin).map_err(|err| failed(&err))?;
        Self::announce(&emitter, &namespace, changed).await
    }

    async fn reset(
        &self, namespace: String, keys: Vec<String>, #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &zbus::Connection, #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<()> {
        let origin = self.writer(&namespace, &header, connection).await?;
        let changed = self.lock().reset(&namespace, &keys, &origin).map_err(|err| failed(&err))?;
        Self::announce(&emitter, &namespace, changed).await
    }

    fn history(&self, namespace: &str, limit: u32) -> fdo::Result<String> {
        let entries = self.lock().history(namespace, limit).map_err(|err| failed(&err))?;
        serde_json::to_string(&entries).map_err(|err| fdo::Error::Failed(err.to_string()))
    }

    async fn undo(
        &self, id: i64, #[zbus(header)] header: Header<'_>, #[zbus(connection)] connection: &zbus::Connection,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<()> {
        let namespace = self.lock().change(id).map_err(|err| failed(&err))?.map(|(namespace, _)| namespace);
        let namespace = namespace.ok_or_else(|| failed(&Error::NotUndoable(id)))?;
        let origin = self.writer(&namespace, &header, connection).await?;
        let (namespace, changed) = self.lock().undo(id, &origin).map_err(|err| failed(&err))?;
        Self::announce(&emitter, &namespace, changed).await
    }

    #[zbus(signal)]
    async fn changed(emitter: &SignalEmitter<'_>, namespace: &str, values: HashMap<String, String>)
    -> zbus::Result<()>;
}

pub fn serve_on(
    builder: zbus::blocking::connection::Builder<'_>, store: Arc<Mutex<Store>>, trusted: Vec<PathBuf>,
) -> zbus::Result<zbus::blocking::Connection> {
    builder.name(BUS_NAME)?.serve_at(PATH, Config::new(store, trusted))?.build()
}

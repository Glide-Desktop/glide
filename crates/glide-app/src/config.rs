use std::collections::HashMap;

use futures_channel::mpsc::{UnboundedReceiver, unbounded};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

pub use glide_ipc::config::{
    BUS_INTERFACE as INTERFACE, BUS_NAME, BUS_PATH as PATH, CHANGED, SHELL, Values, flatten, nest,
};

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Change {
    pub id: i64,
    pub key: String,
    pub before: Option<Value>,
    pub after: Option<Value>,
    pub at: i64,
    pub origin: String,
}

pub fn read<T: DeserializeOwned>(values: &Values) -> serde_json::Result<T> {
    serde_json::from_value(nest(values))
}

fn decode(values: HashMap<String, String>) -> Values {
    values.into_iter().filter_map(|(key, text)| Some((key, serde_json::from_str(&text).ok()?))).collect()
}

fn encode(values: &Values) -> HashMap<String, String> {
    values.iter().map(|(key, value)| (key.clone(), value.to_string())).collect()
}

pub struct Client {
    connection: zbus::blocking::Connection,
}

impl Client {
    pub fn session() -> zbus::Result<Self> {
        Ok(Self { connection: zbus::blocking::Connection::session()? })
    }

    #[must_use]
    pub const fn on(connection: zbus::blocking::Connection) -> Self {
        Self { connection }
    }

    fn call<B, R>(&self, method: &str, body: &B) -> zbus::Result<R>
    where
        B: serde::Serialize + zbus::zvariant::DynamicType,
        R: DeserializeOwned + zbus::zvariant::Type,
    {
        self.connection.call_method(Some(BUS_NAME), PATH, Some(INTERFACE), method, body)?.body().deserialize::<R>()
    }

    pub fn get_all(&self, namespace: &str) -> zbus::Result<Values> {
        self.call::<_, HashMap<String, String>>("GetAll", &(namespace,)).map(decode)
    }

    pub fn set_many(&self, namespace: &str, values: &Values) -> zbus::Result<()> {
        self.call("SetMany", &(namespace, encode(values)))
    }

    pub fn reset(&self, namespace: &str, keys: &[String]) -> zbus::Result<()> {
        self.call("Reset", &(namespace, keys))
    }

    pub fn history(&self, namespace: &str, limit: u32) -> zbus::Result<Vec<Change>> {
        let text: String = self.call("History", &(namespace, limit))?;
        serde_json::from_str(&text).map_err(|err| zbus::Error::Failure(err.to_string()))
    }

    pub fn undo(&self, id: i64) -> zbus::Result<()> {
        self.call("Undo", &(id,))
    }
}

fn changes_of(
    connection: &zbus::blocking::Connection, namespace: &str,
) -> zbus::Result<zbus::blocking::MessageIterator> {
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .path(PATH)?
        .interface(INTERFACE)?
        .member(CHANGED)?
        .arg(0, namespace.to_owned())?
        .build();
    zbus::blocking::MessageIterator::for_match_rule(rule, connection, None)
}

fn follow(
    connection: zbus::blocking::Connection, namespace: &str, sink: &(impl Fn(Values) -> bool + Send),
) -> zbus::Result<()> {
    let changes = changes_of(&connection, namespace)?;
    let client = Client::on(connection);
    if !sink(client.get_all(namespace)?) {
        return Ok(());
    }
    for message in changes {
        message?;
        if !sink(client.get_all(namespace)?) {
            return Ok(());
        }
    }
    Err(zbus::Error::Failure("the session bus connection closed".into()))
}

pub fn watch_with(namespace: &str, sink: impl Fn(Values) -> bool + Send + 'static) {
    let namespace = namespace.to_owned();
    let spawned = std::thread::Builder::new().name(format!("glide-config-{namespace}")).spawn(move || {
        let mut backoff = crate::RETRY;
        loop {
            match zbus::blocking::Connection::session().and_then(|connection| follow(connection, &namespace, &sink)) {
                Ok(()) => return,
                Err(err) => tracing::debug!("stopped following the {namespace} settings: {err}"),
            }
            std::thread::sleep(backoff);
            backoff = (backoff * 2).min(crate::MAX_RETRY);
        }
    });
    if let Err(err) = spawned {
        tracing::warn!("cannot follow the settings: {err}");
    }
}

#[must_use]
pub fn watch(namespace: &str) -> UnboundedReceiver<Values> {
    let (sender, receiver) = unbounded();
    watch_with(namespace, move |values| sender.unbounded_send(values).is_ok());
    receiver
}

use std::collections::HashMap;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use glide_ipc::config::{BUS_INTERFACE as INTERFACE, BUS_NAME, BUS_PATH as PATH, Values, flatten, nest};

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
        B: Serialize + zbus::zvariant::DynamicType,
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

fn values_of<T: Serialize>(value: &T) -> Values {
    serde_json::to_value(value).map(flatten).unwrap_or_default()
}

#[must_use]
pub fn changed_keys<T: Serialize>(before: &T, after: &T) -> Values {
    let before = values_of(before);
    values_of(after).into_iter().filter(|(key, value)| before.get(key) != Some(value)).collect()
}

#[must_use]
pub fn load<T: DeserializeOwned + Default>(namespace: &str) -> T {
    Client::session()
        .and_then(|client| client.get_all(namespace))
        .map(|values| read(&values).unwrap_or_default())
        .unwrap_or_default()
}

pub fn update<T>(namespace: &str, change: impl FnOnce(&mut T)) -> zbus::Result<T>
where
    T: Serialize + DeserializeOwned + Default + Clone,
{
    let client = Client::session()?;
    let before: T = read(&client.get_all(namespace)?).unwrap_or_default();
    let mut after = before.clone();
    change(&mut after);
    let changed = changed_keys(&before, &after);
    if !changed.is_empty() {
        client.set_many(namespace, &changed)?;
    }
    Ok(after)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
    #[serde(default, rename_all = "kebab-case")]
    struct Prefs {
        clear_after: u32,
        offer: bool,
    }

    #[test]
    fn only_the_keys_a_change_touched_are_written() {
        let before = Prefs { clear_after: 30, offer: true };
        let after = Prefs { offer: false, ..before };
        assert_eq!(changed_keys(&before, &after), Values::from([("offer".to_owned(), Value::Bool(false))]));
        assert!(changed_keys(&before, &before).is_empty());
    }
}

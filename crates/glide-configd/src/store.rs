use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use serde_json::Value;

pub type Values = BTreeMap<String, Value>;

const SCHEMA_VERSION: i64 = 1;

#[derive(Debug)]
pub enum Error {
    UnknownNamespace(String),
    UnknownKey { namespace: String, key: String },
    WrongType { namespace: String, key: String },
    NotUndoable(i64),
    Db(rusqlite::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownNamespace(namespace) => write!(f, "no defaults are installed for {namespace}"),
            Self::UnknownKey { namespace, key } => write!(f, "{namespace} has no key {key}"),
            Self::WrongType { namespace, key } => write!(f, "{namespace}.{key} takes another kind of value"),
            Self::NotUndoable(id) => write!(f, "change {id} was changed again or does not exist"),
            Self::Db(err) => write!(f, "settings database: {err}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<rusqlite::Error> for Error {
    fn from(err: rusqlite::Error) -> Self {
        Self::Db(err)
    }
}

#[derive(Debug, Clone, Default)]
pub struct Defaults(BTreeMap<String, Values>);

impl Defaults {
    #[must_use]
    pub fn load(dirs: &[PathBuf]) -> Self {
        let mut defaults = BTreeMap::new();
        for dir in dirs {
            let Ok(entries) = std::fs::read_dir(dir) else { continue };
            for path in entries.flatten().map(|entry| entry.path()) {
                let Some(namespace) = path.file_stem().and_then(|stem| stem.to_str()).map(str::to_owned) else {
                    continue;
                };
                if path.extension().is_none_or(|ext| ext != "json") || defaults.contains_key(&namespace) {
                    continue;
                }
                match std::fs::read_to_string(&path)
                    .map_err(|err| err.to_string())
                    .and_then(|text| serde_json::from_str::<Values>(&text).map_err(|err| err.to_string()))
                {
                    Ok(values) => {
                        defaults.insert(namespace, values);
                    }
                    Err(err) => tracing::warn!("skipping defaults {}: {err}", path.display()),
                }
            }
        }
        Self(defaults)
    }

    #[must_use]
    pub fn from_namespaces(namespaces: BTreeMap<String, Values>) -> Self {
        Self(namespaces)
    }

    fn namespace(&self, namespace: &str) -> Result<&Values, Error> {
        self.0.get(namespace).ok_or_else(|| Error::UnknownNamespace(namespace.to_owned()))
    }

    fn default_of(&self, namespace: &str, key: &str) -> Result<&Value, Error> {
        self.namespace(namespace)?
            .get(key)
            .ok_or_else(|| Error::UnknownKey { namespace: namespace.to_owned(), key: key.to_owned() })
    }
}

fn same_kind(default: &Value, value: &Value) -> bool {
    matches!(
        (default, value),
        (Value::Null, _)
            | (Value::Bool(_), Value::Bool(_))
            | (Value::Number(_), Value::Number(_))
            | (Value::String(_), Value::String(_))
            | (Value::Array(_), Value::Array(_))
            | (Value::Object(_), Value::Object(_))
    )
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Entry {
    pub id: i64,
    pub key: String,
    pub before: Option<Value>,
    pub after: Option<Value>,
    pub at: i64,
    pub origin: String,
}

pub struct Store {
    db: Connection,
    defaults: Defaults,
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX))
}

fn parse(text: Option<String>) -> Option<Value> {
    text.and_then(|text| serde_json::from_str(&text).ok())
}

impl Store {
    pub fn open(path: &Path, defaults: Defaults) -> Result<Self, Error> {
        let db = Connection::open(path)?;
        db.pragma_update(None, "journal_mode", "WAL")?;
        Self::with(db, defaults)
    }

    pub fn in_memory(defaults: Defaults) -> Result<Self, Error> {
        Self::with(Connection::open_in_memory()?, defaults)
    }

    fn with(db: Connection, defaults: Defaults) -> Result<Self, Error> {
        let version: i64 = db.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if version < SCHEMA_VERSION {
            db.execute_batch(
                "CREATE TABLE IF NOT EXISTS settings (
                     namespace TEXT NOT NULL, key TEXT NOT NULL, value TEXT NOT NULL,
                     PRIMARY KEY (namespace, key));
                 CREATE TABLE IF NOT EXISTS history (
                     id INTEGER PRIMARY KEY AUTOINCREMENT, namespace TEXT NOT NULL, key TEXT NOT NULL,
                     before TEXT, after TEXT, at INTEGER NOT NULL, origin TEXT NOT NULL);
                 CREATE INDEX IF NOT EXISTS history_by_namespace ON history (namespace, id);",
            )?;
            db.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        }
        Ok(Self { db, defaults })
    }

    pub fn reload_defaults(&mut self, defaults: Defaults) {
        self.defaults = defaults;
    }

    fn stored(&self, namespace: &str, key: &str) -> Result<Option<Value>, Error> {
        let text = self
            .db
            .query_row("SELECT value FROM settings WHERE namespace = ?1 AND key = ?2", params![namespace, key], |row| {
                row.get::<_, String>(0)
            })
            .optional()?;
        Ok(parse(text))
    }

    pub fn get(&self, namespace: &str, key: &str) -> Result<Value, Error> {
        let default = self.defaults.default_of(namespace, key)?;
        Ok(self.stored(namespace, key)?.unwrap_or_else(|| default.clone()))
    }

    pub fn get_all(&self, namespace: &str) -> Result<Values, Error> {
        let mut values = self.defaults.namespace(namespace)?.clone();
        let mut statement = self.db.prepare("SELECT key, value FROM settings WHERE namespace = ?1")?;
        let rows = statement.query_map(params![namespace], |row| Ok((row.get::<_, String>(0)?, row.get(1)?)))?;
        for row in rows {
            let (key, text) = row?;
            if let (Some(slot), Some(value)) = (values.get_mut(&key), parse(Some(text))) {
                *slot = value;
            }
        }
        Ok(values)
    }

    pub fn set_many(&mut self, namespace: &str, values: Values, origin: &str) -> Result<Values, Error> {
        for (key, value) in &values {
            if !same_kind(self.defaults.default_of(namespace, key)?, value) {
                return Err(Error::WrongType { namespace: namespace.to_owned(), key: key.clone() });
            }
        }
        let tx = self.db.transaction()?;
        let mut changed = Values::new();
        for (key, value) in values {
            let default = self.defaults.default_of(namespace, &key)?;
            let before = parse(
                tx.query_row(
                    "SELECT value FROM settings WHERE namespace = ?1 AND key = ?2",
                    params![namespace, key],
                    |row| row.get::<_, String>(0),
                )
                .optional()?,
            );
            if before.as_ref().unwrap_or(default) == &value {
                continue;
            }
            let after = (&value != default).then(|| value.to_string());
            match &after {
                Some(text) => tx.execute(
                    "INSERT INTO settings (namespace, key, value) VALUES (?1, ?2, ?3)
                     ON CONFLICT (namespace, key) DO UPDATE SET value = excluded.value",
                    params![namespace, key, text],
                )?,
                None => {
                    tx.execute("DELETE FROM settings WHERE namespace = ?1 AND key = ?2", params![namespace, key])?
                }
            };
            tx.execute(
                "INSERT INTO history (namespace, key, before, after, at, origin) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![namespace, key, before.map(|value| value.to_string()), after, now(), origin],
            )?;
            changed.insert(key, value);
        }
        tx.commit()?;
        Ok(changed)
    }

    pub fn reset(&mut self, namespace: &str, keys: &[String], origin: &str) -> Result<Values, Error> {
        let defaults = keys
            .iter()
            .map(|key| Ok((key.clone(), self.defaults.default_of(namespace, key)?.clone())))
            .collect::<Result<Values, Error>>()?;
        self.set_many(namespace, defaults, origin)
    }

    pub fn history(&self, namespace: &str, limit: u32) -> Result<Vec<Entry>, Error> {
        let mut statement = self.db.prepare(
            "SELECT id, key, before, after, at, origin FROM history WHERE namespace = ?1 ORDER BY id DESC LIMIT ?2",
        )?;
        let rows = statement.query_map(params![namespace, limit], |row| {
            Ok(Entry {
                id: row.get(0)?,
                key: row.get(1)?,
                before: parse(row.get(2)?),
                after: parse(row.get(3)?),
                at: row.get(4)?,
                origin: row.get(5)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn change(&self, id: i64) -> Result<Option<(String, Entry)>, Error> {
        Ok(self
            .db
            .query_row(
                "SELECT namespace, key, before, after, at, origin FROM history WHERE id = ?1",
                params![id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        Entry {
                            id,
                            key: row.get(1)?,
                            before: parse(row.get(2)?),
                            after: parse(row.get(3)?),
                            at: row.get(4)?,
                            origin: row.get(5)?,
                        },
                    ))
                },
            )
            .optional()?)
    }

    pub fn undo(&mut self, id: i64, origin: &str) -> Result<(String, Values), Error> {
        let (namespace, entry) = self.change(id)?.ok_or(Error::NotUndoable(id))?;
        if self.stored(&namespace, &entry.key)? != entry.after {
            return Err(Error::NotUndoable(id));
        }
        let restored = match entry.before {
            Some(value) => value,
            None => self.defaults.default_of(&namespace, &entry.key)?.clone(),
        };
        let changed = self.set_many(&namespace, Values::from([(entry.key, restored)]), origin)?;
        Ok((namespace, changed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn store() -> Store {
        let shell = Values::from([
            ("topbar.accent".to_owned(), json!("blue")),
            ("topbar.tray".to_owned(), json!(true)),
            ("general.terminal".to_owned(), Value::Null),
            ("keybinds".to_owned(), json!([])),
        ]);
        Store::in_memory(Defaults::from_namespaces(BTreeMap::from([("shell".to_owned(), shell)]))).expect("store")
    }

    #[test]
    fn unset_keys_read_their_defaults_and_only_differences_are_stored() {
        let mut store = store();
        assert_eq!(store.get("shell", "topbar.accent").unwrap(), json!("blue"));
        let changed = store.set_many("shell", Values::from([("topbar.accent".into(), json!("amber"))]), "t").unwrap();
        assert_eq!(changed, Values::from([("topbar.accent".into(), json!("amber"))]));
        assert_eq!(store.get_all("shell").unwrap()["topbar.accent"], json!("amber"));
        store.set_many("shell", Values::from([("topbar.accent".into(), json!("blue"))]), "t").unwrap();
        let rows: i64 = store.db.query_row("SELECT count(*) FROM settings", [], |row| row.get(0)).unwrap();
        assert_eq!(rows, 0, "a value equal to its default is not kept");
    }

    #[test]
    fn unknown_keys_and_wrong_kinds_are_refused_and_nothing_is_written() {
        let mut store = store();
        let values = Values::from([("topbar.accent".into(), json!("amber")), ("topbar.nope".into(), json!(1))]);
        assert!(matches!(store.set_many("shell", values, "t"), Err(Error::UnknownKey { .. })));
        assert!(matches!(
            store.set_many("shell", Values::from([("topbar.tray".into(), json!("yes"))]), "t"),
            Err(Error::WrongType { .. })
        ));
        assert!(matches!(store.get_all("other"), Err(Error::UnknownNamespace(_))));
        assert_eq!(store.get("shell", "topbar.accent").unwrap(), json!("blue"), "the whole batch was refused");
    }

    #[test]
    fn a_null_default_takes_any_kind_and_arrays_stay_whole() {
        let mut store = store();
        store.set_many("shell", Values::from([("general.terminal".into(), json!("foot"))]), "t").unwrap();
        let binds = json!([{ "key": "Super+T", "action": "terminal" }]);
        store.set_many("shell", Values::from([("keybinds".into(), binds.clone())]), "t").unwrap();
        assert_eq!(store.get("shell", "general.terminal").unwrap(), json!("foot"));
        assert_eq!(store.get("shell", "keybinds").unwrap(), binds);
    }

    #[test]
    fn undo_restores_the_previous_value_once_and_only_while_it_still_stands() {
        let mut store = store();
        store.set_many("shell", Values::from([("topbar.accent".into(), json!("amber"))]), "t").unwrap();
        store.set_many("shell", Values::from([("topbar.accent".into(), json!("green"))]), "t").unwrap();
        let history = store.history("shell", 10).unwrap();
        assert_eq!(history.len(), 2);
        let (first, last) = (history[1].id, history[0].id);
        assert!(matches!(store.undo(first, "t"), Err(Error::NotUndoable(_))), "green replaced amber since");
        let (namespace, changed) = store.undo(last, "t").unwrap();
        assert_eq!((namespace.as_str(), changed["topbar.accent"].clone()), ("shell", json!("amber")));
        assert!(matches!(store.undo(last, "t"), Err(Error::NotUndoable(_))));
    }

    #[test]
    fn reset_returns_keys_to_their_defaults() {
        let mut store = store();
        store.set_many("shell", Values::from([("topbar.tray".into(), json!(false))]), "t").unwrap();
        let changed = store.reset("shell", &["topbar.tray".into()], "t").unwrap();
        assert_eq!(changed["topbar.tray"], json!(true));
        assert!(store.reset("shell", &["topbar.tray".into()], "t").unwrap().is_empty(), "already default");
    }

    #[test]
    fn a_reopened_database_keeps_its_values() {
        let dir = std::env::temp_dir().join(format!("glide-configd-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.db");
        let defaults = store().defaults;
        {
            let mut first = Store::open(&path, defaults.clone()).unwrap();
            first.set_many("shell", Values::from([("topbar.accent".into(), json!("amber"))]), "t").unwrap();
        }
        let second = Store::open(&path, defaults).unwrap();
        assert_eq!(second.get("shell", "topbar.accent").unwrap(), json!("amber"));
        let _ = std::fs::remove_dir_all(dir);
    }
}

use std::collections::HashMap;

use serde_json::Value;

use super::api::{ApiError, list, text};
use super::session::Session;

pub const NEW_PREFIX: &str = "new:";

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case")]
pub enum Pending {
    Save { id: String, body: Value, base: Option<String> },
    Trash { id: String },
    Restore { id: String },
    Purge { id: String },
}

impl Pending {
    #[must_use]
    pub fn id(&self) -> &str {
        match self {
            Self::Save { id, .. } | Self::Trash { id } | Self::Restore { id } | Self::Purge { id } => id,
        }
    }

    #[must_use]
    pub fn renamed(self, names: &HashMap<String, String>) -> Self {
        let rename = |id: String| names.get(&id).cloned().unwrap_or(id);
        match self {
            Self::Save { id, body, base } => Self::Save { id: rename(id), body, base },
            Self::Trash { id } => Self::Trash { id: rename(id) },
            Self::Restore { id } => Self::Restore { id: rename(id) },
            Self::Purge { id } => Self::Purge { id: rename(id) },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StoredClash {
    pub id: String,
    pub body: Value,
    pub server: Value,
}

#[must_use]
pub fn is_new(id: &str) -> bool {
    id.starts_with(NEW_PREFIX)
}

#[must_use]
pub fn normalize(queue: Vec<Pending>) -> Vec<Pending> {
    let purged_new: Vec<String> = queue
        .iter()
        .filter(|pending| matches!(pending, Pending::Purge { id } if is_new(id)))
        .map(|pending| pending.id().to_string())
        .collect();
    let mut out: Vec<Pending> = Vec::new();
    for pending in queue {
        if purged_new.iter().any(|id| id == pending.id()) {
            continue;
        }
        if let Pending::Save { id, body, base } = pending {
            let earlier = out.iter().position(|known| matches!(known, Pending::Save { id: other, .. } if *other == id));
            let base = match earlier.map(|index| out.remove(index)) {
                Some(Pending::Save { base: first, .. }) => first,
                _ => base,
            };
            out.push(Pending::Save { id, body, base });
        } else {
            out.push(pending);
        }
    }
    out
}

#[derive(Debug, Default)]
pub struct Flushed {
    pub kept: Vec<Pending>,
    pub clashes: Vec<StoredClash>,
    pub renamed: HashMap<String, String>,
    pub pushed: usize,
    pub offline: bool,
    pub failure: Option<String>,
}

pub fn push(session: &mut Session, pending: &Pending, now: u64) -> Result<Option<String>, ApiError> {
    let created = |item: Value| text(&item, "id").map(str::to_string);
    match pending {
        Pending::Save { id, body, .. } if is_new(id) => {
            session.call(now, |client, token| client.post("/ciphers", token, body)).map(created)
        }
        Pending::Save { id, body, .. } => {
            session.call(now, |client, token| client.put(&format!("/ciphers/{id}"), token, Some(body))).map(|_| None)
        }
        Pending::Trash { id } => {
            session.call(now, |client, token| client.put(&format!("/ciphers/{id}/delete"), token, None)).map(|_| None)
        }
        Pending::Restore { id } => {
            session.call(now, |client, token| client.put(&format!("/ciphers/{id}/restore"), token, None)).map(|_| None)
        }
        Pending::Purge { id } => {
            session.call(now, |client, token| client.delete(&format!("/ciphers/{id}"), token)).map(|_| None)
        }
    }
}

pub fn flush(session: &mut Session, queue: Vec<Pending>, server: &Value, now: u64) -> Flushed {
    let revisions: HashMap<&str, &Value> =
        list(server, "ciphers").iter().filter_map(|item| Some((text(item, "id")?, item))).collect();
    let mut flushed = Flushed::default();
    let mut refused: Vec<String> = Vec::new();
    for pending in normalize(queue) {
        let pending = pending.renamed(&flushed.renamed);
        if refused.iter().any(|id| id == pending.id()) {
            continue;
        }
        if flushed.offline || (is_new(pending.id()) && !matches!(pending, Pending::Save { .. })) {
            flushed.kept.push(pending);
            continue;
        }
        if let Pending::Save { id, body, base: Some(base) } = &pending
            && let Some(current) = revisions.get(id.as_str())
            && text(current, "revisionDate") != Some(base.as_str())
        {
            flushed.clashes.push(StoredClash { id: id.clone(), body: body.clone(), server: (*current).clone() });
            continue;
        }
        match push(session, &pending, now) {
            Ok(created) => {
                flushed.pushed += 1;
                if let Some(created) = created {
                    flushed.renamed.insert(pending.id().to_string(), created);
                }
            }
            Err(ApiError::Offline(reason)) => {
                eprintln!("glide-keychain: Bitwarden is offline ({reason})");
                flushed.offline = true;
                flushed.kept.push(pending);
            }
            Err(error) => {
                eprintln!("glide-keychain: a queued Bitwarden change was refused ({error})");
                if is_new(pending.id()) {
                    refused.push(pending.id().to_string());
                }
                flushed.failure = Some(error.to_string());
            }
        }
    }
    flushed
}

pub fn fetch(session: &mut Session, now: u64) -> Result<Value, ApiError> {
    session.call(now, |client, token| client.get("/sync?excludeDomains=true", token))
}

#[derive(Debug)]
pub struct SyncJob {
    pub session: Session,
    pub queue: Vec<Pending>,
}

#[derive(Debug)]
pub struct SyncDone {
    pub session: Session,
    pub consumed: usize,
    pub flushed: Flushed,
    pub fresh: Result<Value, ApiError>,
}

impl SyncJob {
    #[must_use]
    pub fn run(self, now: u64) -> SyncDone {
        let Self { mut session, queue } = self;
        let consumed = queue.len();
        let fresh = match fetch(&mut session, now) {
            Ok(fresh) => fresh,
            Err(error) => return SyncDone { session, consumed: 0, flushed: Flushed::default(), fresh: Err(error) },
        };
        let flushed = flush(&mut session, queue, &fresh, now);
        let fresh = if flushed.pushed > 0 { fetch(&mut session, now).unwrap_or(fresh) } else { fresh };
        SyncDone { session, consumed, flushed, fresh: Ok(fresh) }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn save(id: &str, base: Option<&str>, tag: u8) -> Pending {
        Pending::Save { id: id.into(), body: json!({ "tag": tag }), base: base.map(str::to_string) }
    }

    #[test]
    fn later_saves_replace_earlier_ones_but_keep_the_first_base() {
        let queue = vec![save("a", Some("r1"), 1), Pending::Trash { id: "b".into() }, save("a", Some("r2"), 2)];
        assert_eq!(normalize(queue), vec![Pending::Trash { id: "b".into() }, save("a", Some("r1"), 2)]);
    }

    #[test]
    fn a_new_item_purged_before_it_reached_the_server_leaves_nothing_behind() {
        let queue =
            vec![save("new:x", None, 1), Pending::Trash { id: "new:x".into() }, Pending::Purge { id: "new:x".into() }];
        assert!(normalize(queue).is_empty());
    }

    #[test]
    fn ids_follow_a_new_item_once_the_server_names_it() {
        let names = HashMap::from([("new:x".to_string(), "c9".to_string())]);
        assert_eq!(Pending::Trash { id: "new:x".into() }.renamed(&names), Pending::Trash { id: "c9".into() });
        assert_eq!(Pending::Trash { id: "c1".into() }.renamed(&names), Pending::Trash { id: "c1".into() });
    }
}

use std::collections::BTreeMap;
use std::path::PathBuf;

use glide_ipc::keychain::{Candidate, Hidden, Request, Response};

use crate::vault::{self, Contents, Error, Item, Key, Params, Secret};

pub const PROGRAM_ATTRIBUTE: &str = "glide:program";
pub const APP_ATTRIBUTE: &str = "glide:app";
const USERNAME_ATTRIBUTES: [&str; 3] = ["username", "username_value", "user"];

#[must_use]
pub fn default_path() -> Option<PathBuf> {
    glide_config::data_home().map(|dir| dir.join("glide").join("keychain"))
}

#[derive(Debug)]
pub struct Daemon {
    path: PathBuf,
    params: Params,
    open: Option<(Contents, Key)>,
}

impl Daemon {
    #[must_use]
    pub const fn new(path: PathBuf, params: Params) -> Self {
        Self { path, params, open: None }
    }

    pub fn handle(&mut self, request: Request) -> Response {
        match request {
            Request::Status => Response::Status { exists: self.path.exists(), unlocked: self.open.is_some() },
            Request::Create { password } => self.create(&password),
            Request::Unlock { password } => self.unlock(&password),
            Request::Lock => {
                self.open = None;
                Response::Done
            }
            Request::Candidates { program, app_id } => match &self.open {
                Some((contents, _)) => {
                    Response::Candidates { list: candidates(contents, program.as_deref(), app_id.as_deref()) }
                }
                None => Response::Locked,
            },
            Request::Secret { id } => match self.open.as_ref().map(|(contents, _)| contents.get(id)) {
                Some(Some(item)) => {
                    Response::Secret { secret: Hidden::new(String::from_utf8_lossy(item.secret.expose()).into_owned()) }
                }
                Some(None) => Response::Error { message: "no such item".into() },
                None => Response::Locked,
            },
        }
    }

    fn create(&mut self, password: &Hidden) -> Response {
        if self.path.exists() {
            return Response::Error { message: "a keychain already exists".into() };
        }
        let key = match Key::derive(password.expose().as_bytes(), self.params) {
            Ok(key) => key,
            Err(err) => return Response::Error { message: err.to_string() },
        };
        self.open = Some((Contents::default(), key));
        self.save()
    }

    fn unlock(&mut self, password: &Hidden) -> Response {
        let file = match std::fs::read(&self.path) {
            Ok(file) => file,
            Err(err) => return Response::Error { message: err.to_string() },
        };
        match vault::open(&file, password.expose().as_bytes()) {
            Ok(opened) => {
                self.open = Some(opened);
                Response::Done
            }
            Err(Error::WrongPassword) => Response::WrongPassword,
            Err(err) => Response::Error { message: err.to_string() },
        }
    }

    fn save(&self) -> Response {
        let Some((contents, key)) = &self.open else {
            return Response::Locked;
        };
        let sealed = match vault::seal(contents, key) {
            Ok(sealed) => sealed,
            Err(err) => return Response::Error { message: err.to_string() },
        };
        match vault::write_atomically(&self.path, &sealed) {
            Ok(()) => Response::Done,
            Err(err) => Response::Error { message: err.to_string() },
        }
    }

    pub fn edit(&mut self, change: impl FnOnce(&mut Contents)) -> Response {
        let Some((contents, _)) = &mut self.open else {
            return Response::Locked;
        };
        change(contents);
        self.save()
    }

    #[must_use]
    pub fn contents(&self) -> Option<&Contents> {
        self.open.as_ref().map(|(contents, _)| contents)
    }

    pub fn add_login(&mut self, login: Login) -> Result<u64, Response> {
        let now = now();
        let mut id = 0;
        let item = Item {
            id: 0,
            collection: LOGIN_COLLECTION.into(),
            label: String::new(),
            attributes: BTreeMap::new(),
            secret: Secret::new(Vec::new()),
            content_type: "text/plain".into(),
            created: now,
            modified: now,
        };
        match self.edit(|contents| id = contents.add(login.apply(item, now))) {
            Response::Done => Ok(id),
            other => Err(other),
        }
    }

    pub fn update_login(&mut self, id: u64, login: Login) -> Response {
        if self.contents().is_some_and(|contents| contents.get(id).is_none()) {
            return Response::Error { message: "no such item".into() };
        }
        let now = now();
        self.edit(|contents| {
            if let Some(item) = contents.get_mut(id) {
                *item = login.apply(item.clone(), now);
            }
        })
    }

    pub fn delete(&mut self, id: u64) -> Response {
        if self.contents().is_some_and(|contents| contents.get(id).is_none()) {
            return Response::Error { message: "no such item".into() };
        }
        self.edit(|contents| {
            contents.remove(id);
        })
    }
}

pub const LOGIN_COLLECTION: &str = "login";

#[derive(Debug, Clone)]
pub struct Login {
    pub label: String,
    pub username: String,
    pub password: Hidden,
    pub app: String,
    pub program: String,
}

impl Login {
    fn apply(self, mut item: Item, now: u64) -> Item {
        item.label = self.label;
        for (key, value) in [("username", self.username), (APP_ATTRIBUTE, self.app), (PROGRAM_ATTRIBUTE, self.program)]
        {
            if value.is_empty() {
                item.attributes.remove(key);
            } else {
                item.attributes.insert(key.to_string(), value);
            }
        }
        if !self.password.expose().is_empty() {
            item.secret = Secret::new(self.password.expose().as_bytes().to_vec());
        }
        item.modified = now;
        item
    }
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.as_secs())
}

#[must_use]
pub fn username(item: &Item) -> String {
    USERNAME_ATTRIBUTES.iter().find_map(|key| item.attributes.get(*key)).cloned().unwrap_or_default()
}

#[must_use]
pub fn candidates(contents: &Contents, program: Option<&str>, app_id: Option<&str>) -> Vec<Candidate> {
    let mut found: Vec<&Item> = contents
        .items
        .iter()
        .filter(|item| {
            let same = |key: &str, wanted: Option<&str>| {
                wanted.filter(|wanted| !wanted.is_empty()).is_some_and(|wanted| {
                    item.attributes.get(key).is_some_and(|value| value.eq_ignore_ascii_case(wanted))
                })
            };
            same(PROGRAM_ATTRIBUTE, program) || same(APP_ATTRIBUTE, app_id)
        })
        .collect();
    found.sort_by(|a, b| b.modified.cmp(&a.modified).then(a.id.cmp(&b.id)));
    found
        .into_iter()
        .map(|item| Candidate { id: item.id, label: item.label.clone(), username: username(item) })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FAST: Params = Params { memory_kib: 64, passes: 1, lanes: 1 };

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("glide-keychain-daemon-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("keychain")
    }

    fn hidden(text: &str) -> Hidden {
        Hidden::new(text.into())
    }

    fn login(label: &str, attributes: &[(&str, &str)], modified: u64) -> Item {
        Item {
            id: 0,
            collection: "login".into(),
            label: label.into(),
            attributes: attributes.iter().map(|(key, value)| ((*key).to_string(), (*value).to_string())).collect(),
            secret: Secret::new(b"s3cret".to_vec()),
            content_type: "text/plain".into(),
            created: modified,
            modified,
        }
    }

    #[test]
    fn a_new_keychain_is_created_locked_and_unlocked_again() {
        let path = temp("cycle");
        let mut daemon = Daemon::new(path.clone(), FAST);
        assert_eq!(daemon.handle(Request::Status), Response::Status { exists: false, unlocked: false });
        assert_eq!(daemon.handle(Request::Create { password: hidden("pw") }), Response::Done);
        assert_eq!(daemon.handle(Request::Status), Response::Status { exists: true, unlocked: true });
        assert!(matches!(daemon.handle(Request::Create { password: hidden("pw") }), Response::Error { .. }));
        assert_eq!(daemon.handle(Request::Lock), Response::Done);
        assert_eq!(daemon.handle(Request::Candidates { program: None, app_id: None }), Response::Locked);
        assert_eq!(daemon.handle(Request::Unlock { password: hidden("nope") }), Response::WrongPassword);
        assert_eq!(daemon.handle(Request::Unlock { password: hidden("pw") }), Response::Done);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn edits_survive_a_restart_of_the_daemon() {
        let path = temp("persist");
        let mut daemon = Daemon::new(path.clone(), FAST);
        daemon.handle(Request::Create { password: hidden("pw") });
        let mut id = 0;
        assert_eq!(
            daemon.edit(|contents| id = contents.add(login("Telegram", &[(APP_ATTRIBUTE, "org.telegram.desktop")], 1))),
            Response::Done
        );
        let mut again = Daemon::new(path.clone(), FAST);
        again.handle(Request::Unlock { password: hidden("pw") });
        assert_eq!(again.handle(Request::Secret { id }), Response::Secret { secret: hidden("s3cret") });
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn candidates_match_the_program_or_the_app_id_newest_first() {
        let mut contents = Contents::default();
        contents.add(login("old", &[(APP_ATTRIBUTE, "org.Telegram.desktop"), ("username", "me")], 1));
        contents.add(login("new", &[(PROGRAM_ATTRIBUTE, "/usr/bin/telegram-desktop")], 5));
        contents.add(login("other", &[(APP_ATTRIBUTE, "firefox")], 9));
        let found = candidates(&contents, Some("/usr/bin/telegram-desktop"), Some("org.telegram.desktop"));
        assert_eq!(found.iter().map(|c| c.label.as_str()).collect::<Vec<_>>(), ["new", "old"]);
        assert_eq!(found[1].username, "me");
        assert!(candidates(&contents, None, Some("")).is_empty(), "an empty app id matches nothing");
    }

    #[test]
    fn a_locked_daemon_reveals_nothing() {
        let mut daemon = Daemon::new(temp("locked"), FAST);
        assert_eq!(daemon.handle(Request::Secret { id: 1 }), Response::Locked);
        assert_eq!(daemon.edit(|_| {}), Response::Locked);
        assert!(daemon.contents().is_none());
    }

    fn entry(label: &str, password: &str) -> Login {
        Login {
            label: label.into(),
            username: "me".into(),
            password: hidden(password),
            app: "org.telegram.desktop".into(),
            program: String::new(),
        }
    }

    #[test]
    fn a_login_is_added_updated_and_deleted() {
        let path = temp("logins");
        let mut daemon = Daemon::new(path.clone(), FAST);
        daemon.handle(Request::Create { password: hidden("pw") });
        let id = daemon.add_login(entry("Telegram", "first")).unwrap();
        let found = candidates(daemon.contents().unwrap(), None, Some("org.telegram.desktop"));
        assert_eq!(found, [Candidate { id, label: "Telegram".into(), username: "me".into() }]);
        assert_eq!(daemon.update_login(id, entry("Telegram Desktop", "")), Response::Done);
        assert_eq!(
            daemon.handle(Request::Secret { id }),
            Response::Secret { secret: hidden("first") },
            "an empty password keeps the saved one"
        );
        assert_eq!(daemon.contents().unwrap().get(id).unwrap().label, "Telegram Desktop");
        assert!(!daemon.contents().unwrap().get(id).unwrap().attributes.contains_key(PROGRAM_ATTRIBUTE));
        assert_eq!(daemon.delete(id), Response::Done);
        assert!(matches!(daemon.delete(id), Response::Error { .. }));
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}

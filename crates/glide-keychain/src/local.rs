use std::path::PathBuf;

use glide_ipc::keychain::{AppLink, Draft, Hidden, ItemKind, LOCAL, SyncState, Vault, VaultKind};

use crate::provider::{Fault, Provider, Record};
use crate::vault::{self, Contents, Item, Key, Params};

pub(crate) const PROGRAM_ATTRIBUTE: &str = "glide:program";
pub(crate) const APP_ATTRIBUTE: &str = "glide:app";
const USERNAME_ATTRIBUTES: [&str; 3] = ["username", "username_value", "user"];

pub struct Local {
    path: PathBuf,
    params: Params,
    name: String,
    open: Option<Open>,
}

struct Open {
    key: Key,
    records: Vec<Record>,
    folders: Vec<String>,
    next_id: u64,
    services: Vec<Item>,
}

pub const SERVICE_COLLECTION: &str = "secret-service";

const fn collection(kind: ItemKind) -> &'static str {
    match kind {
        ItemKind::Login => "login",
        ItemKind::Note => "note",
        ItemKind::Card => "card",
        ItemKind::Identity => "identity",
    }
}

fn kind_of(collection: &str) -> ItemKind {
    match collection {
        "note" => ItemKind::Note,
        "card" => ItemKind::Card,
        "identity" => ItemKind::Identity,
        _ => ItemKind::Login,
    }
}

#[must_use]
pub fn username(item: &Item) -> String {
    USERNAME_ATTRIBUTES.iter().find_map(|key| item.attributes.get(*key)).cloned().unwrap_or_default()
}

fn record(item: &Item) -> Record {
    let mut apps = item.apps.clone();
    if apps.is_empty() {
        let app_id = item.attributes.get(APP_ATTRIBUTE).cloned().unwrap_or_default();
        let program = item.attributes.get(PROGRAM_ATTRIBUTE).cloned().unwrap_or_default();
        if !app_id.is_empty() || !program.is_empty() {
            apps.push(AppLink { name: String::new(), app_id, program });
        }
    }
    Record {
        id: item.id.to_string(),
        kind: kind_of(&item.collection),
        username: username(item),
        name: item.label.clone(),
        password: item.secret.clone(),
        totp: item.totp.clone(),
        notes: item.notes.clone(),
        sites: item.sites.clone(),
        apps,
        fields: item.fields.clone(),
        folder: item.folder.clone(),
        favorite: item.favorite,
        created: item.created,
        modified: item.modified,
        deleted: item.deleted,
        history: item.history.clone(),
        card: item.card.clone(),
        passkeys: Vec::new(),
    }
}

fn item(record: &Record) -> Item {
    let mut attributes = std::collections::BTreeMap::new();
    if !record.username.is_empty() {
        attributes.insert("username".into(), record.username.clone());
    }
    Item {
        id: record.id.parse().unwrap_or_default(),
        attributes,
        secret: record.password.clone(),
        modified: record.modified,
        totp: record.totp.clone(),
        notes: record.notes.clone(),
        sites: record.sites.clone(),
        apps: record.apps.clone(),
        fields: record.fields.clone(),
        folder: record.folder.clone(),
        favorite: record.favorite,
        deleted: record.deleted,
        history: record.history.clone(),
        card: record.card.clone(),
        ..Item::new(collection(record.kind), &record.name, record.created)
    }
}

impl Local {
    #[must_use]
    pub fn new(path: PathBuf, params: Params, name: &str) -> Self {
        Self { path, params, name: name.to_string(), open: None }
    }

    fn opened(&self) -> Result<&Open, Fault> {
        self.open.as_ref().ok_or(Fault::Locked)
    }

    fn opened_mut(&mut self) -> Result<&mut Open, Fault> {
        self.open.as_mut().ok_or(Fault::Locked)
    }

    fn save_file(&self) -> Result<(), Fault> {
        let open = self.opened()?;
        let contents = Contents {
            items: open.records.iter().map(item).chain(open.services.iter().cloned()).collect(),
            next_id: open.next_id,
            folders: open.folders.clone(),
        };
        let sealed = vault::seal(&contents, &open.key)?;
        vault::write_atomically(&self.path, &sealed).map_err(|err| Fault::Failed(err.to_string()))
    }

    fn find(&mut self, id: &str) -> Result<&mut Record, Fault> {
        self.opened_mut()?.records.iter_mut().find(|record| record.id == id).ok_or(Fault::NotFound)
    }

    fn adopt(&mut self, contents: &Contents, key: Key) {
        let mut folders = contents.folders.clone();
        let (services, logins): (Vec<&Item>, Vec<&Item>) =
            contents.items.iter().partition(|item| item.collection == SERVICE_COLLECTION);
        let records: Vec<Record> = logins.into_iter().map(record).collect();
        for record in &records {
            if !record.folder.is_empty() && !folders.contains(&record.folder) {
                folders.push(record.folder.clone());
            }
        }
        let services = services.into_iter().cloned().collect();
        self.open = Some(Open { key, records, folders, next_id: contents.next_id, services });
    }
}

impl Provider for Local {
    fn vault(&self) -> Vault {
        Vault {
            id: LOCAL.into(),
            name: self.name.clone(),
            kind: VaultKind::Local,
            account: String::new(),
            server: String::new(),
            exists: self.exists(),
            unlocked: self.open.is_some(),
            sync: SyncState::Local,
            count: self
                .open
                .as_ref()
                .map_or(0, |open| open.records.iter().filter(|record| record.deleted.is_none()).count() as u32),
        }
    }

    fn exists(&self) -> bool {
        self.path.exists()
    }

    fn unlocked(&self) -> bool {
        self.open.is_some()
    }

    fn create(&mut self, password: &Hidden) -> Result<(), Fault> {
        if self.exists() {
            return Err(Fault::Failed("a keychain already exists".into()));
        }
        let key = Key::derive(password.expose().as_bytes(), self.params)?;
        self.adopt(&Contents::default(), key);
        self.save_file()
    }

    fn unlock(&mut self, password: &Hidden) -> Result<(), Fault> {
        let file = std::fs::read(&self.path).map_err(|err| Fault::Failed(err.to_string()))?;
        let (contents, key) = vault::open(&file, password.expose().as_bytes())?;
        self.adopt(&contents, key);
        Ok(())
    }

    fn lock(&mut self) {
        self.open = None;
    }

    fn change_password(&mut self, old: &Hidden, new: &Hidden) -> Result<(), Fault> {
        let file = std::fs::read(&self.path).map_err(|err| Fault::Failed(err.to_string()))?;
        let (contents, _) = vault::open(&file, old.expose().as_bytes())?;
        let key = Key::derive(new.expose().as_bytes(), self.params)?;
        let previous = self.open.take();
        self.adopt(&contents, key);
        let saved = self.save_file();
        if saved.is_err() {
            self.open = previous;
        }
        saved
    }

    fn records(&self) -> Result<&[Record], Fault> {
        Ok(&self.opened()?.records)
    }

    fn folders(&self) -> Result<Vec<String>, Fault> {
        Ok(self.opened()?.folders.clone())
    }

    fn save(&mut self, draft: &Draft, now: u64) -> Result<String, Fault> {
        let id = if let Some(existing) = &draft.item {
            self.find(&existing.id)?.apply(draft, now);
            existing.id.clone()
        } else {
            let open = self.opened_mut()?;
            open.next_id += 1;
            let mut record = Record { id: open.next_id.to_string(), created: now, ..Record::default() };
            record.apply(draft, now);
            open.records.push(record);
            open.next_id.to_string()
        };
        let open = self.opened_mut()?;
        if !draft.folder.trim().is_empty() && !open.folders.iter().any(|folder| folder == draft.folder.trim()) {
            open.folders.push(draft.folder.trim().to_string());
        }
        self.save_file()?;
        Ok(id)
    }

    fn trash(&mut self, id: &str, now: u64) -> Result<(), Fault> {
        self.find(id)?.deleted = Some(now);
        self.save_file()
    }

    fn restore(&mut self, id: &str) -> Result<(), Fault> {
        self.find(id)?.deleted = None;
        self.save_file()
    }

    fn purge(&mut self, id: &str) -> Result<(), Fault> {
        let open = self.opened_mut()?;
        let before = open.records.len();
        open.records.retain(|record| record.id != id);
        if open.records.len() == before {
            return Err(Fault::NotFound);
        }
        self.save_file()
    }

    fn rename(&mut self, name: &str) {
        self.name = name.to_string();
    }

    fn app_secrets(&self) -> Result<&[Item], Fault> {
        Ok(&self.opened()?.services)
    }

    fn edit_app_secrets(&mut self, edit: &mut dyn FnMut(&mut Vec<Item>, &mut u64)) -> Result<(), Fault> {
        let open = self.opened_mut()?;
        edit(&mut open.services, &mut open.next_id);
        for service in &mut open.services {
            service.collection = SERVICE_COLLECTION.to_string();
        }
        self.save_file()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glide_ipc::keychain::ItemRef;

    const FAST: Params = Params { memory_kib: 64, passes: 1, lanes: 1 };

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("glide-keychain-local-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("keychain")
    }

    fn hidden(text: &str) -> Hidden {
        Hidden::new(text.into())
    }

    fn draft(name: &str, password: &str) -> Draft {
        Draft {
            vault: LOCAL.into(),
            name: name.into(),
            username: "me".into(),
            password: Some(hidden(password)),
            apps: vec![AppLink {
                name: "Telegram".into(),
                app_id: "org.telegram.desktop".into(),
                program: String::new(),
            }],
            folder: "Робота".into(),
            ..Draft::default()
        }
    }

    #[test]
    fn items_survive_locking_and_keep_their_history() {
        let path = temp("cycle");
        let mut local = Local::new(path.clone(), FAST, "Особисте");
        local.create(&hidden("pw")).unwrap();
        let id = local.save(&draft("Telegram", "first"), 10).unwrap();
        let mut again = draft("Telegram", "second");
        again.item = Some(ItemRef::new(LOCAL, id));
        local.save(&again, 20).unwrap();
        local.lock();
        assert_eq!(local.records().unwrap_err(), Fault::Locked);
        assert_eq!(local.unlock(&hidden("nope")).unwrap_err(), Fault::WrongPassword);
        local.unlock(&hidden("pw")).unwrap();
        let record = &local.records().unwrap()[0];
        assert_eq!((record.password_text().as_str(), record.history.len()), ("second", 1));
        assert_eq!(record.apps[0].app_id, "org.telegram.desktop");
        assert_eq!(local.folders().unwrap(), ["Робота"]);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn a_trashed_item_comes_back_and_a_purged_one_does_not() {
        let path = temp("trash");
        let mut local = Local::new(path.clone(), FAST, "Особисте");
        local.create(&hidden("pw")).unwrap();
        let id = local.save(&draft("x", "y"), 1).unwrap();
        local.trash(&id, 5).unwrap();
        assert_eq!(local.records().unwrap()[0].deleted, Some(5));
        local.restore(&id).unwrap();
        assert_eq!(local.records().unwrap()[0].deleted, None);
        local.purge(&id).unwrap();
        assert!(local.records().unwrap().is_empty());
        assert_eq!(local.purge(&id).unwrap_err(), Fault::NotFound);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn old_app_attributes_become_app_links() {
        let mut old = Item::new("login", "Telegram", 1);
        old.attributes.insert(APP_ATTRIBUTE.into(), "org.telegram.desktop".into());
        old.attributes.insert("username_value".into(), "me".into());
        let converted = record(&old);
        assert_eq!(converted.apps[0].app_id, "org.telegram.desktop");
        assert_eq!(converted.username, "me");
    }

    #[test]
    fn the_password_can_be_changed() {
        let path = temp("change");
        let mut local = Local::new(path.clone(), FAST, "Особисте");
        local.create(&hidden("old")).unwrap();
        local.save(&draft("x", "y"), 1).unwrap();
        assert_eq!(local.change_password(&hidden("wrong"), &hidden("new")).unwrap_err(), Fault::WrongPassword);
        local.change_password(&hidden("old"), &hidden("new")).unwrap();
        local.lock();
        local.unlock(&hidden("new")).unwrap();
        assert_eq!(local.records().unwrap().len(), 1);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}

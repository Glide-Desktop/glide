use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use glide_ipc::keychain::{Draft, Hidden, SyncState, Vault, VaultKind};
use keepass::config::DatabaseVersion;
use keepass::db::{EntryId, GroupId, Times};
use keepass::{Database, DatabaseKey};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use super::{FOLDER_SEPARATOR, folders, records, write};
use crate::provider::{Fault, Provider, Record};
use crate::transfer::ImportError;

const BIN_NAME: &str = "Recycle Bin";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Registered {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Stamp {
    modified: Option<SystemTime>,
    len: u64,
}

fn stamp(path: &Path) -> Option<Stamp> {
    let meta = std::fs::metadata(path).ok()?;
    Some(Stamp { modified: meta.modified().ok(), len: meta.len() })
}

struct Open {
    database: Database,
    password: Zeroizing<String>,
    stamp: Option<Stamp>,
    records: Vec<Record>,
    folders: Vec<String>,
}

impl Open {
    fn writable(&self) -> bool {
        matches!(self.database.config.version, DatabaseVersion::KDB4(_))
    }

    fn adopt(&mut self, database: Database, stamp: Option<Stamp>) {
        self.records = records(&database);
        self.folders = folders(&database);
        self.database = database;
        self.stamp = stamp;
    }
}

pub struct KeePass {
    id: String,
    name: String,
    path: PathBuf,
    open: Option<Open>,
}

impl std::fmt::Debug for KeePass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeePass").field("id", &self.id).field("open", &self.open.is_some()).finish_non_exhaustive()
    }
}

fn fault(error: ImportError) -> Fault {
    match error {
        ImportError::WrongPassword => Fault::WrongPassword,
        other => Fault::Failed(other.to_string()),
    }
}

fn read(path: &Path, password: &str) -> Result<(Database, Option<Stamp>), Fault> {
    let before = stamp(path);
    let bytes = Zeroizing::new(std::fs::read(path).map_err(|err| Fault::Failed(err.to_string()))?);
    let database = super::open(&bytes, password).map_err(fault)?;
    Ok((database, before))
}

fn replace(path: &Path, bytes: &[u8]) -> Result<(), Fault> {
    let failed = |err: std::io::Error| Fault::Failed(format!("the KeePass file was not saved: {err}"));
    let dir = path.parent().filter(|dir| !dir.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."));
    let name = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    let temp = dir.join(format!(".{name}.glide-{}", std::process::id()));
    let permissions = std::fs::metadata(path).map(|meta| meta.permissions()).map_err(failed)?;
    let written = (|| {
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&temp)?;
        file.write_all(bytes)?;
        file.set_permissions(permissions)?;
        file.sync_all()?;
        std::fs::rename(&temp, path)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    written.map_err(failed)
}

fn find(database: &Database, id: &str) -> Result<EntryId, Fault> {
    database
        .iter_all_entries()
        .find(|entry| entry.id().to_string() == id)
        .map(|entry| entry.id())
        .ok_or(Fault::NotFound)
}

fn group_for(database: &mut Database, folder: &str) -> GroupId {
    let bin = database.recycle_bin().map(|group| group.id());
    let mut current = database.root().id();
    for name in folder.split(['/', '\\']).map(str::trim).filter(|name| !name.is_empty()) {
        let existing = database.group(current).and_then(|group| {
            group.groups().find(|child| Some(child.id()) != bin && child.name.trim() == name).map(|child| child.id())
        });
        current = match existing {
            Some(id) => id,
            None => match database.group_mut(current) {
                Some(mut parent) => parent.add_group().edit(|group| group.name = name.to_string()).id(),
                None => current,
            },
        };
    }
    current
}

fn bin_of(database: &mut Database) -> GroupId {
    if let Some(bin) = database.recycle_bin() {
        return bin.id();
    }
    let bin = database.root_mut().add_group().edit(|group| group.name = BIN_NAME.to_string()).id();
    database.meta.recyclebin_uuid = Some(bin.uuid());
    database.meta.recyclebin_enabled = Some(true);
    bin
}

fn place(database: &mut Database, entry: EntryId, group: GroupId) -> Result<(), Fault> {
    let mut found = database.entry_mut(entry).ok_or(Fault::NotFound)?;
    if found.as_ref().parent().id() == group {
        return Ok(());
    }
    found.move_to(group).map_err(|_| Fault::NotFound)?;
    found.times.location_changed = Some(Times::now());
    Ok(())
}

impl KeePass {
    #[must_use]
    pub fn new(registered: Registered) -> Self {
        Self { id: registered.id, name: registered.name, path: registered.path, open: None }
    }

    #[must_use]
    pub fn registered(&self) -> Registered {
        Registered { id: self.id.clone(), name: self.name.clone(), path: self.path.clone() }
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    fn opened(&self) -> Result<&Open, Fault> {
        self.open.as_ref().ok_or(Fault::Locked)
    }

    fn refresh(&mut self) -> Result<(), Fault> {
        let path = self.path.clone();
        let open = self.open.as_mut().ok_or(Fault::Locked)?;
        if stamp(&path) == open.stamp {
            return Ok(());
        }
        let (database, at) = read(&path, &open.password)?;
        open.adopt(database, at);
        Ok(())
    }

    fn change<T>(&mut self, edit: impl FnOnce(&mut Database) -> Result<T, Fault>) -> Result<T, Fault> {
        self.refresh()?;
        let path = self.path.clone();
        let open = self.open.as_mut().ok_or(Fault::Locked)?;
        if !open.writable() {
            return Err(Fault::Failed(
                "this KeePass database is in the KDBX 3 format, which Glide only reads; save it as KDBX 4 in KeePassXC to edit it here".into(),
            ));
        }
        let mut database = open.database.clone();
        let result = edit(&mut database)?;
        let mut bytes = Zeroizing::new(Vec::new());
        database
            .save(&mut *bytes, DatabaseKey::new().with_password(&open.password))
            .map_err(|err| Fault::Failed(format!("the KeePass file was not saved: {err}")))?;
        replace(&path, &bytes)?;
        open.adopt(database, stamp(&path));
        Ok(result)
    }
}

impl Provider for KeePass {
    fn vault(&self) -> Vault {
        Vault {
            id: self.id.clone(),
            name: self.name.clone(),
            kind: VaultKind::KeePass,
            account: String::new(),
            server: self.path.display().to_string(),
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

    fn unlock(&mut self, password: &Hidden) -> Result<(), Fault> {
        let (database, at) = read(&self.path, password.expose())?;
        let mut open = Open {
            database: Database::new(),
            password: Zeroizing::new(password.expose().to_string()),
            stamp: None,
            records: Vec::new(),
            folders: Vec::new(),
        };
        open.adopt(database, at);
        self.open = Some(open);
        Ok(())
    }

    fn lock(&mut self) {
        self.open = None;
    }

    fn records(&self) -> Result<&[Record], Fault> {
        Ok(&self.opened()?.records)
    }

    fn folders(&self) -> Result<Vec<String>, Fault> {
        Ok(self.opened()?.folders.clone())
    }

    fn save(&mut self, draft: &Draft, _now: u64) -> Result<String, Fault> {
        self.change(|database| {
            let folder = draft.folder.split(FOLDER_SEPARATOR).collect::<Vec<_>>().join("/");
            let group = group_for(database, &folder);
            let Some(item) = &draft.item else {
                let mut parent = database.group_mut(group).ok_or(Fault::NotFound)?;
                let mut entry = parent.add_entry();
                write(&mut entry, draft);
                return Ok(entry.id().to_string());
            };
            let id = find(database, &item.id)?;
            let bin = database.recycle_bin().map(|bin| bin.id());
            let mut entry = database.entry_mut(id).ok_or(Fault::NotFound)?;
            entry.edit_tracking(|tracked| write(&mut tracked.as_mut(), draft));
            entry.times.last_modification = Some(Times::now());
            if Some(entry.as_ref().parent().id()) != bin {
                place(database, id, group)?;
            }
            Ok(item.id.clone())
        })
    }

    fn trash(&mut self, id: &str, _now: u64) -> Result<(), Fault> {
        self.change(|database| {
            let entry = find(database, id)?;
            let bin = bin_of(database);
            place(database, entry, bin)
        })
    }

    fn restore(&mut self, id: &str) -> Result<(), Fault> {
        self.change(|database| {
            let entry = find(database, id)?;
            let bin = database.recycle_bin().map(|bin| bin.id());
            let root = database.root().id();
            let back = database
                .entry(entry)
                .and_then(|found| found.previous_parent().map(|group| group.id()))
                .filter(|group| Some(*group) != bin)
                .unwrap_or(root);
            place(database, entry, back)
        })
    }

    fn purge(&mut self, id: &str) -> Result<(), Fault> {
        self.change(|database| {
            let entry = find(database, id)?;
            database.entry_mut(entry).ok_or(Fault::NotFound)?.remove();
            database.deleted_objects.insert(entry.uuid(), Some(Times::now()));
            Ok(())
        })
    }

    fn sync(&mut self) -> Result<(), Fault> {
        if self.open.is_some() { self.refresh() } else { Ok(()) }
    }

    fn rename(&mut self, name: &str) {
        if !name.trim().is_empty() {
            self.name = name.trim().to_string();
        }
    }

    fn forget(&mut self) -> Result<(), Fault> {
        self.open = None;
        Ok(())
    }

    fn registration(&self) -> Option<Registered> {
        Some(self.registered())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use glide_ipc::keychain::{AppLink, CardDraft, FieldDraft, ItemKind, ItemRef, Matching, Site};
    use keepass::db::fields;

    fn file(name: &str, password: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("glide-kdbx-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("Passwords.kdbx");
        sample(&path, password);
        path
    }

    fn vault(path: &Path) -> KeePass {
        KeePass::new(Registered { id: "keepass-1".into(), name: "KeePass".into(), path: path.to_path_buf() })
    }

    pub(crate) fn sample(path: &Path, password: &str) {
        let mut config = keepass::config::DatabaseConfig::default();
        config.kdf_config = keepass::config::KdfConfig::Aes { rounds: 10 };
        let mut database = Database::with_config(config);
        database.root_mut().add_group().edit(|group| group.name = "Work".into()).add_entry().edit(|entry| {
            entry.set_unprotected(fields::TITLE, "Jira");
            entry.set_unprotected(fields::USERNAME, "artem");
            entry.set_protected(fields::PASSWORD, "first");
        });
        let mut bytes = Vec::new();
        database.save(&mut bytes, DatabaseKey::new().with_password(password)).expect("save");
        std::fs::write(path, bytes).expect("write");
    }

    fn reopen(path: &Path, password: &str) -> Vec<Record> {
        records(&super::super::open(&std::fs::read(path).expect("read"), password).expect("open"))
    }

    fn login(name: &str, password: &str) -> Draft {
        Draft {
            kind: ItemKind::Login,
            name: name.into(),
            username: "artem".into(),
            password: Some(Hidden::new(password.into())),
            folder: "Work / Git".into(),
            sites: vec![
                Site { uri: "https://github.com".into(), matching: Matching::Domain },
                Site { uri: "https://gist.github.com".into(), matching: Matching::Domain },
            ],
            apps: vec![AppLink {
                app_id: "io.github.desktop".into(),
                program: String::new(),
                name: "GitHub Desktop".into(),
            }],
            fields: vec![FieldDraft { name: "Recovery".into(), value: Hidden::new("1234".into()), hidden: true }],
            totp: Some(Hidden::new("JBSWY3DPEHPK3PXP".into())),
            notes: "notes".into(),
            favorite: true,
            ..Draft::default()
        }
    }

    #[test]
    fn a_saved_login_is_written_into_the_file_and_read_back() {
        let path = file("save", "door");
        let mut keepass = vault(&path);
        assert_eq!(keepass.unlock(&Hidden::new("window".into())), Err(Fault::WrongPassword));
        keepass.unlock(&Hidden::new("door".into())).expect("unlock");
        assert_eq!(keepass.records().expect("records").len(), 1);
        let id = keepass.save(&login("GitHub", "hunter2"), 0).expect("save");

        let records = reopen(&path, "door");
        let github = records.iter().find(|record| record.id == id).expect("saved");
        assert_eq!((github.folder.as_str(), github.password_text().as_str()), ("Work / Git", "hunter2"));
        assert_eq!(github.sites.len(), 2);
        assert_eq!(github.apps.len(), 1);
        assert_eq!(github.apps[0].app_id, "io.github.desktop");
        assert_eq!((github.fields.len(), github.fields[0].hidden), (1, true));
        assert_eq!(String::from_utf8_lossy(github.totp.expose()), "JBSWY3DPEHPK3PXP");
        assert!(github.favorite);
        assert_eq!(keepass.folders().expect("folders"), ["Work", "Work / Git"]);
    }

    #[test]
    fn an_edit_keeps_the_old_password_in_the_entry_history() {
        let path = file("history", "door");
        let mut keepass = vault(&path);
        keepass.unlock(&Hidden::new("door".into())).expect("unlock");
        let id = keepass.records().expect("records")[0].id.clone();
        let mut edit = login("Jira", "second");
        edit.item = Some(ItemRef::new("keepass-1", &id));
        edit.folder = "Work".into();
        edit.fields.clear();
        keepass.save(&edit, 0).expect("edit");
        let jira = reopen(&path, "door").into_iter().find(|record| record.id == id).expect("jira");
        assert_eq!(jira.password_text(), "second");
        assert_eq!(
            jira.history
                .iter()
                .map(|past| String::from_utf8_lossy(past.secret.expose()).into_owned())
                .collect::<Vec<_>>(),
            ["first"]
        );
        assert!(jira.fields.is_empty());
    }

    #[test]
    fn trash_restore_and_purge_move_entries_through_the_recycle_bin() {
        let path = file("trash", "door");
        let mut keepass = vault(&path);
        keepass.unlock(&Hidden::new("door".into())).expect("unlock");
        let id = keepass.records().expect("records")[0].id.clone();
        keepass.trash(&id, 0).expect("trash");
        let trashed = reopen(&path, "door");
        assert!(trashed[0].deleted.is_some());
        keepass.restore(&id).expect("restore");
        let back = reopen(&path, "door");
        assert_eq!((back[0].deleted, back[0].folder.as_str()), (None, "Work"));
        keepass.trash(&id, 0).expect("trash again");
        keepass.purge(&id).expect("purge");
        assert!(reopen(&path, "door").is_empty());
        assert!(!keepass.folders().expect("folders").iter().any(|folder| folder.contains("Recycle")));
    }

    #[test]
    fn a_change_made_elsewhere_is_kept_when_glide_writes() {
        let path = file("outside", "door");
        let mut keepass = vault(&path);
        keepass.unlock(&Hidden::new("door".into())).expect("unlock");
        let bytes = std::fs::read(&path).expect("read");
        let mut outside = super::super::open(&bytes, "door").expect("open");
        outside.root_mut().add_entry().edit(|entry| {
            entry.set_unprotected(fields::TITLE, "Added in KeePassXC");
            entry.set_protected(fields::PASSWORD, "elsewhere");
        });
        let mut saved = Vec::new();
        outside.save(&mut saved, DatabaseKey::new().with_password("door")).expect("save");
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(&path, saved).expect("write");
        keepass.save(&login("GitHub", "hunter2"), 0).expect("save");
        let names: Vec<String> = reopen(&path, "door").into_iter().map(|record| record.name).collect();
        assert!(
            names.contains(&"Added in KeePassXC".to_string()) && names.contains(&"GitHub".to_string()),
            "{names:?}"
        );
        assert_eq!(keepass.records().expect("records").len(), 3);
    }

    #[test]
    fn cards_and_notes_keep_their_kind() {
        let path = file("kinds", "door");
        let mut keepass = vault(&path);
        keepass.unlock(&Hidden::new("door".into())).expect("unlock");
        let card = Draft {
            kind: ItemKind::Card,
            name: "Visa".into(),
            card: Some(CardDraft {
                holder: "Artem".into(),
                brand: "Visa".into(),
                number: Hidden::new("4111111111111111".into()),
                code: Hidden::new("123".into()),
                expiry: "12/30".into(),
            }),
            ..Draft::default()
        };
        let id = keepass.save(&card, 0).expect("card");
        let note = Draft { kind: ItemKind::Note, name: "Wi-Fi".into(), notes: "guest".into(), ..Draft::default() };
        keepass.save(&note, 0).expect("note");
        let records = reopen(&path, "door");
        let visa = records.iter().find(|record| record.id == id).expect("card");
        assert_eq!(visa.kind, ItemKind::Card);
        let stored = visa.card.as_ref().expect("card fields");
        assert_eq!((stored.holder.as_str(), stored.number.expose()), ("Artem", b"4111111111111111".as_slice()));
        assert!(visa.fields.is_empty(), "card fields are not shown twice");
        assert!(records.iter().any(|record| record.kind == ItemKind::Note && record.name == "Wi-Fi"));
    }

    #[test]
    fn the_file_keeps_its_permissions() {
        use std::os::unix::fs::PermissionsExt as _;
        let path = file("mode", "door");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).expect("chmod");
        let mut keepass = vault(&path);
        keepass.unlock(&Hidden::new("door".into())).expect("unlock");
        keepass.save(&login("GitHub", "hunter2"), 0).expect("save");
        assert_eq!(std::fs::metadata(&path).expect("meta").permissions().mode() & 0o777, 0o640);
        let leftovers = std::fs::read_dir(path.parent().expect("dir")).expect("dir").count();
        assert_eq!(leftovers, 1, "no temporary file is left behind");
    }
}

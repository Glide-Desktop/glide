use std::collections::HashMap;
use std::path::PathBuf;

use glide_ipc::keychain::{
    Candidate, Card, Code, Detail, Field, Folder, Hidden, ItemKind, ItemRef, Keep, LOCAL, Report, Request, Response,
    SecretKind, Summary, Vault, strength,
};

use crate::local::Local;
use crate::provider::{Fault, Provider, Record};
use crate::totp::Totp;
use crate::vault::Params;

const OLD_AFTER: u64 = 365 * 24 * 60 * 60;
pub const LOCAL_NAME: &str = "Особисте";

#[must_use]
pub fn default_path() -> Option<PathBuf> {
    glide_config::data_home().map(|dir| dir.join("glide").join("keychain"))
}

pub struct Daemon {
    providers: Vec<Box<dyn Provider>>,
    default: String,
    clock: fn() -> u64,
}

impl std::fmt::Debug for Daemon {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Daemon").field("vaults", &self.providers.len()).finish_non_exhaustive()
    }
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.as_secs())
}

fn failed(fault: Fault) -> Response {
    match fault {
        Fault::Locked => Response::Locked,
        Fault::WrongPassword => Response::WrongPassword,
        Fault::Unsupported => Response::Unsupported,
        other => Response::Error { message: other.to_string() },
    }
}

fn done(result: Result<(), Fault>) -> Response {
    result.map_or_else(failed, |()| Response::Done)
}

fn text(secret: &crate::vault::Secret) -> String {
    String::from_utf8_lossy(secret.expose()).into_owned()
}

#[must_use]
pub fn mutates(request: &Request) -> bool {
    matches!(
        request,
        Request::Create { .. }
            | Request::Unlock { .. }
            | Request::Lock { .. }
            | Request::ChangePassword { .. }
            | Request::Save { .. }
            | Request::Trash { .. }
            | Request::Restore { .. }
            | Request::Purge { .. }
            | Request::AddBitwarden { .. }
            | Request::RemoveVault { .. }
            | Request::RenameVault { .. }
            | Request::SetDefault { .. }
            | Request::Sync { .. }
            | Request::Resolve { .. }
            | Request::CreateSend { .. }
            | Request::DisableSend { .. }
            | Request::DeleteSend { .. }
    )
}

impl Daemon {
    #[must_use]
    pub fn new(path: PathBuf, params: Params) -> Self {
        Self::with_providers(vec![Box::new(Local::new(path, params, LOCAL_NAME))])
    }

    #[must_use]
    pub fn with_providers(providers: Vec<Box<dyn Provider>>) -> Self {
        Self { providers, default: LOCAL.into(), clock: now }
    }

    #[must_use]
    pub fn with_clock(mut self, clock: fn() -> u64) -> Self {
        self.clock = clock;
        self
    }

    pub fn add(&mut self, provider: Box<dyn Provider>) {
        self.providers.push(provider);
    }

    fn index(&self, vault: &str) -> Result<usize, Fault> {
        self.providers.iter().position(|provider| provider.vault().id == vault).ok_or(Fault::NotFound)
    }

    fn provider(&self, vault: &str) -> Result<&dyn Provider, Fault> {
        Ok(self.providers[self.index(vault)?].as_ref())
    }

    fn provider_mut(&mut self, vault: &str) -> Result<&mut Box<dyn Provider>, Fault> {
        let index = self.index(vault)?;
        Ok(&mut self.providers[index])
    }

    fn record(&self, item: &ItemRef) -> Result<&Record, Fault> {
        self.provider(&item.vault)?.records()?.iter().find(|record| record.id == item.id).ok_or(Fault::NotFound)
    }

    fn open_records(&self) -> impl Iterator<Item = (&dyn Provider, &Record)> {
        self.providers
            .iter()
            .filter_map(|provider| provider.records().ok().map(|records| (provider.as_ref(), records)))
            .flat_map(|(provider, records)| records.iter().map(move |record| (provider, record)))
    }

    fn reuse(&self) -> HashMap<String, Vec<ItemRef>> {
        let mut seen: HashMap<String, Vec<ItemRef>> = HashMap::new();
        for (provider, record) in self.open_records() {
            if record.deleted.is_none() && record.kind == ItemKind::Login && !record.password.expose().is_empty() {
                seen.entry(record.password_text()).or_default().push(ItemRef::new(provider.vault().id, &record.id));
            }
        }
        seen
    }

    fn summary(vault: &str, record: &Record, reuse: &HashMap<String, Vec<ItemRef>>) -> Summary {
        let password = record.password_text();
        let reused = reuse.get(&password).map_or(0, |group| group.len().saturating_sub(1)) as u32;
        Summary {
            item: ItemRef::new(vault, &record.id),
            kind: record.kind,
            name: record.name.clone(),
            username: record.username.clone(),
            folder: record.folder.clone(),
            sites: record.sites.iter().map(|site| site.uri.clone()).collect(),
            apps: record.apps.iter().map(|app| app.app_id.clone()).collect(),
            has_password: !password.is_empty(),
            has_totp: !record.totp.expose().is_empty(),
            has_passkey: !record.passkeys.is_empty(),
            favorite: record.favorite,
            created: record.created,
            modified: record.modified,
            deleted: record.deleted.is_some(),
            strength: strength(&password),
            reused,
        }
    }

    fn items(&self) -> Vec<Summary> {
        let reuse = self.reuse();
        let mut list: Vec<Summary> =
            self.open_records().map(|(provider, record)| Self::summary(&provider.vault().id, record, &reuse)).collect();
        list.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then(a.item.cmp(&b.item)));
        list
    }

    fn detail(&self, item: &ItemRef) -> Result<Detail, Fault> {
        let record = self.record(item)?;
        let reuse = self.reuse();
        let card = record.card.as_ref().map(|card| {
            let number = text(&card.number);
            let digits: String = number.chars().filter(char::is_ascii_digit).collect();
            Card {
                holder: card.holder.clone(),
                brand: card.brand.clone(),
                last_digits: digits.chars().skip(digits.len().saturating_sub(4)).collect(),
                expiry: card.expiry.clone(),
            }
        });
        Ok(Detail {
            summary: Self::summary(&item.vault, record, &reuse),
            notes: record.notes.clone(),
            sites: record.sites.clone(),
            apps: record.apps.clone(),
            fields: record
                .fields
                .iter()
                .map(|field| Field {
                    name: field.name.clone(),
                    value: if field.hidden { String::new() } else { text(&field.value) },
                    hidden: field.hidden,
                })
                .collect(),
            passkeys: record.passkeys.clone(),
            history: record.history.len() as u32,
            card,
            password_length: record.password_text().chars().count() as u32,
        })
    }

    fn secret(&self, item: &ItemRef, kind: &SecretKind) -> Result<Hidden, Fault> {
        let record = self.record(item)?;
        let secret = match kind {
            SecretKind::Password => text(&record.password),
            SecretKind::Totp => text(&record.totp),
            SecretKind::Field(name) => record
                .fields
                .iter()
                .find(|field| &field.name == name)
                .map(|field| text(&field.value))
                .ok_or(Fault::NotFound)?,
            SecretKind::CardNumber => record.card.as_ref().map(|card| text(&card.number)).ok_or(Fault::NotFound)?,
            SecretKind::CardCode => record.card.as_ref().map(|card| text(&card.code)).ok_or(Fault::NotFound)?,
        };
        Ok(Hidden::new(secret))
    }

    fn code(&self, item: &ItemRef) -> Result<Code, Fault> {
        let record = self.record(item)?;
        let totp = Totp::parse(&text(&record.totp)).ok_or(Fault::NotFound)?;
        let at = (self.clock)();
        Ok(Code { code: totp.code_at(at), seconds_left: totp.seconds_left(at), period: totp.period })
    }

    fn candidate(provider: &dyn Provider, record: &Record) -> Candidate {
        Candidate {
            item: ItemRef::new(provider.vault().id, &record.id),
            label: record.name.clone(),
            username: record.username.clone(),
            vault_name: provider.vault().name,
            has_totp: !record.totp.expose().is_empty(),
        }
    }

    fn candidates(&self, program: Option<&str>, app_id: Option<&str>) -> Vec<Candidate> {
        let same = |value: &str, wanted: Option<&str>| {
            wanted.filter(|wanted| !wanted.is_empty()).is_some_and(|wanted| value.eq_ignore_ascii_case(wanted))
        };
        let mut found: Vec<(u64, Candidate)> = self
            .open_records()
            .filter(|(_, record)| {
                record.deleted.is_none()
                    && record.apps.iter().any(|app| same(&app.program, program) || same(&app.app_id, app_id))
            })
            .map(|(provider, record)| (record.modified, Self::candidate(provider, record)))
            .collect();
        found.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.item.cmp(&b.1.item)));
        found.into_iter().map(|(_, candidate)| candidate).collect()
    }

    fn search(&self, query: &str, limit: u32) -> Vec<Candidate> {
        let query = query.trim().to_lowercase();
        let mut found: Vec<(u8, u64, Candidate)> = self
            .open_records()
            .filter(|(_, record)| record.deleted.is_none() && record.kind == ItemKind::Login)
            .filter_map(|(provider, record)| {
                let name = record.name.to_lowercase();
                let rank = if query.is_empty() {
                    2
                } else if name.starts_with(&query) {
                    0
                } else if name.contains(&query)
                    || record.username.to_lowercase().contains(&query)
                    || record.sites.iter().any(|site| site.uri.to_lowercase().contains(&query))
                {
                    1
                } else {
                    return None;
                };
                Some((rank, record.modified, Self::candidate(provider, record)))
            })
            .collect();
        found.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)).then(a.2.label.cmp(&b.2.label)));
        found.into_iter().take(limit as usize).map(|(_, _, candidate)| candidate).collect()
    }

    fn report(&self) -> Report {
        let at = (self.clock)();
        let mut report = Report::default();
        for (provider, record) in self.open_records() {
            if record.deleted.is_some() || record.kind != ItemKind::Login || record.password.expose().is_empty() {
                continue;
            }
            report.checked += 1;
            let item = ItemRef::new(provider.vault().id, &record.id);
            if strength(&record.password_text()) <= 1 {
                report.weak.push(item.clone());
            }
            if at.saturating_sub(record.modified) > OLD_AFTER {
                report.old.push(item);
            }
        }
        let mut groups: Vec<Vec<ItemRef>> = self.reuse().into_values().filter(|group| group.len() > 1).collect();
        for group in &mut groups {
            group.sort();
        }
        groups.sort_by(|a, b| b.len().cmp(&a.len()).then(a.cmp(b)));
        report.reused = groups;
        report
    }

    fn save(&mut self, draft: &glide_ipc::keychain::Draft) -> Result<ItemRef, Fault> {
        let at = (self.clock)();
        let target = if draft.vault.is_empty() { self.default.clone() } else { draft.vault.clone() };
        match &draft.item {
            Some(existing) if existing.vault != target => {
                let mut moved = draft.clone();
                moved.item = None;
                let id = self.provider_mut(&target)?.save(&moved, at)?;
                self.provider_mut(&existing.vault)?.purge(&existing.id)?;
                Ok(ItemRef::new(target, id))
            }
            _ => {
                let id = self.provider_mut(&target)?.save(draft, at)?;
                Ok(ItemRef::new(target, id))
            }
        }
    }

    #[must_use]
    pub fn vaults(&self) -> Vec<Vault> {
        self.providers.iter().map(|provider| provider.vault()).collect()
    }

    pub fn handle(&mut self, request: Request) -> Response {
        match request {
            Request::Status => Response::Status {
                exists: self.providers.iter().any(|provider| provider.exists()),
                unlocked: self.providers.iter().any(|provider| provider.unlocked()),
            },
            Request::Vaults => Response::Vaults { list: self.vaults(), default: self.default.clone() },
            Request::Create { password } => done(self.provider_mut(LOCAL).and_then(|local| local.create(&password))),
            Request::Unlock { vault, password } => done(self.provider_mut(&vault).and_then(|p| p.unlock(&password))),
            Request::Lock { vault: Some(vault) } => done(self.provider_mut(&vault).map(|provider| provider.lock())),
            Request::Lock { vault: None } => {
                for provider in &mut self.providers {
                    provider.lock();
                }
                Response::Done
            }
            Request::RequestUnlock { .. } => Response::Done,
            Request::ChangePassword { vault, old, new } => {
                done(self.provider_mut(&vault).and_then(|provider| provider.change_password(&old, &new)))
            }
            Request::Items => Response::Items { list: self.items() },
            Request::Item { item } => {
                self.detail(&item).map_or_else(failed, |detail| Response::Item { detail: Box::new(detail) })
            }
            Request::Secret { item, secret } => {
                self.secret(&item, &secret).map_or_else(failed, |secret| Response::Secret { secret })
            }
            Request::Totp { item } => self.code(&item).map_or_else(failed, |code| Response::Code { code }),
            Request::Save { draft } => self.save(&draft).map_or_else(failed, |item| Response::Saved { item }),
            Request::Trash { item } => {
                let at = (self.clock)();
                done(self.provider_mut(&item.vault).and_then(|provider| provider.trash(&item.id, at)))
            }
            Request::Restore { item } => {
                done(self.provider_mut(&item.vault).and_then(|provider| provider.restore(&item.id)))
            }
            Request::Purge { item } => {
                done(self.provider_mut(&item.vault).and_then(|provider| provider.purge(&item.id)))
            }
            Request::Folders => Response::Folders {
                list: self
                    .providers
                    .iter()
                    .filter_map(|provider| {
                        let vault = provider.vault().id;
                        provider.folders().ok().map(|folders| (vault, folders))
                    })
                    .flat_map(|(vault, folders)| {
                        folders.into_iter().map(move |name| Folder { vault: vault.clone(), id: name.clone(), name })
                    })
                    .collect(),
            },
            Request::Candidates { program, app_id } => {
                Response::Candidates { list: self.candidates(program.as_deref(), app_id.as_deref()) }
            }
            Request::Search { query, limit } => Response::Candidates { list: self.search(&query, limit) },
            Request::Report => Response::Report { report: self.report() },
            other => self.accounts(other),
        }
    }

    fn accounts(&mut self, request: Request) -> Response {
        match request {
            Request::RenameVault { vault, name } => {
                done(self.provider_mut(&vault).map(|provider| provider.rename(name.trim())))
            }
            Request::SetDefault { vault } => match self.index(&vault) {
                Ok(_) => {
                    self.default = vault;
                    Response::Done
                }
                Err(fault) => failed(fault),
            },
            Request::Sync { vault } => {
                let mut result = Ok(());
                for provider in &mut self.providers {
                    if vault.as_ref().is_none_or(|vault| &provider.vault().id == vault) && provider.unlocked() {
                        result = result.and(provider.sync());
                    }
                }
                done(result)
            }
            Request::Conflicts => {
                Response::Conflicts { list: self.providers.iter().flat_map(|provider| provider.conflicts()).collect() }
            }
            Request::Resolve { item, keep } => done(self.resolve(&item, keep)),
            Request::Sends => {
                let mut list = Vec::new();
                for provider in &self.providers {
                    if let Ok(sends) = provider.sends() {
                        list.extend(sends);
                    }
                }
                Response::Sends { list }
            }
            Request::CreateSend { draft } => self
                .provider_mut(&draft.vault)
                .and_then(|provider| provider.create_send(&draft))
                .map_or_else(failed, |send| Response::SendCreated { send: Box::new(send) }),
            Request::DisableSend { vault, id } => {
                done(self.provider_mut(&vault).and_then(|provider| provider.disable_send(&id)))
            }
            Request::DeleteSend { vault, id } => {
                done(self.provider_mut(&vault).and_then(|provider| provider.delete_send(&id)))
            }
            Request::RemoveVault { vault } => {
                if vault == LOCAL {
                    return Response::Unsupported;
                }
                match self.index(&vault) {
                    Ok(index) => {
                        let result = self.providers[index].forget();
                        self.providers.remove(index);
                        if self.default == vault {
                            self.default = LOCAL.into();
                        }
                        done(result)
                    }
                    Err(fault) => failed(fault),
                }
            }
            _ => Response::Unsupported,
        }
    }

    fn resolve(&mut self, item: &ItemRef, keep: Keep) -> Result<(), Fault> {
        self.provider_mut(&item.vault)?.resolve(&item.id, keep)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glide_ipc::keychain::{AppLink, Draft, Site};

    const FAST: Params = Params { memory_kib: 64, passes: 1, lanes: 1 };
    const NOW: u64 = 2_000_000_000;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("glide-keychain-daemon-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("keychain")
    }

    fn hidden(text: &str) -> Hidden {
        Hidden::new(text.into())
    }

    fn daemon(name: &str) -> (Daemon, PathBuf) {
        let path = temp(name);
        let mut daemon = Daemon::new(path.clone(), FAST).with_clock(|| NOW);
        assert_eq!(daemon.handle(Request::Create { password: hidden("pw") }), Response::Done);
        (daemon, path)
    }

    fn login(name: &str, password: &str, app: &str) -> Draft {
        Draft {
            vault: LOCAL.into(),
            name: name.into(),
            username: "me".into(),
            password: Some(hidden(password)),
            apps: if app.is_empty() {
                Vec::new()
            } else {
                vec![AppLink { name: String::new(), app_id: app.into(), program: String::new() }]
            },
            sites: vec![Site { uri: format!("{}.example", name.to_lowercase()), ..Site::default() }],
            ..Draft::default()
        }
    }

    fn save(daemon: &mut Daemon, draft: Draft) -> ItemRef {
        match daemon.handle(Request::Save { draft: Box::new(draft) }) {
            Response::Saved { item } => item,
            other => panic!("{other:?}"),
        }
    }

    fn cleanup(path: &std::path::Path) {
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn a_vault_is_created_locked_and_unlocked_again() {
        let (mut daemon, path) = daemon("cycle");
        assert_eq!(daemon.handle(Request::Status), Response::Status { exists: true, unlocked: true });
        assert_eq!(daemon.handle(Request::Lock { vault: None }), Response::Done);
        assert_eq!(daemon.handle(Request::Items), Response::Items { list: Vec::new() });
        assert_eq!(
            daemon.handle(Request::Unlock { vault: LOCAL.into(), password: hidden("nope") }),
            Response::WrongPassword
        );
        assert_eq!(daemon.handle(Request::Unlock { vault: LOCAL.into(), password: hidden("pw") }), Response::Done);
        cleanup(&path);
    }

    #[test]
    fn candidates_match_the_app_newest_first_and_skip_the_trash() {
        let (mut daemon, path) = daemon("candidates");
        let old = save(&mut daemon, login("Old", "a", "org.Telegram.desktop"));
        save(&mut daemon, login("Other", "b", "firefox"));
        let Response::Candidates { list } =
            daemon.handle(Request::Candidates { program: None, app_id: Some("org.telegram.desktop".into()) })
        else {
            panic!()
        };
        assert_eq!(list.iter().map(|c| c.label.as_str()).collect::<Vec<_>>(), ["Old"]);
        assert_eq!(list[0].vault_name, LOCAL_NAME);
        daemon.handle(Request::Trash { item: old });
        assert_eq!(
            daemon.handle(Request::Candidates { program: None, app_id: Some("org.telegram.desktop".into()) }),
            Response::Candidates { list: Vec::new() }
        );
        cleanup(&path);
    }

    #[test]
    fn summaries_flag_weak_and_reused_passwords() {
        let (mut daemon, path) = daemon("report");
        save(&mut daemon, login("Router", "admin123", ""));
        let a = save(&mut daemon, login("Hetzner", "Hetzner2024", ""));
        let b = save(&mut daemon, login("GitLab", "Hetzner2024", ""));
        save(&mut daemon, login("GitHub", "c7#Vq2!mWx9tLp4Rz8", ""));
        let Response::Items { list } = daemon.handle(Request::Items) else { panic!() };
        let by_name = |name: &str| list.iter().find(|item| item.name == name).unwrap();
        assert_eq!((by_name("Router").strength, by_name("GitHub").strength), (1, 4));
        assert_eq!(by_name("Hetzner").reused, 1);
        let Response::Report { report } = daemon.handle(Request::Report) else { panic!() };
        assert_eq!(report.weak.len(), 1);
        let mut group = vec![a, b];
        group.sort();
        assert_eq!(report.reused, [group]);
        assert_eq!(report.checked, 4);
        assert!(report.old.is_empty());
        cleanup(&path);
    }

    #[test]
    fn a_totp_code_comes_from_the_stored_key() {
        let (mut daemon, path) = daemon("totp");
        let mut draft = login("GitHub", "x", "");
        draft.totp = Some(hidden("GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ"));
        let item = save(&mut daemon, draft);
        let Response::Code { code } = daemon.handle(Request::Totp { item: item.clone() }) else { panic!() };
        assert_eq!(code.code, "279037");
        assert_eq!((code.seconds_left, code.period), (10, 30));
        let Response::Item { detail } = daemon.handle(Request::Item { item }) else { panic!() };
        assert!(detail.summary.has_totp);
        cleanup(&path);
    }

    #[test]
    fn secrets_are_only_given_on_request() {
        let (mut daemon, path) = daemon("secrets");
        let item = save(&mut daemon, login("GitHub", "hunter2", ""));
        let Response::Item { detail } = daemon.handle(Request::Item { item: item.clone() }) else { panic!() };
        assert!(!format!("{detail:?}").contains("hunter2"));
        assert_eq!(detail.password_length, 7);
        assert_eq!(
            daemon.handle(Request::Secret { item, secret: SecretKind::Password }),
            Response::Secret { secret: hidden("hunter2") }
        );
        cleanup(&path);
    }

    #[test]
    fn search_ranks_prefixes_first() {
        let (mut daemon, path) = daemon("search");
        save(&mut daemon, login("GitHub", "a", ""));
        save(&mut daemon, login("MyGit", "b", ""));
        save(&mut daemon, login("Steam", "c", ""));
        let Response::Candidates { list } = daemon.handle(Request::Search { query: "git".into(), limit: 5 }) else {
            panic!()
        };
        assert_eq!(list.iter().map(|c| c.label.as_str()).collect::<Vec<_>>(), ["GitHub", "MyGit"]);
        cleanup(&path);
    }

    #[test]
    fn mutating_requests_are_known() {
        assert!(mutates(&Request::Lock { vault: None }));
        assert!(!mutates(&Request::Items));
    }
}

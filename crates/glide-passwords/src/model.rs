use glide_ipc::keychain::{ItemKind, ItemRef, Report, Summary, SyncState, Vault, VaultKind};
use glide_sdk::widget::VaultState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Section {
    All,
    #[default]
    Logins,
    Passkeys,
    Codes,
    Cards,
    Notes,
    Sends,
    Check,
    Trash,
}

impl Section {
    pub const CATEGORIES: [Self; 7] =
        [Self::All, Self::Logins, Self::Passkeys, Self::Codes, Self::Cards, Self::Notes, Self::Sends];

    #[must_use]
    pub fn holds(self, item: &Summary) -> bool {
        match self {
            Self::Trash => item.deleted,
            _ if item.deleted => false,
            Self::All | Self::Check => true,
            Self::Logins => item.kind == ItemKind::Login,
            Self::Passkeys => item.has_passkey,
            Self::Codes => item.has_totp,
            Self::Cards => item.kind == ItemKind::Card,
            Self::Notes => item.kind == ItemKind::Note,
            Self::Sends => false,
        }
    }
}

#[must_use]
pub fn in_vault(item: &Summary, vault: Option<&str>) -> bool {
    vault.is_none_or(|vault| item.item.vault == vault)
}

#[must_use]
pub fn found(item: &Summary, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    query.is_empty()
        || item.name.to_lowercase().contains(&query)
        || item.username.to_lowercase().contains(&query)
        || item.sites.iter().any(|site| site.to_lowercase().contains(&query))
        || item.folder.to_lowercase().contains(&query)
}

#[must_use]
pub fn visible<'a>(
    items: &'a [Summary], vault: Option<&str>, section: Section, query: &str, by_name: bool,
) -> Vec<&'a Summary> {
    let mut list: Vec<&Summary> =
        items.iter().filter(|item| in_vault(item, vault) && section.holds(item) && found(item, query)).collect();
    if by_name {
        list.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then(a.item.cmp(&b.item)));
    } else {
        list.sort_by(|a, b| b.modified.cmp(&a.modified).then(a.name.cmp(&b.name)));
    }
    list
}

#[must_use]
pub fn count(items: &[Summary], vault: Option<&str>, section: Section) -> usize {
    items.iter().filter(|item| in_vault(item, vault) && section.holds(item)).count()
}

#[must_use]
pub fn problems(report: &Report, vault: Option<&str>) -> usize {
    let mut flagged: Vec<&ItemRef> = report
        .weak
        .iter()
        .chain(report.reused.iter().flatten())
        .chain(report.leaked.iter())
        .filter(|item| vault.is_none_or(|vault| item.vault == vault))
        .collect();
    flagged.sort();
    flagged.dedup();
    flagged.len()
}

#[must_use]
pub fn vault_state(vault: &Vault) -> VaultState {
    match (&vault.kind, &vault.sync) {
        (VaultKind::Local, _) | (_, SyncState::Local) => VaultState::Local,
        (_, SyncState::Synced { .. } | SyncState::Syncing) => VaultState::Synced,
        (_, SyncState::Offline { .. } | SyncState::Failed { .. }) => VaultState::Pending,
    }
}

#[must_use]
pub fn pending(vaults: &[Vault]) -> u32 {
    vaults
        .iter()
        .map(|vault| match vault.sync {
            SyncState::Offline { pending } => pending,
            _ => 0,
        })
        .sum()
}

#[must_use]
pub fn vault_name<'a>(vaults: &'a [Vault], id: &'a str) -> &'a str {
    vaults.iter().find(|vault| vault.id == id).map_or(id, |vault| vault.name.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(name: &str, vault: &str, kind: ItemKind, modified: u64) -> Summary {
        Summary {
            item: ItemRef::new(vault, name),
            kind,
            name: name.into(),
            username: format!("{}-user", name.to_lowercase()),
            folder: String::new(),
            sites: vec![format!("{}.example", name.to_lowercase())],
            apps: Vec::new(),
            has_password: true,
            has_totp: name == "GitHub",
            has_passkey: false,
            favorite: false,
            created: 0,
            modified,
            deleted: false,
            strength: 3,
            reused: 0,
        }
    }

    fn sample() -> Vec<Summary> {
        let mut trashed = item("Old", "local", ItemKind::Login, 9);
        trashed.deleted = true;
        vec![
            item("GitHub", "bw", ItemKind::Login, 5),
            item("Steam", "local", ItemKind::Login, 7),
            item("Recovery codes", "bw", ItemKind::Note, 1),
            trashed,
        ]
    }

    #[test]
    fn sections_split_kinds_and_keep_the_trash_apart() {
        let items = sample();
        assert_eq!(count(&items, None, Section::All), 3);
        assert_eq!(count(&items, None, Section::Logins), 2);
        assert_eq!(count(&items, None, Section::Codes), 1);
        assert_eq!(count(&items, None, Section::Notes), 1);
        assert_eq!(count(&items, None, Section::Trash), 1);
        assert_eq!(count(&items, Some("bw"), Section::All), 2);
    }

    #[test]
    fn recent_comes_first_unless_sorted_by_name() {
        let items = sample();
        let names = |list: Vec<&Summary>| list.iter().map(|item| item.name.clone()).collect::<Vec<_>>();
        assert_eq!(names(visible(&items, None, Section::Logins, "", false)), ["Steam", "GitHub"]);
        assert_eq!(names(visible(&items, None, Section::Logins, "", true)), ["GitHub", "Steam"]);
    }

    #[test]
    fn search_reads_names_users_and_sites() {
        let items = sample();
        assert_eq!(visible(&items, None, Section::All, "steam-user", false).len(), 1);
        assert_eq!(visible(&items, None, Section::All, "github.example", false).len(), 1);
        assert!(visible(&items, None, Section::All, "nothing", false).is_empty());
    }

    #[test]
    fn problems_count_each_item_once() {
        let report = Report {
            weak: vec![ItemRef::new("a", "1")],
            reused: vec![vec![ItemRef::new("a", "1"), ItemRef::new("b", "2")]],
            ..Report::default()
        };
        assert_eq!(problems(&report, None), 2);
        assert_eq!(problems(&report, Some("b")), 1);
    }
}

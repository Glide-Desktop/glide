mod vault;

#[cfg(test)]
pub(crate) use vault::tests::sample as vault_tests_sample;
pub use vault::{KeePass, Registered};

use glide_ipc::keychain::{CardDraft, Draft, FieldDraft, Hidden, ItemKind, Matching, Passkey};
use keepass::db::{CustomDataItem, CustomDataValue, DatabaseOpenError, EntryMut, EntryRef, GroupId, GroupRef, fields};
use keepass::error::DatabaseKeyError;
use keepass::{Database, DatabaseKey};

use crate::bitwarden::cipher::app_uri;
use crate::provider::Record;
use crate::transfer::{ImportError, hidden, site_or_app};
use crate::vault::{PastSecret, Secret, StoredCard, StoredField};

const SIGNATURE: [u8; 4] = [0x03, 0xd9, 0xa2, 0x9a];
const LEGACY_SEED: &str = "TOTP Seed";
const LEGACY_SETTINGS: &str = "TOTP Settings";
const EXTRA_URL: &str = "KP2A_URL";
const PASSKEY: &str = "KPEX_PASSKEY_";
const PASSKEY_SITE: &str = "KPEX_PASSKEY_RELYING_PARTY";
const PASSKEY_USER: &str = "KPEX_PASSKEY_USERNAME";
const KIND: &str = "Glide.Kind";
const FAVORITE: &str = "Favorite";
const CARD_HOLDER: &str = "Card holder";
const CARD_BRAND: &str = "Card brand";
const CARD_NUMBER: &str = "Card number";
const CARD_CODE: &str = "Card code";
const CARD_EXPIRY: &str = "Card expiry";
const CARD_FIELDS: [&str; 5] = [CARD_HOLDER, CARD_BRAND, CARD_NUMBER, CARD_CODE, CARD_EXPIRY];
pub(crate) const FOLDER_SEPARATOR: &str = " / ";

#[must_use]
pub fn is_kdbx(bytes: &[u8]) -> bool {
    bytes.starts_with(&SIGNATURE)
}

pub(crate) fn open(bytes: &[u8], password: &str) -> Result<Database, ImportError> {
    Database::parse(bytes, DatabaseKey::new().with_password(password)).map_err(|error| match error {
        DatabaseOpenError::Key(DatabaseKeyError::IncorrectKey) => ImportError::WrongPassword,
        DatabaseOpenError::UnsupportedVersion => {
            ImportError::Unsupported("this KeePass database version is not supported".into())
        }
        other => ImportError::Malformed(format!("the KeePass database does not open: {other}")),
    })
}

pub fn parse(bytes: &[u8], password: Option<&str>) -> Result<Vec<Draft>, ImportError> {
    let password = password.filter(|password| !password.is_empty()).ok_or(ImportError::NeedsPassword)?;
    let database = open(bytes, password)?;
    Ok(records(&database).iter().filter(|record| record.deleted.is_none()).map(draft_of).collect())
}

pub(crate) fn records(database: &Database) -> Vec<Record> {
    let bin = database.recycle_bin().map(|group| group.id());
    let mut records = Vec::new();
    collect(&database.root(), &[], false, bin, &mut records);
    records
}

pub(crate) fn folders(database: &Database) -> Vec<String> {
    let bin = database.recycle_bin().map(|group| group.id());
    let mut folders = Vec::new();
    walk_folders(&database.root(), &[], bin, &mut folders);
    folders
}

fn walk_folders(group: &GroupRef<'_>, path: &[&str], bin: Option<GroupId>, folders: &mut Vec<String>) {
    for child in group.groups() {
        if Some(child.id()) == bin || child.name.trim().is_empty() {
            continue;
        }
        let mut inner = path.to_vec();
        inner.push(child.name.trim());
        folders.push(inner.join(FOLDER_SEPARATOR));
        walk_folders(&child, &inner, bin, folders);
    }
}

fn collect(group: &GroupRef<'_>, path: &[&str], deleted: bool, bin: Option<GroupId>, records: &mut Vec<Record>) {
    let folder = path.join(FOLDER_SEPARATOR);
    for entry in group.entries() {
        records.extend(record_of(&entry, folder.clone(), deleted));
    }
    for child in group.groups() {
        let into_bin = Some(child.id()) == bin;
        let mut inner = path.to_vec();
        if !into_bin && !child.name.trim().is_empty() {
            inner.push(child.name.trim());
        }
        collect(&child, &inner, deleted || into_bin, bin, records);
    }
}

fn seconds(stamp: Option<i64>) -> u64 {
    stamp.and_then(|stamp| u64::try_from(stamp).ok()).unwrap_or(0)
}

fn secret(text: &str) -> Secret {
    Secret::new(text.as_bytes().to_vec())
}

fn text(secret: &Secret) -> String {
    String::from_utf8_lossy(secret.expose()).into_owned()
}

fn reserved(key: &str, kind: ItemKind) -> bool {
    fields::KNOWN_FIELDS.contains(&key)
        || [fields::OTP, LEGACY_SEED, LEGACY_SETTINGS].contains(&key)
        || key.starts_with(EXTRA_URL)
        || key.starts_with(PASSKEY)
        || (kind == ItemKind::Card && CARD_FIELDS.contains(&key))
}

fn marked_kind(entry: &keepass::db::Entry) -> Option<ItemKind> {
    match entry.custom_data.get(KIND).and_then(|item| item.value.as_ref()) {
        Some(CustomDataValue::String(kind)) => match kind.as_str() {
            "glide:note" => Some(ItemKind::Note),
            "glide:card" => Some(ItemKind::Card),
            "glide:identity" => Some(ItemKind::Identity),
            _ => None,
        },
        _ => None,
    }
}

const fn kind_mark(kind: ItemKind) -> Option<&'static str> {
    match kind {
        ItemKind::Login => None,
        ItemKind::Note => Some("glide:note"),
        ItemKind::Card => Some("glide:card"),
        ItemKind::Identity => Some("glide:identity"),
    }
}

fn record_of(entry: &EntryRef<'_>, folder: String, deleted: bool) -> Option<Record> {
    let get = |name: &str| entry.get(name).map(str::trim).unwrap_or_default();
    let raw = |name: &str| entry.get(name).unwrap_or_default();
    let password = raw(fields::PASSWORD);
    let notes = raw(fields::NOTES);
    let mut urls: Vec<_> = entry.fields.iter().filter(|(key, _)| key.starts_with(EXTRA_URL)).collect();
    urls.sort_by(|left, right| left.0.cmp(right.0));
    let mut sites = Vec::new();
    let mut apps = Vec::new();
    for uri in std::iter::once(get(fields::URL)).chain(urls.iter().map(|(_, value)| value.get().as_str())) {
        let (site, app) = site_or_app(uri, Matching::Domain);
        sites.extend(site);
        apps.extend(app);
    }
    let mut name = get(fields::TITLE).to_string();
    if name.is_empty() {
        name = sites.first().map_or_else(|| get(fields::USERNAME).to_string(), |site| site.uri.clone());
    }
    if name.is_empty() {
        name =
            notes.lines().map(str::trim).find(|line| !line.is_empty()).unwrap_or_default().chars().take(48).collect();
    }
    if name.is_empty() && password.is_empty() && notes.is_empty() {
        return None;
    }
    let kind = marked_kind(entry).unwrap_or(
        if password.is_empty() && get(fields::USERNAME).is_empty() && sites.is_empty() && !notes.is_empty() {
            ItemKind::Note
        } else {
            ItemKind::Login
        },
    );
    let totp = match entry.get(fields::OTP).filter(|otp| !otp.trim().is_empty()) {
        Some(otp) => otp.trim().to_string(),
        None => entry.get(LEGACY_SEED).map(|seed| legacy_totp(seed, entry.get(LEGACY_SETTINGS))).unwrap_or_default(),
    };
    let mut custom: Vec<_> = entry
        .fields
        .iter()
        .filter(|(key, value)| !reserved(key, kind) && !value.get().is_empty())
        .map(|(key, value)| StoredField { name: key.clone(), value: secret(value.get()), hidden: value.is_protected() })
        .collect();
    custom.sort_by(|left, right| left.name.cmp(&right.name));
    let created = seconds(entry.times.creation.map(|at| at.and_utc().timestamp()));
    let modified = seconds(entry.times.last_modification.map(|at| at.and_utc().timestamp())).max(created);
    let mut history: Vec<PastSecret> = Vec::new();
    for past in entry.history.as_ref().map(|history| history.get_entries().as_slice()).unwrap_or_default() {
        let old = past.get(fields::PASSWORD).unwrap_or_default();
        if old.is_empty()
            || old == password
            || history.last().is_some_and(|last| last.secret.expose() == old.as_bytes())
        {
            continue;
        }
        let until = seconds(past.times.last_modification.map(|at| at.and_utc().timestamp()));
        history.push(PastSecret { secret: secret(old), until });
    }
    let passkeys = entry
        .get(PASSKEY_SITE)
        .filter(|site| !site.trim().is_empty())
        .map(|site| Passkey { rp_id: site.trim().to_string(), user_name: get(PASSKEY_USER).to_string(), created })
        .into_iter()
        .collect();
    Some(Record {
        id: entry.id().to_string(),
        kind,
        name,
        username: get(fields::USERNAME).to_string(),
        password: secret(password),
        totp: secret(&totp),
        notes: notes.to_string(),
        sites,
        apps,
        fields: custom,
        folder,
        favorite: entry.tags.iter().any(|tag| tag.eq_ignore_ascii_case(FAVORITE)),
        created,
        modified,
        deleted: deleted
            .then(|| seconds(entry.times.location_changed.map(|at| at.and_utc().timestamp())).max(modified)),
        history,
        card: (kind == ItemKind::Card).then(|| StoredCard {
            holder: get(CARD_HOLDER).to_string(),
            brand: get(CARD_BRAND).to_string(),
            number: secret(get(CARD_NUMBER)),
            code: secret(get(CARD_CODE)),
            expiry: get(CARD_EXPIRY).to_string(),
        }),
        passkeys,
    })
}

#[must_use]
pub(crate) fn draft_of(record: &Record) -> Draft {
    Draft {
        kind: record.kind,
        name: record.name.clone(),
        username: record.username.clone(),
        password: hidden(&text(&record.password)),
        totp: hidden(&text(&record.totp)),
        folder: record.folder.clone(),
        sites: record.sites.clone(),
        apps: record.apps.clone(),
        fields: record
            .fields
            .iter()
            .map(|field| FieldDraft {
                name: field.name.clone(),
                value: Hidden::new(text(&field.value)),
                hidden: field.hidden,
            })
            .collect(),
        notes: record.notes.clone(),
        favorite: record.favorite,
        card: record.card.as_ref().map(|card| CardDraft {
            holder: card.holder.clone(),
            brand: card.brand.clone(),
            number: Hidden::new(text(&card.number)),
            code: Hidden::new(text(&card.code)),
            expiry: card.expiry.clone(),
        }),
        history: record.history.iter().map(|past| (Hidden::new(text(&past.secret)), past.until)).collect(),
        ..Draft::default()
    }
}

fn put(entry: &mut EntryMut<'_>, key: &str, value: &str, protected: bool) {
    if value.is_empty() {
        entry.fields.remove(key);
    } else if protected {
        entry.set_protected(key, value);
    } else {
        entry.set_unprotected(key, value);
    }
}

pub(crate) fn write(entry: &mut EntryMut<'_>, draft: &Draft) {
    put(entry, fields::TITLE, draft.name.trim(), false);
    put(entry, fields::USERNAME, draft.username.trim(), false);
    if let Some(password) = &draft.password {
        put(entry, fields::PASSWORD, password.expose(), true);
    }
    put(entry, fields::NOTES, &draft.notes, false);
    if let Some(totp) = &draft.totp {
        put(entry, fields::OTP, totp.expose().trim(), true);
        entry.fields.remove(LEGACY_SEED);
        entry.fields.remove(LEGACY_SETTINGS);
    }
    let mut uris: Vec<String> =
        draft.sites.iter().map(|site| site.uri.trim().to_string()).filter(|uri| !uri.is_empty()).collect();
    uris.extend(draft.apps.iter().filter(|app| !app.app_id.trim().is_empty() || !app.program.is_empty()).map(app_uri));
    entry.fields.retain(|key, _| !key.starts_with(EXTRA_URL));
    let mut uris = uris.into_iter();
    put(entry, fields::URL, &uris.next().unwrap_or_default(), false);
    for (index, uri) in uris.enumerate() {
        let key = if index == 0 { EXTRA_URL.to_string() } else { format!("{EXTRA_URL}_{index}") };
        entry.set_unprotected(key, uri);
    }
    entry
        .fields
        .retain(|key, _| reserved(key, draft.kind) || draft.fields.iter().any(|field| field.name.trim() == key));
    for field in
        draft.fields.iter().filter(|field| !field.name.trim().is_empty() && !reserved(field.name.trim(), draft.kind))
    {
        if field.hidden && field.value.is_empty() && entry.fields.contains_key(field.name.trim()) {
            continue;
        }
        put(entry, field.name.trim(), field.value.expose(), field.hidden);
    }
    if let Some(card) = draft.card.as_ref().filter(|_| draft.kind == ItemKind::Card) {
        put(entry, CARD_HOLDER, card.holder.trim(), false);
        put(entry, CARD_BRAND, card.brand.trim(), false);
        put(entry, CARD_EXPIRY, card.expiry.trim(), false);
        if !card.number.is_empty() {
            put(entry, CARD_NUMBER, card.number.expose(), true);
        }
        if !card.code.is_empty() {
            put(entry, CARD_CODE, card.code.expose(), true);
        }
    }
    match kind_mark(draft.kind) {
        Some(mark) => {
            let item =
                CustomDataItem { value: Some(CustomDataValue::String(mark.to_string())), ..CustomDataItem::default() };
            entry.custom_data.insert(KIND.to_string(), item);
        }
        None => {
            entry.custom_data.remove(KIND);
        }
    }
    entry.tags.retain(|tag| !tag.eq_ignore_ascii_case(FAVORITE));
    if draft.favorite {
        entry.tags.push(FAVORITE.to_string());
    }
}

fn legacy_totp(seed: &str, settings: Option<&str>) -> String {
    let seed = seed.trim();
    let (period, digits) = settings.and_then(|settings| settings.split_once(';')).unwrap_or(("30", "6"));
    if (period.trim(), digits.trim()) == ("30", "6") || !digits.trim().chars().all(|ch| ch.is_ascii_digit()) {
        return seed.to_string();
    }
    format!("otpauth://totp/KeePass?secret={seed}&period={}&digits={}", period.trim(), digits.trim())
}

#[cfg(test)]
mod tests {
    use super::*;
    use keepass::config::{DatabaseConfig, KdfConfig};
    use keepass::db::Value;

    fn saved(password: &str) -> Vec<u8> {
        let mut config = DatabaseConfig::default();
        config.kdf_config = KdfConfig::Aes { rounds: 10 };
        let mut database = Database::with_config(config);
        let mut root = database.root_mut();
        root.add_entry().edit(|entry| {
            entry.set_unprotected(fields::TITLE, "Router");
            entry.set_unprotected(fields::USERNAME, "admin");
            entry.set_protected(fields::PASSWORD, "hunter2");
        });
        root.add_group()
            .edit(|group| group.name = "Work".into())
            .add_group()
            .edit(|group| group.name = "Git".into())
            .add_entry()
            .edit(|entry| {
                entry.set_unprotected(fields::TITLE, "GitHub");
                entry.set_unprotected(fields::USERNAME, "artem");
                entry.set_protected(fields::PASSWORD, "c7#Vq2!mWx9tLp4Rz8");
                entry.set_unprotected(fields::URL, "https://github.com/login");
                entry.set_unprotected("KP2A_URL_1", "https://gist.github.com");
                entry.set_protected(fields::OTP, "otpauth://totp/GitHub:artem?secret=JBSWY3DPEHPK3PXP");
                entry.set_protected("Recovery", "1234-5678");
                entry.set_unprotected("Empty", "");
                entry.set(fields::NOTES, Value::unprotected("backup codes elsewhere".to_string()));
            });
        root.add_group().edit(|group| group.name = "Notes".into()).add_entry().edit(|entry| {
            entry.set_unprotected(fields::TITLE, "Wi-Fi");
            entry.set_unprotected(fields::NOTES, "guest: plum-ocean");
            entry.set_unprotected(LEGACY_SEED, "GEZDGNBVGY3TQOJQ");
            entry.set_unprotected(LEGACY_SETTINGS, "30;8");
        });
        let bin = root.add_group().edit(|group| group.name = "Recycle Bin".into()).id();
        root.group_mut(bin).expect("bin").add_entry().edit(|entry| {
            entry.set_unprotected(fields::TITLE, "Old");
            entry.set_protected(fields::PASSWORD, "gone");
        });
        database.meta.recyclebin_uuid = Some(bin.uuid());
        let mut bytes = Vec::new();
        database.save(&mut bytes, DatabaseKey::new().with_password(password)).expect("save");
        bytes
    }

    #[test]
    fn a_database_is_read_with_folders_fields_and_codes() {
        let bytes = saved("door");
        assert!(is_kdbx(&bytes));
        let mut drafts = parse(&bytes, Some("door")).expect("open");
        drafts.sort_by(|left, right| left.name.cmp(&right.name));
        let names: Vec<_> = drafts.iter().map(|draft| (draft.name.as_str(), draft.folder.as_str())).collect();
        assert_eq!(
            names,
            [("GitHub", "Work / Git"), ("Router", ""), ("Wi-Fi", "Notes")],
            "the recycle bin is left out"
        );

        let github = &drafts[0];
        assert_eq!(github.password.as_ref().map(Hidden::expose), Some("c7#Vq2!mWx9tLp4Rz8"));
        assert_eq!(
            github.sites.iter().map(|site| site.uri.as_str()).collect::<Vec<_>>(),
            ["https://github.com/login", "https://gist.github.com"]
        );
        assert_eq!(
            github.totp.as_ref().map(Hidden::expose),
            Some("otpauth://totp/GitHub:artem?secret=JBSWY3DPEHPK3PXP")
        );
        assert_eq!(github.fields.len(), 1);
        assert_eq!(
            (github.fields[0].name.as_str(), github.fields[0].value.expose(), github.fields[0].hidden),
            ("Recovery", "1234-5678", true)
        );
        assert_eq!(github.notes, "backup codes elsewhere");

        let wifi = &drafts[2];
        assert_eq!(wifi.kind, ItemKind::Note);
        assert_eq!(
            wifi.totp.as_ref().map(Hidden::expose),
            Some("otpauth://totp/KeePass?secret=GEZDGNBVGY3TQOJQ&period=30&digits=8")
        );
    }

    #[test]
    fn a_database_sent_as_base64_is_recognised() {
        use base64::Engine as _;
        let encoded = base64::engine::general_purpose::STANDARD.encode(saved("door"));
        assert_eq!(crate::transfer::parse_file(&encoded, true, Some("door")).expect("open").len(), 3);
        assert_eq!(crate::transfer::parse_file(&encoded, true, None), Err(ImportError::NeedsPassword));
        let garbage = base64::engine::general_purpose::STANDARD.encode([0xff, 0xfe, 0x00, 0x13]);
        assert!(matches!(crate::transfer::parse_file(&garbage, true, None), Err(ImportError::Unsupported(_))));
        let csv = base64::engine::general_purpose::STANDARD.encode("name,password\nRouter,admin\n");
        assert_eq!(crate::transfer::parse_file(&csv, true, None).expect("csv").len(), 1);
    }

    #[test]
    fn the_password_is_asked_for_and_checked() {
        let bytes = saved("door");
        assert_eq!(parse(&bytes, None), Err(ImportError::NeedsPassword));
        assert_eq!(parse(&bytes, Some("window")), Err(ImportError::WrongPassword));
        assert!(!is_kdbx(b"name,password\n"));
    }

    #[test]
    #[ignore = "needs databases saved by KeePass or KeePassXC in $GLIDE_KDBX_DIR"]
    fn databases_from_other_programs_open() {
        let dir = std::env::var("GLIDE_KDBX_DIR").expect("GLIDE_KDBX_DIR");
        let password = std::env::var("GLIDE_KDBX_PASSWORD").expect("GLIDE_KDBX_PASSWORD");
        let mut opened = 0;
        for entry in std::fs::read_dir(dir).expect("dir") {
            let path = entry.expect("entry").path();
            let bytes = std::fs::read(&path).expect("read");
            let drafts = parse(&bytes, Some(&password)).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            for draft in &drafts {
                println!(
                    "{}: {:?} [{}] password={} notes={} totp={} fields={}",
                    path.display(),
                    draft.name,
                    draft.folder,
                    draft.password.is_some(),
                    draft.notes.len(),
                    draft.totp.is_some(),
                    draft.fields.len()
                );
            }
            assert!(!drafts.is_empty(), "{} has no entries", path.display());
            opened += 1;
        }
        assert!(opened > 0);
    }

    #[test]
    fn standard_legacy_codes_keep_only_the_seed() {
        assert_eq!(legacy_totp(" JBSWY3DP ", None), "JBSWY3DP");
        assert_eq!(legacy_totp("JBSWY3DP", Some("30;6")), "JBSWY3DP");
        assert_eq!(legacy_totp("JBSWY3DP", Some("30;S")), "JBSWY3DP", "Steam codes are left to the user");
    }
}

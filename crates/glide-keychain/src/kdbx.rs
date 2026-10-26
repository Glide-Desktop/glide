use glide_ipc::keychain::{Draft, FieldDraft, Hidden, ItemKind, Matching};
use keepass::db::{DatabaseOpenError, GroupRef, fields};
use keepass::error::DatabaseKeyError;
use keepass::{Database, DatabaseKey};

use crate::transfer::{ImportError, hidden, site_or_app};

const SIGNATURE: [u8; 4] = [0x03, 0xd9, 0xa2, 0x9a];
const LEGACY_SEED: &str = "TOTP Seed";
const LEGACY_SETTINGS: &str = "TOTP Settings";
const EXTRA_URL: &str = "KP2A_URL";

#[must_use]
pub fn is_kdbx(bytes: &[u8]) -> bool {
    bytes.starts_with(&SIGNATURE)
}

pub fn parse(bytes: &[u8], password: Option<&str>) -> Result<Vec<Draft>, ImportError> {
    let password = password.filter(|password| !password.is_empty()).ok_or(ImportError::NeedsPassword)?;
    let database = Database::parse(bytes, DatabaseKey::new().with_password(password)).map_err(|error| match error {
        DatabaseOpenError::Key(DatabaseKeyError::IncorrectKey) => ImportError::WrongPassword,
        DatabaseOpenError::UnsupportedVersion => {
            ImportError::Unsupported("this KeePass database version is not supported".into())
        }
        other => ImportError::Malformed(format!("the KeePass database does not open: {other}")),
    })?;
    let bin = database.recycle_bin().map(|group| group.id());
    let mut drafts = Vec::new();
    for group in database.root().groups() {
        walk(&group, &[], bin, &mut drafts);
    }
    for entry in database.root().entries() {
        drafts.extend(draft(&entry, String::new()));
    }
    Ok(drafts)
}

fn walk(group: &GroupRef<'_>, path: &[&str], bin: Option<keepass::db::GroupId>, drafts: &mut Vec<Draft>) {
    if Some(group.id()) == bin {
        return;
    }
    let mut path = path.to_vec();
    path.push(group.name.trim());
    let folder = path.iter().filter(|name| !name.is_empty()).copied().collect::<Vec<_>>().join(" / ");
    for entry in group.entries() {
        drafts.extend(draft(&entry, folder.clone()));
    }
    for child in group.groups() {
        walk(&child, &path, bin, drafts);
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

fn draft(entry: &keepass::db::Entry, folder: String) -> Option<Draft> {
    let get = |name: &str| entry.get(name).map(str::trim).unwrap_or_default();
    let password = entry.get(fields::PASSWORD).unwrap_or_default();
    let notes = entry.get(fields::NOTES).unwrap_or_default();
    let (site, app) = site_or_app(get(fields::URL), Matching::Domain);
    let mut name = get(fields::TITLE).to_string();
    if name.is_empty() {
        name = site.as_ref().map_or_else(|| get(fields::USERNAME).to_string(), |site| site.uri.clone());
    }
    if name.is_empty() {
        name =
            notes.lines().map(str::trim).find(|line| !line.is_empty()).unwrap_or_default().chars().take(48).collect();
    }
    if name.is_empty() && password.is_empty() && notes.is_empty() {
        return None;
    }
    let totp = match entry.get(fields::OTP).filter(|otp| !otp.trim().is_empty()) {
        Some(otp) => otp.trim().to_string(),
        None => entry.get(LEGACY_SEED).map(|seed| legacy_totp(seed, entry.get(LEGACY_SETTINGS))).unwrap_or_default(),
    };
    let mut sites: Vec<_> = site.into_iter().collect();
    let mut apps: Vec<_> = app.into_iter().collect();
    let mut extra: Vec<_> = entry.fields.iter().collect();
    extra.sort_by(|left, right| left.0.cmp(right.0));
    let mut custom = Vec::new();
    for (key, value) in extra {
        if fields::KNOWN_FIELDS.contains(&key.as_str())
            || [fields::OTP, LEGACY_SEED, LEGACY_SETTINGS].contains(&key.as_str())
        {
            continue;
        }
        if key.starts_with(EXTRA_URL) {
            let (site, app) = site_or_app(value.get(), Matching::Domain);
            sites.extend(site);
            apps.extend(app);
            continue;
        }
        if value.get().is_empty() {
            continue;
        }
        custom.push(FieldDraft {
            name: key.clone(),
            value: Hidden::new(value.get().clone()),
            hidden: value.is_protected(),
        });
    }
    let note = password.is_empty() && get(fields::USERNAME).is_empty() && sites.is_empty() && !notes.is_empty();
    Some(Draft {
        kind: if note { ItemKind::Note } else { ItemKind::Login },
        name,
        username: get(fields::USERNAME).to_string(),
        password: hidden(password),
        totp: hidden(&totp),
        folder,
        sites,
        apps,
        fields: custom,
        notes: notes.to_string(),
        ..Draft::default()
    })
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

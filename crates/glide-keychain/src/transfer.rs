use std::collections::HashMap;

use glide_ipc::keychain::{AppLink, CardDraft, Draft, FieldDraft, Hidden, ItemKind, Matching, Site};
use serde_json::{Value, json};
use zeroize::Zeroizing;

use crate::bitwarden::api::{field, list, number, text};
use crate::bitwarden::cipher::{app_uri, parse_app_uri};
use crate::bitwarden::crypto::{self, Kdf, SymmetricKey};
use crate::provider::Record;

pub const EXPORT_ITERATIONS: u32 = 600_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportError {
    NeedsPassword,
    WrongPassword,
    Unsupported(String),
    Malformed(String),
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NeedsPassword => f.write_str("the file is protected with a password"),
            Self::WrongPassword => f.write_str("the password does not open the file"),
            Self::Unsupported(what) | Self::Malformed(what) => f.write_str(what),
        }
    }
}

#[must_use]
pub fn csv_rows(input: &str) -> Vec<Vec<Zeroizing<String>>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cell = Zeroizing::new(String::new());
    let mut quoted = false;
    let mut chars = input.trim_start_matches('\u{feff}').chars().peekable();
    while let Some(ch) = chars.next() {
        match (quoted, ch) {
            (true, '"') if chars.peek() == Some(&'"') => {
                cell.push('"');
                chars.next();
            }
            (true, '"') => quoted = false,
            (false, '"') if cell.is_empty() => quoted = true,
            (false, ',') => row.push(std::mem::replace(&mut cell, Zeroizing::new(String::new()))),
            (false, '\r') if chars.peek() == Some(&'\n') => {}
            (false, '\n' | '\r') => {
                row.push(std::mem::replace(&mut cell, Zeroizing::new(String::new())));
                if row.iter().any(|value| !value.is_empty()) {
                    rows.push(std::mem::take(&mut row));
                } else {
                    row.clear();
                }
            }
            (_, other) => cell.push(other),
        }
    }
    row.push(cell);
    if row.iter().any(|value| !value.is_empty()) {
        rows.push(row);
    }
    rows
}

fn column(header: &[Zeroizing<String>], names: &[&str]) -> Option<usize> {
    header.iter().position(|title| {
        let title = title.trim().to_lowercase();
        names.iter().any(|name| title == *name)
    })
}

fn hidden(text: &str) -> Option<Hidden> {
    (!text.is_empty()).then(|| Hidden::new(text.to_string()))
}

fn site_or_app(uri: &str, matching: Matching) -> (Option<Site>, Option<AppLink>) {
    let uri = uri.trim();
    if uri.is_empty() {
        return (None, None);
    }
    match parse_app_uri(uri) {
        Some(app) => (None, Some(app)),
        None => (Some(Site { uri: uri.to_string(), matching }), None),
    }
}

fn from_csv(input: &str) -> Result<Vec<Draft>, ImportError> {
    let rows = csv_rows(input);
    let Some((header, body)) = rows.split_first() else {
        return Err(ImportError::Malformed("the file is empty".into()));
    };
    let name = column(header, &["name", "title", "account", "label"]);
    let username = column(header, &["login_username", "username", "user name", "login", "user", "email"]);
    let password = column(header, &["login_password", "password", "pass"]);
    let url = column(header, &["login_uri", "url", "uri", "website", "web site", "origin"]);
    let notes = column(header, &["notes", "note", "extra", "comments", "comment"]);
    let totp = column(header, &["login_totp", "totp", "otpauth", "otp"]);
    let folder = column(header, &["folder", "group", "grouping"]);
    let kind = column(header, &["type"]);
    if password.is_none() && notes.is_none() {
        return Err(ImportError::Unsupported("the file has no password or notes column".into()));
    }
    let cell = |row: &[Zeroizing<String>], index: Option<usize>| -> String {
        index.and_then(|index| row.get(index)).map(|value| value.trim().to_string()).unwrap_or_default()
    };
    let mut drafts = Vec::new();
    for row in body {
        let (site, app) = site_or_app(&cell(row, url), Matching::Domain);
        let note = cell(row, kind) == "note";
        let mut title = cell(row, name);
        if title.is_empty() {
            title = site.as_ref().map(|site| site.uri.clone()).unwrap_or_default();
        }
        let secret = index_value(row, password);
        if title.is_empty() && secret.is_empty() {
            continue;
        }
        drafts.push(Draft {
            kind: if note { ItemKind::Note } else { ItemKind::Login },
            name: title,
            username: cell(row, username),
            password: hidden(&secret),
            totp: hidden(&cell(row, totp)),
            folder: cell(row, folder).trim_matches('/').replace('/', " / "),
            sites: site.into_iter().collect(),
            apps: app.into_iter().collect(),
            notes: index_value(row, notes).to_string(),
            ..Draft::default()
        });
    }
    Ok(drafts)
}

fn index_value(row: &[Zeroizing<String>], index: Option<usize>) -> Zeroizing<String> {
    Zeroizing::new(index.and_then(|index| row.get(index)).map(|value| value.to_string()).unwrap_or_default())
}

const fn kind_of(code: Option<u64>) -> Option<ItemKind> {
    match code {
        Some(1) => Some(ItemKind::Login),
        Some(2) => Some(ItemKind::Note),
        Some(3) => Some(ItemKind::Card),
        Some(4) => Some(ItemKind::Identity),
        _ => None,
    }
}

const fn matching_of(code: Option<u64>) -> Matching {
    match code {
        Some(1) => Matching::Host,
        Some(2) => Matching::StartsWith,
        Some(3) => Matching::Exact,
        Some(4) => Matching::Regex,
        Some(5) => Matching::Never,
        _ => Matching::Domain,
    }
}

const fn code_of_matching(matching: Matching) -> Option<u64> {
    match matching {
        Matching::Domain => None,
        Matching::Host => Some(1),
        Matching::StartsWith => Some(2),
        Matching::Exact => Some(3),
        Matching::Regex => Some(4),
        Matching::Never => Some(5),
    }
}

fn plain(value: &Value, name: &str) -> String {
    text(value, name).unwrap_or_default().to_string()
}

fn item_draft(item: &Value, folders: &HashMap<String, String>) -> Option<Draft> {
    let kind = kind_of(number(item, "type"))?;
    let mut draft = Draft {
        kind,
        name: plain(item, "name"),
        notes: plain(item, "notes"),
        favorite: field(item, "favorite").and_then(Value::as_bool).unwrap_or(false),
        folder: text(item, "folderId").and_then(|id| folders.get(id)).cloned().unwrap_or_default(),
        ..Draft::default()
    };
    if let Some(login) = field(item, "login").filter(|login| login.is_object()) {
        draft.username = plain(login, "username");
        draft.password = hidden(&plain(login, "password"));
        draft.totp = hidden(&plain(login, "totp"));
        for uri in list(login, "uris") {
            let (site, app) = site_or_app(&plain(uri, "uri"), matching_of(number(uri, "match")));
            draft.sites.extend(site);
            draft.apps.extend(app);
        }
    }
    for entry in list(item, "fields") {
        if number(entry, "type") == Some(3) {
            continue;
        }
        draft.fields.push(FieldDraft {
            name: plain(entry, "name"),
            value: Hidden::new(plain(entry, "value")),
            hidden: number(entry, "type") == Some(1),
        });
    }
    if let Some(card) = field(item, "card").filter(|card| card.is_object()) {
        let month = plain(card, "expMonth");
        let year = plain(card, "expYear");
        let expiry = if month.is_empty() && year.is_empty() {
            String::new()
        } else {
            format!("{month:0>2}/{}", year.get(year.len().saturating_sub(2)..).unwrap_or_default())
        };
        draft.card = Some(CardDraft {
            holder: plain(card, "cardholderName"),
            brand: plain(card, "brand"),
            number: Hidden::new(plain(card, "number")),
            code: Hidden::new(plain(card, "code")),
            expiry,
        });
    }
    Some(draft)
}

fn from_plain_json(value: &Value) -> Vec<Draft> {
    let folders: HashMap<String, String> = list(value, "folders")
        .iter()
        .filter_map(|folder| Some((text(folder, "id")?.to_string(), text(folder, "name")?.to_string())))
        .collect();
    list(value, "items").iter().filter_map(|item| item_draft(item, &folders)).collect()
}

fn protected_key(value: &Value, password: &str) -> Result<SymmetricKey, ImportError> {
    let salt = text(value, "salt").ok_or_else(|| ImportError::Malformed("the file has no salt".into()))?;
    let iterations = number(value, "kdfIterations").unwrap_or(u64::from(EXPORT_ITERATIONS)) as u32;
    let memory = number(value, "kdfMemory").map(|memory| memory as u32);
    let parallelism = number(value, "kdfParallelism").map(|lanes| lanes as u32);
    let kdf = Kdf::from_prelogin(number(value, "kdfType").unwrap_or(0) as u8, iterations, memory, parallelism)
        .map_err(|err| ImportError::Unsupported(err.to_string()))?;
    let derived = crypto::derive_key(password.as_bytes(), salt.as_bytes(), kdf)
        .map_err(|err| ImportError::Unsupported(err.to_string()))?;
    Ok(SymmetricKey::stretch(&derived))
}

pub fn parse(input: &str, password: Option<&str>) -> Result<Vec<Draft>, ImportError> {
    let trimmed = input.trim_start_matches('\u{feff}').trim_start();
    if !trimmed.starts_with('{') {
        return from_csv(input);
    }
    let value: Value = serde_json::from_str(trimmed)
        .map_err(|err| ImportError::Malformed(format!("the JSON does not parse: {err}")))?;
    if field(&value, "encrypted").and_then(Value::as_bool) != Some(true) {
        return Ok(from_plain_json(&value));
    }
    if field(&value, "passwordProtected").and_then(Value::as_bool) != Some(true) {
        return Err(ImportError::Unsupported(
            "this export is tied to its Bitwarden account; export it again with a file password".into(),
        ));
    }
    let password = password.filter(|password| !password.is_empty()).ok_or(ImportError::NeedsPassword)?;
    let key = protected_key(&value, password)?;
    let check = text(&value, "encKeyValidation_DO_NOT_EDIT").unwrap_or_default();
    key.decrypt(check).map_err(|_| ImportError::WrongPassword)?;
    let data = text(&value, "data").ok_or_else(|| ImportError::Malformed("the file has no data".into()))?;
    let data = Zeroizing::new(key.decrypt_text(data).map_err(|_| ImportError::WrongPassword)?);
    let inner: Value =
        serde_json::from_str(&data).map_err(|err| ImportError::Malformed(format!("the JSON does not parse: {err}")))?;
    Ok(from_plain_json(&inner))
}

fn secret_text(secret: &crate::vault::Secret) -> Value {
    let text = String::from_utf8_lossy(secret.expose());
    if text.is_empty() { Value::Null } else { json!(text) }
}

const fn type_code(kind: ItemKind) -> u64 {
    match kind {
        ItemKind::Login => 1,
        ItemKind::Note => 2,
        ItemKind::Card => 3,
        ItemKind::Identity => 4,
    }
}

fn plain_item(record: &Record, folders: &HashMap<String, String>) -> Value {
    let mut item = json!({
        "id": crypto::uuid_v4().unwrap_or_default(),
        "organizationId": null,
        "folderId": folders.get(&record.folder),
        "type": type_code(record.kind),
        "reprompt": 0,
        "name": record.name,
        "notes": if record.notes.is_empty() { Value::Null } else { json!(record.notes) },
        "favorite": record.favorite,
        "collectionIds": null,
        "fields": record.fields.iter().map(|entry| json!({
            "name": entry.name,
            "value": secret_text(&entry.value),
            "type": u8::from(entry.hidden),
        })).collect::<Vec<_>>(),
    });
    let mut uris: Vec<Value> =
        record.sites.iter().map(|site| json!({ "uri": site.uri, "match": code_of_matching(site.matching) })).collect();
    uris.extend(record.apps.iter().map(|app| json!({ "uri": app_uri(app), "match": 5 })));
    match record.kind {
        ItemKind::Login => {
            item["login"] = json!({
                "username": if record.username.is_empty() { Value::Null } else { json!(record.username) },
                "password": secret_text(&record.password),
                "totp": secret_text(&record.totp),
                "uris": uris,
            });
        }
        ItemKind::Note => item["secureNote"] = json!({ "type": 0 }),
        ItemKind::Card => {
            if let Some(card) = &record.card {
                let (month, year) = card.expiry.split_once('/').unwrap_or((card.expiry.as_str(), ""));
                let year = if year.trim().len() == 2 { format!("20{}", year.trim()) } else { year.trim().to_string() };
                item["card"] = json!({
                    "cardholderName": card.holder,
                    "brand": card.brand,
                    "number": secret_text(&card.number),
                    "code": secret_text(&card.code),
                    "expMonth": month.trim().trim_start_matches('0'),
                    "expYear": year,
                });
            }
        }
        ItemKind::Identity => item["identity"] = json!({}),
    }
    item
}

pub fn export(records: &[&Record], password: &str) -> Result<String, crypto::CryptoError> {
    let mut folders: HashMap<String, String> = HashMap::new();
    for record in records {
        if !record.folder.is_empty() && !folders.contains_key(&record.folder) {
            folders.insert(record.folder.clone(), crypto::uuid_v4()?);
        }
    }
    let folder_list: Vec<Value> = folders.iter().map(|(name, id)| json!({ "id": id, "name": name })).collect();
    let items: Vec<Value> = records.iter().map(|record| plain_item(record, &folders)).collect();
    let data = Zeroizing::new(
        serde_json::to_string(&json!({ "encrypted": false, "folders": folder_list, "items": items }))
            .map_err(|_| crypto::CryptoError::Malformed)?,
    );
    let salt = crypto::url_b64(&crypto::random_bytes::<16>()?);
    let kdf = Kdf::Pbkdf2 { iterations: EXPORT_ITERATIONS };
    let derived = crypto::derive_key(password.as_bytes(), salt.as_bytes(), kdf)?;
    let key = SymmetricKey::stretch(&derived);
    let file = json!({
        "encrypted": true,
        "passwordProtected": true,
        "salt": salt,
        "kdfType": 0,
        "kdfIterations": EXPORT_ITERATIONS,
        "kdfMemory": null,
        "kdfParallelism": null,
        "encKeyValidation_DO_NOT_EDIT": key.encrypt_text(&crypto::uuid_v4()?)?,
        "data": key.encrypt_text(&data)?,
    });
    serde_json::to_string_pretty(&file).map_err(|_| crypto::CryptoError::Malformed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::Secret;

    #[test]
    fn csv_cells_keep_quotes_commas_and_newlines() {
        let rows = csv_rows("name,password\r\n\"Git, Hub\",\"p\"\"w\nx\"\r\n\r\nplain,1\n");
        let rows: Vec<Vec<String>> = rows.iter().map(|row| row.iter().map(|cell| cell.to_string()).collect()).collect();
        assert_eq!(rows, [vec!["name", "password"], vec!["Git, Hub", "p\"w\nx"], vec!["plain", "1"]]);
    }

    #[test]
    fn browser_and_keepass_exports_map_by_their_headers() {
        let chrome = "name,url,username,password,note\nGitHub,https://github.com/login,artem,hunter2,\n";
        let drafts = parse(chrome, None).expect("chrome");
        assert_eq!((drafts[0].name.as_str(), drafts[0].username.as_str()), ("GitHub", "artem"));
        assert_eq!(drafts[0].password.as_ref().map(Hidden::expose), Some("hunter2"));
        assert_eq!(drafts[0].sites[0].uri, "https://github.com/login");
        let firefox = "\"url\",\"username\",\"password\",\"httpRealm\"\n\"https://example.org\",\"me\",\"pw\",\"\"\n";
        let drafts = parse(firefox, None).expect("firefox");
        assert_eq!(drafts[0].name, "https://example.org", "a nameless row is named after its site");
        let keepass = "\"Group\",\"Title\",\"Username\",\"Password\",\"URL\",\"Notes\",\"TOTP\"\n\"Root/Work\",\"Jira\",\"a\",\"b\",\"\",\"n\",\"otpauth://totp/x?secret=JBSWY3DP\"\n";
        let drafts = parse(keepass, None).expect("keepass");
        assert_eq!(drafts[0].folder, "Root / Work");
        assert!(drafts[0].totp.is_some());
        assert_eq!(
            parse("a,b\n1,2\n", None).map(|_| ()),
            Err(ImportError::Unsupported("the file has no password or notes column".into()))
        );
    }

    fn record() -> Record {
        Record {
            kind: ItemKind::Login,
            name: "GitHub".into(),
            username: "artem-dev".into(),
            password: Secret::new(b"c7#Vq2!mWx9tLp4Rz8".to_vec()),
            totp: Secret::new(b"JBSWY3DPEHPK3PXP".to_vec()),
            sites: vec![Site { uri: "github.com".into(), matching: Matching::Host }],
            apps: vec![AppLink {
                name: "GitHub Desktop".into(),
                app_id: "io.github.desktop".into(),
                program: String::new(),
            }],
            folder: "Робота".into(),
            notes: "n".into(),
            ..Record::default()
        }
    }

    #[test]
    fn an_export_is_ciphertext_and_imports_back_with_its_password() {
        let original = record();
        let file = export(&[&original], "file password").expect("export");
        assert!(!file.contains("GitHub") && !file.contains("c7#Vq2") && !file.contains("artem-dev"));
        assert_eq!(parse(&file, None).map(|_| ()), Err(ImportError::NeedsPassword));
        assert_eq!(parse(&file, Some("wrong")).map(|_| ()), Err(ImportError::WrongPassword));
        let drafts = parse(&file, Some("file password")).expect("import");
        assert_eq!(drafts.len(), 1);
        let draft = &drafts[0];
        assert_eq!(
            (draft.name.as_str(), draft.username.as_str(), draft.folder.as_str()),
            ("GitHub", "artem-dev", "Робота")
        );
        assert_eq!(draft.password.as_ref().map(Hidden::expose), Some("c7#Vq2!mWx9tLp4Rz8"));
        assert_eq!(draft.sites, original.sites);
        assert_eq!(draft.apps, original.apps, "app links survive as glideapp:// URIs");
    }

    #[test]
    fn a_plain_bitwarden_export_keeps_cards_notes_and_hidden_fields() {
        let json = r#"{"encrypted":false,"folders":[{"id":"f","name":"Дім"}],"items":[
            {"type":3,"name":"Visa","folderId":"f","card":{"cardholderName":"A","brand":"Visa","number":"4111111111111111","code":"123","expMonth":"3","expYear":"2029"}},
            {"type":2,"name":"Коди","notes":"1 2 3","secureNote":{"type":0}},
            {"type":1,"name":"Wi-Fi","login":{"password":"x"},"fields":[{"name":"PIN","value":"1234","type":1},{"name":null,"type":3,"linkedId":100}]},
            {"type":5,"name":"SSH","sshKey":{}}]}"#;
        let drafts = parse(json, None).expect("import");
        assert_eq!(drafts.len(), 3, "types Glide cannot hold are skipped");
        assert_eq!(drafts[0].card.as_ref().map(|card| card.expiry.as_str()), Some("03/29"));
        assert_eq!(drafts[0].folder, "Дім");
        assert_eq!(drafts[1].kind, ItemKind::Note);
        assert_eq!(drafts[2].fields.len(), 1);
        assert!(drafts[2].fields[0].hidden);
    }

    #[test]
    fn an_account_bound_export_is_refused_with_a_reason() {
        let json = r#"{"encrypted":true,"encKeyValidation_DO_NOT_EDIT":"2.a|b|c","data":"2.a|b|c"}"#;
        assert!(matches!(parse(json, Some("pw")), Err(ImportError::Unsupported(_))));
    }
}

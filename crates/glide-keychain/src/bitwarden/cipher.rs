use std::collections::HashMap;

use glide_ipc::keychain::{AppLink, ItemKind, Matching, Passkey, Site};
use serde_json::{Map, Value, json};

use super::api::{field, list, number, text};
use super::crypto::{CryptoError, SymmetricKey, decrypt_optional};
use super::dates;
use crate::provider::Record;
use crate::vault::{PastSecret, Secret, StoredCard, StoredField};

const APP_SCHEME: &str = "glideapp://";
const HISTORY: usize = 5;

#[derive(Debug, Clone)]
pub struct Keys {
    pub user: SymmetricKey,
    pub orgs: HashMap<String, SymmetricKey>,
}

impl Keys {
    pub fn for_cipher(&self, raw: &Value) -> Result<SymmetricKey, CryptoError> {
        let base = match text(raw, "organizationId").filter(|id| !id.is_empty()) {
            Some(org) => self.orgs.get(org).ok_or(CryptoError::Decrypt)?,
            None => &self.user,
        };
        match text(raw, "key").filter(|key| !key.is_empty()) {
            Some(wrapped) => base.decrypt_key(wrapped),
            None => Ok(base.clone()),
        }
    }
}

const fn kind_of(code: u64) -> ItemKind {
    match code {
        2 => ItemKind::Note,
        3 => ItemKind::Card,
        4 => ItemKind::Identity,
        _ => ItemKind::Login,
    }
}

const fn code_of(kind: ItemKind) -> u64 {
    match kind {
        ItemKind::Login => 1,
        ItemKind::Note => 2,
        ItemKind::Card => 3,
        ItemKind::Identity => 4,
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

fn percent_encode(text: &str) -> String {
    text.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => (byte as char).to_string(),
            other => format!("%{other:02X}"),
        })
        .collect()
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && let Some(hex) = text.get(index + 1..index + 3)
            && let Ok(value) = u8::from_str_radix(hex, 16)
        {
            out.push(value);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[must_use]
pub fn app_uri(app: &AppLink) -> String {
    format!(
        "{APP_SCHEME}{}?program={}&name={}",
        percent_encode(&app.app_id),
        percent_encode(&app.program),
        percent_encode(&app.name)
    )
}

#[must_use]
pub fn parse_app_uri(uri: &str) -> Option<AppLink> {
    let rest = uri.strip_prefix(APP_SCHEME)?;
    let (id, query) = rest.split_once('?').unwrap_or((rest, ""));
    let mut app = AppLink { name: String::new(), app_id: percent_decode(id), program: String::new() };
    for pair in query.split('&') {
        match pair.split_once('=') {
            Some(("program", value)) => app.program = percent_decode(value),
            Some(("name", value)) => app.name = percent_decode(value),
            _ => {}
        }
    }
    Some(app)
}

fn secret(text: String) -> Secret {
    Secret::new(text.into_bytes())
}

fn date(raw: &Value, name: &str) -> Option<u64> {
    text(raw, name).and_then(dates::parse)
}

pub fn record<S: std::hash::BuildHasher>(
    raw: &Value, keys: &Keys, folders: &HashMap<String, String, S>,
) -> Result<Record, CryptoError> {
    let key = keys.for_cipher(raw)?;
    let decrypt = |value: Option<&str>| decrypt_optional(&key, value);
    let mut record = Record {
        id: text(raw, "id").unwrap_or_default().to_string(),
        kind: kind_of(number(raw, "type").unwrap_or(1)),
        name: decrypt(text(raw, "name"))?,
        notes: decrypt(text(raw, "notes"))?,
        favorite: field(raw, "favorite").and_then(Value::as_bool).unwrap_or(false),
        folder: text(raw, "folderId").and_then(|id| folders.get(id)).cloned().unwrap_or_default(),
        created: date(raw, "creationDate").or_else(|| date(raw, "revisionDate")).unwrap_or(0),
        modified: date(raw, "revisionDate").unwrap_or(0),
        deleted: date(raw, "deletedDate"),
        ..Record::default()
    };
    if let Some(login) = field(raw, "login").filter(|login| login.is_object()) {
        record.username = decrypt(text(login, "username"))?;
        record.password = secret(decrypt(text(login, "password"))?);
        record.totp = secret(decrypt(text(login, "totp"))?);
        for uri in list(login, "uris") {
            let value = decrypt(text(uri, "uri"))?;
            if value.is_empty() {
                continue;
            }
            match parse_app_uri(&value) {
                Some(app) => record.apps.push(app),
                None => record.sites.push(Site { uri: value, matching: matching_of(number(uri, "match")) }),
            }
        }
        for credential in list(login, "fido2Credentials") {
            record.passkeys.push(Passkey {
                rp_id: decrypt(text(credential, "rpId"))?,
                user_name: decrypt(text(credential, "userName"))?,
                created: date(credential, "creationDate").unwrap_or(0),
            });
        }
    }
    for entry in list(raw, "fields") {
        let kind = number(entry, "type").unwrap_or(0);
        if kind == 3 {
            continue;
        }
        record.fields.push(StoredField {
            name: decrypt(text(entry, "name"))?,
            value: secret(decrypt(text(entry, "value"))?),
            hidden: kind == 1,
        });
    }
    if let Some(card) = field(raw, "card").filter(|card| card.is_object()) {
        let month = decrypt(text(card, "expMonth"))?;
        let year = decrypt(text(card, "expYear"))?;
        let expiry = match (month.is_empty(), year.is_empty()) {
            (true, true) => String::new(),
            _ => format!("{month:0>2}/{}", year.get(year.len().saturating_sub(2)..).unwrap_or_default()),
        };
        record.card = Some(StoredCard {
            holder: decrypt(text(card, "cardholderName"))?,
            brand: decrypt(text(card, "brand"))?,
            number: secret(decrypt(text(card, "number"))?),
            code: secret(decrypt(text(card, "code"))?),
            expiry,
        });
    }
    for past in list(raw, "passwordHistory") {
        record.history.push(PastSecret {
            secret: secret(decrypt(text(past, "password"))?),
            until: date(past, "lastUsedDate").unwrap_or(0),
        });
    }
    Ok(record)
}

fn sealed(key: &SymmetricKey, value: &str) -> Result<Value, CryptoError> {
    if value.is_empty() { Ok(Value::Null) } else { key.encrypt_text(value).map(Value::String) }
}

fn sealed_bytes(key: &SymmetricKey, value: &Secret) -> Result<Value, CryptoError> {
    if value.expose().is_empty() { Ok(Value::Null) } else { key.encrypt(value.expose()).map(Value::String) }
}

const fn is_known_type(code: Option<u64>) -> bool {
    matches!(code, None | Some(1..=4))
}

fn split_expiry(expiry: &str) -> (String, String) {
    let (month, year) = expiry.split_once('/').unwrap_or((expiry, ""));
    let year = year.trim();
    let year = if year.len() == 2 { format!("20{year}") } else { year.to_string() };
    (month.trim().trim_start_matches('0').to_string(), year)
}

pub fn request(record: &Record, raw: Option<&Value>, keys: &Keys, folder: Option<&str>) -> Result<Value, CryptoError> {
    let empty = Value::Object(Map::new());
    let raw = raw.unwrap_or(&empty);
    let key = keys.for_cipher(raw)?;
    let mut body = raw.as_object().cloned().unwrap_or_default();
    for read_only in ["object", "edit", "viewPassword", "permissions", "collectionIds", "attachments"] {
        body.remove(read_only);
    }
    let known = is_known_type(number(raw, "type"));
    if known {
        body.insert("type".into(), json!(code_of(record.kind)));
    }
    body.insert("name".into(), sealed(&key, &record.name)?);
    body.insert("notes".into(), sealed(&key, &record.notes)?);
    body.insert("favorite".into(), json!(record.favorite));
    body.insert("folderId".into(), folder.map_or(Value::Null, |id| json!(id)));
    body.insert("lastKnownRevisionDate".into(), field(raw, "revisionDate").cloned().unwrap_or(Value::Null));
    if known && (record.kind == ItemKind::Login || field(raw, "login").is_some_and(Value::is_object)) {
        let mut login = field(raw, "login").and_then(Value::as_object).cloned().unwrap_or_default();
        login.insert("username".into(), sealed(&key, &record.username)?);
        login.insert("password".into(), sealed_bytes(&key, &record.password)?);
        login.insert("totp".into(), sealed_bytes(&key, &record.totp)?);
        let mut uris = Vec::new();
        for site in &record.sites {
            uris.push(json!({ "uri": key.encrypt_text(&site.uri)?, "match": code_of_matching(site.matching) }));
        }
        for app in &record.apps {
            uris.push(json!({ "uri": key.encrypt_text(&app_uri(app))?, "match": 5 }));
        }
        login.insert("uris".into(), Value::Array(uris));
        body.insert("login".into(), Value::Object(login));
    }
    let mut fields = Vec::new();
    for entry in &record.fields {
        fields.push(json!({
            "type": u8::from(entry.hidden),
            "name": sealed(&key, &entry.name)?,
            "value": sealed_bytes(&key, &entry.value)?,
        }));
    }
    fields.extend(list(raw, "fields").iter().filter(|entry| number(entry, "type") == Some(3)).cloned());
    body.insert("fields".into(), Value::Array(fields));
    match (record.kind, &record.card) {
        (ItemKind::Card, Some(card)) => {
            let (month, year) = split_expiry(&card.expiry);
            body.insert(
                "card".into(),
                json!({
                    "cardholderName": sealed(&key, &card.holder)?,
                    "brand": sealed(&key, &card.brand)?,
                    "number": sealed_bytes(&key, &card.number)?,
                    "code": sealed_bytes(&key, &card.code)?,
                    "expMonth": sealed(&key, &month)?,
                    "expYear": sealed(&key, &year)?,
                }),
            );
        }
        (ItemKind::Note, _) => {
            body.insert("secureNote".into(), json!({ "type": 0 }));
        }
        _ => {}
    }
    let mut history = Vec::new();
    for past in record.history.iter().rev().take(HISTORY) {
        history
            .push(json!({ "password": sealed_bytes(&key, &past.secret)?, "lastUsedDate": dates::format(past.until) }));
    }
    history.reverse();
    body.insert("passwordHistory".into(), if history.is_empty() { Value::Null } else { Value::Array(history) });
    Ok(Value::Object(body))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys() -> Keys {
        Keys { user: SymmetricKey::generate().expect("key"), orgs: HashMap::new() }
    }

    fn sample() -> Record {
        Record {
            kind: ItemKind::Login,
            name: "GitHub".into(),
            username: "artem-dev".into(),
            password: Secret::new(b"c7#Vq2!mWx9tLp4Rz8".to_vec()),
            totp: Secret::new(b"JBSWY3DPEHPK3PXP".to_vec()),
            notes: "Коди окремо.".into(),
            sites: vec![Site { uri: "github.com".into(), matching: Matching::Domain }],
            apps: vec![AppLink {
                name: "GitHub Desktop".into(),
                app_id: "io.github.desktop".into(),
                program: "/usr/bin/github desktop".into(),
            }],
            fields: vec![StoredField { name: "PIN".into(), value: Secret::new(b"1234".to_vec()), hidden: true }],
            folder: "Робота".into(),
            favorite: true,
            history: vec![PastSecret { secret: Secret::new(b"old".to_vec()), until: 1_704_371_696 }],
            ..Record::default()
        }
    }

    #[test]
    fn a_record_survives_the_trip_through_a_cipher() {
        let keys = keys();
        let folders = HashMap::from([("f1".to_string(), "Робота".to_string())]);
        let mut body = request(&sample(), None, &keys, Some("f1")).expect("request");
        body.as_object_mut().expect("object").insert("id".into(), json!("c1"));
        let text = serde_json::to_string(&body).expect("json");
        assert!(!text.contains("artem-dev") && !text.contains("c7#Vq2") && !text.contains("GitHub"));
        let back = record(&body, &keys, &folders).expect("record");
        let original = sample();
        assert_eq!(back.id, "c1");
        assert_eq!((back.name.as_str(), back.username.as_str()), ("GitHub", "artem-dev"));
        assert_eq!(back.password, original.password);
        assert_eq!(back.totp, original.totp);
        assert_eq!(back.sites, original.sites);
        assert_eq!(back.apps, original.apps);
        assert_eq!(back.fields, original.fields);
        assert_eq!(back.folder, "Робота");
        assert!(back.favorite);
        assert_eq!(back.history, original.history);
    }

    #[test]
    fn fields_the_app_does_not_know_are_kept_on_update() {
        let keys = keys();
        let raw = json!({
            "id": "c1", "type": 1, "revisionDate": "2024-01-04T12:34:56Z", "reprompt": 1,
            "login": { "fido2Credentials": [{"credentialId": "x"}] },
            "identity": null,
        });
        let body = request(&sample(), Some(&raw), &keys, None).expect("request");
        assert_eq!(body["reprompt"], json!(1));
        assert_eq!(body["login"]["fido2Credentials"][0]["credentialId"], json!("x"));
        assert_eq!(body["lastKnownRevisionDate"], json!("2024-01-04T12:34:56Z"));
    }

    #[test]
    fn an_item_type_the_app_does_not_know_keeps_its_type_and_linked_fields_survive() {
        let keys = keys();
        let ssh = json!({ "id": "k1", "type": 5, "sshKey": { "privateKey": "2.x|y|z" } });
        let record = Record { kind: ItemKind::Login, name: "Server key".into(), ..Record::default() };
        let body = request(&record, Some(&ssh), &keys, None).expect("request");
        assert_eq!(body["type"], json!(5));
        assert!(body.get("login").is_none(), "an SSH key must not become a login");
        assert_eq!(body["sshKey"]["privateKey"], json!("2.x|y|z"));
        let linked = json!({ "id": "c1", "type": 1, "fields": [{ "type": 3, "name": null, "linkedId": 100 }] });
        let body = request(&sample(), Some(&linked), &keys, None).expect("request");
        let fields = body["fields"].as_array().expect("fields");
        assert!(fields.iter().any(|entry| entry["linkedId"] == json!(100)));
        assert_eq!(fields.len(), 2);
    }

    #[test]
    fn cards_and_notes_map_to_their_types() {
        let keys = keys();
        let card = Record {
            kind: ItemKind::Card,
            name: "Visa".into(),
            card: Some(StoredCard {
                holder: "Artem".into(),
                brand: "Visa".into(),
                number: Secret::new(b"4111111111111111".to_vec()),
                code: Secret::new(b"123".to_vec()),
                expiry: "03/29".into(),
            }),
            ..Record::default()
        };
        let body = request(&card, None, &keys, None).expect("request");
        assert_eq!(body["type"], json!(3));
        let back = record(&body, &keys, &HashMap::new()).expect("record");
        assert_eq!(back.card.as_ref().map(|card| card.expiry.as_str()), Some("03/29"));
        let note = Record { kind: ItemKind::Note, name: "Коди".into(), notes: "1 2 3".into(), ..Record::default() };
        let body = request(&note, None, &keys, None).expect("request");
        assert_eq!((body["type"].clone(), body["secureNote"]["type"].clone()), (json!(2), json!(0)));
    }

    #[test]
    fn app_links_round_trip_through_their_uri() {
        let app = AppLink {
            name: "Telegram Desktop".into(),
            app_id: "org.telegram.desktop".into(),
            program: "/usr/bin/tele gram".into(),
        };
        assert_eq!(parse_app_uri(&app_uri(&app)), Some(app));
        assert_eq!(parse_app_uri("https://github.com"), None);
    }
}

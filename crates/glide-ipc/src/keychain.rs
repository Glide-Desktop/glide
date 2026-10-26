use std::fmt;

use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(transparent)]
pub struct Hidden(String);

impl Hidden {
    #[must_use]
    pub const fn new(text: String) -> Self {
        Self(text)
    }

    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for Hidden {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Hidden({} bytes)", self.0.len())
    }
}

pub const LOCAL: &str = "local";
pub const BUS_NAME: &str = "org.glide.Keychain";
pub const BUS_PATH: &str = "/org/glide/Keychain1";
pub const BUS_INTERFACE: &str = "org.glide.Keychain1";
pub const CHANGED: &str = "Changed";

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ItemRef {
    pub vault: String,
    pub id: String,
}

impl ItemRef {
    #[must_use]
    pub fn new(vault: impl Into<String>, id: impl Into<String>) -> Self {
        Self { vault: vault.into(), id: id.into() }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VaultKind {
    Local,
    Bitwarden,
    #[serde(rename = "keepass")]
    KeePass,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum SyncState {
    Local,
    Syncing,
    Synced { at: u64 },
    Offline { pending: u32 },
    Failed { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Vault {
    pub id: String,
    pub name: String,
    pub kind: VaultKind,
    pub account: String,
    pub server: String,
    pub exists: bool,
    pub unlocked: bool,
    pub sync: SyncState,
    pub count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ItemKind {
    #[default]
    Login,
    Note,
    Card,
    Identity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Matching {
    #[default]
    Domain,
    Host,
    StartsWith,
    Exact,
    Regex,
    Never,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Site {
    pub uri: String,
    #[serde(default)]
    pub matching: Matching,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AppLink {
    pub name: String,
    pub app_id: String,
    #[serde(default)]
    pub program: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Field {
    pub name: String,
    pub value: String,
    pub hidden: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldDraft {
    pub name: String,
    pub value: Hidden,
    pub hidden: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Passkey {
    pub rp_id: String,
    pub user_name: String,
    pub created: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Card {
    pub holder: String,
    pub brand: String,
    pub last_digits: String,
    pub expiry: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Summary {
    pub item: ItemRef,
    pub kind: ItemKind,
    pub name: String,
    pub username: String,
    pub folder: String,
    pub sites: Vec<String>,
    pub apps: Vec<String>,
    pub has_password: bool,
    pub has_totp: bool,
    pub has_passkey: bool,
    pub favorite: bool,
    pub created: u64,
    pub modified: u64,
    pub deleted: bool,
    pub strength: u8,
    pub reused: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Detail {
    pub summary: Summary,
    pub notes: String,
    pub sites: Vec<Site>,
    pub apps: Vec<AppLink>,
    pub fields: Vec<Field>,
    pub passkeys: Vec<Passkey>,
    pub history: u32,
    pub card: Option<Card>,
    pub password_length: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Draft {
    pub item: Option<ItemRef>,
    pub vault: String,
    pub kind: ItemKind,
    pub name: String,
    pub username: String,
    pub password: Option<Hidden>,
    pub totp: Option<Hidden>,
    pub folder: String,
    pub sites: Vec<Site>,
    pub apps: Vec<AppLink>,
    pub fields: Vec<FieldDraft>,
    pub notes: String,
    pub favorite: bool,
    pub card: Option<CardDraft>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CardDraft {
    pub holder: String,
    pub brand: String,
    pub number: Hidden,
    pub code: Hidden,
    pub expiry: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "name", rename_all = "kebab-case")]
pub enum SecretKind {
    Password,
    Totp,
    Field(String),
    CardNumber,
    CardCode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Code {
    pub code: String,
    pub seconds_left: u32,
    pub period: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Candidate {
    pub item: ItemRef,
    pub label: String,
    pub username: String,
    pub vault_name: String,
    pub has_totp: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Folder {
    pub vault: String,
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Send {
    pub vault: String,
    pub id: String,
    pub name: String,
    pub link: String,
    pub created: u64,
    pub expires: Option<u64>,
    pub deletion: u64,
    pub max_access: Option<u32>,
    pub access_count: u32,
    pub disabled: bool,
    pub has_password: bool,
    pub hide_text: bool,
    pub hide_email: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SendDraft {
    pub vault: String,
    pub name: String,
    pub text: Hidden,
    pub lifetime_hours: u32,
    pub max_access: Option<u32>,
    pub password: Option<Hidden>,
    pub hide_text: bool,
    pub hide_email: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Server {
    Us,
    Eu,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Login {
    pub server: String,
    pub email: String,
    pub password: Hidden,
    pub name: String,
    pub two_factor: Option<TwoFactor>,
    pub device_code: Option<String>,
    #[serde(default)]
    pub api_key: Option<ApiKey>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiKey {
    pub client_id: String,
    pub client_secret: Hidden,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TwoFactorMethod {
    Authenticator,
    Email,
    Yubikey,
    Duo,
    WebAuthn,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TwoFactor {
    pub method: TwoFactorMethod,
    pub code: String,
    pub remember: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conflict {
    pub item: ItemRef,
    pub local: Detail,
    pub local_modified: u64,
    pub remote: Detail,
    pub remote_modified: u64,
    pub local_password: Hidden,
    pub remote_password: Hidden,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Keep {
    Local,
    Remote,
    Both,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Report {
    pub weak: Vec<ItemRef>,
    pub reused: Vec<Vec<ItemRef>>,
    pub old: Vec<ItemRef>,
    pub leaked: Vec<ItemRef>,
    pub checked: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Request {
    Status,
    Vaults,
    Create {
        password: Hidden,
    },
    Unlock {
        vault: String,
        password: Hidden,
    },
    Lock {
        vault: Option<String>,
    },
    RequestUnlock {
        vault: Option<String>,
    },
    ChangePassword {
        vault: String,
        old: Hidden,
        new: Hidden,
    },
    Items,
    Item {
        item: ItemRef,
    },
    Secret {
        item: ItemRef,
        secret: SecretKind,
    },
    Totp {
        item: ItemRef,
    },
    Save {
        draft: Box<Draft>,
    },
    Trash {
        item: ItemRef,
    },
    Restore {
        item: ItemRef,
    },
    Purge {
        item: ItemRef,
    },
    Folders,
    Candidates {
        program: Option<String>,
        app_id: Option<String>,
    },
    Search {
        query: String,
        limit: u32,
    },
    Report,
    AddBitwarden {
        login: Box<Login>,
    },
    AddKeePass {
        path: String,
        password: Hidden,
        name: String,
    },
    SendLoginEmail {
        login: Box<Login>,
    },
    CheckLeaks,
    Import {
        vault: String,
        contents: Hidden,
        password: Option<Hidden>,
        #[serde(default)]
        binary: bool,
    },
    Export {
        vault: Option<String>,
        password: Hidden,
    },
    SecretService {
        enabled: bool,
    },
    RemoveVault {
        vault: String,
    },
    RenameVault {
        vault: String,
        name: String,
    },
    SetDefault {
        vault: String,
    },
    Sync {
        vault: Option<String>,
    },
    Conflicts,
    Resolve {
        item: ItemRef,
        keep: Keep,
    },
    Sends,
    CreateSend {
        draft: Box<SendDraft>,
    },
    SendText {
        vault: String,
        id: String,
    },
    EditSend {
        id: String,
        draft: Box<SendDraft>,
        keep_password: bool,
    },
    DisableSend {
        vault: String,
        id: String,
    },
    DeleteSend {
        vault: String,
        id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Response {
    Status { exists: bool, unlocked: bool },
    Vaults { list: Vec<Vault>, default: String },
    Done,
    WrongPassword,
    Locked,
    Items { list: Vec<Summary> },
    Item { detail: Box<Detail> },
    Secret { secret: Hidden },
    Code { code: Code },
    Saved { item: ItemRef },
    Folders { list: Vec<Folder> },
    Candidates { list: Vec<Candidate> },
    Report { report: Report },
    NeedsTwoFactor { methods: Vec<TwoFactorMethod> },
    NeedsDeviceCode,
    VaultAdded { vault: String },
    Conflicts { list: Vec<Conflict> },
    Sends { list: Vec<Send> },
    SendCreated { send: Box<Send> },
    Imported { count: u32, skipped: u32 },
    Exported { data: String, count: u32 },
    NeedsFilePassword,
    Unsupported,
    Error { message: String },
    UnlockRequested { vault: Option<String>, requester: Option<String> },
    Changed,
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Envelope<T> {
    pub id: u64,
    pub body: T,
}

pub const EVENT: u64 = 0;

const COMMON: [&str; 24] = [
    "password",
    "123456",
    "12345678",
    "qwerty",
    "admin",
    "letmein",
    "welcome",
    "iloveyou",
    "monkey",
    "dragon",
    "football",
    "abc123",
    "111111",
    "sunshine",
    "princess",
    "master",
    "login",
    "passw0rd",
    "starwars",
    "qwertyuiop",
    "1q2w3e4r",
    "zaq12wsx",
    "пароль",
    "йцукен",
];

#[must_use]
pub fn entropy_bits(password: &str) -> f64 {
    let chars: Vec<char> = password.chars().collect();
    if chars.is_empty() {
        return 0.0;
    }
    let lower = password.to_lowercase();
    if COMMON.iter().any(|common| lower.contains(common)) && chars.len() < 14 {
        return 10.0;
    }
    let words: Vec<&str> = password.split(['-', ' ', '_', '.']).filter(|word| !word.is_empty()).collect();
    let spelled = words.iter().filter(|word| word.chars().filter(|ch| ch.is_alphabetic()).count() >= 3).count();
    if words.len() >= 3 && spelled + 1 >= words.len() {
        return words.len() as f64 * 12.9 + 10.0;
    }
    let mut pool: f64 = 0.0;
    if chars.iter().any(char::is_ascii_lowercase) {
        pool += 26.0;
    }
    if chars.iter().any(char::is_ascii_uppercase) {
        pool += 26.0;
    }
    if chars.iter().any(char::is_ascii_digit) {
        pool += 10.0;
    }
    if chars.iter().any(|ch| ch.is_ascii_punctuation() || *ch == ' ') {
        pool += 33.0;
    }
    if chars.iter().any(|ch| !ch.is_ascii()) {
        pool += 66.0;
    }
    let mut distinct = chars.clone();
    distinct.sort_unstable();
    distinct.dedup();
    let repeats = chars.len() - distinct.len();
    let effective = chars.len() as f64 - repeats as f64 * 0.5;
    effective.max(1.0) * f64::log2(pool.max(2.0))
}

#[must_use]
pub fn strength(password: &str) -> u8 {
    let bits = entropy_bits(password);
    if password.is_empty() {
        0
    } else if bits < 40.0 {
        1
    } else if bits < 60.0 {
        2
    } else if bits < 90.0 {
        3
    } else {
        4
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_round_trips_through_one_json_line() {
        let request = Envelope {
            id: 7,
            body: Request::Unlock { vault: LOCAL.into(), password: Hidden::new("pa\"ss\nword".into()) },
        };
        let line = serde_json::to_string(&request).unwrap();
        assert!(!line.contains('\n'), "the channel is line based");
        assert_eq!(serde_json::from_str::<Envelope<Request>>(&line).unwrap(), request);
    }

    #[test]
    fn debug_hides_passwords_and_secrets() {
        let shown = format!(
            "{:?} {:?}",
            Request::Create { password: Hidden::new("hunter2".into()) },
            Response::Secret { secret: Hidden::new("hunter2".into()) }
        );
        assert!(!shown.contains("hunter2"), "{shown}");
    }

    #[test]
    fn a_draft_with_secrets_never_prints_them() {
        let draft = Draft {
            password: Some(Hidden::new("hunter2".into())),
            totp: Some(Hidden::new("JBSWY3DPEHPK3PXP".into())),
            ..Draft::default()
        };
        let shown = format!("{draft:?}");
        assert!(!shown.contains("hunter2") && !shown.contains("JBSWY3DP"), "{shown}");
    }

    #[test]
    fn strength_ranks_common_short_long_and_phrases() {
        assert_eq!(strength(""), 0);
        assert_eq!(strength("admin123"), 1);
        assert_eq!(strength("Hetzner2024"), 2);
        assert_eq!(strength("sunflower-orbit-42-kettle"), 3);
        assert_eq!(strength("c7#Vq2!mWx9tLp4Rz8"), 4);
        assert_eq!(strength("aaaaaaaaaaaa"), 1, "repeats count for little");
    }
}

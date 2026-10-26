use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde_json::{Value, json};
use zeroize::Zeroizing;

const TIMEOUT: Duration = Duration::from_secs(30);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(8);
const BODY_LIMIT: u64 = 256 * 1024 * 1024;
pub const CLIENT_ID: &str = "cli";
pub const DEVICE_TYPE: &str = "8";
pub const DEVICE_NAME: &str = "Glide";

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Endpoints {
    pub identity: String,
    pub api: String,
    pub web: String,
}

impl Endpoints {
    #[must_use]
    pub fn for_server(server: &str) -> Self {
        let base = server.trim().trim_end_matches('/');
        let cloud = |region: &str| Self {
            identity: format!("https://identity.bitwarden.{region}"),
            api: format!("https://api.bitwarden.{region}"),
            web: format!("https://vault.bitwarden.{region}"),
        };
        match base {
            "https://vault.bitwarden.com" | "https://bitwarden.com" => cloud("com"),
            "https://vault.bitwarden.eu" | "https://bitwarden.eu" => cloud("eu"),
            _ => Self { identity: format!("{base}/identity"), api: format!("{base}/api"), web: base.to_string() },
        }
    }

    #[must_use]
    pub fn send_link(&self, access_id: &str, secret: &[u8]) -> String {
        let key = URL_SAFE_NO_PAD.encode(secret);
        if self.web == "https://vault.bitwarden.com" {
            format!("https://send.bitwarden.com/#{access_id}/{key}")
        } else {
            format!("{}/#/send/{access_id}/{key}", self.web)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiError {
    Offline(String),
    Unauthorized,
    TwoFactor(Vec<u8>),
    DeviceVerification,
    WrongPassword,
    Rejected(String),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Offline(reason) => write!(f, "the server cannot be reached: {reason}"),
            Self::Unauthorized => f.write_str("the session has expired"),
            Self::TwoFactor(_) => f.write_str("two-step login is required"),
            Self::DeviceVerification => f.write_str("the server wants this device verified"),
            Self::WrongPassword => f.write_str("the email or master password is wrong"),
            Self::Rejected(message) => f.write_str(message),
        }
    }
}

#[must_use]
pub fn field<'a>(value: &'a Value, name: &str) -> Option<&'a Value> {
    let object = value.as_object()?;
    object.get(name).or_else(|| object.iter().find(|(key, _)| key.eq_ignore_ascii_case(name)).map(|(_, value)| value))
}

pub fn text<'a>(value: &'a Value, name: &str) -> Option<&'a str> {
    field(value, name).and_then(Value::as_str)
}

pub fn number(value: &Value, name: &str) -> Option<u64> {
    field(value, name).and_then(Value::as_u64)
}

pub fn list<'a>(value: &'a Value, name: &str) -> &'a [Value] {
    field(value, name).and_then(Value::as_array).map_or(&[], Vec::as_slice)
}

#[derive(Debug, Clone)]
pub struct Prelogin {
    pub kdf: u8,
    pub iterations: u32,
    pub memory: Option<u32>,
    pub parallelism: Option<u32>,
}

#[derive(Clone)]
pub struct Client {
    agent: ureq::Agent,
    pub endpoints: Endpoints,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client").field("endpoints", &self.endpoints).finish_non_exhaustive()
    }
}

fn message_of(body: &Value) -> String {
    let nested = field(body, "errorModel").and_then(|model| text(model, "message"));
    nested
        .or_else(|| text(body, "message"))
        .or_else(|| text(body, "error_description"))
        .or_else(|| text(body, "error"))
        .unwrap_or("the server refused the request")
        .to_string()
}

fn auth_error(body: &Value) -> ApiError {
    let providers: Vec<u8> = list(body, "TwoFactorProviders")
        .iter()
        .filter_map(|provider| {
            provider.as_u64().or_else(|| provider.as_str().and_then(|text| text.parse().ok())).map(|id| id as u8)
        })
        .collect();
    if !providers.is_empty() {
        return ApiError::TwoFactor(providers);
    }
    let message = message_of(body);
    let lower = message.to_lowercase();
    if lower.contains("new device") || lower.contains("device verification") {
        ApiError::DeviceVerification
    } else if lower.contains("username or password") || lower.contains("invalid_username_or_password") {
        ApiError::WrongPassword
    } else {
        ApiError::Rejected(message)
    }
}

impl Client {
    #[must_use]
    pub fn new(endpoints: Endpoints) -> Self {
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(TIMEOUT))
            .timeout_connect(Some(CONNECT_TIMEOUT))
            .user_agent(concat!("Glide-Passwords/", env!("CARGO_PKG_VERSION")))
            .build()
            .into();
        Self { agent, endpoints }
    }

    fn finish(response: Result<ureq::http::Response<ureq::Body>, ureq::Error>) -> Result<(u16, Value), ApiError> {
        let mut response = response.map_err(|err| ApiError::Offline(err.to_string()))?;
        let status = response.status().as_u16();
        let body = response
            .body_mut()
            .with_config()
            .limit(BODY_LIMIT)
            .read_to_string()
            .map_err(|err| ApiError::Offline(err.to_string()))?;
        let value =
            if body.trim().is_empty() { Value::Null } else { serde_json::from_str(&body).unwrap_or(Value::Null) };
        Ok((status, value))
    }

    fn checked(result: (u16, Value)) -> Result<Value, ApiError> {
        match result {
            (200..=299, value) => Ok(value),
            (401, _) => Err(ApiError::Unauthorized),
            (_, value) => Err(ApiError::Rejected(message_of(&value))),
        }
    }

    pub fn prelogin(&self, email: &str) -> Result<Prelogin, ApiError> {
        let url = format!("{}/accounts/prelogin", self.endpoints.identity);
        let body = Self::checked(Self::finish(self.agent.post(&url).send_json(json!({ "email": email })))?)?;
        Ok(Prelogin {
            kdf: number(&body, "kdf").unwrap_or(0) as u8,
            iterations: number(&body, "kdfIterations").unwrap_or(600_000) as u32,
            memory: number(&body, "kdfMemory").map(|value| value as u32),
            parallelism: number(&body, "kdfParallelism").map(|value| value as u32),
        })
    }

    pub fn token(&self, form: &[(&str, &str)], email: Option<&str>) -> Result<Value, ApiError> {
        let url = format!("{}/connect/token", self.endpoints.identity);
        let mut request = self.agent.post(&url);
        if let Some(email) = email {
            request = request.header("Auth-Email", URL_SAFE_NO_PAD.encode(email.as_bytes()));
        }
        let (status, body) = Self::finish(request.send_form(form.iter().copied()))?;
        match status {
            200..=299 => Ok(body),
            400 | 401 => Err(auth_error(&body)),
            _ => Err(ApiError::Rejected(message_of(&body))),
        }
    }

    fn send(&self, method: &str, path: &str, token: &str, body: Option<&Value>) -> Result<Value, ApiError> {
        let url = format!("{}{path}", self.endpoints.api);
        let bearer = format!("Bearer {token}");
        let result = match (method, body) {
            ("GET", _) => self.agent.get(&url).header("Authorization", &bearer).call(),
            ("DELETE", _) => self.agent.delete(&url).header("Authorization", &bearer).call(),
            ("POST", Some(body)) => self.agent.post(&url).header("Authorization", &bearer).send_json(body),
            ("POST", None) => self.agent.post(&url).header("Authorization", &bearer).send_empty(),
            ("PUT", Some(body)) => self.agent.put(&url).header("Authorization", &bearer).send_json(body),
            (_, _) => self.agent.put(&url).header("Authorization", &bearer).send_empty(),
        };
        Self::checked(Self::finish(result)?)
    }

    pub fn get(&self, path: &str, token: &str) -> Result<Value, ApiError> {
        self.send("GET", path, token, None)
    }

    pub fn post(&self, path: &str, token: &str, body: &Value) -> Result<Value, ApiError> {
        self.send("POST", path, token, Some(body))
    }

    pub fn put(&self, path: &str, token: &str, body: Option<&Value>) -> Result<Value, ApiError> {
        self.send("PUT", path, token, body)
    }

    pub fn delete(&self, path: &str, token: &str) -> Result<Value, ApiError> {
        self.send("DELETE", path, token, None)
    }

    pub fn post_public(&self, path: &str, body: &Value) -> Result<Value, ApiError> {
        let url = format!("{}{path}", self.endpoints.api);
        Self::checked(Self::finish(self.agent.post(&url).send_json(body))?)
    }

    pub fn register(&self, body: &Value) -> Result<Value, ApiError> {
        let url = format!("{}/accounts/register", self.endpoints.identity);
        Self::checked(Self::finish(self.agent.post(&url).send_json(body))?)
    }
}

#[derive(Debug, Clone, Default)]
pub struct Tokens {
    pub access: Zeroizing<String>,
    pub refresh: Zeroizing<String>,
    pub expires: u64,
}

impl Tokens {
    #[must_use]
    pub fn from_response(body: &Value, now: u64) -> Option<Self> {
        Some(Self {
            access: Zeroizing::new(text(body, "access_token")?.to_string()),
            refresh: Zeroizing::new(text(body, "refresh_token").unwrap_or_default().to_string()),
            expires: now + number(body, "expires_in").unwrap_or(3600).saturating_sub(60),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cloud_and_self_hosted_servers_resolve_their_endpoints() {
        let us = Endpoints::for_server("https://vault.bitwarden.com/");
        assert_eq!(us.identity, "https://identity.bitwarden.com");
        assert_eq!(us.api, "https://api.bitwarden.com");
        let eu = Endpoints::for_server("https://vault.bitwarden.eu");
        assert_eq!(eu.api, "https://api.bitwarden.eu");
        let own = Endpoints::for_server("https://vault.home.lan");
        assert_eq!(own.identity, "https://vault.home.lan/identity");
        assert_eq!(own.api, "https://vault.home.lan/api");
    }

    #[test]
    fn send_links_carry_the_key_after_the_access_id() {
        let own = Endpoints::for_server("https://vault.home.lan");
        assert_eq!(own.send_link("abc", &[0xfb, 0xff]), "https://vault.home.lan/#/send/abc/-_8");
        let us = Endpoints::for_server("https://vault.bitwarden.com");
        assert!(us.send_link("abc", &[1]).starts_with("https://send.bitwarden.com/#abc/"));
    }

    #[test]
    fn login_errors_are_told_apart() {
        let two = json!({"error": "invalid_grant", "TwoFactorProviders": [0, "1"]});
        assert_eq!(auth_error(&two), ApiError::TwoFactor(vec![0, 1]));
        let wrong = json!({"error_description": "invalid_username_or_password"});
        assert_eq!(auth_error(&wrong), ApiError::WrongPassword);
        let model = json!({"ErrorModel": {"Message": "Username or password is incorrect. Try again"}});
        assert_eq!(auth_error(&model), ApiError::WrongPassword);
        let device = json!({"error_description": "New device verification required."});
        assert_eq!(auth_error(&device), ApiError::DeviceVerification);
    }

    #[test]
    fn fields_are_found_in_either_case() {
        let body = json!({"Key": "a", "privateKey": "b", "kdfIterations": 5});
        assert_eq!(text(&body, "key"), Some("a"));
        assert_eq!(text(&body, "PrivateKey"), Some("b"));
        assert_eq!(number(&body, "KdfIterations"), Some(5));
    }
}

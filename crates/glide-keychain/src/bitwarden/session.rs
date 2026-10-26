use zeroize::Zeroizing;

use super::Account;
use super::api::{self, ApiError, Client, Tokens};
use super::crypto::SymmetricKey;

#[derive(Clone)]
pub struct Session {
    client: Client,
    device: String,
    email: String,
    user: SymmetricKey,
    password_hash: Zeroizing<String>,
    refresh: Option<String>,
    remember: Option<String>,
    api_client: Option<String>,
    api_secret: Option<String>,
    tokens: Option<Tokens>,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session").field("email", &self.email).finish_non_exhaustive()
    }
}

#[must_use]
pub fn device_form(device: &str) -> Vec<(&str, &str)> {
    vec![("deviceType", api::DEVICE_TYPE), ("deviceIdentifier", device), ("deviceName", api::DEVICE_NAME)]
}

impl Session {
    #[must_use]
    pub fn new(
        account: &Account, user: SymmetricKey, password_hash: Zeroizing<String>, tokens: Option<Tokens>,
    ) -> Self {
        Self {
            client: Client::new(api::Endpoints::for_server(&account.server)),
            device: account.device.clone(),
            email: account.email.clone(),
            user,
            password_hash,
            refresh: account.refresh.clone(),
            remember: account.remember.clone(),
            api_client: account.api_client.clone(),
            api_secret: account.api_secret.clone(),
            tokens,
        }
    }

    #[must_use]
    pub const fn client(&self) -> &Client {
        &self.client
    }

    #[must_use]
    pub const fn user(&self) -> &SymmetricKey {
        &self.user
    }

    #[must_use]
    pub fn sealed_refresh(&self) -> Option<String> {
        self.refresh.clone()
    }

    pub fn forget_token(&mut self) {
        self.tokens = None;
    }

    pub fn adopt_tokens(&mut self, other: &Self) {
        self.tokens.clone_from(&other.tokens);
        self.refresh.clone_from(&other.refresh);
    }

    fn reveal(&self, sealed: Option<&String>) -> Option<Zeroizing<String>> {
        sealed.and_then(|sealed| self.user.decrypt_text(sealed).ok()).map(Zeroizing::new)
    }

    fn attempts(&self) -> Vec<Vec<(&'static str, Zeroizing<String>)>> {
        let owned = |text: &str| Zeroizing::new(text.to_string());
        let mut attempts = Vec::new();
        if let Some(refresh) = self.reveal(self.refresh.as_ref()) {
            attempts.push(vec![
                ("grant_type", owned("refresh_token")),
                ("client_id", owned(api::CLIENT_ID)),
                ("refresh_token", refresh),
            ]);
        }
        if let (Some(client), Some(secret)) = (self.api_client.as_deref(), self.reveal(self.api_secret.as_ref())) {
            attempts.push(vec![
                ("grant_type", owned("client_credentials")),
                ("scope", owned("api")),
                ("client_id", owned(client)),
                ("client_secret", secret),
            ]);
        }
        let mut password = vec![
            ("grant_type", owned("password")),
            ("scope", owned("api offline_access")),
            ("client_id", owned(api::CLIENT_ID)),
            ("username", owned(&self.email)),
            ("password", self.password_hash.clone()),
        ];
        if let Some(remember) = self.reveal(self.remember.as_ref()) {
            password.extend([("twoFactorToken", remember), ("twoFactorProvider", owned("5"))]);
        }
        attempts.push(password);
        attempts
    }

    pub fn token(&mut self, now: u64) -> Result<Zeroizing<String>, ApiError> {
        if let Some(tokens) = self.tokens.as_ref().filter(|tokens| tokens.expires > now) {
            return Ok(tokens.access.clone());
        }
        let mut last = ApiError::Unauthorized;
        for attempt in self.attempts() {
            let mut form = device_form(&self.device);
            form.extend(attempt.iter().map(|(key, value)| (*key, value.as_str())));
            let password_grant = attempt.first().is_some_and(|(_, value)| value.as_str() == "password");
            match self.client.token(&form, password_grant.then_some(self.email.as_str())) {
                Ok(body) => {
                    let tokens = Tokens::from_response(&body, now).ok_or(ApiError::Unauthorized)?;
                    if !tokens.refresh.is_empty() {
                        self.refresh = self.user.encrypt_text(&tokens.refresh).ok();
                    }
                    let access = tokens.access.clone();
                    self.tokens = Some(tokens);
                    return Ok(access);
                }
                Err(ApiError::Offline(reason)) => return Err(ApiError::Offline(reason)),
                Err(other) => last = other,
            }
        }
        Err(last)
    }

    pub fn call<T>(&mut self, now: u64, action: impl Fn(&Client, &str) -> Result<T, ApiError>) -> Result<T, ApiError> {
        let token = self.token(now)?;
        match action(&self.client, &token) {
            Err(ApiError::Unauthorized) => {
                self.tokens = None;
                let token = self.token(now)?;
                action(&self.client, &token)
            }
            other => other,
        }
    }
}

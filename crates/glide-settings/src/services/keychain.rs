use std::sync::OnceLock;

const NAME: &str = "org.glide.Keychain";
const PATH: &str = "/org/glide/Keychain1";
const INTERFACE: &str = "org.glide.Keychain1";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Login {
    pub id: u64,
    pub label: String,
    pub username: String,
    pub app: String,
    pub program: String,
    pub modified: u64,
}

impl Login {
    #[must_use]
    pub fn matches(&self, query: &str) -> bool {
        let query = query.trim().to_lowercase();
        query.is_empty()
            || [&self.label, &self.username, &self.app, &self.program]
                .iter()
                .any(|field| field.to_lowercase().contains(&query))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum KeychainState {
    #[default]
    Loading,
    Unreachable,
    Missing,
    Locked,
    Open(Vec<Login>),
}

impl KeychainState {
    #[must_use]
    pub fn logins(&self) -> &[Login] {
        match self {
            Self::Open(logins) => logins,
            _ => &[],
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Draft {
    pub id: Option<u64>,
    pub label: String,
    pub username: String,
    pub password: String,
    pub app: String,
    pub program: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DraftProblem {
    NoLabel,
    NoTarget,
    NoPassword,
    LineBreak,
}

impl Draft {
    #[must_use]
    pub fn problem(&self) -> Option<DraftProblem> {
        if self.label.trim().is_empty() {
            Some(DraftProblem::NoLabel)
        } else if self.app.trim().is_empty() && self.program.trim().is_empty() {
            Some(DraftProblem::NoTarget)
        } else if self.id.is_none() && self.password.is_empty() {
            Some(DraftProblem::NoPassword)
        } else if self.password.contains(['\n', '\r']) {
            Some(DraftProblem::LineBreak)
        } else {
            None
        }
    }
}

fn connection() -> Result<&'static zbus::blocking::Connection, String> {
    static CONNECTION: OnceLock<Result<zbus::blocking::Connection, String>> = OnceLock::new();
    CONNECTION
        .get_or_init(|| zbus::blocking::Connection::session().map_err(|err| err.to_string()))
        .as_ref()
        .map_err(Clone::clone)
}

fn call<B, R>(method: &str, body: &B) -> Result<R, String>
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
    R: for<'d> zbus::zvariant::DynamicDeserialize<'d>,
{
    let proxy = zbus::blocking::Proxy::new(connection()?, NAME, PATH, INTERFACE).map_err(|err| err.to_string())?;
    proxy.call(method, body).map_err(|err| match err {
        zbus::Error::MethodError(_, Some(message), _) => message,
        other => other.to_string(),
    })
}

type Row = (u64, String, String, String, String, u64);

#[must_use]
pub fn read() -> KeychainState {
    match call::<_, (bool, bool)>("Status", &()) {
        Err(_) => KeychainState::Unreachable,
        Ok((false, _)) => KeychainState::Missing,
        Ok((true, false)) => KeychainState::Locked,
        Ok((true, true)) => match call::<_, Vec<Row>>("Items", &()) {
            Ok(rows) => {
                let mut logins: Vec<Login> = rows
                    .into_iter()
                    .map(|(id, label, username, app, program, modified)| Login {
                        id,
                        label,
                        username,
                        app,
                        program,
                        modified,
                    })
                    .collect();
                logins.sort_by_key(|login| login.label.to_lowercase());
                KeychainState::Open(logins)
            }
            Err(_) => KeychainState::Locked,
        },
    }
}

pub fn create(password: &str) -> Result<String, String> {
    call::<_, ()>("Create", &(password,)).map(|()| String::new())
}

pub fn request_unlock() -> Result<String, String> {
    call::<_, ()>("RequestUnlock", &()).map(|()| String::new())
}

pub fn lock() -> Result<String, String> {
    call::<_, ()>("Lock", &()).map(|()| String::new())
}

pub fn save(draft: &Draft) -> Result<String, String> {
    let fields =
        (draft.label.trim(), draft.username.trim(), draft.password.as_str(), draft.app.trim(), draft.program.trim());
    match draft.id {
        Some(id) => call::<_, ()>("Update", &(id, fields.0, fields.1, fields.2, fields.3, fields.4)),
        None => call::<_, u64>("Add", &fields).map(|_| ()),
    }
    .map(|()| String::new())
}

pub fn delete(id: u64) -> Result<String, String> {
    call::<_, ()>("Delete", &(id,)).map(|()| String::new())
}

pub fn password(id: u64) -> Result<String, String> {
    call::<_, String>("Password", &(id,))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft() -> Draft {
        Draft {
            label: "Telegram".into(),
            app: "org.telegram.desktop".into(),
            password: "pw".into(),
            ..Draft::default()
        }
    }

    #[test]
    fn a_draft_needs_a_name_an_app_and_a_password_when_new() {
        assert_eq!(draft().problem(), None);
        assert_eq!(Draft { label: " ".into(), ..draft() }.problem(), Some(DraftProblem::NoLabel));
        assert_eq!(Draft { app: String::new(), ..draft() }.problem(), Some(DraftProblem::NoTarget));
        assert_eq!(
            Draft { app: String::new(), program: "/usr/bin/telegram-desktop".into(), ..draft() }.problem(),
            None
        );
        assert_eq!(Draft { password: String::new(), ..draft() }.problem(), Some(DraftProblem::NoPassword));
        assert_eq!(
            Draft { id: Some(3), password: String::new(), ..draft() }.problem(),
            None,
            "an edit may keep the saved password"
        );
        assert_eq!(Draft { password: "a\nb".into(), ..draft() }.problem(), Some(DraftProblem::LineBreak));
    }

    #[test]
    fn search_looks_at_every_visible_field() {
        let login = Login { label: "Work mail".into(), username: "me@example.com".into(), ..Login::default() };
        assert!(login.matches(""));
        assert!(login.matches("MAIL"));
        assert!(login.matches("example"));
        assert!(!login.matches("telegram"));
    }
}

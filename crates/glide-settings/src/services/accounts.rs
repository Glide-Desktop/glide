use std::path::PathBuf;

use zbus::blocking::{Connection, Proxy};
use zbus::proxy::MethodFlags;
use zbus::zvariant::OwnedObjectPath;

use super::run;

const ACCOUNTS: &str = "org.freedesktop.Accounts";
const ACCOUNTS_PATH: &str = "/org/freedesktop/Accounts";
const USER: &str = "org.freedesktop.Accounts.User";

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Account {
    pub path: String,
    pub uid: u64,
    pub user_name: String,
    pub real_name: String,
    pub icon_file: String,
    pub administrator: bool,
    pub automatic_login: bool,
    pub locked: bool,
    pub login_frequency: u64,
    pub login_time: i64,
    pub shell: String,
    pub home: String,
    pub language: String,
}

impl Account {
    #[must_use]
    pub fn display_name(&self) -> &str {
        if self.real_name.trim().is_empty() { &self.user_name } else { self.real_name.trim() }
    }

    #[must_use]
    pub fn icon(&self) -> Option<PathBuf> {
        let path = PathBuf::from(&self.icon_file);
        (!self.icon_file.is_empty() && path.is_file()).then_some(path)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Session {
    pub id: String,
    pub user: String,
    pub seat: String,
    pub tty: String,
    pub kind: String,
    pub class: String,
    pub state: String,
    pub remote: bool,
    pub service: String,
    pub started: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UsersState {
    pub available: bool,
    pub me: Option<Account>,
    pub others: Vec<Account>,
    pub sessions: Vec<Session>,
    pub current_session: String,
    pub linger: Option<bool>,
    pub groups: Vec<String>,
    pub faces: Vec<PathBuf>,
}

#[must_use]
pub fn parse_session_ids(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| line.split_whitespace().next())
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .collect()
}

#[must_use]
pub(crate) fn parse_properties(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.trim().to_string(), value.trim().to_string()))
        .collect()
}

#[must_use]
pub fn parse_session(text: &str) -> Session {
    let mut session = Session::default();
    for (key, value) in parse_properties(text) {
        match key.as_str() {
            "Id" => session.id = value,
            "Name" => session.user = value,
            "Seat" => session.seat = value,
            "TTY" => session.tty = value,
            "Type" => session.kind = value,
            "Class" => session.class = value,
            "State" => session.state = value,
            "Remote" => session.remote = value == "yes",
            "Service" => session.service = value,
            "Timestamp" => session.started = parse_timestamp(&value, &super::tz::Zone::system()),
            _ => {}
        }
    }
    session
}

#[must_use]
pub fn parse_timestamp(text: &str, zone: &super::tz::Zone) -> Option<i64> {
    let mut parts = text.split_whitespace().skip(1);
    let (date, time) = (parts.next()?, parts.next()?);
    let date: Vec<&str> = date.split('-').collect();
    let time: Vec<&str> = time.split(':').collect();
    let ([year, month, day], [hour, minute, second]) = (date.as_slice(), time.as_slice()) else {
        return None;
    };
    let local = super::tz::from_civil(
        year.parse().ok()?,
        month.parse().ok()?,
        day.parse().ok()?,
        hour.parse().ok()?,
        minute.parse().ok()?,
        second.parse().ok()?,
    );
    Some(zone.to_utc(local))
}

fn user_proxy<'a>(bus: &'a Connection, path: &'a str) -> zbus::Result<Proxy<'a>> {
    zbus::blocking::proxy::Builder::new(bus)
        .destination(ACCOUNTS)?
        .path(path)?
        .interface(USER)?
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
}

fn read_account(bus: &Connection, path: &str) -> zbus::Result<Account> {
    let user = user_proxy(bus, path)?;
    let text = |name: &str| user.get_property::<String>(name).unwrap_or_default();
    Ok(Account {
        path: path.to_string(),
        uid: user.get_property::<u64>("Uid")?,
        user_name: text("UserName"),
        real_name: text("RealName"),
        icon_file: text("IconFile"),
        administrator: user.get_property::<i32>("AccountType").unwrap_or(0) == 1,
        automatic_login: user.get_property::<bool>("AutomaticLogin").unwrap_or(false),
        locked: user.get_property::<bool>("Locked").unwrap_or(false),
        login_frequency: user.get_property::<u64>("LoginFrequency").unwrap_or(0),
        login_time: user.get_property::<i64>("LoginTime").unwrap_or(0),
        shell: text("Shell"),
        home: text("HomeDirectory"),
        language: text("Language"),
    })
}

fn current_uid() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status
        .lines()
        .find_map(|line| line.strip_prefix("Uid:"))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|uid| uid.parse().ok())
}

fn read_accounts() -> zbus::Result<(Option<Account>, Vec<Account>)> {
    let bus = Connection::system()?;
    let manager = Proxy::new(&bus, ACCOUNTS, ACCOUNTS_PATH, ACCOUNTS)?;
    let uid = current_uid().unwrap_or(u64::MAX);
    let me = manager
        .call::<_, _, OwnedObjectPath>("FindUserById", &(uid.cast_signed(),))
        .ok()
        .and_then(|path| read_account(&bus, path.as_str()).ok());
    let paths: Vec<OwnedObjectPath> = manager.call("ListCachedUsers", &())?;
    let mut others: Vec<Account> = paths
        .iter()
        .filter_map(|path| read_account(&bus, path.as_str()).ok())
        .filter(|account| account.uid != uid)
        .collect();
    others.sort_by_cached_key(|account| account.display_name().to_lowercase());
    Ok((me, others))
}

fn read_faces() -> Vec<PathBuf> {
    let mut faces: Vec<PathBuf> = ["/usr/share/pixmaps/faces", "/usr/share/plasma/avatars"]
        .iter()
        .filter_map(|dir| std::fs::read_dir(dir).ok())
        .flat_map(Iterator::flatten)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| matches!(ext.to_ascii_lowercase().as_str(), "png" | "jpg" | "jpeg"))
        })
        .collect();
    faces.sort();
    faces.truncate(24);
    faces
}

const SESSION_PROPERTIES: [&str; 10] =
    ["Id", "Name", "Seat", "TTY", "Type", "Class", "State", "Remote", "Service", "Timestamp"];

#[must_use]
pub fn read() -> UsersState {
    let (me, others, available) = match read_accounts() {
        Ok((me, others)) => (me, others, true),
        Err(_) => (None, Vec::new(), false),
    };
    let sessions: Vec<Session> = run::output("loginctl", &["list-sessions", "--no-legend"])
        .map(|text| parse_session_ids(&text))
        .unwrap_or_default()
        .into_iter()
        .filter_map(|id| {
            let mut args = vec!["show-session", id.as_str()];
            for property in SESSION_PROPERTIES {
                args.extend(["-p", property]);
            }
            run::output("loginctl", &args)
        })
        .map(|text| parse_session(&text))
        .filter(|session| session.class != "manager" && session.class != "manager-early")
        .collect();
    let user = me.as_ref().map(|me| me.user_name.clone()).or_else(|| std::env::var("USER").ok()).unwrap_or_default();
    let linger = (!user.is_empty())
        .then(|| run::output("loginctl", &["show-user", &user, "-p", "Linger", "--value"]))
        .flatten()
        .map(|text| text.trim() == "yes");
    let groups = run::output("id", &["-Gn"])
        .map(|text| text.split_whitespace().map(str::to_string).collect())
        .unwrap_or_default();
    UsersState {
        available,
        me,
        others,
        sessions,
        current_session: std::env::var("XDG_SESSION_ID").unwrap_or_default(),
        linger,
        groups,
        faces: read_faces(),
    }
}

fn describe(err: zbus::Error) -> String {
    match err {
        zbus::Error::MethodError(name, message, _) => {
            let detail = message.unwrap_or_default();
            if name.as_str().ends_with("PermissionDenied") || name.as_str().contains("NotAuthorized") {
                format!("Permission denied. {detail}").trim().to_string()
            } else {
                format!("{detail} ({name})")
            }
        }
        other => format!("AccountsService: {other}"),
    }
}

fn call_user_with<B>(path: &str, method: &str, body: &B) -> Result<String, String>
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
{
    let result = (|| -> zbus::Result<()> {
        let bus = Connection::system()?;
        let user = user_proxy(&bus, path)?;
        user.call_with_flags::<_, _, ()>(method, MethodFlags::AllowInteractiveAuth.into(), body).map(|_| ())
    })();
    result.map(|()| String::new()).map_err(describe)
}

fn call_user(path: &str, method: &str, argument: &str) -> Result<String, String> {
    call_user_with(path, method, &(argument,))
}

fn call_manager<B, R>(method: &str, body: &B) -> Result<R, String>
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
    R: serde::de::DeserializeOwned + zbus::zvariant::Type,
{
    let result = (|| -> zbus::Result<Option<R>> {
        let bus = Connection::system()?;
        let manager = Proxy::new(&bus, ACCOUNTS, ACCOUNTS_PATH, ACCOUNTS)?;
        manager.call_with_flags::<_, _, R>(method, MethodFlags::AllowInteractiveAuth.into(), body)
    })();
    result.map_err(describe)?.ok_or_else(|| format!("AccountsService: {method} returned nothing"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserNameProblem {
    Empty,
    TooLong,
    BadStart,
    BadCharacter,
}

#[must_use]
pub fn user_name_problem(name: &str) -> Option<UserNameProblem> {
    let Some(first) = name.chars().next() else {
        return Some(UserNameProblem::Empty);
    };
    if name.chars().count() > 32 {
        return Some(UserNameProblem::TooLong);
    }
    if !(first.is_ascii_lowercase() || first == '_') {
        return Some(UserNameProblem::BadStart);
    }
    let allowed = |ch: char| ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '_' | '-');
    (!name.chars().all(allowed)).then_some(UserNameProblem::BadCharacter)
}

#[must_use]
pub fn suggest_user_name(full_name: &str) -> String {
    let word = full_name.split_whitespace().next().unwrap_or_default();
    let name: String = word
        .chars()
        .flat_map(char::to_lowercase)
        .filter(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit())
        .take(32)
        .collect();
    match name.chars().next() {
        Some(first) if first.is_ascii_lowercase() => name,
        _ => String::new(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasswordProblem {
    Empty,
    Mismatch,
    LineBreak,
}

#[must_use]
pub fn password_problem(password: &str, confirm: &str) -> Option<PasswordProblem> {
    if password.is_empty() {
        Some(PasswordProblem::Empty)
    } else if password.contains(['\n', '\r']) {
        Some(PasswordProblem::LineBreak)
    } else if password != confirm {
        Some(PasswordProblem::Mismatch)
    } else {
        None
    }
}

pub fn hash_password(password: &str) -> Result<String, String> {
    use std::io::Write;
    use std::process::{Command, Stdio};
    if password.contains(['\n', '\r']) {
        return Err("A password cannot contain a line break".to_string());
    }
    let mut child = Command::new("openssl")
        .args(["passwd", "-6", "-stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("openssl: {err}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(password.as_bytes()).and_then(|()| stdin.write_all(b"\n")).map_err(|err| err.to_string())?;
    }
    let output = child.wait_with_output().map_err(|err| format!("openssl: {err}"))?;
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines()
        .map(str::trim)
        .find(|line| line.starts_with("$6$"))
        .map(str::to_string)
        .ok_or_else(|| "openssl could not hash the password".to_string())
}

pub(crate) fn set_password(path: &str, password: &str) -> Result<String, String> {
    let hash = hash_password(password)?;
    call_user_with(path, "SetPassword", &(hash, String::new()))
}

pub(crate) fn tell_the_shell_about(password: String) {
    let password = glide_ipc::keychain::Hidden::new(password);
    if let Err(err) = crate::services::ipc::request(&glide_ipc::Request::LoginPasswordChanged { password }) {
        eprintln!("glide-settings: the shell did not take the new login password: {err}");
    }
}

pub(crate) fn set_account_type(path: &str, administrator: bool) -> Result<String, String> {
    call_user_with(path, "SetAccountType", &(i32::from(administrator),))
}

pub(crate) fn create_user(
    user_name: &str, real_name: &str, administrator: bool, password: &str,
) -> Result<String, String> {
    if let Some(problem) = user_name_problem(user_name) {
        return Err(format!("{problem:?}"));
    }
    let real_name = clean_real_name(real_name).ok_or_else(|| "That name is too long".to_string())?;
    let hash = hash_password(password)?;
    let path: OwnedObjectPath =
        call_manager("CreateUser", &(user_name.to_string(), real_name, i32::from(administrator)))?;
    call_user_with(path.as_str(), "SetPassword", &(hash, String::new()))
}

pub(crate) fn delete_user(uid: u64, remove_files: bool) -> Result<String, String> {
    call_manager::<_, ()>("DeleteUser", &(uid.cast_signed(), remove_files)).map(|()| String::new())
}

#[must_use]
pub fn clean_real_name(name: &str) -> Option<String> {
    let name: String = name.chars().filter(|c| !c.is_control() && *c != ':').collect();
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    (name.chars().count() <= 255).then_some(name)
}

pub(crate) fn set_real_name(path: &str, name: &str) -> Result<String, String> {
    let name = clean_real_name(name).ok_or_else(|| "That name is too long".to_string())?;
    call_user(path, "SetRealName", &name)
}

pub fn set_icon(path: &str, file: &str) -> Result<String, String> {
    let file = super::vpn::expand_home(file);
    if !file.is_file() {
        return Err(format!("{} is not a file", file.display()));
    }
    let valid = image::ImageReader::open(&file)
        .and_then(image::ImageReader::with_guessed_format)
        .ok()
        .and_then(|reader| reader.into_dimensions().ok())
        .is_some();
    if !valid {
        return Err(format!("{} is not a PNG or JPEG image", file.display()));
    }
    call_user(path, "SetIconFile", &file.to_string_lossy())
}

pub(crate) fn set_linger(user: &str, on: bool) -> Result<String, String> {
    let verb = if on { "enable-linger" } else { "disable-linger" };
    run::status("loginctl", &[verb, user])
}

pub(crate) fn terminate_session(id: &str) -> Result<String, String> {
    run::status("loginctl", &["terminate-session", id])
}

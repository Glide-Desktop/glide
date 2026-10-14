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

#[must_use]
pub fn initials(name: &str) -> String {
    let words: Vec<&str> = name.split_whitespace().collect();
    let letters: String = match words.as_slice() {
        [] => String::new(),
        [one] => one.chars().take(1).collect(),
        [first, .., last] => first.chars().take(1).chain(last.chars().take(1)).collect(),
    };
    letters.to_uppercase()
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
    pub since: String,
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
pub fn parse_properties(text: &str) -> Vec<(String, String)> {
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
            "Timestamp" => session.since = value,
            _ => {}
        }
    }
    session
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

fn call_user(path: &str, method: &str, argument: &str) -> Result<String, String> {
    let result = (|| -> zbus::Result<()> {
        let bus = Connection::system()?;
        let user = user_proxy(&bus, path)?;
        user.call_with_flags::<_, _, ()>(method, MethodFlags::AllowInteractiveAuth.into(), &(argument,)).map(|_| ())
    })();
    result.map(|()| String::new()).map_err(|err| match err {
        zbus::Error::MethodError(name, message, _) => {
            let detail = message.unwrap_or_default();
            if name.as_str().ends_with("PermissionDenied") || name.as_str().contains("NotAuthorized") {
                format!("Permission denied. {detail}").trim().to_string()
            } else {
                format!("{detail} ({name})")
            }
        }
        other => format!("AccountsService: {other}"),
    })
}

#[must_use]
pub fn clean_real_name(name: &str) -> Option<String> {
    let name: String = name.chars().filter(|c| !c.is_control() && *c != ':').collect();
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    (name.chars().count() <= 255).then_some(name)
}

pub fn set_real_name(path: &str, name: &str) -> Result<String, String> {
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

pub fn set_linger(user: &str, on: bool) -> Result<String, String> {
    let verb = if on { "enable-linger" } else { "disable-linger" };
    run::status("loginctl", &[verb, user])
}

pub fn terminate_session(id: &str) -> Result<String, String> {
    run::status("loginctl", &["terminate-session", id])
}

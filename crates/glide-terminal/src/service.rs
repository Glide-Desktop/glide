use std::path::PathBuf;
use std::sync::{Mutex, OnceLock, PoisonError};

use glide_app::integration::{TERMINAL_BUS, TERMINAL_PATH};
use iced::futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use zbus::blocking::Connection;
use zbus::fdo::{RequestNameFlags, RequestNameReply};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenTab {
    pub directory: Option<PathBuf>,
    pub command: Option<(String, Vec<String>)>,
    pub token: Option<String>,
}

impl OpenTab {
    #[must_use]
    pub fn new(directory: &str, mut argv: Vec<String>, token: &str) -> Self {
        let directory = (!directory.is_empty()).then(|| crate::app::uri_path(directory));
        let command = (!argv.is_empty()).then(|| {
            let program = argv.remove(0);
            (program, argv)
        });
        Self { directory, command, token: (!token.is_empty()).then(|| token.to_string()) }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    OpenTab(OpenTab),
    FocusTab(u64),
    NewTab,
}

fn quicklist(requests: UnboundedSender<Request>) -> glide_app::quicklist::QuickMenu {
    let lang = crate::i18n::Lang::from_env();
    let labels =
        vec![crate::i18n::Key::NewTab.text(lang).to_string(), crate::i18n::Key::NewWindow.text(lang).to_string()];
    glide_app::quicklist::QuickMenu::new(labels, move |index| {
        if index == 0 {
            let _ = requests.unbounded_send(Request::NewTab);
            return;
        }
        let program = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("glide-terminal"));
        if let Ok(mut child) = std::process::Command::new(program).spawn() {
            let _ = std::thread::Builder::new().name("glide-terminal-window".into()).spawn(move || child.wait());
        }
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabEntry {
    pub id: u64,
    pub title: String,
    pub directory: String,
}

static TABS: Mutex<Vec<TabEntry>> = Mutex::new(Vec::new());

pub fn publish_tabs(tabs: Vec<TabEntry>) {
    *TABS.lock().unwrap_or_else(PoisonError::into_inner) = tabs;
}

const SEARCH_NAME: &str = "org.glide.Terminal.Search";
const SEARCH_PATH: &str = "/org/glide/Terminal/SearchProvider";

struct Tabs {
    requests: UnboundedSender<Request>,
}

impl glide_app::search_provider::Source for Tabs {
    fn search(&self, terms: &[String]) -> Vec<glide_app::search_provider::Item> {
        TABS.lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .filter(|tab| glide_app::search_provider::matches(&format!("{} {}", tab.title, tab.directory), terms))
            .map(|tab| glide_app::search_provider::Item {
                id: tab.id.to_string(),
                name: tab.title.clone(),
                description: tab.directory.clone(),
            })
            .collect()
    }

    fn activate(&self, id: &str) {
        if let Ok(id) = id.parse() {
            let _ = self.requests.unbounded_send(Request::FocusTab(id));
        }
    }
}

struct Terminal {
    requests: UnboundedSender<Request>,
}

#[zbus::interface(name = "org.glide.Terminal1")]
impl Terminal {
    fn open_tab(&self, directory: &str, argv: Vec<String>, activation_token: &str) {
        let _ = self.requests.unbounded_send(Request::OpenTab(OpenTab::new(directory, argv, activation_token)));
    }
}

struct Service {
    _connection: Connection,
    requests: Mutex<Option<UnboundedReceiver<Request>>>,
}

static SERVICE: OnceLock<Service> = OnceLock::new();

fn serve(builder: zbus::blocking::connection::Builder<'_>) -> Option<(Connection, UnboundedReceiver<Request>)> {
    let (sender, receiver) = unbounded();
    let tabs = glide_app::search_provider::Provider::new(Tabs { requests: sender.clone() });
    let menu = quicklist(sender.clone());
    let connection = builder
        .serve_at(TERMINAL_PATH, Terminal { requests: sender })
        .ok()?
        .serve_at(SEARCH_PATH, tabs)
        .ok()?
        .serve_at(glide_app::quicklist::PATH, menu)
        .ok()?
        .build()
        .ok()?;
    match connection.request_name_with_flags(TERMINAL_BUS, RequestNameFlags::DoNotQueue.into()) {
        Ok(RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner) => {
            let _ = connection.request_name_with_flags(SEARCH_NAME, RequestNameFlags::DoNotQueue.into());
            glide_app::quicklist::keep_announced(&connection, "glide-terminal");
            Some((connection, receiver))
        }
        _ => None,
    }
}

pub fn claim() -> bool {
    let Some((connection, receiver)) = zbus::blocking::connection::Builder::session().ok().and_then(serve) else {
        return false;
    };
    SERVICE.set(Service { _connection: connection, requests: Mutex::new(Some(receiver)) }).is_ok()
}

#[must_use]
pub fn requests() -> UnboundedReceiver<Request> {
    SERVICE
        .get()
        .and_then(|service| service.requests.lock().unwrap_or_else(PoisonError::into_inner).take())
        .unwrap_or_else(|| unbounded().1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_names_a_directory_a_command_and_a_token() {
        let tab = OpenTab::new("/tmp", vec!["htop".into(), "-d".into(), "5".into()], "tok");
        assert_eq!(tab.directory, Some(PathBuf::from("/tmp")));
        assert_eq!(tab.command, Some(("htop".into(), vec!["-d".into(), "5".into()])));
        assert_eq!(tab.token.as_deref(), Some("tok"));
    }

    #[test]
    fn a_second_terminal_cannot_take_the_name_and_calls_reach_the_first() {
        use std::io::BufRead;
        let Ok(mut daemon) = std::process::Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
        else {
            return;
        };
        let mut address = String::new();
        if let Some(out) = daemon.stdout.take() {
            let _ = std::io::BufReader::new(out).read_line(&mut address);
        }
        let builder = || zbus::blocking::connection::Builder::address(address.trim()).expect("address");
        let (_first, mut requests) = serve(builder()).expect("the first terminal owns the name");
        assert!(serve(builder()).is_none(), "a second terminal must not steal the name");

        let client = builder().build().expect("client");
        client
            .call_method(
                Some(TERMINAL_BUS),
                TERMINAL_PATH,
                Some(glide_app::integration::TERMINAL_INTERFACE),
                "OpenTab",
                &("/srv", vec!["top".to_string()], "tok"),
            )
            .expect("call");
        let Request::OpenTab(request) = requests.try_recv().expect("a queued request") else {
            panic!("expected an OpenTab request");
        };
        assert_eq!(request.directory, Some(PathBuf::from("/srv")));
        assert_eq!(request.command, Some(("top".into(), Vec::new())));

        publish_tabs(vec![TabEntry { id: 7, title: "cargo build".into(), directory: "~/Glide".into() }]);
        let reply = client
            .call_method(
                Some(SEARCH_NAME),
                SEARCH_PATH,
                Some(glide_app::search_provider::INTERFACE),
                "GetInitialResultSet",
                &(vec!["glide".to_string()],),
            )
            .expect("search");
        let ids: Vec<String> = reply.body().deserialize().expect("ids");
        assert_eq!(ids, ["7"]);
        client
            .call_method(
                Some(SEARCH_NAME),
                SEARCH_PATH,
                Some(glide_app::search_provider::INTERFACE),
                "ActivateResult",
                &("7", vec!["glide".to_string()], 0_u32),
            )
            .expect("activate");
        assert_eq!(requests.try_recv().expect("focus"), Request::FocusTab(7));
        let _ = daemon.kill();
        let _ = daemon.wait();
    }

    #[test]
    fn empty_fields_mean_the_defaults() {
        let tab = OpenTab::new("", Vec::new(), "");
        assert_eq!(tab, OpenTab { directory: None, command: None, token: None });
    }
}

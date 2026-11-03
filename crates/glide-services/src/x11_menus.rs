use std::time::{Duration, Instant};

use glide_appmenu::Source;
use x11rb::{
    connection::Connection,
    protocol::xproto::{Atom, AtomEnum, ConnectionExt, Window},
    rust_connection::RustConnection,
};

const REFRESH: Duration = Duration::from_millis(1500);
const RECONNECT: Duration = Duration::from_secs(5);
const PROPERTY_LIMIT: u32 = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct X11Window {
    pub id: u32,
    pub instance: String,
    pub class: String,
    pub title: String,
    pub pid: Option<u32>,
    pub gtk: Option<Source>,
}

struct Atoms {
    net_client_list: Atom,
    net_wm_name: Atom,
    net_wm_pid: Atom,
    utf8_string: Atom,
    compound_text: Atom,
    gtk_bus: Atom,
    gtk_menubar: Atom,
    gtk_app: Atom,
    gtk_window: Atom,
}

struct Live {
    connection: RustConnection,
    root: Window,
    atoms: Atoms,
}

type Shared = std::sync::Arc<std::sync::Mutex<Vec<X11Window>>>;

#[derive(Debug, Default)]
pub struct X11Bridge {
    shared: Shared,
    started: bool,
}

#[derive(Default)]
struct Poller {
    live: Option<Live>,
    last_attempt: Option<Instant>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Encoding {
    Utf8,
    Latin1,
    Compound,
}

fn latin1(raw: &[u8]) -> String {
    raw.iter().map(|&byte| char::from(byte)).collect()
}

fn compound(raw: &[u8]) -> String {
    let mut out = String::new();
    let mut utf8 = false;
    let mut index = 0;
    while index < raw.len() {
        if raw[index] == 0x1b {
            let rest = &raw[index + 1..];
            if rest.starts_with(b"%G") {
                utf8 = true;
            } else if rest.starts_with(b"%@") {
                utf8 = false;
            }
            index += 1;
            while index < raw.len() && (0x20..=0x2f).contains(&raw[index]) {
                index += 1;
            }
            index += 1;
            continue;
        }
        let end = raw[index..].iter().position(|&byte| byte == 0x1b).map_or(raw.len(), |at| index + at);
        let run = &raw[index..end];
        if utf8 {
            out.push_str(&String::from_utf8_lossy(run));
        } else {
            out.push_str(&latin1(run));
        }
        index = end;
    }
    out
}

fn decode(raw: &[u8], encoding: Encoding) -> String {
    match encoding {
        Encoding::Utf8 => String::from_utf8_lossy(raw).into_owned(),
        Encoding::Latin1 => latin1(raw),
        Encoding::Compound => compound(raw),
    }
}

fn words(raw: &[u8]) -> impl Iterator<Item = u32> + '_ {
    raw.chunks_exact(4).map(|c| u32::from_ne_bytes([c[0], c[1], c[2], c[3]]))
}

fn intern(connection: &RustConnection, name: &str) -> Option<Atom> {
    connection.intern_atom(false, name.as_bytes()).ok()?.reply().ok().map(|reply| reply.atom)
}

fn connect() -> Option<Live> {
    std::env::var_os("DISPLAY")?;
    let (connection, screen) = x11rb::connect(None).ok()?;
    let root = connection.setup().roots.get(screen)?.root;
    let atoms = Atoms {
        net_client_list: intern(&connection, "_NET_CLIENT_LIST")?,
        net_wm_name: intern(&connection, "_NET_WM_NAME")?,
        net_wm_pid: intern(&connection, "_NET_WM_PID")?,
        utf8_string: intern(&connection, "UTF8_STRING")?,
        compound_text: intern(&connection, "COMPOUND_TEXT")?,
        gtk_bus: intern(&connection, "_GTK_UNIQUE_BUS_NAME")?,
        gtk_menubar: intern(&connection, "_GTK_MENUBAR_OBJECT_PATH")?,
        gtk_app: intern(&connection, "_GTK_APPLICATION_OBJECT_PATH")?,
        gtk_window: intern(&connection, "_GTK_WINDOW_OBJECT_PATH")?,
    };
    Some(Live { connection, root, atoms })
}

impl Live {
    fn property(&self, window: Window, atom: Atom, kind: Atom) -> Option<Vec<u8>> {
        let reply = self.connection.get_property(false, window, atom, kind, 0, PROPERTY_LIMIT).ok()?.reply().ok()?;
        (reply.value_len > 0).then_some(reply.value)
    }

    fn text(&self, window: Window, atom: Atom) -> Option<String> {
        let reply =
            self.connection.get_property(false, window, atom, AtomEnum::ANY, 0, PROPERTY_LIMIT).ok()?.reply().ok()?;
        if reply.value_len == 0 || reply.format != 8 {
            return None;
        }
        let encoding = if reply.type_ == self.atoms.utf8_string {
            Encoding::Utf8
        } else if reply.type_ == self.atoms.compound_text {
            Encoding::Compound
        } else if reply.type_ == Atom::from(AtomEnum::STRING) {
            Encoding::Latin1
        } else {
            return None;
        };
        let text = decode(&reply.value, encoding);
        let text = text.trim_end_matches('\0');
        (!text.is_empty()).then(|| text.to_string())
    }

    fn client_list(&self) -> Option<Vec<Window>> {
        let tree = self.connection.query_tree(self.root).ok()?.reply().ok()?;
        let listed = self
            .property(self.root, self.atoms.net_client_list, AtomEnum::WINDOW.into())
            .map(|raw| words(&raw).collect::<Vec<_>>())
            .unwrap_or_default();
        if !listed.is_empty() {
            return Some(listed);
        }
        let mut out = Vec::new();
        for child in tree.children {
            out.push(child);
            if let Ok(Ok(inner)) = self.connection.query_tree(child).map(x11rb::cookie::Cookie::reply) {
                out.extend(inner.children);
            }
        }
        Some(out)
    }

    fn snapshot(&self) -> Option<Vec<X11Window>> {
        let ids = self.client_list()?;
        Some(ids.into_iter().filter_map(|id| self.window(id)).collect())
    }

    fn window(&self, id: Window) -> Option<X11Window> {
        let class_raw = self.property(id, AtomEnum::WM_CLASS.into(), AtomEnum::STRING.into())?;
        let mut parts = class_raw.split(|byte| *byte == 0).map(latin1);
        let instance = parts.next().unwrap_or_default();
        let class = parts.next().unwrap_or_default();
        let title = self
            .text(id, self.atoms.net_wm_name)
            .or_else(|| self.text(id, AtomEnum::WM_NAME.into()))
            .unwrap_or_default();
        let pid =
            self.property(id, self.atoms.net_wm_pid, AtomEnum::CARDINAL.into()).and_then(|raw| words(&raw).next());
        let gtk = self.text(id, self.atoms.gtk_bus).zip(self.text(id, self.atoms.gtk_menubar)).map(|(bus, menubar)| {
            Source::Gtk {
                bus,
                menubar,
                app: self.text(id, self.atoms.gtk_app),
                window: self.text(id, self.atoms.gtk_window),
            }
        });
        Some(X11Window { id, instance, class, title, pid, gtk })
    }
}

impl Poller {
    fn ensure_connected(&mut self) -> bool {
        if self.live.is_some() {
            return true;
        }
        let due = self.last_attempt.is_none_or(|attempt| attempt.elapsed() >= RECONNECT);
        if !due {
            return false;
        }
        self.last_attempt = Some(Instant::now());
        self.live = connect();
        if self.live.is_some() {
            tracing::info!("connected to the X11 display for menu discovery");
        }
        self.live.is_some()
    }

    fn poll(&mut self) -> Vec<X11Window> {
        if !self.ensure_connected() {
            return Vec::new();
        }
        self.live.as_ref().and_then(Live::snapshot).unwrap_or_else(|| {
            tracing::info!("lost the X11 display; menu discovery over X11 paused");
            self.live = None;
            Vec::new()
        })
    }

    fn run(mut self, shared: &Shared) {
        loop {
            let windows = self.poll();
            match shared.lock() {
                Ok(mut slot) => *slot = windows,
                Err(_) => return,
            }
            if std::sync::Arc::strong_count(shared) == 1 {
                return;
            }
            std::thread::sleep(REFRESH);
        }
    }
}

impl X11Bridge {
    pub fn windows(&mut self) -> Vec<X11Window> {
        if !self.started {
            self.started = true;
            let shared = std::sync::Arc::clone(&self.shared);
            let spawned = std::thread::Builder::new()
                .name(String::from("glide-x11-menus"))
                .spawn(move || Poller::default().run(&shared));
            if let Err(err) = spawned {
                tracing::warn!("could not start X11 menu discovery: {err}");
            }
        }
        self.shared.lock().map(|windows| windows.clone()).unwrap_or_default()
    }
}

#[must_use]
pub fn match_window<'a>(windows: &'a [X11Window], app_id: &str, title: &str) -> Option<&'a X11Window> {
    let same_app = |w: &X11Window| w.class.eq_ignore_ascii_case(app_id) || w.instance.eq_ignore_ascii_case(app_id);
    let mut candidates: Vec<&X11Window> = windows.iter().filter(|w| same_app(w)).collect();
    if candidates.is_empty() {
        candidates = windows.iter().filter(|w| !title.is_empty() && w.title == title).collect();
    }
    candidates
        .iter()
        .find(|w| !title.is_empty() && w.title == title)
        .or_else(|| (candidates.len() == 1).then(|| &candidates[0]))
        .copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(id: u32, class: &str, title: &str) -> X11Window {
        X11Window {
            id,
            instance: class.to_lowercase(),
            class: class.to_string(),
            title: title.to_string(),
            pid: None,
            gtk: None,
        }
    }

    #[test]
    fn a_live_display_lists_windows_with_a_class() {
        if std::env::var_os("DISPLAY").is_none() {
            eprintln!("no DISPLAY; skipping");
            return;
        }
        let windows = Poller::default().poll();
        assert!(
            windows.iter().all(|w| !w.class.is_empty() || !w.instance.is_empty()),
            "every listed window carries WM_CLASS: {windows:?}"
        );
    }

    #[test]
    fn the_bridge_never_blocks_the_caller_on_the_x_server() {
        let mut bridge = X11Bridge::default();
        let started = Instant::now();
        let _ = bridge.windows();
        let _ = bridge.windows();
        assert!(started.elapsed() < Duration::from_millis(50), "reading the cache must not wait for X11 round trips");
    }

    #[test]
    fn a_lone_window_of_the_app_matches_even_when_titles_differ() {
        let windows = [window(1, "Google-chrome", "Inbox"), window(2, "Kate", "notes")];
        assert_eq!(match_window(&windows, "google-chrome", "Something else").map(|w| w.id), Some(1));
    }

    #[test]
    fn several_windows_of_the_app_are_told_apart_by_title() {
        let windows = [window(1, "Kate", "a.rs"), window(2, "Kate", "b.rs")];
        assert_eq!(match_window(&windows, "kate", "b.rs").map(|w| w.id), Some(2));
        assert_eq!(match_window(&windows, "kate", "c.rs"), None, "guessing between siblings would open the wrong menu");
    }

    #[test]
    fn x11_strings_are_decoded_by_their_declared_type() {
        assert_eq!(decode(b"Caf\xe9", Encoding::Latin1), "Café", "STRING is Latin-1, not UTF-8");
        assert_eq!(decode("Café".as_bytes(), Encoding::Utf8), "Café");
        assert_eq!(decode(b"Caf\xe9", Encoding::Compound), "Café", "compound text starts in Latin-1");
        let mixed = b"a \x1b%G\xd0\x9f\xd1\x80\xd0\xb8\x1b%@ \xe9";
        assert_eq!(decode(mixed, Encoding::Compound), "a При é");
        assert_eq!(decode(b"x\x1b$)Ay", Encoding::Compound), "xy", "other charset switches are dropped");
    }

    #[test]
    fn a_title_alone_is_the_last_resort() {
        let windows = [window(1, "libreoffice-writer", "doc.odt - Writer")];
        assert_eq!(match_window(&windows, "soffice", "doc.odt - Writer").map(|w| w.id), Some(1));
        assert_eq!(match_window(&windows, "soffice", "other"), None);
    }
}

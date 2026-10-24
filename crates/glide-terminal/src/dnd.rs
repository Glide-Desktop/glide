use std::io::Read;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, PoisonError};

use iced::futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use wayland_backend::client::{Backend, ObjectId};
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::wl_data_device::{self, WlDataDevice};
use wayland_client::protocol::wl_data_device_manager::{DndAction, WlDataDeviceManager};
use wayland_client::protocol::wl_data_offer::{self, WlDataOffer};
use wayland_client::protocol::{wl_registry, wl_seat, wl_surface};
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle};

const URI_LIST: &str = "text/uri-list";
const TEXTS: [&str; 3] = ["text/plain;charset=utf-8", "text/plain", "UTF8_STRING"];
const MAX_DROP: u64 = 16 * 1024 * 1024;
static ACTIVE_DROPS: AtomicUsize = AtomicUsize::new(0);

struct Reading;

impl Drop for Reading {
    fn drop(&mut self) {
        ACTIVE_DROPS.fetch_sub(1, Ordering::Relaxed);
    }
}

fn read_drop(mut reader: std::os::unix::net::UnixStream) -> std::io::Result<Vec<u8>> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut bytes = Vec::new();
    let mut buffer = [0; 8192];
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return Err(std::io::ErrorKind::TimedOut.into());
        }
        reader.set_read_timeout(Some(remaining))?;
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            return Ok(bytes);
        }
        if (bytes.len() + count) as u64 > MAX_DROP {
            return Err(std::io::Error::other("drop exceeds size limit"));
        }
        bytes.extend_from_slice(&buffer[..count]);
    }
}

pub use glide_sdk::window::Handles;

#[derive(Debug, Clone, PartialEq)]
pub enum Dropped {
    Hover(Option<(f32, f32)>),
    Paths(Vec<PathBuf>, (f32, f32)),
    Text(String, (f32, f32)),
}

#[must_use]
pub fn quote(path: &std::path::Path) -> String {
    let text = path.to_string_lossy();
    let plain = |c: char| c.is_alphanumeric() || "/._-+,:@%~=".contains(c);
    if !text.is_empty() && text.chars().all(plain) {
        return text.into_owned();
    }
    format!("'{}'", text.replace('\'', r"'\''"))
}

#[must_use]
pub(crate) fn quote_bytes(path: &std::path::Path) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    if path.to_str().is_some() {
        return quote(path).into_bytes();
    }
    let mut quoted = vec![b'\''];
    for &byte in path.as_os_str().as_bytes() {
        if byte == b'\'' {
            quoted.extend_from_slice(b"'\\''");
        } else {
            quoted.push(byte);
        }
    }
    quoted.push(b'\'');
    quoted
}

#[must_use]
pub fn parse(mime: &str, bytes: &[u8], at: (f32, f32)) -> Option<Dropped> {
    let text = String::from_utf8_lossy(bytes);
    if mime != URI_LIST {
        return (!text.is_empty()).then(|| Dropped::Text(text.into_owned(), at));
    }
    let entries: Vec<&str> =
        text.lines().map(str::trim).filter(|line| !line.is_empty() && !line.starts_with('#')).collect();
    if entries.is_empty() {
        return None;
    }
    if entries.iter().all(|entry| entry.starts_with("file://"))
        && let Some(paths) = entries.iter().map(|entry| crate::app::local_uri_path(entry)).collect::<Option<Vec<_>>>()
    {
        return Some(Dropped::Paths(paths, at));
    }
    Some(Dropped::Text(entries.join(" "), at))
}

#[derive(Default)]
struct Offer {
    mimes: Vec<String>,
    copied: bool,
}

struct State {
    events: UnboundedSender<Dropped>,
    connection: Connection,
    drag: Option<WlDataOffer>,
    hover: (f32, f32),
}

#[must_use]
pub fn events(handles: &Handles) -> UnboundedReceiver<Dropped> {
    let (sender, receiver) = unbounded();
    let handles = *handles;
    let _ = std::thread::Builder::new().name(String::from("glide-terminal-dnd")).spawn(move || run(handles, sender));
    receiver
}

fn run(handles: Handles, events: UnboundedSender<Dropped>) -> Result<(), String> {
    // SAFETY: `display` is the live `wl_display` iced's window was created on, and the window lives for the
    // rest of the process.
    let backend = unsafe { Backend::from_foreign_display(handles.display as *mut _) };
    let connection = Connection::from_backend(backend);
    // SAFETY: `surface` is the live `wl_surface` of that same window, owned by winit.
    let surface_id = unsafe { ObjectId::from_ptr(wl_surface::WlSurface::interface(), handles.surface as *mut _) }
        .map_err(|error| error.to_string())?;
    wl_surface::WlSurface::from_id(&connection, surface_id).map_err(|error| error.to_string())?;
    let (globals, mut queue): (_, EventQueue<State>) =
        registry_queue_init(&connection).map_err(|error| error.to_string())?;
    let qh = queue.handle();
    let seat: wl_seat::WlSeat = globals.bind(&qh, 1..=7, ()).map_err(|error| error.to_string())?;
    let manager: WlDataDeviceManager = globals.bind(&qh, 3..=3, ()).map_err(|error| error.to_string())?;
    manager.get_data_device(&seat, &qh, ());
    let mut state = State { events, connection, drag: None, hover: (0.0, 0.0) };
    loop {
        queue.blocking_dispatch(&mut state).map_err(|error| error.to_string())?;
        if state.events.is_closed() {
            return Ok(());
        }
    }
}

fn mimes(offer: &WlDataOffer) -> Vec<String> {
    offer
        .data::<Mutex<Offer>>()
        .map(|offer| offer.lock().unwrap_or_else(PoisonError::into_inner).mimes.clone())
        .unwrap_or_default()
}

fn wanted(offer: &WlDataOffer) -> Option<&'static str> {
    let offered = mimes(offer);
    std::iter::once(URI_LIST).chain(TEXTS).find(|mime| offered.iter().any(|offered| offered == mime))
}

impl State {
    fn drop_offer(&mut self) {
        let Some(offer) = self.drag.take() else { return };
        let Some(mime) = wanted(&offer) else {
            offer.destroy();
            return;
        };
        if ACTIVE_DROPS
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |active| (active < 4).then_some(active + 1))
            .is_err()
        {
            offer.destroy();
            return;
        }
        let reading = Reading;
        let Ok((reader, writer)) = std::os::unix::net::UnixStream::pair() else {
            offer.destroy();
            return;
        };
        if reader.set_read_timeout(Some(std::time::Duration::from_secs(10))).is_err() {
            offer.destroy();
            return;
        }
        offer.receive(mime.to_string(), std::os::fd::AsFd::as_fd(&writer));
        drop(writer);
        let _ = self.connection.flush();
        let (events, connection, at) = (self.events.clone(), self.connection.clone(), self.hover);
        let failed_offer = offer.clone();
        let spawned = std::thread::Builder::new().name(String::from("glide-terminal-drop")).spawn(move || {
            let _reading = reading;
            if let Ok(bytes) = read_drop(reader)
                && let Some(dropped) = parse(mime, &bytes, at)
            {
                let _ = events.unbounded_send(dropped);
            }
            if offer.version() >= 3
                && offer
                    .data::<Mutex<Offer>>()
                    .is_some_and(|data| data.lock().unwrap_or_else(PoisonError::into_inner).copied)
            {
                offer.finish();
            }
            offer.destroy();
            let _ = connection.flush();
        });
        if spawned.is_err() {
            failed_offer.destroy();
        }
    }
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut Self, _: &wl_registry::WlRegistry, _: wl_registry::Event, _: &GlobalListContents, _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

wayland_client::delegate_noop!(State: ignore wl_seat::WlSeat);
wayland_client::delegate_noop!(State: ignore WlDataDeviceManager);

impl Dispatch<WlDataDevice, ()> for State {
    fn event(
        state: &mut Self, _: &WlDataDevice, event: wl_data_device::Event, (): &(), _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_data_device::Event::Enter { serial, x, y, id, .. } => {
                state.hover = (x as f32, y as f32);
                if let Some(offer) = &id {
                    let mime = wanted(offer);
                    offer.accept(serial, mime.map(str::to_string));
                    if offer.version() >= 3 {
                        let actions = if mime.is_some() { DndAction::Copy } else { DndAction::empty() };
                        offer.set_actions(actions, actions);
                    }
                }
                state.drag = id;
                let _ = state.events.unbounded_send(Dropped::Hover(Some(state.hover)));
            }
            wl_data_device::Event::Motion { x, y, .. } => {
                state.hover = (x as f32, y as f32);
                let _ = state.events.unbounded_send(Dropped::Hover(Some(state.hover)));
            }
            wl_data_device::Event::Leave => {
                if let Some(offer) = state.drag.take() {
                    offer.destroy();
                }
                let _ = state.events.unbounded_send(Dropped::Hover(None));
            }
            wl_data_device::Event::Drop => {
                state.drop_offer();
                let _ = state.events.unbounded_send(Dropped::Hover(None));
            }
            wl_data_device::Event::Selection { id: Some(offer) } => offer.destroy(),
            _ => {}
        }
    }

    wayland_client::event_created_child!(State, WlDataDevice, [
        wl_data_device::EVT_DATA_OFFER_OPCODE => (WlDataOffer, Mutex::new(Offer::default())),
    ]);
}

impl Dispatch<WlDataOffer, Mutex<Offer>> for State {
    fn event(
        _: &mut Self, _: &WlDataOffer, event: wl_data_offer::Event, data: &Mutex<Offer>, _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let mut data = data.lock().unwrap_or_else(PoisonError::into_inner);
        match event {
            wl_data_offer::Event::Offer { mime_type } => data.mimes.push(mime_type),
            wl_data_offer::Event::Action { dnd_action } => {
                data.copied = dnd_action == wayland_client::WEnum::Value(DndAction::Copy);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn dropped_paths_are_quoted_only_when_the_shell_would_split_them() {
        assert_eq!(quote(Path::new("/home/me/notes.txt")), "/home/me/notes.txt");
        assert_eq!(quote(Path::new("/home/me/My Files/a b.txt")), "'/home/me/My Files/a b.txt'");
        assert_eq!(quote(Path::new("/tmp/it's")), r"'/tmp/it'\''s'");
        assert_eq!(quote(Path::new("/tmp/$x;rm")), "'/tmp/$x;rm'");
    }

    #[test]
    fn file_lists_become_paths_and_anything_else_text() {
        let at = (1.0, 2.0);
        let files = parse(URI_LIST, b"# comment\r\nfile:///tmp/a%20b\r\nfile:///etc/hosts\r\n", at);
        assert_eq!(files, Some(Dropped::Paths(vec![PathBuf::from("/tmp/a b"), PathBuf::from("/etc/hosts")], at)));
        let link = parse(URI_LIST, b"https://glide.dev/\r\n", at);
        assert_eq!(link, Some(Dropped::Text(String::from("https://glide.dev/"), at)));
        assert_eq!(parse(TEXTS[0], "ls -la".as_bytes(), at), Some(Dropped::Text(String::from("ls -la"), at)));
        assert_eq!(parse(URI_LIST, b"\r\n", at), None);
    }

    #[test]
    fn remote_file_uris_are_never_reinterpreted_as_local_paths() {
        let at = (0.0, 0.0);
        assert_eq!(
            parse(URI_LIST, b"file://remote/etc/hosts", at),
            Some(Dropped::Text("file://remote/etc/hosts".into(), at))
        );
    }

    #[test]
    fn quoting_preserves_non_utf8_filename_bytes() {
        use std::os::unix::ffi::OsStrExt;
        let path = std::path::Path::new(std::ffi::OsStr::from_bytes(b"/tmp/a\xff'b"));
        assert_eq!(quote_bytes(path), b"'/tmp/a\xff'\\''b'");
    }

    #[test]
    fn drop_reads_stop_at_eof_and_reject_oversized_data() {
        use std::io::Write;
        let (reader, mut writer) = std::os::unix::net::UnixStream::pair().unwrap();
        let thread = std::thread::spawn(move || {
            writer.write_all(b"hello").unwrap();
        });
        assert_eq!(read_drop(reader).unwrap(), b"hello");
        thread.join().unwrap();
        let (reader, mut writer) = std::os::unix::net::UnixStream::pair().unwrap();
        let thread = std::thread::spawn(move || {
            let _ = writer.write_all(&vec![0; MAX_DROP as usize + 1]);
        });
        assert!(read_drop(reader).is_err());
        thread.join().unwrap();
    }
}

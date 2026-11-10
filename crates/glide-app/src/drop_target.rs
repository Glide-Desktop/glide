use std::io::Read;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use futures_channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use wayland_backend::client::{Backend, ObjectId};
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::wl_data_device::{self, WlDataDevice};
use wayland_client::protocol::wl_data_device_manager::{DndAction, WlDataDeviceManager};
use wayland_client::protocol::wl_data_offer::{self, WlDataOffer};
use wayland_client::protocol::{wl_registry, wl_seat, wl_surface};
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle};

use crate::menubar::Handles;

pub const URI_LIST: &str = "text/uri-list";
pub const TEXTS: [&str; 3] = ["text/plain;charset=utf-8", "text/plain", "UTF8_STRING"];
pub const MAX_DROP: u64 = 16 * 1024 * 1024;
const READ_LIMIT: Duration = Duration::from_secs(10);
const MAX_READS: usize = 4;
static ACTIVE_DROPS: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Clone, PartialEq)]
pub enum Dropped {
    Hover(Option<(f32, f32)>),
    Uris(Vec<String>, (f32, f32)),
    Text(String, (f32, f32)),
}

#[must_use]
pub fn parse(mime: &str, bytes: &[u8], at: (f32, f32)) -> Option<Dropped> {
    let text = String::from_utf8_lossy(bytes);
    if mime != URI_LIST {
        let text = text.trim();
        return (!text.is_empty()).then(|| Dropped::Text(text.to_string(), at));
    }
    let uris: Vec<String> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect();
    (!uris.is_empty()).then_some(Dropped::Uris(uris, at))
}

pub fn read_drop(mut reader: std::os::unix::net::UnixStream) -> std::io::Result<Vec<u8>> {
    let deadline = Instant::now() + READ_LIMIT;
    let mut bytes = Vec::new();
    let mut buffer = [0; 8192];
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
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

struct Reading;

impl Drop for Reading {
    fn drop(&mut self) {
        ACTIVE_DROPS.fetch_sub(1, Ordering::Relaxed);
    }
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
    name: &'static str,
    window: wl_surface::WlSurface,
}

#[must_use]
pub fn events(handles: &Handles, name: &'static str) -> UnboundedReceiver<Dropped> {
    let (sender, receiver) = unbounded();
    let handles = *handles;
    let spawned = std::thread::Builder::new().name(format!("{name}-dnd")).spawn(move || {
        if let Err(err) = run(handles, sender, name) {
            tracing::warn!("{name}: drops onto the window are off: {err}");
        }
    });
    if let Err(err) = spawned {
        tracing::warn!("{name}: cannot listen for drops: {err}");
    }
    receiver
}

fn run(handles: Handles, events: UnboundedSender<Dropped>, name: &'static str) -> Result<(), String> {
    // SAFETY: `display` is the live `wl_display` the toolkit's window was created on, and that window lives for the
    // rest of the process.
    let backend = unsafe { Backend::from_foreign_display(handles.display() as *mut _) };
    let connection = Connection::from_backend(backend);
    // SAFETY: `surface` is the live `wl_surface` of that same window, owned by the toolkit.
    let surface_id = unsafe { ObjectId::from_ptr(wl_surface::WlSurface::interface(), handles.surface() as *mut _) }
        .map_err(|error| error.to_string())?;
    let window = wl_surface::WlSurface::from_id(&connection, surface_id).map_err(|error| error.to_string())?;
    let (globals, mut queue): (_, EventQueue<State>) =
        registry_queue_init(&connection).map_err(|error| error.to_string())?;
    let qh = queue.handle();
    let seat: wl_seat::WlSeat = globals.bind(&qh, 1..=7, ()).map_err(|error| error.to_string())?;
    let manager: WlDataDeviceManager = globals.bind(&qh, 3..=3, ()).map_err(|error| error.to_string())?;
    manager.get_data_device(&seat, &qh, ());
    let mut state = State { events, connection, drag: None, hover: (0.0, 0.0), name, window };
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

#[must_use]
pub fn wanted(offered: &[String]) -> Option<&'static str> {
    std::iter::once(URI_LIST).chain(TEXTS).find(|mime| offered.iter().any(|offered| offered == mime))
}

impl State {
    fn drop_offer(&mut self) {
        let Some(offer) = self.drag.take() else { return };
        let Some(mime) = wanted(&mimes(&offer)) else {
            offer.destroy();
            return;
        };
        if ACTIVE_DROPS
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |active| (active < MAX_READS).then_some(active + 1))
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
        offer.receive(mime.to_string(), std::os::fd::AsFd::as_fd(&writer));
        drop(writer);
        let _ = self.connection.flush();
        let (events, connection, at) = (self.events.clone(), self.connection.clone(), self.hover);
        let failed_offer = offer.clone();
        let spawned = std::thread::Builder::new().name(format!("{}-drop", self.name)).spawn(move || {
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
            wl_data_device::Event::Enter { serial, surface, x, y, id } => {
                if surface != state.window {
                    if let Some(offer) = id {
                        offer.accept(serial, None);
                        offer.destroy();
                    }
                    state.drag = None;
                    return;
                }
                state.hover = (x as f32, y as f32);
                if let Some(offer) = &id {
                    let mime = wanted(&mimes(offer));
                    offer.accept(serial, mime.map(str::to_string));
                    if offer.version() >= 3 {
                        let actions = if mime.is_some() { DndAction::Copy } else { DndAction::empty() };
                        offer.set_actions(actions, actions);
                    }
                }
                let usable = id.as_ref().is_some_and(|offer| wanted(&mimes(offer)).is_some());
                state.drag = id;
                if usable {
                    let _ = state.events.unbounded_send(Dropped::Hover(Some(state.hover)));
                }
            }
            wl_data_device::Event::Motion { x, y, .. } => {
                state.hover = (x as f32, y as f32);
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

    #[test]
    fn a_uri_list_keeps_every_uri_and_drops_comments() {
        let at = (1.0, 2.0);
        let dropped = parse(URI_LIST, b"# from Files\r\nfile:///tmp/a%20b.mkv\r\nhttps://glide.dev/live.m3u8\r\n", at);
        assert_eq!(
            dropped,
            Some(Dropped::Uris(vec!["file:///tmp/a%20b.mkv".into(), "https://glide.dev/live.m3u8".into()], at))
        );
        assert_eq!(parse(URI_LIST, b"\r\n# only a comment\r\n", at), None);
    }

    #[test]
    fn plain_text_is_trimmed_and_empty_text_is_nothing() {
        let at = (0.0, 0.0);
        assert_eq!(parse(TEXTS[0], b"  /home/me/a.mkv\n", at), Some(Dropped::Text("/home/me/a.mkv".into(), at)));
        assert_eq!(parse(TEXTS[1], b" \n", at), None);
    }

    #[test]
    fn a_uri_list_is_preferred_over_text() {
        let offered = vec!["text/plain".to_string(), URI_LIST.to_string()];
        assert_eq!(wanted(&offered), Some(URI_LIST));
        assert_eq!(wanted(&["UTF8_STRING".to_string()]), Some("UTF8_STRING"));
        assert_eq!(wanted(&["image/png".to_string()]), None);
    }

    #[test]
    fn reads_stop_at_the_end_and_refuse_oversized_drops() {
        use std::io::Write;
        let (reader, mut writer) = std::os::unix::net::UnixStream::pair().unwrap();
        let thread = std::thread::spawn(move || writer.write_all(b"file:///a").unwrap());
        assert_eq!(read_drop(reader).unwrap(), b"file:///a");
        thread.join().unwrap();
        let (reader, mut writer) = std::os::unix::net::UnixStream::pair().unwrap();
        let thread = std::thread::spawn(move || {
            let _ = writer.write_all(&vec![0; MAX_DROP as usize + 1]);
        });
        assert!(read_drop(reader).is_err());
        thread.join().unwrap();
    }
}

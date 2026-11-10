use std::io::{Read, Write};
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex, PoisonError};

use glide_app::menubar::Handles;
use wayland_backend::client::Backend;
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::{
    wl_data_device, wl_data_device_manager, wl_data_offer, wl_data_source, wl_keyboard, wl_pointer, wl_registry,
    wl_seat,
};
use wayland_client::{Connection, Dispatch, EventQueue, QueueHandle};

pub const PNG: &str = "image/png";

struct State {
    serial: u32,
    payload: Arc<Mutex<Vec<u8>>>,
    source: Option<wl_data_source::WlDataSource>,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut Self, _: &wl_registry::WlRegistry, _: wl_registry::Event, _: &GlobalListContents, _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_pointer::WlPointer, ()> for State {
    fn event(
        state: &mut Self, _: &wl_pointer::WlPointer, event: wl_pointer::Event, (): &(), _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_pointer::Event::Enter { serial, .. } | wl_pointer::Event::Button { serial, .. } = event {
            state.serial = serial;
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for State {
    fn event(
        state: &mut Self, _: &wl_keyboard::WlKeyboard, event: wl_keyboard::Event, (): &(), _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_keyboard::Event::Enter { serial, .. } | wl_keyboard::Event::Key { serial, .. } = event {
            state.serial = serial;
        }
    }
}

impl Dispatch<wl_data_source::WlDataSource, ()> for State {
    fn event(
        state: &mut Self, source: &wl_data_source::WlDataSource, event: wl_data_source::Event, (): &(), _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_data_source::Event::Send { mime_type, fd } if mime_type == PNG => {
                let bytes = state.payload.lock().unwrap_or_else(PoisonError::into_inner).clone();
                let _ =
                    std::thread::Builder::new().name("glide-video-paste".into()).spawn(move || write_all(fd, &bytes));
            }
            wl_data_source::Event::Cancelled => {
                source.destroy();
                if state.source.as_ref() == Some(source) {
                    state.source = None;
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_data_device::WlDataDevice, ()> for State {
    fn event(
        _: &mut Self, _: &wl_data_device::WlDataDevice, event: wl_data_device::Event, (): &(), _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_data_device::Event::Selection { id: Some(offer) } = event {
            offer.destroy();
        }
    }

    wayland_client::event_created_child!(State, wl_data_device::WlDataDevice, [
        wl_data_device::EVT_DATA_OFFER_OPCODE => (wl_data_offer::WlDataOffer, ()),
    ]);
}

fn write_all(fd: OwnedFd, bytes: &[u8]) {
    let mut file = std::fs::File::from(fd);
    let _ = file.write_all(bytes);
}

wayland_client::delegate_noop!(State: ignore wl_seat::WlSeat);
wayland_client::delegate_noop!(State: ignore wl_data_device_manager::WlDataDeviceManager);
wayland_client::delegate_noop!(State: ignore wl_data_offer::WlDataOffer);

pub struct Clipboard {
    sender: mpsc::Sender<Vec<u8>>,
    wake: Option<UnixStream>,
}

impl Clipboard {
    #[must_use]
    pub fn new(handles: Handles) -> Self {
        let (sender, receiver) = mpsc::channel();
        let display = handles.display();
        let (wake, woken) = match UnixStream::pair().and_then(|(wake, woken)| {
            woken.set_nonblocking(true)?;
            wake.set_nonblocking(true)?;
            Ok((wake, woken))
        }) {
            Ok(pair) => pair,
            Err(err) => {
                tracing::warn!("could not start the clipboard helper: {err}");
                return Self { sender, wake: None };
            }
        };
        let spawned = std::thread::Builder::new().name("glide-video-clipboard".into()).spawn(move || {
            if let Err(err) = run(display, &receiver, &woken) {
                tracing::debug!("the clipboard helper stopped: {err}");
            }
        });
        if let Err(err) = spawned {
            tracing::warn!("could not start the clipboard helper: {err}");
        }
        Self { sender, wake: Some(wake) }
    }

    pub fn offer_png(&self, png: Vec<u8>) {
        if self.sender.send(png).is_ok()
            && let Some(mut wake) = self.wake.as_ref()
        {
            let _ = wake.write(&[1]);
        }
    }
}

fn drain(mut woken: &UnixStream) {
    let mut scratch = [0_u8; 64];
    while woken.read(&mut scratch).is_ok_and(|read| read > 0) {}
}

fn run(display: usize, commands: &Receiver<Vec<u8>>, woken: &UnixStream) -> Result<(), String> {
    if display == 0 {
        return Err("no display".into());
    }
    // SAFETY: the display comes from a window's live raw handle, which `Handles::new` callers vouch lives for the
    // rest of the process.
    let backend = unsafe { Backend::from_foreign_display(display as *mut _) };
    let connection = Connection::from_backend(backend);
    let (globals, mut queue): (_, EventQueue<State>) =
        registry_queue_init(&connection).map_err(|err| err.to_string())?;
    let qh = queue.handle();
    let seat: wl_seat::WlSeat = globals.bind(&qh, 1..=5, ()).map_err(|err| err.to_string())?;
    let manager: wl_data_device_manager::WlDataDeviceManager =
        globals.bind(&qh, 1..=3, ()).map_err(|err| err.to_string())?;
    let _pointer = seat.get_pointer(&qh, ());
    let _keyboard = seat.get_keyboard(&qh, ());
    let device = manager.get_data_device(&seat, &qh, ());
    let mut state = State { serial: 0, payload: Arc::new(Mutex::new(Vec::new())), source: None };
    queue.roundtrip(&mut state).map_err(|err| err.to_string())?;
    loop {
        queue.dispatch_pending(&mut state).map_err(|err| err.to_string())?;
        queue.flush().map_err(|err| err.to_string())?;
        if let Some(guard) = queue.prepare_read() {
            let waiting = |fd| libc::pollfd { fd, events: libc::POLLIN, revents: 0 };
            let mut fds = [waiting(guard.connection_fd().as_raw_fd()), waiting(woken.as_raw_fd())];
            // SAFETY: `fds` is two valid pollfds for the duration of the call.
            let ready = unsafe { libc::poll(fds.as_mut_ptr(), 2, -1) };
            if ready > 0 && fds[0].revents != 0 {
                guard.read().map_err(|err| err.to_string())?;
            }
        }
        drain(woken);
        loop {
            match commands.try_recv() {
                Ok(png) => {
                    *state.payload.lock().unwrap_or_else(PoisonError::into_inner) = png;
                    if let Some(old) = state.source.take() {
                        old.destroy();
                    }
                    let source = manager.create_data_source(&qh, ());
                    source.offer(PNG.into());
                    device.set_selection(Some(&source), state.serial);
                    state.source = Some(source);
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => return Ok(()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wake_bytes_are_drained_without_blocking_and_a_closed_end_reads_as_done() {
        let (mut wake, woken) = UnixStream::pair().expect("a socket pair");
        woken.set_nonblocking(true).expect("non-blocking");
        drain(&woken);
        wake.write_all(&[1, 1, 1]).expect("wake");
        drain(&woken);
        let mut one = [0_u8; 1];
        assert!((&woken).read(&mut one).is_err(), "nothing is left to wake for");
        drop(wake);
        drain(&woken);
        assert_eq!((&woken).read(&mut one).ok(), Some(0), "the owner is gone");
    }
}

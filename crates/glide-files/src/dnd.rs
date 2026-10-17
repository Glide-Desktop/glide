use std::io::{Read, Write};
use std::os::fd::{FromRawFd, OwnedFd};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError, mpsc};

use iced::futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use wayland_backend::client::{Backend, ObjectId};
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::wl_buffer::WlBuffer;
use wayland_client::protocol::wl_compositor::WlCompositor;
use wayland_client::protocol::wl_data_device::{self, WlDataDevice};
use wayland_client::protocol::wl_data_device_manager::{DndAction, WlDataDeviceManager};
use wayland_client::protocol::wl_data_offer::{self, WlDataOffer};
use wayland_client::protocol::wl_data_source::{self, WlDataSource};
use wayland_client::protocol::wl_shm::{self, WlShm};
use wayland_client::protocol::{wl_keyboard, wl_pointer, wl_registry, wl_seat, wl_shm_pool, wl_surface};
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle, WEnum};

use crate::fs::{file_uri, path_from_arg};

pub const URI_LIST: &str = "text/uri-list";
pub const GNOME_FILES: &str = "x-special/gnome-copied-files";
const PLAIN: &str = "text/plain;charset=utf-8";
const PASTABLE: [(&str, &str, bool); 7] = [
    ("image/png", "png", true),
    ("image/jpeg", "jpg", true),
    ("image/webp", "webp", true),
    ("image/gif", "gif", true),
    (PLAIN, "txt", false),
    ("text/plain", "txt", false),
    ("UTF8_STRING", "txt", false),
];
const POLL_MS: i32 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Handles {
    pub display: usize,
    pub surface: usize,
}

#[derive(Debug)]
pub enum Command {
    StartDrag { paths: Vec<PathBuf>, art: Option<PathBuf> },
    SetSelection { paths: Vec<PathBuf>, cut: bool },
    ClearSelection,
    Paste(PathBuf),
}

#[derive(Debug, Clone)]
pub struct Link {
    commands: Arc<Mutex<mpsc::Sender<Command>>>,
    accepting: Arc<AtomicBool>,
}

impl Link {
    pub fn send(&self, command: Command) -> bool {
        self.commands.lock().unwrap_or_else(PoisonError::into_inner).send(command).is_ok()
    }

    pub fn set_accepting(&self, accepting: bool) {
        self.accepting.store(accepting, Ordering::Relaxed);
    }
}

impl PartialEq for Link {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.commands, &other.commands)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Ready(Link),
    Hover(Option<(f64, f64)>),
    Drop { paths: Vec<PathBuf>, at: (f64, f64), moved: bool },
    Pasted { paths: Vec<PathBuf>, target: PathBuf, cut: bool },
    PastedData { target: PathBuf, bytes: Vec<u8>, extension: &'static str, image: bool },
    DragEnded,
    SelectionLost,
}

#[must_use]
pub fn uri_list(paths: &[PathBuf]) -> String {
    paths.iter().map(|path| file_uri(path)).collect::<Vec<_>>().join("\r\n") + "\r\n"
}

#[must_use]
pub fn parse_uri_list(text: &str) -> Vec<PathBuf> {
    text.lines().map(str::trim).filter(|line| line.starts_with("file://")).filter_map(path_from_arg).collect()
}

#[must_use]
pub fn gnome_files(paths: &[PathBuf], cut: bool) -> String {
    std::iter::once(String::from(if cut { "cut" } else { "copy" }))
        .chain(paths.iter().map(|path| file_uri(path)))
        .collect::<Vec<_>>()
        .join("\n")
}

#[must_use]
pub fn parse_gnome_files(text: &str) -> Option<(bool, Vec<PathBuf>)> {
    let mut lines = text.lines();
    let cut = match lines.next()?.trim() {
        "cut" => true,
        "copy" => false,
        _ => return None,
    };
    let rest: String = lines.collect::<Vec<_>>().join("\n");
    Some((cut, parse_uri_list(&rest)))
}

fn plain(paths: &[PathBuf]) -> String {
    paths.iter().map(|path| path.display().to_string()).collect::<Vec<_>>().join("\n")
}

#[derive(Debug)]
struct Payload {
    uris: String,
    gnome: String,
    plain: String,
    selection: bool,
}

#[derive(Debug, Default)]
struct OfferInfo {
    mimes: Vec<String>,
    moved: bool,
}

type OfferData = Mutex<OfferInfo>;

struct State {
    events: UnboundedSender<Event>,
    connection: Connection,
    manager: WlDataDeviceManager,
    device: WlDataDevice,
    surface: wl_surface::WlSurface,
    pointer_serial: u32,
    input_serial: u32,
    drag: Option<WlDataOffer>,
    hover: (f64, f64),
    selection: Option<WlDataOffer>,
    own_selection: Option<WlDataSource>,
    accepting: Arc<AtomicBool>,
    answered: Option<(u32, bool)>,
    compositor: Option<WlCompositor>,
    shm: Option<WlShm>,
    icon: Option<(wl_surface::WlSurface, WlBuffer)>,
}

#[must_use]
pub fn events(handles: &Handles) -> UnboundedReceiver<Event> {
    let (sender, receiver) = unbounded();
    let handles = *handles;
    let spawned = std::thread::Builder::new().name(String::from("glide-files-dnd")).spawn(move || {
        if let Err(err) = run(handles, sender) {
            eprintln!("glide-files: drag and drop unavailable: {err}");
        }
    });
    if let Err(err) = spawned {
        eprintln!("glide-files: cannot start the drag and drop thread: {err}");
    }
    receiver
}

fn run(handles: Handles, events: UnboundedSender<Event>) -> Result<(), String> {
    // SAFETY: `display` is the live `wl_display` iced's window was created on, and the
    // window outlives this thread's use of it for the lifetime of the process.
    let backend = unsafe { Backend::from_foreign_display(handles.display as *mut _) };
    let connection = Connection::from_backend(backend);
    // SAFETY: `surface` is the live `wl_surface` of the same window, owned by winit.
    let surface_id = unsafe { ObjectId::from_ptr(wl_surface::WlSurface::interface(), handles.surface as *mut _) }
        .map_err(|err| err.to_string())?;
    let surface = wl_surface::WlSurface::from_id(&connection, surface_id).map_err(|err| err.to_string())?;
    serve(connection, surface, events)
}

pub fn serve(
    connection: Connection, surface: wl_surface::WlSurface, events: UnboundedSender<Event>,
) -> Result<(), String> {
    let (globals, mut queue): (_, EventQueue<State>) =
        registry_queue_init(&connection).map_err(|err| err.to_string())?;
    let qh = queue.handle();
    let seat: wl_seat::WlSeat = globals.bind(&qh, 1..=7, ()).map_err(|err| err.to_string())?;
    let manager: WlDataDeviceManager = globals.bind(&qh, 3..=3, ()).map_err(|err| err.to_string())?;
    seat.get_pointer(&qh, ());
    seat.get_keyboard(&qh, ());
    let device = manager.get_data_device(&seat, &qh, ());
    let compositor: Option<WlCompositor> = globals.bind(&qh, 3..=6, ()).ok();
    let shm: Option<WlShm> = globals.bind(&qh, 1..=1, ()).ok();
    let (commands, inbox) = mpsc::channel();
    let accepting = Arc::new(AtomicBool::new(true));
    let link = Link { commands: Arc::new(Mutex::new(commands)), accepting: Arc::clone(&accepting) };
    events.unbounded_send(Event::Ready(link)).map_err(|err| err.to_string())?;
    let mut state = State {
        events,
        connection,
        manager,
        device,
        surface,
        pointer_serial: 0,
        input_serial: 0,
        drag: None,
        hover: (0.0, 0.0),
        selection: None,
        own_selection: None,
        accepting,
        answered: None,
        compositor,
        shm,
        icon: None,
    };
    loop {
        queue.dispatch_pending(&mut state).map_err(|err| err.to_string())?;
        state.answer_hover();
        loop {
            match inbox.try_recv() {
                Ok(command) => state.handle(command, &qh),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => return Ok(()),
            }
        }
        queue.flush().map_err(|err| err.to_string())?;
        if let Some(guard) = queue.prepare_read() {
            let mut poll = libc::pollfd {
                fd: std::os::fd::AsRawFd::as_raw_fd(&guard.connection_fd()),
                events: libc::POLLIN,
                revents: 0,
            };
            // SAFETY: `poll` points to one valid `pollfd` for the duration of the call.
            let ready = unsafe { libc::poll(&raw mut poll, 1, POLL_MS) };
            if ready > 0 {
                guard.read().map_err(|err| err.to_string())?;
            }
        }
    }
}

impl State {
    fn handle(&mut self, command: Command, qh: &QueueHandle<Self>) {
        match command {
            Command::StartDrag { paths, art } if !paths.is_empty() => {
                let source = self.source(&paths, false, false, qh);
                source.set_actions(DndAction::Copy | DndAction::Move | DndAction::Ask);
                self.drop_icon();
                let image = art.as_deref().and_then(|art| crate::drag_icon::render(art, paths.len()));
                self.icon = image.and_then(|image| self.icon_surface(&image, qh));
                let icon = self.icon.as_ref().map(|(surface, _)| surface);
                self.device.start_drag(Some(&source), &self.surface, icon, self.pointer_serial);
                if let Some((surface, buffer)) = &self.icon {
                    if surface.version() >= 3 {
                        surface.set_buffer_scale(crate::drag_icon::SCALE.cast_signed());
                    }
                    surface.attach(Some(buffer), 0, 0);
                    surface.damage(0, 0, i32::MAX, i32::MAX);
                    surface.commit();
                }
            }
            Command::StartDrag { .. } => {}
            Command::SetSelection { paths, cut } => {
                let source = self.source(&paths, cut, true, qh);
                self.device.set_selection(Some(&source), self.input_serial);
                self.own_selection = Some(source);
            }
            Command::ClearSelection => {
                if self.own_selection.take().is_some() {
                    self.device.set_selection(None, self.input_serial);
                }
            }
            Command::Paste(target) => self.paste(target),
        }
    }

    fn icon_surface(
        &self, image: &crate::drag_icon::DragImage, qh: &QueueHandle<Self>,
    ) -> Option<(wl_surface::WlSurface, WlBuffer)> {
        let (compositor, shm) = (self.compositor.as_ref()?, self.shm.as_ref()?);
        let memory = shared_memory(&image.bgra)?;
        let (width, height) = (image.width.cast_signed(), image.height.cast_signed());
        let pool = shm.create_pool(std::os::fd::AsFd::as_fd(&memory), i32::try_from(image.bgra.len()).ok()?, qh, ());
        let buffer = pool.create_buffer(0, width, height, width * 4, wl_shm::Format::Argb8888, qh, ());
        pool.destroy();
        Some((compositor.create_surface(qh, ()), buffer))
    }

    fn drop_icon(&mut self) {
        if let Some((surface, buffer)) = self.icon.take() {
            surface.destroy();
            buffer.destroy();
        }
    }

    fn source(&self, paths: &[PathBuf], cut: bool, selection: bool, qh: &QueueHandle<Self>) -> WlDataSource {
        let payload = Payload { uris: uri_list(paths), gnome: gnome_files(paths, cut), plain: plain(paths), selection };
        let source = self.manager.create_data_source(qh, payload);
        for mime in [URI_LIST, GNOME_FILES, PLAIN] {
            source.offer(mime.to_string());
        }
        source
    }

    fn paste(&self, target: PathBuf) {
        let Some(offer) = self.selection.clone() else {
            return;
        };
        let mimes = offer_info(&offer).mimes;
        let offered = |wanted: &str| mimes.iter().any(|mime| mime == wanted);
        let events = self.events.clone();
        let mime = if offered(GNOME_FILES) {
            GNOME_FILES
        } else if offered(URI_LIST) {
            URI_LIST
        } else if let Some(&(mime, extension, image)) = PASTABLE.iter().find(|(mime, _, _)| offered(mime)) {
            self.receive(&offer, mime, move |bytes| {
                if !bytes.is_empty() {
                    let _ = events.unbounded_send(Event::PastedData { target, bytes, extension, image });
                }
            });
            return;
        } else {
            return;
        };
        self.receive(&offer, mime, move |bytes| {
            let text = String::from_utf8_lossy(&bytes);
            let (cut, paths) = if mime == GNOME_FILES {
                parse_gnome_files(&text).unwrap_or_default()
            } else {
                (false, parse_uri_list(&text))
            };
            if !paths.is_empty() {
                let _ = events.unbounded_send(Event::Pasted { paths, target, cut });
            }
        });
    }

    fn receive(&self, offer: &WlDataOffer, mime: &str, done: impl FnOnce(Vec<u8>) + Send + 'static) {
        let Some((reader, writer)) = pipe() else {
            return;
        };
        offer.receive(mime.to_string(), std::os::fd::AsFd::as_fd(&writer));
        drop(writer);
        let _ = self.connection.flush();
        let _ = std::thread::Builder::new().name(String::from("glide-files-receive")).spawn(move || {
            let mut bytes = Vec::new();
            let mut file = std::fs::File::from(reader);
            if file.read_to_end(&mut bytes).is_ok() {
                done(bytes);
            }
        });
    }

    fn answer_hover(&mut self) {
        let (Some(offer), Some((serial, answered))) = (&self.drag, self.answered) else {
            return;
        };
        let accepting = self.accepting.load(Ordering::Relaxed);
        if accepting != answered {
            answer(offer, serial, accepting);
            self.answered = Some((serial, accepting));
        }
    }

    fn drop_offer(&mut self) {
        self.answered = None;
        let Some(offer) = self.drag.take() else {
            return;
        };
        let info = offer_info(&offer);
        if !self.accepting.load(Ordering::Relaxed) || !info.mimes.iter().any(|mime| mime == URI_LIST) {
            offer.destroy();
            return;
        }
        let moved = info.moved;
        let at = self.hover;
        let events = self.events.clone();
        let connection = self.connection.clone();
        let finishing = offer.clone();
        self.receive(&offer, URI_LIST, move |bytes| {
            let paths = parse_uri_list(&String::from_utf8_lossy(&bytes));
            if !paths.is_empty() {
                let _ = events.unbounded_send(Event::Drop { paths, at, moved });
            }
            if finishing.version() >= 3 {
                finishing.finish();
            }
            finishing.destroy();
            let _ = connection.flush();
        });
    }
}

fn answer(offer: &WlDataOffer, serial: u32, accepting: bool) {
    let accepts = accepting && offer_info(offer).mimes.iter().any(|mime| mime == URI_LIST);
    offer.accept(serial, accepts.then(|| URI_LIST.to_string()));
    if offer.version() >= 3 {
        let actions = if accepts { DndAction::Copy | DndAction::Move } else { DndAction::empty() };
        offer.set_actions(actions, DndAction::Copy);
    }
}

fn offer_info(offer: &WlDataOffer) -> OfferInfo {
    offer
        .data::<OfferData>()
        .map(|data| {
            let info = data.lock().unwrap_or_else(PoisonError::into_inner);
            OfferInfo { mimes: info.mimes.clone(), moved: info.moved }
        })
        .unwrap_or_default()
}

fn shared_memory(bytes: &[u8]) -> Option<OwnedFd> {
    // SAFETY: the name is a NUL-terminated literal and the flags are valid for `memfd_create`.
    let raw = unsafe { libc::memfd_create(c"glide-files-drag".as_ptr(), libc::MFD_CLOEXEC) };
    if raw < 0 {
        return None;
    }
    // SAFETY: `memfd_create` succeeded, so `raw` is an open descriptor owned by nobody else.
    let fd = unsafe { OwnedFd::from_raw_fd(raw) };
    let mut file = std::fs::File::from(fd);
    file.write_all(bytes).ok()?;
    Some(OwnedFd::from(file))
}

fn pipe() -> Option<(OwnedFd, OwnedFd)> {
    let mut fds = [0; 2];
    // SAFETY: `fds` has room for the two descriptors `pipe2` writes on success.
    if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
        return None;
    }
    // SAFETY: `pipe2` succeeded, so both descriptors are open and owned by nobody else.
    Some(unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) })
}

fn write_payload(fd: OwnedFd, text: &str) {
    let mut file = std::fs::File::from(fd);
    let _ = file.write_all(text.as_bytes());
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
wayland_client::delegate_noop!(State: WlCompositor);
wayland_client::delegate_noop!(State: ignore wl_surface::WlSurface);
wayland_client::delegate_noop!(State: ignore WlShm);
wayland_client::delegate_noop!(State: wl_shm_pool::WlShmPool);
wayland_client::delegate_noop!(State: ignore WlBuffer);

impl Dispatch<wl_pointer::WlPointer, ()> for State {
    fn event(
        state: &mut Self, _: &wl_pointer::WlPointer, event: wl_pointer::Event, (): &(), _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_pointer::Event::Enter { serial, .. } => state.pointer_serial = serial,
            wl_pointer::Event::Button { serial, .. } => {
                state.pointer_serial = serial;
                state.input_serial = serial;
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for State {
    fn event(
        state: &mut Self, _: &wl_keyboard::WlKeyboard, event: wl_keyboard::Event, (): &(), _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_keyboard::Event::Enter { serial, .. } | wl_keyboard::Event::Key { serial, .. } = event {
            state.input_serial = serial;
        }
    }
}

impl Dispatch<WlDataDevice, ()> for State {
    fn event(
        state: &mut Self, _: &WlDataDevice, event: wl_data_device::Event, (): &(), _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_data_device::Event::Enter { serial, x, y, id, .. } => {
                state.hover = (x, y);
                let accepting = state.accepting.load(Ordering::Relaxed);
                if let Some(offer) = &id {
                    answer(offer, serial, accepting);
                }
                state.answered = id.as_ref().map(|_| (serial, accepting));
                state.drag = id;
                let _ = state.events.unbounded_send(Event::Hover(Some((x, y))));
            }
            wl_data_device::Event::Motion { x, y, .. } => {
                state.hover = (x, y);
                let _ = state.events.unbounded_send(Event::Hover(Some((x, y))));
            }
            wl_data_device::Event::Leave => {
                state.answered = None;
                if let Some(offer) = state.drag.take() {
                    offer.destroy();
                }
                let _ = state.events.unbounded_send(Event::Hover(None));
            }
            wl_data_device::Event::Drop => {
                state.drop_offer();
                let _ = state.events.unbounded_send(Event::Hover(None));
            }
            wl_data_device::Event::Selection { id } => {
                if let Some(old) = state.selection.take() {
                    old.destroy();
                }
                state.selection = id;
            }
            _ => {}
        }
    }

    wayland_client::event_created_child!(State, WlDataDevice, [
        wl_data_device::EVT_DATA_OFFER_OPCODE => (WlDataOffer, Mutex::new(OfferInfo::default())),
    ]);
}

impl Dispatch<WlDataOffer, OfferData> for State {
    fn event(
        _: &mut Self, _: &WlDataOffer, event: wl_data_offer::Event, data: &OfferData, _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let mut info = data.lock().unwrap_or_else(PoisonError::into_inner);
        match event {
            wl_data_offer::Event::Offer { mime_type } => info.mimes.push(mime_type),
            wl_data_offer::Event::Action { dnd_action: WEnum::Value(action) } => info.moved = action == DndAction::Move,
            _ => {}
        }
    }
}

impl Dispatch<WlDataSource, Payload> for State {
    fn event(
        state: &mut Self, source: &WlDataSource, event: wl_data_source::Event, payload: &Payload, _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_data_source::Event::Send { mime_type, fd } => {
                let text = match mime_type.as_str() {
                    URI_LIST => &payload.uris,
                    GNOME_FILES => &payload.gnome,
                    _ => &payload.plain,
                };
                write_payload(fd, text);
            }
            wl_data_source::Event::Cancelled => {
                if payload.selection {
                    if state.own_selection.as_ref() == Some(source) {
                        state.own_selection = None;
                        let _ = state.events.unbounded_send(Event::SelectionLost);
                    }
                } else {
                    state.drop_icon();
                    let _ = state.events.unbounded_send(Event::DragEnded);
                }
                source.destroy();
            }
            wl_data_source::Event::DndFinished => {
                state.drop_icon();
                let _ = state.events.unbounded_send(Event::DragEnded);
                source.destroy();
            }
            _ => {}
        }
    }
}

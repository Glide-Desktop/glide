use std::io::{Read, Write};
use std::os::fd::{FromRawFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use glide_files::dnd::{Command, Dropped, Event, Link, serve};
use iced::futures::channel::mpsc::{UnboundedReceiver, unbounded};
use wayland_server::backend::{ClientData, ClientId, DisconnectReason};
use wayland_server::protocol::wl_data_device_manager::{DndAction, WlDataDeviceManager};
use wayland_server::protocol::wl_data_offer::WlDataOffer;
use wayland_server::protocol::wl_data_source::WlDataSource;
use wayland_server::protocol::{
    wl_buffer, wl_compositor, wl_data_device, wl_data_device_manager, wl_data_offer, wl_data_source, wl_keyboard,
    wl_pointer, wl_seat, wl_shm, wl_shm_pool, wl_surface,
};
use wayland_server::{Client, DataInit, Dispatch, Display, DisplayHandle, GlobalDispatch, New, Resource};

const URI_LIST: &str = "text/uri-list";
const GNOME: &str = "x-special/gnome-copied-files";

#[derive(Default)]
struct Compositor {
    surface: Option<wl_surface::WlSurface>,
    device: Option<wl_data_device::WlDataDevice>,
    dragged: Option<WlDataSource>,
    icon: Option<wl_surface::WlSurface>,
    shown: Vec<(wl_surface::WlSurface, i32, i32)>,
    moved: (i32, i32),
    scales: Vec<i32>,
    selection: Option<WlDataSource>,
    accepted: Vec<Option<String>>,
    actions: Vec<(DndAction, DndAction)>,
    finished: bool,
}

type Offered = Mutex<Vec<String>>;

struct Peer;

impl ClientData for Peer {
    fn initialized(&self, _: ClientId) {}
    fn disconnected(&self, _: ClientId, _: DisconnectReason) {}
}

impl GlobalDispatch<wl_compositor::WlCompositor, ()> for Compositor {
    fn bind(
        _: &mut Self, _: &DisplayHandle, _: &Client, resource: New<wl_compositor::WlCompositor>, (): &(),
        init: &mut DataInit<'_, Self>,
    ) {
        init.init(resource, ());
    }
}

impl Dispatch<wl_compositor::WlCompositor, ()> for Compositor {
    fn request(
        state: &mut Self, _: &Client, _: &wl_compositor::WlCompositor, request: wl_compositor::Request, (): &(),
        _: &DisplayHandle, init: &mut DataInit<'_, Self>,
    ) {
        if let wl_compositor::Request::CreateSurface { id } = request {
            let surface = init.init(id, ());
            state.surface.get_or_insert(surface);
        }
    }
}

impl Dispatch<wl_surface::WlSurface, ()> for Compositor {
    fn request(
        state: &mut Self, _: &Client, surface: &wl_surface::WlSurface, request: wl_surface::Request, (): &(),
        _: &DisplayHandle, _: &mut DataInit<'_, Self>,
    ) {
        match request {
            wl_surface::Request::Attach { buffer: Some(buffer), x, y } => {
                assert!(surface.version() < 5 || (x, y) == (0, 0), "attach takes no offset from version 5");
                let &(width, height) = buffer.data::<(i32, i32)>().expect("buffer size");
                state.shown.push((surface.clone(), width, height));
                state.moved = (state.moved.0 + x, state.moved.1 + y);
            }
            wl_surface::Request::Offset { x, y } => state.moved = (state.moved.0 + x, state.moved.1 + y),
            wl_surface::Request::SetBufferScale { scale } => state.scales.push(scale),
            _ => {}
        }
    }
}

impl GlobalDispatch<wl_shm::WlShm, ()> for Compositor {
    fn bind(
        _: &mut Self, _: &DisplayHandle, _: &Client, resource: New<wl_shm::WlShm>, (): &(),
        init: &mut DataInit<'_, Self>,
    ) {
        init.init(resource, ());
    }
}

impl Dispatch<wl_shm::WlShm, ()> for Compositor {
    fn request(
        _: &mut Self, _: &Client, _: &wl_shm::WlShm, request: wl_shm::Request, (): &(), _: &DisplayHandle,
        init: &mut DataInit<'_, Self>,
    ) {
        if let wl_shm::Request::CreatePool { id, .. } = request {
            init.init(id, ());
        }
    }
}

impl Dispatch<wl_shm_pool::WlShmPool, ()> for Compositor {
    fn request(
        _: &mut Self, _: &Client, _: &wl_shm_pool::WlShmPool, request: wl_shm_pool::Request, (): &(),
        _: &DisplayHandle, init: &mut DataInit<'_, Self>,
    ) {
        if let wl_shm_pool::Request::CreateBuffer { id, width, height, .. } = request {
            init.init(id, (width, height));
        }
    }
}

impl Dispatch<wl_buffer::WlBuffer, (i32, i32)> for Compositor {
    fn request(
        _: &mut Self, _: &Client, _: &wl_buffer::WlBuffer, _: wl_buffer::Request, _: &(i32, i32), _: &DisplayHandle,
        _: &mut DataInit<'_, Self>,
    ) {
    }
}

impl GlobalDispatch<wl_seat::WlSeat, ()> for Compositor {
    fn bind(
        _: &mut Self, _: &DisplayHandle, _: &Client, resource: New<wl_seat::WlSeat>, (): &(),
        init: &mut DataInit<'_, Self>,
    ) {
        init.init(resource, ());
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for Compositor {
    fn request(
        _: &mut Self, _: &Client, _: &wl_seat::WlSeat, request: wl_seat::Request, (): &(), _: &DisplayHandle,
        init: &mut DataInit<'_, Self>,
    ) {
        match request {
            wl_seat::Request::GetPointer { id } => {
                init.init(id, ());
            }
            wl_seat::Request::GetKeyboard { id } => {
                init.init(id, ());
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_pointer::WlPointer, ()> for Compositor {
    fn request(
        _: &mut Self, _: &Client, _: &wl_pointer::WlPointer, _: wl_pointer::Request, (): &(), _: &DisplayHandle,
        _: &mut DataInit<'_, Self>,
    ) {
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for Compositor {
    fn request(
        _: &mut Self, _: &Client, _: &wl_keyboard::WlKeyboard, _: wl_keyboard::Request, (): &(), _: &DisplayHandle,
        _: &mut DataInit<'_, Self>,
    ) {
    }
}

impl GlobalDispatch<WlDataDeviceManager, ()> for Compositor {
    fn bind(
        _: &mut Self, _: &DisplayHandle, _: &Client, resource: New<WlDataDeviceManager>, (): &(),
        init: &mut DataInit<'_, Self>,
    ) {
        init.init(resource, ());
    }
}

impl Dispatch<WlDataDeviceManager, ()> for Compositor {
    fn request(
        state: &mut Self, _: &Client, _: &WlDataDeviceManager, request: wl_data_device_manager::Request, (): &(),
        _: &DisplayHandle, init: &mut DataInit<'_, Self>,
    ) {
        match request {
            wl_data_device_manager::Request::CreateDataSource { id } => {
                init.init(id, Mutex::new(Vec::new()));
            }
            wl_data_device_manager::Request::GetDataDevice { id, .. } => state.device = Some(init.init(id, ())),
            _ => {}
        }
    }
}

impl Dispatch<WlDataSource, Offered> for Compositor {
    fn request(
        _: &mut Self, _: &Client, _: &WlDataSource, request: wl_data_source::Request, offered: &Offered,
        _: &DisplayHandle, _: &mut DataInit<'_, Self>,
    ) {
        if let wl_data_source::Request::Offer { mime_type } = request {
            offered.lock().expect("offers").push(mime_type);
        }
    }
}

impl Dispatch<wl_data_device::WlDataDevice, ()> for Compositor {
    fn request(
        state: &mut Self, _: &Client, _: &wl_data_device::WlDataDevice, request: wl_data_device::Request, (): &(),
        _: &DisplayHandle, _: &mut DataInit<'_, Self>,
    ) {
        match request {
            wl_data_device::Request::StartDrag { source, icon, .. } => {
                state.dragged = source;
                state.icon = icon;
            }
            wl_data_device::Request::SetSelection { source, .. } => state.selection = source,
            _ => {}
        }
    }
}

impl Dispatch<WlDataOffer, String> for Compositor {
    fn request(
        state: &mut Self, _: &Client, _: &WlDataOffer, request: wl_data_offer::Request, payload: &String,
        _: &DisplayHandle, _: &mut DataInit<'_, Self>,
    ) {
        match request {
            wl_data_offer::Request::Accept { mime_type, .. } => state.accepted.push(mime_type),
            wl_data_offer::Request::Receive { fd, .. } => {
                let mut pipe = std::fs::File::from(fd);
                pipe.write_all(payload.as_bytes()).expect("payload");
            }
            wl_data_offer::Request::SetActions { dnd_actions, preferred_action } => {
                if let (wayland_server::WEnum::Value(all), wayland_server::WEnum::Value(preferred)) =
                    (dnd_actions, preferred_action)
                {
                    state.actions.push((all, preferred));
                }
            }
            wl_data_offer::Request::Finish => state.finished = true,
            _ => {}
        }
    }
}

struct Harness {
    display: Display<Compositor>,
    state: Compositor,
    client: Client,
    events: UnboundedReceiver<Event>,
    seen: Vec<Event>,
}

impl Harness {
    fn start() -> (Self, Link) {
        Self::with_compositor(4)
    }

    fn with_compositor(version: u32) -> (Self, Link) {
        let display: Display<Compositor> = Display::new().expect("display");
        let handle = display.handle();
        handle.create_global::<Compositor, wl_compositor::WlCompositor, ()>(version, ());
        handle.create_global::<Compositor, wl_seat::WlSeat, ()>(7, ());
        handle.create_global::<Compositor, WlDataDeviceManager, ()>(3, ());
        handle.create_global::<Compositor, wl_shm::WlShm, ()>(1, ());
        let (ours, theirs) = UnixStream::pair().expect("socket pair");
        let client = display.handle().insert_client(ours, Arc::new(Peer)).expect("client");
        let (sender, events) = unbounded();
        std::thread::spawn(move || client_side(theirs, sender));
        let mut harness = Self { display, state: Compositor::default(), client, events, seen: Vec::new() };
        let link = harness.wait(|harness| {
            harness.seen.iter().find_map(|event| match event {
                Event::Ready(link) if harness.state.device.is_some() && harness.state.surface.is_some() => {
                    Some(link.clone())
                }
                _ => None,
            })
        });
        (harness, link)
    }

    fn pump(&mut self) {
        self.display.dispatch_clients(&mut self.state).expect("dispatch");
        self.display.flush_clients().expect("flush");
        while let Ok(event) = self.events.try_recv() {
            self.seen.push(event);
        }
    }

    fn wait<T>(&mut self, mut done: impl FnMut(&Self) -> Option<T>) -> T {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            self.pump();
            if let Some(found) = done(self) {
                return found;
            }
            assert!(Instant::now() < deadline, "timed out; saw {:?}", self.seen);
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn offer(&mut self, payload: &str, mimes: &[&str]) -> WlDataOffer {
        let handle = self.display.handle();
        let offer = self
            .client
            .create_resource::<WlDataOffer, String, Compositor>(&handle, 3, payload.to_string())
            .expect("offer");
        let device = self.state.device.clone().expect("device");
        device.data_offer(&offer);
        for mime in mimes {
            offer.offer((*mime).to_string());
        }
        offer
    }

    fn read_source(&mut self, source: &WlDataSource, mime: &str) -> String {
        let mut fds = [0; 2];
        // SAFETY: `fds` has room for the two descriptors `pipe2` writes on success.
        assert_eq!(unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) }, 0);
        // SAFETY: `pipe2` succeeded, so both descriptors are open and owned by nobody else.
        let (reader, writer) = unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) };
        source.send(mime.to_string(), std::os::fd::AsFd::as_fd(&writer));
        drop(writer);
        let done = Arc::new(Mutex::new(None));
        let sink = Arc::clone(&done);
        std::thread::spawn(move || {
            let mut text = String::new();
            let _ = std::fs::File::from(reader).read_to_string(&mut text);
            *sink.lock().expect("text") = Some(text);
        });
        self.wait(|_| done.lock().expect("text").clone())
    }
}

#[derive(Default)]
struct Painter;

wayland_client::delegate_noop!(Painter: wayland_client::protocol::wl_compositor::WlCompositor);
wayland_client::delegate_noop!(Painter: ignore wayland_client::protocol::wl_surface::WlSurface);

impl
    wayland_client::Dispatch<
        wayland_client::protocol::wl_registry::WlRegistry,
        wayland_client::globals::GlobalListContents,
    > for Painter
{
    fn event(
        _: &mut Self, _: &wayland_client::protocol::wl_registry::WlRegistry,
        _: wayland_client::protocol::wl_registry::Event, _: &wayland_client::globals::GlobalListContents,
        _: &wayland_client::Connection, _: &wayland_client::QueueHandle<Self>,
    ) {
    }
}

fn client_side(stream: UnixStream, events: iced::futures::channel::mpsc::UnboundedSender<Event>) {
    let connection = wayland_client::Connection::from_socket(stream).expect("connect");
    let (globals, mut queue) = wayland_client::globals::registry_queue_init::<Painter>(&connection).expect("registry");
    let handle = queue.handle();
    let compositor: wayland_client::protocol::wl_compositor::WlCompositor =
        globals.bind(&handle, 1..=4, ()).expect("compositor");
    let surface = compositor.create_surface(&handle, ());
    queue.roundtrip(&mut Painter).expect("roundtrip");
    serve(connection, surface, events).expect("dnd thread");
}

#[test]
fn a_file_dropped_from_another_app_arrives_with_its_position_and_action() {
    let (mut harness, _link) = Harness::start();
    let offer = harness.offer("# from a browser\r\nfile:///tmp/a%20b.txt\r\n", &[URI_LIST, "text/plain"]);
    offer.source_actions(DndAction::Copy | DndAction::Move);
    let surface = harness.state.surface.clone().expect("surface");
    let device = harness.state.device.clone().expect("device");
    device.enter(7, &surface, 10.0, 20.0, Some(&offer));
    offer.action(DndAction::Move);
    device.motion(1, 11.0, 21.0);
    harness.pump();
    device.drop();
    let dropped = harness.wait(|harness| {
        harness.seen.iter().find_map(|event| match event {
            Event::Drop { paths, at, moved } => Some((paths.clone(), *at, *moved)),
            _ => None,
        })
    });
    assert_eq!(dropped, (vec![PathBuf::from("/tmp/a b.txt")], (11.0, 21.0), true));
    harness.wait(|harness| harness.state.finished.then_some(()));
    assert_eq!(harness.state.accepted.first(), Some(&Some(String::from(URI_LIST))), "the uri-list is accepted");
    assert_eq!(harness.state.actions.first(), Some(&(DndAction::Copy | DndAction::Move, DndAction::Copy)));
    assert!(harness.seen.contains(&Event::Hover(Some((10.0, 20.0)))));
}

fn drop_data(payload: &str, mimes: &[&str]) -> (Harness, (Vec<u8>, Dropped)) {
    let (mut harness, _link) = Harness::start();
    let offer = harness.offer(payload, mimes);
    offer.source_actions(DndAction::Copy | DndAction::Move);
    let surface = harness.state.surface.clone().expect("surface");
    let device = harness.state.device.clone().expect("device");
    device.enter(3, &surface, 5.0, 5.0, Some(&offer));
    harness.pump();
    device.drop();
    let dropped = harness.wait(|harness| {
        harness.seen.iter().find_map(|event| match event {
            Event::DroppedData { bytes, kind, .. } => Some((bytes.clone(), kind.clone())),
            _ => None,
        })
    });
    harness.wait(|harness| harness.state.finished.then_some(()));
    (harness, dropped)
}

#[test]
fn a_browser_image_is_saved_under_the_name_it_offers() {
    let chrome = [
        "text/x-moz-url",
        "text/html",
        "text/plain;charset=utf-8",
        "application/octet-stream;name=\"logo.YcYY.svg\"",
        "chromium/x-web-custom-data",
    ];
    let (harness, (bytes, kind)) = drop_data("<svg/>", &chrome);
    assert_eq!(bytes, b"<svg/>");
    assert!(matches!(kind, Dropped::Named { name: Some(ref name), .. } if name == "logo.YcYY.svg"), "{kind:?}");
    assert_eq!(
        harness.state.accepted.first(),
        Some(&Some(String::from("application/octet-stream;name=\"logo.YcYY.svg\""))),
        "Chrome offers no uri-list for an image, so the file contents are accepted"
    );
    assert_eq!(harness.state.actions.first().map(|(all, _)| *all), Some(DndAction::Copy), "data is only copied");
}

#[test]
fn text_and_web_links_are_saved_as_text() {
    let (_, (bytes, kind)) = drop_data("hello", &["text/plain"]);
    assert_eq!(
        (bytes.as_slice(), kind),
        (b"hello".as_slice(), Dropped::Data { mime: "text/plain", extension: "txt", image: false })
    );
    let (_, (bytes, kind)) = drop_data("https://example.com/\r\n", &[URI_LIST, "text/plain;charset=utf-8"]);
    assert_eq!(bytes, b"https://example.com/\r\n", "a uri-list with no local file falls back to the text");
    assert!(matches!(kind, Dropped::Data { image: false, .. }), "{kind:?}");
}

#[test]
fn a_view_that_takes_no_drops_refuses_files_until_it_takes_them_again() {
    let (mut harness, link) = Harness::start();
    link.set_accepting(false);
    let offer = harness.offer("file:///tmp/a.txt\r\n", &[URI_LIST]);
    offer.source_actions(DndAction::Copy | DndAction::Move);
    let surface = harness.state.surface.clone().expect("surface");
    let device = harness.state.device.clone().expect("device");
    device.enter(9, &surface, 4.0, 4.0, Some(&offer));
    harness.wait(|harness| (!harness.state.accepted.is_empty()).then_some(()));
    assert_eq!(harness.state.accepted, [None], "the Trash view shows no drop cursor");
    assert_eq!(harness.state.actions.first().map(|(all, _)| *all), Some(DndAction::empty()));

    link.set_accepting(true);
    harness.wait(|harness| (harness.state.accepted.len() > 1).then_some(()));
    assert_eq!(harness.state.accepted.last(), Some(&Some(String::from(URI_LIST))), "a folder that takes drops again");
    link.set_accepting(false);
    harness.wait(|harness| (harness.state.accepted.len() > 2).then_some(()));
    device.drop();
    harness.pump();
    std::thread::sleep(Duration::from_millis(100));
    harness.pump();
    assert!(
        !harness.seen.iter().any(|event| matches!(event, Event::Drop { .. })),
        "a drop on a refusing view reaches nothing"
    );
}

#[test]
fn a_drag_out_of_files_carries_the_items_icon_under_the_pointer() {
    use glide_files::drag_icon::{SCALE, SIZE};
    let art = std::env::temp_dir().join(format!("glide-drag-art-{}.svg", std::process::id()));
    std::fs::write(&art, r##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><rect width="16" height="16" fill="#3b82f6"/></svg>"##)
        .expect("art");
    let side = (SIZE * SCALE).cast_signed();
    for (count, expected) in [(1, side), (3, side + side / 8)] {
        let (mut harness, link) = Harness::start();
        let paths = (0..count).map(|index| PathBuf::from(format!("/tmp/{index}.txt"))).collect();
        assert!(link.send(Command::StartDrag { paths, art: Some(art.clone()) }));
        let icon = harness.wait(|harness| harness.state.icon.clone());
        let (surface, width, height) = harness.wait(|harness| harness.state.shown.last().cloned());
        assert_eq!(surface, icon, "the buffer goes on the surface handed to start_drag");
        assert_eq!((width, height), (expected, expected), "{count} items: a stack draws a second icon behind");
        assert_eq!(
            harness.state.scales.last(),
            Some(&SCALE.cast_signed()),
            "drawn at twice the size for sharp scaling"
        );
        harness.state.dragged.clone().expect("source").dnd_finished();
        harness.wait(|harness| harness.seen.contains(&Event::DragEnded).then_some(()));
    }
    let _ = std::fs::remove_file(art);
}

#[test]
fn the_drag_icon_sits_beside_the_pointer_where_the_in_window_ghost_does() {
    let art = std::env::temp_dir().join(format!("glide-drag-offset-{}.svg", std::process::id()));
    std::fs::write(
        &art,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><rect width="16" height="16"/></svg>"#,
    )
    .expect("art");
    for version in [4, 6] {
        let (mut harness, link) = Harness::with_compositor(version);
        assert!(link.send(Command::StartDrag { paths: vec![PathBuf::from("/tmp/a.txt")], art: Some(art.clone()) }));
        harness.wait(|harness| harness.state.shown.last().cloned());
        harness.wait(|harness| (harness.state.moved != (0, 0)).then_some(()));
        assert_eq!(
            harness.state.moved,
            glide_files::drag_icon::OFFSET,
            "wl_compositor v{version}: the icon's corner is moved off the hotspot"
        );
        harness.state.dragged.clone().expect("source").dnd_finished();
        harness.wait(|harness| harness.seen.contains(&Event::DragEnded).then_some(()));
    }
    let _ = std::fs::remove_file(art);
}

#[test]
fn dragging_out_of_files_offers_uris_and_ends_when_the_target_finishes() {
    let (mut harness, link) = Harness::start();
    assert!(link.send(Command::StartDrag { paths: vec![PathBuf::from("/tmp/x y.png")], art: None }));
    let source = harness.wait(|harness| harness.state.dragged.clone());
    assert!(harness.state.icon.is_none(), "no art, no icon surface");
    let offered = source.data::<Offered>().expect("offers").lock().expect("offers").clone();
    assert_eq!(offered, [URI_LIST, GNOME, "text/plain;charset=utf-8"]);
    assert_eq!(harness.read_source(&source, URI_LIST), "file:///tmp/x%20y.png\r\n");
    assert_eq!(harness.read_source(&source, GNOME), "copy\nfile:///tmp/x%20y.png");
    source.dnd_finished();
    harness.wait(|harness| harness.seen.contains(&Event::DragEnded).then_some(()));
}

#[test]
fn copy_and_paste_go_through_the_system_clipboard() {
    let (mut harness, link) = Harness::start();
    assert!(link.send(Command::SetSelection { paths: vec![PathBuf::from("/tmp/cut me.txt")], cut: true }));
    let source = harness.wait(|harness| harness.state.selection.clone());
    assert_eq!(harness.read_source(&source, GNOME), "cut\nfile:///tmp/cut%20me.txt");

    let offer = harness.offer("copy\nfile:///tmp/p.txt", &[GNOME, URI_LIST]);
    harness.state.device.clone().expect("device").selection(Some(&offer));
    harness.pump();
    assert!(link.send(Command::Paste(PathBuf::from("/tmp/target"))));
    let pasted = harness.wait(|harness| {
        harness.seen.iter().find_map(|event| match event {
            Event::Pasted { paths, target, cut } => Some((paths.clone(), target.clone(), *cut)),
            _ => None,
        })
    });
    assert_eq!(pasted, (vec![PathBuf::from("/tmp/p.txt")], PathBuf::from("/tmp/target"), false));

    source.cancelled();
    harness.wait(|harness| harness.seen.contains(&Event::SelectionLost).then_some(()));
}

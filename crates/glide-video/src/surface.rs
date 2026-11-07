use std::os::fd::{AsFd, FromRawFd, OwnedFd};

use glide_app::menubar::Handles;
use wayland_backend::client::{Backend, ObjectId};
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::{
    wl_buffer, wl_compositor, wl_region, wl_registry, wl_shm, wl_shm_pool, wl_subcompositor, wl_subsurface, wl_surface,
};
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle};
use wayland_protocols::wp::content_type::v1::client::wp_content_type_manager_v1::WpContentTypeManagerV1;
use wayland_protocols::wp::content_type::v1::client::wp_content_type_v1::{self, WpContentTypeV1};
use wayland_protocols::wp::idle_inhibit::zv1::client::zwp_idle_inhibit_manager_v1::ZwpIdleInhibitManagerV1;
use wayland_protocols::wp::idle_inhibit::zv1::client::zwp_idle_inhibitor_v1::ZwpIdleInhibitorV1;

struct State;

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut Self, _: &wl_registry::WlRegistry, _: wl_registry::Event, _: &GlobalListContents, _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

wayland_client::delegate_noop!(State: ignore wl_compositor::WlCompositor);
wayland_client::delegate_noop!(State: ignore wl_subcompositor::WlSubcompositor);
wayland_client::delegate_noop!(State: ignore wl_subsurface::WlSubsurface);
wayland_client::delegate_noop!(State: ignore wl_surface::WlSurface);
wayland_client::delegate_noop!(State: ignore wl_region::WlRegion);
wayland_client::delegate_noop!(State: ignore wl_shm::WlShm);
wayland_client::delegate_noop!(State: ignore wl_shm_pool::WlShmPool);
wayland_client::delegate_noop!(State: ignore wl_buffer::WlBuffer);
wayland_client::delegate_noop!(State: ignore WpContentTypeManagerV1);
wayland_client::delegate_noop!(State: ignore WpContentTypeV1);
wayland_client::delegate_noop!(State: ignore ZwpIdleInhibitManagerV1);
wayland_client::delegate_noop!(State: ignore ZwpIdleInhibitorV1);

pub struct Surface {
    display: usize,
    queue: EventQueue<State>,
    parent: wl_surface::WlSurface,
    child: wl_surface::WlSurface,
    subsurface: wl_subsurface::WlSubsurface,
    buffer: wl_buffer::WlBuffer,
    content: Option<WpContentTypeV1>,
    idle: Option<ZwpIdleInhibitManagerV1>,
    inhibitor: Option<ZwpIdleInhibitorV1>,
}

fn transparent_pixel() -> std::io::Result<OwnedFd> {
    // SAFETY: the name is a NUL-terminated literal and the flags are valid for `memfd_create`.
    let raw = unsafe { libc::memfd_create(c"glide-video-pixel".as_ptr(), libc::MFD_CLOEXEC) };
    if raw < 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: `memfd_create` succeeded, so `raw` is an open descriptor owned by nobody else.
    let fd = unsafe { OwnedFd::from_raw_fd(raw) };
    let file = std::fs::File::from(fd);
    file.set_len(4)?;
    Ok(OwnedFd::from(file))
}

impl Surface {
    pub fn new(handles: Handles) -> Result<Self, String> {
        let (display, parent) = (handles.display(), handles.surface());
        if display == 0 || parent == 0 {
            return Err("the window has no Wayland surface".into());
        }
        // SAFETY: `Handles` can only be built through its unsafe constructor, whose caller vouches that `display`
        // is the live wl_display of a window that stays alive for the rest of the process.
        let backend = unsafe { Backend::from_foreign_display(display as *mut _) };
        let connection = Connection::from_backend(backend);
        let (globals, mut queue): (_, EventQueue<State>) =
            registry_queue_init(&connection).map_err(|err| err.to_string())?;
        let qh = queue.handle();
        let compositor: wl_compositor::WlCompositor =
            globals.bind(&qh, 1..=4, ()).map_err(|err| format!("wl_compositor: {err}"))?;
        let subcompositor: wl_subcompositor::WlSubcompositor =
            globals.bind(&qh, 1..=1, ()).map_err(|err| format!("wl_subcompositor: {err}"))?;
        let shm: wl_shm::WlShm = globals.bind(&qh, 1..=1, ()).map_err(|err| format!("wl_shm: {err}"))?;
        // SAFETY: as above; `parent` is that window's live wl_surface on the same display.
        let id = unsafe { ObjectId::from_ptr(wl_surface::WlSurface::interface(), parent as *mut _) }
            .map_err(|err| err.to_string())?;
        let parent = wl_surface::WlSurface::from_id(&connection, id).map_err(|err| err.to_string())?;
        let surface = compositor.create_surface(&qh, ());
        let subsurface = subcompositor.get_subsurface(&surface, &parent, &qh, ());
        subsurface.place_below(&parent);
        subsurface.set_position(0, 0);
        subsurface.set_desync();
        let empty = compositor.create_region(&qh, ());
        surface.set_input_region(Some(&empty));
        empty.destroy();
        let memory = transparent_pixel().map_err(|err| err.to_string())?;
        let pool = shm.create_pool(memory.as_fd(), 4, &qh, ());
        let buffer = pool.create_buffer(0, 1, 1, 4, wl_shm::Format::Argb8888, &qh, ());
        pool.destroy();
        surface.attach(Some(&buffer), 0, 0);
        surface.damage(0, 0, 1, 1);
        let content = globals.bind::<WpContentTypeManagerV1, _, _>(&qh, 1..=1, ()).ok().map(|manager| {
            let content = manager.get_surface_content_type(&surface, &qh, ());
            content.set_content_type(wp_content_type_v1::Type::Video);
            manager.destroy();
            content
        });
        surface.commit();
        let idle: Option<ZwpIdleInhibitManagerV1> = globals.bind(&qh, 1..=1, ()).ok();
        queue.roundtrip(&mut State).map_err(|err| err.to_string())?;
        Ok(Self { display, queue, parent, child: surface, subsurface, buffer, content, idle, inhibitor: None })
    }

    #[must_use]
    pub fn display(&self) -> usize {
        self.display
    }

    #[must_use]
    pub fn handle(&self) -> usize {
        self.child.id().as_ptr() as usize
    }

    pub fn inhibit_idle(&mut self, on: bool) {
        match (on, self.inhibitor.take(), &self.idle) {
            (true, None, Some(manager)) => {
                self.inhibitor = Some(manager.create_inhibitor(&self.parent, &self.queue.handle(), ()));
            }
            (true, kept @ Some(_), _) => self.inhibitor = kept,
            (false, Some(inhibitor), _) => inhibitor.destroy(),
            _ => {}
        }
        self.flush();
    }

    #[must_use]
    pub fn inhibiting(&self) -> bool {
        self.inhibitor.is_some()
    }

    fn flush(&mut self) {
        if let Err(err) = self.queue.dispatch_pending(&mut State) {
            tracing::debug!("video surface queue: {err}");
        }
        if let Err(err) = self.queue.flush() {
            tracing::debug!("video surface flush: {err}");
        }
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        if let Some(inhibitor) = self.inhibitor.take() {
            inhibitor.destroy();
        }
        if let Some(content) = self.content.take() {
            content.destroy();
        }
        self.subsurface.destroy();
        self.child.destroy();
        self.buffer.destroy();
        self.flush();
    }
}

use wayland_backend::client::{Backend, ObjectId};
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::{wl_registry, wl_surface};
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle};
use wayland_protocols::xdg::activation::v1::client::xdg_activation_v1::XdgActivationV1;

struct Activator;

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for Activator {
    fn event(
        _: &mut Self, _: &wl_registry::WlRegistry, _: wl_registry::Event, _: &GlobalListContents, _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

wayland_client::delegate_noop!(Activator: ignore XdgActivationV1);

pub fn activate(handles: crate::menubar::Handles, token: &str) -> Result<(), String> {
    let (display, surface) = (handles.display, handles.surface);
    if token.is_empty() || display == 0 || surface == 0 {
        return Err("no activation token or window".into());
    }
    // SAFETY: `Handles` can only be built through its unsafe constructor, whose caller vouches that both are the
    // live wl_display and wl_surface of a window that stays alive for the rest of the process.
    let backend = unsafe { Backend::from_foreign_display(display as *mut _) };
    let connection = Connection::from_backend(backend);
    // SAFETY: as above; `surface` is that window's live wl_surface on the same display.
    let id = unsafe { ObjectId::from_ptr(wl_surface::WlSurface::interface(), surface as *mut _) }
        .map_err(|err| err.to_string())?;
    let surface = wl_surface::WlSurface::from_id(&connection, id).map_err(|err| err.to_string())?;
    let (globals, mut queue): (_, EventQueue<Activator>) =
        registry_queue_init(&connection).map_err(|err| err.to_string())?;
    let qh = queue.handle();
    let activation: XdgActivationV1 = globals.bind(&qh, 1..=1, ()).map_err(|err| err.to_string())?;
    activation.activate(token.to_string(), &surface);
    queue.roundtrip(&mut Activator).map_err(|err| err.to_string())?;
    activation.destroy();
    Ok(())
}

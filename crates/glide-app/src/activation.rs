use std::sync::{Mutex, PoisonError};

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

struct Bound {
    display: usize,
    connection: Connection,
    queue: EventQueue<Activator>,
    activation: XdgActivationV1,
}

static BOUND: Mutex<Option<Bound>> = Mutex::new(None);

fn bind(display: usize) -> Result<Bound, String> {
    // SAFETY: `Handles` can only be built through its unsafe constructor, whose caller vouches that `display` is
    // the live wl_display of a window that stays alive for the rest of the process.
    let backend = unsafe { Backend::from_foreign_display(display as *mut _) };
    let connection = Connection::from_backend(backend);
    let (globals, queue): (_, EventQueue<Activator>) =
        registry_queue_init(&connection).map_err(|err| err.to_string())?;
    let activation: XdgActivationV1 = globals.bind(&queue.handle(), 1..=1, ()).map_err(|err| err.to_string())?;
    Ok(Bound { display, connection, queue, activation })
}

pub fn activate(handles: crate::menubar::Handles, token: &str) -> Result<(), String> {
    let (display, surface) = (handles.display, handles.surface);
    if token.is_empty() || display == 0 || surface == 0 {
        return Err("no activation token or window".into());
    }
    let mut bound = BOUND.lock().unwrap_or_else(PoisonError::into_inner);
    if bound.as_ref().is_none_or(|known| known.display != display) {
        if let Some(old) = bound.take() {
            old.activation.destroy();
        }
        *bound = Some(bind(display)?);
    }
    let Some(known) = bound.as_mut() else { return Err("no activation global".into()) };
    // SAFETY: as above; `surface` is that window's live wl_surface on the same display.
    let id = unsafe { ObjectId::from_ptr(wl_surface::WlSurface::interface(), surface as *mut _) }
        .map_err(|err| err.to_string())?;
    let surface = wl_surface::WlSurface::from_id(&known.connection, id).map_err(|err| err.to_string())?;
    known.activation.activate(token.to_string(), &surface);
    if let Err(err) = known.queue.roundtrip(&mut Activator) {
        *bound = None;
        return Err(err.to_string());
    }
    Ok(())
}

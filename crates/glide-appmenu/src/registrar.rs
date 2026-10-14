use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use zbus::{message::Header, object_server::SignalEmitter, zvariant::OwnedObjectPath};

use crate::{
    client::{AppMenuEvent, Sink},
    model::Source,
};

pub const REGISTRAR_NAME: &str = "com.canonical.AppMenu.Registrar";
pub const REGISTRAR_PATH: &str = "/com/canonical/AppMenu/Registrar";

struct Registrar {
    windows: Arc<Mutex<HashMap<u32, (String, OwnedObjectPath)>>>,
    sink: Sink,
    connection: zbus::blocking::Connection,
}

impl Registrar {
    fn windows(&self) -> std::sync::MutexGuard<'_, HashMap<u32, (String, OwnedObjectPath)>> {
        self.windows.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    async fn pid_of(&self, sender: &str) -> Option<u32> {
        let proxy = zbus::fdo::DBusProxy::new(self.connection.inner()).await.ok()?;
        let name = zbus::names::BusName::try_from(sender).ok()?;
        proxy.get_connection_unix_process_id(name).await.ok()
    }
}

#[zbus::interface(name = "com.canonical.AppMenu.Registrar")]
impl Registrar {
    async fn register_window(
        &self, window_id: u32, menu_object_path: OwnedObjectPath, #[zbus(header)] header: Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) {
        let Some(sender) = header.sender().map(ToString::to_string) else {
            return;
        };
        self.windows().insert(window_id, (sender.clone(), menu_object_path.clone()));
        let pid = self.pid_of(&sender).await;
        (self.sink)(AppMenuEvent::Registered {
            window_id,
            source: Source::DbusMenu { bus: sender.clone(), path: menu_object_path.to_string() },
            pid,
        });
        let _ = Self::window_registered(&emitter, window_id, &sender, menu_object_path).await;
    }

    async fn unregister_window(&self, window_id: u32, #[zbus(signal_emitter)] emitter: SignalEmitter<'_>) {
        let removed = self.windows().remove(&window_id);
        if removed.is_some() {
            (self.sink)(AppMenuEvent::Unregistered { window_id });
            let _ = Self::window_unregistered(&emitter, window_id).await;
        }
    }

    fn get_menu_for_window(&self, window_id: u32) -> (String, OwnedObjectPath) {
        self.windows()
            .get(&window_id)
            .cloned()
            .unwrap_or_else(|| (String::new(), OwnedObjectPath::try_from("/").expect("root is a valid object path")))
    }

    #[zbus(signal)]
    async fn window_registered(
        emitter: &SignalEmitter<'_>, window_id: u32, service: &str, path: OwnedObjectPath,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn window_unregistered(emitter: &SignalEmitter<'_>, window_id: u32) -> zbus::Result<()>;
}

pub fn serve(connection: &zbus::blocking::Connection, sink: Sink) -> anyhow::Result<()> {
    let registrar = Registrar { windows: Arc::default(), sink, connection: connection.clone() };
    connection.object_server().at(REGISTRAR_PATH, registrar)?;
    connection.request_name(REGISTRAR_NAME)?;
    Ok(())
}

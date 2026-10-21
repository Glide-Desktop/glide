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
const REGISTRAR_INTERFACE: &str = "com.canonical.AppMenu.Registrar";

type Windows = HashMap<u32, (String, OwnedObjectPath)>;

fn lock(windows: &Mutex<Windows>) -> std::sync::MutexGuard<'_, Windows> {
    windows.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn remove_owned(windows: &mut Windows, window_id: u32, sender: &str) -> bool {
    if windows.get(&window_id).is_some_and(|(owner, _)| owner == sender) {
        windows.remove(&window_id);
        return true;
    }
    false
}

fn remove_owner(windows: &mut Windows, owner: &str) -> Vec<u32> {
    let mut gone: Vec<u32> = windows.iter().filter(|(_, (sender, _))| sender == owner).map(|(id, _)| *id).collect();
    gone.sort_unstable();
    for id in &gone {
        windows.remove(id);
    }
    gone
}

struct Registrar {
    windows: Arc<Mutex<Windows>>,
    sink: Sink,
    connection: zbus::blocking::Connection,
}

impl Registrar {
    fn windows(&self) -> std::sync::MutexGuard<'_, Windows> {
        lock(&self.windows)
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

    async fn unregister_window(
        &self, window_id: u32, #[zbus(header)] header: Header<'_>, #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) {
        let Some(sender) = header.sender().map(ToString::to_string) else {
            return;
        };
        let removed = remove_owned(&mut self.windows(), window_id, &sender);
        if removed {
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
    let windows: Arc<Mutex<Windows>> = Arc::default();
    let registrar = Registrar { windows: windows.clone(), sink: sink.clone(), connection: connection.clone() };
    connection.object_server().at(REGISTRAR_PATH, registrar)?;
    connection.request_name(REGISTRAR_NAME)?;
    watch_owners(connection, windows, sink);
    Ok(())
}

fn watch_owners(connection: &zbus::blocking::Connection, windows: Arc<Mutex<Windows>>, sink: Sink) {
    let signals = zbus::blocking::fdo::DBusProxy::new(connection).and_then(|proxy| proxy.receive_name_owner_changed());
    let signals = match signals {
        Ok(signals) => signals,
        Err(err) => {
            tracing::debug!("registered menus will not follow vanished clients: {err}");
            return;
        }
    };
    let connection = connection.clone();
    let spawned = std::thread::Builder::new().name("glide-appmenu-owners".into()).spawn(move || {
        for signal in signals {
            let Ok(args) = signal.args() else {
                continue;
            };
            if args.new_owner().is_some() {
                continue;
            }
            let gone = remove_owner(&mut lock(&windows), args.name().as_str());
            for window_id in gone {
                sink(AppMenuEvent::Unregistered { window_id });
                let emitted = connection.emit_signal(
                    None::<()>,
                    REGISTRAR_PATH,
                    REGISTRAR_INTERFACE,
                    "WindowUnregistered",
                    &(window_id,),
                );
                if let Err(err) = emitted {
                    tracing::debug!("could not announce an unregistered window: {err}");
                }
            }
        }
    });
    if let Err(err) = spawned {
        tracing::debug!("failed to spawn the registrar owner watcher: {err}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn windows() -> Windows {
        let path = OwnedObjectPath::try_from("/menu").expect("valid path");
        [
            (1, (":1.5".to_string(), path.clone())),
            (2, (":1.5".to_string(), path.clone())),
            (3, (":1.6".to_string(), path)),
        ]
        .into_iter()
        .collect()
    }

    #[test]
    fn only_the_registering_client_can_unregister_a_window() {
        let mut windows = windows();
        assert!(!remove_owned(&mut windows, 1, ":1.6"));
        assert!(windows.contains_key(&1));
        assert!(remove_owned(&mut windows, 1, ":1.5"));
        assert!(!windows.contains_key(&1));
        assert!(!remove_owned(&mut windows, 1, ":1.5"));
    }

    #[test]
    fn a_vanished_client_takes_its_windows_with_it() {
        let mut windows = windows();
        assert_eq!(remove_owner(&mut windows, ":1.5"), [1, 2]);
        assert_eq!(windows.keys().copied().collect::<Vec<_>>(), [3]);
        assert!(remove_owner(&mut windows, ":1.9").is_empty());
    }
}

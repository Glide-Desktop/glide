use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use zbus::{message::Header, object_server::SignalEmitter, zvariant::OwnedObjectPath};

use crate::{
    client::{AppMenuEvent, Sink},
    model::Source,
};

pub const REGISTRAR_NAME: &str = "com.canonical.AppMenu.Registrar";
pub const REGISTRAR_PATH: &str = "/com/canonical/AppMenu/Registrar";
const REGISTRAR_INTERFACE: &str = "com.canonical.AppMenu.Registrar";

struct Registration {
    owner: String,
    path: OwnedObjectPath,
    generation: u64,
}

type Windows = HashMap<u32, Registration>;

fn lock(windows: &Mutex<Windows>) -> std::sync::MutexGuard<'_, Windows> {
    windows.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn claim(
    windows: &mut Windows, generations: &AtomicU64, window_id: u32, sender: &str, path: &OwnedObjectPath,
) -> Option<u64> {
    if windows.get(&window_id).is_some_and(|known| known.owner != sender) {
        return None;
    }
    let generation = generations.fetch_add(1, Ordering::Relaxed);
    windows.insert(window_id, Registration { owner: sender.to_string(), path: path.clone(), generation });
    Some(generation)
}

fn announce(windows: &Windows, sink: &Sink, window_id: u32, generation: u64, pid: Option<u32>) -> bool {
    let Some(known) = windows.get(&window_id).filter(|known| known.generation == generation) else {
        return false;
    };
    sink(AppMenuEvent::Registered {
        window_id,
        source: Source::DbusMenu { bus: known.owner.clone(), path: known.path.to_string() },
        pid,
    });
    true
}

fn remove_owned(windows: &mut Windows, window_id: u32, sender: &str) -> bool {
    if windows.get(&window_id).is_some_and(|known| known.owner == sender) {
        windows.remove(&window_id);
        return true;
    }
    false
}

fn remove_owner(windows: &mut Windows, owner: &str) -> Vec<u32> {
    let mut gone: Vec<u32> = windows.iter().filter(|(_, known)| known.owner == owner).map(|(id, _)| *id).collect();
    gone.sort_unstable();
    for id in &gone {
        windows.remove(id);
    }
    gone
}

fn unregister(windows: &mut Windows, sink: &Sink, window_id: u32, sender: &str) -> bool {
    let removed = remove_owned(windows, window_id, sender);
    if removed {
        sink(AppMenuEvent::Unregistered { window_id });
    }
    removed
}

fn drop_owner(windows: &mut Windows, sink: &Sink, owner: &str) -> Vec<u32> {
    let gone = remove_owner(windows, owner);
    for window_id in &gone {
        sink(AppMenuEvent::Unregistered { window_id: *window_id });
    }
    gone
}

struct Registrar {
    windows: Arc<Mutex<Windows>>,
    generations: AtomicU64,
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
        let claimed = claim(&mut self.windows(), &self.generations, window_id, &sender, &menu_object_path);
        let Some(generation) = claimed else {
            return;
        };
        let pid = self.pid_of(&sender).await;
        let announced = announce(&self.windows(), &self.sink, window_id, generation, pid);
        if announced {
            let _ = Self::window_registered(&emitter, window_id, &sender, menu_object_path).await;
        }
    }

    async fn unregister_window(
        &self, window_id: u32, #[zbus(header)] header: Header<'_>, #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) {
        let Some(sender) = header.sender().map(ToString::to_string) else {
            return;
        };
        let removed = unregister(&mut self.windows(), &self.sink, window_id, &sender);
        if removed {
            let _ = Self::window_unregistered(&emitter, window_id).await;
        }
    }

    fn get_menu_for_window(&self, window_id: u32) -> (String, OwnedObjectPath) {
        self.windows().get(&window_id).map_or_else(
            || (String::new(), OwnedObjectPath::try_from("/").expect("root is a valid object path")),
            |known| (known.owner.clone(), known.path.clone()),
        )
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
    let registrar = Registrar {
        windows: windows.clone(),
        generations: AtomicU64::new(0),
        sink: sink.clone(),
        connection: connection.clone(),
    };
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
            let gone = drop_owner(&mut lock(&windows), &sink, args.name().as_str());
            for window_id in gone {
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

    fn path(text: &str) -> OwnedObjectPath {
        OwnedObjectPath::try_from(text).expect("valid path")
    }

    fn windows() -> Windows {
        let registration =
            |owner: &str, generation| Registration { owner: owner.to_string(), path: path("/menu"), generation };
        [(1, registration(":1.5", 0)), (2, registration(":1.5", 1)), (3, registration(":1.6", 2))].into_iter().collect()
    }

    type Seen = Arc<Mutex<Vec<AppMenuEvent>>>;

    fn recorder() -> (Sink, Seen) {
        let seen = Seen::default();
        let log = Arc::clone(&seen);
        let sink: Sink = Arc::new(move |event| log.lock().expect("log").push(event));
        (sink, seen)
    }

    fn registrations_after(events: &[AppMenuEvent]) -> HashMap<u32, Source> {
        let mut known = HashMap::new();
        for event in events {
            match event {
                AppMenuEvent::Registered { window_id, source, .. } => {
                    known.insert(*window_id, source.clone());
                }
                AppMenuEvent::Unregistered { window_id } => {
                    known.remove(window_id);
                }
                AppMenuEvent::Menu { .. } | AppMenuEvent::Gone { .. } => {}
            }
        }
        known
    }

    #[test]
    fn a_window_unregistered_while_its_pid_was_resolved_is_never_announced_afterwards() {
        let mut windows = Windows::new();
        let (sink, seen) = recorder();
        let generations = AtomicU64::new(0);
        let generation = claim(&mut windows, &generations, 42, ":1.5", &path("/menu")).expect("a free id");
        assert!(unregister(&mut windows, &sink, 42, ":1.5"));
        assert!(!announce(&windows, &sink, 42, generation, Some(77)), "the registration is already gone");
        let events = seen.lock().expect("log");
        assert_eq!(*events, [AppMenuEvent::Unregistered { window_id: 42 }]);
        assert!(registrations_after(&events).is_empty(), "the compositor must not keep window 42");
    }

    #[test]
    fn a_window_registered_again_while_its_pid_was_resolved_announces_only_the_latest_path() {
        let mut windows = Windows::new();
        let (sink, seen) = recorder();
        let generations = AtomicU64::new(0);
        let first = claim(&mut windows, &generations, 7, ":1.5", &path("/old")).expect("a free id");
        let second = claim(&mut windows, &generations, 7, ":1.5", &path("/new")).expect("the owner may re-register");
        assert!(!announce(&windows, &sink, 7, first, Some(1)));
        assert!(announce(&windows, &sink, 7, second, Some(1)));
        assert_eq!(
            *seen.lock().expect("log"),
            [AppMenuEvent::Registered {
                window_id: 7,
                source: Source::DbusMenu { bus: ":1.5".into(), path: "/new".into() },
                pid: Some(1),
            }]
        );
    }

    #[test]
    fn a_window_owned_by_another_client_cannot_be_claimed() {
        let mut windows = windows();
        let generations = AtomicU64::new(10);
        assert_eq!(claim(&mut windows, &generations, 1, ":1.6", &path("/other")), None);
        assert_eq!(windows[&1].owner, ":1.5");
        assert_eq!(generations.load(Ordering::Relaxed), 10, "a refused claim takes no generation");
    }

    #[test]
    fn a_vanished_clients_windows_reach_the_sink_in_the_step_that_removes_them() {
        let mut windows = windows();
        let (sink, seen) = recorder();
        assert_eq!(drop_owner(&mut windows, &sink, ":1.5"), [1, 2]);
        assert_eq!(
            *seen.lock().expect("log"),
            [AppMenuEvent::Unregistered { window_id: 1 }, AppMenuEvent::Unregistered { window_id: 2 }]
        );
        assert!(drop_owner(&mut windows, &sink, ":1.5").is_empty());
        assert_eq!(seen.lock().expect("log").len(), 2, "a second pass announces nothing");
    }

    #[test]
    fn only_a_real_unregistration_reaches_the_sink() {
        let mut windows = windows();
        let (sink, seen) = recorder();
        assert!(!unregister(&mut windows, &sink, 1, ":1.6"));
        assert!(!unregister(&mut windows, &sink, 99, ":1.5"));
        assert!(seen.lock().expect("log").is_empty());
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

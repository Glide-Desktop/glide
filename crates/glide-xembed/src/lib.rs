pub mod pixmap;
pub mod sni;
pub mod tray;

use std::{
    sync::{Arc, Mutex, PoisonError},
    time::Duration,
};

pub use tray::CONTAINER_CLASS;

const FIRST_RETRY: Duration = Duration::from_secs(2);
const LAST_RETRY: Duration = Duration::from_secs(30);

fn reregister_on_new_watcher(registered: tray::Registered) -> anyhow::Result<()> {
    let connection = zbus::blocking::Connection::session()?;
    let dbus = zbus::blocking::fdo::DBusProxy::new(&connection)?;
    let changes = dbus.receive_name_owner_changed_with_args(&[(0, sni::WATCHER)])?;
    std::thread::Builder::new().name("glide-xembed-watcher".into()).spawn(move || {
        for change in changes {
            let Ok(args) = change.args() else {
                continue;
            };
            if args.new_owner().is_none() {
                continue;
            }
            let names = registered.lock().unwrap_or_else(PoisonError::into_inner).clone();
            for name in names {
                if let Err(err) = sni::register(&connection, &name) {
                    tracing::info!(name, "could not register with the new watcher: {err}");
                }
            }
        }
    })?;
    Ok(())
}

pub fn serve() -> anyhow::Result<()> {
    let registered: tray::Registered = Arc::new(Mutex::new(Vec::new()));
    reregister_on_new_watcher(Arc::clone(&registered))?;
    let control = zbus::blocking::Connection::session()?;
    let mut wait = FIRST_RETRY;
    loop {
        match tray::Host::connect(None, None, control.clone(), Arc::clone(&registered)) {
            Ok(host) => {
                wait = FIRST_RETRY;
                if let Err(err) = host.run() {
                    tracing::warn!("the X11 tray stopped: {err}");
                }
            }
            Err(err) if wait == FIRST_RETRY => tracing::info!("the X11 tray is not available yet: {err}"),
            Err(err) => tracing::debug!("the X11 tray is still not available: {err}"),
        }
        std::thread::sleep(wait);
        wait = (wait * 2).min(LAST_RETRY);
    }
}

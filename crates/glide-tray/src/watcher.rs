use std::sync::{Arc, Mutex, MutexGuard, mpsc};

use zbus::{message::Header, names::BusName, object_server::SignalEmitter};

use crate::{
    model::{Address, Registry},
    service::Request,
};

pub(crate) struct Watcher {
    pub(crate) registry: Arc<Mutex<Registry>>,
    pub(crate) tx: mpsc::Sender<Request>,
}

pub(crate) fn lock(registry: &Mutex<Registry>) -> MutexGuard<'_, Registry> {
    registry.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

async fn has_owner(connection: &zbus::Connection, bus: &str) -> bool {
    let Ok(name) = BusName::try_from(bus) else {
        return false;
    };
    let Ok(proxy) = zbus::fdo::DBusProxy::new(connection).await else {
        return false;
    };
    proxy.name_has_owner(name).await.unwrap_or(false)
}

#[zbus::interface(name = "org.kde.StatusNotifierWatcher")]
impl Watcher {
    async fn register_status_notifier_item(
        &self, service: &str, #[zbus(header)] header: Header<'_>, #[zbus(connection)] connection: &zbus::Connection,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) {
        let sender = header.sender().map(ToString::to_string);
        let Some(address) = Address::parse(service, sender.as_deref()) else {
            tracing::debug!(service, ?sender, "refusing a malformed tray registration");
            return;
        };
        if !has_owner(connection, &address.bus).await {
            tracing::debug!(service, ?sender, "refusing a tray registration whose bus name has no owner");
            return;
        }
        let added = lock(&self.registry).add_item(address.clone());
        if added {
            let _ = Self::status_notifier_item_registered(&emitter, &address.key()).await;
        }
        let _ = self.tx.send(Request::Register(address, sender));
    }

    async fn register_status_notifier_host(
        &self, service: &str, #[zbus(header)] header: Header<'_>, #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) {
        let sender = header.sender().map(ToString::to_string);
        let name = if service.trim().is_empty() { sender.unwrap_or_default() } else { service.to_string() };
        if lock(&self.registry).add_host(&name) {
            let _ = Self::status_notifier_host_registered(&emitter).await;
        }
    }

    #[zbus(property)]
    fn registered_status_notifier_items(&self) -> Vec<String> {
        lock(&self.registry).keys()
    }

    #[zbus(property)]
    fn is_status_notifier_host_registered(&self) -> bool {
        lock(&self.registry).has_hosts()
    }

    #[zbus(property)]
    #[allow(clippy::unused_self)] // zbus properties are methods on the served object.
    fn protocol_version(&self) -> i32 {
        0
    }

    #[zbus(signal)]
    async fn status_notifier_item_registered(emitter: &SignalEmitter<'_>, service: &str) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn status_notifier_item_unregistered(emitter: &SignalEmitter<'_>, service: &str) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn status_notifier_host_registered(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn status_notifier_host_unregistered(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_watcher_reports_a_host_only_while_one_is_registered() {
        let (tx, _rx) = mpsc::channel();
        let watcher = Watcher { registry: Arc::new(Mutex::new(Registry::default())), tx };
        assert!(!watcher.is_status_notifier_host_registered(), "apps must not minimise into a tray nobody shows");
        lock(&watcher.registry).add_host("org.kde.StatusNotifierHost-1");
        assert!(watcher.is_status_notifier_host_registered());
        lock(&watcher.registry).vanish("org.kde.StatusNotifierHost-1");
        assert!(!watcher.is_status_notifier_host_registered());
    }
}

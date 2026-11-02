#![allow(clippy::unused_self)] // zbus properties and methods require &self

use std::sync::Arc;
use tokio::sync::Mutex;
use zbus::object_server::SignalEmitter;
use zbus::{connection, interface};

use crate::protocol::pair::PairState;
use crate::state::PhoneManager;

pub const BUS_NAME: &str = "org.glide.Phone";
pub const OBJECT_PATH: &str = "/org/glide/Phone";

pub struct PhoneDaemon {
    manager: Arc<Mutex<PhoneManager>>,
}

impl PhoneDaemon {
    #[must_use]
    pub fn new(manager: Arc<Mutex<PhoneManager>>) -> Self {
        Self { manager }
    }
}

#[interface(name = "org.glide.Phone")]
impl PhoneDaemon {
    pub async fn list_devices_json(&self) -> String {
        let mgr = self.manager.lock().await;
        let cards = mgr.sorted_cards();
        serde_json::to_string(&cards).unwrap_or_else(|_| "[]".to_string())
    }

    pub async fn ring_phone(&self, device_id: &str) -> bool {
        let mgr = self.manager.lock().await;
        if device_id.is_empty() {
            if let Some(dev) = mgr.devices.values().find(|d| d.pair_state == PairState::Paired && d.connected) {
                return mgr.ring_phone(&dev.id).is_ok();
            }
            if let Some(dev) = mgr.devices.values().find(|d| d.pair_state == PairState::Paired) {
                return mgr.ring_phone(&dev.id).is_ok();
            }
            return false;
        }
        mgr.ring_phone(device_id).is_ok()
    }

    pub async fn stop_alarm(&self, device_id: &str) -> bool {
        let mut mgr = self.manager.lock().await;
        if device_id.is_empty() {
            let ids: Vec<String> = mgr.devices.keys().cloned().collect();
            for id in ids {
                mgr.stop_alarm(&id);
            }
        } else {
            mgr.stop_alarm(device_id);
        }
        true
    }

    pub async fn accept_pair(&self, device_id: &str) -> bool {
        let mut mgr = self.manager.lock().await;
        mgr.accept_pair(device_id, String::new()).is_ok()
    }

    pub async fn reject_pair(&self, device_id: &str) -> bool {
        let mut mgr = self.manager.lock().await;
        mgr.reject_pair(device_id).is_ok()
    }

    pub async fn dismiss_notification(&self, device_id: &str, notif_id: &str) -> bool {
        let mut mgr = self.manager.lock().await;
        mgr.dismiss_notification(device_id, notif_id).is_some()
    }

    pub async fn clear_notifications(&self, device_id: &str) -> bool {
        let mut mgr = self.manager.lock().await;
        if device_id.is_empty() {
            let ids: Vec<String> = mgr.devices.keys().cloned().collect();
            let mut cleared = 0;
            for id in ids {
                cleared += mgr.clear_notifications(&id);
            }
            return cleared > 0;
        }
        mgr.clear_notifications(device_id) > 0
    }

    pub async fn set_clipboard(&self, device_id: &str, content: &str) -> bool {
        let mut mgr = self.manager.lock().await;
        mgr.set_clipboard(device_id, content).is_ok()
    }

    pub async fn clipboard_text(&self) -> String {
        let mgr = self.manager.lock().await;
        mgr.clipboard_text().unwrap_or_default()
    }

    pub async fn share_text(&self, device_id: &str, text: &str) -> bool {
        let mgr = self.manager.lock().await;
        mgr.share_text(device_id, text).is_ok()
    }

    pub async fn share_url(&self, device_id: &str, url: &str) -> bool {
        let mgr = self.manager.lock().await;
        mgr.share_url(device_id, url).is_ok()
    }

    pub async fn take_presenter_event(&self) -> String {
        let mut mgr = self.manager.lock().await;
        mgr.take_presenter_event().and_then(|event| serde_json::to_string(&event).ok()).unwrap_or_default()
    }

    pub async fn is_ringing(&self) -> bool {
        let mgr = self.manager.lock().await;
        mgr.is_ringing()
    }

    #[zbus(signal)]
    pub async fn state_changed(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;
}

pub async fn start_dbus_service(
    manager: Arc<Mutex<PhoneManager>>,
) -> Result<connection::Connection, Box<dyn std::error::Error + Send + Sync>> {
    let daemon = PhoneDaemon::new(manager.clone());

    let conn = connection::Builder::session()?.name(BUS_NAME)?.serve_at(OBJECT_PATH, daemon)?.build().await?;

    let notify = {
        let mgr = manager.lock().await;
        mgr.notify.clone()
    };

    let conn_clone = conn.clone();
    tokio::spawn(async move {
        loop {
            notify.notified().await;
            let _ = conn_clone.emit_signal(None::<()>, OBJECT_PATH, BUS_NAME, "StateChanged", &()).await;
        }
    });

    Ok(conn)
}

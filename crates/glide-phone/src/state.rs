use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Notify;
use tokio::sync::mpsc::{UnboundedSender, WeakUnboundedSender};

use crate::config::{PairedDevice, PhoneConfig};
use crate::crypto::cert::IdentityCert;
use crate::plugins::battery::{BatteryState, handle_battery_packet};
use crate::plugins::clipboard::{create_clipboard_packet, handle_clipboard_packet};
use crate::plugins::find_device::handle_find_this_device;
use crate::plugins::notifications::{
    ActiveNotification, NotificationEvent, create_action_packet, create_dismiss_packet, create_reply_packet,
    create_request_packet, handle_notification_packet_with_payload,
};
use crate::plugins::presenter::{PresenterEvent, handle_presenter_packet};
use crate::plugins::share::{create_share_text_packet, create_share_url_packet, handle_share_packet};
use crate::protocol::identity::DeviceType;
use crate::protocol::packet::{
    NetworkPacket, PACKET_TYPE_BATTERY, PACKET_TYPE_CLIPBOARD, PACKET_TYPE_FINDMYPHONE_REQUEST,
    PACKET_TYPE_NOTIFICATION, PACKET_TYPE_PAIR, PACKET_TYPE_PING, PACKET_TYPE_PRESENTER, PACKET_TYPE_SHARE_REQUEST,
};
use crate::protocol::pair::{PairBody, PairState};
use crate::sound::AlarmHandle;
use crate::ui::PhoneCardData;

pub type PacketSender = UnboundedSender<NetworkPacket>;

const CONNECTING_COOLDOWN: Duration = Duration::from_secs(5);
pub const MAX_KNOWN_DEVICES: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhoneDeviceState {
    pub id: String,
    pub name: String,
    pub device_type: DeviceType,
    pub pair_state: PairState,
    pub battery: Option<BatteryState>,
    pub notifications: HashMap<String, ActiveNotification>,
    pub seen_notifications: HashMap<String, (String, String)>,
    pub peer_fingerprint: Option<String>,
    pub connected: bool,
}

pub struct PhoneManager {
    pub config: PhoneConfig,
    pub cert: IdentityCert,
    pub devices: HashMap<String, PhoneDeviceState>,
    alarms: HashMap<String, AlarmHandle>,
    senders: HashMap<String, PacketSender>,
    connecting: HashMap<String, Instant>,
    clipboard: Option<(String, i64)>,
    presenter_events: VecDeque<(String, PresenterEvent)>,
    pub notify: Arc<Notify>,
}

impl PhoneManager {
    #[must_use]
    pub fn new(config: PhoneConfig, cert: IdentityCert) -> Self {
        let mut devices = HashMap::new();
        for (id, paired) in &config.paired_devices {
            devices.insert(
                id.clone(),
                PhoneDeviceState {
                    id: id.clone(),
                    name: paired.name.clone(),
                    device_type: DeviceType::Phone,
                    pair_state: PairState::Paired,
                    battery: None,
                    notifications: HashMap::new(),
                    seen_notifications: HashMap::new(),
                    peer_fingerprint: if paired.fingerprint.is_empty() {
                        None
                    } else {
                        Some(paired.fingerprint.clone())
                    },
                    connected: false,
                },
            );
        }
        Self {
            config,
            cert,
            devices,
            alarms: HashMap::new(),
            senders: HashMap::new(),
            connecting: HashMap::new(),
            clipboard: None,
            presenter_events: VecDeque::new(),
            notify: Arc::new(Notify::new()),
        }
    }

    #[must_use]
    pub fn is_connected(&self, device_id: &str) -> bool {
        self.senders.contains_key(device_id)
    }

    #[must_use]
    pub fn has_any_connection(&self) -> bool {
        !self.senders.is_empty()
    }

    #[must_use]
    pub fn is_connecting(&self, device_id: &str) -> bool {
        self.connecting.get(device_id).is_some_and(|since| since.elapsed() < CONNECTING_COOLDOWN)
    }

    pub fn mark_connecting(&mut self, device_id: &str) {
        self.connecting.insert(device_id.to_string(), Instant::now());
    }

    pub fn clear_connecting(&mut self, device_id: &str) {
        self.connecting.remove(device_id);
    }

    pub fn register_discovered(&mut self, id: String, name: String, device_type: DeviceType) -> bool {
        let paired_fp = self.config.paired_devices.get(&id).map(|p| p.fingerprint.clone());
        let is_paired = paired_fp.is_some();
        if !is_paired && !self.devices.contains_key(&id) && self.devices.len() >= MAX_KNOWN_DEVICES {
            return false;
        }
        let pair_state = if is_paired { PairState::Paired } else { PairState::Unpaired };

        let entry = self.devices.entry(id.clone()).or_insert_with(|| PhoneDeviceState {
            id,
            name: name.clone(),
            device_type,
            pair_state,
            battery: None,
            notifications: HashMap::new(),
            seen_notifications: HashMap::new(),
            peer_fingerprint: paired_fp.filter(|s| !s.is_empty()),
            connected: false,
        });
        let changed = entry.name != name
            || entry.device_type != device_type
            || (is_paired && entry.pair_state != PairState::Paired);
        entry.name = name;
        entry.device_type = device_type;
        if is_paired {
            entry.pair_state = PairState::Paired;
        }
        if changed {
            self.notify.notify_waiters();
        }
        true
    }

    pub fn register_connection(&mut self, device_id: String, sender: PacketSender, peer_fingerprint: Option<String>) {
        self.connecting.remove(&device_id);
        if let Some(dev) = self.devices.get_mut(&device_id) {
            dev.connected = true;
            if let Some(fp) = peer_fingerprint.filter(|s| !s.is_empty()) {
                dev.peer_fingerprint = Some(fp);
            }
        }
        self.senders.insert(device_id, sender);
        self.notify.notify_waiters();
    }

    pub fn unregister_connection(&mut self, device_id: &str, sender: &WeakUnboundedSender<NetworkPacket>) {
        if !sender
            .upgrade()
            .is_some_and(|sender| self.senders.get(device_id).is_some_and(|current| current.same_channel(&sender)))
        {
            return;
        }
        self.senders.remove(device_id);
        self.connecting.remove(device_id);
        self.presenter_events.retain(|(id, _)| id != device_id);
        if let Some(dev) = self.devices.get_mut(device_id) {
            dev.connected = false;
            if dev.pair_state == PairState::RequestedByPeer {
                dev.pair_state = PairState::Unpaired;
            }
        }
        self.notify.notify_waiters();
    }

    pub fn send_to_device(&self, device_id: &str, packet: NetworkPacket) {
        if let Some(sender) = self.senders.get(device_id) {
            let _ = sender.send(packet);
        }
    }

    pub fn handle_packet(
        &mut self, device_id: &str, packet: &NetworkPacket,
    ) -> Result<Option<NetworkPacket>, Box<dyn std::error::Error + Send + Sync>> {
        self.handle_packet_with_payload(device_id, packet, None)
    }

    pub fn handle_packet_with_payload(
        &mut self, device_id: &str, packet: &NetworkPacket, payload_png: Option<&[u8]>,
    ) -> Result<Option<NetworkPacket>, Box<dyn std::error::Error + Send + Sync>> {
        let Some(device) = self.devices.get(device_id) else {
            return Ok(None);
        };
        let device_name = device.name.clone();

        if device.pair_state != PairState::Paired && packet.packet_type != PACKET_TYPE_PAIR {
            return Ok(None);
        }

        if self.handle_aux_packet(device_id, packet, &device_name) {
            return Ok(None);
        }

        let Some(device) = self.devices.get_mut(device_id) else {
            return Ok(None);
        };

        match packet.packet_type.as_str() {
            PACKET_TYPE_BATTERY => {
                if let Ok(battery_state) = handle_battery_packet(&device_name, packet)
                    && device.battery != Some(battery_state)
                {
                    device.battery = Some(battery_state);
                    self.notify.notify_waiters();
                }
            }
            PACKET_TYPE_NOTIFICATION => {
                if let Ok(Some(event)) = handle_notification_packet_with_payload(
                    &device_name,
                    packet,
                    payload_png,
                    &mut device.notifications,
                    &mut device.seen_notifications,
                ) {
                    match event {
                        NotificationEvent::Added(notification) => {
                            device.notifications.insert(notification.id.clone(), notification);
                        }
                        NotificationEvent::Dismissed(id) => {
                            device.notifications.remove(&id);
                        }
                    }
                    self.notify.notify_waiters();
                }
            }
            PACKET_TYPE_FINDMYPHONE_REQUEST => {
                if let Some(ringing) = self.alarms.remove(device_id).filter(AlarmHandle::is_active) {
                    ringing.stop();
                } else {
                    self.alarms.insert(device_id.to_string(), handle_find_this_device(&device_name));
                }
                self.notify.notify_waiters();
            }
            PACKET_TYPE_PING => {
                let msg = packet.body.get("message").and_then(serde_json::Value::as_str).unwrap_or("Ping!");
                let notif = glide_sdk::system::Notification::new("glide-phone", format!("{device_name}: Ping"))
                    .body(msg)
                    .icon("phone-symbolic");
                if let Ok(handle) = tokio::runtime::Handle::try_current() {
                    handle.spawn(async move {
                        let _ = glide_sdk::system::notify(notif).await;
                    });
                }
            }
            PACKET_TYPE_PAIR => {
                if let Ok(pair_body) = PairBody::from_packet(packet) {
                    if pair_body.pair {
                        if device.pair_state == PairState::RequestedByLocal {
                            device.pair_state = PairState::Paired;
                            let fp = device.peer_fingerprint.clone().unwrap_or_default();
                            self.config.paired_devices.insert(
                                device_id.to_string(),
                                PairedDevice { id: device_id.to_string(), name: device_name, fingerprint: fp },
                            );
                            self.config.save_if_configured();
                            self.send_to_device(device_id, create_request_packet());
                        } else if pair_body.timestamp.is_some()
                            && !matches!(device.pair_state, PairState::Paired | PairState::RequestedByPeer)
                        {
                            device.pair_state = PairState::RequestedByPeer;
                            let notif = glide_sdk::system::Notification::new(
                                "glide-phone",
                                format!("Запит на з'єднання від {device_name}"),
                            )
                            .body("Відкрийте Phone Link, щоб підтвердити підключення.")
                            .icon("phone-symbolic");
                            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                                handle.spawn(async move {
                                    let _ = glide_sdk::system::notify(notif).await;
                                });
                            }
                        }
                    } else {
                        device.pair_state = PairState::Unpaired;
                        self.presenter_events.retain(|(id, _)| id != device_id);
                        self.config.paired_devices.remove(device_id);
                        self.config.save_if_configured();
                    }
                    self.notify.notify_waiters();
                }
            }
            _ => {}
        }

        Ok(None)
    }

    fn handle_aux_packet(&mut self, device_id: &str, packet: &NetworkPacket, device_name: &str) -> bool {
        match packet.packet_type.as_str() {
            PACKET_TYPE_CLIPBOARD => {
                if let Ok(clipboard) = handle_clipboard_packet(packet) {
                    let timestamp = clipboard.timestamp.unwrap_or(packet.id);
                    if self.clipboard.as_ref().is_none_or(|(_, current)| timestamp >= *current) {
                        self.clipboard = Some((clipboard.content, timestamp));
                        self.notify.notify_waiters();
                    }
                }
                true
            }
            PACKET_TYPE_PRESENTER | "kdeconnect.mousepad.request" => {
                if let Some(event) = handle_presenter_packet(packet) {
                    if let (
                        PresenterEvent::Move { dx, dy },
                        Some((id, PresenterEvent::Move { dx: pending_x, dy: pending_y })),
                    ) = (event, self.presenter_events.back_mut())
                        && id == device_id
                        && *pending_x * dx >= 0.0
                        && *pending_y * dy >= 0.0
                    {
                        *pending_x = (*pending_x + dx).clamp(-1.0, 1.0);
                        *pending_y = (*pending_y + dy).clamp(-1.0, 1.0);
                        self.notify.notify_waiters();
                        return true;
                    }
                    if self.presenter_events.len() >= 256 {
                        if let Some(index) = self
                            .presenter_events
                            .iter()
                            .position(|(_, event)| matches!(event, PresenterEvent::Move { .. }))
                        {
                            self.presenter_events.remove(index);
                        } else {
                            self.presenter_events.pop_front();
                        }
                    }
                    self.presenter_events.push_back((device_id.to_string(), event));
                    self.notify.notify_waiters();
                }
                true
            }
            PACKET_TYPE_SHARE_REQUEST => {
                if let Ok(share) = handle_share_packet(packet) {
                    crate::plugins::share::deliver(device_name, share);
                }
                true
            }
            _ => false,
        }
    }

    pub fn accept_pair(
        &mut self, device_id: &str, fingerprint: String,
    ) -> Result<NetworkPacket, Box<dyn std::error::Error + Send + Sync>> {
        if self.devices.get(device_id).is_some_and(|device| device.pair_state == PairState::RequestedByPeer)
            && !self.is_connected(device_id)
            && fingerprint.is_empty()
        {
            return Err("phone disconnected before pairing could be accepted".into());
        }
        let device = self.devices.get_mut(device_id).ok_or_else(|| format!("device not found: {device_id}"))?;

        let has_explicit_fp = !fingerprint.is_empty();
        let resolved_fp = if fingerprint.is_empty() {
            device.peer_fingerprint.clone().unwrap_or_default()
        } else {
            device.peer_fingerprint = Some(fingerprint.clone());
            fingerprint
        };

        let should_finalize = device.pair_state == PairState::RequestedByPeer || has_explicit_fp;
        let pair_body = if should_finalize {
            device.pair_state = PairState::Paired;
            let dev_name = device.name.clone();
            self.config.paired_devices.insert(
                device_id.to_string(),
                PairedDevice { id: device_id.to_string(), name: dev_name, fingerprint: resolved_fp },
            );
            self.config.save_if_configured();
            PairBody::accept()
        } else {
            device.pair_state = PairState::RequestedByLocal;
            PairBody::request()
        };

        let is_now_paired = device.pair_state == PairState::Paired;
        let packet = pair_body.to_packet()?;
        self.send_to_device(device_id, packet.clone());
        if is_now_paired {
            self.send_to_device(device_id, create_request_packet());
        }
        self.notify.notify_waiters();
        Ok(packet)
    }

    pub fn send_pending_pair_request(&self, device_id: &str) {
        if self.devices.get(device_id).is_some_and(|device| device.pair_state == PairState::RequestedByLocal)
            && let Ok(packet) = PairBody::request().to_packet()
        {
            self.send_to_device(device_id, packet);
        }
    }

    pub fn reject_pair(&mut self, device_id: &str) -> Result<NetworkPacket, Box<dyn std::error::Error + Send + Sync>> {
        self.presenter_events.retain(|(id, _)| id != device_id);
        if let Some(device) = self.devices.get_mut(device_id) {
            device.pair_state = PairState::Unpaired;
            device.notifications.clear();
            device.seen_notifications.clear();
        }
        self.config.paired_devices.remove(device_id);
        self.config.save_if_configured();
        let pair_body = PairBody::reject();
        let packet = pair_body.to_packet()?;
        self.send_to_device(device_id, packet.clone());
        self.notify.notify_waiters();
        Ok(packet)
    }

    pub fn dismiss_notification(&mut self, device_id: &str, notif_id: &str) -> Option<NetworkPacket> {
        let device = self.devices.get_mut(device_id)?;
        device.notifications.remove(notif_id)?;
        let packet = create_dismiss_packet(notif_id);
        self.send_to_device(device_id, packet.clone());
        self.notify.notify_waiters();
        Some(packet)
    }

    pub fn clear_notifications(&mut self, device_id: &str) -> usize {
        let ids: Vec<String> = if let Some(device) = self.devices.get_mut(device_id) {
            device.notifications.drain().map(|(id, _)| id).collect()
        } else {
            return 0;
        };
        let count = ids.len();
        for id in ids {
            let packet = create_dismiss_packet(&id);
            self.send_to_device(device_id, packet);
        }
        if count > 0 {
            self.notify.notify_waiters();
        }
        count
    }

    #[must_use]
    pub fn sorted_cards(&self) -> Vec<PhoneCardData> {
        let mut cards: Vec<PhoneCardData> = self
            .devices
            .values()
            .map(|s| {
                let mut notifications: Vec<ActiveNotification> = s.notifications.values().cloned().collect();
                notifications.sort_by(|a, b| b.seq.cmp(&a.seq).then_with(|| a.id.cmp(&b.id)));
                PhoneCardData {
                    id: s.id.clone(),
                    name: s.name.clone(),
                    device_type: s.device_type,
                    pair_state: s.pair_state,
                    battery: s.battery,
                    notifications,
                    peer_fingerprint: s.peer_fingerprint.clone(),
                    connected: s.connected,
                }
            })
            .collect();
        cards.sort_by(|a, b| {
            let rank = |d: &PhoneCardData| match (d.pair_state, d.connected) {
                (PairState::Paired, true) => 0,
                (PairState::RequestedByPeer, _) => 1,
                (PairState::RequestedByLocal, _) => 2,
                (PairState::Paired, false) => 3,
                (PairState::Unpaired, _) => 4,
            };
            rank(a).cmp(&rank(b)).then_with(|| a.name.cmp(&b.name)).then_with(|| a.id.cmp(&b.id))
        });
        cards
    }

    pub fn ring_phone(&self, device_id: &str) -> Result<NetworkPacket, Box<dyn std::error::Error + Send + Sync>> {
        if !self.devices.contains_key(device_id) {
            return Err(format!("unknown device {device_id}").into());
        }
        let packet = crate::plugins::find_device::create_ring_packet();
        self.send_to_device(device_id, packet.clone());
        self.notify.notify_waiters();
        Ok(packet)
    }

    pub fn stop_alarm(&mut self, device_id: &str) {
        if let Some(handle) = self.alarms.remove(device_id) {
            handle.stop();
            self.notify.notify_waiters();
        }
    }

    #[must_use]
    pub fn is_ringing(&self) -> bool {
        self.alarms.values().any(AlarmHandle::is_active)
    }

    #[must_use]
    pub fn reply_notification(&self, device_id: &str, reply_id: &str, message: &str) -> NetworkPacket {
        let packet = create_reply_packet(reply_id, message);
        self.send_to_device(device_id, packet.clone());
        packet
    }

    #[must_use]
    pub fn trigger_notification_action(&self, device_id: &str, key: &str, action: &str) -> NetworkPacket {
        let packet = create_action_packet(key, action);
        self.send_to_device(device_id, packet.clone());
        packet
    }

    pub fn set_clipboard(
        &mut self, device_id: &str, content: &str,
    ) -> Result<NetworkPacket, Box<dyn std::error::Error + Send + Sync>> {
        if !self.is_connected(device_id) {
            return Err(format!("device is not connected: {device_id}").into());
        }
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis().min(i64::MAX as u128) as i64;
        let packet = create_clipboard_packet(content, timestamp);
        self.clipboard = Some((content.to_string(), timestamp));
        self.send_to_device(device_id, packet.clone());
        self.notify.notify_waiters();
        Ok(packet)
    }

    #[must_use]
    pub fn clipboard_text(&self) -> Option<String> {
        self.clipboard.as_ref().map(|(content, _)| content.clone())
    }

    #[must_use]
    pub fn take_presenter_event(&mut self) -> Option<PresenterEvent> {
        self.presenter_events.pop_front().map(|(_, event)| event)
    }

    pub fn share_text(
        &self, device_id: &str, text: &str,
    ) -> Result<NetworkPacket, Box<dyn std::error::Error + Send + Sync>> {
        if !self.is_connected(device_id) {
            return Err(format!("device is not connected: {device_id}").into());
        }
        let packet = create_share_text_packet(text);
        self.send_to_device(device_id, packet.clone());
        Ok(packet)
    }

    pub fn share_url(
        &self, device_id: &str, url: &str,
    ) -> Result<NetworkPacket, Box<dyn std::error::Error + Send + Sync>> {
        if !self.is_connected(device_id) {
            return Err(format!("device is not connected: {device_id}").into());
        }
        let packet = create_share_url_packet(url);
        self.send_to_device(device_id, packet.clone());
        Ok(packet)
    }
}

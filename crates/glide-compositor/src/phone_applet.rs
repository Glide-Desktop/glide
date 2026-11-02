use std::sync::{OnceLock, mpsc};
use std::time::Duration;

use glide_topbar::phone::{PhoneNotification, PhonePairState, PhoneState};
use smithay::backend::input::KeyState;
use smithay::input::keyboard::{FilterResult, KeyboardSource};
use smithay::utils::SERIAL_COUNTER;
use zbus::blocking::{Connection, MessageIterator, fdo::DBusProxy};

const BUS_NAME: &str = "org.glide.Phone";
const OBJECT_PATH: &str = "/org/glide/Phone";
const INTERFACE: &str = "org.glide.Phone";
const POLL_INTERVAL: Duration = Duration::from_secs(2);
const RULES: [&str; 2] = [
    "type='signal',interface='org.glide.Phone',member='StateChanged'",
    "type='signal',sender='org.freedesktop.DBus',interface='org.freedesktop.DBus',member='NameOwnerChanged',arg0='org.glide.Phone'",
];

static POKE: OnceLock<mpsc::SyncSender<()>> = OnceLock::new();

pub fn poke() {
    if let Some(tx) = POKE.get() {
        let _ = tx.try_send(());
    }
}

#[derive(serde::Deserialize)]
struct WireNotification {
    id: String,
    #[serde(default)]
    app_name: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    body: String,
}

#[derive(serde::Deserialize)]
struct WireBattery {
    #[serde(alias = "current_charge")]
    charge: i32,
    #[serde(default)]
    is_charging: bool,
}

#[derive(serde::Deserialize)]
struct WireDevice {
    id: String,
    name: String,
    #[serde(default)]
    connected: bool,
    #[serde(default)]
    pair_state: String,
    #[serde(default)]
    battery: Option<WireBattery>,
    #[serde(default)]
    notifications: Vec<WireNotification>,
}

fn parse_pair_state(raw: &str) -> PhonePairState {
    match raw {
        "RequestedByLocal" => PhonePairState::RequestedByLocal,
        "RequestedByPeer" => PhonePairState::RequestedByPeer,
        "Paired" => PhonePairState::Paired,
        _ => PhonePairState::Unpaired,
    }
}

fn priority(dev: &WireDevice) -> Option<u8> {
    match (dev.pair_state.as_str(), dev.connected) {
        ("RequestedByPeer", _) => Some(3),
        ("Paired", true) => Some(2),
        ("Paired", false) => Some(1),
        _ => None,
    }
}

#[must_use]
pub fn parse_phone_state(devices_json: &str, ringing: bool) -> Option<PhoneState> {
    let devices: Vec<WireDevice> = serde_json::from_str(devices_json).ok()?;
    let chosen = devices.into_iter().filter_map(|dev| Some((priority(&dev)?, dev))).max_by_key(|(prio, _)| *prio)?.1;
    let pair_state = parse_pair_state(&chosen.pair_state);
    let (battery, charging) = match chosen.battery {
        Some(bat) => (u8::try_from(bat.charge.clamp(0, 100)).ok(), bat.is_charging),
        None => (None, false),
    };
    let notifications = chosen
        .notifications
        .into_iter()
        .map(|notif| PhoneNotification { id: notif.id, app_name: notif.app_name, title: notif.title, body: notif.body })
        .collect();
    Some(PhoneState {
        id: chosen.id,
        name: chosen.name,
        connected: chosen.connected,
        pair_state,
        battery,
        charging,
        ringing,
        notifications,
    })
}

fn query_state(connection: &Connection) -> Option<PhoneState> {
    let json: String = connection
        .call_method(Some(BUS_NAME), OBJECT_PATH, Some(INTERFACE), "ListDevicesJson", &())
        .and_then(|reply| reply.body().deserialize())
        .ok()?;
    let ringing: bool = connection
        .call_method(Some(BUS_NAME), OBJECT_PATH, Some(INTERFACE), "IsRinging", &())
        .and_then(|reply| reply.body().deserialize())
        .unwrap_or(false);
    parse_phone_state(&json, ringing)
}

#[derive(Clone, Copy, serde::Deserialize)]
pub enum PresenterEvent {
    Move { dx: f64, dy: f64 },
    Key { code: u32, ctrl: bool },
    Stop,
}

pub fn start(sink: impl Fn(Option<PhoneState>) + Send + 'static, presenter: impl Fn(PresenterEvent) + Send + 'static) {
    let (tx, rx) = mpsc::sync_channel::<()>(1);
    let _ = POKE.set(tx.clone());

    if let Ok(signal_conn) = Connection::session()
        && let Ok(bus) = DBusProxy::new(&signal_conn)
    {
        for rule in RULES {
            if let Ok(match_rule) = zbus::MatchRule::try_from(rule) {
                let _ = bus.add_match_rule(match_rule);
            }
        }
        let _ = std::thread::Builder::new().name("glide-phone-signals".into()).spawn(move || {
            for message in MessageIterator::from(&signal_conn) {
                if message.is_ok() {
                    let _ = tx.try_send(());
                }
            }
        });
    }

    let _ = std::thread::Builder::new().name("glide-phone-applet".into()).spawn(move || {
        let mut query_conn = Connection::session().ok();
        let mut last: Option<PhoneState> = None;
        loop {
            if query_conn.is_none() {
                query_conn = Connection::session().ok();
            }
            let current = query_conn.as_ref().and_then(query_state);
            if let Some(connection) = query_conn.as_ref() {
                for _ in 0..256 {
                    let event: Option<String> = connection
                        .call_method(Some(BUS_NAME), OBJECT_PATH, Some(INTERFACE), "TakePresenterEvent", &())
                        .ok()
                        .and_then(|reply| reply.body().deserialize().ok());
                    let Some(event) = event.filter(|event| !event.is_empty()) else { break };
                    if let Ok(event) = serde_json::from_str(&event) {
                        presenter(event);
                    }
                }
            }
            if current != last {
                last.clone_from(&current);
                sink(current);
            }
            let _ = rx.recv_timeout(POLL_INTERVAL);
        }
    });
}

fn call_daemon(method: &'static str, device_id: Option<String>) {
    let _ = std::thread::Builder::new().name("glide-phone-call".into()).spawn(move || {
        let Ok(connection) = Connection::session() else {
            return;
        };
        if let Some(id) = device_id {
            let _ = connection.call_method(Some(BUS_NAME), OBJECT_PATH, Some(INTERFACE), method, &(id.as_str(),));
        } else {
            let _ = connection.call_method(Some(BUS_NAME), OBJECT_PATH, Some(INTERFACE), method, &());
        }
        poke();
    });
}

impl crate::GlideState {
    pub fn handle_presenter(&mut self, event: PresenterEvent) {
        if self.screen_locked() || self.greeter.is_some() {
            self.session.presenter = None;
            return;
        }
        match event {
            PresenterEvent::Key { code, ctrl } => {
                if !matches!(code, 104 | 109 | 1 | 63 | 78 | 74) {
                    return;
                }
                let Some(keyboard) = self.seat.get_keyboard() else { return };
                let time = smithay::backend::input::InputTime::now();
                let source = KeyboardSource::new_auxiliary();
                let send = |state: &mut Self, key: u32, pressed| {
                    keyboard.input_from_source(
                        source,
                        state,
                        (key + 8).into(),
                        pressed,
                        SERIAL_COUNTER.next_serial(),
                        time,
                        |_, _, _| FilterResult::<()>::Forward,
                    );
                };
                if ctrl {
                    send(self, 29, KeyState::Pressed);
                }
                send(self, code, KeyState::Pressed);
                send(self, code, KeyState::Released);
                if ctrl {
                    send(self, 29, KeyState::Released);
                }
                return;
            }
            PresenterEvent::Move { dx, dy } if dx.is_finite() && dy.is_finite() => {
                let pointer = self.seat.get_pointer().map(|pointer| pointer.current_location()).unwrap_or_default();
                let output = self
                    .session
                    .presenter
                    .as_ref()
                    .and_then(|(_, _, output)| self.space.output_geometry(output).map(|_| output.clone()))
                    .or_else(|| {
                        self.space
                            .outputs()
                            .find(|output| {
                                self.space
                                    .output_geometry(output)
                                    .is_some_and(|area| area.contains(pointer.to_i32_floor()))
                            })
                            .cloned()
                    })
                    .or_else(|| self.space.outputs().next().cloned());
                let Some(output) = output else { return };
                let position = self.session.presenter.as_ref().map_or((0.5, 0.5), |(position, _, _)| *position);
                let ratio = self
                    .space
                    .output_geometry(&output)
                    .map_or(1.0, |area| f64::from(area.size.w) / f64::from(area.size.h.max(1)));
                self.session.presenter = Some((
                    ((position.0 + dx).clamp(0.0, 1.0), (position.1 + dy * ratio).clamp(0.0, 1.0)),
                    std::time::Instant::now(),
                    output,
                ));
                let started = self.session.presenter.as_ref().map(|(_, at, _)| *at);
                let timer = smithay::reexports::calloop::timer::Timer::from_duration(Duration::from_millis(500));
                if let Err(err) = self.loop_handle.insert_source(timer, move |_, (), state| {
                    if state.session.presenter.as_ref().is_some_and(|(_, at, _)| Some(*at) == started) {
                        state.session.presenter = None;
                        state.queue_redraw();
                    }
                    smithay::reexports::calloop::timer::TimeoutAction::Drop
                }) {
                    tracing::warn!(?err, "failed to arm presenter pointer timer");
                }
            }
            PresenterEvent::Move { .. } => return,
            PresenterEvent::Stop => self.session.presenter = None,
        }
        self.queue_redraw();
    }
    pub(crate) fn run_phone_action(&mut self, action: crate::topbar::TopbarAction) {
        use crate::topbar::{OpenPopup, TopbarAction};
        match action {
            TopbarAction::TogglePhone => {
                self.topbar.toggle_popup(OpenPopup::Phone);
                poke();
            }
            TopbarAction::PhoneRing => {
                if let Some(phone) = self.topbar.phone() {
                    if phone.ringing {
                        call_daemon("StopAlarm", None);
                    } else {
                        call_daemon("RingPhone", Some(phone.id.clone()));
                    }
                }
            }
            TopbarAction::PhoneAcceptPair => {
                if let Some(id) = self.topbar.phone().map(|phone| phone.id.clone()) {
                    call_daemon("AcceptPair", Some(id));
                }
            }
            TopbarAction::PhoneRejectPair => {
                if let Some(id) = self.topbar.phone().map(|phone| phone.id.clone()) {
                    self.topbar.close_popup();
                    call_daemon("RejectPair", Some(id));
                }
            }
            TopbarAction::PhoneClearNotifications => {
                if let Some(id) = self.topbar.phone().map(|phone| phone.id.clone()) {
                    call_daemon("ClearNotifications", Some(id));
                }
            }
            TopbarAction::PhoneOpenApp => {
                self.topbar.close_popup();
                if let Err(err) = self.spawn_user_command("glide-phone") {
                    tracing::warn!(%err, "failed to start glide-phone");
                }
            }
            _ => return,
        }
        self.queue_redraw();
    }

    pub fn handle_phone(&mut self, state: Option<PhoneState>) {
        if self.topbar.set_phone(state) {
            self.queue_redraw();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_connected_paired_device_over_offline_paired_device() {
        let json = r#"[
            {"id":"old","name":"Old Phone","connected":false,"pair_state":"Paired","battery":null,"notifications":[]},
            {"id":"pixel","name":"Pixel 6","connected":true,"pair_state":"Paired","battery":{"charge":38,"is_charging":false,"is_low":false},"notifications":[{"id":"1","app_name":"Signal","title":"Alice","body":"Hi"}]}
        ]"#;
        let state = parse_phone_state(json, false).expect("phone state");
        assert_eq!(state.id, "pixel");
        assert_eq!(state.name, "Pixel 6");
        assert!(state.connected);
        assert_eq!(state.battery, Some(38));
        assert!(!state.charging);
        assert_eq!(state.notifications.len(), 1);
    }

    #[test]
    fn incoming_pair_request_takes_priority_over_paired_device() {
        let json = r#"[
            {"id":"pixel","name":"Pixel 8","connected":true,"pair_state":"Paired","battery":null,"notifications":[]},
            {"id":"new","name":"Galaxy S24","connected":true,"pair_state":"RequestedByPeer","battery":null,"notifications":[]}
        ]"#;
        let state = parse_phone_state(json, false).expect("phone state");
        assert_eq!(state.id, "new");
        assert_eq!(state.pair_state, PhonePairState::RequestedByPeer);
    }

    #[test]
    fn unpaired_devices_alone_hide_the_applet() {
        let json = r#"[
            {"id":"stranger","name":"Stranger Phone","connected":false,"pair_state":"Unpaired","battery":null,"notifications":[]}
        ]"#;
        assert!(parse_phone_state(json, false).is_none());
    }
}

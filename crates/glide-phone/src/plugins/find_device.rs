use futures_util::StreamExt as _;
use serde_json::json;

use crate::i18n::{Key, naming, t};
use crate::protocol::packet::{NetworkPacket, PACKET_TYPE_FINDMYPHONE_REQUEST};
use crate::sound::{AlarmHandle, start_alarm_loop};

#[must_use]
pub fn create_ring_packet() -> NetworkPacket {
    NetworkPacket::new(PACKET_TYPE_FINDMYPHONE_REQUEST, json!({}))
}

#[must_use]
pub fn handle_find_this_device(device_name: &str) -> AlarmHandle {
    let handle = start_alarm_loop();

    let notification = glide_sdk::system::Notification::new("glide-phone", naming(Key::Finding, device_name))
        .body(t(Key::FindingHint))
        .icon("audio-volume-high-symbolic")
        .urgency(glide_sdk::system::Urgency::Critical)
        .action("stop", t(Key::StopAlarm));

    if let Ok(handle_rt) = tokio::runtime::Handle::try_current() {
        let alarm = handle.clone();
        handle_rt.spawn(async move {
            let Ok(shown) = glide_sdk::system::notify(notification).await else {
                return;
            };
            let mut events = glide_sdk::system::notification_events();
            while alarm.is_active() {
                match tokio::time::timeout(STOP_POLL, events.next()).await {
                    Ok(Some(event)) if stops_alarm(&event, shown) => alarm.stop(),
                    Ok(None) => break,
                    _ => {}
                }
            }
        });
    }
    handle
}

const STOP_POLL: std::time::Duration = std::time::Duration::from_millis(500);

fn stops_alarm(event: &glide_sdk::system::NotificationEvent, shown: u32) -> bool {
    matches!(event, glide_sdk::system::NotificationEvent::Action { id, .. } if *id == shown)
}

#[cfg(test)]
mod tests {
    use super::*;
    use glide_sdk::system::NotificationEvent;

    #[test]
    fn only_a_click_on_the_alarms_own_notification_stops_it() {
        assert!(stops_alarm(&NotificationEvent::Action { id: 7, key: "stop".into() }, 7));
        assert!(stops_alarm(&NotificationEvent::Action { id: 7, key: "default".into() }, 7), "the body says click");
        assert!(!stops_alarm(&NotificationEvent::Action { id: 8, key: "stop".into() }, 7));
        assert!(!stops_alarm(&NotificationEvent::Replied { id: 7, text: String::new() }, 7));
    }
}

use serde_json::json;

use crate::protocol::packet::{NetworkPacket, PACKET_TYPE_FINDMYPHONE_REQUEST};
use crate::sound::{AlarmHandle, start_alarm_loop};

#[must_use]
pub fn create_ring_packet() -> NetworkPacket {
    NetworkPacket::new(PACKET_TYPE_FINDMYPHONE_REQUEST, json!({}))
}

#[must_use]
pub fn handle_find_this_device(device_name: &str) -> AlarmHandle {
    let handle = start_alarm_loop();

    let title = format!("{device_name} is finding this device!");
    let notification = glide_sdk::system::Notification::new("glide-phone", title)
        .body("Device alarm is ringing. Click to stop.")
        .icon("audio-volume-high-symbolic")
        .urgency(glide_sdk::system::Urgency::Critical)
        .action("stop", "Stop Alarm");

    if let Ok(handle_rt) = tokio::runtime::Handle::try_current() {
        handle_rt.spawn(async move {
            let _ = glide_sdk::system::notify(notification).await;
        });
    }
    handle
}

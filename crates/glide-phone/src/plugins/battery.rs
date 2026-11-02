use serde::{Deserialize, Serialize};

use crate::protocol::packet::{NetworkPacket, PACKET_TYPE_BATTERY};

pub const THRESHOLD_NONE: i32 = 0;
pub const THRESHOLD_BATTERY_LOW: i32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatteryPayload {
    #[serde(rename = "currentCharge")]
    pub current_charge: i32,
    #[serde(rename = "isCharging")]
    pub is_charging: bool,
    #[serde(rename = "thresholdEvent", default)]
    pub threshold_event: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatteryState {
    pub charge: i32,
    pub is_charging: bool,
    pub is_low: bool,
}

impl BatteryPayload {
    pub fn to_packet(&self) -> Result<NetworkPacket, serde_json::Error> {
        let value = serde_json::to_value(self)?;
        Ok(NetworkPacket::new(PACKET_TYPE_BATTERY, value))
    }

    pub fn from_packet(packet: &NetworkPacket) -> Result<Self, serde_json::Error> {
        serde_json::from_value(packet.body.clone())
    }
}

#[must_use]
pub fn local_battery_packet() -> Option<NetworkPacket> {
    let info = glide_shell::battery::read_battery()?;
    let is_charging =
        matches!(info.state, glide_shell::battery::BatteryState::Charging | glide_shell::battery::BatteryState::Full);
    let charge = i32::from(info.pct);
    let threshold_event = if charge <= 15 && !is_charging { THRESHOLD_BATTERY_LOW } else { THRESHOLD_NONE };

    let payload = BatteryPayload { current_charge: charge, is_charging, threshold_event };
    payload.to_packet().ok()
}

pub fn handle_battery_packet(device_name: &str, packet: &NetworkPacket) -> Result<BatteryState, serde_json::Error> {
    let payload = BatteryPayload::from_packet(packet)?;
    let is_low = payload.threshold_event == THRESHOLD_BATTERY_LOW && !payload.is_charging;

    if is_low {
        let title = format!("{device_name}: Low Battery");
        let body = format!("Battery at {}%", payload.current_charge);
        let notification = glide_sdk::system::Notification::new("glide-phone", title)
            .body(body)
            .icon("battery-caution-symbolic")
            .urgency(glide_sdk::system::Urgency::Critical);
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let _ = glide_sdk::system::notify(notification).await;
            });
        }
    }

    Ok(BatteryState { charge: payload.current_charge, is_charging: payload.is_charging, is_low })
}

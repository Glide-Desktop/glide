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

pub fn handle_battery_packet(packet: &NetworkPacket) -> Result<BatteryState, serde_json::Error> {
    let payload = BatteryPayload::from_packet(packet)?;
    let is_low = payload.threshold_event == THRESHOLD_BATTERY_LOW && !payload.is_charging;
    Ok(BatteryState { charge: payload.current_charge, is_charging: payload.is_charging, is_low })
}

#[must_use]
pub fn turned_low(before: Option<BatteryState>, now: BatteryState) -> bool {
    now.is_low && !before.is_some_and(|before| before.is_low)
}

pub fn announce_low(device_name: &str, charge: i32) {
    use crate::i18n::{Key, naming, t};
    let body = t(Key::BatteryAt).replace("{percent}", &charge.to_string());
    crate::plugins::announce(
        glide_sdk::system::Notification::new("glide-phone", naming(Key::LowBattery, device_name))
            .body(body)
            .icon("battery-caution-symbolic")
            .urgency(glide_sdk::system::Urgency::Critical),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_low_battery_is_announced_when_it_turns_low_not_with_every_percent_it_loses() {
        let low = |charge| BatteryState { charge, is_charging: false, is_low: true };
        let fine = BatteryState { charge: 40, is_charging: false, is_low: false };
        assert!(turned_low(None, low(15)), "the first report of a low phone");
        assert!(turned_low(Some(fine), low(15)));
        assert!(!turned_low(Some(low(15)), low(14)), "the phone repeats the event with every report while low");
        assert!(!turned_low(Some(low(14)), fine));
        assert!(turned_low(Some(fine), low(15)), "low again after a charge");
    }
}

use serde::{Deserialize, Serialize};

use crate::protocol::packet::{NetworkPacket, PACKET_TYPE_PRESENTER};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum PresenterEvent {
    Move { dx: f64, dy: f64 },
    Key { code: u32, ctrl: bool },
    Stop,
}

pub fn handle_presenter_packet(packet: &NetworkPacket) -> Option<PresenterEvent> {
    if packet.packet_type == "kdeconnect.mousepad.request" {
        if packet.body.get("alt").and_then(serde_json::Value::as_bool) == Some(true)
            || packet.body.get("super").and_then(serde_json::Value::as_bool) == Some(true)
            || packet.body.get("shift").and_then(serde_json::Value::as_bool) == Some(true)
        {
            return None;
        }
        let ctrl = packet.body.get("ctrl").and_then(serde_json::Value::as_bool).unwrap_or(false);
        let code = match (
            packet.body.get("specialKey").and_then(serde_json::Value::as_u64),
            packet.body.get("key").and_then(serde_json::Value::as_str),
            ctrl,
        ) {
            (Some(8), _, false) => 104,
            (Some(9), _, false) => 109,
            (Some(14), _, false) => 1,
            (Some(25), _, false) => 63,
            (None, Some("+"), true) => 78,
            (None, Some("-"), true) => 74,
            _ => return None,
        };
        return Some(PresenterEvent::Key { code, ctrl });
    }
    if packet.packet_type != PACKET_TYPE_PRESENTER {
        return None;
    }
    if packet.body.get("stop").and_then(serde_json::Value::as_bool) == Some(true) {
        return Some(PresenterEvent::Stop);
    }
    let dx = packet.body.get("dx").and_then(serde_json::Value::as_f64).unwrap_or(0.0);
    let dy = packet.body.get("dy").and_then(serde_json::Value::as_f64).unwrap_or(0.0);
    (dx.is_finite() && dy.is_finite()).then_some(PresenterEvent::Move { dx, dy })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn presenter_motion_preserves_both_axes() {
        assert_eq!(
            handle_presenter_packet(&NetworkPacket::new(PACKET_TYPE_PRESENTER, json!({"dx": 0.1, "dy": -0.2}))),
            Some(PresenterEvent::Move { dx: 0.1, dy: -0.2 })
        );
    }

    #[test]
    fn android_presentation_controls_map_to_their_keys() {
        for (body, code, ctrl) in [
            (json!({"specialKey": 8}), 104, false),
            (json!({"specialKey": 9}), 109, false),
            (json!({"specialKey": 14}), 1, false),
            (json!({"specialKey": 25}), 63, false),
            (json!({"key": "+", "ctrl": true}), 78, true),
            (json!({"key": "-", "ctrl": true}), 74, true),
        ] {
            assert_eq!(
                handle_presenter_packet(&NetworkPacket::new("kdeconnect.mousepad.request", body)),
                Some(PresenterEvent::Key { code, ctrl })
            );
        }
    }

    #[test]
    fn presenter_rejects_unrelated_keyboard_commands() {
        for body in
            [json!({"specialKey": 9, "alt": true}), json!({"key": "q", "ctrl": true}), json!({"specialKey": 21})]
        {
            assert_eq!(handle_presenter_packet(&NetworkPacket::new("kdeconnect.mousepad.request", body)), None);
        }
        assert_eq!(
            handle_presenter_packet(&NetworkPacket::new(PACKET_TYPE_PRESENTER, json!({"stop": true, "dx": 1}))),
            Some(PresenterEvent::Stop)
        );
    }
}

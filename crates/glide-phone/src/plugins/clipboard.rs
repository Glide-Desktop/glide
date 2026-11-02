use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::protocol::packet::{NetworkPacket, PACKET_TYPE_CLIPBOARD};

pub const MAX_CLIPBOARD_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClipboardPayload {
    pub content: String,
    #[serde(default)]
    pub timestamp: Option<i64>,
}

#[must_use]
pub fn create_clipboard_packet(content: &str, timestamp: i64) -> NetworkPacket {
    NetworkPacket::new(PACKET_TYPE_CLIPBOARD, json!({ "content": content, "timestamp": timestamp }))
}

pub fn handle_clipboard_packet(packet: &NetworkPacket) -> Result<ClipboardPayload, &'static str> {
    if packet.packet_type != PACKET_TYPE_CLIPBOARD {
        return Err("unexpected clipboard packet");
    }
    let content = packet.body.get("content").and_then(serde_json::Value::as_str).ok_or("clipboard content missing")?;
    if content.len() > MAX_CLIPBOARD_BYTES {
        return Err("clipboard content exceeds size limit");
    }
    Ok(ClipboardPayload {
        content: content.to_string(),
        timestamp: packet.body.get("timestamp").and_then(serde_json::Value::as_i64),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_round_trip_preserves_timestamp() {
        let packet = create_clipboard_packet("Привіт", 42);
        assert_eq!(
            handle_clipboard_packet(&packet).unwrap(),
            ClipboardPayload { content: "Привіт".into(), timestamp: Some(42) }
        );
    }
}

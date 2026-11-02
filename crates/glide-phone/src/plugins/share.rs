use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::protocol::packet::{NetworkPacket, PACKET_TYPE_SHARE_REQUEST};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SharePayload {
    Text(String),
    Url(String),
    File { filename: String },
}

#[must_use]
pub fn create_share_text_packet(text: &str) -> NetworkPacket {
    NetworkPacket::new(PACKET_TYPE_SHARE_REQUEST, json!({ "text": text }))
}

#[must_use]
pub fn create_share_url_packet(url: &str) -> NetworkPacket {
    NetworkPacket::new(PACKET_TYPE_SHARE_REQUEST, json!({ "url": url }))
}

pub fn handle_share_packet(packet: &NetworkPacket) -> Result<SharePayload, &'static str> {
    if packet.packet_type != PACKET_TYPE_SHARE_REQUEST {
        return Err("unexpected share packet");
    }
    if let Some(text) = packet.body.get("text").and_then(serde_json::Value::as_str) {
        return Ok(SharePayload::Text(text.to_string()));
    }
    if let Some(url) = packet.body.get("url").and_then(serde_json::Value::as_str) {
        return Ok(SharePayload::Url(url.to_string()));
    }
    let filename = packet.body.get("filename").and_then(serde_json::Value::as_str).ok_or("share payload missing")?;
    let filename = filename.rsplit(['/', '\\']).next().unwrap_or_default();
    if filename.is_empty() || filename == "." || filename == ".." {
        return Err("invalid shared filename");
    }
    Ok(SharePayload::File { filename: filename.to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filenames_cannot_escape_the_download_directory() {
        let packet = NetworkPacket::new(PACKET_TYPE_SHARE_REQUEST, json!({"filename": "../../photo.jpg"}));
        assert_eq!(handle_share_packet(&packet).unwrap(), SharePayload::File { filename: "photo.jpg".into() });
    }
}

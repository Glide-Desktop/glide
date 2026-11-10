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

const MAX_SHARED_URL_BYTES: usize = 2048;
const MAX_SHARED_TEXT_BYTES: usize = 1024 * 1024;

fn openable(url: &str) -> Option<&str> {
    let url = url.trim();
    let web = ["http://", "https://"]
        .iter()
        .any(|scheme| url.get(..scheme.len()).is_some_and(|head| head.eq_ignore_ascii_case(scheme)));
    let plain = !url.contains(|c: char| c.is_whitespace() || c.is_control());
    (web && plain && url.len() <= MAX_SHARED_URL_BYTES).then_some(url)
}

fn announce(summary: String, body: String, icon: &'static str) {
    let notification = glide_sdk::system::Notification::new("glide-phone", summary).body(body).icon(icon);
    tokio::spawn(async move {
        let _ = glide_sdk::system::notify(notification).await;
    });
}

async fn copy_text(text: &str) -> bool {
    use tokio::io::AsyncWriteExt as _;
    let spawned = tokio::process::Command::new("wl-copy")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    let Ok(mut child) = spawned else {
        return false;
    };
    let written = match child.stdin.take() {
        Some(mut stdin) => stdin.write_all(text.as_bytes()).await.is_ok(),
        None => false,
    };
    child.wait().await.is_ok_and(|status| status.success()) && written
}

pub fn deliver(device: &str, share: SharePayload) {
    if tokio::runtime::Handle::try_current().is_err() {
        return;
    }
    let device = device.to_string();
    match share {
        SharePayload::File { .. } => {}
        SharePayload::Url(url) => {
            let Some(url) = openable(&url).map(str::to_string) else {
                tracing::warn!("ignoring a shared link that is not a plain web address");
                return;
            };
            tokio::spawn(async move {
                let opened = tokio::process::Command::new("xdg-open").arg(&url).status().await;
                if !opened.is_ok_and(|status| status.success()) {
                    announce(format!("Link from {device}"), url, "emblem-shared-symbolic");
                }
            });
        }
        SharePayload::Text(text) => {
            if text.trim().is_empty() || text.len() > MAX_SHARED_TEXT_BYTES {
                return;
            }
            tokio::spawn(async move {
                if copy_text(&text).await {
                    announce(format!("Text from {device} copied"), text, "edit-paste-symbolic");
                } else {
                    announce(format!("Text from {device}"), text, "emblem-shared-symbolic");
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plain_web_addresses_are_opened() {
        assert_eq!(openable(" https://example.org/a?b=c "), Some("https://example.org/a?b=c"));
        assert_eq!(openable("HTTP://example.org"), Some("HTTP://example.org"));
        for refused in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "steam://run/1",
            "example.org",
            "https://example.org/ --new-window",
            "https://example.org/\nfile:///x",
            "",
        ] {
            assert_eq!(openable(refused), None, "{refused:?}");
        }
        assert_eq!(openable(&format!("https://e.org/{}", "a".repeat(MAX_SHARED_URL_BYTES))), None);
    }

    #[test]
    fn filenames_cannot_escape_the_download_directory() {
        let packet = NetworkPacket::new(PACKET_TYPE_SHARE_REQUEST, json!({"filename": "../../photo.jpg"}));
        assert_eq!(handle_share_packet(&packet).unwrap(), SharePayload::File { filename: "photo.jpg".into() });
    }
}

use serde::{Deserialize, Serialize};

use super::packet::{NetworkPacket, PACKET_TYPE_IDENTITY};

pub const PROTOCOL_VERSION: i32 = 8;
pub const UDP_PORT: u16 = 1716;
pub const MIN_TCP_PORT: u16 = 1716;
pub const MAX_TCP_PORT: u16 = 1764;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeviceType {
    Desktop,
    Laptop,
    #[serde(alias = "smartphone")]
    Phone,
    Tablet,
    Tv,
    #[serde(other)]
    Unknown,
}

impl DeviceType {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Desktop => "desktop",
            Self::Laptop => "laptop",
            Self::Phone => "phone",
            Self::Tablet => "tablet",
            Self::Tv => "tv",
            Self::Unknown => "unknown",
        }
    }
}

const fn default_device_type() -> DeviceType {
    DeviceType::Phone
}

fn default_device_name() -> String {
    "Device".to_string()
}

fn optional_protocol_version<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Option<i32>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Version {
        Number(i32),
        Text(String),
    }
    match Option::<Version>::deserialize(deserializer)? {
        Some(Version::Number(version)) => Ok(Some(version)),
        Some(Version::Text(version)) => version.parse().map(Some).map_err(serde::de::Error::custom),
        None => Ok(None),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentityBody {
    #[serde(rename = "deviceId")]
    pub device_id: String,
    #[serde(rename = "deviceName", default = "default_device_name")]
    pub device_name: String,
    #[serde(rename = "deviceType", default = "default_device_type")]
    pub device_type: DeviceType,
    #[serde(rename = "protocolVersion")]
    pub protocol_version: i32,
    #[serde(rename = "tcpPort", skip_serializing_if = "Option::is_none")]
    pub tcp_port: Option<u16>,
    #[serde(rename = "targetDeviceId", skip_serializing_if = "Option::is_none", default)]
    pub target_device_id: Option<String>,
    #[serde(
        rename = "targetProtocolVersion",
        skip_serializing_if = "Option::is_none",
        default,
        deserialize_with = "optional_protocol_version"
    )]
    pub target_protocol_version: Option<i32>,
    #[serde(rename = "incomingCapabilities", default)]
    pub incoming_capabilities: Vec<String>,
    #[serde(rename = "outgoingCapabilities", default)]
    pub outgoing_capabilities: Vec<String>,
}

impl IdentityBody {
    pub fn to_packet(&self) -> Result<NetworkPacket, serde_json::Error> {
        let value = serde_json::to_value(self)?;
        Ok(NetworkPacket::new(PACKET_TYPE_IDENTITY, value))
    }

    pub fn from_packet(packet: &NetworkPacket) -> Result<Self, serde_json::Error> {
        serde_json::from_value(packet.body.clone())
    }

    pub fn new_connection_packet(
        device_id: &str, device_name: &str, target_device_id: &str,
    ) -> Result<NetworkPacket, serde_json::Error> {
        Ok(NetworkPacket::new(
            PACKET_TYPE_IDENTITY,
            serde_json::json!({
                "deviceId": device_id,
                "deviceName": device_name,
                "protocolVersion": PROTOCOL_VERSION,
                "targetDeviceId": target_device_id,
                "targetProtocolVersion": PROTOCOL_VERSION,
            }),
        ))
    }

    pub fn new_discovery_packet(
        device_id: &str, device_name: &str, tcp_port: u16,
    ) -> Result<NetworkPacket, serde_json::Error> {
        Ok(NetworkPacket::new(
            PACKET_TYPE_IDENTITY,
            serde_json::json!({
                "deviceId": device_id,
                "deviceName": device_name,
                "protocolVersion": PROTOCOL_VERSION,
                "tcpPort": tcp_port,
            }),
        ))
    }
}

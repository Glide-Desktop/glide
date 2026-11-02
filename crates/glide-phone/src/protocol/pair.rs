use serde::{Deserialize, Serialize};

use super::packet::{NetworkPacket, PACKET_TYPE_PAIR};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairBody {
    pub pair: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<i64>,
}

impl PairBody {
    #[must_use]
    pub const fn new(pair: bool) -> Self {
        Self { pair, timestamp: None }
    }

    #[must_use]
    pub fn request() -> Self {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .and_then(|d| i64::try_from(d.as_secs()).ok());
        Self { pair: true, timestamp }
    }

    #[must_use]
    pub const fn accept() -> Self {
        Self { pair: true, timestamp: None }
    }

    #[must_use]
    pub const fn reject() -> Self {
        Self { pair: false, timestamp: None }
    }

    pub fn to_packet(&self) -> Result<NetworkPacket, serde_json::Error> {
        let value = serde_json::to_value(self)?;
        Ok(NetworkPacket::new(PACKET_TYPE_PAIR, value))
    }

    pub fn from_packet(packet: &NetworkPacket) -> Result<Self, serde_json::Error> {
        serde_json::from_value(packet.body.clone())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PairState {
    #[default]
    Unpaired,
    RequestedByPeer,
    RequestedByLocal,
    Paired,
}

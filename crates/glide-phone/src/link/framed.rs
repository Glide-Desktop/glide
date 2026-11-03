use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::codec::{Framed, LinesCodec};

use crate::protocol::packet::NetworkPacket;

pub const MAX_PACKET_BYTES: usize = 4 * 1024 * 1024;

pub struct PacketStream<S> {
    framed: Framed<S, LinesCodec>,
}

impl<S> PacketStream<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    #[must_use]
    pub fn new(stream: S) -> Self {
        Self { framed: Framed::new(stream, LinesCodec::new_with_max_length(MAX_PACKET_BYTES)) }
    }

    pub async fn send_packet(
        &mut self, packet: &NetworkPacket,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let serialized = packet.serialize()?;
        let trimmed = serialized.trim_end_matches('\n');
        self.framed.send(trimmed.to_string()).await?;
        Ok(())
    }

    pub async fn next_packet(&mut self) -> Result<Option<NetworkPacket>, Box<dyn std::error::Error + Send + Sync>> {
        match self.framed.next().await {
            Some(Ok(line)) => {
                let packet = NetworkPacket::parse(&line)?;
                Ok(Some(packet))
            }
            Some(Err(err)) => Err(Box::new(err)),
            None => Ok(None),
        }
    }
}

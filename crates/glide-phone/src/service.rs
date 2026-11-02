use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::Mutex;
use tokio_rustls::{TlsAcceptor, TlsConnector};

use crate::config::PhoneConfig;
use crate::crypto::cert::{IdentityCert, compute_fingerprint};
use crate::crypto::tls::{make_client_config, make_server_config};
use crate::discovery::udp::{DiscoveredPeer, broadcast_identity, receive_discovery_packet, send_direct_discovery};
use crate::link::framed::PacketStream;
use crate::plugins::battery::local_battery_packet;
use crate::plugins::notifications::create_request_packet;
use crate::plugins::share::{SharePayload, handle_share_packet};
use crate::protocol::identity::{IdentityBody, MAX_TCP_PORT, MIN_TCP_PORT, PROTOCOL_VERSION};
use crate::protocol::packet::{
    NetworkPacket, PACKET_TYPE_IDENTITY, PACKET_TYPE_NOTIFICATION, PACKET_TYPE_SHARE_REQUEST,
};
use crate::protocol::pair::PairState;
use crate::state::PhoneManager;

const MAX_PRE_TLS_IDENTITY_BYTES: usize = 8192;
const MAX_NOTIFICATION_PAYLOAD_BYTES: usize = 512 * 1024;
const MAX_SHARE_PAYLOAD_BYTES: usize = 1024 * 1024 * 1024;
const HANDSHAKE_TIMEOUT: tokio::time::Duration = tokio::time::Duration::from_secs(10);
const PAYLOAD_TIMEOUT: tokio::time::Duration = tokio::time::Duration::from_secs(3);

async fn read_plaintext_line(stream: &mut TcpStream) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let mut buf = Vec::with_capacity(512);
    let mut byte = [0u8; 1];
    loop {
        let n = stream.read(&mut byte).await?;
        if n == 0 {
            return Err("EOF before newline in pre-TLS identity".into());
        }
        buf.push(byte[0]);
        if byte[0] == b'\n' {
            break;
        }
        if buf.len() >= MAX_PRE_TLS_IDENTITY_BYTES {
            return Err("pre-TLS identity packet exceeded maximum length".into());
        }
    }
    Ok(String::from_utf8(buf)?)
}

pub struct PhoneService {
    manager: Arc<Mutex<PhoneManager>>,
    tcp_port: u16,
}

impl PhoneService {
    pub async fn bind(
        config: PhoneConfig, cert: IdentityCert,
    ) -> Result<(Self, TcpListener), Box<dyn std::error::Error + Send + Sync>> {
        let mut port = MIN_TCP_PORT;
        let listener = loop {
            match TcpListener::bind((Ipv4Addr::UNSPECIFIED, port)).await {
                Ok(listener) => break listener,
                Err(_) if port < MAX_TCP_PORT => {
                    port += 1;
                }
                Err(err) => return Err(Box::new(err)),
            }
        };

        let manager = Arc::new(Mutex::new(PhoneManager::new(config, cert)));
        tracing::info!(port, "phone TCP listener bound");
        Ok((Self { manager, tcp_port: port }, listener))
    }

    #[must_use]
    pub fn manager(&self) -> Arc<Mutex<PhoneManager>> {
        self.manager.clone()
    }

    #[must_use]
    pub const fn tcp_port(&self) -> u16 {
        self.tcp_port
    }

    #[must_use]
    pub fn local_identity(&self, device_id: String, device_name: String) -> IdentityBody {
        IdentityBody {
            device_id,
            device_name,
            device_type: crate::protocol::identity::DeviceType::Desktop,
            protocol_version: PROTOCOL_VERSION,
            tcp_port: Some(self.tcp_port),
            target_device_id: None,
            target_protocol_version: None,
            incoming_capabilities: vec![
                "kdeconnect.battery".to_string(),
                "kdeconnect.clipboard".to_string(),
                "kdeconnect.notification".to_string(),
                "kdeconnect.presenter".to_string(),
                "kdeconnect.mousepad.request".to_string(),
                "kdeconnect.share.request".to_string(),
                "kdeconnect.findmyphone.request".to_string(),
                "kdeconnect.ping".to_string(),
            ],
            outgoing_capabilities: vec![
                "kdeconnect.battery".to_string(),
                "kdeconnect.clipboard".to_string(),
                "kdeconnect.notification.reply".to_string(),
                "kdeconnect.notification.action".to_string(),
                "kdeconnect.notification.request".to_string(),
                "kdeconnect.findmyphone.request".to_string(),
                "kdeconnect.ping".to_string(),
                "kdeconnect.share.request".to_string(),
            ],
        }
    }

    pub async fn run_discovery(
        &self, udp_socket: Arc<UdpSocket>, custom_ips: Vec<IpAddr>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let (device_id, device_name) = {
            let mgr = self.manager.lock().await;
            (mgr.config.device_id.clone(), mgr.config.device_name.clone())
        };
        let identity = self.local_identity(device_id.clone(), device_name);

        let socket_send = udp_socket.clone();
        let id_clone = identity.clone();
        let ips = custom_ips;
        let mgr_broadcast = self.manager.clone();
        tokio::spawn(async move {
            if let Err(err) = broadcast_identity(&socket_send, &id_clone, &ips).await {
                tracing::warn!(%err, "phone discovery broadcast failed");
            }
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(15)).await;
                let has_connected = {
                    let mgr = mgr_broadcast.lock().await;
                    mgr.has_any_connection()
                };
                if !has_connected && let Err(err) = broadcast_identity(&socket_send, &id_clone, &ips).await {
                    tracing::warn!(%err, "phone discovery broadcast failed");
                }
            }
        });

        loop {
            if let Some(DiscoveredPeer { identity: peer_id, addr }) =
                receive_discovery_packet(&udp_socket, &device_id).await?
            {
                let mut mgr = self.manager.lock().await;
                mgr.register_discovered(peer_id.device_id.clone(), peer_id.device_name.clone(), peer_id.device_type);

                if mgr.is_connected(&peer_id.device_id) || mgr.is_connecting(&peer_id.device_id) {
                    continue;
                }

                if let Some(port) = peer_id.tcp_port {
                    mgr.mark_connecting(&peer_id.device_id);
                    drop(mgr);
                    let self_clone = self.clone_service();
                    let peer_id_clone = peer_id.device_id.clone();
                    tokio::spawn(async move {
                        if let Err(err) = self_clone.connect_to_peer(addr, port, peer_id_clone.clone()).await {
                            tracing::warn!(%addr, port, %err, "phone outgoing connection failed");
                            let mut mgr = self_clone.manager.lock().await;
                            mgr.clear_connecting(&peer_id_clone);
                        }
                    });
                } else {
                    drop(mgr);
                    if let Err(err) = send_direct_discovery(&udp_socket, &identity, addr.ip()).await {
                        tracing::warn!(%addr, %err, "phone discovery reply failed");
                    }
                }
            }
        }
    }

    fn clone_service(&self) -> Self {
        Self { manager: self.manager.clone(), tcp_port: self.tcp_port }
    }

    pub async fn connect_to_peer(
        &self, peer_addr: SocketAddr, tcp_port: u16, target_device_id: String,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let connect_addr = SocketAddr::new(peer_addr.ip(), tcp_port);
        let mut stream = tokio::time::timeout(HANDSHAKE_TIMEOUT, TcpStream::connect(connect_addr)).await??;

        let (cert, allowed_fingerprint, conn_pkt) = {
            let mgr = self.manager.lock().await;
            let fp = mgr
                .config
                .paired_devices
                .get(&target_device_id)
                .map(|p| p.fingerprint.clone())
                .filter(|s| !s.is_empty());
            let pkt =
                IdentityBody::new_connection_packet(&mgr.config.device_id, &mgr.config.device_name, &target_device_id)?;
            (mgr.cert.clone(), fp, pkt)
        };

        let raw_identity = conn_pkt.serialize()?;
        stream.write_all(raw_identity.as_bytes()).await?;
        stream.flush().await?;

        let server_config = make_server_config(&cert, allowed_fingerprint)?;
        let acceptor = TlsAcceptor::from(Arc::new(server_config));
        let tls_stream = tokio::time::timeout(HANDSHAKE_TIMEOUT, acceptor.accept(stream)).await??;

        let peer_fp = tls_stream
            .get_ref()
            .1
            .peer_certificates()
            .and_then(|certs| certs.first())
            .map(|c| compute_fingerprint(c.as_ref()));

        self.handle_connection_stream(tls_stream, connect_addr, peer_fp, &target_device_id, PROTOCOL_VERSION).await
    }

    pub async fn handle_incoming_connection(
        &self, mut stream: TcpStream, peer_addr: SocketAddr,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let line = tokio::time::timeout(HANDSHAKE_TIMEOUT, read_plaintext_line(&mut stream)).await??;
        let pre_tls_pkt = NetworkPacket::parse(&line)?;
        if pre_tls_pkt.packet_type != PACKET_TYPE_IDENTITY {
            return Err("expected identity packet before TLS".into());
        }
        let pre_tls_identity = IdentityBody::from_packet(&pre_tls_pkt)?;
        if pre_tls_identity.target_protocol_version.is_some_and(|version| version != PROTOCOL_VERSION) {
            return Err("target protocol version mismatch".into());
        }

        let (cert, allowed_fingerprint) = {
            let mgr = self.manager.lock().await;
            if let Some(target) = &pre_tls_identity.target_device_id
                && target != &mgr.config.device_id
            {
                return Err("target device ID mismatch".into());
            }
            let fp = mgr
                .config
                .paired_devices
                .get(&pre_tls_identity.device_id)
                .map(|p| p.fingerprint.clone())
                .filter(|s| !s.is_empty());
            (mgr.cert.clone(), fp)
        };

        let client_config = make_client_config(&cert, allowed_fingerprint)?;
        let connector = TlsConnector::from(Arc::new(client_config));
        let server_name = rustls::pki_types::ServerName::try_from("kdeconnect")?;
        let tls_stream = tokio::time::timeout(HANDSHAKE_TIMEOUT, connector.connect(server_name, stream)).await??;

        let peer_fp = tls_stream
            .get_ref()
            .1
            .peer_certificates()
            .and_then(|certs| certs.first())
            .map(|c| compute_fingerprint(c.as_ref()));

        self.handle_connection_stream(
            tls_stream,
            peer_addr,
            peer_fp,
            &pre_tls_identity.device_id,
            pre_tls_identity.protocol_version,
        )
        .await
    }

    async fn handle_connection_stream<S>(
        &self, stream: S, peer_addr: SocketAddr, peer_fp: Option<String>, expected_id: &str, expected_version: i32,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
    {
        let mut packet_stream = PacketStream::new(stream);

        let (device_id, device_name) = {
            let mgr = self.manager.lock().await;
            (mgr.config.device_id.clone(), mgr.config.device_name.clone())
        };
        let my_identity = self.local_identity(device_id, device_name);
        packet_stream.send_packet(&my_identity.to_packet()?).await?;

        let Some(initial_packet) = tokio::time::timeout(HANDSHAKE_TIMEOUT, packet_stream.next_packet()).await?? else {
            return Err("EOF before secure identity".into());
        };
        if initial_packet.packet_type != PACKET_TYPE_IDENTITY {
            return Err("expected secure identity packet".into());
        }
        let peer_identity = IdentityBody::from_packet(&initial_packet)?;
        if peer_identity.device_id != expected_id || peer_identity.protocol_version != expected_version {
            return Err("secure identity does not match discovery identity".into());
        }
        if peer_fp.is_none() {
            return Err("peer did not provide a certificate".into());
        }

        let peer_id = peer_identity.device_id.clone();
        let peer_name = peer_identity.device_name.clone();

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let own_sender = tx.downgrade();

        let is_paired = {
            let mut mgr = self.manager.lock().await;
            mgr.register_discovered(peer_id.clone(), peer_name.clone(), peer_identity.device_type);
            mgr.register_connection(peer_id.clone(), tx, peer_fp);
            mgr.send_pending_pair_request(&peer_id);
            mgr.devices.get(&peer_id).is_some_and(|d| d.pair_state == PairState::Paired)
        };
        tracing::info!(%peer_addr, %peer_id, "phone session established");

        if let Some(battery_pkt) = local_battery_packet() {
            let _ = packet_stream.send_packet(&battery_pkt).await;
        }
        if is_paired {
            let _ = packet_stream.send_packet(&create_request_packet()).await;
        }

        loop {
            tokio::select! {
                out = rx.recv() => {
                    match out {
                        Some(packet) => {
                            if let Err(err) = packet_stream.send_packet(&packet).await {
                                tracing::warn!(%peer_addr, %err, "phone packet send failed");
                                break;
                            }
                        }
                        None => break,
                    }
                }
                pkt = packet_stream.next_packet() => {
                    match pkt {
                        Ok(Some(packet)) => {
                            let payload = self.download_payload(peer_addr, &peer_id, &packet).await;
                            if packet.packet_type == PACKET_TYPE_SHARE_REQUEST
                                && let (Some(payload), Ok(SharePayload::File { filename })) = (payload.as_ref(), handle_share_packet(&packet))
                            {
                                let device = peer_name.clone();
                                let bytes = payload.clone();
                                tokio::task::spawn_blocking(move || save_shared_file(&filename, &bytes, &device)).await.ok();
                            }
                            let response = {
                                let mut mgr = self.manager.lock().await;
                                mgr.handle_packet_with_payload(&peer_id, &packet, payload.as_deref())
                            };
                            let response = match response {
                                Ok(response) => response,
                                Err(err) => {
                                    tracing::warn!(%peer_addr, %err, "phone packet handling failed");
                                    break;
                                }
                            };
                            if let Some(resp) = response {
                                let err = packet_stream.send_packet(&resp).await.is_err();
                                if err {
                                    break;
                                }
                            }
                        }
                        Ok(None) => break,
                        Err(err) => {
                            tracing::warn!(%peer_addr, %err, "phone packet receive failed");
                            break;
                        }
                    }
                }
            }
        }

        {
            let mut mgr = self.manager.lock().await;
            mgr.unregister_connection(&peer_id, &own_sender);
        }

        tracing::info!(%peer_addr, %peer_id, "phone session ended");
        Ok(())
    }

    async fn download_payload(&self, peer_addr: SocketAddr, peer_id: &str, packet: &NetworkPacket) -> Option<Vec<u8>> {
        if packet.packet_type != PACKET_TYPE_NOTIFICATION && packet.packet_type != PACKET_TYPE_SHARE_REQUEST {
            return None;
        }
        if packet.body.get("isCancel").and_then(serde_json::Value::as_bool).unwrap_or(false) {
            return None;
        }
        let limit = if packet.packet_type == PACKET_TYPE_SHARE_REQUEST {
            MAX_SHARE_PAYLOAD_BYTES
        } else {
            MAX_NOTIFICATION_PAYLOAD_BYTES
        };
        let size =
            packet.payload_size.and_then(|raw| usize::try_from(raw).ok()).filter(|&raw| raw > 0).unwrap_or(limit);
        if size > limit {
            return None;
        }
        let port = u16::try_from(packet.payload_transfer_info.as_ref()?.get("port")?.as_u64()?).ok()?;
        let (cert, allowed_fp) = {
            let mgr = self.manager.lock().await;
            let fp = mgr.config.paired_devices.get(peer_id).map(|p| p.fingerprint.clone()).filter(|s| !s.is_empty());
            (mgr.cert.clone(), fp)
        };
        let client_config = make_client_config(&cert, allowed_fp).ok()?;
        let connector = TlsConnector::from(Arc::new(client_config));
        let server_name = rustls::pki_types::ServerName::try_from("kdeconnect").ok()?;
        let target = SocketAddr::new(peer_addr.ip(), port);
        tokio::time::timeout(PAYLOAD_TIMEOUT, async move {
            let stream = TcpStream::connect(target).await.ok()?;
            let mut tls = connector.connect(server_name, stream).await.ok()?;
            let mut buf = Vec::with_capacity(size.min(64 * 1024));
            let mut chunk = [0u8; 8192];
            while buf.len() < size {
                let limit = (size - buf.len()).min(chunk.len());
                match tls.read(&mut chunk[..limit]).await {
                    Ok(0) => break,
                    Ok(n) => buf.extend_from_slice(&chunk[..n]),
                    Err(err) if err.kind() == std::io::ErrorKind::UnexpectedEof && !buf.is_empty() => break,
                    Err(_) => return None,
                }
            }
            (!buf.is_empty()).then_some(buf)
        })
        .await
        .ok()
        .flatten()
    }
}

fn save_shared_file(filename: &str, bytes: &[u8], device: &str) {
    let Some(home) = glide_config::home() else {
        return;
    };
    let dir = home.join("Downloads");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let mut target = dir.join(filename);
    let stem = std::path::Path::new(&filename).file_stem().and_then(|value| value.to_str()).unwrap_or("file");
    let extension = std::path::Path::new(&filename).extension().and_then(|value| value.to_str()).unwrap_or("");
    for index in 1..1000 {
        if !target.exists() {
            break;
        }
        let name =
            if extension.is_empty() { format!("{stem} ({index})") } else { format!("{stem} ({index}).{extension}") };
        target = dir.join(name);
    }
    if std::fs::write(&target, bytes).is_ok() {
        let notification = glide_sdk::system::Notification::new("glide-phone", format!("File received from {device}"))
            .body(target.display().to_string())
            .icon("folder-download");
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let _ = glide_sdk::system::notify(notification).await;
            });
        }
    }
}

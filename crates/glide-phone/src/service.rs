use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
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
    NetworkPacket, PACKET_TYPE_IDENTITY, PACKET_TYPE_NOTIFICATION, PACKET_TYPE_PAIR, PACKET_TYPE_SHARE_REQUEST,
};
use crate::protocol::pair::PairState;
use crate::state::PhoneManager;

const MAX_PRE_TLS_IDENTITY_BYTES: usize = 8192;
const MAX_NOTIFICATION_PAYLOAD_BYTES: u64 = 512 * 1024;
const MAX_SHARE_PAYLOAD_BYTES: u64 = 1024 * 1024 * 1024;
const HANDSHAKE_TIMEOUT: tokio::time::Duration = tokio::time::Duration::from_secs(10);
const PAYLOAD_CONNECT_TIMEOUT: tokio::time::Duration = tokio::time::Duration::from_secs(3);
const PAYLOAD_IDLE_TIMEOUT: tokio::time::Duration = tokio::time::Duration::from_secs(10);
const MAX_CONCURRENT_TRANSFERS: usize = 4;

static TRANSFERS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(MAX_CONCURRENT_TRANSFERS);

fn announces_usable_port(port: Option<u16>) -> bool {
    port.is_none_or(|port| (MIN_TCP_PORT..=MAX_TCP_PORT).contains(&port))
}

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
                if !announces_usable_port(peer_id.tcp_port) {
                    tracing::debug!(%addr, port = ?peer_id.tcp_port, "ignoring a discovery packet with a foreign port");
                    continue;
                }
                let mut mgr = self.manager.lock().await;
                let known =
                    mgr.register_discovered(peer_id.device_id.clone(), &peer_id.device_name, peer_id.device_type);

                if !known || mgr.is_connected(&peer_id.device_id) || mgr.is_connecting(&peer_id.device_id) {
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

    async fn admit(
        &self, identity: &IdentityBody, sender: crate::state::PacketSender, fingerprint: Option<String>,
    ) -> Option<(bool, String)> {
        let mut mgr = self.manager.lock().await;
        if !mgr.register_discovered(identity.device_id.clone(), &identity.device_name, identity.device_type) {
            return None;
        }
        mgr.register_connection(identity.device_id.clone(), sender, fingerprint);
        mgr.send_pending_pair_request(&identity.device_id);
        let device = mgr.devices.get(&identity.device_id)?;
        Some((device.pair_state == PairState::Paired, device.name.clone()))
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

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let own_sender = tx.downgrade();

        let Some((is_paired, peer_name)) = self.admit(&peer_identity, tx, peer_fp.clone()).await else {
            return Err("the device is not admitted: too many unpaired devices, or an unusable id".into());
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
                            let payload = if self.is_paired(&peer_id).await {
                                self.receive_payload(peer_addr, &peer_id, &peer_name, &packet).await
                            } else {
                                None
                            };
                            let response = {
                                let mut mgr = self.manager.lock().await;
                                if packet.packet_type == PACKET_TYPE_PAIR && !mgr.presents(&peer_id, peer_fp.as_deref()) {
                                    tracing::warn!(%peer_addr, %peer_id, "ignoring a pairing packet from a replaced session");
                                    continue;
                                }
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

    async fn is_paired(&self, peer_id: &str) -> bool {
        self.manager.lock().await.devices.get(peer_id).is_some_and(|device| device.pair_state == PairState::Paired)
    }

    async fn receive_payload(
        &self, peer_addr: SocketAddr, peer_id: &str, device: &str, packet: &NetworkPacket,
    ) -> Option<Vec<u8>> {
        if packet.body.get("isCancel").and_then(serde_json::Value::as_bool).unwrap_or(false) {
            return None;
        }
        match packet.packet_type.as_str() {
            PACKET_TYPE_NOTIFICATION => {
                let (mut stream, size) =
                    self.open_payload(peer_addr, peer_id, packet, MAX_NOTIFICATION_PAYLOAD_BYTES).await?;
                read_payload(&mut stream, size).await
            }
            PACKET_TYPE_SHARE_REQUEST => {
                let Ok(SharePayload::File { filename }) = handle_share_packet(packet) else {
                    return None;
                };
                let service = self.clone_service();
                let (peer_id, device, packet) = (peer_id.to_string(), device.to_string(), packet.clone());
                tokio::spawn(async move {
                    let Ok(_slot) = TRANSFERS.acquire().await else {
                        return;
                    };
                    service.receive_shared_file(peer_addr, &peer_id, &device, &packet, &filename).await;
                });
                None
            }
            _ => None,
        }
    }

    async fn receive_shared_file(
        &self, peer_addr: SocketAddr, peer_id: &str, device: &str, packet: &NetworkPacket, filename: &str,
    ) {
        let Some((mut stream, size)) = self.open_payload(peer_addr, peer_id, packet, MAX_SHARE_PAYLOAD_BYTES).await
        else {
            tracing::warn!(%peer_addr, "a shared file could not be fetched");
            return;
        };
        let exact = packet.payload_size.is_some_and(|raw| raw > 0);
        if let Some(target) = save_shared_file(&mut stream, size, exact, filename).await {
            announce_shared_file(&target, device);
        } else {
            tracing::warn!(%peer_addr, "a shared file did not arrive whole and was not kept");
        }
    }

    async fn open_payload(
        &self, peer_addr: SocketAddr, peer_id: &str, packet: &NetworkPacket, limit: u64,
    ) -> Option<(tokio_rustls::client::TlsStream<TcpStream>, u64)> {
        let size = packet.payload_size.and_then(|raw| u64::try_from(raw).ok()).filter(|&raw| raw > 0).unwrap_or(limit);
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
        let stream = tokio::time::timeout(PAYLOAD_CONNECT_TIMEOUT, async move {
            let stream = TcpStream::connect(target).await.ok()?;
            connector.connect(server_name, stream).await.ok()
        })
        .await
        .ok()
        .flatten()?;
        Some((stream, size))
    }
}

async fn read_chunk(stream: &mut (impl tokio::io::AsyncRead + Unpin), chunk: &mut [u8]) -> Option<usize> {
    match tokio::time::timeout(PAYLOAD_IDLE_TIMEOUT, stream.read(chunk)).await {
        Ok(Ok(count)) => Some(count),
        Ok(Err(err)) if err.kind() == std::io::ErrorKind::UnexpectedEof => Some(0),
        _ => None,
    }
}

async fn read_payload(stream: &mut (impl tokio::io::AsyncRead + Unpin), size: u64) -> Option<Vec<u8>> {
    let size = usize::try_from(size).ok()?;
    let mut buf = Vec::with_capacity(size.min(64 * 1024));
    let mut chunk = [0u8; 8192];
    while buf.len() < size {
        let want = (size - buf.len()).min(chunk.len());
        match read_chunk(stream, &mut chunk[..want]).await? {
            0 => break,
            count => buf.extend_from_slice(&chunk[..count]),
        }
    }
    (!buf.is_empty()).then_some(buf)
}

fn create_unique(dir: &Path, filename: &str) -> Option<(std::fs::File, PathBuf)> {
    std::fs::create_dir_all(dir).ok()?;
    let name = Path::new(filename);
    let stem = name.file_stem().and_then(|value| value.to_str()).unwrap_or("file");
    let extension = name.extension().and_then(|value| value.to_str()).unwrap_or("");
    for index in 0..1000 {
        let candidate = match (index, extension.is_empty()) {
            (0, _) => filename.to_string(),
            (_, true) => format!("{stem} ({index})"),
            (_, false) => format!("{stem} ({index}).{extension}"),
        };
        let target = dir.join(candidate);
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&target) {
            Ok(file) => return Some((file, target)),
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) => return None,
        }
    }
    None
}

async fn save_shared_file(
    stream: &mut (impl tokio::io::AsyncRead + Unpin), size: u64, exact: bool, filename: &str,
) -> Option<PathBuf> {
    let dir = glide_config::home()?.join("Downloads");
    save_stream_into(stream, size, exact, &dir, filename).await
}

async fn save_stream_into(
    stream: &mut (impl tokio::io::AsyncRead + Unpin), size: u64, exact: bool, dir: &Path, filename: &str,
) -> Option<PathBuf> {
    use std::io::Write as _;
    let (mut file, target) = create_unique(dir, filename)?;
    let mut chunk = vec![0u8; 64 * 1024];
    let mut received = 0_u64;
    let whole = loop {
        if received >= size {
            break true;
        }
        let want = usize::try_from(size - received).unwrap_or(usize::MAX).min(chunk.len());
        match read_chunk(stream, &mut chunk[..want]).await {
            Some(0) => break !exact && received > 0,
            Some(count) if file.write_all(&chunk[..count]).is_ok() => received += count as u64,
            _ => break false,
        }
    };
    drop(file);
    if whole {
        Some(target)
    } else {
        let _ = std::fs::remove_file(&target);
        None
    }
}

fn announce_shared_file(target: &Path, device: &str) {
    let summary = crate::i18n::naming(crate::i18n::Key::FileReceived, device);
    crate::plugins::announce(
        glide_sdk::system::Notification::new("glide-phone", summary)
            .body(target.display().to_string())
            .icon("folder-download"),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("glide-phone-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_shared_file_never_replaces_one_that_is_already_there() {
        let dir = scratch("unique");
        let (_, first) = create_unique(&dir, "photo.jpg").expect("first");
        let (_, second) = create_unique(&dir, "photo.jpg").expect("second");
        let (_, bare) = create_unique(&dir, "notes").expect("no extension");
        let (_, bare_again) = create_unique(&dir, "notes").expect("no extension, taken");
        assert_eq!(first, dir.join("photo.jpg"));
        assert_eq!(second, dir.join("photo (1).jpg"));
        assert_eq!((bare, bare_again), (dir.join("notes"), dir.join("notes (1)")));
        std::os::unix::fs::symlink(dir.join("elsewhere"), dir.join("link.txt")).expect("symlink");
        let (_, beside) = create_unique(&dir, "link.txt").expect("a dangling link is not written through");
        assert_eq!(beside, dir.join("link (1).txt"));
        assert!(!dir.join("elsewhere").exists());
        std::fs::remove_dir_all(dir).expect("cleanup");
    }

    #[tokio::test]
    async fn a_transfer_that_stops_short_of_its_announced_size_leaves_no_file() {
        let dir = scratch("short");
        let mut whole: &[u8] = b"0123456789";
        let kept = save_stream_into(&mut whole, 10, true, &dir, "a.bin").await.expect("complete");
        assert_eq!(std::fs::read(&kept).expect("kept"), b"0123456789");
        let mut short: &[u8] = b"01234";
        assert_eq!(save_stream_into(&mut short, 10, true, &dir, "b.bin").await, None);
        assert!(!dir.join("b.bin").exists(), "the partial file is removed");
        let mut unsized_stream: &[u8] = b"01234";
        let open_ended = save_stream_into(&mut unsized_stream, 1024, false, &dir, "c.bin").await.expect("until EOF");
        assert_eq!(std::fs::read(open_ended).expect("kept"), b"01234");
        let mut long: &[u8] = b"0123456789";
        let capped = save_stream_into(&mut long, 4, true, &dir, "d.bin").await.expect("capped");
        assert_eq!(std::fs::read(capped).expect("kept"), b"0123", "never more than the announced size");
        std::fs::remove_dir_all(dir).expect("cleanup");
    }

    #[test]
    fn a_peer_is_only_dialled_on_a_port_of_the_protocol_range() {
        assert!(announces_usable_port(None), "a peer without a port is answered over UDP");
        assert!(announces_usable_port(Some(MIN_TCP_PORT)) && announces_usable_port(Some(MAX_TCP_PORT)));
        for foreign in [22, 80, 1715, 1765, 6379] {
            assert!(!announces_usable_port(Some(foreign)), "{foreign}");
        }
    }
}

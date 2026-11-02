use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tokio::net::UdpSocket;

use crate::protocol::identity::{IdentityBody, UDP_PORT};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredPeer {
    pub addr: SocketAddr,
    pub identity: IdentityBody,
}

pub async fn bind_discovery_socket() -> std::io::Result<UdpSocket> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, UDP_PORT)).await?;
    socket.set_broadcast(true)?;
    Ok(socket)
}

#[must_use]
pub fn interface_broadcast_addresses() -> Vec<Ipv4Addr> {
    let mut out = Vec::new();
    let mut ifaddrs = std::ptr::null_mut();
    // SAFETY: getifaddrs allocates linked list of ifaddrs structs which is freed with freeifaddrs.
    let ret = unsafe { libc::getifaddrs(&raw mut ifaddrs) };
    if ret != 0 || ifaddrs.is_null() {
        return out;
    }
    let mut curr = ifaddrs;
    while !curr.is_null() {
        // SAFETY: curr is a valid pointer in the linked list.
        let ifa = unsafe { &*curr };
        let flags = ifa.ifa_flags as i32;
        let is_up = (flags & libc::IFF_UP) != 0;
        let is_running = (flags & libc::IFF_RUNNING) != 0;
        let is_loopback = (flags & libc::IFF_LOOPBACK) != 0;
        let has_broadcast = (flags & libc::IFF_BROADCAST) != 0;

        if is_up && is_running && !is_loopback && has_broadcast && !ifa.ifa_ifu.is_null() && !ifa.ifa_addr.is_null() {
            // SAFETY: ifa_addr is non-null and checked for AF_INET.
            let family = unsafe { (*ifa.ifa_addr).sa_family as i32 };
            if family == libc::AF_INET {
                let broad_sock = ifa.ifa_ifu.cast::<libc::sockaddr_in>();
                // SAFETY: broad_sock is a valid sockaddr_in pointer.
                let sin_addr = unsafe { (*broad_sock).sin_addr.s_addr };
                let ip = Ipv4Addr::from(u32::from_be(sin_addr));
                if !ip.is_unspecified() && !ip.is_loopback() && !out.contains(&ip) {
                    out.push(ip);
                }
            }
        }
        curr = ifa.ifa_next;
    }
    // SAFETY: ifaddrs was allocated by getifaddrs.
    unsafe { libc::freeifaddrs(ifaddrs) };
    out
}

pub async fn broadcast_identity(
    socket: &UdpSocket, identity: &IdentityBody, custom_ips: &[IpAddr],
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let packet = if let Some(port) = identity.tcp_port {
        IdentityBody::new_discovery_packet(&identity.device_id, &identity.device_name, port)?
    } else {
        identity.to_packet()?
    };
    let payload = packet.serialize()?;
    let bytes = payload.as_bytes();

    let iface_addrs = interface_broadcast_addresses();
    if iface_addrs.is_empty() {
        let broadcast_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::BROADCAST), UDP_PORT);
        if let Err(err) = socket.send_to(bytes, broadcast_addr).await {
            tracing::warn!(%broadcast_addr, %err, "phone broadcast send failed");
        }
    } else {
        for ip in iface_addrs {
            let dest = SocketAddr::new(IpAddr::V4(ip), UDP_PORT);
            if let Err(err) = socket.send_to(bytes, dest).await {
                tracing::debug!(%dest, %err, "phone interface broadcast failed");
            }
        }
    }

    for &ip in custom_ips {
        let dest = SocketAddr::new(ip, UDP_PORT);
        if let Err(err) = socket.send_to(bytes, dest).await {
            tracing::warn!(%dest, %err, "phone custom discovery send failed");
        }
    }

    Ok(())
}

pub async fn send_direct_discovery(
    socket: &UdpSocket, identity: &IdentityBody, target_ip: IpAddr,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let packet = if let Some(port) = identity.tcp_port {
        IdentityBody::new_discovery_packet(&identity.device_id, &identity.device_name, port)?
    } else {
        identity.to_packet()?
    };
    let payload = packet.serialize()?;
    let dest = SocketAddr::new(target_ip, UDP_PORT);
    socket.send_to(payload.as_bytes(), dest).await?;
    Ok(())
}

pub async fn receive_discovery_packet(
    socket: &UdpSocket, own_device_id: &str,
) -> Result<Option<DiscoveredPeer>, Box<dyn std::error::Error + Send + Sync>> {
    let mut buf = [0u8; 8192];
    let (len, sender) = socket.recv_from(&mut buf).await?;
    let Ok(text) = std::str::from_utf8(&buf[..len]) else {
        return Ok(None);
    };

    let Ok(packet) = crate::protocol::packet::NetworkPacket::parse(text) else {
        return Ok(None);
    };

    if packet.packet_type != crate::protocol::packet::PACKET_TYPE_IDENTITY {
        return Ok(None);
    }

    let Ok(identity) = IdentityBody::from_packet(&packet) else {
        return Ok(None);
    };

    if identity.device_id == own_device_id {
        return Ok(None);
    }

    Ok(Some(DiscoveredPeer { addr: sender, identity }))
}

use std::collections::BTreeMap;
use std::net::IpAddr;
use std::time::{Duration, SystemTime};

use crate::pb;
use crate::time::{duration_from, system_time};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum State {
    #[default]
    Idle,
    Connecting,
    Connected,
    NeedsLogin,
    LoginFailed,
    SessionExpired,
    Unavailable,
}

impl State {
    #[must_use]
    pub fn parse(text: &str) -> Self {
        match text.to_ascii_lowercase().as_str() {
            "connected" => Self::Connected,
            "connecting" => Self::Connecting,
            "needslogin" => Self::NeedsLogin,
            "loginfailed" => Self::LoginFailed,
            "sessionexpired" => Self::SessionExpired,
            "daemonunavailable" => Self::Unavailable,
            _ => Self::Idle,
        }
    }

    #[must_use]
    pub const fn needs_login(self) -> bool {
        matches!(self, Self::NeedsLogin | Self::LoginFailed | Self::SessionExpired)
    }

    #[must_use]
    pub const fn is_connected(self) -> bool {
        matches!(self, Self::Connected)
    }

    #[must_use]
    pub const fn is_up(self) -> bool {
        matches!(self, Self::Connected | Self::Connecting)
    }

    #[must_use]
    pub const fn needs_down_before_switch(self) -> bool {
        matches!(self, Self::Connected | Self::Connecting | Self::NeedsLogin | Self::LoginFailed | Self::SessionExpired)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Link {
    pub url: String,
    pub connected: bool,
    pub error: String,
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct LocalPeer {
    pub ip: String,
    pub ipv6: String,
    pub pub_key: String,
    pub fqdn: String,
    pub kernel_interface: bool,
    pub rosenpass: bool,
    pub rosenpass_permissive: bool,
    pub networks: Vec<String>,
    pub wireguard_port: i32,
}

impl std::fmt::Debug for LocalPeer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalPeer").field("networks", &self.networks.len()).finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PeerStatus {
    Connected,
    Connecting,
    #[default]
    Disconnected,
}

impl PeerStatus {
    fn parse(text: &str) -> Self {
        match text.to_ascii_lowercase().as_str() {
            "connected" => Self::Connected,
            "connecting" => Self::Connecting,
            _ => Self::Disconnected,
        }
    }
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct Ice {
    pub kind: String,
    pub endpoint: String,
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct Peer {
    pub ip: String,
    pub ipv6: String,
    pub pub_key: String,
    pub fqdn: String,
    pub status: PeerStatus,
    pub status_since: Option<SystemTime>,
    pub relayed: bool,
    pub local_ice: Ice,
    pub remote_ice: Ice,
    pub relay_address: String,
    pub last_handshake: Option<SystemTime>,
    pub bytes_rx: u64,
    pub bytes_tx: u64,
    pub latency: Option<Duration>,
    pub rosenpass: bool,
    pub networks: Vec<String>,
}

impl Peer {
    #[must_use]
    pub fn name(&self) -> &str {
        let short = self.fqdn.split('.').next().unwrap_or_default();
        if short.is_empty() { &self.ip } else { short }
    }
}

impl std::fmt::Debug for Peer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Peer").field("status", &self.status).field("relayed", &self.relayed).finish_non_exhaustive()
    }
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct Relay {
    pub uri: String,
    pub available: bool,
    pub error: String,
    pub transport: String,
}

impl std::fmt::Debug for Relay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Relay").field("available", &self.available).finish_non_exhaustive()
    }
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct DnsGroup {
    pub servers: Vec<String>,
    pub domains: Vec<String>,
    pub enabled: bool,
    pub error: String,
}

impl std::fmt::Debug for DnsGroup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DnsGroup").field("enabled", &self.enabled).finish_non_exhaustive()
    }
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct SshSession {
    pub username: String,
    pub remote_address: String,
    pub command: String,
    pub jwt_username: String,
    pub port_forwards: Vec<String>,
}

impl std::fmt::Debug for SshSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SshSession").finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SshServer {
    pub enabled: bool,
    pub sessions: Vec<SshSession>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Severity {
    #[default]
    Info,
    Warning,
    Error,
    Critical,
}

impl Severity {
    const fn from_proto(value: i32) -> Self {
        match value {
            1 => Self::Warning,
            2 => Self::Error,
            3 => Self::Critical,
            _ => Self::Info,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Category {
    #[default]
    Network,
    Dns,
    Authentication,
    Connectivity,
    System,
}

impl Category {
    const fn from_proto(value: i32) -> Self {
        match value {
            1 => Self::Dns,
            2 => Self::Authentication,
            3 => Self::Connectivity,
            4 => Self::System,
            _ => Self::Network,
        }
    }
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct Event {
    pub id: String,
    pub severity: Severity,
    pub category: Category,
    pub message: String,
    pub user_message: String,
    pub timestamp: Option<SystemTime>,
    pub metadata: BTreeMap<String, String>,
}

impl Event {
    #[must_use]
    pub fn meta(&self, key: &str) -> Option<&str> {
        self.metadata.get(key).map(String::as_str)
    }
}

impl std::fmt::Debug for Event {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Event")
            .field("severity", &self.severity)
            .field("category", &self.category)
            .field("keys", &self.metadata.keys().collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}

impl From<pb::SystemEvent> for Event {
    fn from(event: pb::SystemEvent) -> Self {
        Self {
            severity: Severity::from_proto(event.severity),
            category: Category::from_proto(event.category),
            timestamp: event.timestamp.as_ref().and_then(system_time),
            metadata: event.metadata.into_iter().collect(),
            id: event.id,
            message: event.message,
            user_message: event.user_message,
        }
    }
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct Status {
    pub state: State,
    pub daemon_version: String,
    pub management: Link,
    pub signal: Link,
    pub local: LocalPeer,
    pub peers: Vec<Peer>,
    pub relays: Vec<Relay>,
    pub dns: Vec<DnsGroup>,
    pub events: Vec<Event>,
    pub forwarding_rules: i32,
    pub lazy_connection: bool,
    pub ssh: SshServer,
    pub networks_revision: u64,
    pub session_expires_at: Option<SystemTime>,
}

impl Status {
    #[must_use]
    pub fn unavailable() -> Self {
        Self { state: State::Unavailable, ..Self::default() }
    }

    #[must_use]
    pub fn connected_peers(&self) -> usize {
        self.peers.iter().filter(|peer| peer.status == PeerStatus::Connected).count()
    }
}

impl std::fmt::Debug for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Status")
            .field("state", &self.state)
            .field("daemon_version", &self.daemon_version)
            .field("peers", &self.peers.len())
            .field("connected", &self.connected_peers())
            .field("networks_revision", &self.networks_revision)
            .finish_non_exhaustive()
    }
}

fn link(url: String, connected: bool, error: String) -> Link {
    Link { url, connected, error }
}

impl From<pb::StatusResponse> for Status {
    fn from(response: pb::StatusResponse) -> Self {
        let full = response.full_status.unwrap_or_default();
        let management = full.management_state.unwrap_or_default();
        let signal = full.signal_state.unwrap_or_default();
        let local = full.local_peer_state.unwrap_or_default();
        let ssh = full.ssh_server_state.unwrap_or_default();
        Self {
            state: State::parse(&response.status),
            daemon_version: response.daemon_version,
            management: link(management.url, management.connected, management.error),
            signal: link(signal.url, signal.connected, signal.error),
            local: LocalPeer {
                ip: local.ip,
                ipv6: local.ipv6,
                pub_key: local.pub_key,
                fqdn: local.fqdn,
                kernel_interface: local.kernel_interface,
                rosenpass: local.rosenpass_enabled,
                rosenpass_permissive: local.rosenpass_permissive,
                networks: local.networks,
                wireguard_port: local.wg_port,
            },
            peers: full.peers.into_iter().map(Peer::from).collect(),
            relays: full
                .relays
                .into_iter()
                .map(|relay| Relay {
                    uri: relay.uri,
                    available: relay.available,
                    error: relay.error,
                    transport: relay.transport,
                })
                .collect(),
            dns: full
                .dns_servers
                .into_iter()
                .map(|group| DnsGroup {
                    servers: group.servers,
                    domains: group.domains,
                    enabled: group.enabled,
                    error: group.error,
                })
                .collect(),
            events: full.events.into_iter().map(Event::from).collect(),
            forwarding_rules: full.number_of_forwarding_rules,
            lazy_connection: full.lazy_connection_enabled,
            ssh: SshServer {
                enabled: ssh.enabled,
                sessions: ssh
                    .sessions
                    .into_iter()
                    .map(|session| SshSession {
                        username: session.username,
                        remote_address: session.remote_address,
                        command: session.command,
                        jwt_username: session.jwt_username,
                        port_forwards: session.port_forwards,
                    })
                    .collect(),
            },
            networks_revision: full.networks_revision,
            session_expires_at: response.session_expires_at.as_ref().and_then(system_time),
        }
    }
}

impl From<pb::PeerState> for Peer {
    fn from(peer: pb::PeerState) -> Self {
        Self {
            status: PeerStatus::parse(&peer.conn_status),
            status_since: peer.conn_status_update.as_ref().and_then(system_time),
            last_handshake: peer.last_wireguard_handshake.as_ref().and_then(system_time),
            latency: peer.latency.as_ref().and_then(duration_from),
            bytes_rx: u64::try_from(peer.bytes_rx).unwrap_or_default(),
            bytes_tx: u64::try_from(peer.bytes_tx).unwrap_or_default(),
            local_ice: Ice { kind: peer.local_ice_candidate_type, endpoint: peer.local_ice_candidate_endpoint },
            remote_ice: Ice { kind: peer.remote_ice_candidate_type, endpoint: peer.remote_ice_candidate_endpoint },
            ip: peer.ip,
            ipv6: peer.ipv6,
            pub_key: peer.pub_key,
            fqdn: peer.fqdn,
            relayed: peer.relayed,
            relay_address: peer.relay_address,
            rosenpass: peer.rosenpass_enabled,
            networks: peer.networks,
        }
    }
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct Network {
    pub id: String,
    pub range: String,
    pub selected: bool,
    pub domains: Vec<String>,
    pub resolved_ips: Vec<(String, Vec<String>)>,
}

impl Network {
    #[must_use]
    pub fn is_exit_node(&self) -> bool {
        is_default_route(&self.range)
    }

    #[must_use]
    pub fn overlaps(&self, other: &Self) -> bool {
        self.id != other.id && !self.range.is_empty() && self.range == other.range
    }
}

impl std::fmt::Debug for Network {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Network").field("selected", &self.selected).finish_non_exhaustive()
    }
}

impl From<pb::Network> for Network {
    fn from(network: pb::Network) -> Self {
        let mut resolved: Vec<(String, Vec<String>)> =
            network.resolved_i_ps.into_iter().map(|(domain, list)| (domain, list.ips)).collect();
        resolved.sort();
        Self {
            id: network.id,
            range: network.range,
            selected: network.selected,
            domains: network.domains,
            resolved_ips: resolved,
        }
    }
}

#[must_use]
pub fn is_default_route(range: &str) -> bool {
    range.split(',').any(|part| {
        let Some((address, bits)) = part.trim().split_once('/') else {
            return false;
        };
        bits.trim() == "0" && address.trim().parse::<IpAddr>().is_ok_and(|address| address.is_unspecified())
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExitNode {
    pub id: String,
    pub selected: bool,
}

#[must_use]
pub fn exit_nodes(networks: &[Network]) -> Vec<ExitNode> {
    let mut nodes: Vec<ExitNode> = networks
        .iter()
        .filter(|network| network.is_exit_node())
        .map(|network| ExitNode { id: network.id.clone(), selected: network.selected })
        .collect();
    nodes.sort_by_key(|node| node.id.to_lowercase());
    nodes
}

#[must_use]
pub fn resources(networks: &[Network]) -> Vec<Network> {
    let mut list: Vec<Network> = networks.iter().filter(|network| !network.is_exit_node()).cloned().collect();
    list.sort_by_key(|network| network.id.to_lowercase());
    list
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub active: bool,
    pub email: Option<String>,
}

impl Profile {
    #[must_use]
    pub fn is_default(&self) -> bool {
        self.id == "default"
    }
}

impl std::fmt::Debug for Profile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Profile").field("id", &self.id).field("active", &self.active).finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActiveProfile {
    pub id: String,
    pub name: String,
    pub username: String,
}

#[derive(Clone, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // Mirrors the daemon's GetConfigResponse, one switch per setting
pub struct Config {
    pub management_url: String,
    pub admin_url: String,
    pub config_file: String,
    pub log_file: String,
    pub pre_shared_key_set: bool,
    pub interface_name: String,
    pub wireguard_port: i64,
    pub mtu: i64,
    pub disable_auto_connect: bool,
    pub server_ssh_allowed: bool,
    pub rosenpass_enabled: bool,
    pub rosenpass_permissive: bool,
    pub disable_notifications: bool,
    pub lazy_connection_enabled: bool,
    pub block_inbound: bool,
    pub network_monitor: bool,
    pub disable_dns: bool,
    pub disable_client_routes: bool,
    pub disable_server_routes: bool,
    pub block_lan_access: bool,
    pub enable_ssh_root: bool,
    pub enable_ssh_sftp: bool,
    pub enable_ssh_local_port_forwarding: bool,
    pub enable_ssh_remote_port_forwarding: bool,
    pub disable_ssh_auth: bool,
    pub ssh_jwt_cache_ttl: i32,
    pub disable_ipv6: bool,
    pub remote_jobs_allowed: bool,
    pub mdm_managed: Vec<String>,
}

impl Config {
    #[must_use]
    pub fn managed(&self, key: &str) -> bool {
        self.mdm_managed.iter().any(|field| field == key)
    }
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("pre_shared_key_set", &self.pre_shared_key_set)
            .field("disable_notifications", &self.disable_notifications)
            .field("mdm_managed", &self.mdm_managed)
            .finish_non_exhaustive()
    }
}

impl From<pb::GetConfigResponse> for Config {
    fn from(config: pb::GetConfigResponse) -> Self {
        Self {
            pre_shared_key_set: !config.pre_shared_key.is_empty(),
            management_url: config.management_url,
            admin_url: config.admin_url,
            config_file: config.config_file,
            log_file: config.log_file,
            interface_name: config.interface_name,
            wireguard_port: config.wireguard_port,
            mtu: config.mtu,
            disable_auto_connect: config.disable_auto_connect,
            server_ssh_allowed: config.server_ssh_allowed,
            rosenpass_enabled: config.rosenpass_enabled,
            rosenpass_permissive: config.rosenpass_permissive,
            disable_notifications: config.disable_notifications,
            lazy_connection_enabled: config.lazy_connection_enabled,
            block_inbound: config.block_inbound,
            network_monitor: config.network_monitor,
            disable_dns: config.disable_dns,
            disable_client_routes: config.disable_client_routes,
            disable_server_routes: config.disable_server_routes,
            block_lan_access: config.block_lan_access,
            enable_ssh_root: config.enable_ssh_root,
            enable_ssh_sftp: config.enable_sshsftp,
            enable_ssh_local_port_forwarding: config.enable_ssh_local_port_forwarding,
            enable_ssh_remote_port_forwarding: config.enable_ssh_remote_port_forwarding,
            disable_ssh_auth: config.disable_ssh_auth,
            ssh_jwt_cache_ttl: config.ssh_jwt_cache_ttl,
            disable_ipv6: config.disable_ipv6,
            remote_jobs_allowed: config.remote_jobs_allowed,
            mdm_managed: config.m_dm_managed_fields,
        }
    }
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct ConfigPatch {
    pub management_url: Option<String>,
    pub admin_url: Option<String>,
    pub interface_name: Option<String>,
    pub wireguard_port: Option<i64>,
    pub mtu: Option<i64>,
    pub pre_shared_key: Option<String>,
    pub disable_auto_connect: Option<bool>,
    pub server_ssh_allowed: Option<bool>,
    pub rosenpass_enabled: Option<bool>,
    pub rosenpass_permissive: Option<bool>,
    pub disable_notifications: Option<bool>,
    pub lazy_connection_enabled: Option<bool>,
    pub block_inbound: Option<bool>,
    pub network_monitor: Option<bool>,
    pub disable_dns: Option<bool>,
    pub disable_client_routes: Option<bool>,
    pub disable_server_routes: Option<bool>,
    pub block_lan_access: Option<bool>,
    pub enable_ssh_root: Option<bool>,
    pub enable_ssh_sftp: Option<bool>,
    pub enable_ssh_local_port_forwarding: Option<bool>,
    pub enable_ssh_remote_port_forwarding: Option<bool>,
    pub disable_ssh_auth: Option<bool>,
    pub ssh_jwt_cache_ttl: Option<i32>,
    pub disable_ipv6: Option<bool>,
}

impl ConfigPatch {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    #[must_use]
    pub fn guarded(&self) -> Self {
        Self {
            management_url: self.management_url.clone(),
            server_ssh_allowed: self.server_ssh_allowed,
            enable_ssh_root: self.enable_ssh_root,
            disable_ssh_auth: self.disable_ssh_auth,
            ..Self::default()
        }
    }

    #[must_use]
    pub fn up_flags(&self) -> Vec<String> {
        let mut flags = Vec::new();
        if let Some(url) = &self.management_url {
            flags.push(format!("--management-url {url}"));
        }
        if let Some(allowed) = self.server_ssh_allowed {
            flags.push(format!("--allow-server-ssh={allowed}"));
        }
        if let Some(root) = self.enable_ssh_root {
            flags.push(format!("--enable-ssh-root={root}"));
        }
        if let Some(disabled) = self.disable_ssh_auth {
            flags.push(format!("--disable-ssh-auth={disabled}"));
        }
        flags
    }

    pub(crate) fn into_request(self, profile: &ActiveProfile) -> pb::SetConfigRequest {
        pb::SetConfigRequest {
            username: profile.username.clone(),
            profile_name: profile.id.clone(),
            management_url: self.management_url.unwrap_or_default(),
            admin_url: self.admin_url.unwrap_or_default(),
            rosenpass_enabled: self.rosenpass_enabled,
            interface_name: self.interface_name,
            wireguard_port: self.wireguard_port,
            optional_pre_shared_key: self.pre_shared_key,
            disable_auto_connect: self.disable_auto_connect,
            server_ssh_allowed: self.server_ssh_allowed,
            rosenpass_permissive: self.rosenpass_permissive,
            network_monitor: self.network_monitor,
            disable_client_routes: self.disable_client_routes,
            disable_server_routes: self.disable_server_routes,
            disable_dns: self.disable_dns,
            block_lan_access: self.block_lan_access,
            disable_notifications: self.disable_notifications,
            lazy_connection_enabled: self.lazy_connection_enabled,
            block_inbound: self.block_inbound,
            mtu: self.mtu,
            enable_ssh_root: self.enable_ssh_root,
            enable_sshsftp: self.enable_ssh_sftp,
            enable_ssh_local_port_forwarding: self.enable_ssh_local_port_forwarding,
            enable_ssh_remote_port_forwarding: self.enable_ssh_remote_port_forwarding,
            disable_ssh_auth: self.disable_ssh_auth,
            ssh_jwt_cache_ttl: self.ssh_jwt_cache_ttl,
            disable_ipv6: self.disable_ipv6,
            ..pb::SetConfigRequest::default()
        }
    }
}

impl std::fmt::Debug for ConfigPatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConfigPatch")
            .field("pre_shared_key", &self.pre_shared_key.as_ref().map(|_| "<redacted>"))
            .field("management_url", &self.management_url.is_some())
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // Independent feature gates the daemon reports from its MDM policy
pub struct Features {
    pub disable_profiles: bool,
    pub disable_update_settings: bool,
    pub disable_networks: bool,
    pub disable_advanced_view: Option<bool>,
}

impl From<pb::GetFeaturesResponse> for Features {
    fn from(features: pb::GetFeaturesResponse) -> Self {
        Self {
            disable_profiles: features.disable_profiles,
            disable_update_settings: features.disable_update_settings,
            disable_networks: features.disable_networks,
            disable_advanced_view: features.disable_advanced_view,
        }
    }
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct LoginStart {
    pub needs_sso: bool,
    pub user_code: String,
    pub uri: String,
    pub profile_id: String,
}

impl std::fmt::Debug for LoginStart {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoginStart").field("needs_sso", &self.needs_sso).finish_non_exhaustive()
    }
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct ExtendStart {
    pub uri: String,
    pub user_code: String,
    pub device_code: String,
    pub expires_in: Duration,
}

impl std::fmt::Debug for ExtendStart {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExtendStart").field("expires_in", &self.expires_in).finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Extended {
    Until(Option<SystemTime>),
    Preempted,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Anonymize {
    None,
    #[default]
    Default,
    Strict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BundleOptions {
    pub anonymize: Anonymize,
    pub system_info: bool,
    pub upload: bool,
    pub log_file_count: u32,
}

impl Default for BundleOptions {
    fn default() -> Self {
        Self { anonymize: Anonymize::Default, system_info: true, upload: true, log_file_count: 1 }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BundleResult {
    pub path: String,
    pub uploaded_key: String,
    pub upload_failure: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Panic,
    Fatal,
    Error,
    Warn,
    #[default]
    Info,
    Debug,
    Trace,
}

impl LogLevel {
    pub(crate) const fn from_proto(value: i32) -> Self {
        match value {
            1 => Self::Panic,
            2 => Self::Fatal,
            3 => Self::Error,
            4 => Self::Warn,
            6 => Self::Debug,
            7 => Self::Trace,
            _ => Self::Info,
        }
    }

    pub(crate) const fn to_proto(self) -> i32 {
        match self {
            Self::Panic => 1,
            Self::Fatal => 2,
            Self::Error => 3,
            Self::Warn => 4,
            Self::Info => 5,
            Self::Debug => 6,
            Self::Trace => 7,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UpdateResult {
    pub success: bool,
    pub error: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ports {
    Any,
    One(u32),
    Range(u32, u32),
}

impl Ports {
    fn from_proto(info: Option<pb::PortInfo>) -> Self {
        match info.and_then(|info| info.port_selection) {
            Some(pb::port_info::PortSelection::Port(port)) => Self::One(port),
            Some(pb::port_info::PortSelection::Range(range)) => Self::Range(range.start, range.end),
            None => Self::Any,
        }
    }
}

impl std::fmt::Display for Ports {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Any => f.write_str("*"),
            Self::One(port) => write!(f, "{port}"),
            Self::Range(start, end) => write!(f, "{start}-{end}"),
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct ForwardingRule {
    pub protocol: String,
    pub destination: Ports,
    pub translated_address: String,
    pub translated_hostname: String,
    pub translated_port: Ports,
}

impl std::fmt::Debug for ForwardingRule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ForwardingRule").field("protocol", &self.protocol).finish_non_exhaustive()
    }
}

impl From<pb::ForwardingRule> for ForwardingRule {
    fn from(rule: pb::ForwardingRule) -> Self {
        Self {
            protocol: rule.protocol,
            destination: Ports::from_proto(rule.destination_port),
            translated_address: rule.translated_address,
            translated_hostname: rule.translated_hostname,
            translated_port: Ports::from_proto(rule.translated_port),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daemon_status_strings_map_to_states_and_unknown_ones_read_as_idle() {
        assert_eq!(State::parse("Connected"), State::Connected);
        assert_eq!(State::parse("NeedsLogin"), State::NeedsLogin);
        assert_eq!(State::parse("SessionExpired"), State::SessionExpired);
        assert_eq!(State::parse("LoginFailed"), State::LoginFailed);
        assert_eq!(State::parse("Connecting"), State::Connecting);
        assert_eq!(State::parse("Idle"), State::Idle);
        assert_eq!(State::parse("Whatever"), State::Idle);
        assert!(State::SessionExpired.needs_login() && !State::Connected.needs_login());
        assert!(State::NeedsLogin.needs_down_before_switch() && !State::Idle.needs_down_before_switch());
    }

    #[test]
    fn default_routes_are_exit_nodes_even_when_joined_with_their_ipv6_partner() {
        assert!(is_default_route("0.0.0.0/0"));
        assert!(is_default_route("::/0"));
        assert!(is_default_route("0.0.0.0/0, ::/0"));
        assert!(is_default_route("10.0.0.0/8, ::/0"));
        assert!(!is_default_route("10.0.0.0/8"));
        assert!(!is_default_route("0.0.0.0/8"));
        assert!(!is_default_route("1.0.0.0/0"));
        assert!(!is_default_route(""));
        assert!(!is_default_route("example.com"));
    }

    #[test]
    fn exit_nodes_and_resources_split_the_network_list_and_sort_by_name() {
        let network = |id: &str, range: &str, selected| Network {
            id: id.to_string(),
            range: range.to_string(),
            selected,
            ..Network::default()
        };
        let list = [
            network("office", "10.1.0.0/16", true),
            network("Zurich", "0.0.0.0/0", false),
            network("berlin", "0.0.0.0/0, ::/0", true),
            network("Apps", "", false),
        ];
        assert_eq!(
            exit_nodes(&list),
            [ExitNode { id: "berlin".into(), selected: true }, ExitNode { id: "Zurich".into(), selected: false }]
        );
        let names: Vec<String> = resources(&list).into_iter().map(|network| network.id).collect();
        assert_eq!(names, ["Apps", "office"]);
    }

    #[test]
    fn peers_are_named_by_the_first_label_of_their_fqdn_or_their_address() {
        let peer = Peer { fqdn: "laptop.netbird.cloud".into(), ip: "100.64.0.2".into(), ..Peer::default() };
        assert_eq!(peer.name(), "laptop");
        let bare = Peer { ip: "100.64.0.3".into(), ..Peer::default() };
        assert_eq!(bare.name(), "100.64.0.3");
    }

    #[test]
    fn debug_output_never_carries_keys_addresses_or_the_pre_shared_key() {
        let peer = Peer { pub_key: "SECRETKEY".into(), fqdn: "host.example".into(), ..Peer::default() };
        let patch = ConfigPatch { pre_shared_key: Some("hunter2".into()), ..ConfigPatch::default() };
        let start = LoginStart { uri: "https://idp/verify?code=1".into(), ..LoginStart::default() };
        let text = format!("{peer:?} {patch:?} {start:?}");
        for secret in ["SECRETKEY", "host.example", "hunter2", "idp/verify"] {
            assert!(!text.contains(secret), "{secret} leaked: {text}");
        }
    }

    #[test]
    fn the_guarded_part_of_a_patch_keeps_only_what_the_daemon_restricts_to_root() {
        let patch = ConfigPatch {
            management_url: Some("https://nb.example:443".into()),
            server_ssh_allowed: Some(true),
            disable_dns: Some(true),
            ..ConfigPatch::default()
        };
        let guarded = patch.guarded();
        assert_eq!(guarded.disable_dns, None);
        assert_eq!(guarded.server_ssh_allowed, Some(true));
        assert_eq!(guarded.up_flags(), ["--management-url https://nb.example:443", "--allow-server-ssh=true"]);
        assert!(ConfigPatch::default().is_empty() && !guarded.is_empty());
    }
}

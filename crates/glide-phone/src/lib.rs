pub mod client;
pub mod config;
pub mod crypto;
pub mod daemon;
pub mod discovery;
pub mod i18n;
pub mod link;
pub mod plugins;
pub mod protocol;
pub mod service;
pub mod sound;
pub mod state;
pub mod ui;

pub use config::{PairedDevice, PhoneConfig};
pub use crypto::cert::{IdentityCert, compute_fingerprint};
pub use discovery::udp::{DiscoveredPeer, bind_discovery_socket, broadcast_identity, receive_discovery_packet};
pub use link::framed::PacketStream;
pub use plugins::battery::{BatteryPayload, BatteryState, handle_battery_packet, local_battery_packet};
pub use plugins::clipboard::{ClipboardPayload, create_clipboard_packet, handle_clipboard_packet};
pub use plugins::find_device::{create_ring_packet, handle_find_this_device};
pub use plugins::notifications::{
    ActiveNotification, NotificationEvent, NotificationPayload, create_action_packet, create_reply_packet,
    create_request_packet, extract_auth_code, handle_notification_packet,
};
pub use plugins::presenter::{PresenterEvent, handle_presenter_packet};
pub use plugins::share::{SharePayload, create_share_text_packet, create_share_url_packet, handle_share_packet};
pub use protocol::identity::{DeviceType, IdentityBody, MAX_TCP_PORT, MIN_TCP_PORT, PROTOCOL_VERSION, UDP_PORT};
pub use protocol::packet::{
    NetworkPacket, PACKET_TYPE_BATTERY, PACKET_TYPE_CLIPBOARD, PACKET_TYPE_FINDMYPHONE_REQUEST, PACKET_TYPE_IDENTITY,
    PACKET_TYPE_NOTIFICATION, PACKET_TYPE_NOTIFICATION_ACTION, PACKET_TYPE_NOTIFICATION_REPLY,
    PACKET_TYPE_NOTIFICATION_REQUEST, PACKET_TYPE_PAIR, PACKET_TYPE_PING, PACKET_TYPE_PRESENTER,
    PACKET_TYPE_SHARE_REQUEST,
};
pub use protocol::pair::{PairBody, PairState};
pub use service::PhoneService;
pub use sound::{AlarmHandle, start_alarm_loop};
pub use state::{PhoneDeviceState, PhoneManager};
pub use ui::PhoneApp;

#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

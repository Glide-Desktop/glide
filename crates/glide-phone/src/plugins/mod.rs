pub mod battery;
pub mod clipboard;
pub mod find_device;
pub mod notifications;
pub mod presenter;
pub mod share;

pub use battery::{BatteryPayload, BatteryState, handle_battery_packet, local_battery_packet};
pub use clipboard::{ClipboardPayload, create_clipboard_packet, handle_clipboard_packet};
pub use find_device::{create_ring_packet, handle_find_this_device};
pub use notifications::{
    ActiveNotification, NotificationEvent, NotificationPayload, create_action_packet, create_reply_packet,
    create_request_packet, extract_auth_code, handle_notification_packet,
};
pub use presenter::{PresenterEvent, handle_presenter_packet};
pub use share::{SharePayload, create_share_text_packet, create_share_url_packet, handle_share_packet};

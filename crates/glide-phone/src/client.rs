use zbus::proxy;

#[proxy(interface = "org.glide.Phone", default_service = "org.glide.Phone", default_path = "/org/glide/Phone")]
pub trait PhoneDaemon {
    fn list_devices_json(&self) -> zbus::Result<String>;
    fn ring_phone(&self, device_id: &str) -> zbus::Result<bool>;
    fn stop_alarm(&self, device_id: &str) -> zbus::Result<bool>;
    fn accept_pair(&self, device_id: &str) -> zbus::Result<bool>;
    fn reject_pair(&self, device_id: &str) -> zbus::Result<bool>;
    fn dismiss_notification(&self, device_id: &str, notif_id: &str) -> zbus::Result<bool>;
    fn set_clipboard(&self, device_id: &str, content: &str) -> zbus::Result<bool>;
    fn clipboard_text(&self) -> zbus::Result<String>;
    fn share_text(&self, device_id: &str, text: &str) -> zbus::Result<bool>;
    fn share_url(&self, device_id: &str, url: &str) -> zbus::Result<bool>;
    fn take_presenter_event(&self) -> zbus::Result<String>;
    fn is_ringing(&self) -> zbus::Result<bool>;
}

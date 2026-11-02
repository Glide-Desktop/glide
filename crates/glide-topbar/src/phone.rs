#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PhonePairState {
    #[default]
    Unpaired,
    RequestedByLocal,
    RequestedByPeer,
    Paired,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhoneNotification {
    pub id: String,
    pub app_name: String,
    pub title: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PhoneState {
    pub id: String,
    pub name: String,
    pub connected: bool,
    pub pair_state: PhonePairState,
    pub battery: Option<u8>,
    pub charging: bool,
    pub ringing: bool,
    pub notifications: Vec<PhoneNotification>,
}

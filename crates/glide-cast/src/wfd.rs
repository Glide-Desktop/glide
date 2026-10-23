pub const CONTROL_PORT: u16 = 7236;
const DEVICE_INFO: u8 = 0;
const SESSION_AVAILABLE: u16 = 0b01 << 4;
const MAX_THROUGHPUT_MBPS: u16 = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Source,
    PrimarySink,
    SecondarySink,
    DualRole,
}

impl Role {
    const fn from_bits(bits: u16) -> Self {
        match bits & 0b11 {
            0 => Self::Source,
            1 => Self::PrimarySink,
            2 => Self::SecondarySink,
            _ => Self::DualRole,
        }
    }

    #[must_use]
    pub const fn shows_video(self) -> bool {
        matches!(self, Self::PrimarySink | Self::DualRole)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceInfo {
    pub role: Role,
    pub available: bool,
    pub content_protection: bool,
    pub control_port: u16,
    pub throughput_mbps: u16,
}

#[must_use]
pub fn source_subelements() -> Vec<u8> {
    let info = SESSION_AVAILABLE;
    let mut bytes = vec![DEVICE_INFO, 0, 6];
    bytes.extend_from_slice(&info.to_be_bytes());
    bytes.extend_from_slice(&CONTROL_PORT.to_be_bytes());
    bytes.extend_from_slice(&MAX_THROUGHPUT_MBPS.to_be_bytes());
    bytes
}

#[must_use]
pub fn device_info(subelements: &[u8]) -> Option<DeviceInfo> {
    let mut rest = subelements;
    while rest.len() >= 3 {
        let id = rest[0];
        let length = usize::from(u16::from_be_bytes([rest[1], rest[2]]));
        let body = rest.get(3..3 + length)?;
        if id == DEVICE_INFO && length >= 6 {
            let bits = u16::from_be_bytes([body[0], body[1]]);
            return Some(DeviceInfo {
                role: Role::from_bits(bits),
                available: (bits >> 4) & 0b11 == 0b01,
                content_protection: bits & (1 << 8) != 0,
                control_port: u16::from_be_bytes([body[2], body[3]]),
                throughput_mbps: u16::from_be_bytes([body[4], body[5]]),
            });
        }
        rest = &rest[3 + length..];
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glide_announces_itself_as_an_available_source_on_the_wfd_port() {
        assert_eq!(source_subelements(), [0x00, 0x00, 0x06, 0x00, 0x10, 0x1c, 0x44, 0x00, 0xc8]);
        let info = device_info(&source_subelements()).unwrap();
        assert_eq!(info.role, Role::Source);
        assert!(info.available && !info.content_protection);
        assert_eq!((info.control_port, info.throughput_mbps), (7236, 200));
    }

    #[test]
    fn a_tv_reports_itself_as_a_primary_sink() {
        let tv = [0x00, 0x00, 0x06, 0x01, 0x11, 0x1c, 0x44, 0x00, 0x32];
        let info = device_info(&tv).unwrap();
        assert_eq!(info.role, Role::PrimarySink);
        assert!(info.available && info.content_protection && info.role.shows_video());
    }

    #[test]
    fn other_subelements_are_skipped_and_truncated_ones_rejected() {
        let mut ies = vec![0x01, 0x00, 0x06, 1, 2, 3, 4, 5, 6];
        ies.extend_from_slice(&[0x00, 0x00, 0x06, 0x00, 0x11, 0x1c, 0x44, 0x00, 0x0a]);
        assert_eq!(device_info(&ies).unwrap().role, Role::PrimarySink);
        assert_eq!(device_info(&[0x00, 0x00, 0x06, 0x00]), None);
        assert_eq!(device_info(&[]), None);
    }
}

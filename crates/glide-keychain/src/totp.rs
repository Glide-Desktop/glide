use hmac::{Hmac, Mac};
use zeroize::Zeroizing;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Algorithm {
    Sha1,
    Sha256,
    Sha512,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Totp {
    key: Zeroizing<Vec<u8>>,
    pub algorithm: Algorithm,
    pub digits: u32,
    pub period: u32,
}

fn base32(text: &str) -> Option<Vec<u8>> {
    let mut cleaned: String =
        text.chars().filter(|ch| !ch.is_whitespace() && *ch != '-').map(|ch| ch.to_ascii_uppercase()).collect();
    while !cleaned.len().is_multiple_of(8) {
        cleaned.push('=');
    }
    LENIENT_BASE32.decode(cleaned.as_bytes()).ok().filter(|key| !key.is_empty())
}

static LENIENT_BASE32: std::sync::LazyLock<data_encoding::Encoding> = std::sync::LazyLock::new(|| {
    let mut spec = data_encoding::BASE32.specification();
    spec.check_trailing_bits = false;
    spec.encoding().expect("RFC 4648 base32 without the trailing-bit check is a valid specification")
});

fn decode_query(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).ok();
                match hex.and_then(|hex| u8::from_str_radix(hex, 16).ok()) {
                    Some(byte) => {
                        out.push(byte);
                        index += 3;
                        continue;
                    }
                    None => out.push(b'%'),
                }
            }
            b'+' => out.push(b' '),
            byte => out.push(byte),
        }
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

impl Totp {
    #[must_use]
    pub fn parse(source: &str) -> Option<Self> {
        let source = source.trim();
        let Some(rest) = source.strip_prefix("otpauth://totp/") else {
            return Some(Self {
                key: Zeroizing::new(base32(source)?),
                algorithm: Algorithm::Sha1,
                digits: 6,
                period: 30,
            });
        };
        let query = rest.split_once('?').map_or("", |(_, query)| query);
        let mut totp = Self { key: Zeroizing::new(Vec::new()), algorithm: Algorithm::Sha1, digits: 6, period: 30 };
        for pair in query.split('&') {
            let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
            let value = decode_query(value);
            match name.to_ascii_lowercase().as_str() {
                "secret" => totp.key = Zeroizing::new(base32(&value)?),
                "algorithm" => {
                    totp.algorithm = match value.to_ascii_uppercase().as_str() {
                        "SHA1" => Algorithm::Sha1,
                        "SHA256" => Algorithm::Sha256,
                        "SHA512" => Algorithm::Sha512,
                        _ => return None,
                    };
                }
                "digits" => totp.digits = value.parse().ok().filter(|digits| (6..=10).contains(digits))?,
                "period" => totp.period = value.parse().ok().filter(|period| *period > 0)?,
                _ => {}
            }
        }
        (!totp.key.is_empty()).then_some(totp)
    }

    fn mac(&self, counter: u64) -> Vec<u8> {
        let message = counter.to_be_bytes();
        match self.algorithm {
            Algorithm::Sha1 => {
                let mut mac = Hmac::<sha1::Sha1>::new_from_slice(&self.key).expect("hmac takes any key length");
                mac.update(&message);
                mac.finalize().into_bytes().to_vec()
            }
            Algorithm::Sha256 => {
                let mut mac = Hmac::<sha2::Sha256>::new_from_slice(&self.key).expect("hmac takes any key length");
                mac.update(&message);
                mac.finalize().into_bytes().to_vec()
            }
            Algorithm::Sha512 => {
                let mut mac = Hmac::<sha2::Sha512>::new_from_slice(&self.key).expect("hmac takes any key length");
                mac.update(&message);
                mac.finalize().into_bytes().to_vec()
            }
        }
    }

    #[must_use]
    pub fn code_at(&self, unix: u64) -> String {
        let digest = self.mac(unix / u64::from(self.period));
        let offset = usize::from(digest[digest.len() - 1] & 0x0f);
        let binary =
            u32::from_be_bytes([digest[offset] & 0x7f, digest[offset + 1], digest[offset + 2], digest[offset + 3]]);
        let modulo = 10_u64.pow(self.digits);
        format!("{:0width$}", u64::from(binary) % modulo, width = self.digits as usize)
    }

    #[must_use]
    pub fn seconds_left(&self, unix: u64) -> u32 {
        self.period - (unix % u64::from(self.period)) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RFC_SHA1: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

    #[test]
    fn the_rfc_6238_vectors_match() {
        let totp = Totp { digits: 8, ..Totp::parse(RFC_SHA1).unwrap() };
        assert_eq!(totp.code_at(59), "94287082");
        assert_eq!(totp.code_at(1_111_111_109), "07081804");
        assert_eq!(totp.code_at(2_000_000_000), "69279037");
    }

    #[test]
    fn sha256_follows_its_rfc_vector() {
        let key = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZA";
        let uri = format!("otpauth://totp/x?secret={key}&algorithm=SHA256&digits=8");
        assert_eq!(Totp::parse(&uri).unwrap().code_at(59), "46119246");
    }

    #[test]
    fn an_otpauth_uri_sets_digits_and_period() {
        let totp =
            Totp::parse("otpauth://totp/GitHub:artem?secret=jbsw%20y3dp-ehpk3pxp&issuer=GitHub&period=60").unwrap();
        assert_eq!((totp.digits, totp.period), (6, 60));
        assert_eq!(totp.seconds_left(61), 59);
        assert_eq!(totp.code_at(0).len(), 6);
    }

    #[test]
    fn a_plain_key_tolerates_spaces_and_lower_case() {
        assert_eq!(
            Totp::parse("jbsw y3dp ehpk 3pxp").unwrap().code_at(0),
            Totp::parse("JBSWY3DPEHPK3PXP").unwrap().code_at(0)
        );
        assert!(Totp::parse("not base32!").is_none());
        assert!(Totp::parse("JBSWY3DPEHPK3PXPJBSWY3DPEH").is_some(), "unused trailing bits are ignored, as apps do");
        assert!(Totp::parse("otpauth://totp/x?issuer=nobody").is_none());
    }
}

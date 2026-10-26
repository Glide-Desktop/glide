use aes::cipher::{BlockDecryptMut, BlockEncryptMut, KeyIvInit, block_padding::Pkcs7};
use rsa::BigUint;
use sha2::Sha256;
use zeroize::Zeroizing;

pub const PLAIN: &str = "plain";
pub const DH: &str = "dh-ietf1024-sha256-aes128-cbc-pkcs7";

const PRIME: &str = concat!(
    "FFFFFFFFFFFFFFFFC90FDAA22168C234C4C6628B80DC1CD1",
    "29024E088A67CC74020BBEA63B139B22514A08798E3404DD",
    "EF9519B3CD3A431B302B0A6DF25F14374FE1356D6D51C245",
    "E485B576625E7EC6F44C42E9A637ED6B0BFF5CB6F406B7ED",
    "EE386BFB5A899FA5AE9F24117C4B1FE649286651ECE65381",
    "FFFFFFFFFFFFFFFF",
);
const BYTES: usize = 128;

type Encryptor = cbc::Encryptor<aes::Aes128>;
type Decryptor = cbc::Decryptor<aes::Aes128>;

#[derive(Clone)]
pub struct SessionKey(Zeroizing<[u8; 16]>);

impl std::fmt::Debug for SessionKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SessionKey(..)")
    }
}

fn prime() -> BigUint {
    BigUint::parse_bytes(PRIME.as_bytes(), 16).expect("the RFC 2409 group 2 prime parses")
}

fn padded(value: &BigUint) -> Zeroizing<Vec<u8>> {
    let bytes = value.to_bytes_be();
    let mut out = Zeroizing::new(vec![0u8; BYTES.saturating_sub(bytes.len())]);
    out.extend_from_slice(&bytes);
    out
}

#[must_use]
pub fn agree(peer: &[u8]) -> Option<(Vec<u8>, SessionKey)> {
    let prime = prime();
    let peer = BigUint::from_bytes_be(peer);
    let one = BigUint::from(1u8);
    if peer.bits() == 0 || peer <= one || peer >= &prime - &one {
        return None;
    }
    let mut private = Zeroizing::new([0u8; BYTES]);
    getrandom::fill(private.as_mut()).ok()?;
    let exponent = BigUint::from_bytes_be(private.as_ref());
    let public = BigUint::from(2u8).modpow(&exponent, &prime);
    let shared = padded(&peer.modpow(&exponent, &prime));
    Some((public.to_bytes_be(), derive(&shared)))
}

pub struct Client {
    exponent: BigUint,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Client(..)")
    }
}

impl Client {
    #[must_use]
    pub fn start() -> Option<(Self, Vec<u8>)> {
        let mut private = Zeroizing::new([0u8; BYTES]);
        getrandom::fill(private.as_mut()).ok()?;
        let exponent = BigUint::from_bytes_be(private.as_ref());
        let public = BigUint::from(2u8).modpow(&exponent, &prime()).to_bytes_be();
        Some((Self { exponent }, public))
    }

    #[must_use]
    pub fn finish(&self, server: &[u8]) -> SessionKey {
        derive(&padded(&BigUint::from_bytes_be(server).modpow(&self.exponent, &prime())))
    }
}

fn derive(shared: &[u8]) -> SessionKey {
    let mut key = Zeroizing::new([0u8; 16]);
    hkdf::Hkdf::<Sha256>::new(None, shared).expand(&[], key.as_mut()).expect("16 bytes is a valid HKDF length");
    SessionKey(key)
}

impl SessionKey {
    #[must_use]
    pub fn seal(&self, plain: &[u8]) -> Option<(Vec<u8>, Vec<u8>)> {
        let mut iv = [0u8; 16];
        getrandom::fill(&mut iv).ok()?;
        let sealed = Encryptor::new(self.0.as_ref().into(), &iv.into()).encrypt_padded_vec_mut::<Pkcs7>(plain);
        Some((iv.to_vec(), sealed))
    }

    #[must_use]
    pub fn open(&self, iv: &[u8], sealed: &[u8]) -> Option<Zeroizing<Vec<u8>>> {
        let iv: [u8; 16] = iv.try_into().ok()?;
        Decryptor::new(self.0.as_ref().into(), &iv.into())
            .decrypt_padded_vec_mut::<Pkcs7>(sealed)
            .ok()
            .map(Zeroizing::new)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_sides_reach_the_same_key_and_secrets_round_trip() {
        let (client, public) = Client::start().expect("client");
        let (server_public, server_key) = agree(&public).expect("a valid peer");
        let client_key = client.finish(&server_public);
        let (iv, sealed) = client_key.seal(b"chrome-key").expect("seal");
        assert_ne!(sealed, b"chrome-key");
        assert_eq!(server_key.open(&iv, &sealed).expect("open").as_slice(), b"chrome-key");
        let (iv, sealed) = server_key.seal(b"reply").expect("seal");
        assert_eq!(client_key.open(&iv, &sealed).expect("open").as_slice(), b"reply");
    }

    #[test]
    fn values_outside_the_group_are_refused() {
        assert!(agree(&[]).is_none());
        assert!(agree(&[1]).is_none());
        assert!(agree(&(prime() - BigUint::from(1u8)).to_bytes_be()).is_none());
        assert!(agree(&prime().to_bytes_be()).is_none());
        assert!(SessionKey(Zeroizing::new([0u8; 16])).open(&[0u8; 3], &[0u8; 16]).is_none());
    }
}

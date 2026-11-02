use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair};
use std::fmt::Write;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CertError {
    #[error("rcgen generation error: {0}")]
    Rcgen(#[from] rcgen::Error),
    #[error("certificate IO error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone)]
pub struct IdentityCert {
    pub cert_pem: String,
    pub key_pem: String,
    pub cert_der: Vec<u8>,
    pub key_der: Vec<u8>,
    pub fingerprint: String,
}

impl IdentityCert {
    pub fn generate(common_name: &str) -> Result<Self, CertError> {
        let key_pair = KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256)?;
        let mut params = CertificateParams::default();
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, common_name);
        params.distinguished_name = dn;

        let cert = params.self_signed(&key_pair)?;
        let cert_der = cert.der().to_vec();
        let key_der = key_pair.serialize_der();
        let cert_pem = cert.pem();
        let key_pem = key_pair.serialize_pem();
        let fingerprint = compute_fingerprint(&cert_der);

        Ok(Self { cert_pem, key_pem, cert_der, key_der, fingerprint })
    }

    #[must_use]
    pub fn from_der(cert_der: Vec<u8>, key_der: Vec<u8>, cert_pem: String, key_pem: String) -> Self {
        let fingerprint = compute_fingerprint(&cert_der);
        Self { cert_pem, key_pem, cert_der, key_der, fingerprint }
    }
}

#[must_use]
pub fn compute_fingerprint(cert_der: &[u8]) -> String {
    use ring::digest::{SHA256, digest};
    let hash = digest(&SHA256, cert_der);
    let mut out = String::with_capacity(hash.as_ref().len() * 3);
    for (i, byte) in hash.as_ref().iter().enumerate() {
        if i > 0 {
            out.push(':');
        }
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

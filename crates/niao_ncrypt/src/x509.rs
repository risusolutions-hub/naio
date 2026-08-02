use crate::error::{NcryptError, NcryptResult};
use niao_crypto::hex;
use sha2::{Digest, Sha256};
use x509_parser::prelude::*;

#[derive(Debug, Clone)]
pub struct ParsedCert {
    pub subject: String,
    pub issuer: String,
    pub serial: String,
    pub not_before: i64,
    pub not_after: i64,
    pub version: u32,
    pub is_ca: bool,
    pub signature_algorithm: String,
    pub public_key_algorithm: String,
    pub public_key_pem: String,
    pub fingerprint_sha256: String,
    pub san_dns: Vec<String>,
    pub raw_der: Vec<u8>,
}

fn extract_san_dns(cert: &X509Certificate) -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(Some(ext)) = cert.subject_alternative_name() {
        for name in ext.value.general_names.iter() {
            if let GeneralName::DNSName(d) = name {
                out.push(d.to_string());
            }
        }
    }
    out
}

fn public_key_info(cert: &X509Certificate) -> (String, String) {
    let alg = cert.public_key().algorithm.algorithm.to_id_string();
    let kind = if alg.contains("1.2.840.113549.1.1.1") {
        "rsa"
    } else if alg.contains("1.3.101.112") {
        "ed25519"
    } else if alg.contains("1.2.840.10045") {
        "ec"
    } else {
        "unknown"
    };
    let pem_str = ::pem::encode(&::pem::Pem::new(
        "PUBLIC KEY",
        cert.public_key().raw.to_vec(),
    ));
    (kind.to_string(), pem_str)
}

fn load_der(input: &[u8]) -> NcryptResult<Vec<u8>> {
    if input.starts_with(b"-----BEGIN") {
        let text = std::str::from_utf8(input).map_err(|e| {
            NcryptError::ParseFailed(format!("certificate PEM is not valid UTF-8: {e}"))
        })?;
        let pems = ::pem::parse_many(text)
            .map_err(|e| NcryptError::ParseFailed(format!("invalid PEM: {e}")))?;
        pems.into_iter()
            .find(|p| p.tag() == "CERTIFICATE")
            .map(|p| p.contents().to_vec())
            .ok_or_else(|| NcryptError::ParseFailed("PEM contains no CERTIFICATE block".into()))
    } else {
        Ok(input.to_vec())
    }
}

/// Parse an X.509 certificate from PEM or DER.
pub fn x509_parse(input: &[u8]) -> NcryptResult<ParsedCert> {
    let der = load_der(input)?;
    let (_, cert) = X509Certificate::from_der(&der)
        .map_err(|e| NcryptError::ParseFailed(format!("X.509 parse failed: {e}")))?;

    let fp = hex::encode(&Sha256::digest(&der));
    let (public_key_algorithm, public_key_pem) = public_key_info(&cert);
    let is_ca = cert
        .basic_constraints()
        .ok()
        .flatten()
        .map(|bc| bc.value.ca)
        .unwrap_or(false);

    Ok(ParsedCert {
        subject: cert.subject().to_string(),
        issuer: cert.issuer().to_string(),
        serial: cert.raw_serial_as_string(),
        not_before: cert.validity().not_before.timestamp(),
        not_after: cert.validity().not_after.timestamp(),
        version: cert.version().0,
        is_ca,
        signature_algorithm: cert.signature_algorithm.algorithm.to_id_string(),
        public_key_algorithm,
        public_key_pem,
        fingerprint_sha256: fp,
        san_dns: extract_san_dns(&cert),
        raw_der: der,
    })
}

/// Convert PEM certificate to DER bytes.
pub fn x509_pem_to_der(input: &str) -> NcryptResult<Vec<u8>> {
    load_der(input.as_bytes())
}

/// Return SHA-256 fingerprint hex of certificate bytes.
pub fn x509_fingerprint_sha256(input: &[u8]) -> NcryptResult<String> {
    let der = load_der(input)?;
    Ok(hex::encode(&Sha256::digest(&der)))
}

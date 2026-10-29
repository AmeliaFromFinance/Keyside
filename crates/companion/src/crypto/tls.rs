//! Self-signed certificate for browsers on the LAN. The secure channel does not rely on it;
//! browsers need HTTPS for the crypto APIs the deck uses.

use std::net::IpAddr;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair, SanType};
use rustls::ServerConfig;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use sha2::{Digest, Sha256};

use crate::store;

pub struct Tls {
    pub config: Arc<ServerConfig>,
    /// SHA-256 of the certificate, shown as AA:BB:... so people can compare it in their browser.
    pub fingerprint: String,
}

/// Loads the stored certificate, or makes a new one when it is missing, expired soon,
/// or does not cover every current address.
pub fn load_or_create(dir: &Path, ips: &[IpAddr], hostname: &str) -> Result<Tls> {
    let cert_path = dir.join("tls-cert.der");
    let key_path = dir.join("tls-key.der");
    let meta_path = dir.join("tls.json");

    let meta: Option<Meta> = store::read_json(&meta_path).ok().flatten();
    let now = unix_now();
    let reuse = match &meta {
        Some(m) => m.expires > now + 30 * 86_400 && ips.iter().all(|ip| m.ips.contains(ip)) && cert_path.exists() && key_path.exists(),
        None => false,
    };

    if !reuse {
        let mut all_ips: Vec<IpAddr> = meta.map(|m| m.ips).unwrap_or_default();
        for ip in ips {
            if !all_ips.contains(ip) {
                all_ips.push(*ip);
            }
        }
        let (cert, key) = generate(&all_ips, hostname)?;
        store::write_private(&key_path, &key)?;
        store::write_atomic(&cert_path, &cert)?;
        store::write_json(&meta_path, &Meta { ips: all_ips, expires: now + VALID_SECS })?;
    }

    let cert = std::fs::read(&cert_path).context("reading certificate")?;
    let key = std::fs::read(&key_path).context("reading certificate key")?;
    let fingerprint = Sha256::digest(&cert).iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(":");

    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut config = ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()?
        .with_no_client_auth()
        .with_single_cert(vec![CertificateDer::from(cert)], PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key)))?;
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(Tls { config: Arc::new(config), fingerprint })
}

const VALID_SECS: u64 = 3 * 365 * 86_400;

#[derive(serde::Serialize, serde::Deserialize)]
struct Meta {
    ips: Vec<IpAddr>,
    expires: u64,
}

fn generate(ips: &[IpAddr], hostname: &str) -> Result<(Vec<u8>, Vec<u8>)> {
    let mut params = CertificateParams::default();
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, "Keyside companion");
    params.distinguished_name = dn;
    params.subject_alt_names = ips.iter().map(|ip| SanType::IpAddress(*ip)).collect();
    params.subject_alt_names.push(SanType::DnsName("localhost".try_into()?));
    if let Ok(name) = format!("{}.local", sanitize_host(hostname)).try_into() {
        params.subject_alt_names.push(SanType::DnsName(name));
    }
    let start = time::OffsetDateTime::now_utc() - time::Duration::days(1);
    params.not_before = start;
    params.not_after = start + time::Duration::seconds(VALID_SECS as i64);
    let key = KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256)?;
    let cert = params.self_signed(&key)?;
    Ok((cert.der().to_vec(), key.serialize_der()))
}

fn sanitize_host(h: &str) -> String {
    let s: String = h.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').take(63).collect();
    if s.is_empty() { "keyside".into() } else { s.to_ascii_lowercase() }
}

fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

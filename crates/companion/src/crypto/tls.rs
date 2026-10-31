//! HTTPS certificate for browsers on the LAN. The secure channel does not rely on it;
//! browsers need HTTPS for the crypto APIs the deck uses.
//!
//! A small certificate authority signs it. Phones can trust that authority once (iPhones need
//! this for home screen apps), and it only vouches for local network addresses, so it can't be
//! used to impersonate other websites.

use std::net::IpAddr;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use rcgen::{
    BasicConstraints, CertificateParams, CidrSubnet, DistinguishedName, DnType, ExtendedKeyUsagePurpose, GeneralSubtree, IsCa, Issuer, KeyPair, KeyUsagePurpose, NameConstraints,
    SanType,
};
use rustls::ServerConfig;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use sha2::{Digest, Sha256};

use crate::store;

pub struct Tls {
    pub config: Arc<ServerConfig>,
    /// SHA-256 of the certificate, shown as AA:BB:... so people can compare it in their browser.
    pub fingerprint: String,
    /// The certificate authority, for phones to install.
    pub ca: Vec<u8>,
    /// SHA-256 of the authority, to compare when installing it.
    pub ca_fingerprint: String,
}

/// Loads the stored certificate, or makes a new one when it is missing, expired soon,
/// or does not cover every current address.
pub fn load_or_create(dir: &Path, ips: &[IpAddr], hostname: &str) -> Result<Tls> {
    let cert_path = dir.join("tls-cert.der");
    let key_path = dir.join("tls-key.der");
    let meta_path = dir.join("tls.json");
    let ca_path = dir.join("ca-cert.der");
    let ca_key_path = dir.join("ca-key.der");

    // Older versions made a self-signed certificate. Start over with an authority.
    let fresh_ca = !(ca_path.exists() && ca_key_path.exists());
    if fresh_ca {
        let (cert, key) = generate_ca()?;
        store::write_private(&ca_key_path, &key)?;
        store::write_atomic(&ca_path, &cert)?;
    }
    let ca_key = KeyPair::try_from(std::fs::read(&ca_key_path).context("reading certificate authority key")?.as_slice())?;
    let ca = std::fs::read(&ca_path).context("reading certificate authority")?;

    let ips: Vec<IpAddr> = ips.iter().copied().filter(local).collect();
    let meta: Option<Meta> = store::read_json(&meta_path).ok().flatten();
    let now = unix_now();
    let reuse = !fresh_ca
        && match &meta {
            Some(m) => m.expires > now + 30 * 86_400 && ips.iter().all(|ip| m.ips.contains(ip)) && cert_path.exists() && key_path.exists(),
            None => false,
        };

    if !reuse {
        // Keep addresses seen before, so moving between known networks reuses the certificate.
        let mut all_ips: Vec<IpAddr> = meta.map(|m| m.ips).unwrap_or_default();
        all_ips.retain(|ip| !ips.contains(ip) && local(ip));
        if all_ips.len() + ips.len() > MAX_SAN_IPS {
            all_ips.drain(..(all_ips.len() + ips.len()).saturating_sub(MAX_SAN_IPS).min(all_ips.len()));
        }
        all_ips.extend_from_slice(&ips);
        let (cert, key) = generate(&all_ips, hostname, &ca_key)?;
        store::write_private(&key_path, &key)?;
        store::write_atomic(&cert_path, &cert)?;
        store::write_json(&meta_path, &Meta { ips: all_ips, expires: now + VALID_SECS })?;
    }

    let cert = std::fs::read(&cert_path).context("reading certificate")?;
    let key = std::fs::read(&key_path).context("reading certificate key")?;
    let fingerprint = sha256(&cert);
    let ca_fingerprint = sha256(&ca);

    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut config = ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()?
        .with_no_client_auth()
        .with_single_cert(vec![CertificateDer::from(cert), CertificateDer::from(ca.clone())], PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key)))?;
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(Tls { config: Arc::new(config), fingerprint, ca, ca_fingerprint })
}

fn sha256(der: &[u8]) -> String {
    Sha256::digest(der).iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(":")
}

/// Apple refuses server certificates valid for longer than this.
const VALID_SECS: u64 = 397 * 86_400;
const CA_VALID_DAYS: i64 = 20 * 365;
const MAX_SAN_IPS: usize = 32;

#[derive(serde::Serialize, serde::Deserialize)]
struct Meta {
    ips: Vec<IpAddr>,
    expires: u64,
}

/// Private, link-local, loopback and Tailscale addresses: the only ones the authority covers.
fn local(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => v.is_private() || v.is_loopback() || v.is_link_local() || (v.octets()[0] == 100 && (64..128).contains(&v.octets()[1])),
        IpAddr::V6(v) => v.is_loopback() || (v.segments()[0] & 0xfe00) == 0xfc00 || (v.segments()[0] & 0xffc0) == 0xfe80,
    }
}

fn ca_params() -> CertificateParams {
    let mut params = CertificateParams::default();
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, "Keyside local network");
    params.distinguished_name = dn;
    params.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
    params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign, KeyUsagePurpose::DigitalSignature];
    let v4 = |a: [u8; 4], prefix| GeneralSubtree::IpAddress(CidrSubnet::from_v4_prefix(a, prefix));
    let v6 = |a: [u16; 8], prefix| GeneralSubtree::IpAddress(CidrSubnet::from_v6_prefix(std::net::Ipv6Addr::from(a).octets(), prefix));
    params.name_constraints = Some(NameConstraints {
        permitted_subtrees: vec![
            v4([10, 0, 0, 0], 8),
            v4([172, 16, 0, 0], 12),
            v4([192, 168, 0, 0], 16),
            v4([127, 0, 0, 0], 8),
            v4([169, 254, 0, 0], 16),
            v4([100, 64, 0, 0], 10),
            v6([0xfc00, 0, 0, 0, 0, 0, 0, 0], 7),
            v6([0xfe80, 0, 0, 0, 0, 0, 0, 0], 10),
            v6([0, 0, 0, 0, 0, 0, 0, 1], 128),
            GeneralSubtree::DnsName("local".into()),
            GeneralSubtree::DnsName("localhost".into()),
        ],
        excluded_subtrees: Vec::new(),
    });
    params
}

fn generate_ca() -> Result<(Vec<u8>, Vec<u8>)> {
    let mut params = ca_params();
    let start = time::OffsetDateTime::now_utc() - time::Duration::days(1);
    params.not_before = start;
    params.not_after = start + time::Duration::days(CA_VALID_DAYS);
    let key = KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256)?;
    let cert = params.self_signed(&key)?;
    Ok((cert.der().to_vec(), key.serialize_der()))
}

fn generate(ips: &[IpAddr], hostname: &str, ca_key: &KeyPair) -> Result<(Vec<u8>, Vec<u8>)> {
    let mut params = CertificateParams::default();
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, "Keyside companion");
    params.distinguished_name = dn;
    params.subject_alt_names = ips.iter().map(|ip| SanType::IpAddress(*ip)).collect();
    params.subject_alt_names.push(SanType::DnsName("localhost".try_into()?));
    if let Ok(name) = format!("{}.local", sanitize_host(hostname)).try_into() {
        params.subject_alt_names.push(SanType::DnsName(name));
    }
    params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    params.use_authority_key_identifier_extension = true;
    let start = time::OffsetDateTime::now_utc() - time::Duration::days(1);
    params.not_before = start;
    params.not_after = start + time::Duration::seconds(VALID_SECS as i64);
    let key = KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256)?;
    let issuer = Issuer::new(ca_params(), ca_key);
    let cert = params.signed_by(&key, &issuer)?;
    Ok((cert.der().to_vec(), key.serialize_der()))
}

fn sanitize_host(h: &str) -> String {
    let s: String = h.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').take(63).collect();
    if s.is_empty() { "keyside".into() } else { s.to_ascii_lowercase() }
}

fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustls::client::WebPkiServerVerifier;
    use rustls::client::danger::ServerCertVerifier;
    use rustls::pki_types::{ServerName, UnixTime};

    fn verifier(ca: &[u8]) -> Arc<WebPkiServerVerifier> {
        let mut roots = rustls::RootCertStore::empty();
        roots.add(CertificateDer::from(ca.to_vec())).unwrap();
        WebPkiServerVerifier::builder_with_provider(Arc::new(roots), Arc::new(rustls::crypto::ring::default_provider())).build().unwrap()
    }

    #[test]
    fn phones_that_trust_the_authority_trust_the_companion() {
        let dir = tempfile::tempdir().unwrap();
        let ip: IpAddr = "192.168.1.20".parse().unwrap();
        let public: IpAddr = "8.8.8.8".parse().unwrap();
        let tls = load_or_create(dir.path(), &[ip, public], "Studio PC").unwrap();
        let leaf = CertificateDer::from(std::fs::read(dir.path().join("tls-cert.der")).unwrap());
        let v = verifier(&tls.ca);
        let now = UnixTime::now();
        v.verify_server_cert(&leaf, &[], &ServerName::IpAddress(ip.into()), &[], now).expect("local address");
        v.verify_server_cert(&leaf, &[], &ServerName::try_from("studiopc.local").unwrap(), &[], now).expect("mDNS name");
        assert!(v.verify_server_cert(&leaf, &[], &ServerName::IpAddress(public.into()), &[], now).is_err(), "public addresses are left out");

        // Same addresses again: the certificate is kept, and so is the authority.
        let again = load_or_create(dir.path(), &[ip], "Studio PC").unwrap();
        assert_eq!((again.fingerprint, again.ca_fingerprint), (tls.fingerprint, tls.ca_fingerprint));
    }

    #[test]
    fn the_authority_cannot_vouch_for_other_websites() {
        let dir = tempfile::tempdir().unwrap();
        let tls = load_or_create(dir.path(), &["10.0.0.5".parse().unwrap()], "pc").unwrap();
        let ca_key = KeyPair::try_from(std::fs::read(dir.path().join("ca-key.der")).unwrap().as_slice()).unwrap();
        let mut params = CertificateParams::new(vec!["example.com".to_string()]).unwrap();
        params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
        let key = KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).unwrap();
        let forged = params.signed_by(&key, &Issuer::new(ca_params(), &ca_key)).unwrap();
        let name = ServerName::try_from("example.com").unwrap();
        assert!(verifier(&tls.ca).verify_server_cert(forged.der(), &[], &name, &[], UnixTime::now()).is_err());
    }

    #[test]
    fn older_self_signed_certificates_are_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let ip: IpAddr = "192.168.1.20".parse().unwrap();
        let (cert, key) = {
            let mut p = CertificateParams::new(vec!["localhost".to_string()]).unwrap();
            p.subject_alt_names.push(SanType::IpAddress(ip));
            let k = KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).unwrap();
            (p.self_signed(&k).unwrap().der().to_vec(), k.serialize_der())
        };
        std::fs::write(dir.path().join("tls-cert.der"), &cert).unwrap();
        std::fs::write(dir.path().join("tls-key.der"), key).unwrap();
        store::write_json(&dir.path().join("tls.json"), &Meta { ips: vec![ip], expires: unix_now() + 1000 * 86_400 }).unwrap();
        let tls = load_or_create(dir.path(), &[ip], "pc").unwrap();
        assert_ne!(tls.fingerprint, sha256(&cert));
        let leaf = CertificateDer::from(std::fs::read(dir.path().join("tls-cert.der")).unwrap());
        verifier(&tls.ca).verify_server_cert(&leaf, &[], &ServerName::IpAddress(ip.into()), &[], UnixTime::now()).unwrap();
    }
}

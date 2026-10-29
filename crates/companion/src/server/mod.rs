//! Network side: the loopback editor listener and the LAN listener for paired devices.

pub mod admin;
pub mod api;
pub mod catalog;
pub mod conn;
pub mod handshake;
pub mod lan;
pub mod ops;
pub mod web;

use std::net::IpAddr;
use std::sync::Arc;

use anyhow::Result;
use serde_json::{Value, json};

use crate::app::App;

/// Private LAN, link-local, Tailscale/CGNAT and loopback addresses of this PC.
pub fn local_addresses() -> Vec<IpAddr> {
    let mut v: Vec<IpAddr> = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .map(|i| i.ip())
        .filter(|ip| match ip {
            IpAddr::V4(v4) => v4.is_private() || v4.octets()[0] == 100 && (64..128).contains(&v4.octets()[1]),
            IpAddr::V6(v6) => (v6.segments()[0] & 0xfe00) == 0xfc00,
        })
        .collect();
    v.sort_by_key(|ip| match ip {
        IpAddr::V4(v4) if v4.octets()[0] == 192 => 0,
        IpAddr::V4(v4) if v4.octets()[0] == 10 => 1,
        IpAddr::V4(v4) if v4.octets()[0] == 172 => 2,
        IpAddr::V4(_) => 3,
        IpAddr::V6(_) => 4,
    });
    v.dedup();
    v
}

/// What the pairing QR code holds. The link opens the browser deck; the native app reads
/// the same fragment. The fragment never reaches any server.
pub fn pairing_payload(app: &App, pid: &str, code: &str) -> Value {
    let mut hosts: Vec<String> = app.addresses.read().iter().map(|ip| if ip.is_ipv6() { format!("[{ip}]") } else { ip.to_string() }).collect();
    if hosts.is_empty() {
        // No private network found: only devices on this PC can connect.
        hosts.push("127.0.0.1".into());
    }
    let port = app.settings.read().lan_port;
    let cid = app.identity.read().cid.clone();
    let name = app.settings.read().name.clone();
    let enc = |s: &str| url::form_urlencoded::byte_serialize(s.as_bytes()).collect::<String>();
    let first = hosts.first().cloned().unwrap_or_else(|| "127.0.0.1".into());
    let frag = format!("pair&h={}&p={port}&c={cid}&i={pid}&k={code}&n={}", enc(&hosts.join(",")), enc(&name));
    json!({
        "url": format!("https://{first}:{port}/#{frag}"),
        "hosts": hosts,
        "port": port,
        "expiresIn": crate::devices::PAIRING_TTL.as_secs(),
        "fingerprint": *app.tls_fingerprint.read(),
    })
}

pub async fn start(app: Arc<App>) -> Result<()> {
    let addrs = local_addresses();
    *app.addresses.write() = addrs.clone();
    let settings = app.settings.read().clone();
    let hostname = settings.name.clone();
    let mut cert_ips = addrs.clone();
    cert_ips.push(IpAddr::from([127, 0, 0, 1]));
    let tls = crate::crypto::tls::load_or_create(&app.paths.dir("tls")?, &cert_ips, &hostname)?;
    *app.tls_fingerprint.write() = tls.fingerprint.clone();

    admin::start(app.clone(), settings.admin_port).await?;
    if settings.lan_enabled {
        lan::start(app.clone(), settings.lan_bind, settings.lan_port, tls.config.clone()).await?;
        if settings.mdns {
            crate::mdns::advertise(&app, settings.lan_port);
        }
    }
    Ok(())
}

//! Announces the companion on the local network as _keyside._tcp, so apps can find it
//! again when the PC's address changes. Only the companion id and name are published.

use std::sync::OnceLock;

use crate::app::App;

static DAEMON: OnceLock<mdns_sd::ServiceDaemon> = OnceLock::new();

pub fn advertise(app: &App, port: u16) {
    let ips = app.addresses.read().clone();
    if ips.is_empty() {
        return;
    }
    let name = app.settings.read().name.clone();
    let cid = app.identity.read().cid.clone();
    let host: String = name.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').take(40).collect();
    let host = format!("{}.local.", if host.is_empty() { "keyside" } else { &host });
    let props = [("cid", cid.as_str()), ("v", "1")];
    let result = mdns_sd::ServiceDaemon::new().and_then(|d| {
        let info = mdns_sd::ServiceInfo::new("_keyside._tcp.local.", &name, &host, ips.as_slice(), port, &props[..])?;
        d.register(info)?;
        Ok(d)
    });
    match result {
        Ok(d) => {
            let _ = DAEMON.set(d);
        }
        Err(e) => tracing::warn!("network announcement off: {e}"),
    }
}

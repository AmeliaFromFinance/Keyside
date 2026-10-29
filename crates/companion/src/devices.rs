//! Paired devices, pairing codes and PIN checks.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::config::clean_name;
use crate::crypto::channel::{DeviceInfo, b64, unb64};
use crate::{ids, store};

pub const PAIRING_TTL: Duration = Duration::from_secs(120);
const PIN_FREE_TRIES: u32 = 5;
const PIN_MAX_LOCK: Duration = Duration::from_secs(3600);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    /// Press keys and switch pages.
    Deck,
    /// Like deck, limited to chosen pages.
    Moderator,
    /// Deck plus layout editing. Can't add actions that run programs.
    Editor,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub app: String,
    pub scope: Scope,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pages: Vec<String>,
    /// Pre-shared key for the secure channel, base64url.
    pub secret: String,
    /// Argon2id hash of the PIN, when one is required.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pin: Option<String>,
    pub created: u64,
    #[serde(default)]
    pub last_seen: u64,
}

/// What the editor sees. Never includes the secret or the PIN hash.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevicePublic {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub app: String,
    pub scope: Scope,
    pub pages: Vec<String>,
    pub has_pin: bool,
    pub created: u64,
    pub last_seen: u64,
    pub online: bool,
}

#[derive(Debug, Clone)]
pub struct Pairing {
    pub pid: Vec<u8>,
    pub code: Vec<u8>,
    pub scope: Scope,
    pub pages: Vec<String>,
    pub pin: Option<String>,
    pub expires: Instant,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingDevice {
    pub pid: String,
    pub name: String,
    pub kind: String,
    pub app: String,
    pub sas: String,
    pub address: String,
}

#[derive(Default)]
struct PinState {
    fails: u32,
    until: Option<Instant>,
}

pub enum PinCheck {
    Ok,
    Wrong,
    Locked(Duration),
}

pub struct Devices {
    path: PathBuf,
    devices: HashMap<String, Device>,
    pairing: Option<Pairing>,
    pending: HashMap<String, PendingDevice>,
    pins: HashMap<String, PinState>,
}

impl Devices {
    pub fn load(path: PathBuf) -> Result<Self> {
        let list: Vec<Device> = store::read_json(&path)?.unwrap_or_default();
        Ok(Self { path, devices: list.into_iter().map(|d| (d.id.clone(), d)).collect(), pairing: None, pending: HashMap::new(), pins: HashMap::new() })
    }

    fn save(&self) -> Result<()> {
        let mut list: Vec<&Device> = self.devices.values().collect();
        list.sort_by_key(|d| d.created);
        store::write_json_private(&self.path, &list)
    }

    pub fn get(&self, id: &str) -> Option<&Device> {
        self.devices.get(id)
    }

    pub fn list(&self, online: &dyn Fn(&str) -> bool) -> Vec<DevicePublic> {
        let mut v: Vec<DevicePublic> = self
            .devices
            .values()
            .map(|d| DevicePublic {
                id: d.id.clone(),
                name: d.name.clone(),
                kind: d.kind.clone(),
                app: d.app.clone(),
                scope: d.scope,
                pages: d.pages.clone(),
                has_pin: d.pin.is_some(),
                created: d.created,
                last_seen: d.last_seen,
                online: online(&d.id),
            })
            .collect();
        v.sort_by_key(|d| d.created);
        v
    }

    /// Starts a pairing window, replacing any earlier one. Returns (pairing id, code).
    pub fn start_pairing(&mut self, scope: Scope, pages: Vec<String>, pin: Option<&str>) -> Result<(String, String)> {
        let pin = pin.map(hash_pin).transpose()?;
        let p = Pairing { pid: ids::random_bytes(8), code: ids::random_bytes(16), scope, pages, pin, expires: Instant::now() + PAIRING_TTL };
        let out = (b64(&p.pid), b64(&p.code));
        self.pairing = Some(p);
        Ok(out)
    }

    pub fn cancel_pairing(&mut self) {
        self.pairing = None;
    }

    /// Takes the pairing for a hello. Single use: a second hello with the same id fails.
    pub fn take_pairing(&mut self, pid: &str) -> Option<Pairing> {
        let pid = unb64(pid, Some(8)).ok()?;
        let p = self.pairing.take()?;
        if p.expires < Instant::now() {
            return None;
        }
        if !bool::from(subtle::ConstantTimeEq::ct_eq(p.pid.as_slice(), pid.as_slice())) {
            self.pairing = Some(p);
            return None;
        }
        Some(p)
    }

    pub fn add_pending(&mut self, pending: PendingDevice) {
        self.pending.insert(pending.pid.clone(), pending);
    }

    pub fn remove_pending(&mut self, pid: &str) -> Option<PendingDevice> {
        self.pending.remove(pid)
    }

    pub fn pending(&self) -> Vec<PendingDevice> {
        self.pending.values().cloned().collect()
    }

    /// Creates the device record once the person approved it on the PC.
    pub fn create(&mut self, info: &DeviceInfo, pairing: &Pairing) -> Result<Device> {
        let d = Device {
            id: ids::random_b64(16),
            name: clean_name(&info.name, 40).unwrap_or_else(|| "Device".into()),
            kind: pick(&info.kind, &["phone", "tablet", "desktop", "browser"], "phone"),
            app: pick(&info.app, &["web", "android", "ios", "desktop"], "web"),
            scope: pairing.scope,
            pages: pairing.pages.clone(),
            secret: ids::random_b64(32),
            pin: pairing.pin.clone(),
            created: now(),
            last_seen: now(),
        };
        self.devices.insert(d.id.clone(), d.clone());
        self.save()?;
        Ok(d)
    }

    pub fn update(&mut self, id: &str, name: Option<&str>, scope: Option<Scope>, pages: Option<Vec<String>>, pin: Option<Option<&str>>) -> Result<bool> {
        let new_pin = match pin {
            Some(Some(p)) => Some(Some(hash_pin(p)?)),
            Some(None) => Some(None),
            None => None,
        };
        let Some(d) = self.devices.get_mut(id) else { return Ok(false) };
        if let Some(n) = name.and_then(|n| clean_name(n, 40)) {
            d.name = n;
        }
        if let Some(s) = scope {
            d.scope = s;
        }
        if let Some(p) = pages {
            d.pages = p;
        }
        if let Some(p) = new_pin {
            d.pin = p;
            self.pins.remove(id);
        }
        self.save()?;
        Ok(true)
    }

    pub fn revoke(&mut self, id: &str) -> Result<bool> {
        let gone = self.devices.remove(id).is_some();
        self.pins.remove(id);
        if gone {
            self.save()?;
        }
        Ok(gone)
    }

    pub fn touch(&mut self, id: &str) {
        if let Some(d) = self.devices.get_mut(id) {
            d.last_seen = now();
            let _ = self.save();
        }
    }

    /// Checks a PIN with growing lockouts after five wrong tries.
    pub fn check_pin(&mut self, id: &str, pin: &str) -> PinCheck {
        let Some(hash) = self.devices.get(id).and_then(|d| d.pin.clone()) else { return PinCheck::Ok };
        let st = self.pins.entry(id.to_string()).or_default();
        if let Some(until) = st.until {
            let now = Instant::now();
            if until > now {
                return PinCheck::Locked(until - now);
            }
        }
        if verify_pin(&hash, pin) {
            *st = PinState::default();
            return PinCheck::Ok;
        }
        st.fails += 1;
        if st.fails >= PIN_FREE_TRIES {
            let lock = Duration::from_secs(30).saturating_mul(1 << (st.fails - PIN_FREE_TRIES).min(10)).min(PIN_MAX_LOCK);
            st.until = Some(Instant::now() + lock);
            return PinCheck::Locked(lock);
        }
        PinCheck::Wrong
    }
}

fn pick(v: &str, allowed: &[&str], fallback: &str) -> String {
    if allowed.contains(&v) { v.to_string() } else { fallback.to_string() }
}

pub fn valid_pin(pin: &str) -> bool {
    (4..=12).contains(&pin.len()) && pin.bytes().all(|b| b.is_ascii_digit())
}

fn hash_pin(pin: &str) -> Result<String> {
    use argon2::password_hash::PasswordHasher;
    anyhow::ensure!(valid_pin(pin), "PIN must be 4 to 12 digits");
    Ok(argon2::Argon2::default().hash_password(pin.as_bytes()).map_err(|e| anyhow::anyhow!("hashing PIN: {e}"))?.to_string())
}

fn verify_pin(hash: &str, pin: &str) -> bool {
    use argon2::password_hash::{PasswordVerifier, phc::PasswordHash};
    PasswordHash::new(hash).is_ok_and(|h| argon2::Argon2::default().verify_password(pin.as_bytes(), &h).is_ok())
}

pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn devices() -> (tempfile::TempDir, Devices) {
        let dir = tempfile::tempdir().unwrap();
        let d = Devices::load(dir.path().join("devices.json")).unwrap();
        (dir, d)
    }

    #[test]
    fn pairing_is_single_use_and_expires() {
        let (_t, mut d) = devices();
        let (pid, _) = d.start_pairing(Scope::Deck, vec![], None).unwrap();
        assert!(d.take_pairing("AAAAAAAAAAA").is_none());
        assert!(d.take_pairing(&pid).is_some());
        assert!(d.take_pairing(&pid).is_none());
        let (pid, _) = d.start_pairing(Scope::Deck, vec![], None).unwrap();
        d.pairing.as_mut().unwrap().expires = Instant::now() - Duration::from_secs(1);
        assert!(d.take_pairing(&pid).is_none());
    }

    #[test]
    fn pin_locks_out_after_five_misses() {
        let (_t, mut d) = devices();
        d.start_pairing(Scope::Deck, vec![], Some("2468")).unwrap();
        let p = d.pairing.clone().unwrap();
        let dev = d.create(&DeviceInfo { name: "Phone".into(), kind: "phone".into(), app: "web".into() }, &p).unwrap();
        assert!(matches!(d.check_pin(&dev.id, "2468"), PinCheck::Ok));
        for _ in 0..4 {
            assert!(matches!(d.check_pin(&dev.id, "0000"), PinCheck::Wrong));
        }
        assert!(matches!(d.check_pin(&dev.id, "0000"), PinCheck::Locked(_)));
        assert!(matches!(d.check_pin(&dev.id, "2468"), PinCheck::Locked(_)), "right PIN still waits");
    }

    #[test]
    fn stored_file_is_private() {
        let (t, mut d) = devices();
        d.start_pairing(Scope::Editor, vec![], None).unwrap();
        let p = d.pairing.clone().unwrap();
        d.create(&DeviceInfo { name: "\u{7}Tab\nlet".into(), kind: "evil".into(), app: "web".into() }, &p).unwrap();
        let saved = std::fs::read_to_string(t.path().join("devices.json")).unwrap();
        assert!(saved.contains("\"Tablet\"") && saved.contains("\"phone\""));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(t.path().join("devices.json")).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }
}

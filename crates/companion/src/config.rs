use std::net::IpAddr;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::{ids, store};

pub const DEFAULT_ADMIN_PORT: u16 = 7442;
pub const DEFAULT_LAN_PORT: u16 = 7443;
pub const DEFAULT_REGISTRY: &str = "https://raw.githubusercontent.com/OWNER/phonedeck/main/registry/index.json";

#[derive(Debug, Clone)]
pub struct Paths {
    pub root: PathBuf,
}

impl Paths {
    pub fn new(override_dir: Option<PathBuf>) -> Result<Self> {
        let root = match override_dir {
            Some(d) => d,
            None => directories::ProjectDirs::from("dev", "phonedeck", "PhoneDeck").context("no home directory")?.data_dir().to_path_buf(),
        };
        store::private_dir(&root)?;
        Ok(Self { root })
    }

    pub fn file(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }

    pub fn dir(&self, name: &str) -> Result<PathBuf> {
        let p = self.root.join(name);
        store::private_dir(&p)?;
        Ok(p)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// Shown on paired devices.
    pub name: String,
    pub admin_port: u16,
    pub lan_port: u16,
    /// Address the deck listens on. "0.0.0.0" for every network, or one address.
    pub lan_bind: IpAddr,
    pub lan_enabled: bool,
    /// Also serve the inbound API on the LAN port (HTTPS). Off by default: loopback only.
    pub api_on_lan: bool,
    pub mdns: bool,
    /// Switch profiles to match the focused app.
    pub follow_focus: bool,
    pub hardware_decks: bool,
    pub registry_url: String,
    pub locale: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            name: hostname(),
            admin_port: DEFAULT_ADMIN_PORT,
            lan_port: DEFAULT_LAN_PORT,
            lan_bind: IpAddr::from([0, 0, 0, 0]),
            lan_enabled: true,
            api_on_lan: false,
            mdns: true,
            follow_focus: true,
            hardware_decks: true,
            registry_url: DEFAULT_REGISTRY.into(),
            locale: "en".into(),
        }
    }
}

impl Settings {
    pub fn load(paths: &Paths) -> Result<Self> {
        let mut s: Settings = store::read_json(&paths.file("settings.json"))?.unwrap_or_default();
        s.name = clean_name(&s.name, 40).unwrap_or_else(hostname);
        Ok(s)
    }

    pub fn save(&self, paths: &Paths) -> Result<()> {
        store::write_json(&paths.file("settings.json"), self)
    }
}

/// Long-lived values that identify this companion. Private to the current user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Identity {
    /// Companion id, part of every handshake transcript.
    pub cid: String,
    /// Token the desktop app and `phonedeck open` use to sign in to the local editor.
    pub admin_token: String,
}

impl Identity {
    pub fn load_or_create(paths: &Paths) -> Result<Self> {
        let p = paths.file("identity.json");
        if let Some(id) = store::read_json::<Identity>(&p)? {
            return Ok(id);
        }
        let id = Identity { cid: ids::random_b64(16), admin_token: ids::random_b64(32) };
        store::write_json_private(&p, &id)?;
        Ok(id)
    }

    pub fn rotate_admin_token(&mut self, paths: &Paths) -> Result<()> {
        self.admin_token = ids::random_b64(32);
        store::write_json_private(&paths.file("identity.json"), self)
    }
}

pub fn hostname() -> String {
    let raw =
        std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")).ok().or_else(|| std::fs::read_to_string("/etc/hostname").ok()).unwrap_or_else(|| "PhoneDeck".into());
    clean_name(&raw, 40).unwrap_or_else(|| "PhoneDeck".into())
}

/// Trims, drops control characters and caps length. None when nothing usable is left.
pub fn clean_name(s: &str, max: usize) -> Option<String> {
    let s: String = s.chars().filter(|c| !c.is_control()).collect::<String>().trim().chars().take(max).collect();
    if s.is_empty() { None } else { Some(s) }
}

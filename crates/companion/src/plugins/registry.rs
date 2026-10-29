//! Plugin and layout registry. The index is a JSON file in a public Git repository.
//! Packages are checked against the hash and signing key listed in the index.

use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::package::{MAX_TOTAL, Package};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginEntry {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    #[serde(default)]
    pub description: String,
    pub url: String,
    pub sha256: String,
    /// Base64url ed25519 key that must have signed the package.
    pub signer: String,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutEntry {
    pub id: String,
    pub name: String,
    pub author: String,
    #[serde(default)]
    pub description: String,
    pub url: String,
    pub sha256: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Index {
    pub version: u32,
    #[serde(default)]
    pub plugins: Vec<PluginEntry>,
    #[serde(default)]
    pub layouts: Vec<LayoutEntry>,
}

pub struct Registry {
    cache: Mutex<Option<(Instant, String, Index)>>,
}

fn https(url: &str) -> Result<url::Url> {
    let u = url::Url::parse(url).context("invalid registry URL")?;
    if u.scheme() != "https" {
        bail!("registry links must use https");
    }
    Ok(u)
}

async fn fetch(http: &reqwest::Client, url: &str, limit: usize) -> Result<Vec<u8>> {
    let mut resp = http.get(https(url)?).timeout(Duration::from_secs(60)).send().await?.error_for_status()?;
    let mut out = Vec::new();
    while let Some(chunk) = resp.chunk().await? {
        out.extend_from_slice(&chunk);
        if out.len() > limit {
            bail!("download is larger than expected");
        }
    }
    Ok(out)
}

fn sha_hex(b: &[u8]) -> String {
    Sha256::digest(b).iter().map(|x| format!("{x:02x}")).collect()
}

impl Default for Registry {
    fn default() -> Self {
        Self { cache: Mutex::new(None) }
    }
}

impl Registry {
    pub async fn index(&self, http: &reqwest::Client, url: &str) -> Result<Index> {
        if let Some((at, u, idx)) = self.cache.lock().as_ref()
            && u == url
            && at.elapsed() < Duration::from_secs(600)
        {
            return Ok(idx.clone());
        }
        let raw = fetch(http, url, 5 * 1024 * 1024).await?;
        let idx: Index = serde_json::from_slice(&raw).context("registry index is invalid")?;
        if idx.version != 1 {
            bail!("unsupported registry version");
        }
        *self.cache.lock() = Some((Instant::now(), url.to_string(), idx.clone()));
        Ok(idx)
    }

    pub async fn download_plugin(&self, http: &reqwest::Client, entry: &PluginEntry) -> Result<Package> {
        let bytes = fetch(http, &entry.url, MAX_TOTAL as usize).await?;
        if sha_hex(&bytes) != entry.sha256.to_ascii_lowercase() {
            bail!("download does not match the registry hash");
        }
        let pkg = Package::from_zip(&bytes)?;
        if pkg.signer.as_deref() != Some(entry.signer.as_str()) {
            bail!("package is not signed by the key listed in the registry");
        }
        if pkg.manifest.id != entry.id || pkg.manifest.version != entry.version {
            bail!("package does not match its registry entry");
        }
        Ok(pkg)
    }

    pub async fn download_layout(&self, http: &reqwest::Client, entry: &LayoutEntry) -> Result<String> {
        let bytes = fetch(http, &entry.url, 512 * 1024).await?;
        if sha_hex(&bytes) != entry.sha256.to_ascii_lowercase() {
            bail!("download does not match the registry hash");
        }
        Ok(String::from_utf8(bytes).context("layout is not text")?.trim().to_string())
    }
}

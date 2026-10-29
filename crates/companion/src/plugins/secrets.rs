//! Plugin secrets (passwords, tokens). Stored in the OS keychain when one is available,
//! otherwise in a file only the current user can read.

use std::collections::BTreeMap;
use std::path::PathBuf;

use parking_lot::Mutex;

use crate::store;

const SERVICE: &str = "PhoneDeck";

pub struct Secrets {
    keychain: bool,
    file: PathBuf,
    cache: Mutex<Option<BTreeMap<String, String>>>,
}

impl Secrets {
    pub fn open(file: PathBuf) -> Self {
        let keychain = std::env::var_os("PHONEDECK_NO_KEYCHAIN").is_none() && probe();
        if !keychain {
            tracing::info!("no system keychain found; plugin secrets are kept in {}", file.display());
        }
        Self { keychain, file, cache: Mutex::new(None) }
    }

    fn account(plugin: &str, field: &str) -> String {
        format!("{plugin}:{field}")
    }

    pub fn get(&self, plugin: &str, field: &str) -> Option<String> {
        let acct = Self::account(plugin, field);
        if self.keychain {
            return keyring::Entry::new(SERVICE, &acct).ok()?.get_password().ok();
        }
        self.load().get(&acct).cloned()
    }

    pub fn set(&self, plugin: &str, field: &str, value: Option<&str>) -> anyhow::Result<()> {
        let acct = Self::account(plugin, field);
        if self.keychain {
            let e = keyring::Entry::new(SERVICE, &acct)?;
            match value {
                Some(v) => e.set_password(v)?,
                None => {
                    let _ = e.delete_credential();
                }
            }
            return Ok(());
        }
        let mut map = self.load();
        match value {
            Some(v) => map.insert(acct, v.to_string()),
            None => map.remove(&acct),
        };
        store::write_json_private(&self.file, &map)?;
        *self.cache.lock() = Some(map);
        Ok(())
    }

    fn load(&self) -> BTreeMap<String, String> {
        let mut c = self.cache.lock();
        if c.is_none() {
            *c = Some(store::read_json(&self.file).ok().flatten().unwrap_or_default());
        }
        c.clone().unwrap_or_default()
    }
}

fn probe() -> bool {
    let Ok(e) = keyring::Entry::new(SERVICE, "probe") else { return false };
    match e.get_password() {
        Ok(_) => true,
        Err(keyring::Error::NoEntry) => true,
        Err(_) => false,
    }
}

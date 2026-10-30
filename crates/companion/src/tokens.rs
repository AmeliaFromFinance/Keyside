//! Tokens for the inbound HTTP API. Only a hash of each token is stored.

use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

use crate::{config::clean_name, devices::now, ids, store};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApiScope {
    /// Press keys and run actions.
    Press,
    /// Read and write user.* variables.
    Variables,
    /// Change how keys look and switch pages.
    Display,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Token {
    pub id: String,
    pub name: String,
    pub scopes: Vec<ApiScope>,
    hash: String,
    pub created: u64,
    #[serde(default)]
    pub last_used: u64,
}

pub struct Tokens {
    path: PathBuf,
    list: Vec<Token>,
}

fn hash(t: &str) -> String {
    Sha256::digest(t.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

impl Tokens {
    pub fn load(path: PathBuf) -> Result<Self> {
        Ok(Self { list: store::read_json_or_reset(&path)?.unwrap_or_default(), path })
    }

    fn save(&self) -> Result<()> {
        store::write_json_private(&self.path, &self.list)
    }

    pub fn list(&self) -> Vec<Token> {
        self.list.clone()
    }

    /// Returns the new token. It is shown once and can't be read back.
    pub fn create(&mut self, name: &str, scopes: Vec<ApiScope>) -> Result<(Token, String)> {
        anyhow::ensure!(self.list.len() < 50, "too many tokens");
        let secret = format!("pd_{}", ids::random_b64(32));
        let t = Token { id: ids::short(), name: clean_name(name, 40).unwrap_or_else(|| "Token".into()), scopes, hash: hash(&secret), created: now(), last_used: 0 };
        self.list.push(t.clone());
        self.save()?;
        Ok((t, secret))
    }

    pub fn revoke(&mut self, id: &str) -> Result<bool> {
        let before = self.list.len();
        self.list.retain(|t| t.id != id);
        self.save()?;
        Ok(before != self.list.len())
    }

    pub fn check(&mut self, presented: &str) -> Option<Token> {
        if !presented.starts_with("pd_") || presented.len() > 100 {
            return None;
        }
        let h = hash(presented);
        let t = self.list.iter_mut().find(|t| bool::from(t.hash.as_bytes().ct_eq(h.as_bytes())))?;
        if now() > t.last_used + 60 {
            t.last_used = now();
            let out = t.clone();
            let _ = self.save();
            return Some(out);
        }
        Some(t.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_check_and_revoke() {
        let t = tempfile::tempdir().unwrap();
        let mut tk = Tokens::load(t.path().join("tokens.json")).unwrap();
        let (tok, secret) = tk.create("Streamer.bot", vec![ApiScope::Variables]).unwrap();
        assert!(tk.check(&secret).is_some());
        assert!(tk.check("pd_wrong").is_none());
        assert!(!std::fs::read_to_string(t.path().join("tokens.json")).unwrap().contains(&secret));
        tk.revoke(&tok.id).unwrap();
        assert!(tk.check(&secret).is_none());
    }
}

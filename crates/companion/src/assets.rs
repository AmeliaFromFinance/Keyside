//! Uploaded key images and sounds. Files are named by content hash and checked by their
//! first bytes, so a renamed file can't pass as another type. Images reach clients only as
//! data URLs inside the encrypted channel; there is no HTTP route that serves them.

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Result, bail};
use base64::Engine as _;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::store;

pub const MAX_IMAGE: usize = 512 * 1024;
pub const MAX_SOUND: usize = 10 * 1024 * 1024;
const MAX_FILES: usize = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Image,
    Sound,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Meta {
    pub id: String,
    pub kind: Kind,
    pub mime: String,
    pub name: String,
    pub size: usize,
}

pub struct Assets {
    dir: PathBuf,
    index: Mutex<BTreeMap<String, Meta>>,
}

pub fn sniff(b: &[u8]) -> Option<(Kind, &'static str, &'static str)> {
    let starts = |p: &[u8]| b.starts_with(p);
    if starts(b"\x89PNG\r\n\x1a\n") {
        return Some((Kind::Image, "image/png", "png"));
    }
    if starts(&[0xFF, 0xD8, 0xFF]) {
        return Some((Kind::Image, "image/jpeg", "jpg"));
    }
    if starts(b"GIF87a") || starts(b"GIF89a") {
        return Some((Kind::Image, "image/gif", "gif"));
    }
    if b.len() > 12 && starts(b"RIFF") && &b[8..12] == b"WEBP" {
        return Some((Kind::Image, "image/webp", "webp"));
    }
    if b.len() > 12 && starts(b"RIFF") && &b[8..12] == b"WAVE" {
        return Some((Kind::Sound, "audio/wav", "wav"));
    }
    if starts(b"OggS") {
        return Some((Kind::Sound, "audio/ogg", "ogg"));
    }
    if starts(b"fLaC") {
        return Some((Kind::Sound, "audio/flac", "flac"));
    }
    if starts(b"ID3") || (b.len() > 2 && b[0] == 0xFF && b[1] & 0xE0 == 0xE0) {
        return Some((Kind::Sound, "audio/mpeg", "mp3"));
    }
    let head = std::str::from_utf8(&b[..b.len().min(1024)]).ok()?.trim_start_matches('\u{feff}').trim_start();
    if head.starts_with("<svg") || (head.starts_with("<?xml") && head.contains("<svg")) {
        return Some((Kind::Image, "image/svg+xml", "svg"));
    }
    None
}

impl Assets {
    pub fn open(dir: PathBuf) -> Result<Self> {
        store::private_dir(&dir)?;
        let index: BTreeMap<String, Meta> = store::read_json(&dir.join("index.json"))?.unwrap_or_default();
        Ok(Self { dir, index: Mutex::new(index) })
    }

    pub fn put(&self, name: &str, bytes: &[u8]) -> Result<Meta> {
        let Some((kind, mime, ext)) = sniff(bytes) else { bail!("unsupported file type") };
        let limit = if kind == Kind::Image { MAX_IMAGE } else { MAX_SOUND };
        if bytes.len() > limit {
            bail!("file is larger than {} KB", limit / 1024);
        }
        let id = Sha256::digest(bytes).iter().take(16).map(|b| format!("{b:02x}")).collect::<String>();
        let mut idx = self.index.lock();
        if let Some(m) = idx.get(&id) {
            return Ok(m.clone());
        }
        if idx.len() >= MAX_FILES {
            bail!("asset library is full");
        }
        store::write_atomic(&self.dir.join(format!("{id}.{ext}")), bytes)?;
        let meta = Meta { id: id.clone(), kind, mime: mime.into(), name: crate::config::clean_name(name, 80).unwrap_or_else(|| id.clone()), size: bytes.len() };
        idx.insert(id, meta.clone());
        store::write_json(&self.dir.join("index.json"), &*idx)?;
        Ok(meta)
    }

    fn path(&self, m: &Meta) -> PathBuf {
        let ext = match m.mime.as_str() {
            "image/png" => "png",
            "image/jpeg" => "jpg",
            "image/gif" => "gif",
            "image/webp" => "webp",
            "image/svg+xml" => "svg",
            "audio/wav" => "wav",
            "audio/ogg" => "ogg",
            "audio/flac" => "flac",
            _ => "mp3",
        };
        self.dir.join(format!("{}.{ext}", m.id))
    }

    pub fn list(&self) -> Vec<Meta> {
        self.index.lock().values().cloned().collect()
    }

    pub fn data_url(&self, id: &str) -> Option<String> {
        let m = self.index.lock().get(id).cloned().filter(|m| m.kind == Kind::Image)?;
        let bytes = std::fs::read(self.path(&m)).ok()?;
        Some(format!("data:{};base64,{}", m.mime, base64::engine::general_purpose::STANDARD.encode(bytes)))
    }

    pub fn sound_path(&self, id: &str) -> Option<PathBuf> {
        let m = self.index.lock().get(id).cloned().filter(|m| m.kind == Kind::Sound)?;
        Some(self.path(&m))
    }

    pub fn delete(&self, id: &str) -> Result<bool> {
        let mut idx = self.index.lock();
        let Some(m) = idx.remove(id) else { return Ok(false) };
        let _ = std::fs::remove_file(self.path(&m));
        store::write_json(&self.dir.join("index.json"), &*idx)?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniffs_by_content_not_name() {
        assert_eq!(sniff(b"\x89PNG\r\n\x1a\nxxxx").unwrap().1, "image/png");
        assert_eq!(sniff(b"  <svg xmlns='http://www.w3.org/2000/svg'/>").unwrap().1, "image/svg+xml");
        assert!(sniff(b"<html><script>").is_none());
        assert!(sniff(b"MZ\x90\x00").is_none());
    }

    #[test]
    fn dedupes_and_limits() {
        let t = tempfile::tempdir().unwrap();
        let a = Assets::open(t.path().to_path_buf()).unwrap();
        let png = b"\x89PNG\r\n\x1a\n0000";
        let m1 = a.put("x.png", png).unwrap();
        let m2 = a.put("y.png", png).unwrap();
        assert_eq!(m1.id, m2.id);
        assert!(a.data_url(&m1.id).unwrap().starts_with("data:image/png;base64,"));
        let big = [b"\x89PNG\r\n\x1a\n".as_slice(), &vec![0u8; MAX_IMAGE]].concat();
        assert!(a.put("big.png", &big).is_err());
        assert!(a.sound_path(&m1.id).is_none());
    }
}

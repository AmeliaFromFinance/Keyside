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
const MAX_FILES: usize = 8000;
const MAX_PACKS: usize = 30;

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
    /// Set for icons that came with an icon pack. Those are listed by pack, not with the
    /// images people uploaded themselves.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pack: Option<String>,
}

/// An imported icon pack: a name and the assets it brought.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pack {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    pub icons: Vec<PackIcon>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackIcon {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

pub struct Assets {
    dir: PathBuf,
    index: Mutex<BTreeMap<String, Meta>>,
    packs: Mutex<Vec<Pack>>,
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
        let index: BTreeMap<String, Meta> = store::read_json_or_reset(&dir.join("index.json"))?.unwrap_or_default();
        let packs: Vec<Pack> = store::read_json_or_reset(&dir.join("packs.json"))?.unwrap_or_default();
        Ok(Self { dir, index: Mutex::new(index), packs: Mutex::new(packs) })
    }

    /// Reads the index again, after a restore.
    pub fn reload(&self) {
        *self.index.lock() = store::read_json_or_reset(&self.dir.join("index.json")).ok().flatten().unwrap_or_default();
    }

    pub fn put(&self, name: &str, bytes: &[u8]) -> Result<Meta> {
        let mut idx = self.index.lock();
        let meta = self.insert(&mut idx, name, bytes, None)?;
        store::write_json(&self.dir.join("index.json"), &*idx)?;
        Ok(meta)
    }

    /// Adds a file to the index without saving the index. An upload of a file that came with
    /// a pack makes it an image of its own, so deleting the pack keeps it.
    fn insert(&self, idx: &mut BTreeMap<String, Meta>, name: &str, bytes: &[u8], pack: Option<&str>) -> Result<Meta> {
        let Some((kind, mime, ext)) = sniff(bytes) else { bail!("unsupported file type") };
        let limit = if kind == Kind::Image { MAX_IMAGE } else { MAX_SOUND };
        if bytes.len() > limit {
            bail!("file is larger than {} KB", limit / 1024);
        }
        let id = Sha256::digest(bytes).iter().take(16).map(|b| format!("{b:02x}")).collect::<String>();
        if let Some(m) = idx.get_mut(&id) {
            if pack.is_none() {
                m.pack = None;
            }
            return Ok(m.clone());
        }
        if idx.len() >= MAX_FILES {
            bail!("asset library is full");
        }
        store::write_atomic(&self.dir.join(format!("{id}.{ext}")), bytes)?;
        let meta = Meta {
            id: id.clone(),
            kind,
            mime: mime.into(),
            name: crate::config::clean_name(name, 80).unwrap_or_else(|| id.clone()),
            size: bytes.len(),
            pack: pack.map(str::to_string),
        };
        idx.insert(id, meta.clone());
        Ok(meta)
    }

    pub fn packs(&self) -> Vec<Pack> {
        self.packs.lock().clone()
    }

    /// Stores the icons of a pack. A pack with the same name and author replaces the old one.
    /// Icons that fail the checks are skipped; returns the pack and how many were skipped.
    pub fn add_pack(&self, parsed: crate::iconpack::Parsed, in_use: &std::collections::HashSet<String>) -> Result<(Pack, usize)> {
        let old = self.packs.lock().iter().find(|p| p.name == parsed.name && p.author == parsed.author).map(|p| p.id.clone());
        if let Some(id) = &old {
            self.delete_pack(id, in_use)?;
        } else if self.packs.lock().len() >= MAX_PACKS {
            bail!("there can be at most {MAX_PACKS} icon packs, delete one first");
        }
        let id = old.unwrap_or_else(|| crate::ids::random_b64(9));
        let mut idx = self.index.lock();
        let mut icons: Vec<PackIcon> = Vec::new();
        let mut skipped = 0;
        for icon in parsed.icons {
            match self.insert(&mut idx, &icon.name, &icon.bytes, Some(&id)) {
                Ok(m) if m.kind == Kind::Image && !icons.iter().any(|i| i.id == m.id) => icons.push(PackIcon { id: m.id, name: icon.name, tags: icon.tags }),
                Ok(_) => {}
                Err(_) => skipped += 1,
            }
        }
        store::write_json(&self.dir.join("index.json"), &*idx)?;
        drop(idx);
        if icons.is_empty() {
            bail!("the pack has no icons Keyside can use");
        }
        let pack = Pack { id, name: parsed.name, author: parsed.author, icons };
        let mut packs = self.packs.lock();
        packs.push(pack.clone());
        store::write_json(&self.dir.join("packs.json"), &*packs)?;
        Ok((pack, skipped))
    }

    /// Removes a pack. Its icons go too, unless a key or template still shows them or another
    /// pack has them: those stay as images of their own.
    pub fn delete_pack(&self, id: &str, in_use: &std::collections::HashSet<String>) -> Result<bool> {
        let mut packs = self.packs.lock();
        let Some(i) = packs.iter().position(|p| p.id == id) else { return Ok(false) };
        let pack = packs.remove(i);
        let mut idx = self.index.lock();
        for icon in &pack.icons {
            let Some(m) = idx.get(&icon.id).cloned() else { continue };
            if m.pack.as_deref() != Some(id) {
                continue;
            }
            let other = packs.iter().find(|p| p.icons.iter().any(|x| x.id == icon.id)).map(|p| p.id.clone());
            if other.is_none() && !in_use.contains(&icon.id) {
                let _ = std::fs::remove_file(self.path(&m));
                idx.remove(&icon.id);
            } else if let Some(e) = idx.get_mut(&icon.id) {
                e.pack = other;
            }
        }
        store::write_json(&self.dir.join("index.json"), &*idx)?;
        store::write_json(&self.dir.join("packs.json"), &*packs)?;
        Ok(true)
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
    fn packs_keep_icons_that_are_still_used() {
        use crate::iconpack::{Icon, Parsed};
        let t = tempfile::tempdir().unwrap();
        let a = Assets::open(t.path().to_path_buf()).unwrap();
        let icon = |n: &str, b: &[u8]| Icon { name: n.into(), tags: vec![], bytes: b.to_vec() };
        let parsed = || Parsed {
            name: "Pack".into(),
            author: Some("Me".into()),
            icons: vec![icon("a", b"\x89PNG\r\n\x1a\naaaa"), icon("b", b"\x89PNG\r\n\x1a\nbbbb"), icon("c", b"\x89PNG\r\n\x1a\ncccc"), icon("bad", b"MZ")],
        };
        let none = std::collections::HashSet::new();
        let (p, skipped) = a.add_pack(parsed(), &none).unwrap();
        assert_eq!((p.icons.len(), skipped), (3, 1));
        assert!(a.list().iter().all(|m| m.pack.as_deref() == Some(p.id.as_str())));
        // Uploading one of them by hand makes it an image of its own.
        let own = a.put("mine.png", b"\x89PNG\r\n\x1a\ncccc").unwrap();
        assert!(own.pack.is_none());
        // Importing again replaces the pack instead of adding a second one.
        let (again, _) = a.add_pack(parsed(), &none).unwrap();
        assert_eq!(again.id, p.id);
        assert_eq!(a.packs().len(), 1);
        let used: std::collections::HashSet<String> = [again.icons[0].id.clone()].into();
        assert!(a.delete_pack(&p.id, &used).unwrap());
        assert!(a.packs().is_empty());
        let left: Vec<String> = a.list().into_iter().map(|m| m.id).collect();
        assert_eq!(left.len(), 2, "the used icon and the uploaded one stay");
        assert!(left.contains(&again.icons[0].id) && left.contains(&own.id));
        assert!(a.list().iter().all(|m| m.pack.is_none()));
        assert!(!a.delete_pack(&p.id, &used).unwrap());
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

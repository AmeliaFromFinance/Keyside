//! Reads icon packs: Elgato Stream Deck packs (.streamDeckIconPack) and plain zips of images.
//!
//! A Stream Deck pack is a zip with one "<id>.sdIconPack" folder holding manifest.json (Name,
//! Author, Version, Icon), icons.json (a list of { path, name, tags }, paths inside icons/)
//! and the icons folder. A plain zip just holds image files, in folders or not.

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::assets::{Kind, MAX_IMAGE, sniff};
use crate::plugins::package::read_zip;

pub const MAX_ZIP: u64 = 50 * 1024 * 1024;
const MAX_ICONS: usize = 1500;
/// More entries than icons are allowed: previews, licence files and folders come along.
const MAX_ENTRIES: usize = MAX_ICONS * 2 + 50;
const MAX_UNPACKED: u64 = 200 * 1024 * 1024;

pub struct Icon {
    pub name: String,
    pub tags: Vec<String>,
    pub bytes: Vec<u8>,
}

pub struct Parsed {
    pub name: String,
    pub author: Option<String>,
    pub icons: Vec<Icon>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Manifest {
    name: String,
    #[serde(default)]
    author: Option<String>,
}

#[derive(Deserialize)]
struct Entry {
    path: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
}

fn json<T: serde::de::DeserializeOwned>(bytes: &[u8], what: &str) -> Result<T> {
    let text = std::str::from_utf8(bytes).with_context(|| format!("{what} is not text"))?;
    serde_json::from_str(text.trim_start_matches('\u{feff}')).with_context(|| format!("{what} is invalid"))
}

fn stem(path: &str) -> String {
    let file = path.rsplit('/').next().unwrap_or(path);
    let base = file.rsplit_once('.').map_or(file, |(b, _)| b);
    base.replace(['_', '-'], " ").trim().to_string()
}

fn clean(s: &str, max: usize) -> Option<String> {
    crate::config::clean_name(s, max)
}

/// Unpacks and checks a pack. `file_name` names plain zips, which have no manifest.
pub fn parse(bytes: &[u8], file_name: &str) -> Result<Parsed> {
    if bytes.len() as u64 > MAX_ZIP {
        bail!("icon pack is larger than 50 MB");
    }
    let files: Vec<(String, Vec<u8>)> = read_zip(bytes, MAX_ENTRIES, MAX_IMAGE as u64 * 4, MAX_UNPACKED)?
        .into_iter()
        .filter(|(n, _)| !n.starts_with("__MACOSX/") && !n.rsplit('/').next().unwrap_or("").starts_with('.'))
        .collect();
    let find = |name: &str| files.iter().find(|(n, _)| n == name).map(|(_, b)| b.as_slice());
    // The shallowest manifest.json that has an icons.json beside it.
    let root = files
        .iter()
        .filter_map(|(n, _)| n.strip_suffix("manifest.json"))
        .filter(|dir| dir.is_empty() || dir.ends_with('/'))
        .filter(|dir| find(&format!("{dir}icons.json")).is_some())
        .min_by_key(|dir| dir.matches('/').count())
        .map(str::to_string);

    let mut icons = Vec::new();
    let (name, author) = if let Some(root) = root {
        let m: Manifest = json(find(&format!("{root}manifest.json")).unwrap_or_default(), "manifest.json")?;
        let list: Vec<Entry> = json(find(&format!("{root}icons.json")).unwrap_or_default(), "icons.json")?;
        if list.len() > MAX_ICONS {
            bail!("icon pack has more than {MAX_ICONS} icons");
        }
        for e in list {
            let path = format!("{root}icons/{}", e.path);
            let Some(bytes) = find(&path) else { continue };
            let tags = e.tags.iter().filter_map(|t| clean(t, 40)).take(20).collect();
            icons.push(Icon { name: e.name.as_deref().and_then(|n| clean(n, 80)).unwrap_or_else(|| stem(&e.path)), tags, bytes: bytes.to_vec() });
        }
        (clean(&m.name, 60).context("icon pack has no name")?, m.author.as_deref().and_then(|a| clean(a, 60)))
    } else {
        for (path, bytes) in &files {
            if sniff(bytes).is_some_and(|(k, _, _)| k == Kind::Image) {
                if icons.len() >= MAX_ICONS {
                    bail!("icon pack has more than {MAX_ICONS} icons");
                }
                // Folders become tags, so "arrows/left.png" turns up when searching for arrows.
                let tags = path.split('/').rev().skip(1).filter_map(|t| clean(t, 40)).collect();
                icons.push(Icon { name: clean(&stem(path), 80).unwrap_or_else(|| "Icon".into()), tags, bytes: bytes.clone() });
            }
        }
        let base = file_name.rsplit(['/', '\\']).next().unwrap_or(file_name);
        (clean(&stem(base), 60).unwrap_or_else(|| "Icons".into()), None)
    };
    // Only images, whatever the names say.
    icons.retain(|i| sniff(&i.bytes).is_some_and(|(k, _, _)| k == Kind::Image));
    if icons.is_empty() {
        bail!("no PNG, JPEG, GIF, WebP or SVG icons found");
    }
    Ok(Parsed { name, author, icons })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n0000";

    fn zip(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = std::io::Cursor::new(Vec::new());
        let mut z = zip::ZipWriter::new(&mut out);
        for (n, b) in files {
            z.start_file(*n, zip::write::SimpleFileOptions::default()).unwrap();
            z.write_all(b).unwrap();
        }
        z.finish().unwrap();
        out.into_inner()
    }

    #[test]
    fn reads_stream_deck_packs() {
        let manifest = br#"{"Name":"Neon Icons","Author":"Ana","Version":"1.0","Icon":"icon.svg"}"#;
        let list = br#"[{"path":"play.png","name":"Play","tags":["media"]},{"path":"stop.svg","name":"Stop"},{"path":"missing.png","name":"Gone"},{"path":"evil.png","name":"Not an image"}]"#;
        let svg = b"<svg xmlns='http://www.w3.org/2000/svg'/>";
        let p = parse(
            &zip(&[
                ("com.ana.neon.sdIconPack/manifest.json", manifest),
                ("com.ana.neon.sdIconPack/icons.json", list),
                ("com.ana.neon.sdIconPack/icons/play.png", PNG),
                ("com.ana.neon.sdIconPack/icons/stop.svg", svg),
                ("com.ana.neon.sdIconPack/icons/evil.png", b"MZ\x90\x00"),
                ("com.ana.neon.sdIconPack/previews/1-preview.png", PNG),
            ]),
            "neon.streamDeckIconPack",
        )
        .unwrap();
        assert_eq!((p.name.as_str(), p.author.as_deref()), ("Neon Icons", Some("Ana")));
        assert_eq!(p.icons.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(), ["Play", "Stop"]);
        assert_eq!(p.icons[0].tags, ["media"]);
    }

    #[test]
    fn reads_plain_zips_of_images() {
        let p = parse(&zip(&[("arrows/left_arrow.png", PNG), ("readme.txt", b"hi"), ("__MACOSX/arrows/._left_arrow.png", PNG), ("run.exe", b"MZ")]), "C:\\Downloads\\My icons.zip").unwrap();
        assert_eq!(p.name, "My icons");
        assert_eq!(p.icons.len(), 1);
        assert_eq!((p.icons[0].name.as_str(), p.icons[0].tags.as_slice()), ("left arrow", ["arrows".to_string()].as_slice()));
    }

    #[test]
    fn rejects_unsafe_or_empty_zips() {
        assert!(parse(&zip(&[("../escape.png", PNG)]), "x.zip").is_err());
        assert!(parse(&zip(&[("/abs.png", PNG)]), "x.zip").is_err());
        assert!(parse(&zip(&[("notes.txt", b"hello")]), "x.zip").is_err());
        assert!(parse(b"not a zip", "x.zip").is_err());
        let big = [PNG, &vec![0u8; MAX_IMAGE * 4]].concat();
        assert!(parse(&zip(&[("big.png", &big)]), "x.zip").is_err());
    }
}

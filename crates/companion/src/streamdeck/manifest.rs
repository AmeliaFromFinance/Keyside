//! Stream Deck plugin manifests (.sdPlugin/manifest.json) and the packages they come in.

use std::io::Read;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::plugins::package::{MAX_FILE, MAX_FILES, MAX_TOTAL, safe_path};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SdManifest {
    pub name: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(rename = "UUID", default)]
    pub uuid: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub code_path: Option<String>,
    #[serde(default)]
    pub code_path_win: Option<String>,
    #[serde(default)]
    pub code_path_mac: Option<String>,
    #[serde(default)]
    pub property_inspector_path: Option<String>,
    #[serde(default)]
    pub actions: Vec<SdAction>,
    #[serde(rename = "Nodejs", default)]
    pub nodejs: Option<Value>,
    #[serde(rename = "URL", default)]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SdAction {
    #[serde(rename = "UUID")]
    pub uuid: String,
    pub name: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub states: Vec<SdState>,
    #[serde(default)]
    pub property_inspector_path: Option<String>,
    #[serde(default)]
    pub tooltip: Option<String>,
    #[serde(default)]
    pub disable_automatic_states: bool,
    #[serde(default)]
    pub controllers: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SdState {
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
}

/// How the plugin's code runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Runtime {
    /// JavaScript, run by the sandboxed Deno with Node compatibility.
    Node,
    /// A program for this operating system. Runs with the person's full rights.
    Native,
    /// A web page plugin. These need Elgato's embedded browser and can't run here.
    Html,
}

pub fn valid_uuid(s: &str, max: usize) -> bool {
    (3..=max).contains(&s.len())
        && s.contains('.')
        && !s.starts_with('.')
        && !s.ends_with('.')
        && !s.contains("..")
        && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-' || b == b'_')
}

fn text(s: &str, max: usize) -> String {
    s.chars().filter(|c| !c.is_control()).take(max).collect()
}

impl SdManifest {
    /// The entry for this OS, if there is one.
    pub fn code_path(&self) -> Option<&str> {
        let os = if cfg!(windows) {
            self.code_path_win.as_deref()
        } else if cfg!(target_os = "macos") {
            self.code_path_mac.as_deref()
        } else {
            None
        };
        os.or(self.code_path.as_deref())
    }

    pub fn runtime(&self) -> Option<Runtime> {
        let p = self.code_path()?.to_ascii_lowercase();
        Some(if p.ends_with(".html") || p.ends_with(".htm") {
            Runtime::Html
        } else if self.nodejs.is_some() || p.ends_with(".js") || p.ends_with(".mjs") || p.ends_with(".cjs") {
            Runtime::Node
        } else {
            Runtime::Native
        })
    }

    pub fn action(&self, uuid: &str) -> Option<&SdAction> {
        self.actions.iter().find(|a| a.uuid == uuid)
    }

    fn check(&mut self, uuid: &str) -> Result<()> {
        if self.uuid.as_deref().is_some_and(|u| u != uuid) {
            bail!("manifest UUID doesn't match the plugin folder name");
        }
        self.name = text(&self.name, 60);
        self.author = text(&self.author, 60);
        self.version = text(&self.version, 20);
        self.description = text(&self.description, 300);
        if self.name.trim().is_empty() {
            bail!("plugin has no name");
        }
        let code = self.code_path().context("plugin has no code for this system")?;
        if !safe_path(code) {
            bail!("plugin code path is unsafe");
        }
        for p in [&self.property_inspector_path, &self.icon].into_iter().flatten() {
            if !safe_path(p) {
                bail!("plugin lists an unsafe path: {p}");
            }
        }
        if self.actions.is_empty() || self.actions.len() > 100 {
            bail!("plugin must have 1 to 100 actions");
        }
        let mut seen = std::collections::HashSet::new();
        for a in &mut self.actions {
            if !valid_uuid(&a.uuid, 90) || !seen.insert(a.uuid.clone()) {
                bail!("action UUID \"{}\" is invalid or repeated", a.uuid);
            }
            a.name = text(&a.name, 40);
            a.tooltip = a.tooltip.as_deref().map(|t| text(t, 200));
            a.states.truncate(8);
            for p in [&a.property_inspector_path, &a.icon].into_iter().flatten().chain(a.states.iter().filter_map(|s| s.image.as_ref())) {
                if !safe_path(p) {
                    bail!("action {} lists an unsafe path", a.uuid);
                }
            }
        }
        Ok(())
    }
}

/// An unpacked .streamDeckPlugin: files are relative to the .sdPlugin folder.
pub struct SdPackage {
    pub uuid: String,
    pub manifest: SdManifest,
    pub files: Vec<(String, Vec<u8>, bool)>,
}

/// Reads a .streamDeckPlugin (a zip holding one "<uuid>.sdPlugin" folder).
pub fn unpack(bytes: &[u8]) -> Result<SdPackage> {
    if bytes.len() as u64 > MAX_TOTAL {
        bail!("plugin is larger than 50 MB");
    }
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).context("not a Stream Deck plugin file")?;
    if zip.len() > MAX_FILES * 4 {
        bail!("plugin has too many files");
    }
    let mut root: Option<String> = None;
    let mut files = Vec::new();
    let mut total = 0u64;
    for i in 0..zip.len() {
        let mut f = zip.by_index(i)?;
        let name = f.name()?.to_string();
        if f.is_dir() || name.starts_with("__MACOSX/") || name.ends_with("/.DS_Store") {
            continue;
        }
        if f.is_symlink() || !safe_path(&name) {
            bail!("plugin contains an unsafe path: {name}");
        }
        let (top, rest) = name.split_once('/').context("plugin files must be inside a .sdPlugin folder")?;
        match &root {
            None => root = Some(top.to_string()),
            Some(r) if r != top => bail!("plugin has more than one top folder"),
            _ => {}
        }
        if f.size() > MAX_FILE {
            bail!("{name} is too large");
        }
        total += f.size();
        if total > MAX_TOTAL * 4 {
            bail!("plugin unpacks to more than 200 MB");
        }
        let declared = f.size();
        let mut buf = Vec::with_capacity(declared as usize);
        (&mut f).take(declared + 1).read_to_end(&mut buf)?;
        if buf.len() as u64 != declared {
            bail!("{name} does not match its declared size");
        }
        let exec = f.unix_mode().is_some_and(|m| m & 0o111 != 0);
        files.push((rest.to_string(), buf, exec));
    }
    let root = root.context("plugin is empty")?;
    let uuid = root.strip_suffix(".sdPlugin").context("plugin folder must end in .sdPlugin")?.to_string();
    if !valid_uuid(&uuid, 60) {
        bail!("plugin UUID \"{uuid}\" is not valid");
    }
    let raw = files.iter().find(|(n, _, _)| n == "manifest.json").context("plugin has no manifest.json")?;
    let text = String::from_utf8_lossy(&raw.1);
    let mut manifest: SdManifest = serde_json::from_str(text.trim_start_matches('\u{feff}')).context("manifest.json is invalid")?;
    manifest.check(&uuid)?;
    let code = manifest.code_path().unwrap_or_default().to_string();
    if !files.iter().any(|(n, _, _)| *n == code) {
        bail!("plugin code {code} is missing");
    }
    Ok(SdPackage { uuid, manifest, files })
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::io::Write;

    pub fn zip(files: &[(&str, &str)]) -> Vec<u8> {
        let mut out = std::io::Cursor::new(Vec::new());
        let mut z = zip::ZipWriter::new(&mut out);
        for (n, body) in files {
            z.start_file(*n, zip::write::SimpleFileOptions::default()).unwrap();
            z.write_all(body.as_bytes()).unwrap();
        }
        z.finish().unwrap();
        out.into_inner()
    }

    pub const MANIFEST: &str = r#"{"Name":"Counter","Author":"Me","Version":"1.0.0","CodePath":"bin/plugin.js","Nodejs":{"Version":"20"},
        "Actions":[{"UUID":"com.me.counter.inc","Name":"Count","States":[{"Image":"imgs/key"}],"PropertyInspectorPath":"pi.html"}]}"#;

    #[test]
    fn reads_a_node_plugin() {
        let p = unpack(&zip(&[("com.me.counter.sdPlugin/manifest.json", MANIFEST), ("com.me.counter.sdPlugin/bin/plugin.js", "//")])).unwrap();
        assert_eq!(p.uuid, "com.me.counter");
        assert_eq!(p.manifest.runtime(), Some(Runtime::Node));
        assert_eq!(p.files.len(), 2);
    }

    #[test]
    fn rejects_unsafe_packages() {
        let evil = MANIFEST.replace("bin/plugin.js", "../../evil.js");
        assert!(unpack(&zip(&[("com.me.x.sdPlugin/manifest.json", &evil)])).is_err());
        assert!(unpack(&zip(&[("com.me.x.sdPlugin/manifest.json", MANIFEST), ("other/file", "x"), ("com.me.x.sdPlugin/bin/plugin.js", "")])).is_err());
        assert!(unpack(&zip(&[("../com.me.x.sdPlugin/manifest.json", MANIFEST)])).is_err());
        assert!(unpack(&zip(&[("com.me.x.sdPlugin/manifest.json", MANIFEST)])).is_err(), "code file missing");
        assert!(unpack(&zip(&[("bad uuid.sdPlugin/manifest.json", MANIFEST)])).is_err());
    }
}

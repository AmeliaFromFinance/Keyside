//! Plugin packages (.pdp): a zip with manifest.json, the bundled entry file and assets.
//! Optional SIGNATURE file: an ed25519 signature over every file's hash.

use std::io::{Read, Write};
use std::path::Path;

use anyhow::{Context, Result, bail};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::manifest::{self, Manifest};
use crate::crypto::channel::{b64, unb64};

pub const MAX_FILES: usize = 500;
pub const MAX_TOTAL: u64 = 50 * 1024 * 1024;
pub const MAX_FILE: u64 = 20 * 1024 * 1024;
const SIG_FILE: &str = "SIGNATURE";
const SIG_CONTEXT: &[u8] = b"phonedeck-plugin-v1\n";

#[derive(Debug, Clone)]
pub struct Package {
    pub manifest: Manifest,
    pub files: Vec<(String, Vec<u8>)>,
    /// Base64url ed25519 public key that signed the package, when signed and valid.
    pub signer: Option<String>,
    pub sha256: String,
}

#[derive(Serialize, Deserialize)]
struct SigFile {
    key: String,
    sig: String,
}

/// Relative paths with plain names only: no "..", no roots, no drive letters, no backslashes.
pub fn safe_path(p: &str) -> bool {
    !p.is_empty()
        && p.len() <= 200
        && !p.starts_with('/')
        && !p.contains('\\')
        && !p.contains(':')
        && p.split('/').all(|c| !c.is_empty() && c != "." && c != ".." && !c.chars().any(char::is_control))
}

fn digest_list(files: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut list: Vec<(&str, String)> = files
        .iter()
        .filter(|(n, _)| n != SIG_FILE)
        .map(|(n, b)| (n.as_str(), Sha256::digest(b).iter().map(|x| format!("{x:02x}")).collect()))
        .collect();
    list.sort();
    let mut msg = SIG_CONTEXT.to_vec();
    for (n, h) in list {
        msg.extend_from_slice(n.as_bytes());
        msg.push(0);
        msg.extend_from_slice(h.as_bytes());
        msg.push(b'\n');
    }
    msg
}

impl Package {
    fn build(files: Vec<(String, Vec<u8>)>, sha256: String) -> Result<Self> {
        let mf = files.iter().find(|(n, _)| n == "manifest.json").context("package has no manifest.json")?;
        let manifest: Manifest = serde_json::from_slice(&mf.1).context("manifest.json is invalid")?;
        manifest::validate(&manifest).map_err(anyhow::Error::msg)?;
        if !files.iter().any(|(n, _)| *n == manifest.entry) {
            bail!("entry file {} is missing", manifest.entry);
        }
        let signer = match files.iter().find(|(n, _)| n == SIG_FILE) {
            None => None,
            Some((_, raw)) => {
                let s: SigFile = serde_json::from_slice(raw).context("SIGNATURE is invalid")?;
                let key: [u8; 32] = unb64(&s.key, Some(32)).ok().and_then(|k| k.try_into().ok()).context("bad signing key")?;
                let sig = Signature::from_slice(&unb64(&s.sig, Some(64)).ok().context("bad signature")?).ok().context("bad signature")?;
                VerifyingKey::from_bytes(&key)
                    .ok()
                    .context("bad signing key")?
                    .verify_strict(&digest_list(&files), &sig)
                    .ok()
                    .context("signature does not match the files")?;
                Some(s.key)
            }
        };
        Ok(Self { manifest, files, signer, sha256 })
    }

    pub fn from_zip(bytes: &[u8]) -> Result<Self> {
        if bytes.len() as u64 > MAX_TOTAL {
            bail!("package is larger than 50 MB");
        }
        let sha256 = Sha256::digest(bytes).iter().map(|x| format!("{x:02x}")).collect();
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).context("not a zip file")?;
        if zip.len() > MAX_FILES {
            bail!("package has more than {MAX_FILES} files");
        }
        let mut total = 0u64;
        let mut files = Vec::new();
        for i in 0..zip.len() {
            let mut f = zip.by_index(i)?;
            let name = f.name()?.to_string();
            if f.is_dir() {
                continue;
            }
            if f.is_symlink() || !safe_path(&name) {
                bail!("package contains an unsafe path: {name}");
            }
            if f.size() > MAX_FILE {
                bail!("{name} is too large");
            }
            total += f.size();
            if total > MAX_TOTAL {
                bail!("package unpacks to more than 50 MB");
            }
            let declared = f.size();
            let mut buf = Vec::with_capacity(declared as usize);
            // Read one byte past the declared size to catch lying headers.
            (&mut f).take(declared + 1).read_to_end(&mut buf)?;
            if buf.len() as u64 != declared {
                bail!("{name} does not match its declared size");
            }
            if files.iter().any(|(n, _)| *n == name) {
                bail!("package lists {name} twice");
            }
            files.push((name, buf));
        }
        Self::build(files, sha256)
    }

    /// Reads an unpacked plugin folder. Used for bundled plugins and `phonedeck plugin dev`.
    pub fn from_dir(dir: &Path) -> Result<Self> {
        let mut files = Vec::new();
        let mut total = 0u64;
        walk(dir, dir, &mut files, &mut total)?;
        let mut h = Sha256::new();
        for (n, b) in &files {
            h.update(n.as_bytes());
            h.update(b);
        }
        Self::build(files, h.finalize().iter().map(|x| format!("{x:02x}")).collect())
    }

    /// Writes the files read-only, so a plugin can't change its own code.
    pub fn extract_to(&self, dir: &Path) -> Result<()> {
        std::fs::create_dir_all(dir)?;
        for (name, bytes) in &self.files {
            if !safe_path(name) {
                bail!("unsafe path {name}");
            }
            let path = dir.join(name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut f = std::fs::File::create(&path)?;
            f.write_all(bytes)?;
            let mut perm = f.metadata()?.permissions();
            perm.set_readonly(true);
            std::fs::set_permissions(&path, perm)?;
        }
        Ok(())
    }

    pub fn file(&self, name: &str) -> Option<&[u8]> {
        self.files.iter().find(|(n, _)| n == name).map(|(_, b)| b.as_slice())
    }
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>, total: &mut u64) -> Result<()> {
    for e in std::fs::read_dir(dir)? {
        let e = e?;
        let ft = e.file_type()?;
        let p = e.path();
        if ft.is_symlink() {
            bail!("symlinks are not allowed in plugins: {}", p.display());
        }
        let name = e.file_name().to_string_lossy().to_string();
        // Source, tests and tooling files stay out of the package; only the built plugin goes in.
        if name.starts_with('.') || ["node_modules", "src", "test", "package.json", "package-lock.json", "pnpm-lock.yaml", "tsconfig.json"].contains(&name.as_str()) || name.ends_with(".pdp") {
            continue;
        }
        if ft.is_dir() {
            walk(root, &p, out, total)?;
        } else {
            let rel = p.strip_prefix(root)?.to_string_lossy().replace('\\', "/");
            let bytes = std::fs::read(&p)?;
            *total += bytes.len() as u64;
            if out.len() >= MAX_FILES || *total > MAX_TOTAL || !safe_path(&rel) {
                bail!("plugin folder is too large or has unsafe names");
            }
            out.push((rel, bytes));
        }
    }
    Ok(())
}

/// Packs a plugin folder into a .pdp, signing it when a key is given.
pub fn pack(dir: &Path, key: Option<&SigningKey>) -> Result<Vec<u8>> {
    let pkg = Package::from_dir(dir)?;
    let mut files: Vec<(String, Vec<u8>)> = pkg.files.into_iter().filter(|(n, _)| n != SIG_FILE).collect();
    files.sort_by(|a, b| a.0.cmp(&b.0));
    if let Some(k) = key {
        let sig = k.sign(&digest_list(&files));
        let s = SigFile { key: b64(&k.verifying_key().to_bytes()), sig: b64(&sig.to_bytes()) };
        files.push((SIG_FILE.into(), serde_json::to_vec_pretty(&s)?));
    }
    let mut out = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut out);
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (n, b) in &files {
            z.start_file(n.as_str(), opts)?;
            z.write_all(b)?;
        }
        z.finish()?;
    }
    Ok(out.into_inner())
}

pub fn new_signing_key() -> SigningKey {
    let mut seed = [0u8; 32];
    crate::ids::fill_random(&mut seed);
    SigningKey::from_bytes(&seed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_plugin(dir: &Path) {
        std::fs::write(
            dir.join("manifest.json"),
            r#"{"id":"dev.example.pkg","name":"Pkg","version":"1.0.0","author":"Me","prefix":"pkg","host":"^1.0","entry":"main.js","actions":[]}"#,
        )
        .unwrap();
        std::fs::write(dir.join("main.js"), "console.log(1)").unwrap();
    }

    #[test]
    fn signed_round_trip_and_tamper() {
        let t = tempfile::tempdir().unwrap();
        write_plugin(t.path());
        let key = new_signing_key();
        let zip = pack(t.path(), Some(&key)).unwrap();
        let pkg = Package::from_zip(&zip).unwrap();
        assert_eq!(pkg.signer.as_deref(), Some(b64(&key.verifying_key().to_bytes()).as_str()));

        std::fs::write(t.path().join("main.js"), "console.log(2)").unwrap();
        let mut files = Package::from_dir(t.path()).unwrap().files;
        let sig = Package::from_zip(&zip).unwrap().files.into_iter().find(|(n, _)| n == SIG_FILE).unwrap();
        files.push(sig);
        assert!(Package::build(files, String::new()).is_err(), "changed file must break the signature");
    }

    #[test]
    fn rejects_traversal() {
        assert!(!safe_path("../x"));
        assert!(!safe_path("a/../../x"));
        assert!(!safe_path("/etc/passwd"));
        assert!(!safe_path("C:/x"));
        assert!(!safe_path("a\\b"));
        assert!(safe_path("icons/key.png"));

        let mut out = std::io::Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut out);
            z.start_file("../evil.js", zip::write::SimpleFileOptions::default()).unwrap();
            z.write_all(b"x").unwrap();
            z.finish().unwrap();
        }
        assert!(Package::from_zip(&out.into_inner()).is_err());
    }
}

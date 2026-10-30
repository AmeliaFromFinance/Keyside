//! Backup and restore of what Keyside keeps: layouts, settings, plugins with their settings,
//! paired devices, API tokens, images and sounds, Stream Deck plugins and known decks.
//! Passwords and keys (plugin secrets, device keys, PINs) only go in when the person asks,
//! encrypted with a passphrase (Argon2id and AES-256-GCM).

use std::collections::{BTreeMap, HashMap};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::app::App;
use crate::config::{Identity, Settings};
use crate::crypto::channel::{b64, unb64};
use crate::devices::{Device, Devices, PIN_SECRET};
use crate::model::LayoutFile;
use crate::state::{Change, all_keys};
use crate::store;
use crate::tokens::Tokens;

pub const FORMAT: &str = "keyside-backup";
const VERSION: u32 = 1;
/// Largest backup the editor may upload.
pub const MAX_SIZE: usize = 512 * 1024 * 1024;
const MAX_UNPACKED: u64 = 2 * 1024 * 1024 * 1024;
const MAX_ENTRIES: usize = 200_000;
const MANIFEST: &str = "backup.json";
const VAULT: &str = "secrets.bin";
const FILES: &str = "files/";
pub const MIN_PASSPHRASE: usize = 8;

/// Files in the data folder copied as they are. The layout, settings and variables come from
/// memory instead, so the backup matches what is running.
const PLAIN: &[&str] = &["tokens.json", "known-decks.json", "history.json", "plugins.json", "templates.json", "key-states.json"];
const DEVICES: &str = "devices.json";
/// Folders copied as they are. Caches fill up again on their own.
const DIRS: &[&str] = &["assets", "plugins", "plugin-data", "plugin-config", "streamdeck"];
const SKIP: &[&str] = &["streamdeck/cache"];

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Contents {
    pub layouts: usize,
    pub pages: usize,
    pub keys: usize,
    pub plugins: usize,
    pub plugin_settings: usize,
    pub streamdeck_plugins: usize,
    pub devices: usize,
    pub tokens: usize,
    pub assets: usize,
    pub history: usize,
    pub known_decks: usize,
    pub files: usize,
    pub bytes: u64,
}

/// How the passwords and keys are sealed. The sealed data is `secrets.bin`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sealed {
    pub kdf: String,
    pub m_cost: u32,
    pub t_cost: u32,
    pub p_cost: u32,
    pub salt: String,
    pub nonce: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub format: String,
    pub version: u32,
    /// When it was made (ms since 1970).
    pub created: u64,
    /// Keyside version that made it.
    pub keyside: String,
    /// Name of the PC it came from.
    pub name: String,
    pub contents: Contents,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secrets: Option<Sealed>,
}

/// Passwords and keys, sealed with the passphrase.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Vault {
    /// Plugin secrets and device PINs: (owner, field, value).
    secrets: Vec<(String, String, String)>,
    /// Device id -> (channel key, PIN hash).
    devices: BTreeMap<String, (String, Option<String>)>,
    /// Companion id. Paired devices check it on every connection.
    cid: String,
}

fn kdf(passphrase: &str, salt: &[u8], s: &Sealed) -> Result<[u8; 32]> {
    let params = argon2::Params::new(s.m_cost, s.t_cost, s.p_cost, Some(32)).map_err(|e| anyhow::anyhow!("{e}"))?;
    let mut key = [0u8; 32];
    argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params)
        .hash_password_into(passphrase.as_bytes(), salt, &mut key)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(key)
}

fn seal(passphrase: &str, plain: &[u8]) -> Result<(Sealed, Vec<u8>)> {
    let salt = crate::ids::random_bytes(16);
    let nonce: [u8; 12] = crate::ids::random_bytes(12).try_into().expect("12 bytes");
    let s = Sealed { kdf: "argon2id".into(), m_cost: 19 * 1024, t_cost: 2, p_cost: 1, salt: b64(&salt), nonce: b64(&nonce) };
    let key = kdf(passphrase, &salt, &s)?;
    let ct = Aes256Gcm::new_from_slice(&key)
        .expect("32 byte key")
        .encrypt(&Nonce::from(nonce), Payload { msg: plain, aad: FORMAT.as_bytes() })
        .map_err(|_| anyhow::anyhow!("could not encrypt"))?;
    Ok((s, ct))
}

fn open(passphrase: &str, s: &Sealed, ct: &[u8]) -> Result<Vec<u8>> {
    if s.kdf != "argon2id" || s.m_cost > 1024 * 1024 || s.t_cost > 20 || s.p_cost > 8 {
        bail!("unsupported encryption settings");
    }
    let salt = unb64(&s.salt, None).map_err(|_| anyhow::anyhow!("damaged backup"))?;
    let nonce: [u8; 12] = unb64(&s.nonce, Some(12)).ok().and_then(|n| n.try_into().ok()).context("damaged backup")?;
    let key = kdf(passphrase, &salt, s)?;
    Aes256Gcm::new_from_slice(&key).expect("32 byte key").decrypt(&Nonce::from(nonce), Payload { msg: ct, aad: FORMAT.as_bytes() }).map_err(|_| anyhow::anyhow!("wrong passphrase"))
}

/// Relative paths inside the backup: no way out of the data folder.
fn safe(p: &str) -> bool {
    !p.is_empty()
        && p.len() <= 1024
        && !p.starts_with('/')
        && !p.contains('\\')
        && !p.contains(':')
        && p.split('/').all(|c| !c.is_empty() && c != "." && c != ".." && !c.chars().any(char::is_control))
}

/// Which part of the data folder a path belongs to, if it may be restored at all.
fn allowed(rel: &str) -> bool {
    let top = rel.split('/').next().unwrap_or("");
    (PLAIN.contains(&rel) || [DEVICES, "layout.json", "settings.json", "variables.json"].contains(&rel) || (DIRS.contains(&top) && rel.contains('/')))
        && !SKIP.iter().any(|s| rel == *s || rel.starts_with(&format!("{s}/")))
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) -> Result<()> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Ok(()) };
    for e in entries.flatten() {
        let p = e.path();
        let ft = e.file_type()?;
        let name = e.file_name().to_string_lossy().to_string();
        // Temporary files from interrupted writes.
        if name.starts_with('.') || ft.is_symlink() {
            continue;
        }
        let rel = p.strip_prefix(root)?.to_string_lossy().replace('\\', "/");
        if ft.is_dir() {
            if !SKIP.contains(&rel.as_str()) {
                walk(root, &p, out)?;
            }
        } else if safe(&rel) && allowed(&rel) {
            out.push((rel, p));
        }
    }
    Ok(())
}

fn read_devices(path: &Path) -> Vec<Device> {
    store::read_json(path).ok().flatten().unwrap_or_default()
}

fn count_contents(get: &dyn Fn(&str) -> Option<Vec<u8>>, names: &[String]) -> Contents {
    let json = |n: &str| get(n).and_then(|b| serde_json::from_slice::<Value>(&b).ok());
    let len = |n: &str| json(n).map(|v| v.as_array().map(Vec::len).or(v.as_object().map(|o| o.len())).unwrap_or(0)).unwrap_or(0);
    let layout: Option<LayoutFile> = get("layout.json").and_then(|b| serde_json::from_slice(&b).ok());
    let (layouts, pages, keys) = layout.map(|l| (l.profiles.len(), l.profiles.iter().map(|p| p.pages.len()).sum(), all_keys(&l).count())).unwrap_or_default();
    Contents {
        layouts,
        pages,
        keys,
        plugins: len("plugins.json"),
        plugin_settings: names.iter().filter(|n| n.starts_with("plugin-config/")).count(),
        streamdeck_plugins: len("streamdeck/plugins.json"),
        devices: len(DEVICES),
        tokens: len("tokens.json"),
        assets: len("assets/index.json"),
        history: len("history.json"),
        known_decks: len("known-decks.json"),
        files: names.len(),
        bytes: 0,
    }
}

/// Makes a backup file. With a passphrase, passwords and keys go in too, encrypted.
pub fn create(app: &App, passphrase: Option<&str>) -> Result<Vec<u8>> {
    if passphrase.is_some_and(|p| p.chars().count() < MIN_PASSPHRASE) {
        bail!("the passphrase needs at least {MIN_PASSPHRASE} characters");
    }
    let root = app.paths.root().to_path_buf();
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    {
        let e = app.engine.lock();
        files.push(("layout.json".into(), serde_json::to_vec_pretty(e.layout())?));
        files.push(("variables.json".into(), serde_json::to_vec_pretty(&e.vars_with_prefix("user."))?));
    }
    files.push(("settings.json".into(), serde_json::to_vec_pretty(&*app.settings.read())?));
    let devices = read_devices(&root.join(DEVICES));
    let stripped: Vec<Device> = devices.iter().cloned().map(|d| Device { secret: String::new(), pin: None, ..d }).collect();
    files.push((DEVICES.into(), serde_json::to_vec_pretty(&stripped)?));
    let mut found = Vec::new();
    for f in PLAIN {
        if root.join(f).is_file() {
            found.push((f.to_string(), root.join(f)));
        }
    }
    for d in DIRS {
        walk(&root, &root.join(d), &mut found)?;
    }
    for (rel, path) in found {
        files.push((rel, std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?));
    }

    let names: Vec<String> = files.iter().map(|(n, _)| n.clone()).collect();
    let mut contents = {
        let map: HashMap<&str, &Vec<u8>> = files.iter().map(|(n, b)| (n.as_str(), b)).collect();
        count_contents(&|n| map.get(n).map(|b| (*b).clone()), &names)
    };
    contents.bytes = files.iter().map(|(_, b)| b.len() as u64).sum();

    let mut sealed = None;
    let mut vault_bytes = Vec::new();
    if let Some(pass) = passphrase {
        let mut v = Vault { cid: app.identity.read().cid.clone(), ..Default::default() };
        for d in &devices {
            v.devices.insert(d.id.clone(), (d.secret.clone(), d.pin.clone()));
            if let Some(pin) = app.plugins.secrets.get(PIN_SECRET, &d.id) {
                v.secrets.push((PIN_SECRET.into(), d.id.clone(), pin));
            }
        }
        for (plugin, field) in app.plugins.secret_fields() {
            if let Some(val) = app.plugins.secrets.get(&plugin, &field) {
                v.secrets.push((plugin, field, val));
            }
        }
        let (s, ct) = seal(pass, &serde_json::to_vec(&v)?)?;
        sealed = Some(s);
        vault_bytes = ct;
    }
    let manifest = Manifest {
        format: FORMAT.into(),
        version: VERSION,
        created: crate::server::conn::now_ms(),
        keyside: env!("CARGO_PKG_VERSION").into(),
        name: app.settings.read().name.clone(),
        contents,
        secrets: sealed,
    };

    let mut out = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut out);
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated).large_file(true);
        z.start_file(MANIFEST, opts)?;
        z.write_all(&serde_json::to_vec_pretty(&manifest)?)?;
        if manifest.secrets.is_some() {
            z.start_file(VAULT, opts)?;
            z.write_all(&vault_bytes)?;
        }
        for (n, b) in &files {
            z.start_file(format!("{FILES}{n}"), opts)?;
            z.write_all(b)?;
        }
        z.finish()?;
    }
    Ok(out.into_inner())
}

/// A backup read into memory and checked.
pub struct Backup {
    pub manifest: Manifest,
    files: Vec<(String, Vec<u8>)>,
    vault: Option<Vec<u8>>,
}

/// Reads and checks a backup without changing anything.
pub fn read(bytes: &[u8]) -> Result<Backup> {
    if bytes.len() > MAX_SIZE {
        bail!("the file is larger than {} MB", MAX_SIZE / 1024 / 1024);
    }
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).context("not a Keyside backup")?;
    if zip.len() > MAX_ENTRIES {
        bail!("the backup has too many files");
    }
    let mut manifest: Option<Manifest> = None;
    let mut vault = None;
    let mut files = Vec::new();
    let mut total = 0u64;
    for i in 0..zip.len() {
        let mut f = zip.by_index(i)?;
        if f.is_dir() {
            continue;
        }
        let name = f.name()?.to_string();
        if f.is_symlink() {
            bail!("the backup holds a link: {name}");
        }
        total += f.size();
        if total > MAX_UNPACKED {
            bail!("the backup unpacks to more than 2 GB");
        }
        let declared = f.size();
        let mut buf = Vec::with_capacity(declared.min(64 * 1024 * 1024) as usize);
        (&mut f).take(declared + 1).read_to_end(&mut buf)?;
        if buf.len() as u64 != declared {
            bail!("{name} is damaged");
        }
        match name.as_str() {
            MANIFEST => manifest = Some(serde_json::from_slice(&buf).context("not a Keyside backup")?),
            VAULT => vault = Some(buf),
            _ => {
                let rel = name.strip_prefix(FILES).filter(|r| safe(r) && allowed(r)).with_context(|| format!("the backup holds an unexpected file: {name}"))?;
                if files.iter().any(|(n, _): &(String, Vec<u8>)| n == rel) {
                    bail!("the backup lists {rel} twice");
                }
                files.push((rel.to_string(), buf));
            }
        }
    }
    let mut manifest = manifest.context("not a Keyside backup")?;
    if manifest.format != FORMAT {
        bail!("not a Keyside backup");
    }
    if manifest.version > VERSION {
        bail!("this backup was made by a newer Keyside. Update Keyside first.");
    }
    if manifest.secrets.is_some() != vault.is_some() {
        bail!("the backup is incomplete");
    }
    let layout = files.iter().find(|(n, _)| n == "layout.json").context("the backup has no layout")?;
    let l: LayoutFile = serde_json::from_slice(&layout.1).context("the layout in the backup is damaged")?;
    crate::model::validate(&l).map_err(|e| anyhow::anyhow!("the layout in the backup is invalid: {e}"))?;
    // Count from the files themselves rather than trusting the summary.
    let names: Vec<String> = files.iter().map(|(n, _)| n.clone()).collect();
    let map: HashMap<&str, &Vec<u8>> = files.iter().map(|(n, b)| (n.as_str(), b)).collect();
    let mut c = count_contents(&|n| map.get(n).map(|b| (*b).clone()), &names);
    c.bytes = total;
    manifest.contents = c;
    Ok(Backup { manifest, files, vault })
}

impl Backup {
    fn vault(&self, passphrase: Option<&str>) -> Result<Option<Vault>> {
        let (Some(s), Some(ct)) = (&self.manifest.secrets, &self.vault) else { return Ok(None) };
        let pass = passphrase.filter(|p| !p.is_empty()).context("enter the passphrase used for this backup")?;
        let plain = open(pass, s, ct)?;
        Ok(Some(serde_json::from_slice(&plain).context("damaged backup")?))
    }
}

/// Checks the passphrase without changing anything.
pub fn check_passphrase(b: &Backup, passphrase: Option<&str>) -> Result<()> {
    b.vault(passphrase).map(|_| ())
}

/// Replaces the data with the backup, then reloads it and restarts plugins. Passwords and keys
/// from the backup replace the current ones; a backup without them keeps the current ones,
/// and paired devices it can't give keys to are dropped (they pair again).
pub async fn restore(app: &Arc<App>, b: Backup, passphrase: Option<&str>) -> Result<Value> {
    let vault = b.vault(passphrase)?;
    let root = app.paths.root().to_path_buf();
    let stage = root.join(format!(".restore-{}", crate::ids::random_b64(6)));
    let _ = std::fs::remove_dir_all(&stage);
    store::private_dir(&stage)?;
    let unpack = || -> Result<()> {
        for (rel, bytes) in &b.files {
            let p = stage.join(rel);
            store::private_dir(p.parent().context("bad path")?)?;
            std::fs::write(&p, bytes)?;
        }
        Ok(())
    };
    if let Err(e) = unpack() {
        let _ = std::fs::remove_dir_all(&stage);
        return Err(e).context("unpacking the backup");
    }

    let old_devices = read_devices(&root.join(DEVICES));
    let old_settings = app.settings.read().clone();
    let old_secret_fields = app.plugins.secret_fields();

    // Plugins hold files open (Windows won't replace them), so they stop first.
    app.plugins.stop_all();
    crate::streamdeck::stop_all(app);
    tokio::time::sleep(std::time::Duration::from_millis(600)).await;

    let swap = || -> Result<()> {
        for d in DIRS {
            let (cur, new, old) = (root.join(d), stage.join(d), root.join(format!(".restore-old-{d}")));
            let _ = std::fs::remove_dir_all(&old);
            if cur.exists() {
                std::fs::rename(&cur, &old).with_context(|| format!("moving {d} aside"))?;
            }
            if new.exists() {
                std::fs::rename(&new, &cur).with_context(|| format!("putting {d} in place"))?;
            }
            store::private_dir(&cur)?;
            let _ = std::fs::remove_dir_all(&old);
        }
        for f in PLAIN {
            let new = stage.join(f);
            if new.exists() {
                std::fs::rename(&new, root.join(f))?;
            } else {
                let _ = std::fs::remove_file(root.join(f));
            }
        }
        Ok(())
    };
    let swapped = swap();
    let staged = |n: &str| std::fs::read(stage.join(n)).ok();
    let layout: Option<LayoutFile> = staged("layout.json").and_then(|b| serde_json::from_slice(&b).ok());
    let vars: HashMap<String, Value> = staged("variables.json").and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    let settings_raw = staged("settings.json");
    let devices: Vec<Device> = staged(DEVICES).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&stage);
    if let Err(e) = swapped {
        // Whatever moved stays moved; start what can start so Keyside keeps working.
        app.plugins.reload();
        app.plugins.start_all();
        return Err(e).context("replacing the data");
    }

    // Paired devices: keys from the backup, or the ones this PC already has.
    let mut kept = Vec::new();
    let mut dropped = 0;
    for mut d in devices {
        let keys = match &vault {
            Some(v) => v.devices.get(&d.id).cloned(),
            None => old_devices.iter().find(|o| o.id == d.id).map(|o| (o.secret.clone(), o.pin.clone())),
        };
        match keys {
            Some((secret, pin)) if !secret.is_empty() => {
                d.secret = secret;
                d.pin = pin;
                kept.push(d);
            }
            _ => dropped += 1,
        }
    }
    store::write_json_private(&root.join(DEVICES), &kept)?;

    let mut restart = false;
    if let Some(v) = &vault {
        // Secrets of the current plugins and devices go, the backup's come in.
        for (plugin, field) in old_secret_fields {
            let _ = app.plugins.secrets.set(&plugin, &field, None);
        }
        for d in &old_devices {
            let _ = app.plugins.secrets.set(PIN_SECRET, &d.id, None);
        }
        for (owner, field, value) in &v.secrets {
            app.plugins.secrets.set(owner, field, Some(value))?;
        }
        if !v.cid.is_empty() && v.cid != app.identity.read().cid {
            let mut id: Identity = app.identity.read().clone();
            id.cid = v.cid.clone();
            store::write_json_private(&root.join("identity.json"), &id)?;
            *app.identity.write() = id;
            restart = true;
        }
    }

    // Settings, keeping this PC's editor port so the open editor keeps working.
    let mut settings: Settings = settings_raw.and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_else(|| old_settings.clone());
    settings.admin_port = old_settings.admin_port;
    settings.editor_zoom = old_settings.editor_zoom;
    settings.save(&app.paths)?;
    restart |= settings.lan_port != old_settings.lan_port
        || settings.lan_bind != old_settings.lan_bind
        || settings.lan_enabled != old_settings.lan_enabled
        || settings.mdns != old_settings.mdns
        || settings.api_on_lan != old_settings.api_on_lan;
    *app.settings.write() = settings;

    *app.devices.lock() = Devices::load(root.join(DEVICES))?;
    *app.tokens.lock() = Tokens::load(root.join("tokens.json"))?;
    app.assets.reload();
    app.templates.reload()?;
    app.notify(Change::Templates);
    app.plugins.reload();
    app.streamdeck.reload();
    {
        let mut e = app.engine.lock();
        if let Some(l) = layout {
            e.replace_layout(l)?;
        }
        e.restore_vars(vars);
        e.reload_key_states()?;
        let _ = store::write_json(&root.join("variables.json"), &e.vars_with_prefix("user."));
    }
    // Devices reconnect with their restored keys; removed ones can't.
    let online: Vec<String> = app.online.lock().keys().filter(|id| *id != "local").cloned().collect();
    for id in online {
        app.notify(Change::Updated(id));
    }
    app.notify(Change::Pending);
    if let Some(dir) = app.bundled_plugins.clone() {
        app.plugins.seed_bundled(&dir);
    }
    app.plugins.start_all();
    app.plugins.sync_all().await;
    Ok(json!({ "restartNeeded": restart, "devicesDropped": dropped, "secrets": vault.is_some() }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sealed_secrets_need_the_right_passphrase() {
        let (s, ct) = seal("correct horse", b"hello").unwrap();
        assert_eq!(open("correct horse", &s, &ct).unwrap(), b"hello");
        assert!(open("wrong horse!", &s, &ct).unwrap_err().to_string().contains("wrong passphrase"));
    }

    #[test]
    fn only_known_data_paths_are_restored() {
        for ok in ["layout.json", "devices.json", "assets/ab.png", "streamdeck/plugins/x.sdPlugin/a.js", "plugin-config/dev.keyside.obs.json"] {
            assert!(safe(ok) && allowed(ok), "{ok}");
        }
        for bad in ["identity.json", "secrets.json", "../x", "assets/../identity.json", "tls/tls-key.der", "streamdeck/cache/x", "plugin-cache/a", "assets", "/etc/passwd"] {
            assert!(!(safe(bad) && allowed(bad)), "{bad}");
        }
    }

    fn app(dir: &Path) -> Arc<App> {
        App::new(crate::app::Options { data_dir: Some(dir.into()), bundled_plugins: None }).unwrap()
    }

    #[tokio::test]
    async fn restores_with_and_without_passwords() {
        let a = tempfile::tempdir().unwrap();
        let src = app(a.path());
        let mut l = src.engine.lock().layout().clone();
        l.profiles[0].name = "From backup".into();
        src.engine.lock().replace_layout(l).unwrap();
        src.engine.lock().set_var("user.count", json!(7));
        src.assets.put("dot.svg", b"<svg xmlns='http://www.w3.org/2000/svg'/>").unwrap();
        src.tokens.lock().create("Bot", vec![crate::tokens::ApiScope::Press]).unwrap();
        let dev = Device {
            id: "d1".into(),
            name: "Phone".into(),
            kind: "phone".into(),
            app: "browser".into(),
            scope: crate::devices::Scope::Deck,
            pages: vec![],
            secret: "c2VjcmV0".into(),
            pin: None,
            created: 1,
            last_seen: 0,
        };
        store::write_json_private(&a.path().join(DEVICES), &vec![dev]).unwrap();
        src.plugins.secrets.set(PIN_SECRET, "d1", Some("1234")).unwrap();

        assert!(create(&src, Some("short")).is_err());
        let plain = create(&src, None).unwrap();
        let sealed = create(&src, Some("a long passphrase")).unwrap();
        let stored = read(&plain).unwrap().files.into_iter().find(|(n, _)| n == DEVICES).unwrap().1;
        assert!(!String::from_utf8_lossy(&stored).contains("c2VjcmV0"), "device keys stay out without a passphrase");

        let info = read(&sealed).unwrap();
        assert!(info.manifest.secrets.is_some());
        assert_eq!((info.manifest.contents.devices, info.manifest.contents.tokens, info.manifest.contents.assets), (1, 1, 1));
        assert!(check_passphrase(&info, Some("nope nope nope")).is_err());
        assert!(check_passphrase(&info, None).is_err());

        // A new PC: without passwords the device can't come along.
        let b = tempfile::tempdir().unwrap();
        let dst = app(b.path());
        let r = restore(&dst, read(&plain).unwrap(), None).await.unwrap();
        assert_eq!(r["devicesDropped"], 1);
        assert_eq!(dst.engine.lock().layout().profiles[0].name, "From backup");
        assert_eq!(dst.engine.lock().var("user.count"), Some(json!(7)));
        assert_eq!(dst.assets.list().len(), 1);
        assert_eq!(dst.tokens.lock().list().len(), 1);
        assert!(dst.devices.lock().get("d1").is_none());

        // With them it can, and keeps its PIN and the companion id it paired with.
        let r = restore(&dst, read(&sealed).unwrap(), Some("a long passphrase")).await.unwrap();
        assert_eq!((r["devicesDropped"].as_u64(), r["secrets"].as_bool()), (Some(0), Some(true)));
        assert_eq!(dst.devices.lock().get("d1").unwrap().secret, "c2VjcmV0");
        assert_eq!(dst.plugins.secrets.get(PIN_SECRET, "d1").as_deref(), Some("1234"));
        assert_eq!(dst.identity.read().cid, src.identity.read().cid);
        assert_ne!(dst.identity.read().admin_token, src.identity.read().admin_token, "sign-in stays per PC");

        // Restarting reads the same data back.
        drop(dst);
        let again = app(b.path());
        assert_eq!(again.engine.lock().layout().profiles[0].name, "From backup");
        assert!(again.devices.lock().get("d1").is_some());
        assert_eq!(again.engine.lock().var("user.count"), Some(json!(7)));
    }
}

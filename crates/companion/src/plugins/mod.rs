//! Plugin host. Each plugin runs in its own sandboxed Deno process and talks JSON-RPC
//! over stdio. Every call a plugin makes is checked against what it owns and declared.

pub mod ipc;
pub mod manifest;
pub mod package;
pub mod registry;
pub mod rpc;
pub mod sandbox;
pub mod secrets;

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, Weak};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use tokio::io::AsyncBufReadExt;

use crate::app::App;
use crate::model::{Tone, valid_icon, valid_var_name};
use crate::state::view::{FeedItem, FeedMatch, ViewPatch};
use crate::state::{ConnectionView, Pill};
use crate::store;
use manifest::{Manifest, Permissions};
use rpc::{Incoming, Peer};

const RUN_TIMEOUT: Duration = Duration::from_secs(15);
const OPTIONS_TIMEOUT: Duration = Duration::from_secs(8);
const SLIDE_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_IMAGE_URL: usize = 350 * 1024;
const CRASH_WINDOW: Duration = Duration::from_secs(300);
const CRASH_LIMIT: usize = 5;
/// How long a plugin started for the editor (options, a connection test) keeps running
/// while no key uses it.
const DEMAND_TTL: Duration = Duration::from_secs(600);
const TEST_TIMEOUT: Duration = Duration::from_secs(12);

#[derive(Debug, Clone, Serialize)]
pub struct Step {
    pub instance: String,
    pub action: String,
    pub settings: Map<String, Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// Shipped with Keyside.
    Bundled,
    /// Downloaded from the plugin registry, signature checked.
    Registry,
    /// Installed from a file the person picked. Unverified unless signed.
    File,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Installed {
    pub manifest: Manifest,
    pub enabled: bool,
    pub granted: Permissions,
    pub source: Source,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signer: Option<String>,
    pub installed_at: u64,
}

impl Installed {
    pub fn needs_approval(&self) -> bool {
        !self.manifest.permissions.within(&self.granted)
    }
}

struct Running {
    generation: u64,
    peer: Option<Arc<Peer>>,
    instances: HashSet<String>,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    crashes: VecDeque<Instant>,
    last_toast: Option<Instant>,
    last_browser: Option<Instant>,
    ipc: Option<ipc::Bridge>,
}

#[derive(Default)]
struct State {
    installed: BTreeMap<String, Installed>,
    running: HashMap<String, Running>,
    generation: u64,
}

pub struct Host {
    app: OnceLock<Weak<App>>,
    deno: Option<PathBuf>,
    packages: PathBuf,
    data: PathBuf,
    cache: PathBuf,
    config: PathBuf,
    index: PathBuf,
    pub secrets: secrets::Secrets,
    state: Mutex<State>,
    pub staged: Mutex<HashMap<String, (package::Package, Instant)>>,
    /// Browser sign-ins waiting for their redirect, by OAuth state.
    oauth: Mutex<HashMap<String, OAuthWait>>,
    /// Plugins the editor needs for a while although no key uses them, until the given time.
    /// Keyed by plugin id, Stream Deck plugins included.
    demand: Mutex<HashMap<String, Instant>>,
}

struct OAuthWait {
    plugin: String,
    tx: tokio::sync::oneshot::Sender<Result<String, String>>,
}

const OAUTH_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub prefix: String,
    pub description: Option<String>,
    pub homepage: Option<String>,
    pub icon: Option<String>,
    pub enabled: bool,
    pub status: String,
    /// A key uses it, so it runs. Plugins nobody uses stay stopped.
    pub in_use: bool,
    pub source: Source,
    pub signer: Option<String>,
    pub requested: Permissions,
    pub granted: Permissions,
    pub needs_approval: bool,
    pub connection: Option<manifest::Schema>,
    pub setup: Vec<String>,
    /// Current connection values. Secret fields show only whether they are set.
    pub values: Map<String, Value>,
    pub variables: Vec<manifest::VarDef>,
    pub feeds: Vec<manifest::FeedDef>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogPlugin {
    pub id: String,
    pub name: String,
    pub prefix: String,
    pub icon: Option<String>,
    pub running: bool,
    pub actions: Vec<manifest::ActionDef>,
    pub variables: Vec<manifest::VarDef>,
    pub feeds: Vec<manifest::FeedDef>,
}

impl Host {
    pub fn open(root: &Path) -> Result<Self> {
        let dirs = |n: &str| -> Result<PathBuf> {
            let p = root.join(n);
            store::private_dir(&p)?;
            Ok(p)
        };
        let host = Self {
            app: OnceLock::new(),
            deno: sandbox::find_deno(),
            packages: dirs("plugins")?,
            data: dirs("plugin-data")?,
            cache: dirs("plugin-cache")?,
            config: dirs("plugin-config")?,
            index: root.join("plugins.json"),
            secrets: secrets::Secrets::open(root.join("secrets.json")),
            state: Mutex::new(State::default()),
            staged: Mutex::new(HashMap::new()),
            oauth: Mutex::new(HashMap::new()),
            demand: Mutex::new(HashMap::new()),
        };
        let installed: BTreeMap<String, Installed> = store::read_json_or_reset(&host.index)?.unwrap_or_default();
        host.state.lock().installed = installed.into_iter().filter(|(_, i)| manifest::validate(&i.manifest).is_ok()).collect();
        if host.deno.is_none() {
            tracing::warn!("Deno runtime not found; plugins can't run. Set KEYSIDE_DENO or install Deno.");
        }
        Ok(host)
    }

    pub fn attach(&self, app: &Arc<App>) {
        let _ = self.app.set(Arc::downgrade(app));
    }

    fn app(&self) -> Option<Arc<App>> {
        self.app.get()?.upgrade()
    }

    pub(crate) fn app_handle(&self) -> Option<Arc<App>> {
        self.app()
    }

    fn save(&self, st: &State) -> Result<()> {
        store::write_json(&self.index, &st.installed)
    }

    /// (plugin, field) of every secret connection setting, for backups.
    pub fn secret_fields(&self) -> Vec<(String, String)> {
        let st = self.state.lock();
        st.installed
            .values()
            .flat_map(|i| i.manifest.connection.iter().flat_map(|c| c.properties.iter()).filter(|(_, f)| f.is_secret()).map(|(n, _)| (i.manifest.id.clone(), n.clone())))
            .collect()
    }

    /// Reads the plugin list and secrets again, after a restore. Plugins must be stopped.
    pub fn reload(&self) {
        let installed: BTreeMap<String, Installed> = store::read_json_or_reset(&self.index).ok().flatten().unwrap_or_default();
        self.state.lock().installed = installed.into_iter().filter(|(_, i)| manifest::validate(&i.manifest).is_ok()).collect();
        self.secrets.forget();
        self.refresh_engine();
    }

    pub fn installed_ids(&self) -> HashSet<String> {
        self.state.lock().installed.keys().cloned().collect()
    }

    pub fn get(&self, id: &str) -> Option<Installed> {
        self.state.lock().installed.get(id).cloned()
    }

    fn package_dir(&self, id: &str, version: &str) -> PathBuf {
        self.packages.join(id).join(version)
    }

    /// Connection values with secrets filled in from the keychain. Only for the plugin itself.
    /// Hosts this plugin may reach right now, with templates filled in.
    fn granted_net(&self, m: &Manifest) -> Vec<String> {
        let Some(inst) = self.get(&m.id) else { return Vec::new() };
        let allowed: Vec<String> = m.permissions.net.iter().filter(|n| inst.granted.net.contains(n)).cloned().collect();
        manifest::expand_net(&allowed, &self.connection_values(m))
    }

    /// Called by the editor listener when a browser comes back from a sign-in page.
    pub fn oauth_finish(&self, state: &str, result: Result<String, String>) -> bool {
        match self.oauth.lock().remove(state) {
            Some(w) => w.tx.send(result).is_ok(),
            None => false,
        }
    }

    fn connection_values(&self, m: &Manifest) -> Map<String, Value> {
        let mut v: Map<String, Value> = store::read_json(&self.config.join(format!("{}.json", m.id))).ok().flatten().unwrap_or_default();
        if let Some(schema) = &m.connection {
            for (name, f) in &schema.properties {
                if f.is_secret() {
                    match self.secrets.get(&m.id, name) {
                        Some(s) => v.insert(name.clone(), Value::String(s)),
                        None => v.remove(name),
                    };
                } else if !v.contains_key(name)
                    && let Some(d) = field_default(f)
                {
                    v.insert(name.clone(), d);
                }
            }
        }
        v
    }

    /// Connection values safe to show in the editor.
    fn public_values(&self, m: &Manifest) -> Map<String, Value> {
        let mut v = self.connection_values(m);
        if let Some(schema) = &m.connection {
            for (name, f) in &schema.properties {
                if f.is_secret() {
                    let set = v.contains_key(name);
                    v.insert(name.clone(), json!({ "set": set }));
                }
            }
        }
        v
    }

    pub fn list(&self) -> Vec<PluginInfo> {
        let used = self.in_use();
        let ids: Vec<String> = self.state.lock().installed.keys().cloned().collect();
        // Engine first, then plugin state: never hold both locks at once.
        // Plugins not running show their last known state.
        let statuses: HashMap<String, String> = self
            .app()
            .map(|a| {
                let e = a.engine.lock();
                ids.iter().filter_map(|id| e.connection(id)).map(|c| (c.id, c.status)).collect()
            })
            .unwrap_or_default();
        let st = self.state.lock();
        st.installed
            .values()
            .map(|i| {
                let m = &i.manifest;
                let running = st.running.contains_key(&m.id);
                let status = match statuses.get(&m.id) {
                    Some(s) if i.enabled => s.clone(),
                    _ if i.enabled && !running && !used.contains(&m.id) => "idle".into(),
                    _ if i.enabled => "offline".into(),
                    _ => "disabled".into(),
                };
                PluginInfo {
                    id: m.id.clone(),
                    name: m.name.clone(),
                    version: m.version.clone(),
                    author: m.author.clone(),
                    prefix: m.prefix.clone(),
                    description: m.description.clone(),
                    homepage: m.homepage.clone(),
                    icon: m.icon.clone(),
                    enabled: i.enabled,
                    status,
                    in_use: used.contains(&m.id),
                    source: i.source,
                    signer: i.signer.clone(),
                    requested: m.permissions.clone(),
                    granted: i.granted.clone(),
                    needs_approval: i.needs_approval(),
                    connection: m.connection.clone(),
                    setup: m.setup.clone(),
                    values: self.public_values(m),
                    variables: m.variables.clone(),
                    feeds: m.feeds.clone(),
                }
            })
            .collect()
    }

    pub fn catalog(&self) -> Vec<CatalogPlugin> {
        let st = self.state.lock();
        st.installed
            .values()
            .filter(|i| i.enabled)
            .map(|i| CatalogPlugin {
                id: i.manifest.id.clone(),
                name: i.manifest.name.clone(),
                prefix: i.manifest.prefix.clone(),
                icon: i.manifest.icon.clone(),
                running: st.running.get(&i.manifest.id).is_some_and(|r| r.peer.is_some()),
                actions: i.manifest.actions.clone(),
                variables: i.manifest.variables.clone(),
                feeds: i.manifest.feeds.clone(),
            })
            .collect()
    }

    pub fn action_def(&self, plugin: &str, action: &str) -> Option<manifest::ActionDef> {
        self.state.lock().installed.get(plugin)?.manifest.actions.iter().find(|a| a.id == action).cloned()
    }

    /// Installs or updates a checked package. New permissions wait for approval
    /// unless `approve` is set (the editor shows them first).
    pub fn install(&self, pkg: &package::Package, source: Source, approve: bool) -> Result<()> {
        let m = &pkg.manifest;
        manifest::validate(m).map_err(anyhow::Error::msg)?;
        if !m.permissions.run.is_empty() && source != Source::Bundled {
            bail!("only plugins that ship with Keyside can run programs");
        }
        {
            let st = self.state.lock();
            if let Some(other) = st.installed.values().find(|i| i.manifest.prefix == m.prefix && i.manifest.id != m.id) {
                bail!("prefix \"{}\" is already used by {}", m.prefix, other.manifest.name);
            }
            if let Some(old) = st.installed.get(&m.id)
                && old.signer.is_some()
                && old.signer != pkg.signer
                && source != Source::Bundled
            {
                bail!(if pkg.signer.is_some() {
                    "this update is signed by a different key than the installed version"
                } else {
                    "the installed version is signed and this update isn't"
                });
            }
        }
        let dir = self.package_dir(&m.id, &m.version);
        // Unpack next to the old files first, then stop the plugin and swap. The running
        // process may still hold its files for a moment, so removing them is retried.
        let staging = self.packages.join(&m.id).join(format!(".{}.new", m.version));
        if staging.exists() {
            std::fs::remove_dir_all(&staging)?;
        }
        pkg.extract_to(&staging)?;
        let previous = self.get(&m.id);
        self.stop(&m.id);
        if dir.exists() {
            let mut tries = 0;
            while let Err(e) = std::fs::remove_dir_all(&dir) {
                tries += 1;
                if tries > 10 {
                    let _ = std::fs::remove_dir_all(&staging);
                    return Err(e).context("replacing the old plugin files");
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        std::fs::rename(&staging, &dir).context("moving the plugin into place")?;
        let granted = if approve || source == Source::Bundled { m.permissions.clone() } else { previous.as_ref().map(|p| p.granted.clone()).unwrap_or_default() };
        let inst = Installed {
            manifest: m.clone(),
            enabled: previous.as_ref().is_none_or(|p| p.enabled),
            granted,
            source,
            signer: pkg.signer.clone(),
            installed_at: crate::devices::now(),
        };
        {
            let mut st = self.state.lock();
            st.installed.insert(m.id.clone(), inst);
            self.save(&st)?;
        }
        if let Some(prev) = previous
            && prev.manifest.version != m.version
        {
            let _ = std::fs::remove_dir_all(self.package_dir(&m.id, &prev.manifest.version));
        }
        self.refresh_engine();
        self.start(&m.id);
        Ok(())
    }

    /// Installs bundled plugins that are missing or older than the bundled copy.
    pub fn seed_bundled(&self, dir: &Path) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for e in entries.flatten() {
            let p = e.path();
            // Shared code and hidden folders sit next to the plugins.
            if e.file_name().to_string_lossy().starts_with(['_', '.']) {
                continue;
            }
            let pkg = if p.is_dir() {
                package::Package::from_dir(&p)
            } else if p.extension().is_some_and(|x| x == "pdp") {
                std::fs::read(&p).map_err(Into::into).and_then(|b| package::Package::from_zip(&b))
            } else {
                continue;
            };
            let pkg = match pkg {
                Ok(p) => p,
                Err(err) => {
                    tracing::warn!("skipping bundled plugin {}: {err:#}", p.display());
                    continue;
                }
            };
            let newer = self
                .get(&pkg.manifest.id)
                .is_none_or(|i| i.source == Source::Bundled && semver::Version::parse(&i.manifest.version).ok() < semver::Version::parse(&pkg.manifest.version).ok());
            if newer && let Err(err) = self.install(&pkg, Source::Bundled, true) {
                tracing::warn!("could not install bundled plugin {}: {err:#}", pkg.manifest.id);
            }
        }
    }

    pub fn uninstall(&self, id: &str) -> Result<bool> {
        self.stop(id);
        let removed = {
            let mut st = self.state.lock();
            let r = st.installed.remove(id);
            self.save(&st)?;
            r
        };
        let Some(r) = removed else { return Ok(false) };
        let _ = std::fs::remove_dir_all(self.packages.join(id));
        let _ = std::fs::remove_dir_all(self.data.join(id));
        let _ = std::fs::remove_dir_all(self.cache.join(id));
        let _ = std::fs::remove_file(self.config.join(format!("{id}.json")));
        if let Some(schema) = &r.manifest.connection {
            for (name, f) in &schema.properties {
                if f.is_secret() {
                    let _ = self.secrets.set(id, name, None);
                }
            }
        }
        self.refresh_engine();
        Ok(true)
    }

    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<()> {
        {
            let mut st = self.state.lock();
            let i = st.installed.get_mut(id).context("plugin not installed")?;
            i.enabled = enabled;
            self.save(&st)?;
        }
        if enabled {
            self.start(id)
        } else {
            self.stop(id)
        }
        self.refresh_engine();
        Ok(())
    }

    pub fn approve(&self, id: &str) -> Result<()> {
        {
            let mut st = self.state.lock();
            let i = st.installed.get_mut(id).context("plugin not installed")?;
            i.granted = i.manifest.permissions.clone();
            self.save(&st)?;
        }
        self.restart(id);
        Ok(())
    }

    /// Saves connection settings from the editor or the plugin itself.
    pub fn configure(&self, id: &str, values: &Map<String, Value>, from_plugin: bool) -> Result<()> {
        let m = self.get(id).context("plugin not installed")?.manifest;
        let Some(schema) = &m.connection else { bail!("this plugin has no settings") };
        // Addresses decide what the plugin may reach, so only the person can change them.
        if from_plugin && let Some(k) = values.keys().find(|k| m.permissions.net.iter().any(|n| n.contains(&format!("{{{k}}}")))) {
            bail!("plugins can't change \"{k}\" themselves");
        }
        let old = self.connection_values(&m);
        let mut merged = old.clone();
        for (k, v) in values {
            merged.insert(k.clone(), v.clone());
        }
        let clean = manifest::clean_values(Some(schema), &merged).map_err(anyhow::Error::msg)?;
        let mut plain = Map::new();
        for (name, f) in &schema.properties {
            if f.is_secret() {
                if values.contains_key(name) {
                    self.secrets.set(id, name, clean.get(name).and_then(Value::as_str))?;
                }
            } else if let Some(v) = clean.get(name) {
                plain.insert(name.clone(), v.clone());
            }
        }
        store::write_json(&self.config.join(format!("{id}.json")), &plain)?;
        let net_changed = manifest::expand_net(&m.permissions.net, &old) != manifest::expand_net(&m.permissions.net, &self.connection_values(&m));
        if from_plugin && !net_changed {
            return Ok(());
        }
        // Network permissions come from these values, so the process restarts with the new set.
        self.restart(id);
        Ok(())
    }

    pub fn refresh_engine(&self) {
        if let Some(app) = self.app() {
            let mut ids: HashSet<String> = self.state.lock().installed.iter().filter(|(_, i)| i.enabled).map(|(k, _)| k.clone()).collect();
            ids.extend(app.streamdeck.enabled_ids());
            let mut e = app.engine.lock();
            e.set_installed(ids);
            e.notify(crate::state::Change::Plugins);
        }
    }

    /// Starts the enabled plugins the layout uses.
    pub fn start_all(&self) {
        self.refresh_engine();
        self.reconcile();
    }

    /// Ids of the plugins the layout uses, Stream Deck plugins included.
    pub fn in_use(&self) -> HashSet<String> {
        let prefixes: Vec<(String, String)> = self.state.lock().installed.values().map(|i| (i.manifest.id.clone(), i.manifest.prefix.clone())).collect();
        let Some(app) = self.app() else { return HashSet::new() };
        let e = app.engine.lock();
        used_by(e.layout(), &prefixes)
    }

    /// Used by the layout, or asked for by the editor a short while ago.
    pub fn wanted(&self, id: &str, used: &HashSet<String>) -> bool {
        used.contains(id) || self.demand.lock().get(id).is_some_and(|t| *t > Instant::now())
    }

    /// Lets a plugin run for a while even though no key uses it.
    pub fn lease(&self, id: &str) {
        self.demand.lock().insert(id.to_string(), Instant::now() + DEMAND_TTL);
    }

    /// Starts plugins the layout now uses and stops the ones it no longer needs.
    pub fn reconcile(&self) {
        let Some(app) = self.app() else { return };
        self.demand.lock().retain(|_, t| *t > Instant::now());
        let used = self.in_use();
        let (enabled, running): (Vec<String>, Vec<String>) = {
            let st = self.state.lock();
            (st.installed.iter().filter(|(_, i)| i.enabled).map(|(k, _)| k.clone()).collect(), st.running.keys().cloned().collect())
        };
        for id in &enabled {
            if self.wanted(id, &used) && !running.contains(id) {
                self.launch(id);
            }
        }
        for id in &running {
            if !self.wanted(id, &used) {
                tracing::info!(plugin = %id, "no key uses this plugin anymore; stopping it");
                self.stop(id);
            }
        }
        crate::streamdeck::reconcile(&app, &used);
    }

    /// Starts a plugin for the editor, even when no key uses it yet, and waits up to `wait`
    /// for it to come up. True when it is running.
    pub async fn ensure(&self, id: &str, wait: Duration) -> bool {
        self.lease(id);
        let Some(app) = self.app() else { return false };
        if let Some(uuid) = id.strip_prefix(crate::streamdeck::PREFIX) {
            return crate::streamdeck::ensure(&app, uuid, wait).await;
        }
        if !self.state.lock().running.contains_key(id) {
            self.launch(id);
        }
        let until = Instant::now() + wait;
        while self.peer(id).is_none() {
            if Instant::now() > until || !self.state.lock().running.contains_key(id) {
                return false;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        true
    }

    /// Restarts a plugin and waits for it to report whether it reached its app.
    pub async fn test(&self, id: &str) -> Result<Value> {
        let app = self.app().context("shutting down")?;
        let sd = id.strip_prefix(crate::streamdeck::PREFIX);
        let enabled = match sd {
            Some(uuid) => app.streamdeck.get(uuid).map(|i| i.enabled),
            None => self.get(id).map(|i| i.enabled),
        };
        match enabled {
            None => bail!("plugin not installed"),
            Some(false) => return Ok(json!({ "status": "disabled", "ms": 0 })),
            Some(true) => {}
        }
        self.lease(id);
        let t0 = Instant::now();
        match sd {
            Some(uuid) => {
                crate::streamdeck::stop(&app, uuid);
                crate::streamdeck::start(&app, uuid);
            }
            None => self.restart(id),
        }
        loop {
            tokio::time::sleep(Duration::from_millis(200)).await;
            let c = app.engine.lock().connection(id);
            let done = c.as_ref().is_some_and(|c| c.status != "connecting");
            if done || t0.elapsed() > TEST_TIMEOUT {
                let (status, detail) = c.map(|c| (c.status, c.detail)).unwrap_or_else(|| ("offline".into(), None));
                return Ok(json!({ "status": status, "detail": detail, "ms": t0.elapsed().as_millis() as u64, "timedOut": !done }));
            }
        }
    }

    /// Health of every installed plugin, Stream Deck plugins included.
    pub fn health(&self) -> Value {
        let Some(app) = self.app() else { return json!({}) };
        let used = self.in_use();
        let mut ids: Vec<(String, bool)> = {
            let st = self.state.lock();
            st.installed.keys().map(|id| (id.clone(), st.running.get(id).is_some_and(|r| r.peer.is_some()))).collect()
        };
        ids.extend(app.streamdeck.health_ids());
        let now = crate::server::conn::now_ms();
        let mut e = app.engine.lock();
        let mut out = Map::new();
        for (id, connected) in ids {
            let r = e.health.report(&id, now);
            let mut v = serde_json::to_value(r).unwrap_or(Value::Null);
            v["inUse"] = json!(used.contains(&id));
            v["running"] = json!(connected);
            out.insert(id, v);
        }
        Value::Object(out)
    }

    pub fn stop_all(&self) {
        let ids: Vec<String> = self.state.lock().running.keys().cloned().collect();
        for id in ids {
            self.stop(&id);
        }
    }

    pub fn restart(&self, id: &str) {
        self.stop(id);
        self.start(id);
    }

    pub fn stop(&self, id: &str) {
        let r = self.state.lock().running.remove(id);
        if let Some(mut r) = r
            && let Some(s) = r.stop.take()
        {
            let _ = s.send(());
        }
        if let Some(app) = self.app() {
            let mut e = app.engine.lock();
            // Noted here, not when the process is gone, so a restart can't race with it.
            e.health.exited(id, crate::server::conn::now_ms(), false, None);
            let mine: HashSet<String> = e.instances_for(id).into_iter().map(|i| i.id).collect();
            e.remove_connection(id);
            e.clear_overrides_with_prefix(|inst| mine.contains(inst));
        }
    }

    /// Starts a plugin when the layout uses it (or the editor asked for it).
    pub fn start(&self, id: &str) {
        if self.wanted(id, &self.in_use()) {
            self.launch(id);
        }
    }

    fn launch(&self, id: &str) {
        let Some(app) = self.app() else { return };
        let Some(inst) = self.get(id) else { return };
        if !inst.enabled {
            return;
        }
        if inst.needs_approval() {
            app.engine.lock().set_connection(ConnectionView {
                id: id.into(),
                name: inst.manifest.name.clone(),
                status: "error".into(),
                detail: Some("needs permission approval".into()),
            });
            return;
        }
        let Some(deno) = self.deno.clone() else {
            app.engine.lock().set_connection(ConnectionView {
                id: id.into(),
                name: inst.manifest.name.clone(),
                status: "error".into(),
                detail: Some("Deno runtime missing".into()),
            });
            return;
        };
        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
        let generation = {
            let mut st = self.state.lock();
            if st.running.contains_key(id) {
                return;
            }
            st.generation += 1;
            let g = st.generation;
            st.running.insert(
                id.into(),
                Running { generation: g, peer: None, instances: HashSet::new(), stop: Some(stop_tx), crashes: VecDeque::new(), last_toast: None, last_browser: None, ipc: None },
            );
            g
        };
        let id = id.to_string();
        tokio::spawn(supervise(app, id, generation, deno, stop_rx));
    }

    fn peer(&self, id: &str) -> Option<Arc<Peer>> {
        self.state.lock().running.get(id).and_then(|r| r.peer.clone())
    }

    pub async fn run(&self, plugin: &str, steps: Vec<Step>, phase: &str, device: &str) -> Result<(), String> {
        if let Some(uuid) = plugin.strip_prefix(crate::streamdeck::PREFIX) {
            let app = self.app().ok_or("shutting down")?;
            return crate::streamdeck::run(&app, uuid, steps, phase).await;
        }
        let peer = self.peer(plugin).ok_or_else(|| format!("{} is not running", self.get(plugin).map(|i| i.manifest.name).unwrap_or_else(|| plugin.into())))?;
        let steps = self.clean_steps(plugin, steps)?;
        peer.request("run", json!({ "steps": steps, "phase": phase, "device": device }), RUN_TIMEOUT).await.map(|_| ())
    }

    fn clean_steps(&self, plugin: &str, steps: Vec<Step>) -> Result<Vec<Step>, String> {
        // Fields that take {variables}, with their length caps, per step.
        let mut fills: Vec<Vec<(String, usize)>> = Vec::new();
        let mut steps = {
            let st = self.state.lock();
            let m = &st.installed.get(plugin).ok_or("plugin not installed")?.manifest;
            steps
                .into_iter()
                .map(|mut s| {
                    let def = m.actions.iter().find(|a| a.id == s.action).ok_or_else(|| format!("{} has no action \"{}\"", m.name, s.action))?;
                    s.settings = manifest::clean_values(def.settings.as_ref(), &s.settings)?;
                    fills.push(def.settings.iter().flat_map(|sc| &sc.properties).filter_map(|(n, f)| Some((n.clone(), f.template_cap()?))).collect());
                    Ok(s)
                })
                .collect::<Result<Vec<_>, String>>()?
        };
        if fills.iter().all(Vec::is_empty) {
            return Ok(steps);
        }
        let Some(app) = self.app() else { return Ok(steps) };
        let e = app.engine.lock();
        let get = |n: &str| e.var(n);
        for (s, fields) in steps.iter_mut().zip(&fills) {
            for (name, cap) in fields {
                match s.settings.get_mut(name) {
                    Some(Value::String(v)) => *v = crate::state::view::fill(v, get, *cap),
                    Some(Value::Object(o)) => {
                        for v in o.values_mut() {
                            if let Value::String(x) = v {
                                *x = crate::state::view::fill(x, get, *cap);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(steps)
    }

    /// Waits for the plugin to finish with the value, so the next one can't overtake it.
    pub async fn slide(&self, plugin: &str, step: Step, value: f64) -> Result<(), String> {
        let Some(peer) = self.peer(plugin) else { return Ok(()) };
        let s = self.clean_steps(plugin, vec![step])?.remove(0);
        peer.request("slide", json!({ "instance": s.instance, "action": s.action, "settings": s.settings, "value": value.clamp(0.0, 1.0) }), SLIDE_TIMEOUT).await.map(|_| ())
    }

    pub async fn options(&self, plugin: &str, action: &str, field: &str, settings: &Map<String, Value>) -> Result<Value, String> {
        // The editor may ask before any pushed key uses the plugin.
        if self.peer(plugin).is_none() {
            self.ensure(plugin, OPTIONS_TIMEOUT).await;
        }
        let peer = self.peer(plugin).ok_or("plugin is not running")?;
        let r = peer.request("options", json!({ "action": action, "field": field, "settings": settings }), OPTIONS_TIMEOUT).await?;
        let list = r.as_array().ok_or("plugin sent invalid options")?;
        Ok(Value::Array(
            list.iter()
                .take(500)
                .filter_map(|o| {
                    let v = o.get("value")?.as_str()?.chars().take(200).collect::<String>();
                    let l = o.get("label").and_then(Value::as_str).unwrap_or(&v).chars().take(100).collect::<String>();
                    Some(json!({ "value": v, "label": l }))
                })
                .collect(),
        ))
    }

    /// Sends each running plugin the keys that use it. Called after layout changes.
    pub async fn sync_all(&self) {
        let Some(app) = self.app() else { return };
        crate::streamdeck::sync(&app).await;
        let ids: Vec<String> = self.state.lock().running.keys().cloned().collect();
        for id in ids {
            sync_one(&app, &id).await;
        }
    }
}

/// Plugins a layout uses: through a step on a key, or through a variable or feed a key shows
/// (like "{obs.scene}" in a label, or "twitch.chat" in a widget). Steps waiting for review
/// don't count, since they can't run. `prefixes` lists (plugin id, variable prefix).
pub fn used_by(layout: &crate::model::LayoutFile, prefixes: &[(String, String)]) -> HashSet<String> {
    let mut used = HashSet::new();
    let marks: Vec<(&String, String, String)> = prefixes.iter().map(|(id, p)| (id, format!("{{{p}."), format!("\"{p}."))).collect();
    for (_, _, k) in crate::state::all_keys(layout) {
        for a in k.actions.iter().chain(&k.off_actions) {
            if !a.review
                && let Some((id, _)) = a.plugin()
            {
                used.insert(id.to_string());
            }
        }
        if marks.iter().all(|(id, _, _)| used.contains(*id)) {
            continue;
        }
        let text = serde_json::to_string(k).unwrap_or_default();
        for (id, template, name) in &marks {
            if !used.contains(*id) && (text.contains(template.as_str()) || text.contains(name.as_str())) {
                used.insert((*id).clone());
            }
        }
    }
    used
}

fn field_default(f: &manifest::Field) -> Option<Value> {
    use manifest::Field::*;
    match f {
        String { default: Some(d), .. } => Some(Value::String(d.clone())),
        Integer { default: Some(d), .. } if d.fract() == 0.0 => Some(Value::from(*d as i64)),
        Number { default: Some(d), .. } | Integer { default: Some(d), .. } => serde_json::Number::from_f64(*d).map(Value::Number),
        Boolean { default: Some(d), .. } => Some(Value::Bool(*d)),
        _ => None,
    }
}

async fn sync_one(app: &Arc<App>, id: &str) {
    let instances = app.engine.lock().instances_for(id);
    let set: HashSet<String> = instances.iter().map(|i| i.id.clone()).collect();
    let (peer, gone) = {
        let mut st = app.plugins.state.lock();
        let Some(r) = st.running.get_mut(id) else { return };
        let gone: HashSet<String> = r.instances.difference(&set).cloned().collect();
        r.instances = set;
        (r.peer.clone(), gone)
    };
    if !gone.is_empty() {
        app.engine.lock().clear_overrides_with_prefix(|inst| gone.contains(inst));
    }
    if let Some(p) = peer {
        p.notify("instances.sync", json!({ "instances": instances })).await;
    }
}

async fn supervise(app: Arc<App>, id: String, generation: u64, deno: PathBuf, mut stop: tokio::sync::oneshot::Receiver<()>) {
    let current = |app: &App| app.plugins.state.lock().running.get(&id).is_some_and(|r| r.generation == generation);
    loop {
        if !current(&app) {
            return;
        }
        let Some(inst) = app.plugins.get(&id) else { return };
        let name = inst.manifest.name.clone();
        {
            let mut e = app.engine.lock();
            e.set_connection(ConnectionView { id: id.clone(), name: name.clone(), status: "connecting".into(), detail: None });
            e.health.started(&id, crate::server::conn::now_ms());
        }

        let outcome = match run_once(&app, &inst, generation, &deno, &mut stop).await {
            // `stop` already noted it.
            Ok(Run::Stopped) => return,
            Ok(Run::Ended) => Ok(()),
            Err(e) => Err(e),
        };
        if !current(&app) {
            return;
        }
        {
            let reason = outcome.as_ref().err().map(|e| format!("{e:#}"));
            app.engine.lock().health.exited(&id, crate::server::conn::now_ms(), true, Some(reason.as_deref().unwrap_or("the plugin stopped on its own")));
        }
        {
            let mut st = app.plugins.state.lock();
            if let Some(r) = st.running.get_mut(&id) {
                r.peer = None;
                r.ipc = None;
            }
        }
        let reason = match &outcome {
            Ok(()) => "stopped".to_string(),
            Err(e) => format!("{e:#}"),
        };
        tracing::warn!(plugin = %id, "plugin exited: {reason}");
        {
            let mut e = app.engine.lock();
            e.set_connection(ConnectionView { id: id.clone(), name: name.clone(), status: "offline".into(), detail: Some(reason.chars().take(120).collect()) });
            e.set_pill(&id, None);
        }
        let crashes = {
            let mut st = app.plugins.state.lock();
            let Some(r) = st.running.get_mut(&id) else { return };
            let now = Instant::now();
            r.crashes.push_back(now);
            while r.crashes.front().is_some_and(|t| now.duration_since(*t) > CRASH_WINDOW) {
                r.crashes.pop_front();
            }
            r.crashes.len()
        };
        if crashes >= CRASH_LIMIT {
            let _ = app.plugins.set_enabled(&id, false);
            app.engine.lock().toast(format!("{name} kept stopping and was turned off"), Tone::Danger);
            return;
        }
        let wait = Duration::from_secs(1 << crashes.min(6));
        tokio::select! {
            _ = tokio::time::sleep(wait) => {}
            _ = &mut stop => return,
        }
    }
}

enum Run {
    /// The process ended on its own.
    Ended,
    /// The host asked it to stop and it's gone.
    Stopped,
}

async fn run_once(app: &Arc<App>, inst: &Installed, generation: u64, deno: &Path, stop: &mut tokio::sync::oneshot::Receiver<()>) -> Result<Run> {
    let m = &inst.manifest;
    let host = &app.plugins;
    let pkg = host.package_dir(&m.id, &m.version);
    let data = host.data.join(&m.id);
    let cache = host.cache.join(&m.id);
    store::private_dir(&data)?;
    store::private_dir(&cache)?;
    for p in [&pkg, &data, &cache] {
        if p.to_string_lossy().contains(',') {
            bail!("plugin folders can't contain commas: {}", p.display());
        }
    }
    let conn = host.connection_values(m);
    let allowed: Vec<String> = m.permissions.net.iter().filter(|n| inst.granted.net.contains(n)).cloned().collect();
    let net = manifest::expand_net(&allowed, &conn);
    let entry = pkg.join(&m.entry);
    let s = sandbox::Sandbox { deno, package: &pkg, entry: &entry, data: &data, cache: &cache, net: &net, extra_args: &[] };
    let mut child = sandbox::command(&s).spawn().context("starting Deno")?;
    let stdin = child.stdin.take().context("no stdin")?;
    let stdout = child.stdout.take().context("no stdout")?;
    if let Some(err) = child.stderr.take() {
        let id = m.id.clone();
        tokio::spawn(async move {
            let mut lines = tokio::io::BufReader::new(err).lines();
            let mut count = 0u32;
            while let Ok(Some(l)) = lines.next_line().await {
                count += 1;
                if count <= 500 {
                    tracing::info!(plugin = %id, "{}", l.chars().take(500).collect::<String>());
                }
            }
        });
    }
    let (peer, mut incoming) = Peer::start(stdout, stdin);
    {
        let mut st = host.state.lock();
        match st.running.get_mut(&m.id) {
            Some(r) if r.generation == generation => r.peer = Some(peer.clone()),
            _ => {
                let _ = child.start_kill();
                return Ok(Run::Stopped);
            }
        }
    }
    let init = json!({
        "hostVersion": manifest::HOST_API,
        "pluginId": m.id,
        "prefix": m.prefix,
        "connection": conn,
        "locale": app.settings.read().locale,
        "dataDir": data.display().to_string(),
    });
    let started = tokio::select! {
        r = peer.request("initialize", init, Duration::from_secs(20)) => r,
        _ = &mut *stop => {
            let _ = child.start_kill();
            return Ok(Run::Stopped);
        }
    };
    started.map_err(anyhow::Error::msg).context("plugin did not start")?;
    sync_one(app, &m.id).await;

    loop {
        tokio::select! {
            msg = incoming.recv() => match msg {
                Some(Incoming::Call { id, method, params }) if method == "oauth.authorize" => {
                    // Waits for the person to sign in, so it must not hold up other calls.
                    let (app, m, peer) = (app.clone(), m.clone(), peer.clone());
                    tokio::spawn(async move {
                        let res = oauth_authorize(&app, &m, params).await;
                        if let Some(id) = id {
                            peer.reply(id, res).await;
                        }
                    });
                }
                Some(Incoming::Call { id, method, params }) => {
                    let res = handle_call(app, m, &method, params).await;
                    if let Some(id) = id {
                        peer.reply(id, res).await;
                    } else if let Err(e) = res {
                        tracing::debug!(plugin = %m.id, "rejected {method}: {e}");
                    }
                }
                Some(Incoming::Violation(v)) => {
                    let _ = child.start_kill();
                    bail!("stopped: {v}");
                }
                Some(Incoming::Closed) | None => break,
            },
            status = child.wait() => {
                let status = status?;
                bail!("exited with {status}");
            }
            _ = &mut *stop => {
                // Let the plugin's stop() run (close sockets, say goodbye), then make sure it's gone.
                peer.notify("shutdown", json!({})).await;
                let _ = tokio::time::timeout(Duration::from_secs(1), child.wait()).await;
                let _ = child.start_kill();
                return Ok(Run::Stopped);
            }
        }
    }
    let _ = tokio::time::timeout(Duration::from_secs(2), child.wait()).await;
    let _ = child.start_kill();
    Ok(Run::Ended)
}

fn text_ok(s: &Option<String>, max: usize) -> bool {
    s.as_deref().is_none_or(|s| s.chars().count() <= max && !s.chars().any(char::is_control))
}

pub(crate) fn valid_image(url: &str) -> bool {
    let types = ["data:image/png;base64,", "data:image/jpeg;base64,", "data:image/webp;base64,", "data:image/gif;base64,", "data:image/svg+xml;base64,"];
    url.len() <= MAX_IMAGE_URL
        && types.iter().any(|t| url.starts_with(t))
        && url.split_once(',').is_some_and(|(_, b)| b.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'+' | b'/' | b'=')))
}

fn check_patch(p: &ViewPatch) -> Result<(), String> {
    if !(text_ok(&Some(p.label.clone().unwrap_or_default()), 64) && text_ok(&p.sub, 64) && text_ok(&p.badge, 24) && text_ok(&p.meta, 32)) {
        return Err("text too long".into());
    }
    if p.icon.as_deref().is_some_and(|i| !valid_icon(i)) {
        return Err("invalid icon".into());
    }
    if p.image.as_deref().is_some_and(|i| !valid_image(i)) {
        return Err("image must be a png, jpeg, webp, gif or svg data URL under 350 KB".into());
    }
    if [p.value, p.progress].iter().flatten().any(|v| !(0.0..=1.0).contains(v)) {
        return Err("value and progress must be between 0 and 1".into());
    }
    if p.since.is_some_and(|s| s > (crate::devices::now() + 86_400) * 1000) {
        return Err("since is in the future".into());
    }
    Ok(())
}

async fn handle_call(app: &Arc<App>, m: &Manifest, method: &str, params: Value) -> Result<Value, String> {
    let host = &app.plugins;
    match method {
        "key.set" => {
            let instance = params.get("instance").and_then(Value::as_str).ok_or("missing instance")?;
            if !host.state.lock().running.get(&m.id).is_some_and(|r| r.instances.contains(instance)) {
                return Err("not your key".into());
            }
            let patch = match params.get("view") {
                None | Some(Value::Null) => None,
                Some(v) => {
                    let p: ViewPatch = serde_json::from_value(v.clone()).map_err(|e| format!("invalid view: {e}"))?;
                    check_patch(&p)?;
                    Some(p)
                }
            };
            app.engine.lock().set_override(instance, patch);
            Ok(Value::Null)
        }
        "var.set" => {
            let name = params.get("name").and_then(Value::as_str).ok_or("missing name")?;
            let full = format!("{}.{name}", m.prefix);
            if !valid_var_name(name) || !valid_var_name(&full) {
                return Err("invalid variable name".into());
            }
            let value = params.get("value").cloned().unwrap_or(Value::Null);
            if !app.engine.lock().set_var(&full, value) {
                return Err("variable rejected".into());
            }
            Ok(Value::Null)
        }
        "feed.push" => {
            let feed = params.get("feed").and_then(Value::as_str).ok_or("missing feed")?;
            if !m.feeds.iter().any(|f| f.name == feed) {
                return Err("feed is not declared in the manifest".into());
            }
            let raw = params.get("item").cloned().unwrap_or(Value::Null);
            let mut item: FeedItem = serde_json::from_value(raw.clone()).map_err(|_| "invalid item")?;
            // Kind, source, roles and reward, for chat triggers. Older plugins send none.
            let details: crate::triggers::Details = serde_json::from_value(raw).unwrap_or_default();
            item.text = item.text.chars().filter(|c| !c.is_control()).take(500).collect();
            item.author = item.author.map(|a| a.chars().filter(|c| !c.is_control()).take(40).collect());
            item.color = item.color.filter(|c| c.len() == 7 && c.starts_with('#') && c[1..].bytes().all(|b| b.is_ascii_hexdigit()));
            item.id = item.id.filter(|id| !id.is_empty() && id.len() <= 100);
            let event = crate::triggers::Event::from_item(&m.prefix, feed, &item, &details);
            app.engine.lock().push_feed(&format!("{}.{feed}", m.prefix), item);
            crate::triggers::handle(app, event);
            Ok(Value::Null)
        }
        "feed.remove" => {
            let feed = params.get("feed").and_then(Value::as_str).ok_or("missing feed")?;
            if !m.feeds.iter().any(|f| f.name == feed) {
                return Err("feed is not declared in the manifest".into());
            }
            let ids = match params.get("ids") {
                None | Some(Value::Null) => Vec::new(),
                Some(Value::Array(list)) if list.len() <= 100 => list.iter().map(|v| v.as_str().map(str::to_string)).collect::<Option<Vec<_>>>().ok_or("invalid ids")?,
                _ => return Err("invalid ids".into()),
            };
            let author = params.get("author").and_then(Value::as_str).map(|a| a.chars().filter(|c| !c.is_control()).take(40).collect::<String>()).filter(|a| !a.is_empty());
            let all = params.get("all").and_then(Value::as_bool).unwrap_or(false);
            app.engine.lock().remove_feed(&format!("{}.{feed}", m.prefix), &FeedMatch { ids, author, all });
            Ok(Value::Null)
        }
        "status.set" => {
            let state = params.get("state").and_then(Value::as_str).filter(|s| ["connected", "connecting", "offline", "error"].contains(s)).ok_or("invalid state")?;
            let detail = params.get("detail").and_then(Value::as_str).map(|d| d.chars().filter(|c| !c.is_control()).take(200).collect());
            let mut e = app.engine.lock();
            e.set_connection(ConnectionView { id: m.id.clone(), name: m.name.clone(), status: state.into(), detail });
            let one = |p: &Map<String, Value>| -> Option<Pill> {
                let label: String = p.get("label").and_then(Value::as_str).unwrap_or("").chars().filter(|c| !c.is_control()).take(24).collect();
                let tone = p.get("tone").cloned().and_then(|t| serde_json::from_value(t).ok()).unwrap_or(Tone::Default);
                let since = p.get("since").and_then(Value::as_u64);
                (!label.is_empty()).then(|| Pill { id: m.id.clone(), label, tone, since })
            };
            match (params.get("pills"), params.get("pill")) {
                (Some(Value::Array(list)), _) => e.set_pills(&m.id, list.iter().filter_map(Value::as_object).filter_map(one).collect()),
                (_, Some(Value::Object(p))) => e.set_pills(&m.id, one(p).into_iter().collect()),
                (_, Some(Value::Null)) => e.set_pills(&m.id, Vec::new()),
                _ => {}
            }
            Ok(Value::Null)
        }
        "toast" => {
            {
                let mut st = host.state.lock();
                let r = st.running.get_mut(&m.id).ok_or("not running")?;
                if r.last_toast.is_some_and(|t| t.elapsed() < Duration::from_secs(1)) {
                    return Err("too many toasts".into());
                }
                r.last_toast = Some(Instant::now());
            }
            let text: String = params.get("text").and_then(Value::as_str).unwrap_or("").chars().filter(|c| !c.is_control()).take(160).collect();
            let tone = params.get("tone").cloned().and_then(|t| serde_json::from_value(t).ok()).unwrap_or(Tone::Default);
            app.engine.lock().toast(format!("{}: {text}", m.name), tone);
            Ok(Value::Null)
        }
        "log" => {
            let msg: String = params.get("message").and_then(Value::as_str).unwrap_or("").chars().take(2000).collect();
            tracing::info!(plugin = %m.id, "{msg}");
            Ok(Value::Null)
        }
        "connection.update" => {
            let values = params.get("values").and_then(Value::as_object).ok_or("missing values")?;
            host.configure(&m.id, values, true).map_err(|e| e.to_string())?;
            Ok(Value::Null)
        }
        "browser.open" => {
            let url = params.get("url").and_then(Value::as_str).ok_or("missing url")?;
            let u = url::Url::parse(url).map_err(|_| "invalid url")?;
            if u.scheme() != "https" || url.len() > 2048 {
                return Err("only https links".into());
            }
            {
                let mut st = host.state.lock();
                let r = st.running.get_mut(&m.id).ok_or("not running")?;
                if r.last_browser.is_some_and(|t| t.elapsed() < Duration::from_secs(10)) {
                    return Err("wait a moment before opening another page".into());
                }
                r.last_browser = Some(Instant::now());
            }
            crate::programs::open(u.as_str()).map_err(|e| e.to_string())?;
            app.engine.lock().toast(format!("{} opened a sign-in page in your browser", m.name), Tone::Default);
            Ok(Value::Null)
        }
        "udp.send" => {
            let dest = params.get("host").and_then(Value::as_str).ok_or("missing host")?;
            let port = params.get("port").and_then(Value::as_u64).filter(|p| (1..=65535).contains(p)).ok_or("invalid port")? as u16;
            let data = params.get("data").and_then(Value::as_str).ok_or("missing data")?;
            let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, data).map_err(|_| "data must be base64")?;
            if bytes.len() > 1400 {
                return Err("datagram too large".into());
            }
            let granted = host.get(&m.id).is_some_and(|i| i.granted.udp) && m.permissions.udp;
            let net = host.granted_net(m);
            let target = format!("{dest}:{port}");
            if !granted || !net.iter().any(|n| *n == target || n == dest) {
                return Err("host not allowed".into());
            }
            crate::actions::core::send_udp(dest.trim_start_matches('[').trim_end_matches(']'), port, &bytes).await?;
            Ok(Value::Null)
        }
        "run" => {
            let argv: Vec<String> = params.get("argv").cloned().and_then(|v| serde_json::from_value(v).ok()).ok_or("missing argv")?;
            // Exact match against the manifest, and only for plugins that ship with Keyside.
            let allowed = m.permissions.run.contains(&argv) && host.get(&m.id).is_some_and(|i| i.source == Source::Bundled && i.granted.run.contains(&argv));
            if !allowed {
                return Err("command not allowed".into());
            }
            run_fixed(&argv).await
        }
        "ipc.open" | "ipc.send" => {
            let bridge = params.get("bridge").and_then(Value::as_str).ok_or("missing bridge")?;
            let granted = host.get(&m.id).is_some_and(|i| i.granted.ipc.iter().any(|g| g == bridge) && m.permissions.ipc.iter().any(|g| g == bridge));
            if !granted || bridge != "discord" {
                return Err("bridge not allowed".into());
            }
            if method == "ipc.open" {
                let b = ipc::Bridge::open_discord(app.clone(), m.id.clone()).await.map_err(|e| e.to_string())?;
                if let Some(r) = host.state.lock().running.get_mut(&m.id) {
                    r.ipc = Some(b);
                }
                Ok(Value::Null)
            } else {
                let data = params.get("data").cloned().ok_or("missing data")?;
                let b = host.state.lock().running.get(&m.id).and_then(|r| r.ipc.clone()).ok_or("bridge not open")?;
                b.send(data).map_err(|e| e.to_string())?;
                Ok(Value::Null)
            }
        }
        _ => Err(format!("unknown method {method}")),
    }
}

/// Runs one of a plugin's fixed commands: no shell, no output. A command still running after
/// a few seconds (say it started the app itself) is left running. The program is also looked
/// for in the usual install folders, since apps started from the desktop get a short PATH.
async fn run_fixed(argv: &[String]) -> Result<Value, String> {
    // A few at a time, so a stuck key can't pile up processes.
    static SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(4);
    let _slot = SLOTS.try_acquire().map_err(|_| "busy, try again")?;
    #[cfg(unix)]
    let program = crate::programs::find(&argv[0]).ok_or("not installed")?;
    #[cfg(not(unix))]
    let program = &argv[0];
    let mut cmd = tokio::process::Command::new(program);
    cmd.args(&argv[1..]).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    crate::programs::apply_tokio(&mut cmd);
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // no console window
    let mut child = cmd.spawn().map_err(|e| if e.kind() == std::io::ErrorKind::NotFound { "not installed".to_string() } else { e.to_string() })?;
    match tokio::time::timeout(Duration::from_secs(5), child.wait()).await {
        Ok(Ok(status)) => Ok(json!({ "code": status.code() })),
        Ok(Err(e)) => Err(e.to_string()),
        Err(_) => {
            tokio::spawn(async move {
                let _ = child.wait().await;
            });
            Ok(json!({ "code": null }))
        }
    }
}

/// Opens a sign-in page and waits for the provider to redirect back to the editor listener.
/// The host sets `redirect_uri` and a random `state`, so a plugin can't point the browser
/// at its own redirect or replay someone else's sign-in.
async fn oauth_authorize(app: &Arc<App>, m: &Manifest, params: Value) -> Result<Value, String> {
    let host = &app.plugins;
    let url = params.get("url").and_then(Value::as_str).ok_or("missing url")?;
    let mut u = url::Url::parse(url).map_err(|_| "invalid url")?;
    if u.scheme() != "https" || url.len() > 4096 {
        return Err("only https links".into());
    }
    {
        let mut st = host.state.lock();
        let r = st.running.get_mut(&m.id).ok_or("not running")?;
        if r.last_browser.is_some_and(|t| t.elapsed() < Duration::from_secs(10)) {
            return Err("wait a moment before opening another page".into());
        }
        r.last_browser = Some(Instant::now());
    }
    let redirect = format!("http://127.0.0.1:{}/oauth/callback", app.settings.read().admin_port);
    let state = crate::ids::random_b64(24);
    let kept: Vec<(String, String)> = u.query_pairs().filter(|(k, _)| k != "state" && k != "redirect_uri").map(|(k, v)| (k.into_owned(), v.into_owned())).collect();
    u.query_pairs_mut().clear().extend_pairs(kept).append_pair("redirect_uri", &redirect).append_pair("state", &state);

    let (tx, rx) = tokio::sync::oneshot::channel();
    {
        let mut waits = host.oauth.lock();
        waits.retain(|_, w| w.plugin != m.id);
        waits.insert(state.clone(), OAuthWait { plugin: m.id.clone(), tx });
    }
    if let Err(e) = crate::programs::open(u.as_str()) {
        host.oauth.lock().remove(&state);
        return Err(e.to_string());
    }
    app.engine.lock().toast(format!("{} opened a sign-in page in your browser", m.name), Tone::Default);
    let res = tokio::time::timeout(OAUTH_TIMEOUT, rx).await;
    host.oauth.lock().remove(&state);
    match res {
        Ok(Ok(Ok(code))) => Ok(json!({ "code": code, "redirectUri": redirect })),
        Ok(Ok(Err(e))) => Err(e),
        Ok(Err(_)) => Err("sign-in was replaced by a newer one".into()),
        Err(_) => Err("sign-in timed out".into()),
    }
}

/// Lets the Discord bridge push messages back to its plugin.
pub(crate) async fn ipc_deliver(app: &App, plugin: &str, method: &str, params: Value) {
    if let Some(p) = app.plugins.peer(plugin) {
        p.notify(method, params).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn images_must_be_real_data_urls() {
        assert!(valid_image("data:image/png;base64,iVBORw0KGgo="));
        assert!(!valid_image("https://evil.example/x.png"));
        assert!(!valid_image("data:text/html;base64,PHNjcmlwdD4="));
        assert!(!valid_image("data:image/png;base64,<script>"));
    }

    #[tokio::test]
    async fn fills_variables_only_where_the_plugin_asks() {
        let data = tempfile::tempdir().unwrap();
        let app = App::new(crate::app::Options { data_dir: Some(data.path().into()), bundled_plugins: None }).unwrap();
        let src = tempfile::tempdir().unwrap();
        let text = |extra: Value| {
            let mut f = json!({ "type": "string", "maxLength": 14 });
            f.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
            f
        };
        let manifest = json!({
            "id": "dev.example.tpl", "name": "Tpl", "version": "1.0.0", "author": "Me", "prefix": "tpl", "host": "^1.0", "entry": "main.js",
            "actions": [{ "id": "go", "name": "Go", "settings": { "type": "object", "properties": {
                "filled": text(json!({ "x-template": true })),
                "plain": text(json!({})),
                "args": { "type": "object", "additionalProperties": { "type": "string" }, "x-template": true }
            } } }]
        });
        std::fs::write(src.path().join("manifest.json"), manifest.to_string()).unwrap();
        std::fs::write(src.path().join("main.js"), "").unwrap();
        let pkg = package::Package::from_dir(src.path()).unwrap();
        app.plugins.install(&pkg, Source::File, false).unwrap();
        // No key uses it: listed and reported, but not started.
        assert_eq!(app.plugins.list()[0].status, "idle");
        assert_eq!(app.plugins.health()["dev.example.tpl"]["inUse"], false);
        assert!(app.plugins.state.lock().running.is_empty());
        app.engine.lock().set_var("user.who", json!("Fernando the Great"));
        let settings = json!({ "filled": "hi {user.who}", "plain": "{user.who}", "args": { "a": "{user.who}!", "b": "{nope}" } });
        let step = Step { instance: "i".into(), action: "go".into(), settings: settings.as_object().unwrap().clone() };
        let out = app.plugins.clean_steps("dev.example.tpl", vec![step]).unwrap().remove(0).settings;
        assert_eq!(out["filled"], "hi Fernando th", "capped at the field's length");
        assert_eq!(out["plain"], "{user.who}");
        assert_eq!(out["args"], json!({ "a": "Fernando the Great!", "b": "" }));
    }

    #[tokio::test]
    async fn only_bundled_plugins_may_run_commands() {
        let data = tempfile::tempdir().unwrap();
        let app = App::new(crate::app::Options { data_dir: Some(data.path().into()), bundled_plugins: None }).unwrap();
        let src = tempfile::tempdir().unwrap();
        let manifest = json!({
            "id": "dev.example.runner", "name": "Runner", "version": "1.0.0", "author": "Me", "prefix": "runner",
            "host": "^1.0", "entry": "main.js", "actions": [], "permissions": { "run": [["equibop", "--toggle-mic"]] }
        });
        std::fs::write(src.path().join("manifest.json"), manifest.to_string()).unwrap();
        std::fs::write(src.path().join("main.js"), "").unwrap();
        let pkg = package::Package::from_dir(src.path()).unwrap();
        for source in [Source::File, Source::Registry] {
            let e = app.plugins.install(&pkg, source, true).unwrap_err().to_string();
            assert!(e.contains("ship with Keyside"), "{e}");
        }
        assert!(app.plugins.install(&pkg, Source::Bundled, false).is_ok());
    }

    #[tokio::test]
    async fn plugins_cannot_change_the_addresses_they_may_reach() {
        let data = tempfile::tempdir().unwrap();
        let app = App::new(crate::app::Options { data_dir: Some(data.path().into()), bundled_plugins: None }).unwrap();
        let src = tempfile::tempdir().unwrap();
        let manifest = json!({
            "id": "dev.example.net", "name": "Net", "version": "1.0.0", "author": "Me", "prefix": "net", "host": "^1.0", "entry": "main.js", "actions": [],
            "permissions": { "net": ["{host}:{port}"] },
            "connection": { "type": "object", "properties": {
                "host": { "type": "string", "default": "127.0.0.1" }, "port": { "type": "integer", "default": 4455 }, "token": { "type": "string", "format": "secret" } } }
        });
        std::fs::write(src.path().join("manifest.json"), manifest.to_string()).unwrap();
        std::fs::write(src.path().join("main.js"), "").unwrap();
        app.plugins.install(&package::Package::from_dir(src.path()).unwrap(), Source::File, true).unwrap();
        let set = |v: Value| v.as_object().unwrap().clone();
        assert!(app.plugins.configure("dev.example.net", &set(json!({ "host": "exfil.example" })), true).is_err());
        assert!(app.plugins.configure("dev.example.net", &set(json!({ "token": "t" })), true).is_ok(), "tokens are fine");
        assert!(app.plugins.configure("dev.example.net", &set(json!({ "host": "10.0.0.2" })), false).is_ok(), "the person can");
    }

    #[test]
    fn finds_plugins_by_steps_and_by_the_variables_keys_show() {
        let mut l = crate::state::default_layout();
        let prefixes =
            vec![("dev.keyside.obs".to_string(), "obs".to_string()), ("dev.keyside.twitch".to_string(), "twitch".to_string()), ("dev.keyside.hue".to_string(), "hue".to_string())];
        let used = used_by(&l, &prefixes);
        assert!(used.contains("dev.keyside.obs"));
        assert!(!used.contains("dev.keyside.hue"));
        let k = &mut l.profiles[0].pages[0].keys[0];
        k.label = "{hue.lights} on".into();
        k.actions.clear();
        assert!(used_by(&l, &prefixes).contains("dev.keyside.hue"));
        for p in &mut l.profiles {
            for pg in &mut p.pages {
                for k in &mut pg.keys {
                    for a in &mut k.actions {
                        a.review = true;
                    }
                }
            }
        }
        assert!(!used_by(&l, &prefixes).contains("dev.keyside.obs"), "steps waiting for review can't run");
    }

    #[test]
    fn patches_are_bounded() {
        let mut p = ViewPatch { label: Some("ok".into()), ..Default::default() };
        assert!(check_patch(&p).is_ok());
        p.value = Some(1.5);
        assert!(check_patch(&p).is_err());
        let p = ViewPatch { badge: Some("x".repeat(30)), ..Default::default() };
        assert!(check_patch(&p).is_err());
        assert!(serde_json::from_value::<ViewPatch>(json!({"onclick": "x"})).is_err());
    }
}

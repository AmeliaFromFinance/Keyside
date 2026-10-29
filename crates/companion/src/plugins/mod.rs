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
use crate::state::view::{FeedItem, ViewPatch};
use crate::state::{ConnectionView, Pill};
use crate::store;
use manifest::{Manifest, Permissions};
use rpc::{Incoming, Peer};

const RUN_TIMEOUT: Duration = Duration::from_secs(15);
const OPTIONS_TIMEOUT: Duration = Duration::from_secs(8);
const MAX_IMAGE_URL: usize = 350 * 1024;
const CRASH_WINDOW: Duration = Duration::from_secs(300);
const CRASH_LIMIT: usize = 5;

#[derive(Debug, Clone, Serialize)]
pub struct Step {
    pub instance: String,
    pub action: String,
    pub settings: Map<String, Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// Shipped with PhoneDeck.
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
    pub source: Source,
    pub signer: Option<String>,
    pub requested: Permissions,
    pub granted: Permissions,
    pub needs_approval: bool,
    pub connection: Option<manifest::Schema>,
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
        };
        let installed: BTreeMap<String, Installed> = store::read_json(&host.index)?.unwrap_or_default();
        host.state.lock().installed = installed.into_iter().filter(|(_, i)| manifest::validate(&i.manifest).is_ok()).collect();
        if host.deno.is_none() {
            tracing::warn!("Deno runtime not found; plugins can't run. Set PHONEDECK_DENO or install Deno.");
        }
        Ok(host)
    }

    pub fn attach(&self, app: &Arc<App>) {
        let _ = self.app.set(Arc::downgrade(app));
    }

    fn app(&self) -> Option<Arc<App>> {
        self.app.get()?.upgrade()
    }

    fn save(&self, st: &State) -> Result<()> {
        store::write_json(&self.index, &st.installed)
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
        // Engine first, then plugin state: never hold both locks at once.
        let statuses: HashMap<String, String> = self
            .app()
            .map(|a| a.engine.lock().status().0.into_iter().map(|c| (c.id, c.status)).collect())
            .unwrap_or_default();
        let st = self.state.lock();
        st.installed
            .values()
            .map(|i| {
                let m = &i.manifest;
                let status = match statuses.get(&m.id) {
                    Some(s) if i.enabled => s.clone(),
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
                    source: i.source,
                    signer: i.signer.clone(),
                    requested: m.permissions.clone(),
                    granted: i.granted.clone(),
                    needs_approval: i.needs_approval(),
                    connection: m.connection.clone(),
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
        {
            let st = self.state.lock();
            if let Some(other) = st.installed.values().find(|i| i.manifest.prefix == m.prefix && i.manifest.id != m.id) {
                bail!("prefix \"{}\" is already used by {}", m.prefix, other.manifest.name);
            }
            if let Some(old) = st.installed.get(&m.id)
                && let (Some(a), Some(b)) = (&old.signer, &pkg.signer)
                && a != b
                && source != Source::Bundled
            {
                bail!("this update is signed by a different key than the installed version");
            }
        }
        let dir = self.package_dir(&m.id, &m.version);
        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
        }
        pkg.extract_to(&dir)?;
        let previous = self.get(&m.id);
        self.stop(&m.id);
        let granted = if approve || source == Source::Bundled {
            m.permissions.clone()
        } else {
            previous.as_ref().map(|p| p.granted.clone()).unwrap_or_default()
        };
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
            let pkg = if p.is_dir() { package::Package::from_dir(&p) } else if p.extension().is_some_and(|x| x == "pdp") { std::fs::read(&p).map_err(Into::into).and_then(|b| package::Package::from_zip(&b)) } else { continue };
            let pkg = match pkg {
                Ok(p) => p,
                Err(err) => {
                    tracing::warn!("skipping bundled plugin {}: {err:#}", p.display());
                    continue;
                }
            };
            let newer = self.get(&pkg.manifest.id).is_none_or(|i| {
                i.source == Source::Bundled
                    && semver::Version::parse(&i.manifest.version).ok() < semver::Version::parse(&pkg.manifest.version).ok()
            });
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
        if enabled { self.start(id) } else { self.stop(id) }
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
        let mut merged = self.connection_values(&m);
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
        let net_changed = {
            let before = manifest::expand_net(&m.permissions.net, &merged);
            before != manifest::expand_net(&m.permissions.net, &self.connection_values(&m))
        };
        if from_plugin && !net_changed {
            return Ok(());
        }
        // Network permissions come from these values, so the process restarts with the new set.
        self.restart(id);
        Ok(())
    }

    pub fn refresh_engine(&self) {
        if let Some(app) = self.app() {
            let ids: HashSet<String> = self.state.lock().installed.iter().filter(|(_, i)| i.enabled).map(|(k, _)| k.clone()).collect();
            let mut e = app.engine.lock();
            e.set_installed(ids);
            e.notify(crate::state::Change::Plugins);
        }
    }

    pub fn start_all(&self) {
        let ids: Vec<String> = self.state.lock().installed.iter().filter(|(_, i)| i.enabled).map(|(k, _)| k.clone()).collect();
        self.refresh_engine();
        for id in ids {
            self.start(&id);
        }
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
            let mine: HashSet<String> = e.instances_for(id).into_iter().map(|i| i.id).collect();
            e.remove_connection(id);
            e.clear_overrides_with_prefix(|inst| mine.contains(inst));
        }
    }

    pub fn start(&self, id: &str) {
        let Some(app) = self.app() else { return };
        let Some(inst) = self.get(id) else { return };
        if !inst.enabled {
            return;
        }
        if inst.needs_approval() {
            app.engine.lock().set_connection(ConnectionView { id: id.into(), name: inst.manifest.name.clone(), status: "error".into(), detail: Some("needs permission approval".into()) });
            return;
        }
        let Some(deno) = self.deno.clone() else {
            app.engine.lock().set_connection(ConnectionView { id: id.into(), name: inst.manifest.name.clone(), status: "error".into(), detail: Some("Deno runtime missing".into()) });
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
        let peer = self.peer(plugin).ok_or_else(|| format!("{} is not running", self.get(plugin).map(|i| i.manifest.name).unwrap_or_else(|| plugin.into())))?;
        let steps = self.clean_steps(plugin, steps)?;
        peer.request("run", json!({ "steps": steps, "phase": phase, "device": device }), RUN_TIMEOUT).await.map(|_| ())
    }

    fn clean_steps(&self, plugin: &str, steps: Vec<Step>) -> Result<Vec<Step>, String> {
        let st = self.state.lock();
        let m = &st.installed.get(plugin).ok_or("plugin not installed")?.manifest;
        steps
            .into_iter()
            .map(|mut s| {
                let def = m.actions.iter().find(|a| a.id == s.action).ok_or_else(|| format!("{} has no action \"{}\"", m.name, s.action))?;
                s.settings = manifest::clean_values(def.settings.as_ref(), &s.settings)?;
                Ok(s)
            })
            .collect()
    }

    pub async fn slide(&self, plugin: &str, step: Step, value: f64) {
        let Some(peer) = self.peer(plugin) else { return };
        if let Ok(mut v) = self.clean_steps(plugin, vec![step]) {
            let s = v.remove(0);
            peer.notify("slide", json!({ "instance": s.instance, "action": s.action, "settings": s.settings, "value": value.clamp(0.0, 1.0) })).await;
        }
    }

    pub async fn options(&self, plugin: &str, action: &str, field: &str, settings: &Map<String, Value>) -> Result<Value, String> {
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
        let ids: Vec<String> = self.state.lock().running.keys().cloned().collect();
        for id in ids {
            sync_one(&app, &id).await;
        }
    }
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
        app.engine.lock().set_connection(ConnectionView { id: id.clone(), name: name.clone(), status: "connecting".into(), detail: None });

        let outcome = tokio::select! {
            r = run_once(&app, &inst, generation, &deno) => r,
            _ = &mut stop => {
                return;
            }
        };
        if !current(&app) {
            return;
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

async fn run_once(app: &Arc<App>, inst: &Installed, generation: u64, deno: &Path) -> Result<()> {
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
                return Ok(());
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
    peer.request("initialize", init, Duration::from_secs(20)).await.map_err(anyhow::Error::msg).context("plugin did not start")?;
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
        }
    }
    let _ = tokio::time::timeout(Duration::from_secs(2), child.wait()).await;
    let _ = child.start_kill();
    Ok(())
}

fn text_ok(s: &Option<String>, max: usize) -> bool {
    s.as_deref().is_none_or(|s| s.chars().count() <= max && !s.chars().any(char::is_control))
}

fn valid_image(url: &str) -> bool {
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
            let mut item: FeedItem = serde_json::from_value(params.get("item").cloned().unwrap_or(Value::Null)).map_err(|_| "invalid item")?;
            item.text = item.text.chars().filter(|c| !c.is_control()).take(500).collect();
            item.author = item.author.map(|a| a.chars().filter(|c| !c.is_control()).take(40).collect());
            item.color = item.color.filter(|c| c.len() == 7 && c.starts_with('#') && c[1..].bytes().all(|b| b.is_ascii_hexdigit()));
            app.engine.lock().push_feed(&format!("{}.{feed}", m.prefix), item);
            Ok(Value::Null)
        }
        "status.set" => {
            let state = params.get("state").and_then(Value::as_str).filter(|s| ["connected", "connecting", "offline", "error"].contains(s)).ok_or("invalid state")?;
            let detail = params.get("detail").and_then(Value::as_str).map(|d| d.chars().filter(|c| !c.is_control()).take(120).collect());
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
            open::that_detached(u.as_str()).map_err(|e| e.to_string())?;
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
                b.send(data).await.map_err(|e| e.to_string())?;
                Ok(Value::Null)
            }
        }
        _ => Err(format!("unknown method {method}")),
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
    if let Err(e) = open::that_detached(u.as_str()) {
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

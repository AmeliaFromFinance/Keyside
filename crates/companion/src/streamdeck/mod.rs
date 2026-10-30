//! Stream Deck plugin compatibility. Plugins made for Elgato's Stream Deck app run here
//! unchanged: JavaScript plugins inside the Deno sandbox, native programs only after the
//! person agrees to run them unsandboxed. Their settings panels (property inspectors) open
//! in the desktop editor from a separate origin.
//!
//! Action types look like "sd.<plugin uuid>/<action uuid>".

pub mod hardware;
mod inspector;
pub mod manifest;
mod runtime;

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use tokio::sync::mpsc;

use crate::app::App;
use crate::model::{LayoutFile, Tone};
use crate::plugins::Step;
use crate::plugins::package::safe_path;
use crate::state::all_keys;
use crate::state::view::ViewPatch;
use crate::store;
use manifest::{Runtime, SdManifest};

pub const PREFIX: &str = "sd.";
const STAGE_TTL: Duration = Duration::from_secs(600);
const MAX_SETTINGS: usize = crate::model::MAX_ACTION_SETTINGS;
const MAX_IMAGE: u64 = 300 * 1024;
const FLASH: Duration = Duration::from_millis(1500);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Installed {
    pub uuid: String,
    pub manifest: SdManifest,
    pub enabled: bool,
    /// Hosts a JavaScript plugin may reach besides Keyside itself.
    #[serde(default)]
    pub net: Vec<String>,
    /// Any host at all. Shown with a warning in the editor.
    #[serde(default)]
    pub all_net: bool,
    /// The person agreed to run a native plugin with full rights.
    #[serde(default)]
    pub unsandboxed: bool,
    pub installed_at: u64,
}

impl Installed {
    pub fn plugin_id(&self) -> String {
        format!("{PREFIX}{}", self.uuid)
    }

    /// Why it can't run, if it can't.
    pub fn blocked(&self, have_deno: bool) -> Option<&'static str> {
        match self.manifest.runtime() {
            None => Some("This plugin has no version for this system"),
            Some(Runtime::Html) => Some("Web-page plugins need Elgato's app and can't run in Keyside"),
            Some(Runtime::Native) if !self.unsandboxed => Some("Needs your permission to run unsandboxed"),
            Some(Runtime::Node) if !have_deno => Some("The plugin runtime (Deno) is missing"),
            _ => None,
        }
    }
}

/// Look of one key, as the plugin set it.
#[derive(Debug, Clone, Default)]
struct KeyState {
    title: Option<String>,
    image: Option<String>,
    state: usize,
    flash: Option<(bool, Instant)>,
}

pub(crate) type Outbox = mpsc::Sender<String>;

struct Running {
    generation: u64,
    token: String,
    tx: Option<Outbox>,
    /// Placed keys: instance id -> (action uuid, settings).
    instances: HashMap<String, (String, Map<String, Value>)>,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    crashes: VecDeque<Instant>,
    last_url: Option<Instant>,
    /// Token bucket for settings writes, which go to disk.
    writes: (f64, Instant),
}

/// Settings writes allowed per second for one plugin, with a short burst.
const WRITES_PER_SEC: f64 = 10.0;
const WRITE_BURST: f64 = 30.0;

fn write_allowed(app: &App, uuid: &str) -> bool {
    let mut st = app.streamdeck.state.lock();
    let Some(r) = st.running.get_mut(uuid) else { return false };
    let now = Instant::now();
    let (tokens, last) = r.writes;
    let tokens = (tokens + now.duration_since(last).as_secs_f64() * WRITES_PER_SEC).min(WRITE_BURST);
    r.writes = (tokens - 1.0, now);
    tokens >= 1.0
}

struct Inspector {
    plugin: String,
    context: String,
    action: String,
    tx: Option<Outbox>,
    created: Instant,
}

#[derive(Default)]
struct State {
    installed: BTreeMap<String, Installed>,
    running: HashMap<String, Running>,
    inspectors: HashMap<String, Inspector>,
    views: HashMap<String, KeyState>,
    generation: u64,
}

pub struct Hub {
    root: PathBuf,
    state: Mutex<State>,
    staged: Mutex<HashMap<String, (manifest::SdPackage, Instant)>>,
    images: Mutex<HashMap<PathBuf, Option<String>>>,
    port: OnceLock<u16>,
    inspector_port: OnceLock<u16>,
    admin_port: OnceLock<u16>,
}

impl Hub {
    pub fn open(root: &Path) -> Result<Self> {
        let root = root.join("streamdeck");
        store::private_dir(&root)?;
        let installed: BTreeMap<String, Installed> = store::read_json_or_reset(&root.join("plugins.json"))?.unwrap_or_default();
        Ok(Self {
            root,
            state: Mutex::new(State { installed, ..Default::default() }),
            staged: Mutex::new(HashMap::new()),
            images: Mutex::new(HashMap::new()),
            port: OnceLock::new(),
            inspector_port: OnceLock::new(),
            admin_port: OnceLock::new(),
        })
    }

    fn save(&self, st: &State) -> Result<()> {
        store::write_json(&self.root.join("plugins.json"), &st.installed)
    }

    pub fn plugin_dir(&self, uuid: &str) -> PathBuf {
        self.root.join("plugins").join(uuid)
    }

    fn data_dir(&self, uuid: &str) -> PathBuf {
        self.root.join("data").join(uuid)
    }

    fn cache_dir(&self, uuid: &str) -> PathBuf {
        self.root.join("cache").join(uuid)
    }

    pub fn get(&self, uuid: &str) -> Option<Installed> {
        self.state.lock().installed.get(uuid).cloned()
    }

    /// Plugin ids ("sd.<uuid>") that are on, for the "missing plugin" check on keys.
    pub fn enabled_ids(&self) -> Vec<String> {
        self.state.lock().installed.values().filter(|i| i.enabled).map(Installed::plugin_id).collect()
    }

    /// Entries for the editor palette, in the same shape as Keyside plugins.
    pub fn catalog(&self) -> Vec<Value> {
        let st = self.state.lock();
        st.installed
            .values()
            .filter(|i| i.enabled)
            .map(|i| {
                json!({
                    "id": i.plugin_id(),
                    "name": i.manifest.name,
                    "prefix": "sd",
                    "icon": "puzzle",
                    "running": st.running.get(&i.uuid).is_some_and(|r| r.tx.is_some()),
                    "streamdeck": true,
                    "actions": i.manifest.actions.iter().map(|a| json!({
                        "id": a.uuid,
                        "name": a.name,
                        "icon": "puzzle",
                        "description": a.tooltip,
                    })).collect::<Vec<_>>(),
                })
            })
            .collect()
    }

    fn global_path(&self, uuid: &str) -> PathBuf {
        self.root.join("global").join(format!("{uuid}.json"))
    }

    fn global_settings(&self, uuid: &str) -> Value {
        store::read_json::<Value>(&self.global_path(uuid)).ok().flatten().unwrap_or_else(|| json!({}))
    }
}

/// Reads a key image shipped in the plugin folder. Stream Deck paths leave out the
/// extension and may have an @2x variant.
fn plugin_image(app: &App, uuid: &str, base: &str) -> Option<String> {
    if !safe_path(base) {
        return None;
    }
    let dir = app.streamdeck.plugin_dir(uuid);
    let key = dir.join(base);
    if let Some(hit) = app.streamdeck.images.lock().get(&key) {
        return hit.clone();
    }
    let found = (|| {
        let real_dir = dir.canonicalize().ok()?;
        for suffix in ["@2x.png", ".png", ".svg", ".jpg", ".jpeg", ".gif", ".webp", ""] {
            let p = dir.join(format!("{base}{suffix}"));
            let Ok(real) = p.canonicalize() else { continue };
            if !real.starts_with(&real_dir) || !real.is_file() || real.metadata().ok()?.len() > MAX_IMAGE {
                continue;
            }
            let mime = match real.extension()?.to_str()?.to_ascii_lowercase().as_str() {
                "png" => "image/png",
                "svg" => "image/svg+xml",
                "jpg" | "jpeg" => "image/jpeg",
                "gif" => "image/gif",
                "webp" => "image/webp",
                _ => continue,
            };
            let bytes = std::fs::read(&real).ok()?;
            return Some(format!("data:{mime};base64,{}", base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes)));
        }
        None
    })();
    let mut cache = app.streamdeck.images.lock();
    if cache.len() > 500 {
        cache.clear();
    }
    cache.insert(key, found.clone());
    found
}

/// setImage accepts base64 data URLs, raw SVG data URLs and paths inside the plugin.
fn image_from_plugin(app: &App, uuid: &str, raw: &str) -> Option<String> {
    if raw.is_empty() {
        return None;
    }
    if let Some(rest) =
        raw.strip_prefix("data:image/svg+xml;charset=utf8,").or_else(|| raw.strip_prefix("data:image/svg+xml;charset=utf-8,")).or_else(|| raw.strip_prefix("data:image/svg+xml,"))
    {
        let svg: String = url::form_urlencoded::parse(format!("x={}", rest.replace('+', "%2B")).as_bytes()).map(|(_, v)| v.into_owned()).next().unwrap_or_default();
        let url = format!("data:image/svg+xml;base64,{}", base64::Engine::encode(&base64::engine::general_purpose::STANDARD, svg));
        return crate::plugins::valid_image(&url).then_some(url);
    }
    if raw.starts_with("data:") {
        return crate::plugins::valid_image(raw).then(|| raw.to_string());
    }
    plugin_image(app, uuid, raw.trim_start_matches("./"))
}

/// Parses "profile/page/key/n". Off actions ("o<n>") aren't driven by Stream Deck plugins.
fn split_instance(id: &str) -> Option<(&str, &str, &str, usize)> {
    let mut it = id.rsplitn(4, '/');
    let n = it.next()?.parse().ok()?;
    let key = it.next()?;
    let page = it.next()?;
    let profile = it.next()?;
    Some((profile, page, key, n))
}

fn coordinates(app: &App, instance: &str) -> Value {
    let pos = split_instance(instance).and_then(|(p, pg, k, _)| app.engine.lock().key(p, pg, k).map(|k| (k.x, k.y)));
    let (x, y) = pos.unwrap_or((0, 0));
    json!({ "column": x, "row": y })
}

fn event_payload(app: &App, instance: &str, settings: &Map<String, Value>) -> Value {
    let state = app.streamdeck.state.lock().views.get(instance).map(|v| v.state).unwrap_or(0);
    json!({ "settings": settings, "coordinates": coordinates(app, instance), "state": state, "isInMultiAction": false, "controller": "Keypad" })
}

/// Recomputes the key override from what the plugin set and the action's default states.
fn render(app: &App, uuid: &str, instance: &str) {
    let Some(inst) = app.streamdeck.get(uuid) else { return };
    let action_uuid = instance_action(app, instance);
    let ks = app.streamdeck.state.lock().views.get(instance).cloned().unwrap_or_default();
    let def = action_uuid.as_deref().and_then(|a| inst.manifest.action(a));
    let state_def = def.and_then(|d| d.states.get(ks.state).or(d.states.first())).cloned().unwrap_or_default();
    let mut p = ViewPatch::default();
    let title = ks.title.clone().or(state_def.title.clone()).filter(|t| !t.trim().is_empty());
    p.label = title.map(|t| t.replace(['\n', '\r'], " ").chars().take(64).collect());
    p.image = ks.image.clone().or_else(|| state_def.image.as_deref().and_then(|i| plugin_image(app, uuid, i)));
    if let Some((ok, at)) = ks.flash
        && at.elapsed() < FLASH
    {
        p.badge = Some(if ok { "OK" } else { "!" }.into());
        p.badge_tone = Some(if ok { Tone::Success } else { Tone::Danger });
        if !ok {
            p.tone = Some(Tone::Danger);
        }
    }
    let empty = p.label.is_none() && p.image.is_none() && p.badge.is_none();
    app.engine.lock().set_override(instance, if empty { None } else { Some(p) });
}

fn instance_action(app: &App, instance: &str) -> Option<String> {
    let (p, pg, k, n) = split_instance(instance)?;
    let e = app.engine.lock();
    let a = e.key(p, pg, k)?.actions.get(n)?;
    a.plugin().filter(|(id, _)| id.starts_with(PREFIX)).map(|(_, act)| act.to_string())
}

fn send(tx: &Option<Outbox>, msg: Value) {
    if let Some(tx) = tx
        && tx.try_send(msg.to_string()).is_err()
    {
        tracing::debug!("stream deck message dropped (queue full or closed)");
    }
}

fn to_plugin(app: &App, uuid: &str, msg: Value) {
    let tx = app.streamdeck.state.lock().running.get(uuid).and_then(|r| r.tx.clone());
    send(&tx, msg);
}

fn to_inspectors(app: &App, uuid: &str, context: Option<&str>, msg: Value) {
    let txs: Vec<Outbox> =
        app.streamdeck.state.lock().inspectors.values().filter(|i| i.plugin == uuid && context.is_none_or(|c| c == i.context)).filter_map(|i| i.tx.clone()).collect();
    for tx in txs {
        send(&Some(tx), msg.clone());
    }
}

/// Saves settings for one key from the plugin or its settings panel.
fn store_settings(app: &App, uuid: &str, context: &str, payload: &Value) -> Result<Map<String, Value>, String> {
    if !write_allowed(app, uuid) {
        return Err("too many settings changes".into());
    }
    let settings = payload.as_object().cloned().ok_or("settings must be an object")?;
    if serde_json::to_vec(&settings).map(|v| v.len()).unwrap_or(usize::MAX) > MAX_SETTINGS {
        return Err("settings are larger than 64 KB".into());
    }
    if !app.engine.lock().set_action_settings(context, &format!("{PREFIX}{uuid}/"), settings.clone()) {
        return Err("key not found".into());
    }
    if let Some(r) = app.streamdeck.state.lock().running.get_mut(uuid)
        && let Some(entry) = r.instances.get_mut(context)
    {
        entry.1 = settings.clone();
    }
    Ok(settings)
}

fn store_global(app: &App, uuid: &str, payload: &Value) -> Result<(), String> {
    if !write_allowed(app, uuid) {
        return Err("too many settings changes".into());
    }
    if !payload.is_object() || payload.to_string().len() > MAX_SETTINGS {
        return Err("global settings must be an object under 64 KB".into());
    }
    let path = app.streamdeck.global_path(uuid);
    if let Some(dir) = path.parent() {
        store::private_dir(dir).map_err(|e| e.to_string())?;
    }
    store::write_json_private(&path, payload).map_err(|e| e.to_string())
}

fn open_url(app: &App, uuid: &str, url: &str) {
    let ok = url::Url::parse(url).is_ok_and(|u| matches!(u.scheme(), "https" | "http")) && url.len() <= 2048;
    let allowed = {
        let mut st = app.streamdeck.state.lock();
        match st.running.get_mut(uuid) {
            Some(r) if r.last_url.is_none_or(|t| t.elapsed() > Duration::from_secs(5)) => {
                r.last_url = Some(Instant::now());
                true
            }
            _ => false,
        }
    };
    if ok && allowed && crate::programs::open(url).is_ok() {
        let name = app.streamdeck.get(uuid).map(|i| i.manifest.name).unwrap_or_default();
        app.engine.lock().toast(format!("{name} opened a page in your browser"), Tone::Default);
    }
}

/// A message from a plugin process. `context` must be one of its own keys.
pub(crate) fn from_plugin(app: &App, uuid: &str, msg: &Value) {
    let event = msg.get("event").and_then(Value::as_str).unwrap_or("");
    let context = msg.get("context").and_then(Value::as_str).unwrap_or("");
    let payload = msg.get("payload").cloned().unwrap_or(Value::Null);
    let owns = || app.streamdeck.state.lock().running.get(uuid).is_some_and(|r| r.instances.contains_key(context));
    match event {
        "setTitle" | "setImage" | "setState" | "showAlert" | "showOk" => {
            if !owns() {
                return;
            }
            let image = (event == "setImage").then(|| payload.get("image").and_then(Value::as_str).and_then(|i| image_from_plugin(app, uuid, i)));
            let states = instance_action(app, context).and_then(|a| app.streamdeck.get(uuid).and_then(|i| i.manifest.action(&a).map(|d| d.states.len()))).unwrap_or(1);
            {
                let mut st = app.streamdeck.state.lock();
                let ks = st.views.entry(context.to_string()).or_default();
                match event {
                    "setTitle" => ks.title = payload.get("title").and_then(Value::as_str).map(|t| t.chars().take(64).collect()),
                    "setImage" => ks.image = image.flatten(),
                    "setState" => ks.state = payload.get("state").and_then(Value::as_u64).map(|s| (s as usize).min(states.saturating_sub(1))).unwrap_or(0),
                    _ => ks.flash = Some((event == "showOk", Instant::now())),
                }
            }
            render(app, uuid, context);
            if (event == "showAlert" || event == "showOk")
                && let Some(app) = app.engine_handle()
            {
                let (uuid, context) = (uuid.to_string(), context.to_string());
                tokio::spawn(async move {
                    tokio::time::sleep(FLASH).await;
                    render(&app, &uuid, &context);
                });
            }
        }
        "setSettings" => {
            if owns()
                && let Ok(settings) = store_settings(app, uuid, context, &payload)
            {
                let action = instance_action(app, context).unwrap_or_default();
                to_inspectors(
                    app,
                    uuid,
                    Some(context),
                    json!({ "event": "didReceiveSettings", "action": action, "context": context, "device": runtime::DEVICE, "payload": event_payload(app, context, &settings) }),
                );
            }
        }
        "getSettings" => {
            if !owns() {
                return;
            }
            let settings = app.streamdeck.state.lock().running.get(uuid).and_then(|r| r.instances.get(context).map(|(_, s)| s.clone())).unwrap_or_default();
            let action = instance_action(app, context).unwrap_or_default();
            to_plugin(
                app,
                uuid,
                json!({ "event": "didReceiveSettings", "action": action, "context": context, "device": runtime::DEVICE, "payload": event_payload(app, context, &settings) }),
            );
        }
        "setGlobalSettings" => {
            if store_global(app, uuid, &payload).is_ok() {
                to_inspectors(app, uuid, None, json!({ "event": "didReceiveGlobalSettings", "payload": { "settings": payload } }));
            }
        }
        "getGlobalSettings" => to_plugin(app, uuid, json!({ "event": "didReceiveGlobalSettings", "payload": { "settings": app.streamdeck.global_settings(uuid) } })),
        "openUrl" => open_url(app, uuid, payload.get("url").and_then(Value::as_str).unwrap_or("")),
        "logMessage" => {
            let m: String = payload.get("message").and_then(Value::as_str).unwrap_or("").chars().take(500).collect();
            tracing::info!(plugin = %uuid, "{m}");
        }
        "sendToPropertyInspector" if owns() => {
            let action = instance_action(app, context).unwrap_or_default();
            to_inspectors(app, uuid, Some(context), json!({ "event": "sendToPropertyInspector", "action": action, "context": context, "payload": payload }));
        }
        _ => {}
    }
}

/// A message from a settings panel. Its key is fixed when the panel opens.
pub(crate) fn from_inspector(app: &App, token: &str, msg: &Value) {
    let Some((uuid, context, action)) = app.streamdeck.state.lock().inspectors.get(token).map(|i| (i.plugin.clone(), i.context.clone(), i.action.clone())) else { return };
    let payload = msg.get("payload").cloned().unwrap_or(Value::Null);
    match msg.get("event").and_then(Value::as_str).unwrap_or("") {
        "setSettings" => {
            if let Ok(settings) = store_settings(app, &uuid, &context, &payload) {
                to_plugin(
                    app,
                    &uuid,
                    json!({ "event": "didReceiveSettings", "action": action, "context": context, "device": runtime::DEVICE, "payload": event_payload(app, &context, &settings) }),
                );
            }
        }
        "getSettings" => {
            let settings = current_settings(app, &context);
            let tx = app.streamdeck.state.lock().inspectors.get(token).and_then(|i| i.tx.clone());
            send(
                &tx,
                json!({ "event": "didReceiveSettings", "action": action, "context": context, "device": runtime::DEVICE, "payload": event_payload(app, &context, &settings) }),
            );
        }
        "sendToPlugin" => to_plugin(app, &uuid, json!({ "event": "sendToPlugin", "action": action, "context": context, "payload": payload })),
        "setGlobalSettings" => {
            if store_global(app, &uuid, &payload).is_ok() {
                to_plugin(app, &uuid, json!({ "event": "didReceiveGlobalSettings", "payload": { "settings": payload } }));
            }
        }
        "getGlobalSettings" => {
            let tx = app.streamdeck.state.lock().inspectors.get(token).and_then(|i| i.tx.clone());
            send(&tx, json!({ "event": "didReceiveGlobalSettings", "payload": { "settings": app.streamdeck.global_settings(&uuid) } }));
        }
        "openUrl" => open_url(app, &uuid, payload.get("url").and_then(Value::as_str).unwrap_or("")),
        "logMessage" => {
            let m: String = payload.get("message").and_then(Value::as_str).unwrap_or("").chars().take(500).collect();
            tracing::info!(plugin = %uuid, "settings panel: {m}");
        }
        _ => {}
    }
}

fn current_settings(app: &App, instance: &str) -> Map<String, Value> {
    split_instance(instance).and_then(|(p, pg, k, n)| app.engine.lock().key(p, pg, k).and_then(|k| k.actions.get(n)).map(|a| a.settings.clone())).unwrap_or_default()
}

/// Brings plugins up to date with the layout: willAppear, willDisappear, didReceiveSettings.
pub async fn sync(app: &Arc<App>) {
    let uuids: Vec<String> = app.streamdeck.state.lock().running.keys().cloned().collect();
    for uuid in uuids {
        sync_one(app, &uuid);
    }
}

pub(crate) fn sync_one(app: &App, uuid: &str) {
    let now: HashMap<String, (String, Map<String, Value>)> = app
        .engine
        .lock()
        .instances_for(&format!("{PREFIX}{uuid}"))
        .into_iter()
        .filter(|i| split_instance(&i.id).is_some())
        .map(|i| (i.id, (i.action.split_once('/').map(|(_, a)| a.to_string()).unwrap_or_default(), i.settings)))
        .collect();
    let (tx, before) = {
        let mut st = app.streamdeck.state.lock();
        let Some(r) = st.running.get_mut(uuid) else { return };
        if r.tx.is_none() {
            return;
        }
        let before = std::mem::replace(&mut r.instances, now.clone());
        (r.tx.clone(), before)
    };
    for (id, (action, settings)) in &before {
        if !now.contains_key(id) {
            send(
                &tx,
                json!({ "event": "willDisappear", "action": action, "context": id, "device": runtime::DEVICE, "payload": { "settings": settings, "coordinates": { "column": 0, "row": 0 }, "isInMultiAction": false, "controller": "Keypad" } }),
            );
            app.streamdeck.state.lock().views.remove(id);
            app.engine.lock().set_override(id, None);
        }
    }
    for (id, (action, settings)) in &now {
        match before.get(id).map(|(_, s)| s) {
            None => {
                send(&tx, json!({ "event": "willAppear", "action": action, "context": id, "device": runtime::DEVICE, "payload": event_payload(app, id, settings) }));
                render(app, uuid, id);
            }
            Some(old) if old != settings => {
                send(&tx, json!({ "event": "didReceiveSettings", "action": action, "context": id, "device": runtime::DEVICE, "payload": event_payload(app, id, settings) }));
            }
            _ => {}
        }
    }
}

/// Key presses and releases for Stream Deck actions.
pub async fn run(app: &App, uuid: &str, steps: Vec<Step>, phase: &str) -> Result<(), String> {
    let inst = app.streamdeck.get(uuid).ok_or("plugin not installed")?;
    let tx = app.streamdeck.state.lock().running.get(uuid).and_then(|r| r.tx.clone()).ok_or_else(|| format!("{} isn't running", inst.manifest.name))?;
    for s in steps {
        if split_instance(&s.instance).is_none() {
            continue;
        }
        let settings = current_settings(app, &s.instance);
        let event = if phase == "release" { "keyUp" } else { "keyDown" };
        send(
            &Some(tx.clone()),
            json!({ "event": event, "action": s.action, "context": s.instance, "device": runtime::DEVICE, "payload": event_payload(app, &s.instance, &settings) }),
        );
        // Two-state actions flip on release, as on a real Stream Deck.
        if phase == "release"
            && let Some(def) = inst.manifest.action(&s.action)
            && def.states.len() == 2
            && !def.disable_automatic_states
        {
            {
                let mut st = app.streamdeck.state.lock();
                let ks = st.views.entry(s.instance.clone()).or_default();
                ks.state = 1 - ks.state.min(1);
            }
            render(app, uuid, &s.instance);
        }
    }
    Ok(())
}

/// Keeps settings owned by Stream Deck plugins when the editor saves an older copy of a key.
pub fn keep_settings(old: &LayoutFile, new: &mut LayoutFile) {
    // (profile, page, key, step, action type) -> settings
    type Slot = (String, String, String, usize, String);
    let mut current: HashMap<Slot, Map<String, Value>> = HashMap::new();
    for (p, pg, k) in all_keys(old) {
        for (n, a) in k.actions.iter().enumerate() {
            if a.kind.starts_with(PREFIX) {
                current.insert((p.id.clone(), pg.id.clone(), k.id.clone(), n, a.kind.clone()), a.settings.clone());
            }
        }
    }
    for p in &mut new.profiles {
        for pg in &mut p.pages {
            for k in &mut pg.keys {
                for (n, a) in k.actions.iter_mut().enumerate() {
                    if let Some(s) = current.get(&(p.id.clone(), pg.id.clone(), k.id.clone(), n, a.kind.clone())) {
                        a.settings = s.clone();
                    }
                }
            }
        }
    }
}

pub fn stage(app: &App, bytes: &[u8]) -> Result<Value> {
    let pkg = manifest::unpack(bytes)?;
    let runtime = pkg.manifest.runtime();
    let token = crate::ids::random_b64(16);
    let summary = json!({
        "token": token,
        "uuid": pkg.uuid,
        "name": pkg.manifest.name,
        "author": pkg.manifest.author,
        "version": pkg.manifest.version,
        "description": pkg.manifest.description,
        "runtime": runtime,
        "actions": pkg.manifest.actions.iter().map(|a| a.name.clone()).collect::<Vec<_>>(),
        "update": app.streamdeck.get(&pkg.uuid).map(|i| i.manifest.version),
    });
    let mut staged = app.streamdeck.staged.lock();
    staged.retain(|_, (_, t)| t.elapsed() < STAGE_TTL);
    staged.insert(token, (pkg, Instant::now()));
    Ok(summary)
}

fn valid_host(h: &str) -> bool {
    !h.is_empty() && h.len() <= 255 && !h.starts_with(':') && h.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b':' | b'[' | b']'))
}

fn install(app: &App, pkg: manifest::SdPackage, unsandboxed: bool) -> Result<()> {
    match pkg.manifest.runtime() {
        Some(Runtime::Html) => bail!("This is a web-page plugin. Those need Elgato's own app and can't run in Keyside."),
        Some(Runtime::Native) if !unsandboxed => bail!("This plugin is a program for your system. Allow it to run unsandboxed to install it."),
        None => bail!("This plugin has no version for this system"),
        _ => {}
    }
    let hub = &app.streamdeck;
    let uuid = pkg.uuid.clone();
    stop(app, &uuid);
    let dir = hub.plugin_dir(&uuid);
    let tmp = hub.root.join("plugins").join(format!(".{uuid}.new"));
    let _ = std::fs::remove_dir_all(&tmp);
    store::private_dir(&tmp)?;
    for (name, bytes, exec) in &pkg.files {
        let p = tmp.join(name);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&p, bytes).with_context(|| format!("writing {name}"))?;
        #[cfg(unix)]
        if *exec || pkg.manifest.code_path() == Some(name.as_str()) && pkg.manifest.runtime() == Some(Runtime::Native) {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o700))?;
        }
        #[cfg(not(unix))]
        let _ = exec;
    }
    // The old process may still hold its files for a moment after stop().
    let mut tries = 0;
    while dir.exists() && std::fs::remove_dir_all(&dir).is_err() && tries < 10 {
        tries += 1;
        std::thread::sleep(Duration::from_millis(100));
    }
    std::fs::rename(&tmp, &dir).context("moving the plugin into place")?;
    hub.images.lock().clear();
    {
        let mut st = hub.state.lock();
        let prev = st.installed.get(&uuid).cloned();
        st.installed.insert(
            uuid.clone(),
            Installed {
                uuid: uuid.clone(),
                manifest: pkg.manifest,
                enabled: true,
                net: prev.as_ref().map(|p| p.net.clone()).unwrap_or_default(),
                all_net: prev.as_ref().is_some_and(|p| p.all_net),
                unsandboxed,
                installed_at: crate::devices::now(),
            },
        );
        hub.save(&st)?;
    }
    refresh_engine(app);
    Ok(())
}

pub fn refresh_engine(app: &App) {
    app.plugins.refresh_engine();
}

pub async fn op(app: &Arc<App>, op: &str, a: &Value) -> Result<Value> {
    let hub = &app.streamdeck;
    let uuid = || a.get("uuid").and_then(Value::as_str).filter(|u| manifest::valid_uuid(u, 60)).map(str::to_string).context("missing plugin");
    match op {
        "streamdeck.list" => {
            let have_deno = crate::plugins::sandbox::find_deno().is_some();
            let st = hub.state.lock();
            let list: Vec<Value> = st
                .installed
                .values()
                .map(|i| {
                    let running = st.running.get(&i.uuid).is_some_and(|r| r.tx.is_some());
                    json!({
                        "uuid": i.uuid,
                        "name": i.manifest.name,
                        "author": i.manifest.author,
                        "version": i.manifest.version,
                        "description": i.manifest.description,
                        "runtime": i.manifest.runtime(),
                        "enabled": i.enabled,
                        "running": running,
                        "blocked": i.blocked(have_deno),
                        "net": i.net,
                        "allNet": i.all_net,
                        "unsandboxed": i.unsandboxed,
                        "actions": i.manifest.actions.len(),
                    })
                })
                .collect();
            Ok(json!({ "plugins": list, "deno": have_deno }))
        }
        "streamdeck.install" => {
            let token = a.get("token").and_then(Value::as_str).context("missing token")?;
            let (pkg, _) = hub.staged.lock().remove(token).context("upload expired, pick the file again")?;
            let uuid = pkg.uuid.clone();
            install(app, pkg, a.get("unsandboxed").and_then(Value::as_bool).unwrap_or(false))?;
            start(app, &uuid);
            Ok(json!({ "uuid": uuid }))
        }
        "streamdeck.uninstall" => {
            let uuid = uuid()?;
            stop(app, &uuid);
            {
                let mut st = hub.state.lock();
                st.installed.remove(&uuid);
                hub.save(&st)?;
            }
            for d in [hub.plugin_dir(&uuid), hub.data_dir(&uuid), hub.cache_dir(&uuid)] {
                let _ = std::fs::remove_dir_all(d);
            }
            let _ = std::fs::remove_file(hub.global_path(&uuid));
            refresh_engine(app);
            Ok(Value::Null)
        }
        "streamdeck.grant" => {
            let uuid = uuid()?;
            {
                let mut st = hub.state.lock();
                let i = st.installed.get_mut(&uuid).context("plugin not installed")?;
                if let Some(net) = a.get("net").and_then(Value::as_array) {
                    let hosts: Vec<String> = net.iter().filter_map(Value::as_str).map(|h| h.trim().to_ascii_lowercase()).filter(|h| valid_host(h)).take(32).collect();
                    i.net = hosts;
                }
                if let Some(all) = a.get("allNet").and_then(Value::as_bool) {
                    i.all_net = all;
                }
                if let Some(e) = a.get("enabled").and_then(Value::as_bool) {
                    i.enabled = e;
                }
                if let Some(u) = a.get("unsandboxed").and_then(Value::as_bool) {
                    i.unsandboxed = u;
                }
                hub.save(&st)?;
            }
            stop(app, &uuid);
            start(app, &uuid);
            refresh_engine(app);
            Ok(Value::Null)
        }
        "streamdeck.inspector" => {
            let instance = a.get("instance").and_then(Value::as_str).context("missing key")?;
            inspector::open(app, instance)
        }
        _ => bail!("unknown request"),
    }
}

/// Origin the editor may frame for settings panels.
pub fn inspector_origin(app: &App) -> Option<String> {
    app.streamdeck.inspector_port.get().map(|p| format!("http://localhost:{p}"))
}

/// Starts the plugin socket, the settings panel server and every enabled plugin.
pub async fn start_all(app: &Arc<App>) -> Result<()> {
    let _ = app.streamdeck.admin_port.set(app.settings.read().admin_port);
    runtime::listen(app.clone()).await?;
    inspector::listen(app.clone()).await?;
    let ids: Vec<String> = app.streamdeck.state.lock().installed.values().filter(|i| i.enabled).map(|i| i.uuid.clone()).collect();
    for id in ids {
        start(app, &id);
    }
    refresh_engine(app);
    Ok(())
}

pub fn start(app: &Arc<App>, uuid: &str) {
    runtime::start(app, uuid);
}

pub fn stop_all(app: &App) {
    let ids: Vec<String> = app.streamdeck.state.lock().running.keys().cloned().collect();
    for id in ids {
        stop(app, &id);
    }
}

pub fn stop(app: &App, uuid: &str) {
    let removed = {
        let mut st = app.streamdeck.state.lock();
        st.inspectors.retain(|_, i| i.plugin != uuid);
        st.running.remove(uuid)
    };
    if let Some(mut r) = removed {
        if let Some(s) = r.stop.take() {
            let _ = s.send(());
        }
        let mut e = app.engine.lock();
        for id in r.instances.keys() {
            e.set_override(id, None);
        }
        e.remove_connection(&format!("{PREFIX}{uuid}"));
    }
}

impl App {
    /// An owned handle for spawned tasks. Only used where an `Arc<App>` isn't at hand.
    fn engine_handle(&self) -> Option<Arc<App>> {
        self.plugins.app_handle()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_instances() {
        assert_eq!(split_instance("main/scenes/k1/0"), Some(("main", "scenes", "k1", 0)));
        assert_eq!(split_instance("main/scenes/k1/o0"), None);
    }

    #[test]
    fn checks_hosts() {
        assert!(valid_host("api.weather.example"));
        assert!(valid_host("192.168.1.4:8080"));
        assert!(!valid_host("evil.com/path"));
        assert!(!valid_host("*"));
    }
}

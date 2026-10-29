//! The local socket Stream Deck plugins and settings panels connect to, and the plugin
//! processes themselves.

use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};

use super::manifest::Runtime;
use super::{Installed, PREFIX, Running};
use crate::app::App;
use crate::model::Tone;
use crate::state::ConnectionView;
use crate::store;

pub const DEVICE: &str = "phonedeck";
/// The Stream Deck app version plugins are told they run in.
const APP_VERSION: &str = "6.8.0";
const MAX_MESSAGE: usize = 4 * 1024 * 1024;
const RATE: f64 = 200.0;
const BURST: f64 = 400.0;
const CRASH_WINDOW: Duration = Duration::from_secs(300);
const CRASH_LIMIT: usize = 5;
/// System info Node plugins commonly read. Nothing that identifies the person.
const SYS: &str = "osRelease,osUptime,loadavg,cpus,systemMemoryInfo,hostname,homedir,uid,gid";

pub async fn listen(app: Arc<App>) -> Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.context("starting the Stream Deck plugin socket")?;
    let port = listener.local_addr()?.port();
    let _ = app.streamdeck.port.set(port);
    tokio::spawn(async move {
        loop {
            let Ok((stream, peer)) = listener.accept().await else { continue };
            if !peer.ip().is_loopback() {
                continue;
            }
            let app = app.clone();
            tokio::spawn(async move {
                if let Err(e) = connection(app, stream).await {
                    tracing::debug!("stream deck socket closed: {e:#}");
                }
            });
        }
    });
    Ok(())
}

enum Peer {
    Plugin(String),
    Inspector(String),
}

async fn connection(app: Arc<App>, stream: tokio::net::TcpStream) -> Result<()> {
    let inspector_origin = super::inspector_origin(&app);
    // Browsers always send an Origin. Only our settings panel origin may connect from one.
    let check = |req: &Request, resp: Response| -> Result<Response, ErrorResponse> {
        match req.headers().get("origin").and_then(|o| o.to_str().ok()) {
            Some(o) if Some(o) != inspector_origin.as_deref() => {
                let mut r = ErrorResponse::new(Some("origin not allowed".into()));
                *r.status_mut() = tokio_tungstenite::tungstenite::http::StatusCode::FORBIDDEN;
                Err(r)
            }
            _ => Ok(resp),
        }
    };
    let config = tokio_tungstenite::tungstenite::protocol::WebSocketConfig::default().max_message_size(Some(MAX_MESSAGE)).max_frame_size(Some(MAX_MESSAGE));
    let ws = tokio_tungstenite::accept_hdr_async_with_config(stream, check, Some(config)).await?;
    let (mut sink, mut source) = ws.split();

    let first = tokio::time::timeout(Duration::from_secs(10), source.next()).await.context("no registration")?.context("closed")??;
    let reg: Value = serde_json::from_str(first.to_text()?)?;
    let token = reg.get("uuid").and_then(Value::as_str).unwrap_or("").to_string();
    let (tx, mut rx) = mpsc::channel::<String>(512);
    let peer = match reg.get("event").and_then(Value::as_str) {
        Some("registerPlugin") => {
            let uuid = {
                let mut st = app.streamdeck.state.lock();
                let hit = st.running.iter_mut().find(|(_, r)| !token.is_empty() && r.token == token);
                let Some((uuid, r)) = hit else { anyhow::bail!("unknown plugin") };
                r.tx = Some(tx.clone());
                uuid.clone()
            };
            on_plugin_ready(&app, &uuid, &tx);
            Peer::Plugin(uuid)
        }
        Some("registerPropertyInspector") => {
            let (uuid, context, action) = {
                let mut st = app.streamdeck.state.lock();
                let i = st.inspectors.get_mut(&token).filter(|i| i.tx.is_none()).context("unknown settings panel")?;
                i.tx = Some(tx.clone());
                (i.plugin.clone(), i.context.clone(), i.action.clone())
            };
            super::to_plugin(&app, &uuid, json!({ "event": "propertyInspectorDidAppear", "action": action, "context": context, "device": DEVICE }));
            Peer::Inspector(token.clone())
        }
        _ => anyhow::bail!("expected a registration"),
    };

    let writer = tokio::spawn(async move {
        while let Some(m) = rx.recv().await {
            if sink.send(Message::text(m)).await.is_err() {
                break;
            }
        }
        let _ = sink.close().await;
    });
    let mut tokens = BURST;
    let mut last = Instant::now();
    while let Some(Ok(msg)) = source.next().await {
        let now = Instant::now();
        tokens = (tokens + now.duration_since(last).as_secs_f64() * RATE).min(BURST);
        last = now;
        tokens -= 1.0;
        if tokens < 0.0 {
            tracing::warn!("a Stream Deck plugin sent too many messages and was disconnected");
            break;
        }
        let Message::Text(text) = msg else { continue };
        let Ok(v) = serde_json::from_str::<Value>(&text) else { continue };
        match &peer {
            Peer::Plugin(uuid) => super::from_plugin(&app, uuid, &v),
            Peer::Inspector(token) => super::from_inspector(&app, token, &v),
        }
    }
    writer.abort();
    match peer {
        Peer::Plugin(uuid) => {
            let mut st = app.streamdeck.state.lock();
            if let Some(r) = st.running.get_mut(&uuid)
                && r.tx.as_ref().is_some_and(|t| t.same_channel(&tx))
            {
                r.tx = None;
            }
        }
        Peer::Inspector(token) => {
            let gone = app.streamdeck.state.lock().inspectors.remove(&token);
            if let Some(i) = gone {
                super::to_plugin(&app, &i.plugin, json!({ "event": "propertyInspectorDidDisappear", "action": i.action, "context": i.context, "device": DEVICE }));
            }
        }
    }
    Ok(())
}

fn on_plugin_ready(app: &Arc<App>, uuid: &str, tx: &mpsc::Sender<String>) {
    let _ = tx.try_send(json!({ "event": "deviceDidConnect", "device": DEVICE, "deviceInfo": device_info() }).to_string());
    if let Some(r) = app.streamdeck.state.lock().running.get_mut(uuid) {
        r.instances.clear();
    }
    let name = app.streamdeck.get(uuid).map(|i| i.manifest.name).unwrap_or_default();
    app.engine.lock().set_connection(ConnectionView { id: format!("{PREFIX}{uuid}"), name, status: "connected".into(), detail: None });
    super::sync_one(app, uuid);
}

fn device_info() -> Value {
    json!({ "name": "PhoneDeck", "type": 0, "size": { "columns": 8, "rows": 4 } })
}

pub fn info(app: &App, inst: &Installed) -> Value {
    let platform = if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "mac"
    } else {
        "linux"
    };
    json!({
        "application": { "font": "", "language": app.settings.read().locale, "platform": platform, "platformVersion": "", "version": APP_VERSION },
        "plugin": { "uuid": inst.uuid, "version": inst.manifest.version },
        "devicePixelRatio": 2,
        "colors": {
            "buttonPressedBackgroundColor": "#303030FF", "buttonPressedBorderColor": "#646464FF", "buttonPressedTextColor": "#969696FF",
            "disabledColor": "#007AFF59", "highlightColor": "#007AFFFF", "mouseDownColor": "#2EA8FFFF"
        },
        "devices": [{ "id": DEVICE, "name": "PhoneDeck", "type": 0, "size": { "columns": 8, "rows": 4 } }],
    })
}

/// Starts (or restarts) one plugin under supervision.
pub fn start(app: &Arc<App>, uuid: &str) {
    let Some(inst) = app.streamdeck.get(uuid) else { return };
    if !inst.enabled {
        return;
    }
    let have_deno = crate::plugins::sandbox::find_deno().is_some();
    if let Some(why) = inst.blocked(have_deno) {
        app.engine.lock().set_connection(ConnectionView { id: inst.plugin_id(), name: inst.manifest.name.clone(), status: "error".into(), detail: Some(why.into()) });
        return;
    }
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
    let generation = {
        let mut st = app.streamdeck.state.lock();
        st.generation += 1;
        let g = st.generation;
        st.running.insert(
            uuid.to_string(),
            Running {
                generation: g,
                token: crate::ids::random_b64(24),
                tx: None,
                instances: Default::default(),
                stop: Some(stop_tx),
                crashes: Default::default(),
                last_url: None,
                writes: (super::WRITE_BURST, Instant::now()),
            },
        );
        g
    };
    tokio::spawn(supervise(app.clone(), uuid.to_string(), generation, stop_rx));
}

async fn supervise(app: Arc<App>, uuid: String, generation: u64, mut stop: tokio::sync::oneshot::Receiver<()>) {
    let current = |app: &App| app.streamdeck.state.lock().running.get(&uuid).is_some_and(|r| r.generation == generation);
    loop {
        if !current(&app) {
            return;
        }
        let Some(inst) = app.streamdeck.get(&uuid) else { return };
        let token = app.streamdeck.state.lock().running.get(&uuid).map(|r| r.token.clone()).unwrap_or_default();
        let mut child = match command(&app, &inst, &token).and_then(|mut c| c.spawn().context("starting the plugin")) {
            Ok(c) => c,
            Err(e) => {
                app.engine.lock().set_connection(ConnectionView { id: inst.plugin_id(), name: inst.manifest.name.clone(), status: "error".into(), detail: Some(format!("{e:#}")) });
                return;
            }
        };
        if let Some(err) = child.stderr.take() {
            let id = uuid.clone();
            tokio::spawn(async move {
                use tokio::io::AsyncBufReadExt;
                let mut lines = tokio::io::BufReader::new(err).lines();
                let mut n = 0;
                while let Ok(Some(l)) = lines.next_line().await {
                    n += 1;
                    if n <= 500 {
                        tracing::info!(plugin = %id, "{}", l.chars().take(500).collect::<String>());
                    }
                }
            });
        }
        let status = tokio::select! {
            s = child.wait() => s,
            _ = &mut stop => {
                let _ = child.start_kill();
                let _ = child.wait().await;
                return;
            }
        };
        if !current(&app) {
            return;
        }
        tracing::warn!(plugin = %uuid, "Stream Deck plugin exited: {status:?}");
        let crashes = {
            let mut st = app.streamdeck.state.lock();
            let Some(r) = st.running.get_mut(&uuid) else { return };
            r.tx = None;
            let now = Instant::now();
            r.crashes.push_back(now);
            while r.crashes.front().is_some_and(|t| now.duration_since(*t) > CRASH_WINDOW) {
                r.crashes.pop_front();
            }
            r.crashes.len()
        };
        app.engine.lock().set_connection(ConnectionView { id: inst.plugin_id(), name: inst.manifest.name.clone(), status: "connecting".into(), detail: Some("Restarting".into()) });
        if crashes >= CRASH_LIMIT {
            {
                let mut st = app.streamdeck.state.lock();
                if let Some(i) = st.installed.get_mut(&uuid) {
                    i.enabled = false;
                }
                let _ = app.streamdeck.save(&st);
            }
            super::stop(&app, &uuid);
            super::refresh_engine(&app);
            app.engine.lock().toast(format!("{} kept stopping and was turned off", inst.manifest.name), Tone::Danger);
            return;
        }
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(1 << crashes.min(5))) => {}
            _ = &mut stop => return,
        }
    }
}

fn command(app: &App, inst: &Installed, token: &str) -> Result<tokio::process::Command> {
    let hub = &app.streamdeck;
    let dir = hub.plugin_dir(&inst.uuid);
    let data = hub.data_dir(&inst.uuid);
    let cache = hub.cache_dir(&inst.uuid);
    store::private_dir(&data)?;
    store::private_dir(&cache)?;
    let code = inst.manifest.code_path().context("no code for this system")?;
    let port = *hub.port.get().context("plugin socket not started")?;
    let sd_args = [
        "-port".to_string(),
        port.to_string(),
        "-pluginUUID".into(),
        token.to_string(),
        "-registerEvent".into(),
        "registerPlugin".into(),
        "-info".into(),
        info(app, inst).to_string(),
    ];
    let mut c = match inst.manifest.runtime() {
        Some(Runtime::Node) => {
            let deno = crate::plugins::sandbox::find_deno().context("the plugin runtime (Deno) is missing")?;
            for p in [&dir, &data, &cache] {
                anyhow::ensure!(!p.to_string_lossy().contains(','), "plugin folders can't contain commas");
            }
            let mut net = vec![format!("127.0.0.1:{port}")];
            net.extend(inst.net.iter().cloned());
            let mut c = tokio::process::Command::new(deno);
            c.args(["run", "--quiet", "--no-prompt", "--no-config", "--no-lock", "--no-remote", "--no-npm", "--unstable-detect-cjs", "--deny-run", "--deny-ffi"])
                .arg("--v8-flags=--max-old-space-size=256")
                // The environment is cleared below, so reading it reveals nothing.
                .arg("--allow-env")
                .arg(format!("--allow-sys={SYS}"))
                // Stream Deck plugins expect to write logs and files inside their own folder.
                .arg(format!("--allow-read={},{}", dir.display(), data.display()))
                .arg(format!("--allow-write={},{}", dir.display(), data.display()))
                .arg(if inst.all_net { "--allow-net".to_string() } else { format!("--allow-net={}", net.join(",")) })
                .arg(dir.join(code))
                .args(&sd_args)
                .env_clear()
                .env("DENO_DIR", &cache)
                .env("DENO_NO_UPDATE_CHECK", "1")
                .env("NO_COLOR", "1");
            for k in ["HTTPS_PROXY", "https_proxy", "HTTP_PROXY", "http_proxy", "NO_PROXY", "no_proxy", "DENO_CERT"] {
                if let Ok(v) = std::env::var(k) {
                    c.env(k, v);
                }
            }
            #[cfg(windows)]
            for k in ["SystemRoot", "windir", "TEMP", "TMP"] {
                if let Ok(v) = std::env::var(k) {
                    c.env(k, v);
                }
            }
            c
        }
        Some(Runtime::Native) if inst.unsandboxed => {
            // The person agreed to this: native plugins run like any program they start.
            let mut c = tokio::process::Command::new(dir.join(code));
            c.args(&sd_args);
            c
        }
        _ => anyhow::bail!("this plugin can't run here"),
    };
    c.current_dir(&dir).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::piped()).kill_on_drop(true);
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    Ok(c)
}

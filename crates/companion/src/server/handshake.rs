//! First messages on a device socket: pair (new device) or resume (known device).
//! Nothing but the handshake is accepted until both sides proved their key.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use axum::extract::ws::{Message, WebSocket};
use parking_lot::Mutex;
use serde_json::json;

use super::conn::{self, Link, Who};
use crate::app::App;
use crate::crypto::channel::{Challenge, Confirm, Hello, Mode, ServerHandshake, unb64};
use crate::devices::{PendingDevice, Scope};
use crate::state::Change;

const STEP_TIMEOUT: Duration = Duration::from_secs(10);
const APPROVAL_TIMEOUT: Duration = Duration::from_secs(120);
const PER_MINUTE: u32 = 30;
const FAILS_BEFORE_BLOCK: u32 = 10;
const BLOCK: Duration = Duration::from_secs(300);

#[derive(Default)]
struct IpState {
    window: Option<Instant>,
    count: u32,
    fails: u32,
    blocked_until: Option<Instant>,
}

fn limits() -> &'static Mutex<HashMap<IpAddr, IpState>> {
    static L: OnceLock<Mutex<HashMap<IpAddr, IpState>>> = OnceLock::new();
    L.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Counts an attempt. False when this address is blocked or too busy.
pub fn admit(ip: IpAddr) -> bool {
    let mut m = limits().lock();
    if m.len() > 10_000 {
        m.retain(|_, s| s.blocked_until.is_some_and(|b| b > Instant::now()));
    }
    let s = m.entry(ip).or_default();
    let now = Instant::now();
    if s.blocked_until.is_some_and(|b| b > now) {
        return false;
    }
    if s.window.is_none_or(|w| now.duration_since(w) > Duration::from_secs(60)) {
        s.window = Some(now);
        s.count = 0;
    }
    s.count += 1;
    s.count <= PER_MINUTE
}

fn failed(ip: IpAddr) {
    let mut m = limits().lock();
    let s = m.entry(ip).or_default();
    s.fails += 1;
    if s.fails >= FAILS_BEFORE_BLOCK {
        s.blocked_until = Some(Instant::now() + BLOCK);
        s.fails = 0;
    }
}

fn succeeded(ip: IpAddr) {
    if let Some(s) = limits().lock().get_mut(&ip) {
        s.fails = 0;
    }
}

async fn recv_text(ws: &mut WebSocket) -> Option<String> {
    match tokio::time::timeout(STEP_TIMEOUT, ws.recv()).await {
        Ok(Some(Ok(Message::Text(t)))) if t.len() <= 4096 => Some(t.to_string()),
        _ => None,
    }
}

async fn send_text(ws: &mut WebSocket, v: &impl serde::Serialize) -> bool {
    let Ok(s) = serde_json::to_string(v) else { return false };
    tokio::time::timeout(STEP_TIMEOUT, ws.send(Message::Text(s.into()))).await.is_ok_and(|r| r.is_ok())
}

async fn reject(mut ws: WebSocket, ip: IpAddr, reason: &str) {
    failed(ip);
    // Same delay for every failure, so timing doesn't tell which check failed.
    tokio::time::sleep(Duration::from_millis(400)).await;
    let _ = send_text(&mut ws, &json!({ "t": "error", "reason": reason })).await;
    let _ = ws.send(Message::Close(None)).await;
}

pub async fn run(app: Arc<App>, mut ws: WebSocket, ip: IpAddr) {
    let Some(raw) = recv_text(&mut ws).await else { return };
    let Ok(hello) = serde_json::from_str::<Hello>(&raw) else { return reject(ws, ip, "bad-hello").await };
    let cid_b64 = app.identity.read().cid.clone();
    if hello.cid != cid_b64 {
        return reject(ws, ip, "wrong-companion").await;
    }
    let Ok(cid) = unb64(&cid_b64, Some(16)) else { return };
    match hello.mode {
        Mode::Pair => pair(app, ws, ip, hello, &cid).await,
        Mode::Resume => resume(app, ws, ip, hello, &cid).await,
    }
}

async fn finish_handshake(ws: &mut WebSocket, hs: ServerHandshake, ch: &Challenge) -> Option<crate::crypto::channel::Session> {
    if !send_text(ws, ch).await {
        return None;
    }
    let raw = recv_text(ws).await?;
    let confirm: Confirm = serde_json::from_str(&raw).ok()?;
    hs.accept(&confirm).ok()
}

async fn resume(app: Arc<App>, mut ws: WebSocket, ip: IpAddr, hello: Hello, cid: &[u8]) {
    let device = app.devices.lock().get(&hello.id).cloned();
    let Some(device) = device else { return reject(ws, ip, "unknown-device").await };
    let Ok(secret) = unb64(&device.secret, Some(32)) else { return };
    let Ok((hs, ch)) = ServerHandshake::respond(&hello, cid, &secret, None) else { return reject(ws, ip, "bad-hello").await };
    let Some(session) = finish_handshake(&mut ws, hs, &ch).await else { return reject(ws, ip, "bad-proof").await };
    succeeded(ip);
    let who = Who {
        admin: false,
        device: device.id.clone(),
        name: device.name.clone(),
        scope: device.scope,
        pages: (device.scope == Scope::Moderator).then(|| device.pages.clone()),
    };
    conn::run(app, Link::Secure(ws, session), who, device.pin.is_some()).await;
}

async fn pair(app: Arc<App>, mut ws: WebSocket, ip: IpAddr, hello: Hello, cid: &[u8]) {
    let Some(info) = hello.device.clone() else { return reject(ws, ip, "bad-hello").await };
    let pairing = app.devices.lock().take_pairing(&hello.id);
    let Some(pairing) = pairing else { return reject(ws, ip, "pairing-expired").await };
    let Ok((hs, ch)) = ServerHandshake::respond(&hello, cid, &pairing.code, None) else { return reject(ws, ip, "bad-hello").await };
    let sas = hs.sas().to_string();
    let Some(mut session) = finish_handshake(&mut ws, hs, &ch).await else { return reject(ws, ip, "bad-proof").await };
    succeeded(ip);

    // The QR code may have been seen by someone else, so the person approves on the PC.
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.approvals.lock().insert(hello.id.clone(), tx);
    app.devices.lock().add_pending(PendingDevice {
        pid: hello.id.clone(),
        name: crate::config::clean_name(&info.name, 40).unwrap_or_else(|| "Device".into()),
        kind: info.kind.clone(),
        app: info.app.clone(),
        sas: sas.clone(),
        address: ip.to_string(),
    });
    app.notify(Change::Pending);
    let waiting = match session.seal(json!({ "t": "waiting", "sas": sas }).to_string().as_bytes()) {
        Ok(f) => f,
        Err(_) => return,
    };
    let _ = ws.send(Message::Binary(waiting.into())).await;

    let deadline = tokio::time::sleep(APPROVAL_TIMEOUT);
    tokio::pin!(deadline);
    let mut rx = rx;
    let approved = loop {
        tokio::select! {
            r = &mut rx => break matches!(r, Ok(true)),
            _ = &mut deadline => break false,
            m = ws.recv() => match m {
                // Keep the message counters in step; nothing is acted on until approval.
                Some(Ok(Message::Binary(b))) => {
                    if session.open(&b).is_err() {
                        break false;
                    }
                }
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break false,
                _ => {}
            },
        }
    };
    app.approvals.lock().remove(&hello.id);
    app.devices.lock().remove_pending(&hello.id);
    app.notify(Change::Pending);
    if !approved {
        if let Ok(f) = session.seal(json!({ "t": "bye", "reason": "denied" }).to_string().as_bytes()) {
            let _ = ws.send(Message::Binary(f.into())).await;
        }
        let _ = ws.send(Message::Close(None)).await;
        return;
    }
    let created = app.devices.lock().create(&info, &pairing);
    let Ok(device) = created else { return };
    let companion = json!({ "id": app.identity.read().cid, "name": app.settings.read().name });
    let msg = json!({ "t": "paired", "deviceId": device.id, "secret": device.secret, "companion": companion });
    let Ok(frame) = session.seal(msg.to_string().as_bytes()) else { return };
    if ws.send(Message::Binary(frame.into())).await.is_err() {
        return;
    }
    let who = Who {
        admin: false,
        device: device.id.clone(),
        name: device.name.clone(),
        scope: device.scope,
        pages: (device.scope == Scope::Moderator).then(|| device.pages.clone()),
    };
    // The person just set the PIN on the PC; the new device still has to enter it.
    conn::run(app, Link::Secure(ws, session), who, device.pin.is_some()).await;
}

//! One connected client: a paired device (secure channel) or the local editor (loopback).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::ws::{Message, WebSocket};
use serde_json::{Value, json};
use tokio::sync::{broadcast, mpsc};

use super::ops;
use crate::actions::{self, Ctx, Nav};
use crate::app::App;
use crate::crypto::channel::Session;
use crate::devices::{PinCheck, Scope};
use crate::model::{FormFactor, KeyKind, Tone};
use crate::plugins::Step;
use crate::state::view::KeyView;
use crate::state::{Change, instance_id, key_path};

const SEND_TIMEOUT: Duration = Duration::from_secs(10);
const IDLE_TIMEOUT: Duration = Duration::from_secs(75);
const FLUSH_EVERY: Duration = Duration::from_millis(50);

#[derive(Debug, Clone)]
pub struct Who {
    /// The local editor on this PC.
    pub admin: bool,
    pub device: String,
    pub name: String,
    pub scope: Scope,
    /// Moderators only: allowed pages as "profile/page".
    pub pages: Option<Vec<String>>,
}

impl Who {
    pub fn can_edit(&self) -> bool {
        self.admin || self.scope == Scope::Editor
    }
}

pub enum Link {
    Plain(WebSocket),
    Secure(WebSocket, Session),
}

impl Link {
    pub async fn send(&mut self, v: &Value) -> anyhow::Result<()> {
        let msg = match self {
            Link::Plain(_) => Message::Text(v.to_string().into()),
            Link::Secure(_, s) => Message::Binary(s.seal(v.to_string().as_bytes())?.into()),
        };
        let ws = match self {
            Link::Plain(ws) | Link::Secure(ws, _) => ws,
        };
        tokio::time::timeout(SEND_TIMEOUT, ws.send(msg)).await.map_err(|_| anyhow::anyhow!("send timed out"))??;
        Ok(())
    }

    /// Next message as JSON. None when the socket closed or broke the protocol.
    pub async fn recv(&mut self, max: usize) -> Option<Value> {
        loop {
            let msg = match self {
                Link::Plain(ws) | Link::Secure(ws, _) => ws.recv().await?.ok()?,
            };
            let bytes: Vec<u8> = match (&mut *self, msg) {
                (Link::Plain(_), Message::Text(t)) => t.as_bytes().to_vec(),
                (Link::Secure(_, s), Message::Binary(b)) => s.open(&b).ok()?,
                (_, Message::Ping(_) | Message::Pong(_)) => continue,
                _ => return None,
            };
            if bytes.len() > max {
                return None;
            }
            return serde_json::from_slice(&bytes).ok();
        }
    }
}

enum Internal {
    HoldDone { key: String, generation: u64 },
}

struct Conn {
    app: Arc<App>,
    link: Link,
    who: Who,
    ff: FormFactor,
    profile: String,
    page: String,
    stack: Vec<String>,
    sent: HashMap<String, KeyView>,
    sent_shape: Option<(String, u8, u8, Vec<String>)>,
    locked: bool,
    holds: HashMap<String, u64>,
    hold_gen: u64,
    presses: actions::Presses<String>,
    itx: mpsc::Sender<Internal>,
    tokens: f64,
    last_tokens: Instant,
}

fn ok(id: u64, data: Value) -> Value {
    json!({ "t": "result", "id": id, "ok": true, "data": data })
}

fn fail(id: u64, code: &str, message: &str) -> Value {
    json!({ "t": "result", "id": id, "ok": false, "error": { "code": code, "message": message } })
}

pub async fn run(app: Arc<App>, link: Link, who: Who, locked: bool) {
    let (itx, mut irx) = mpsc::channel(32);
    let mut changes = app.engine.lock().subscribe();
    let mut navs = app.nav.subscribe();
    *app.online.lock().entry(who.device.clone()).or_default() += 1;
    if !who.admin {
        app.devices.lock().touch(&who.device);
        app.notify(Change::Pending);
    }
    let profile = app.engine.lock().active_profile().to_string();
    let mut c = Conn {
        app: app.clone(),
        link,
        who,
        ff: FormFactor::PhonePortrait,
        profile,
        page: String::new(),
        stack: Vec::new(),
        sent: HashMap::new(),
        sent_shape: None,
        locked,
        holds: HashMap::new(),
        hold_gen: 0,
        presses: actions::Presses::default(),
        itx,
        tokens: 60.0,
        last_tokens: Instant::now(),
    };
    c.reset_profile();
    let max_in = if c.who.admin {
        2 * 1024 * 1024
    } else if c.who.can_edit() {
        1024 * 1024
    } else {
        64 * 1024
    };

    let welcome = {
        let id = app.identity.read().cid.clone();
        let name = app.settings.read().name.clone();
        json!({
            "t": "welcome",
            "companion": { "id": id, "name": name, "version": env!("CARGO_PKG_VERSION") },
            "device": { "id": c.who.device, "name": c.who.name, "scope": c.who.scope, "pages": c.who.pages, "admin": c.who.admin },
            "locked": c.locked,
        })
    };
    if c.link.send(&welcome).await.is_err() {
        return finish(&app, &c.who);
    }
    if c.locked {
        let _ = c.link.send(&json!({ "t": "locked", "reason": "pin" })).await;
    } else if c.send_page(true).await.is_err() || c.send_status().await.is_err() {
        return finish(&app, &c.who);
    }

    let (mut dirty_keys, mut dirty_full, mut dirty_status) = (false, false, false);
    let mut flush = tokio::time::interval(FLUSH_EVERY);
    let mut last_in = Instant::now();
    loop {
        tokio::select! {
            msg = c.link.recv(max_in) => {
                let Some(v) = msg else { break };
                last_in = Instant::now();
                match c.handle(v).await {
                    Ok(true) => {}
                    Ok(false) | Err(_) => break,
                }
            }
            ch = changes.recv() => match ch {
                Ok(Change::Keys | Change::Vars) => dirty_keys = true,
                Ok(Change::Layout) => { c.fix_position(); dirty_full = true; }
                Ok(Change::Profile) => {
                    let active = app.engine.lock().active_profile().to_string();
                    if active != c.profile && c.who.pages.is_none() {
                        c.profile = active;
                        c.reset_profile();
                        dirty_full = true;
                    }
                }
                Ok(Change::Status) => dirty_status = true,
                Ok(Change::Toast { text, tone }) => {
                    if !c.locked && c.link.send(&json!({ "t": "toast", "text": text, "tone": tone, "at": now_ms() })).await.is_err() { break; }
                }
                Ok(Change::Revoked(id)) if id == c.who.device => {
                    let _ = c.link.send(&json!({ "t": "bye", "reason": "revoked" })).await;
                    break;
                }
                Ok(Change::Updated(id)) if id == c.who.device => {
                    let _ = c.link.send(&json!({ "t": "bye", "reason": "updated" })).await;
                    break;
                }
                Ok(Change::Pending | Change::Plugins) if c.who.admin => {
                    if c.link.send(&json!({ "t": "event", "name": "admin.changed" })).await.is_err() { break; }
                }
                Ok(_) => {}
                Err(broadcast::error::RecvError::Lagged(_)) => { dirty_full = true; dirty_status = true; }
                Err(broadcast::error::RecvError::Closed) => break,
            },
            n = navs.recv() => {
                if let Ok((device, Nav::Page(p))) = n
                    && (device == c.who.device || device == "*")
                    && c.goto(&p)
                {
                    dirty_full = true;
                }
            }
            Some(Internal::HoldDone { key, generation }) = irx.recv() => {
                if c.holds.get(&key) == Some(&generation) {
                    c.holds.remove(&key);
                    c.fire(&key).await;
                }
            }
            _ = flush.tick() => {
                if last_in.elapsed() > IDLE_TIMEOUT {
                    break;
                }
                if c.locked {
                    continue;
                }
                let r = if dirty_full { c.send_page(true).await } else if dirty_keys { c.send_page(false).await } else { Ok(()) };
                if r.is_err() { break; }
                if dirty_status && c.send_status().await.is_err() { break; }
                (dirty_keys, dirty_full, dirty_status) = (false, false, false);
            }
        }
    }
    c.presses.release_all(&app);
    finish(&app, &c.who);
}

fn finish(app: &App, who: &Who) {
    let mut on = app.online.lock();
    if let Some(n) = on.get_mut(&who.device) {
        *n = n.saturating_sub(1);
        if *n == 0 {
            on.remove(&who.device);
        }
    }
    drop(on);
    if !who.admin {
        app.notify(Change::Pending);
    }
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

impl Conn {
    /// Allowed page ids within the current profile, for moderators.
    fn allowed(&self) -> Option<Vec<String>> {
        self.who.pages.as_ref().map(|list| list.iter().filter_map(|p| p.split_once('/')).filter(|(pr, _)| *pr == self.profile).map(|(_, pg)| pg.to_string()).collect())
    }

    fn reset_profile(&mut self) {
        let e = self.app.engine.lock();
        if let Some(list) = &self.who.pages {
            let has_here = list.iter().any(|p| p.split_once('/').is_some_and(|(pr, _)| pr == self.profile));
            if !has_here && let Some((pr, _)) = list.iter().find_map(|p| p.split_once('/')).filter(|(pr, _)| e.profile(pr).is_some()) {
                self.profile = pr.to_string();
            }
        }
        if e.profile(&self.profile).is_none() {
            self.profile = e.active_profile().to_string();
        }
        drop(e);
        self.stack.clear();
        self.page = self.home();
    }

    fn home(&self) -> String {
        let e = self.app.engine.lock();
        let Some(p) = e.profile(&self.profile) else { return String::new() };
        let allowed = self.allowed();
        if e.may_see(p, &p.home_page, allowed.as_deref()) {
            return p.home_page.clone();
        }
        e.visible_pages(p, allowed.as_deref()).next().map(|pg| pg.id.clone()).unwrap_or_default()
    }

    /// After a layout change, stay on the same page when it still exists.
    fn fix_position(&mut self) {
        let ok = {
            let e = self.app.engine.lock();
            e.profile(&self.profile).is_some_and(|p| e.may_see(p, &self.page, self.allowed().as_deref()))
        };
        if !ok {
            self.reset_profile();
        }
        self.holds.clear();
    }

    fn goto(&mut self, page: &str) -> bool {
        let e = self.app.engine.lock();
        let Some(p) = e.profile(&self.profile) else { return false };
        if !e.may_see(p, page, self.allowed().as_deref()) || page == self.page {
            return false;
        }
        let folder = p.pages.iter().find(|pg| pg.id == page).is_some_and(|pg| pg.folder);
        drop(e);
        if folder {
            self.stack.push(self.page.clone());
        } else {
            self.stack.clear();
        }
        self.page = page.to_string();
        self.holds.clear();
        true
    }

    fn step_page(&mut self, delta: i32) -> bool {
        let e = self.app.engine.lock();
        let Some(p) = e.profile(&self.profile) else { return false };
        let allowed = self.allowed();
        let tops: Vec<String> = e.visible_pages(p, allowed.as_deref()).map(|pg| pg.id.clone()).collect();
        drop(e);
        let base = self.stack.first().cloned().unwrap_or_else(|| self.page.clone());
        let Some(i) = tops.iter().position(|t| *t == base) else { return false };
        let j = i as i32 + delta;
        if j < 0 || j >= tops.len() as i32 {
            return false;
        }
        self.stack.clear();
        self.page = tops[j as usize].clone();
        self.holds.clear();
        true
    }

    async fn send_page(&mut self, full: bool) -> anyhow::Result<()> {
        let (view, profile_name, rev) = {
            let e = self.app.engine.lock();
            let view = e.page_view(&self.profile, &self.page, self.ff, self.allowed().as_deref());
            (view, e.profile(&self.profile).map(|p| p.name.clone()).unwrap_or_default(), e.layout().rev)
        };
        let Some(view) = view else {
            self.sent.clear();
            self.sent_shape = None;
            return self.link.send(&json!({ "t": "page", "rev": rev, "profile": { "id": self.profile, "name": profile_name }, "page": Value::Null, "locked": false })).await;
        };
        let shape = (view.id.clone(), view.cols, view.rows, view.keys.iter().map(|k| format!("{}:{}:{}:{}:{}", k.id, k.x, k.y, k.w, k.h)).collect::<Vec<_>>());
        if full || self.sent_shape.as_ref() != Some(&shape) {
            self.sent = view.keys.iter().map(|k| (k.id.clone(), k.clone())).collect();
            self.sent_shape = Some(shape);
            return self.link.send(&json!({ "t": "page", "rev": rev, "profile": { "id": self.profile, "name": profile_name }, "page": view, "locked": false })).await;
        }
        let changed: Vec<&KeyView> = view.keys.iter().filter(|k| self.sent.get(&k.id) != Some(*k)).collect();
        if changed.is_empty() {
            return Ok(());
        }
        let msg = json!({ "t": "keys", "rev": rev, "page": view.id, "keys": changed });
        for k in &changed {
            self.sent.insert(k.id.clone(), (*k).clone());
        }
        self.link.send(&msg).await
    }

    async fn send_status(&mut self) -> anyhow::Result<()> {
        let (connections, pills) = self.app.engine.lock().status();
        self.link.send(&json!({ "t": "status", "connections": connections, "pills": pills })).await
    }

    fn rate_ok(&mut self) -> bool {
        let now = Instant::now();
        self.tokens = (self.tokens + now.duration_since(self.last_tokens).as_secs_f64() * 30.0).min(60.0);
        self.last_tokens = now;
        if self.tokens < 1.0 {
            return false;
        }
        self.tokens -= 1.0;
        true
    }

    /// Returns false to close the connection.
    async fn handle(&mut self, v: Value) -> anyhow::Result<bool> {
        let t = v.get("t").and_then(Value::as_str).unwrap_or("");
        if t == "ping" {
            let ts = v.get("ts").and_then(Value::as_u64).unwrap_or(0);
            self.link.send(&json!({ "t": "pong", "ts": ts })).await?;
            return Ok(true);
        }
        if !self.rate_ok() {
            return Ok(true);
        }
        if self.locked {
            if t == "unlock" {
                self.unlock(v.get("pin").and_then(Value::as_str).unwrap_or("")).await?;
            }
            return Ok(true);
        }
        match t {
            "viewport" => {
                let ff = v.get("formFactor").cloned().and_then(|f| serde_json::from_value(f).ok()).unwrap_or(FormFactor::PhonePortrait);
                if ff != self.ff {
                    self.ff = ff;
                    self.send_page(true).await?;
                }
            }
            "input" => {
                let key = v.get("key").and_then(Value::as_str).unwrap_or("").to_string();
                match v.get("phase").and_then(Value::as_str) {
                    Some("down") => self.down(&key).await?,
                    Some("up") => self.up(&key, false).await?,
                    Some("cancel") => self.up(&key, true).await?,
                    _ => {}
                }
            }
            "slide" => {
                let key = v.get("key").and_then(Value::as_str).unwrap_or("").to_string();
                if let Some(value) = v.get("value").and_then(Value::as_f64).filter(|x| x.is_finite()) {
                    self.slide(&key, value.clamp(0.0, 1.0)).await;
                }
            }
            "nav" => {
                let moved = match v.get("to").and_then(Value::as_str) {
                    Some("next") => self.step_page(1),
                    Some("prev") => self.step_page(-1),
                    Some("home") => {
                        self.stack.clear();
                        let h = self.home();
                        std::mem::replace(&mut self.page, h) != self.page
                    }
                    Some("back") => self.back(),
                    Some("page") => match v.get("page").and_then(Value::as_str) {
                        Some(p) => {
                            let p = p.to_string();
                            self.goto(&p)
                        }
                        None => false,
                    },
                    _ => false,
                };
                if moved {
                    self.send_page(true).await?;
                }
            }
            "req" => {
                let id = v.get("id").and_then(Value::as_u64).unwrap_or(0);
                let op = v.get("op").and_then(Value::as_str).unwrap_or("").to_string();
                let args = v.get("args").cloned().unwrap_or(Value::Null);
                let res = match op.as_str() {
                    "deck.preview" => self.preview(&args),
                    _ => ops::handle(&self.app, &self.who, &op, args).await,
                };
                let msg = match res {
                    Ok(data) => ok(id, data),
                    Err(e) => fail(id, e.code, &e.message),
                };
                self.link.send(&msg).await?;
            }
            _ => {}
        }
        Ok(true)
    }

    fn back(&mut self) -> bool {
        match self.stack.pop() {
            Some(p) => {
                self.page = p;
                true
            }
            None => false,
        }
    }

    /// Editor preview of any page on any form factor, without moving this connection.
    fn preview(&self, args: &Value) -> Result<Value, ops::OpError> {
        if !self.who.can_edit() {
            return Err(ops::OpError::forbidden());
        }
        let profile = args.get("profile").and_then(Value::as_str).unwrap_or(&self.profile);
        let page = args.get("page").and_then(Value::as_str).ok_or_else(|| ops::OpError::invalid("missing page"))?;
        let ff = args.get("formFactor").cloned().and_then(|f| serde_json::from_value(f).ok()).unwrap_or(FormFactor::PhonePortrait);
        let v = self.app.engine.lock().page_view(profile, page, ff, None);
        Ok(serde_json::to_value(v).unwrap_or(Value::Null))
    }

    async fn unlock(&mut self, pin: &str) -> anyhow::Result<()> {
        let res = self.app.devices.lock().check_pin(&self.who.device, pin);
        match res {
            PinCheck::Ok => {
                self.locked = false;
                self.link.send(&json!({ "t": "unlocked" })).await?;
                self.send_page(true).await?;
                self.send_status().await?;
            }
            PinCheck::Wrong => self.link.send(&json!({ "t": "locked", "reason": "wrong-pin" })).await?,
            PinCheck::Locked(d) => self.link.send(&json!({ "t": "locked", "reason": "too-many-tries", "retryAfter": d.as_secs() + 1 })).await?,
        }
        Ok(())
    }

    fn key(&self, id: &str) -> Option<crate::model::Key> {
        self.app.engine.lock().key(&self.profile, &self.page, id).cloned()
    }

    async fn down(&mut self, id: &str) -> anyhow::Result<()> {
        let Some(key) = self.key(id) else { return Ok(()) };
        match key.kind {
            KeyKind::Folder => {
                if let Some(t) = &key.target
                    && self.goto(t)
                {
                    self.send_page(true).await?;
                }
            }
            KeyKind::Back => {
                if self.back() {
                    self.send_page(true).await?;
                }
            }
            KeyKind::Slider | KeyKind::Widget if key.actions.is_empty() => {}
            KeyKind::Slider => {}
            // Already held on this device: a second finger, or a client repeating itself.
            _ if self.presses.is_down(&id.to_string()) => {}
            _ => {
                self.presses.down(&self.app, id.to_string(), key_path(&self.profile, &self.page, id), key.repeat.is_some());
                match key.hold.filter(|h| *h > 0) {
                    Some(ms) => {
                        self.hold_gen += 1;
                        let generation = self.hold_gen;
                        self.holds.insert(id.to_string(), generation);
                        let tx = self.itx.clone();
                        let key = id.to_string();
                        tokio::spawn(async move {
                            tokio::time::sleep(Duration::from_millis(ms as u64)).await;
                            let _ = tx.send(Internal::HoldDone { key, generation }).await;
                        });
                    }
                    None => self.fire(id).await,
                }
            }
        }
        Ok(())
    }

    async fn up(&mut self, id: &str, cancel: bool) -> anyhow::Result<()> {
        self.presses.up(&self.app, &id.to_string());
        if self.holds.remove(id).is_some() && !cancel {
            self.link.send(&json!({ "t": "toast", "text": "Keep holding to confirm", "tone": Tone::Warning, "at": now_ms(), "local": true })).await?;
        }
        Ok(())
    }

    fn ctx(&self, key: &str) -> Ctx {
        Ctx { device: self.who.device.clone(), profile: self.profile.clone(), page: self.page.clone(), key: key.to_string(), local: self.who.admin }
    }

    async fn fire(&mut self, id: &str) {
        let Some(key) = self.key(id) else { return };
        let (list, off) = if key.kind == KeyKind::Toggle {
            let on = self.app.engine.lock().flip_toggle(&key_path(&self.profile, &self.page, id));
            if on || key.off_actions.is_empty() { (key.actions.clone(), false) } else { (key.off_actions.clone(), true) }
        } else {
            (key.actions.clone(), false)
        };
        self.app.recorder.record(&list);
        self.app.notify(Change::Pressed { profile: self.profile.clone(), page: self.page.clone(), key: id.to_string(), label: key.label.clone(), device: self.who.name.clone() });
        if list.is_empty() {
            return;
        }
        let Some(running) = actions::Running::claim(&self.app, key_path(&self.profile, &self.page, id)) else { return };
        let app = self.app.clone();
        let ctx = self.ctx(id);
        self.presses.fired(&id.to_string(), ctx.clone(), key.actions);
        let repeat = key.repeat;
        tokio::spawn(async move {
            actions::run_key(app, ctx, list, off, repeat).await;
            drop(running);
        });
    }

    /// One value per slider at a time: values that arrive while one is on its way replace
    /// each other, and only the newest goes out next.
    async fn slide(&mut self, id: &str, value: f64) {
        let Some(key) = self.key(id) else { return };
        if key.kind != KeyKind::Slider {
            return;
        }
        let path = key_path(&self.profile, &self.page, id);
        let (profile, page, id) = (self.profile.clone(), self.page.clone(), id.to_string());
        actions::slide_one_at_a_time(&self.app, path, value, move |app, v| {
            let (profile, page, id, key) = (profile.clone(), page.clone(), id.clone(), key.clone());
            async move { apply_slide(&app, &profile, &page, &id, &key, v).await }
        });
    }
}

/// Sends one slider value to every step on the key, and waits until each has handled it.
pub(crate) async fn apply_slide(app: &Arc<App>, profile: &str, page: &str, id: &str, key: &crate::model::Key, value: f64) {
    for (n, a) in key.actions.iter().enumerate() {
        if a.review {
            continue;
        }
        match a.plugin() {
            Some((plugin, action)) => {
                let step = Step { instance: instance_id(profile, page, id, false, n), action: action.to_string(), settings: a.settings.clone() };
                if let Err(e) = app.plugins.slide(plugin, step, value).await {
                    app.engine.lock().toast(e, Tone::Danger);
                }
            }
            #[cfg(any(target_os = "linux", all(windows, feature = "appaudio")))]
            None if a.kind == crate::actions::appaudio::VOLUME => {
                if let Err(e) = crate::actions::appaudio::set_volume(&a.settings, value).await {
                    tracing::debug!("app volume: {e}");
                }
            }
            None if a.kind == "core.var" => {
                let Some(name) = a.settings.get("name").and_then(Value::as_str).filter(|n| n.starts_with("user.")) else { continue };
                let range = key.slider.as_ref().map(|s| (s.min, s.max)).unwrap_or((0.0, 1.0));
                let scaled = range.0 + (range.1 - range.0) * value;
                if let Some(num) = serde_json::Number::from_f64(scaled) {
                    app.engine.lock().set_var(name, Value::Number(num));
                }
            }
            None => {}
        }
    }
}

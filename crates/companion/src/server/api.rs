//! Inbound HTTP API, so other tools (Streamer.bot, scripts) can press keys, set variables
//! and change how keys look. Every request needs a token made in the editor.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::time::Instant;

use axum::extract::ws::{Message, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post, put};
use axum::{Json, Router};
use parking_lot::Mutex;
use serde_json::{Value, json};

use crate::actions::{self, Ctx, Nav};
use crate::app::App;
use crate::model::{self, Action};
use crate::state::view::ViewPatch;
use crate::state::{Change, key_path};
use crate::tokens::{ApiScope, Token};

fn err(code: StatusCode, msg: &str) -> Response {
    (code, Json(json!({ "error": msg }))).into_response()
}

fn buckets() -> &'static Mutex<HashMap<String, (f64, Instant)>> {
    static B: OnceLock<Mutex<HashMap<String, (f64, Instant)>>> = OnceLock::new();
    B.get_or_init(|| Mutex::new(HashMap::new()))
}

fn rate_ok(token: &str) -> bool {
    let mut b = buckets().lock();
    let now = Instant::now();
    let e = b.entry(token.to_string()).or_insert((40.0, now));
    e.0 = (e.0 + now.duration_since(e.1).as_secs_f64() * 20.0).min(40.0);
    e.1 = now;
    if e.0 < 1.0 {
        return false;
    }
    e.0 -= 1.0;
    true
}

fn auth(app: &App, headers: &HeaderMap, query_token: Option<&str>, scope: ApiScope) -> Result<Token, Response> {
    let bearer = headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("Bearer ")).or(query_token);
    let Some(given) = bearer else { return Err(err(StatusCode::UNAUTHORIZED, "missing token")) };
    let Some(t) = app.tokens.lock().check(given.trim()) else { return Err(err(StatusCode::UNAUTHORIZED, "invalid token")) };
    if !t.scopes.contains(&scope) {
        return Err(err(StatusCode::FORBIDDEN, "token lacks this permission"));
    }
    if !rate_ok(&t.id) {
        return Err(err(StatusCode::TOO_MANY_REQUESTS, "slow down"));
    }
    Ok(t)
}

async fn press(State(app): State<Arc<App>>, headers: HeaderMap, Json(b): Json<Value>) -> Response {
    let t = match auth(&app, &headers, None, ApiScope::Press) {
        Ok(t) => t,
        Err(r) => return r,
    };
    let page = b.get("page").and_then(Value::as_str).unwrap_or("");
    let key_id = b.get("key").and_then(Value::as_str).unwrap_or("");
    let (profile, key) = {
        let e = app.engine.lock();
        let profile = b.get("profile").and_then(Value::as_str).unwrap_or(e.active_profile()).to_string();
        let key = e.key(&profile, page, key_id).cloned();
        (profile, key)
    };
    let Some(key) = key else { return err(StatusCode::NOT_FOUND, "key not found") };
    let (list, off) = if key.kind == model::KeyKind::Toggle {
        let on = app.engine.lock().flip_toggle(&key_path(&profile, page, key_id));
        if on || key.off_actions.is_empty() { (key.actions.clone(), false) } else { (key.off_actions.clone(), true) }
    } else {
        (key.actions.clone(), false)
    };
    app.notify(Change::Pressed { profile: profile.clone(), page: page.into(), key: key_id.into(), label: key.label.clone(), device: format!("API: {}", t.name) });
    let ctx = Ctx { device: format!("api:{}", t.id), profile, page: page.into(), key: key_id.into(), local: false };
    tokio::spawn(actions::run_steps(app.clone(), ctx, list, off));
    StatusCode::ACCEPTED.into_response()
}

async fn run_action(State(app): State<Arc<App>>, headers: HeaderMap, Json(b): Json<Value>) -> Response {
    let t = match auth(&app, &headers, None, ApiScope::Press) {
        Ok(t) => t,
        Err(r) => return r,
    };
    let Ok(action) = serde_json::from_value::<Action>(b.get("action").cloned().unwrap_or(Value::Null)) else {
        return err(StatusCode::BAD_REQUEST, "invalid action");
    };
    if !model::valid_action_type(&action.kind) || actions::is_admin_only(&action) {
        return err(StatusCode::FORBIDDEN, "this action can't be run from the API");
    }
    let ctx = Ctx { device: format!("api:{}", t.id), profile: "api".into(), page: "api".into(), key: "api".into(), local: false };
    tokio::spawn(actions::run_steps(app.clone(), ctx, vec![Action { review: false, ..action }], false));
    StatusCode::ACCEPTED.into_response()
}

async fn list_vars(State(app): State<Arc<App>>, headers: HeaderMap) -> Response {
    if let Err(r) = auth(&app, &headers, None, ApiScope::Variables) {
        return r;
    }
    Json(json!({ "variables": app.engine.lock().vars_with_prefix("") })).into_response()
}

async fn set_var(State(app): State<Arc<App>>, headers: HeaderMap, Path(name): Path<String>, Json(b): Json<Value>) -> Response {
    if let Err(r) = auth(&app, &headers, None, ApiScope::Variables) {
        return r;
    }
    if !name.starts_with("user.") {
        return err(StatusCode::FORBIDDEN, "the API can only set variables starting with user.");
    }
    if !app.engine.lock().set_var(&name, b.get("value").cloned().unwrap_or(Value::Null)) {
        return err(StatusCode::BAD_REQUEST, "invalid variable or value");
    }
    StatusCode::NO_CONTENT.into_response()
}

async fn style_key(State(app): State<Arc<App>>, headers: HeaderMap, Path((profile, page, key)): Path<(String, String, String)>, Json(b): Json<Value>) -> Response {
    if let Err(r) = auth(&app, &headers, None, ApiScope::Display) {
        return r;
    }
    if app.engine.lock().key(&profile, &page, &key).is_none() {
        return err(StatusCode::NOT_FOUND, "key not found");
    }
    let patch = if b.is_null() {
        None
    } else {
        match serde_json::from_value::<ViewPatch>(b) {
            Ok(mut p) if p.image.is_none() => {
                for f in [&mut p.label, &mut p.sub, &mut p.badge, &mut p.meta] {
                    *f = f.take().map(|s| s.chars().filter(|c| !c.is_control()).take(64).collect());
                }
                if p.icon.as_deref().is_some_and(|i| !model::valid_icon(i)) || [p.value, p.progress].iter().flatten().any(|v| !(0.0..=1.0).contains(v)) {
                    return err(StatusCode::BAD_REQUEST, "invalid icon or value");
                }
                Some(p)
            }
            _ => return err(StatusCode::BAD_REQUEST, "invalid style"),
        }
    };
    app.engine.lock().set_key_override(&key_path(&profile, &page, &key), patch);
    StatusCode::NO_CONTENT.into_response()
}

async fn show_page(State(app): State<Arc<App>>, headers: HeaderMap, Json(b): Json<Value>) -> Response {
    if let Err(r) = auth(&app, &headers, None, ApiScope::Display) {
        return r;
    }
    let Some(page) = b.get("page").and_then(Value::as_str) else { return err(StatusCode::BAD_REQUEST, "missing page") };
    let device = b.get("device").and_then(Value::as_str).unwrap_or("*");
    app.navigate(device, Nav::Page(page.to_string()));
    StatusCode::ACCEPTED.into_response()
}

async fn activate_profile(State(app): State<Arc<App>>, headers: HeaderMap, Json(b): Json<Value>) -> Response {
    if let Err(r) = auth(&app, &headers, None, ApiScope::Display) {
        return r;
    }
    let Some(p) = b.get("profile").and_then(Value::as_str) else { return err(StatusCode::BAD_REQUEST, "missing profile") };
    if app.engine.lock().profile(p).is_none() {
        return err(StatusCode::NOT_FOUND, "profile not found");
    }
    app.navigate("*", Nav::Profile(p.to_string()));
    StatusCode::ACCEPTED.into_response()
}

/// Events out: key presses and profile switches. Token in the query, since many WebSocket
/// clients can't set headers.
async fn events(State(app): State<Arc<App>>, headers: HeaderMap, Query(q): Query<HashMap<String, String>>, ws: WebSocketUpgrade) -> Response {
    if let Err(r) = auth(&app, &headers, q.get("token").map(String::as_str), ApiScope::Press) {
        return r;
    }
    ws.max_message_size(4096).on_upgrade(move |mut socket| async move {
        let mut rx = app.engine.lock().subscribe();
        loop {
            tokio::select! {
                ch = rx.recv() => {
                    let msg = match ch {
                        Ok(Change::Pressed { profile, page, key, label, device }) => json!({ "event": "press", "profile": profile, "page": page, "key": key, "label": label, "device": device }),
                        Ok(Change::Profile) => json!({ "event": "profile", "profile": app.engine.lock().active_profile() }),
                        Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(_) => break,
                    };
                    if socket.send(Message::Text(msg.to_string().into())).await.is_err() {
                        break;
                    }
                }
                m = socket.recv() => if !matches!(m, Some(Ok(Message::Ping(_) | Message::Pong(_) | Message::Text(_)))) { break },
            }
        }
    })
}

pub fn router() -> Router<Arc<App>> {
    Router::new()
        .route("/keys/press", post(press))
        .route("/keys/{profile}/{page}/{key}", patch(style_key))
        .route("/actions/run", post(run_action))
        .route("/variables", get(list_vars))
        .route("/variables/{name}", put(set_var))
        .route("/pages/show", post(show_page))
        .route("/profiles/activate", post(activate_profile))
        .route("/events", get(events))
}

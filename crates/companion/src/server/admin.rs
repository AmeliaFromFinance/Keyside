//! Loopback-only listener for the desktop editor and the inbound API.
//! Requests must name this machine in the Host header (blocks DNS rebinding) and come
//! from this origin (blocks other websites in the same browser).

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::body::Bytes;
use axum::extract::ws::WebSocketUpgrade;
use axum::extract::{DefaultBodyLimit, Request, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{Value, json};
use subtle::ConstantTimeEq;

use super::conn::{self, Link, Who};
use crate::app::App;
use crate::devices::Scope;
use crate::plugins::Source;

const COOKIE: &str = "pd_admin";
const SESSION_IDLE: Duration = Duration::from_secs(14 * 86_400);

fn allowed_hosts(port: u16) -> [String; 3] {
    [format!("127.0.0.1:{port}"), format!("localhost:{port}"), format!("[::1]:{port}")]
}

fn origin_ok(h: &HeaderMap, port: u16) -> bool {
    let Some(o) = h.get(header::ORIGIN).and_then(|v| v.to_str().ok()) else { return false };
    allowed_hosts(port).iter().any(|host| o == format!("http://{host}"))
}

async fn host_guard(State(app): State<Arc<App>>, req: Request, next: Next) -> Response {
    let port = app.settings.read().admin_port;
    let host = req.headers().get(header::HOST).and_then(|v| v.to_str().ok()).unwrap_or("");
    if !allowed_hosts(port).iter().any(|h| h == host) {
        return (StatusCode::MISDIRECTED_REQUEST, "wrong host").into_response();
    }
    next.run(req).await
}

fn session(app: &App, h: &HeaderMap) -> bool {
    let Some(sid) = h
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|c| c.split(';'))
        .find_map(|kv| kv.trim().strip_prefix(&format!("{COOKIE}=")).map(str::to_string))
    else {
        return false;
    };
    let mut s = app.admin_sessions.lock();
    s.retain(|_, t| t.elapsed() < SESSION_IDLE);
    match s.get_mut(&sid) {
        Some(t) => {
            *t = Instant::now();
            true
        }
        None => false,
    }
}

/// Uploads and other state changes also need a custom header, which plain HTML forms can't send.
fn csrf_ok(app: &App, h: &HeaderMap) -> bool {
    let port = app.settings.read().admin_port;
    origin_ok(h, port) && h.get("x-keyside").is_some() && session(app, h)
}

async fn login(State(app): State<Arc<App>>, headers: HeaderMap, Json(body): Json<Value>) -> Response {
    let port = app.settings.read().admin_port;
    if !origin_ok(&headers, port) {
        return StatusCode::FORBIDDEN.into_response();
    }
    {
        let f = app.login_fails.lock();
        if f.1.is_some_and(|until| until > Instant::now()) {
            return (StatusCode::TOO_MANY_REQUESTS, "Too many tries. Wait a minute.").into_response();
        }
    }
    let given = body.get("token").and_then(Value::as_str).unwrap_or("");
    let ok = {
        let id = app.identity.read();
        bool::from(id.admin_token.as_bytes().ct_eq(given.as_bytes()))
    };
    if !ok {
        let mut f = app.login_fails.lock();
        f.0 += 1;
        if f.0 >= 5 {
            f.1 = Some(Instant::now() + Duration::from_secs(60));
            f.0 = 0;
        }
        return (StatusCode::UNAUTHORIZED, "Sign-in link is not valid. Open the editor from the Keyside app.").into_response();
    }
    *app.login_fails.lock() = (0, None);
    let sid = crate::ids::random_b64(32);
    app.admin_sessions.lock().insert(sid.clone(), Instant::now());
    let cookie = format!("{COOKIE}={sid}; HttpOnly; SameSite=Strict; Path=/; Max-Age={}", SESSION_IDLE.as_secs());
    ([(header::SET_COOKIE, cookie)], Json(json!({ "ok": true }))).into_response()
}

async fn logout(State(app): State<Arc<App>>, headers: HeaderMap) -> Response {
    if csrf_ok(&app, &headers) {
        app.admin_sessions.lock().clear();
    }
    ([(header::SET_COOKIE, format!("{COOKIE}=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0"))], StatusCode::NO_CONTENT).into_response()
}

async fn status(State(app): State<Arc<App>>, headers: HeaderMap) -> Response {
    Json(json!({ "signedIn": session(&app, &headers) })).into_response()
}

async fn admin_ws(State(app): State<Arc<App>>, headers: HeaderMap, ws: WebSocketUpgrade) -> Response {
    let port = app.settings.read().admin_port;
    if !origin_ok(&headers, port) || !session(&app, &headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    ws.max_message_size(2 * 1024 * 1024).on_upgrade(move |socket| async move {
        let who = Who { admin: true, device: "local".into(), name: "This PC".into(), scope: Scope::Editor, pages: None };
        conn::run(app, Link::Plain(socket), who, false).await;
    })
}

async fn upload_plugin(State(app): State<Arc<App>>, headers: HeaderMap, body: Bytes) -> Response {
    if !csrf_ok(&app, &headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    match crate::plugins::package::Package::from_zip(&body) {
        Ok(pkg) => Json(super::ops::stage(&app, pkg, Source::File)).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, format!("{e:#}")).into_response(),
    }
}

async fn upload_streamdeck(State(app): State<Arc<App>>, headers: HeaderMap, body: Bytes) -> Response {
    if !csrf_ok(&app, &headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    match crate::streamdeck::stage(&app, &body) {
        Ok(v) => Json(v).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, format!("{e:#}")).into_response(),
    }
}

async fn upload_asset(State(app): State<Arc<App>>, headers: HeaderMap, body: Bytes) -> Response {
    if !csrf_ok(&app, &headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let name = headers.get("x-file-name").and_then(|v| v.to_str().ok()).unwrap_or("file");
    match app.assets.put(name, &body) {
        Ok(m) => Json(m).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, format!("{e:#}")).into_response(),
    }
}

/// The file name comes in the query (?name=), since headers can't hold every character.
async fn upload_iconpack(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    axum::extract::Query(q): axum::extract::Query<std::collections::HashMap<String, String>>,
    body: Bytes,
) -> Response {
    if !csrf_ok(&app, &headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let name = q.get("name").cloned().unwrap_or_else(|| "Icons".into());
    let in_use = super::ops::icons_in_use(&app);
    let a = app.clone();
    let done = tokio::task::spawn_blocking(move || crate::iconpack::parse(&body, &name).and_then(|p| a.assets.add_pack(p, &in_use))).await;
    match done {
        Ok(Ok((pack, skipped))) => Json(json!({ "id": pack.id, "name": pack.name, "icons": pack.icons.len(), "skipped": skipped })).into_response(),
        Ok(Err(e)) => (StatusCode::BAD_REQUEST, format!("{e:#}")).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// Makes a backup file for download. A passphrase adds passwords and keys, encrypted.
async fn backup_create(State(app): State<Arc<App>>, headers: HeaderMap, Json(body): Json<Value>) -> Response {
    if !csrf_ok(&app, &headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let pass = body.get("passphrase").and_then(Value::as_str).filter(|p| !p.is_empty()).map(str::to_string);
    let a = app.clone();
    match tokio::task::spawn_blocking(move || crate::backup::create(&a, pass.as_deref())).await {
        Ok(Ok(bytes)) => {
            let date = time::OffsetDateTime::now_utc().date();
            let name = format!("keyside-backup-{}.zip", date);
            (
                [
                    (header::CONTENT_TYPE, "application/zip".to_string()),
                    (header::CONTENT_DISPOSITION, format!("attachment; filename=\"{name}\"")),
                    (header::CACHE_CONTROL, "no-store".into()),
                ],
                bytes,
            )
                .into_response()
        }
        Ok(Err(e)) => (StatusCode::BAD_REQUEST, format!("{e:#}")).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// Reads an uploaded backup and keeps it until the person confirms the restore.
async fn backup_inspect(State(app): State<Arc<App>>, headers: HeaderMap, body: Bytes) -> Response {
    if !csrf_ok(&app, &headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    match tokio::task::spawn_blocking(move || crate::backup::read(&body)).await {
        Ok(Ok(b)) => {
            let id = crate::ids::random_b64(12);
            let info = json!({ "id": id, "manifest": b.manifest });
            *app.staged_backup.lock() = Some((id, Instant::now(), b));
            Json(info).into_response()
        }
        Ok(Err(e)) => (StatusCode::BAD_REQUEST, format!("{e:#}")).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// Where OAuth providers send the browser back after sign-in. No session cookie arrives on
/// this cross-site redirect, so the random single-use state is what ties it to a plugin.
async fn oauth_callback(State(app): State<Arc<App>>, axum::extract::Query(q): axum::extract::Query<std::collections::HashMap<String, String>>) -> Response {
    let state = q.get("state").map(String::as_str).unwrap_or("");
    let result = match (q.get("code"), q.get("error")) {
        (Some(code), None) if !code.is_empty() && code.len() <= 2048 => Ok(code.clone()),
        (_, Some(e)) if e == "access_denied" => Err("sign-in was cancelled".to_string()),
        _ => Err("sign-in failed".to_string()),
    };
    let ok = result.is_ok();
    let delivered = !state.is_empty() && app.plugins.oauth_finish(state, result);
    let (title, body) = match (delivered, ok) {
        (true, true) => ("Signed in", "You can close this tab and go back to Keyside."),
        (true, false) => ("Sign-in cancelled", "Nothing was changed. You can close this tab."),
        (false, _) => ("Link expired", "This sign-in link was already used or has expired. Start again from Keyside."),
    };
    let html = format!(
        "<!doctype html><meta charset=utf-8><meta name=viewport content=\"width=device-width\"><title>{title}</title><body style=\"font:16px system-ui;background:#121118;color:#edebf2;display:grid;place-items:center;height:90vh\"><div><h1 style=\"font-size:1.4rem\">{title}</h1><p>{body}</p></div>"
    );
    (
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CONTENT_SECURITY_POLICY, "default-src 'none'; style-src 'unsafe-inline'"),
            (header::REFERRER_POLICY, "no-referrer"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        html,
    )
        .into_response()
}

async fn fallback(State(app): State<Arc<App>>, uri: axum::http::Uri) -> Response {
    let sd = crate::streamdeck::inspector_origin(&app);
    web::serve(uri, sd).await
}

use super::web;

pub async fn start(app: Arc<App>, port: u16) -> anyhow::Result<()> {
    let router = Router::new()
        .route("/admin/status", get(status))
        .route("/admin/login", post(login))
        .route("/admin/logout", post(logout))
        .route("/admin/ws", get(admin_ws))
        .route("/oauth/callback", get(oauth_callback))
        .route("/admin/upload/plugin", post(upload_plugin).layer(DefaultBodyLimit::max(crate::plugins::package::MAX_TOTAL as usize)))
        .route("/admin/upload/streamdeck", post(upload_streamdeck).layer(DefaultBodyLimit::max(crate::plugins::package::MAX_TOTAL as usize)))
        .route("/admin/upload/iconpack", post(upload_iconpack).layer(DefaultBodyLimit::max(crate::iconpack::MAX_ZIP as usize)))
        .route("/admin/upload/asset", post(upload_asset).layer(DefaultBodyLimit::max(crate::assets::MAX_SOUND)))
        .route("/admin/backup", post(backup_create))
        .route("/admin/backup/inspect", post(backup_inspect).layer(DefaultBodyLimit::max(crate::backup::MAX_SIZE)))
        .nest("/api/v1", super::api::router())
        .nest("/overlay", super::overlay::router())
        .fallback(fallback)
        .layer(middleware::from_fn_with_state(app.clone(), host_guard))
        .layer(DefaultBodyLimit::max(256 * 1024))
        .with_state(app.clone());
    let listener = tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], port))).await.map_err(|e| anyhow::anyhow!("editor port {port} is busy: {e}"))?;
    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, router.into_make_service_with_connect_info::<SocketAddr>()).await {
            tracing::error!("editor listener stopped: {e}");
        }
    });
    tracing::info!("editor on http://127.0.0.1:{port}");
    Ok(())
}

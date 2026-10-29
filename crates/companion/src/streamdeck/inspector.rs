//! Serves Stream Deck settings panels (property inspectors) to the desktop editor.
//!
//! Panels are plugin code, so they load from their own loopback origin inside a sandboxed
//! iframe: they can't touch the editor page, and the editor's API rejects their origin.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use serde_json::{Value, json};

use super::{Inspector, PREFIX};
use crate::app::App;
use crate::plugins::package::safe_path;

const UNUSED_TTL: Duration = Duration::from_secs(600);
const MAX_FILE: u64 = 10 * 1024 * 1024;

const BOOT: &str = r#"(() => {
  let p;
  try { p = JSON.parse(decodeURIComponent(location.hash.slice(1))); } catch { return; }
  const go = () => {
    const f = window.connectElgatoStreamDeckSocket || window.connectSocket;
    if (typeof f === "function") f(p.port, p.uuid, "registerPropertyInspector", JSON.stringify(p.info), JSON.stringify(p.actionInfo));
  };
  if (document.readyState === "complete") setTimeout(go, 0);
  else window.addEventListener("load", () => setTimeout(go, 0));
})();
"#;

pub async fn listen(app: Arc<App>) -> Result<()> {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.context("starting the settings panel server")?;
    let port = listener.local_addr()?.port();
    let _ = app.streamdeck.inspector_port.set(port);
    let router = Router::new()
        .route("/__phonedeck/boot.js", get(boot))
        .route("/p/{uuid}/{*path}", get(file))
        .with_state(app.clone());
    let v6 = tokio::net::TcpListener::bind(("::1", port)).await.ok();
    let r2 = router.clone();
    tokio::spawn(async move {
        let _ = axum::serve(listener, router.into_make_service_with_connect_info::<SocketAddr>()).await;
    });
    // "localhost" may resolve to ::1 first.
    if let Some(v6) = v6 {
        tokio::spawn(async move {
            let _ = axum::serve(v6, r2.into_make_service_with_connect_info::<SocketAddr>()).await;
        });
    }
    Ok(())
}

fn host_ok(app: &App, h: &HeaderMap) -> bool {
    let Some(port) = app.streamdeck.inspector_port.get() else { return false };
    let host = h.get(header::HOST).and_then(|v| v.to_str().ok()).unwrap_or("");
    host == format!("localhost:{port}")
}

fn headers(app: &App, uuid: &str, mime: &str) -> HeaderMap {
    let admin = app.streamdeck.admin_port.get().copied().unwrap_or(7442);
    let sd = app.streamdeck.port.get().copied().unwrap_or(0);
    let hosts: Vec<String> = app.streamdeck.get(uuid).map(|i| i.net.iter().map(|h| format!("https://{h}")).collect()).unwrap_or_default();
    let csp = format!(
        "default-src 'self'; script-src 'self' 'unsafe-inline' https://sdpi-components.dev; style-src 'self' 'unsafe-inline' https://sdpi-components.dev; \
         img-src 'self' data: blob:; font-src 'self' data:; connect-src 'self' ws://127.0.0.1:{sd} {}; object-src 'none'; base-uri 'self'; form-action 'none'; \
         frame-ancestors http://127.0.0.1:{admin} http://localhost:{admin} http://[::1]:{admin}",
        hosts.join(" ")
    );
    let mut h = HeaderMap::new();
    let mut set = |k: header::HeaderName, v: &str| {
        if let Ok(v) = HeaderValue::from_str(v) {
            h.insert(k, v);
        }
    };
    set(header::CONTENT_TYPE, mime);
    set(header::CONTENT_SECURITY_POLICY, &csp);
    set(header::X_CONTENT_TYPE_OPTIONS, "nosniff");
    set(header::REFERRER_POLICY, "no-referrer");
    set(header::CACHE_CONTROL, "no-store");
    set(header::HeaderName::from_static("cross-origin-resource-policy"), "same-origin");
    h
}

async fn boot(State(app): State<Arc<App>>, h: HeaderMap) -> Response {
    if !host_ok(&app, &h) {
        return StatusCode::MISDIRECTED_REQUEST.into_response();
    }
    ([(header::CONTENT_TYPE, "text/javascript; charset=utf-8"), (header::CACHE_CONTROL, "no-store")], BOOT).into_response()
}

fn mime(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("").to_ascii_lowercase().as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        _ => "application/octet-stream",
    }
}

async fn file(State(app): State<Arc<App>>, h: HeaderMap, Path((uuid, path)): Path<(String, String)>) -> Response {
    if !host_ok(&app, &h) {
        return StatusCode::MISDIRECTED_REQUEST.into_response();
    }
    if !app.streamdeck.get(&uuid).is_some_and(|i| i.enabled) || !safe_path(&path) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let dir = app.streamdeck.plugin_dir(&uuid);
    let found = (|| {
        let real_dir = dir.canonicalize().ok()?;
        let real = dir.join(&path).canonicalize().ok()?;
        (real.starts_with(&real_dir) && real.is_file() && real.metadata().ok()?.len() <= MAX_FILE).then_some(real)
    })();
    let Some(real) = found else { return StatusCode::NOT_FOUND.into_response() };
    let Ok(mut bytes) = tokio::fs::read(&real).await else { return StatusCode::NOT_FOUND.into_response() };
    let m = mime(&path);
    if m.starts_with("text/html") {
        bytes = inject(&bytes);
    }
    (headers(&app, &uuid, m), Body::from(bytes)).into_response()
}

/// Adds the script that hands the panel its connection details.
fn inject(html: &[u8]) -> Vec<u8> {
    let tag = b"<script src=\"/__phonedeck/boot.js\"></script>";
    let lower = html.to_ascii_lowercase();
    let at = lower.windows(7).rposition(|w| w == b"</body>").unwrap_or(html.len());
    let mut out = Vec::with_capacity(html.len() + tag.len());
    out.extend_from_slice(&html[..at]);
    out.extend_from_slice(tag);
    out.extend_from_slice(&html[at..]);
    out
}

/// Creates a one-time panel session for one key and returns the URL to frame.
pub fn open(app: &Arc<App>, instance: &str) -> Result<Value> {
    let (profile, page, key, n) = super::split_instance(instance).context("that key has no Stream Deck action")?;
    let (kind, settings, x, y) = {
        let e = app.engine.lock();
        let k = e.key(profile, page, key).context("key not found")?;
        let a = k.actions.get(n).context("action not found")?;
        (a.kind.clone(), a.settings.clone(), k.x, k.y)
    };
    let (plugin, action) = kind.strip_prefix(PREFIX).and_then(|k| k.split_once('/')).context("that isn't a Stream Deck action")?;
    let inst = app.streamdeck.get(plugin).context("plugin not installed")?;
    let def = inst.manifest.action(action).context("the plugin no longer has this action")?;
    let pi = def.property_inspector_path.clone().or(inst.manifest.property_inspector_path.clone()).context("this action has no settings")?;
    let port = *app.streamdeck.inspector_port.get().context("settings server not started")?;
    let sd_port = *app.streamdeck.port.get().context("plugin socket not started")?;
    let token = crate::ids::random_b64(24);
    {
        let mut st = app.streamdeck.state.lock();
        st.inspectors.retain(|_, i| i.tx.is_some() || i.created.elapsed() < UNUSED_TTL);
        st.inspectors.insert(token.clone(), Inspector { plugin: plugin.to_string(), context: instance.to_string(), action: action.to_string(), tx: None, created: Instant::now() });
    }
    let params = json!({
        "port": sd_port,
        "uuid": token,
        "info": super::runtime::info(app, &inst),
        "actionInfo": { "action": action, "context": instance, "device": super::runtime::DEVICE, "payload": { "settings": settings, "coordinates": { "column": x, "row": y } } },
    });
    // byte_serialize writes spaces as "+", which decodeURIComponent and URL paths don't undo.
    let enc = |s: &str| url::form_urlencoded::byte_serialize(s.as_bytes()).collect::<String>().replace('+', "%20");
    let hash = enc(&params.to_string());
    let path = pi.split('/').map(enc).collect::<Vec<_>>().join("/");
    Ok(json!({ "url": format!("http://localhost:{port}/p/{plugin}/{path}#{hash}"), "name": def.name }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn injects_before_the_body_ends() {
        let out = String::from_utf8(inject(b"<html><body><p>x</p></BODY></html>")).unwrap();
        assert!(out.contains("<p>x</p><script src=\"/__phonedeck/boot.js\"></script></BODY>"));
        let bare = String::from_utf8(inject(b"<p>no body</p>")).unwrap();
        assert!(bare.ends_with("</script>"));
    }
}

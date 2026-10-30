//! Browser source pages for OBS: a big timer and any text with {variables}. They need no
//! sign-in, so they only answer this PC, only show filled-in variable values, and change
//! nothing.

use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use axum::Router;
use axum::extract::{ConnectInfo, Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use serde_json::{Value, json};
use tokio::sync::broadcast::error::RecvError;

use crate::actions::timers;
use crate::app::App;
use crate::state::Change;
use crate::state::view::{display, fill, truthy};

const MAX_TEMPLATE: usize = 500;
const MAX_OPEN: usize = 64;
static OPEN: AtomicUsize = AtomicUsize::new(0);

const SCRIPT: &str = include_str!("../../assets/overlay/overlay.js");
const STYLE: &str = include_str!("../../assets/overlay/overlay.css");
const PAGE: &str = "<!doctype html><html><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width\"><title>Keyside overlay</title>\
<link rel=\"stylesheet\" href=\"/overlay/overlay.css\"></head><body><div id=\"out\"></div><script src=\"/overlay/overlay.js\"></script></body></html>";

pub fn router() -> Router<Arc<App>> {
    Router::new()
        .route("/timer/{name}", get(timer_page))
        .route("/text", get(text_page))
        .route("/events", get(events))
        .route("/overlay.js", get(script))
        .route("/overlay.css", get(style))
}

/// What one overlay shows.
#[derive(Debug, Clone, PartialEq)]
enum Spec {
    Timer(String),
    Text(String),
}

impl Spec {
    fn from_query(q: &HashMap<String, String>) -> Option<Self> {
        match (q.get("timer"), q.get("t")) {
            (Some(n), None) if timers::valid_name(n) => Some(Self::Timer(n.clone())),
            (None, Some(t)) if !t.trim().is_empty() && t.len() <= MAX_TEMPLATE && !t.chars().any(|c| c.is_control() && c != '\n') => Some(Self::Text(t.clone())),
            _ => None,
        }
    }

    /// Only the filled-in text leaves the companion, never the template sources or anything else.
    fn render(&self, get: impl Fn(&str) -> Option<Value>) -> Value {
        match self {
            Self::Timer(n) => json!({
                "text": get(&format!("timer.{n}")).map(|v| display(&v)).unwrap_or_else(|| "00:00".into()),
                "running": get(&format!("timer.{n}.running")).is_some_and(|v| truthy(&v)),
            }),
            Self::Text(t) => json!({ "text": fill(t, get, MAX_TEMPLATE) }),
        }
    }
}

/// Loopback only, and not from other websites open in a browser on this PC.
fn allowed(addr: &SocketAddr, h: &HeaderMap, port: u16) -> bool {
    if !addr.ip().to_canonical().is_loopback() {
        return false;
    }
    let site = h.get("sec-fetch-site").and_then(|v| v.to_str().ok()).unwrap_or("none");
    if site == "cross-site" || site == "same-site" {
        return false;
    }
    match h.get(header::ORIGIN).and_then(|v| v.to_str().ok()) {
        None => true,
        Some(o) => [format!("http://127.0.0.1:{port}"), format!("http://localhost:{port}"), format!("http://[::1]:{port}")].iter().any(|ok| ok == o),
    }
}

fn harden(r: &mut Response) {
    let h = r.headers_mut();
    let csp = "default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'";
    h.insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static(csp));
    h.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    h.insert(header::REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    h.insert(header::HeaderName::from_static("cross-origin-resource-policy"), HeaderValue::from_static("same-origin"));
}

fn refuse() -> Response {
    let mut r = (StatusCode::FORBIDDEN, "Overlays only open on this PC, for example as an OBS browser source.").into_response();
    harden(&mut r);
    r
}

fn reply(body: &'static str, kind: &'static str) -> Response {
    let mut r = ([(header::CONTENT_TYPE, kind)], body).into_response();
    harden(&mut r);
    r
}

async fn script(ConnectInfo(addr): ConnectInfo<SocketAddr>, State(app): State<Arc<App>>, headers: HeaderMap) -> Response {
    if !allowed(&addr, &headers, app.settings.read().admin_port) {
        return refuse();
    }
    reply(SCRIPT, "text/javascript; charset=utf-8")
}

async fn style(ConnectInfo(addr): ConnectInfo<SocketAddr>, State(app): State<Arc<App>>, headers: HeaderMap) -> Response {
    if !allowed(&addr, &headers, app.settings.read().admin_port) {
        return refuse();
    }
    reply(STYLE, "text/css; charset=utf-8")
}

async fn timer_page(ConnectInfo(addr): ConnectInfo<SocketAddr>, State(app): State<Arc<App>>, headers: HeaderMap, Path(name): Path<String>) -> Response {
    if !allowed(&addr, &headers, app.settings.read().admin_port) {
        return refuse();
    }
    if !timers::valid_name(&name) {
        return (StatusCode::NOT_FOUND, "Timer names are short lowercase letters, digits, - or _.").into_response();
    }
    reply(PAGE, "text/html; charset=utf-8")
}

async fn text_page(ConnectInfo(addr): ConnectInfo<SocketAddr>, State(app): State<Arc<App>>, headers: HeaderMap, Query(q): Query<HashMap<String, String>>) -> Response {
    if !allowed(&addr, &headers, app.settings.read().admin_port) {
        return refuse();
    }
    if !matches!(Spec::from_query(&q), Some(Spec::Text(_))) {
        return (StatusCode::BAD_REQUEST, "Add the text to show, like ?t={twitch.viewers} viewers").into_response();
    }
    reply(PAGE, "text/html; charset=utf-8")
}

/// Counts open overlay streams, so a runaway page can't hold the companion busy.
struct Slot;

impl Slot {
    fn take() -> Option<Self> {
        // Take a slot, then give it back if that went over the limit.
        if OPEN.fetch_add(1, Ordering::SeqCst) < MAX_OPEN {
            Some(Self)
        } else {
            OPEN.fetch_sub(1, Ordering::SeqCst);
            None
        }
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        OPEN.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Server-sent events: the filled-in text, again each time it changes.
async fn events(ConnectInfo(addr): ConnectInfo<SocketAddr>, State(app): State<Arc<App>>, headers: HeaderMap, Query(q): Query<HashMap<String, String>>) -> Response {
    if !allowed(&addr, &headers, app.settings.read().admin_port) {
        return refuse();
    }
    let Some(spec) = Spec::from_query(&q) else { return StatusCode::BAD_REQUEST.into_response() };
    let Some(slot) = Slot::take() else { return (StatusCode::TOO_MANY_REQUESTS, "too many overlays open").into_response() };
    let rx = app.engine.lock().subscribe();
    let stream = futures_util::stream::unfold((app, rx, spec, None::<Value>, slot), |(app, mut rx, spec, last, slot)| async move {
        loop {
            let now = {
                let e = app.engine.lock();
                spec.render(|n| e.var(n))
            };
            if last.as_ref() != Some(&now) {
                let ev = Event::default().data(now.to_string());
                return Some((Ok::<_, Infallible>(ev), (app, rx, spec, Some(now), slot)));
            }
            match rx.recv().await {
                Ok(Change::Vars) | Err(RecvError::Lagged(_)) => {}
                Ok(_) => continue,
                Err(RecvError::Closed) => return None,
            }
        }
    });
    let mut r = Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))).into_response();
    harden(&mut r);
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn reads_what_to_show() {
        assert_eq!(Spec::from_query(&q(&[("timer", "break")])), Some(Spec::Timer("break".into())));
        assert_eq!(Spec::from_query(&q(&[("timer", "Break")])), None);
        assert_eq!(Spec::from_query(&q(&[("timer", "../x")])), None);
        assert_eq!(Spec::from_query(&q(&[("t", "{twitch.viewers} viewers")])), Some(Spec::Text("{twitch.viewers} viewers".into())));
        assert_eq!(Spec::from_query(&q(&[("t", " ")])), None);
        assert_eq!(Spec::from_query(&q(&[("t", &"x".repeat(501))])), None);
        assert_eq!(Spec::from_query(&q(&[("t", "a"), ("timer", "b")])), None);
    }

    #[test]
    fn shows_only_filled_in_values() {
        let vars = |n: &str| match n {
            "twitch.viewers" => Some(json!(42)),
            "timer.break" => Some(json!("04:59")),
            "timer.break.running" => Some(json!(true)),
            _ => None,
        };
        assert_eq!(Spec::Text("{twitch.viewers} viewers{nope}".into()).render(vars), json!({ "text": "42 viewers" }));
        assert_eq!(Spec::Timer("break".into()).render(vars), json!({ "text": "04:59", "running": true }));
        assert_eq!(Spec::Timer("other".into()).render(vars), json!({ "text": "00:00", "running": false }));
    }

    #[test]
    fn only_this_pc_and_not_other_sites() {
        let local: SocketAddr = "127.0.0.1:5000".parse().unwrap();
        let lan: SocketAddr = "192.168.1.20:5000".parse().unwrap();
        let mut h = HeaderMap::new();
        assert!(allowed(&local, &h, 7640));
        assert!(!allowed(&lan, &h, 7640));
        h.insert("sec-fetch-site", HeaderValue::from_static("cross-site"));
        assert!(!allowed(&local, &h, 7640));
        h.insert("sec-fetch-site", HeaderValue::from_static("same-origin"));
        h.insert(header::ORIGIN, HeaderValue::from_static("http://127.0.0.1:7640"));
        assert!(allowed(&local, &h, 7640));
        h.insert(header::ORIGIN, HeaderValue::from_static("http://evil.example"));
        assert!(!allowed(&local, &h, 7640));
    }
}

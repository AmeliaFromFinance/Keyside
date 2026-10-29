//! Serves the built web client (apps/client/dist) with strict security headers.

use axum::body::Body;
use axum::http::{HeaderMap, HeaderValue, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};

#[derive(rust_embed::RustEmbed)]
#[folder = "../../apps/client/dist"]
#[allow_missing = true]
struct Client;

/// Content-Security-Policy for every page. `frame` lists origins allowed in iframes
/// (the Stream Deck settings panels on the local editor).
pub fn csp(frame: Option<&str>) -> String {
    format!(
        "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data: blob:; font-src 'self'; \
         connect-src 'self' ws: wss:; media-src 'self' data:; worker-src 'self'; manifest-src 'self'; object-src 'none'; \
         base-uri 'none'; form-action 'none'; frame-ancestors 'none'; frame-src {}",
        frame.unwrap_or("'none'")
    )
}

pub fn secure_headers(h: &mut HeaderMap, frame: Option<&str>) {
    let set = |h: &mut HeaderMap, k: header::HeaderName, v: &str| {
        if let Ok(v) = HeaderValue::from_str(v) {
            h.insert(k, v);
        }
    };
    set(h, header::CONTENT_SECURITY_POLICY, &csp(frame));
    set(h, header::X_CONTENT_TYPE_OPTIONS, "nosniff");
    set(h, header::REFERRER_POLICY, "no-referrer");
    set(h, header::X_FRAME_OPTIONS, "DENY");
    set(h, header::HeaderName::from_static("cross-origin-opener-policy"), "same-origin");
    set(h, header::HeaderName::from_static("cross-origin-resource-policy"), "same-origin");
    set(h, header::HeaderName::from_static("permissions-policy"), "camera=(self), microphone=(), geolocation=(), usb=(), payment=()");
}

fn mime(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "webmanifest" => "application/manifest+json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

pub async fn serve(uri: Uri, frame: Option<String>) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };
    let (file, name) = match Client::get(path) {
        Some(f) => (f, path.to_string()),
        // Unknown paths without a file extension get the app shell.
        None if path == "index.html" || !path.rsplit('/').next().unwrap_or("").contains('.') => match Client::get("index.html") {
            Some(f) => (f, "index.html".to_string()),
            None => return missing(),
        },
        None => {
            let mut r = StatusCode::NOT_FOUND.into_response();
            secure_headers(r.headers_mut(), None);
            return r;
        }
    };
    let mut resp = Response::new(Body::from(file.data.into_owned()));
    let h = resp.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static(mime(&name)));
    let cache = if name.starts_with("assets/") { "public, max-age=31536000, immutable" } else { "no-cache" };
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    secure_headers(h, frame.as_deref());
    resp
}

fn missing() -> Response {
    let mut r = (StatusCode::SERVICE_UNAVAILABLE, "The PhoneDeck web client is not built. Run `pnpm build` in apps/client.").into_response();
    secure_headers(r.headers_mut(), None);
    r
}

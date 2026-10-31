//! LAN listener for paired devices. One port serves:
//! - HTTPS (browsers): the web deck and the device socket,
//! - plain HTTP (native apps): the device socket only.
//!
//! The device socket is end-to-end encrypted either way, so plain HTTP leaks nothing.

use std::collections::HashMap;
use std::io;
use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;
use std::sync::{Arc, OnceLock};
use std::task::{Context, Poll};
use std::time::Duration;

use axum::Router;
use axum::extract::connect_info::Connected;
use axum::extract::ws::WebSocketUpgrade;
use axum::extract::{ConnectInfo, DefaultBodyLimit, Request, State};
use axum::http::{HeaderValue, StatusCode, Uri, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::get;
use axum::serve::{IncomingStream, Listener};
use parking_lot::Mutex;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_rustls::TlsAcceptor;
use tokio_rustls::server::TlsStream;

use super::{handshake, web};
use crate::app::App;

const MAX_PER_IP: usize = 32;
const MAX_TOTAL: usize = 512;

#[derive(Debug, Clone, Copy)]
pub struct Peer {
    pub addr: SocketAddr,
    pub tls: bool,
}

fn counts() -> &'static Mutex<HashMap<IpAddr, usize>> {
    static C: OnceLock<Mutex<HashMap<IpAddr, usize>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

struct Guard(IpAddr);

impl Guard {
    fn take(ip: IpAddr) -> Option<Self> {
        let mut c = counts().lock();
        let total: usize = c.values().sum();
        if total >= MAX_TOTAL || c.get(&ip).is_some_and(|n| *n >= MAX_PER_IP) {
            return None;
        }
        *c.entry(ip).or_default() += 1;
        Some(Self(ip))
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        let mut c = counts().lock();
        if let Some(n) = c.get_mut(&self.0) {
            *n -= 1;
            if *n == 0 {
                c.remove(&self.0);
            }
        }
    }
}

enum Inner {
    Plain(TcpStream),
    Tls(Box<TlsStream<TcpStream>>),
}

pub struct Io {
    inner: Inner,
    _guard: Guard,
}

impl AsyncRead for Io {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        match &mut self.inner {
            Inner::Plain(s) => Pin::new(s).poll_read(cx, buf),
            Inner::Tls(s) => Pin::new(s.as_mut()).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for Io {
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        match &mut self.inner {
            Inner::Plain(s) => Pin::new(s).poll_write(cx, buf),
            Inner::Tls(s) => Pin::new(s.as_mut()).poll_write(cx, buf),
        }
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match &mut self.inner {
            Inner::Plain(s) => Pin::new(s).poll_flush(cx),
            Inner::Tls(s) => Pin::new(s.as_mut()).poll_flush(cx),
        }
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match &mut self.inner {
            Inner::Plain(s) => Pin::new(s).poll_shutdown(cx),
            Inner::Tls(s) => Pin::new(s.as_mut()).poll_shutdown(cx),
        }
    }
}

struct LanListener {
    rx: mpsc::Receiver<(Io, Peer)>,
    local: SocketAddr,
}

impl Listener for LanListener {
    type Io = Io;
    type Addr = Peer;

    async fn accept(&mut self) -> (Io, Peer) {
        match self.rx.recv().await {
            Some(c) => c,
            None => std::future::pending().await,
        }
    }

    fn local_addr(&self) -> io::Result<Peer> {
        Ok(Peer { addr: self.local, tls: false })
    }
}

impl Connected<IncomingStream<'_, LanListener>> for Peer {
    fn connect_info(stream: IncomingStream<'_, LanListener>) -> Self {
        *stream.remote_addr()
    }
}

async fn classify(tcp: TcpStream, addr: SocketAddr, acceptor: TlsAcceptor, tx: mpsc::Sender<(Io, Peer)>) {
    let Some(guard) = Guard::take(addr.ip()) else { return };
    let _ = tcp.set_nodelay(true);
    let mut first = [0u8; 1];
    let peeked = tokio::time::timeout(Duration::from_secs(10), tcp.peek(&mut first)).await;
    if !matches!(peeked, Ok(Ok(1))) {
        return;
    }
    let (inner, tls) = if first[0] == 0x16 {
        match tokio::time::timeout(Duration::from_secs(10), acceptor.accept(tcp)).await {
            Ok(Ok(s)) => (Inner::Tls(Box::new(s)), true),
            _ => return,
        }
    } else {
        (Inner::Plain(tcp), false)
    };
    let _ = tx.send((Io { inner, _guard: guard }, Peer { addr, tls })).await;
}

async fn device_ws(State(app): State<Arc<App>>, ConnectInfo(peer): ConnectInfo<Peer>, ws: WebSocketUpgrade) -> Response {
    let ip = peer.addr.ip();
    if !handshake::admit(ip) {
        return (StatusCode::TOO_MANY_REQUESTS, "slow down").into_response();
    }
    ws.max_message_size(crate::crypto::channel::MAX_FRAME + 64).on_upgrade(move |socket| handshake::run(app, socket, ip))
}

/// The web deck needs HTTPS (browsers only allow its crypto there). Plain requests are redirected.
async fn fallback(ConnectInfo(peer): ConnectInfo<Peer>, State(app): State<Arc<App>>, req: Request) -> Response {
    if peer.tls {
        return match req.uri().path() {
            "/keyside.mobileconfig" => certificate(&app, true),
            "/keyside-ca.cer" => certificate(&app, false),
            _ => web::serve(req.uri().clone(), None).await,
        };
    }
    let port = app.settings.read().lan_port;
    let host = req.headers().get(header::HOST).and_then(|h| h.to_str().ok()).unwrap_or("");
    let bare = host.rsplit_once(':').map(|(h, _)| h).unwrap_or(host);
    let safe = !bare.is_empty() && bare.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'[' | b']' | b':'));
    if !safe {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let path = req.uri().path_and_query().map(|p| p.as_str()).unwrap_or("/");
    match format!("https://{bare}:{port}{path}").parse::<Uri>() {
        Ok(u) => Redirect::permanent(&u.to_string()).into_response(),
        Err(_) => StatusCode::BAD_REQUEST.into_response(),
    }
}

/// The certificate authority, as an iPhone profile or a plain certificate for other phones.
fn certificate(app: &App, profile: bool) -> Response {
    let ca = app.tls_ca.read().clone();
    if ca.is_empty() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let (body, kind, file) = if profile {
        (mobileconfig(&ca, &app.settings.read().name).into_bytes(), "application/x-apple-aspen-config", "keyside.mobileconfig")
    } else {
        (ca, "application/x-x509-ca-cert", "keyside-ca.cer")
    };
    let mut r = (StatusCode::OK, body).into_response();
    let h = r.headers_mut();
    web::secure_headers(h, None);
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static(kind));
    if let Ok(v) = HeaderValue::from_str(&format!("attachment; filename=\"{file}\"")) {
        h.insert(header::CONTENT_DISPOSITION, v);
    }
    r
}

/// An iPhone configuration profile that installs the authority as a trusted root.
pub(crate) fn mobileconfig(ca: &[u8], pc: &str) -> String {
    use base64::Engine as _;
    use sha2::Digest as _;
    let hash = sha2::Sha256::digest(ca);
    // Stable ids, so installing again replaces the old profile instead of adding another.
    let uuid = |salt: u8| {
        let b: Vec<u8> = hash.iter().map(|x| x ^ salt).take(16).collect();
        format!(
            "{:02X}{:02X}{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7], b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]
        )
    };
    let esc = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    let pc = esc(&pc.chars().filter(|c| !c.is_control()).take(60).collect::<String>());
    let data = base64::engine::general_purpose::STANDARD.encode(ca);
    let (outer, inner) = (uuid(0), uuid(0x5a));
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>PayloadContent</key>
  <array>
    <dict>
      <key>PayloadCertificateFileName</key><string>keyside-ca.cer</string>
      <key>PayloadContent</key><data>{data}</data>
      <key>PayloadDisplayName</key><string>Keyside local network</string>
      <key>PayloadIdentifier</key><string>dev.keyside.ca.{inner}</string>
      <key>PayloadType</key><string>com.apple.security.root</string>
      <key>PayloadUUID</key><string>{inner}</string>
      <key>PayloadVersion</key><integer>1</integer>
    </dict>
  </array>
  <key>PayloadDescription</key><string>Lets this device open Keyside on {pc} without a certificate warning. It only covers local network addresses.</string>
  <key>PayloadDisplayName</key><string>Keyside on {pc}</string>
  <key>PayloadIdentifier</key><string>dev.keyside.profile.{outer}</string>
  <key>PayloadType</key><string>Configuration</string>
  <key>PayloadUUID</key><string>{outer}</string>
  <key>PayloadVersion</key><integer>1</integer>
</dict>
</plist>
"#
    )
}

/// The inbound API on the LAN is opt-in and HTTPS only.
async fn api_guard(ConnectInfo(peer): ConnectInfo<Peer>, State(app): State<Arc<App>>, req: Request, next: Next) -> Response {
    if !peer.tls || !app.settings.read().api_on_lan {
        return StatusCode::NOT_FOUND.into_response();
    }
    next.run(req).await
}

pub async fn start(app: Arc<App>, bind: IpAddr, port: u16, tls: Arc<rustls::ServerConfig>) -> anyhow::Result<()> {
    let tcp = TcpListener::bind(SocketAddr::new(bind, port)).await.map_err(|e| anyhow::anyhow!("deck port {port} is busy: {e}"))?;
    let local = tcp.local_addr()?;
    let acceptor = TlsAcceptor::from(tls);
    let (tx, rx) = mpsc::channel(64);
    tokio::spawn(async move {
        loop {
            match tcp.accept().await {
                Ok((s, addr)) => {
                    tokio::spawn(classify(s, addr, acceptor.clone(), tx.clone()));
                }
                Err(e) => {
                    tracing::warn!("accept failed: {e}");
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        }
    });
    let api = super::api::router().layer(middleware::from_fn_with_state(app.clone(), api_guard));
    let router = Router::new().route("/ws", get(device_ws)).nest("/api/v1", api).fallback(fallback).layer(DefaultBodyLimit::max(64 * 1024)).with_state(app.clone());
    tokio::spawn(async move {
        if let Err(e) = axum::serve(LanListener { rx, local }, router.into_make_service_with_connect_info::<Peer>()).await {
            tracing::error!("deck listener stopped: {e}");
        }
    });
    tracing::info!("deck on port {port}");
    Ok(())
}

//! Built-in actions.

use std::time::Duration;

use serde_json::{Map, Value};

use super::{Ctx, Nav};
use crate::app::App;
use crate::model::{Action, valid_var_name};
use crate::state::view::display;

type Res = Result<Option<Nav>, String>;

fn s<'a>(m: &'a Map<String, Value>, k: &str) -> Option<&'a str> {
    m.get(k).and_then(Value::as_str).filter(|v| !v.is_empty())
}

fn req<'a>(m: &'a Map<String, Value>, k: &str) -> Result<&'a str, String> {
    s(m, k).ok_or_else(|| format!("missing setting \"{k}\""))
}

fn num(m: &Map<String, Value>, k: &str) -> Option<f64> {
    m.get(k).and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|s| s.trim().parse().ok())))
}

/// Fills {variables} in user text. `encode` escapes each value for where it lands.
pub fn fill(app: &App, text: &str, max: usize, encode: impl Fn(&str) -> String) -> String {
    let engine = app.engine.lock();
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('}') {
            Some(end) if valid_var_name(&after[..end]) => {
                let v = engine.var(&after[..end]).map(|v| display(&v)).unwrap_or_default();
                out.push_str(&encode(&v));
                rest = &after[end + 1..];
            }
            _ => {
                out.push('{');
                rest = after;
            }
        }
        if out.len() > max {
            break;
        }
    }
    out.push_str(rest);
    out.truncate(floor_char(&out, max));
    out
}

fn floor_char(s: &str, max: usize) -> usize {
    if s.len() <= max {
        return s.len();
    }
    (0..=max).rev().find(|i| s.is_char_boundary(*i)).unwrap_or(0)
}

fn http_url(u: &str) -> Result<url::Url, String> {
    let parsed = url::Url::parse(u).map_err(|_| "invalid URL".to_string())?;
    if !matches!(parsed.scheme(), "http" | "https") || u.len() > 4096 {
        return Err("only http and https links are allowed".into());
    }
    Ok(parsed)
}

pub async fn run(app: &App, ctx: &Ctx, a: &Action) -> Res {
    let m = &a.settings;
    match a.kind.as_str() {
        "core.wait" => {
            let ms = num(m, "ms").unwrap_or(0.0).clamp(0.0, 60_000.0);
            tokio::time::sleep(Duration::from_millis(ms as u64)).await;
            Ok(None)
        }
        "core.webhook" => webhook(app, m).await,
        "core.open" => {
            let u = http_url(req(m, "url")?)?;
            open::that_detached(u.as_str()).map_err(|e| format!("could not open the link: {e}"))?;
            Ok(None)
        }
        "core.launch" => launch(m),
        "core.keys" => {
            app.input.keys(req(m, "keys")?)?;
            Ok(None)
        }
        "core.text" => {
            let t = req(m, "text")?;
            if t.chars().count() > 2000 {
                return Err("text is longer than 2000 characters".into());
            }
            app.input.text(t)?;
            Ok(None)
        }
        "core.media" => {
            app.input.media(req(m, "key")?)?;
            Ok(None)
        }
        "core.osc" => osc(m).await,
        "core.udp" => udp(app, m).await,
        "core.websocket" => websocket(app, m).await,
        "core.midi" => midi(m),
        "core.sound" => sound(app, m),
        "core.var" => var(app, m),
        "core.timer" => {
            app.timers.apply(app, m)?;
            Ok(None)
        }
        "core.page" => Ok(Some(Nav::Page(req(m, "page")?.to_string()))),
        "core.profile" => Ok(Some(Nav::Profile(req(m, "profile")?.to_string()))),
        other => {
            let _ = ctx;
            Err(format!("unknown action {other}"))
        }
    }
}

async fn webhook(app: &App, m: &Map<String, Value>) -> Res {
    let url = fill(app, req(m, "url")?, 4096, |v| url::form_urlencoded::byte_serialize(v.as_bytes()).collect());
    let url = http_url(&url)?;
    let method = s(m, "method").unwrap_or("POST").to_ascii_uppercase();
    let method = reqwest::Method::from_bytes(method.as_bytes()).map_err(|_| "invalid method".to_string())?;
    if !matches!(method.as_str(), "GET" | "POST" | "PUT" | "PATCH" | "DELETE") {
        return Err("invalid method".into());
    }
    let mut rb = app.http.request(method, url).timeout(Duration::from_secs(10));
    if let Some(h) = m.get("headers").and_then(Value::as_object) {
        for (k, v) in h.iter().take(32) {
            if let Some(v) = v.as_str() {
                rb = rb.header(k.as_str(), v);
            }
        }
    }
    if let Some(body) = s(m, "body") {
        let ct = s(m, "contentType").unwrap_or("application/json").to_string();
        let json = ct.contains("json");
        let body = fill(app, body, 64 * 1024, |v| if json { serde_json::to_string(v).map(|q| q[1..q.len() - 1].to_string()).unwrap_or_default() } else { v.to_string() });
        rb = rb.header(reqwest::header::CONTENT_TYPE, ct).body(body);
    }
    let resp = rb.send().await.map_err(|e| format!("webhook failed: {}", e.without_url()))?;
    if !resp.status().is_success() {
        return Err(format!("webhook answered {}", resp.status()));
    }
    Ok(None)
}

fn launch(m: &Map<String, Value>) -> Res {
    let path = std::path::PathBuf::from(req(m, "path")?);
    if !path.is_absolute() {
        return Err("program path must be absolute".into());
    }
    let args: Vec<String> = lines(m.get("args")).into_iter().take(32).collect();
    let mut cmd = std::process::Command::new(&path);
    cmd.args(args).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    if let Some(dir) = path.parent() {
        cmd.current_dir(dir);
    }
    cmd.spawn().map_err(|e| format!("could not start {}: {e}", path.display()))?;
    Ok(None)
}

/// One value per line from a text field, or the items of a list.
fn lines(v: Option<&Value>) -> Vec<String> {
    match v {
        Some(Value::String(s)) => s.lines().map(str::trim).filter(|l| !l.is_empty()).map(str::to_string).collect(),
        Some(Value::Array(a)) => a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect(),
        _ => Vec::new(),
    }
}

fn host_port(m: &Map<String, Value>) -> Result<(String, u16), String> {
    let host = req(m, "host")?.to_string();
    let port = num(m, "port").filter(|p| (1.0..=65535.0).contains(p)).ok_or("invalid port")? as u16;
    Ok((host, port))
}

async fn osc(m: &Map<String, Value>) -> Res {
    use rosc::{OscMessage, OscPacket, OscType};
    let (host, port) = host_port(m)?;
    let addr = req(m, "address")?;
    if !addr.starts_with('/') {
        return Err("OSC address must start with /".into());
    }
    let args = lines(m.get("args"))
        .into_iter()
        .take(16)
        .map(|v| match v.as_str() {
            "true" => OscType::Bool(true),
            "false" => OscType::Bool(false),
            s => match (s.parse::<i32>(), s.parse::<f32>()) {
                (Ok(i), _) => OscType::Int(i),
                (_, Ok(f)) => OscType::Float(f),
                _ => OscType::String(s.to_string()),
            },
        })
        .collect();
    let packet = rosc::encoder::encode(&OscPacket::Message(OscMessage { addr: addr.to_string(), args })).map_err(|e| e.to_string())?;
    send_udp(&host, port, &packet).await
}

async fn udp(app: &App, m: &Map<String, Value>) -> Res {
    let (host, port) = host_port(m)?;
    let payload = fill(app, req(m, "payload")?, 8192, str::to_string);
    send_udp(&host, port, payload.as_bytes()).await
}

async fn send_udp(host: &str, port: u16, data: &[u8]) -> Res {
    let sock = tokio::net::UdpSocket::bind(("0.0.0.0", 0)).await.map_err(|e| e.to_string())?;
    tokio::time::timeout(Duration::from_secs(3), sock.send_to(data, (host, port)))
        .await
        .map_err(|_| "timed out".to_string())?
        .map_err(|e| format!("send failed: {e}"))?;
    Ok(None)
}

async fn websocket(app: &App, m: &Map<String, Value>) -> Res {
    use futures_util::SinkExt;
    let url = req(m, "url")?;
    let parsed = url::Url::parse(url).map_err(|_| "invalid URL")?;
    if !matches!(parsed.scheme(), "ws" | "wss") {
        return Err("WebSocket URL must start with ws:// or wss://".into());
    }
    let msg = fill(app, req(m, "message")?, 64 * 1024, str::to_string);
    let (mut ws, _) = tokio::time::timeout(Duration::from_secs(5), tokio_tungstenite::connect_async(parsed.as_str()))
        .await
        .map_err(|_| "timed out".to_string())?
        .map_err(|e| format!("could not connect: {e}"))?;
    ws.send(tokio_tungstenite::tungstenite::Message::text(msg)).await.map_err(|e| e.to_string())?;
    let _ = ws.close(None).await;
    Ok(None)
}

#[cfg(feature = "midi")]
fn midi(m: &Map<String, Value>) -> Res {
    let port = req(m, "port")?;
    let channel = num(m, "channel").unwrap_or(1.0).clamp(1.0, 16.0) as u8 - 1;
    let d1 = num(m, "data1").unwrap_or(0.0).clamp(0.0, 127.0) as u8;
    let d2 = num(m, "data2").unwrap_or(127.0).clamp(0.0, 127.0) as u8;
    let msg: Vec<u8> = match req(m, "message")? {
        "note-on" => vec![0x90 | channel, d1, d2],
        "note-off" => vec![0x80 | channel, d1, d2],
        "cc" => vec![0xB0 | channel, d1, d2],
        "program" => vec![0xC0 | channel, d1],
        _ => return Err("unknown MIDI message".into()),
    };
    super::midi::send(port, &msg)?;
    Ok(None)
}

#[cfg(not(feature = "midi"))]
fn midi(_: &Map<String, Value>) -> Res {
    Err("MIDI is not included in this build".into())
}

#[cfg(feature = "sound")]
fn sound(app: &App, m: &Map<String, Value>) -> Res {
    let id = req(m, "asset")?;
    let path = app.assets.sound_path(id).ok_or("sound file not found")?;
    let volume = num(m, "volume").unwrap_or(1.0).clamp(0.0, 1.0) as f32;
    app.sound.play(path, volume, s(m, "mode") == Some("stop-others"))?;
    Ok(None)
}

#[cfg(not(feature = "sound"))]
fn sound(_: &App, _: &Map<String, Value>) -> Res {
    Err("Sound playback is not included in this build".into())
}

fn var(app: &App, m: &Map<String, Value>) -> Res {
    let name = req(m, "name")?;
    if !name.starts_with("user.") || !valid_var_name(name) {
        return Err("keys can only change variables that start with \"user.\"".into());
    }
    let mut e = app.engine.lock();
    let cur = e.var(name).unwrap_or(Value::Null);
    let next = match s(m, "op").unwrap_or("set") {
        "set" => match m.get("value") {
            Some(Value::String(t)) if t == "true" || t == "false" => Value::Bool(t == "true"),
            Some(Value::String(t)) => t.trim().parse::<f64>().ok().and_then(serde_json::Number::from_f64).map(Value::Number).unwrap_or_else(|| Value::String(t.clone())),
            Some(v) => v.clone(),
            None => Value::Null,
        },
        "toggle" => Value::Bool(!crate::state::view::truthy(&cur)),
        "inc" | "dec" => {
            let by = num(m, "value").unwrap_or(1.0) * if s(m, "op") == Some("dec") { -1.0 } else { 1.0 };
            serde_json::Number::from_f64(cur.as_f64().unwrap_or(0.0) + by).map(Value::Number).unwrap_or(Value::Null)
        }
        _ => return Err("unknown variable operation".into()),
    };
    if !e.set_var(name, next) {
        return Err("could not set the variable".into());
    }
    Ok(None)
}

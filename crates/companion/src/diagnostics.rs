//! A plain-text report for bug reports. It holds versions, settings that matter, counts, plugin
//! health and recent log lines, and leaves out anything private: no tokens, passwords, PINs,
//! keys, pairing links, PC or device names, addresses of other devices or layout content.

use std::fmt::Write as _;
use std::sync::{Arc, LazyLock};

use regex::{Captures, Regex};
use serde_json::{Value, json};

use crate::app::App;
use crate::state::all_keys;

const LOG_LINES: usize = 100;
const MASK: &str = "[hidden]";

fn re(p: &str) -> Regex {
    Regex::new(p).expect("valid pattern")
}

static AUTH: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)\b(bearer|basic)\s+[A-Za-z0-9._~+/=-]+"));
static PAIRS: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r#"(?i)\b([a-z_.-]*(?:token|password|passwd|pwd|secret|passphrase|api[_-]?key|apikey|pin|auth|authorization|cookie|session|key))("?\s*[=:]\s*)("[^"]*"|'[^']*'|[^\s&,;"']+)"#,
    )
});
static URL_TAIL: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)\b((?:https?|wss?)://[^\s?#]+)[?#]\S*"));
static HEX: LazyLock<Regex> = LazyLock::new(|| re(r"\b[0-9a-fA-F]{32,}\b"));
static B64: LazyLock<Regex> = LazyLock::new(|| re(r"[A-Za-z0-9+/_-]{20,}={0,2}"));
static IPV4: LazyLock<Regex> = LazyLock::new(|| re(r"\b(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})\b"));
static IPV6: LazyLock<Regex> = LazyLock::new(|| re(r"\b(?:[0-9a-fA-F]{0,4}:){3,7}[0-9a-fA-F]{1,4}\b"));

/// Masks anything that looks like a secret or another device address: `token=…` and similar,
/// bearer headers, link queries and fragments (pairing links), long hex or base64 runs, and
/// IP addresses other than this PC's loopback.
pub fn redact(s: &str) -> String {
    let s = AUTH.replace_all(s, format!("$1 {MASK}"));
    let s = PAIRS.replace_all(&s, format!("${{1}}${{2}}{MASK}"));
    let s = URL_TAIL.replace_all(&s, format!("${{1}}#{MASK}"));
    let s = HEX.replace_all(&s, MASK);
    let s = B64.replace_all(&s, |c: &Captures| {
        let m = &c[0];
        let mixed = m.chars().any(|c| c.is_ascii_uppercase()) && m.chars().any(|c| c.is_ascii_lowercase()) && m.chars().any(|c| c.is_ascii_digit());
        if mixed { MASK.to_string() } else { m.to_string() }
    });
    let s = IPV4.replace_all(&s, |c: &Captures| if &c[1] == "127" || &c[0] == "0.0.0.0" { c[0].to_string() } else { format!("{}.{}.x.x", &c[1], &c[2]) });
    IPV6.replace_all(&s, "[IPv6]").into_owned()
}

fn utc(ms: u64) -> String {
    let t = time::OffsetDateTime::from_unix_timestamp((ms / 1000) as i64).unwrap_or(time::OffsetDateTime::UNIX_EPOCH);
    format!("{:04}-{:02}-{:02} {:02}:{:02} UTC", t.year(), t.month() as u8, t.day(), t.hour(), t.minute())
}

fn duration(secs: u64) -> String {
    let (d, h, m) = (secs / 86_400, secs / 3600 % 24, secs / 60 % 60);
    match (d, h) {
        (0, 0) => format!("{m} min"),
        (0, _) => format!("{h} h {m} min"),
        _ => format!("{d} d {h} h {m} min"),
    }
}

fn on(b: bool) -> &'static str {
    if b { "on" } else { "off" }
}

/// One line of plugin health, like the Health panel shows it.
fn health_line(h: Option<&Value>, now: u64) -> String {
    let Some(h) = h else { return "no data".into() };
    let mut out = match h.get("runningSince").and_then(Value::as_u64) {
        Some(since) => format!("running for {}", duration(now.saturating_sub(since) / 1000)),
        None => "not running".into(),
    };
    let n = |k: &str| h.get(k).and_then(Value::as_u64).unwrap_or(0);
    let _ = write!(out, ", last hour: {} restarts, {} crashes", n("restarts"), n("crashes"));
    if let Some(e) = h.get("lastError").filter(|e| !e.is_null()) {
        let _ = write!(out, ", last error {}: {}", utc(e["at"].as_u64().unwrap_or(0)), e["text"].as_str().unwrap_or(""));
    }
    out
}

/// The report, ready to paste into a bug report.
pub async fn report(app: &Arc<App>) -> String {
    let now = crate::server::conn::now_ms();
    let st = app.settings.read().clone();
    let mut r = String::new();
    let w = &mut r;

    let _ = writeln!(w, "Keyside diagnostics, {}", utc(now));
    let _ = writeln!(w, "Version: {}", env!("CARGO_PKG_VERSION"));
    let _ = writeln!(w, "System: {} {} ({})", std::env::consts::OS, std::env::consts::ARCH, std::env::consts::FAMILY);
    let _ = writeln!(w, "Desktop app: {}", if app.zoom_hook.read().is_some() { "yes" } else { "no" });
    let _ = writeln!(w, "Uptime: {}", duration(app.started.elapsed().as_secs()));
    let _ = writeln!(w, "Plugin runtime (Deno): {}", if crate::plugins::sandbox::find_deno().is_some() { "found" } else { "missing" });

    let _ = writeln!(w, "\nSettings");
    let _ = writeln!(w, "  Editor port: {}", st.admin_port);
    let _ = writeln!(w, "  Device port: {}", st.lan_port);
    let _ = writeln!(w, "  Paired devices can connect: {}", on(st.lan_enabled));
    let _ = writeln!(w, "  Listens on: {}", if st.lan_bind.is_unspecified() { "every network" } else { "one address" });
    let _ = writeln!(w, "  HTTP API from other computers: {}", on(st.api_on_lan));
    let _ = writeln!(w, "  Announce on the network (mDNS): {}", on(st.mdns));
    let _ = writeln!(w, "  Stream Deck hardware: {}", on(st.hardware_decks));
    let _ = writeln!(w, "  Update check: {}", on(st.update_check));
    let _ = writeln!(w, "  Follow the app in front: {}", on(st.follow_focus));
    let _ = writeln!(w, "  Automatic backups: {}", on(st.auto_backup));
    let _ = writeln!(w, "  Language: {}", st.locale);
    let _ = writeln!(w, "  Plugin registry: {}", if st.registry_url == crate::config::DEFAULT_REGISTRY { "default" } else { "custom" });

    let (layouts, pages, keys) = {
        let e = app.engine.lock();
        let l = e.layout();
        (l.profiles.len(), l.profiles.iter().map(|p| p.pages.len()).sum::<usize>(), all_keys(l).count())
    };
    let devices = app.devices.lock().list(&|id| app.is_online(id));
    let _ = writeln!(w, "\nCounts");
    let _ = writeln!(w, "  Layouts: {layouts}, pages: {pages}, keys: {keys}");
    let _ = writeln!(w, "  Paired devices: {}, online now: {}", devices.len(), devices.iter().filter(|d| d.online).count());
    let _ = writeln!(w, "  Templates: {}, images and sounds: {}, API tokens: {}", app.templates.list().len(), app.assets.list().len(), app.tokens.lock().list().len());
    let autos = crate::autobackup::list(app).unwrap_or_default();
    let _ = writeln!(w, "  Automatic backups kept: {}{}", autos.len(), autos.first().map(|b| format!(", newest {}", utc(b.created))).unwrap_or_default());

    let health = app.plugins.health();
    let _ = writeln!(w, "\nPlugins");
    let plugins = app.plugins.list();
    if plugins.is_empty() {
        let _ = writeln!(w, "  none");
    }
    for p in &plugins {
        let detail = app.engine.lock().connection(&p.id).and_then(|c| c.detail).map(|d| format!(" ({d})")).unwrap_or_default();
        let _ = writeln!(
            w,
            "  {} {}: {}, status {}{}, {}",
            p.id,
            p.version,
            if p.enabled { "enabled" } else { "disabled" },
            p.status,
            detail,
            if p.in_use { "in use" } else { "not in use" }
        );
        let _ = writeln!(w, "    {}", health_line(health.get(&p.id), now));
    }

    let _ = writeln!(w, "\nStream Deck plugins");
    let sd = crate::streamdeck::op(app, "streamdeck.list", &json!({})).await.unwrap_or(Value::Null);
    let sd_list = sd["plugins"].as_array().cloned().unwrap_or_default();
    if sd_list.is_empty() {
        let _ = writeln!(w, "  none");
    }
    for p in &sd_list {
        let uuid = p["uuid"].as_str().unwrap_or("");
        let _ = writeln!(
            w,
            "  {} {}: {}, {}{}",
            uuid,
            p["version"].as_str().unwrap_or("?"),
            if p["enabled"].as_bool().unwrap_or(false) { "enabled" } else { "disabled" },
            if p["running"].as_bool().unwrap_or(false) { "running" } else { "stopped" },
            if p["blocked"].as_bool().unwrap_or(false) { ", blocked" } else { "" },
        );
        let _ = writeln!(w, "    {}", health_line(health.get(format!("sd.{uuid}")), now));
    }

    let _ = writeln!(w, "\nHardware and system");
    let decks = crate::streamdeck::hardware::connected();
    let _ = writeln!(w, "  Stream Decks plugged in: {}", if decks.is_empty() { "none".to_string() } else { decks.join(", ") });
    #[cfg(feature = "nowplaying")]
    let media = match crate::actions::nowplaying::available() {
        Some(true) => "available",
        Some(false) => "no media service found",
        None => "not checked yet",
    };
    #[cfg(not(feature = "nowplaying"))]
    let media = "not in this build";
    let _ = writeln!(w, "  Now playing: {media}");
    #[cfg(any(target_os = "linux", all(windows, feature = "appaudio")))]
    let audio = match crate::actions::appaudio::availability() {
        Ok(()) => "available".to_string(),
        Err(why) => format!("off ({why})"),
    };
    #[cfg(not(any(target_os = "linux", all(windows, feature = "appaudio"))))]
    let audio = "not on this system".to_string();
    let _ = writeln!(w, "  Per-app audio: {audio}");

    let _ = writeln!(w, "\nRecent log (info and above, oldest first)");
    let lines = crate::logs::recent(LOG_LINES);
    if lines.is_empty() {
        let _ = writeln!(w, "  none kept");
    }
    for l in lines {
        let _ = writeln!(w, "  {l}");
    }

    // Names people gave the PC and their devices can show up in plugin messages and logs.
    let mut private: Vec<String> = devices.into_iter().map(|d| d.name).collect();
    private.push(st.name.clone());
    if let Some(home) = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }) {
        r = r.replace(&*home.to_string_lossy(), "~");
    }
    for n in private.iter().filter(|n| n.chars().count() >= 3) {
        r = r.replace(n.as_str(), "[name]");
    }
    redact(&r)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::devices::Scope;

    #[test]
    fn masks_secrets() {
        let cases = [
            ("login with token=abc123XYZ done", "abc123XYZ"),
            ("password: hunter2", "hunter2"),
            (r#"{"apiKey":"s3cr3t-value"}"#, "s3cr3t-value"),
            ("Authorization: Bearer eyJhbGciOiJIUzI1NiJ9.e30.sig", "eyJhbGciOiJIUzI1NiJ9"),
            ("open https://192.168.1.20:7443/#pair&h=x&k=Q2xhdWRlQ29kZQ", "k=Q2xhdWRlQ29kZQ"),
            ("sha 0123456789abcdef0123456789abcdef01234567", "0123456789abcdef0123456789abcdef"),
            ("raw pd_Zm9vYmFyYmF6cXV4MTIzNDU2Nzg5MA", "Zm9vYmFyYmF6cXV4MTIzNDU2Nzg5MA"),
            ("phone 192.168.1.20 connected", "192.168.1.20"),
            ("phone fd7a:115c:a1e0::1234 connected", "fd7a:115c"),
        ];
        for (input, secret) in cases {
            let out = redact(input);
            assert!(!out.contains(secret), "{input} -> {out}");
            assert!(out.contains(MASK) || out.contains("x.x") || out.contains("[IPv6]"), "{out}");
        }
        for keep in ["editor on http://127.0.0.1:7442", "plugin dev.keyside.obs-websocket started", "12:30:05 INFO listening on 0.0.0.0:7443", "keyside-backup-2026-10-31.zip"] {
            assert_eq!(redact(keep), keep);
        }
    }

    #[tokio::test]
    async fn report_has_the_version_and_no_secrets() {
        let t = tempfile::tempdir().unwrap();
        let app = App::new(crate::app::Options { data_dir: Some(t.path().into()), bundled_plugins: None }).unwrap();
        let (_, secret) = app.tokens.lock().create("Bot", vec![crate::tokens::ApiScope::Press]).unwrap();
        let (pid, code) = app.devices.lock().start_pairing(Scope::Deck, vec![], Some("4711")).unwrap();
        let link = crate::server::pairing_payload(&app, &pid, &code)["url"].as_str().unwrap().to_string();
        app.plugins.secrets.set("dev.keyside.obs", "password", Some("plugin-secret-1")).unwrap();
        app.settings.write().name = "Amelias Desktop".into();
        crate::logs::push(&format!("debug pairing {link} with token={secret}"));

        let r = report(&app).await;
        assert!(r.contains(&format!("Version: {}", env!("CARGO_PKG_VERSION"))));
        assert!(r.contains("Layouts: 1"));
        let id = app.identity.read().clone();
        for leak in [secret.as_str(), &code, &pid, &link, "4711", "plugin-secret-1", &id.admin_token, &id.cid, "Amelias Desktop"] {
            assert!(!r.contains(leak), "the report shows {leak}");
        }
    }
}

//! Per-app volume and mute on Linux through `pactl`, which PipeWire and PulseAudio both answer.
//! `pactl subscribe` reports changes, so keys follow what happens in other mixers too.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use serde_json::{Map, Value};
use tokio::io::AsyncBufReadExt;
use tokio::process::Command;

use crate::app::App;
use crate::model::Tone;
use crate::state::Change;
use crate::state::view::ViewPatch;

pub const MUTE: &str = "core.app-mute";
pub const VOLUME: &str = "core.app-volume";

#[derive(Debug, Clone, PartialEq)]
pub struct Stream {
    pub index: u32,
    pub name: String,
    pub binary: String,
    pub muted: bool,
    /// 1.0 is 100%.
    pub volume: f64,
}

impl Stream {
    fn is(&self, app: &str) -> bool {
        !app.is_empty() && (self.name.eq_ignore_ascii_case(app) || self.binary.eq_ignore_ascii_case(app))
    }
}

#[derive(Default)]
struct Seen {
    streams: Vec<Stream>,
    /// Apps that played sound since Keyside started, for the editor's list.
    names: BTreeSet<String>,
    available: bool,
}

static STATE: std::sync::LazyLock<Mutex<Seen>> = std::sync::LazyLock::new(Mutex::default);

async fn pactl(args: &[&str]) -> Result<String, String> {
    let out = Command::new("pactl").args(args).stdin(std::process::Stdio::null()).kill_on_drop(true).output().await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound { "pactl isn't installed (it comes with pipewire-pulse or pulseaudio-utils)".to_string() } else { e.to_string() }
    })?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Parses `pactl -f json list sink-inputs`.
pub fn parse(json: &str) -> Vec<Stream> {
    let Ok(Value::Array(list)) = serde_json::from_str::<Value>(json) else { return Vec::new() };
    list.iter()
        .filter_map(|s| {
            let props = s.get("properties")?;
            let prop = |k: &str| props.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
            let channels: Vec<f64> = s.get("volume")?.as_object()?.values().filter_map(|c| c.get("value").and_then(Value::as_f64)).collect();
            let volume = channels.iter().copied().fold(0.0, f64::max) / 65536.0;
            let name = prop("application.name");
            let binary = prop("application.process.binary");
            let index = u32::try_from(s.get("index")?.as_u64()?).ok()?;
            (!name.is_empty() || !binary.is_empty()).then(|| Stream {
                index,
                name: if name.is_empty() { binary.clone() } else { name },
                binary,
                muted: s.get("mute").and_then(Value::as_bool).unwrap_or(false),
                volume,
            })
        })
        .collect()
}

async fn refresh(app: &App) {
    let streams = match pactl(&["-f", "json", "list", "sink-inputs"]).await {
        Ok(j) => parse(&j),
        Err(_) => Vec::new(),
    };
    {
        let mut st = STATE.lock();
        for s in &streams {
            if st.names.len() < 200 {
                st.names.insert(s.name.clone());
            }
        }
        st.streams = streams;
    }
    show(app);
}

fn show(app: &App) {
    let (streams, available) = {
        let st = STATE.lock();
        (st.streams.clone(), st.available)
    };
    let mut e = app.engine.lock();
    for kind in [MUTE, VOLUME] {
        for inst in e.instances_of(kind) {
            let name = inst.settings.get("app").and_then(Value::as_str).unwrap_or_default();
            let mine: Vec<&Stream> = streams.iter().filter(|s| s.is(name)).collect();
            let patch = if !available {
                ViewPatch { disabled: Some(true), sub: Some("Needs pactl".into()), ..Default::default() }
            } else if mine.is_empty() {
                ViewPatch { sub: Some("No sound from it".into()), tone: Some(Tone::Muted), ..Default::default() }
            } else {
                let muted = mine.iter().all(|s| s.muted);
                let vol = mine.iter().map(|s| s.volume).fold(0.0, f64::max);
                let mut p = ViewPatch { meta: Some(format!("{}%", (vol * 100.0).round())), ..Default::default() };
                if kind == VOLUME {
                    p.value = Some(vol.clamp(0.0, 1.0));
                }
                if muted {
                    p.tone = Some(Tone::Danger);
                    p.badge = Some("MUTED".into());
                    p.badge_tone = Some(Tone::Danger);
                    p.icon = Some("volume-x".into());
                } else if kind == MUTE {
                    p.badge = Some("ON".into());
                    p.badge_tone = Some(Tone::Success);
                }
                p
            };
            e.set_override(&inst.id, Some(patch));
        }
    }
}

/// App names to pick from in the editor.
pub fn names() -> Vec<String> {
    STATE.lock().names.iter().cloned().collect()
}

fn matching(m: &Map<String, Value>) -> Result<Vec<u32>, String> {
    let app = m.get("app").and_then(Value::as_str).unwrap_or_default();
    if app.is_empty() {
        return Err("Pick an app first".into());
    }
    let ids: Vec<u32> = STATE.lock().streams.iter().filter(|s| s.is(app)).map(|s| s.index).collect();
    if ids.is_empty() {
        return Err(format!("{app} isn't playing any sound"));
    }
    Ok(ids)
}

pub async fn mute(m: &Map<String, Value>) -> Result<(), String> {
    let ids = matching(m)?;
    let mode = m.get("mode").and_then(Value::as_str).unwrap_or("toggle");
    // Toggle all of the app's streams together, from whether they're all muted now.
    let all_muted = {
        let st = STATE.lock();
        st.streams.iter().filter(|s| ids.contains(&s.index)).all(|s| s.muted)
    };
    let on = match mode {
        "on" => true,
        "off" => false,
        _ => !all_muted,
    };
    for id in ids {
        pactl(&["set-sink-input-mute", &id.to_string(), if on { "1" } else { "0" }]).await?;
    }
    Ok(())
}

/// Sets the app's volume, 0 to 1 for 0 to 100%.
pub async fn set_volume(m: &Map<String, Value>, value: f64) -> Result<(), String> {
    let pct = format!("{}%", (value.clamp(0.0, 1.0) * 100.0).round());
    for id in matching(m)? {
        pactl(&["set-sink-input-volume", &id.to_string(), &pct]).await?;
    }
    Ok(())
}

pub fn spawn(app: Arc<App>) {
    tokio::spawn(async move {
        STATE.lock().available = pactl(&["info"]).await.is_ok();
        if !STATE.lock().available {
            tracing::info!("per-app audio is off: pactl not found or no sound server");
            show(&app);
            return;
        }
        refresh(&app).await;
        let mut layout = app.engine.lock().subscribe();
        loop {
            let child = Command::new("pactl").arg("subscribe").stdin(std::process::Stdio::null()).stdout(std::process::Stdio::piped()).kill_on_drop(true).spawn();
            let Ok(mut child) = child else {
                tokio::time::sleep(Duration::from_secs(10)).await;
                continue;
            };
            let Some(out) = child.stdout.take() else { continue };
            let mut lines = tokio::io::BufReader::new(out).lines();
            loop {
                tokio::select! {
                    line = lines.next_line() => match line {
                        Ok(Some(l)) if l.contains("sink-input") => {
                            // Changes come in bursts. Read them all, then look once.
                            tokio::time::sleep(Duration::from_millis(80)).await;
                            refresh(&app).await;
                        }
                        Ok(Some(_)) => {}
                        _ => break,
                    },
                    r = layout.recv() => if matches!(r, Ok(Change::Layout)) { show(&app) },
                }
            }
            // The sound server restarted. Try again shortly.
            let _ = child.kill().await;
            tokio::time::sleep(Duration::from_secs(3)).await;
            refresh(&app).await;
        }
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_pactl_json() {
        let json = r#"[
            {"index": 42, "mute": false, "volume": {"front-left": {"value": 32768}, "front-right": {"value": 39322}},
             "properties": {"application.name": "Firefox", "application.process.binary": "firefox"}},
            {"index": 43, "mute": true, "volume": {"mono": {"value": 65536}},
             "properties": {"application.process.binary": "spotify"}},
            {"index": 44, "mute": false, "volume": {}, "properties": {}}
        ]"#;
        let s = super::parse(json);
        assert_eq!(s.len(), 2);
        assert_eq!((s[0].index, s[0].name.as_str(), s[0].muted), (42, "Firefox", false));
        assert!((s[0].volume - 0.6).abs() < 0.001, "loudest channel");
        assert_eq!(s[1].name, "spotify");
        assert!(s[1].is("Spotify") && s[0].is("firefox") && !s[0].is(""));
        assert!(super::parse("not json").is_empty());
    }
}

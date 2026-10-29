//! Per-app volume and mute: `pactl` on Linux (PipeWire and PulseAudio both answer it), Core Audio
//! sessions on Windows. Keys follow changes made in other mixers too.

use std::collections::BTreeSet;
use std::sync::Arc;

use parking_lot::Mutex;
use serde_json::{Map, Value};

use crate::app::App;
use crate::model::Tone;
use crate::state::Change;
use crate::state::view::ViewPatch;

pub const MUTE: &str = "core.app-mute";
pub const VOLUME: &str = "core.app-volume";

#[cfg(target_os = "linux")]
#[path = "appaudio/pactl.rs"]
mod backend;
#[cfg(all(windows, feature = "appaudio"))]
#[path = "appaudio/wasapi.rs"]
mod backend;

#[derive(Debug, Clone, PartialEq)]
pub struct Stream {
    /// The sink input on Linux, the process on Windows.
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
/// Asks for a fresh look straight after a key changed something.
static POKE: tokio::sync::Notify = tokio::sync::Notify::const_new();

async fn refresh(app: &App) {
    let streams = backend::list().await;
    {
        let mut st = STATE.lock();
        if st.streams == streams {
            return;
        }
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
                ViewPatch { disabled: Some(true), sub: Some(backend::UNAVAILABLE.into()), ..Default::default() }
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
    backend::set_mute(&ids, on).await?;
    POKE.notify_one();
    Ok(())
}

/// Sets the app's volume, 0 to 1 for 0 to 100%.
pub async fn set_volume(m: &Map<String, Value>, value: f64) -> Result<(), String> {
    backend::set_volume(&matching(m)?, value.clamp(0.0, 1.0)).await?;
    POKE.notify_one();
    Ok(())
}

pub fn spawn(app: Arc<App>) {
    tokio::spawn(async move {
        STATE.lock().available = backend::available().await;
        if !STATE.lock().available {
            tracing::info!("per-app audio is off: {}", backend::UNAVAILABLE);
            show(&app);
            return;
        }
        refresh(&app).await;
        show(&app);
        let mut layout = app.engine.lock().subscribe();
        let mut watcher = backend::Watcher::new();
        loop {
            tokio::select! {
                _ = watcher.changed() => refresh(&app).await,
                _ = POKE.notified() => refresh(&app).await,
                r = layout.recv() => match r {
                    Ok(Change::Layout) => show(&app),
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                    _ => {}
                },
            }
        }
    });
}

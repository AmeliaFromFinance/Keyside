//! Linux through `pactl`. `pactl subscribe` reports changes as they happen.

use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, BufReader, Lines};
use tokio::process::{Child, ChildStdout, Command};

use super::Stream;

pub const UNAVAILABLE: &str = "Needs pactl";

async fn pactl(args: &[&str]) -> Result<String, String> {
    let out = Command::new("pactl").args(args).stdin(std::process::Stdio::null()).kill_on_drop(true).output().await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound { "pactl isn't installed (it comes with pipewire-pulse or pulseaudio-utils)".to_string() } else { e.to_string() }
    })?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

pub async fn available() -> bool {
    pactl(&["info"]).await.is_ok()
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

pub async fn list() -> Vec<Stream> {
    pactl(&["-f", "json", "list", "sink-inputs"]).await.map(|j| parse(&j)).unwrap_or_default()
}

pub async fn set_mute(ids: &[u32], on: bool) -> Result<(), String> {
    for id in ids {
        pactl(&["set-sink-input-mute", &id.to_string(), if on { "1" } else { "0" }]).await?;
    }
    Ok(())
}

pub async fn set_volume(ids: &[u32], value: f64) -> Result<(), String> {
    let pct = format!("{}%", (value * 100.0).round());
    for id in ids {
        pactl(&["set-sink-input-volume", &id.to_string(), &pct]).await?;
    }
    Ok(())
}

pub struct Watcher {
    run: Option<(Child, Lines<BufReader<ChildStdout>>)>,
    /// A change was seen but not reported yet, kept if the wait is cancelled.
    pending: bool,
}

impl Watcher {
    pub fn new() -> Self {
        Self { run: None, pending: false }
    }

    /// Resolves after something changed. Safe to cancel.
    pub async fn changed(&mut self) {
        loop {
            if self.pending {
                // Changes come in bursts. Wait for them all, then look once.
                tokio::time::sleep(Duration::from_millis(80)).await;
                self.pending = false;
                return;
            }
            let Some((_, lines)) = &mut self.run else {
                let child = Command::new("pactl").arg("subscribe").stdin(std::process::Stdio::null()).stdout(std::process::Stdio::piped()).kill_on_drop(true).spawn();
                match child {
                    Ok(mut c) => match c.stdout.take() {
                        Some(out) => self.run = Some((c, BufReader::new(out).lines())),
                        None => tokio::time::sleep(Duration::from_secs(10)).await,
                    },
                    Err(_) => tokio::time::sleep(Duration::from_secs(10)).await,
                }
                continue;
            };
            match lines.next_line().await {
                Ok(Some(l)) if l.contains("sink-input") => self.pending = true,
                Ok(Some(_)) => {}
                _ => {
                    // The sound server restarted. Try again shortly, then look again.
                    self.run = None;
                    self.pending = true;
                    tokio::time::sleep(Duration::from_secs(3)).await;
                }
            }
        }
    }
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

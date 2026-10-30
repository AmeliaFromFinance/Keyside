//! Countdowns and stopwatches. Each timer publishes `timer.<name>` (mm:ss) and
//! `timer.<name>.running`, so any key can show it.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde_json::{Map, Value};

use crate::app::App;
use crate::model::Tone;

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Countdown,
    Stopwatch,
}

struct Timer {
    mode: Mode,
    total: Duration,
    elapsed: Duration,
    started: Option<Instant>,
}

impl Timer {
    fn elapsed(&self) -> Duration {
        self.elapsed + self.started.map(|s| s.elapsed()).unwrap_or_default()
    }

    fn shown(&self) -> Duration {
        match self.mode {
            Mode::Stopwatch => self.elapsed(),
            Mode::Countdown => self.total.saturating_sub(self.elapsed()),
        }
    }
}

#[derive(Default)]
pub struct Timers {
    map: Mutex<HashMap<String, Timer>>,
}

pub fn valid_name(n: &str) -> bool {
    !n.is_empty() && n.len() <= 24 && n.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

fn text(d: Duration) -> String {
    let s = d.as_secs();
    if s >= 3600 { format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60) } else { format!("{:02}:{:02}", s / 60, s % 60) }
}

impl Timers {
    pub fn apply(&self, app: &App, m: &Map<String, Value>) -> Result<(), String> {
        let name = m.get("name").and_then(Value::as_str).filter(|n| valid_name(n)).ok_or("timer name must be short lowercase letters, digits or -")?;
        let mode = match m.get("mode").and_then(Value::as_str).unwrap_or("countdown") {
            "countdown" => Mode::Countdown,
            "stopwatch" => Mode::Stopwatch,
            _ => return Err("unknown timer mode".into()),
        };
        let secs = m.get("seconds").and_then(Value::as_f64).unwrap_or(300.0).clamp(1.0, 86_400.0);
        let op = m.get("op").and_then(Value::as_str).unwrap_or("toggle");
        let reset_on_stop = m.get("resetOnStop").and_then(Value::as_bool).unwrap_or(false);
        {
            let mut map = self.map.lock();
            if map.len() >= 32 && !map.contains_key(name) {
                return Err("too many timers".into());
            }
            let t = map.entry(name.to_string()).or_insert(Timer { mode, total: Duration::from_secs_f64(secs), elapsed: Duration::ZERO, started: None });
            t.mode = mode;
            t.total = Duration::from_secs_f64(secs);
            let start = |t: &mut Timer| {
                if t.mode == Mode::Countdown && t.shown().is_zero() {
                    t.elapsed = Duration::ZERO;
                }
                t.started.get_or_insert_with(Instant::now);
            };
            let stop = |t: &mut Timer| {
                if let Some(s) = t.started.take() {
                    t.elapsed += s.elapsed();
                }
                if reset_on_stop {
                    t.elapsed = Duration::ZERO;
                }
            };
            match op {
                "start" => start(t),
                "stop" => stop(t),
                "toggle" if t.started.is_some() => stop(t),
                "toggle" => start(t),
                "reset" => {
                    t.elapsed = Duration::ZERO;
                    t.started = None;
                }
                _ => return Err("unknown timer operation".into()),
            }
        }
        self.publish(app);
        Ok(())
    }

    fn publish(&self, app: &App) {
        let mut done = Vec::new();
        let vals: Vec<(String, String, bool)> = {
            let mut map = self.map.lock();
            for (name, t) in map.iter_mut() {
                if t.mode == Mode::Countdown && t.started.is_some() && t.shown().is_zero() {
                    t.started = None;
                    t.elapsed = t.total;
                    done.push(name.clone());
                }
            }
            map.iter().map(|(n, t)| (n.clone(), text(t.shown()), t.started.is_some())).collect()
        };
        let mut e = app.engine.lock();
        for (n, txt, running) in vals {
            e.set_var(&format!("timer.{n}"), Value::String(txt));
            e.set_var(&format!("timer.{n}.running"), Value::Bool(running));
        }
        for n in done {
            e.toast(format!("Timer \"{n}\" finished"), Tone::Accent);
        }
    }

    fn any_running(&self) -> bool {
        self.map.lock().values().any(|t| t.started.is_some())
    }

    pub fn spawn_ticker(app: Arc<App>) {
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_millis(500));
            loop {
                tick.tick().await;
                if app.timers.any_running() {
                    app.timers.publish(&app);
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn formats() {
        assert_eq!(super::text(std::time::Duration::from_secs(65)), "01:05");
        assert_eq!(super::text(std::time::Duration::from_secs(3725)), "1:02:05");
        assert!(super::valid_name("break-1"));
        assert!(!super::valid_name("Break"));
    }

    #[test]
    fn can_reset_when_stopped() {
        use serde_json::json;
        let dir = tempfile::tempdir().unwrap();
        let app = crate::app::App::new(crate::app::Options { data_dir: Some(dir.path().into()), bundled_plugins: None }).unwrap();
        let t = super::Timers::default();
        let args = |op: &str, reset: bool| json!({ "name": "x", "mode": "stopwatch", "op": op, "resetOnStop": reset }).as_object().unwrap().clone();
        let elapsed = || t.map.lock()["x"].elapsed;
        t.apply(&app, &args("start", false)).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        t.apply(&app, &args("toggle", false)).unwrap();
        assert!(!elapsed().is_zero(), "keeps its time by default");
        t.apply(&app, &args("start", true)).unwrap();
        t.apply(&app, &args("stop", true)).unwrap();
        assert!(elapsed().is_zero());
    }
}

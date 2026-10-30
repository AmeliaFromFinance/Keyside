//! Plugin health for the Plugins panel: how long a plugin has run, how often it restarted or
//! crashed in the last hour, its last error and its recent status messages. Covers Keyside
//! plugins and Stream Deck plugins ("sd.<uuid>") alike.

use std::collections::{HashMap, VecDeque};

use serde::Serialize;

const WINDOW_MS: u64 = 3_600_000;
const RECENT: usize = 20;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Event {
    pub at: u64,
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LastError {
    pub at: u64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    /// When the current process started (ms since 1970). None while it isn't running.
    pub running_since: Option<u64>,
    pub restarts: usize,
    pub crashes: usize,
    pub last_error: Option<LastError>,
    /// Newest first.
    pub recent: Vec<Event>,
}

#[derive(Default)]
struct Entry {
    running_since: Option<u64>,
    /// Set once the plugin has run before, so the next start counts as a restart.
    ran: bool,
    restarts: VecDeque<u64>,
    crashes: VecDeque<u64>,
    last_error: Option<LastError>,
    recent: VecDeque<Event>,
}

impl Entry {
    fn event(&mut self, at: u64, state: &str, detail: Option<&str>) {
        let e = Event { at, state: state.into(), detail: detail.map(str::to_string) };
        if self.recent.back().is_some_and(|l| l.state == e.state && l.detail == e.detail) {
            return;
        }
        self.recent.push_back(e);
        while self.recent.len() > RECENT {
            self.recent.pop_front();
        }
    }
}

fn trim(q: &mut VecDeque<u64>, now: u64) {
    while q.front().is_some_and(|t| now.saturating_sub(*t) > WINDOW_MS) {
        q.pop_front();
    }
}

#[derive(Default)]
pub struct Tracker {
    map: HashMap<String, Entry>,
}

impl Tracker {
    pub fn started(&mut self, id: &str, now: u64) {
        let e = self.map.entry(id.to_string()).or_default();
        if e.ran {
            e.restarts.push_back(now);
            trim(&mut e.restarts, now);
        }
        e.ran = true;
        e.running_since = Some(now);
        e.event(now, "started", None);
    }

    /// The process ended. `crash` is set when it ended on its own or failed, not when Keyside stopped it.
    pub fn exited(&mut self, id: &str, now: u64, crash: bool, reason: Option<&str>) {
        let e = self.map.entry(id.to_string()).or_default();
        if e.running_since.take().is_none() && !crash {
            return;
        }
        if crash {
            e.crashes.push_back(now);
            trim(&mut e.crashes, now);
            e.last_error = Some(LastError { at: now, text: reason.unwrap_or("stopped unexpectedly").chars().take(300).collect() });
            e.event(now, "crashed", reason);
        } else {
            e.event(now, "stopped", None);
        }
    }

    /// A status change, as shown in the deck header.
    pub fn status(&mut self, id: &str, now: u64, state: &str, detail: Option<&str>) {
        let e = self.map.entry(id.to_string()).or_default();
        if state == "error" {
            e.last_error = Some(LastError { at: now, text: detail.unwrap_or("error").chars().take(300).collect() });
        }
        e.event(now, state, detail);
    }

    pub fn report(&mut self, id: &str, now: u64) -> Report {
        let e = self.map.entry(id.to_string()).or_default();
        trim(&mut e.restarts, now);
        trim(&mut e.crashes, now);
        Report {
            running_since: e.running_since,
            restarts: e.restarts.len(),
            crashes: e.crashes.len(),
            last_error: e.last_error.clone(),
            recent: e.recent.iter().rev().cloned().collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_restarts_and_crashes_in_the_last_hour() {
        let mut t = Tracker::default();
        t.started("obs", 1_000);
        assert_eq!(t.report("obs", 1_000).restarts, 0, "the first start isn't a restart");
        t.exited("obs", 2_000, true, Some("exited with 1"));
        t.started("obs", 3_000);
        t.exited("obs", 4_000, false, None);
        t.started("obs", 5_000);
        let r = t.report("obs", 6_000);
        assert_eq!((r.restarts, r.crashes, r.running_since), (2, 1, Some(5_000)));
        assert_eq!(r.last_error.unwrap(), LastError { at: 2_000, text: "exited with 1".into() });
        assert_eq!(r.recent[0].state, "started", "newest first");
        let later = t.report("obs", 3_000 + WINDOW_MS + 1);
        assert_eq!((later.restarts, later.crashes), (1, 0));
        assert!(later.last_error.is_some(), "the last error stays");
    }

    #[test]
    fn keeps_the_last_twenty_status_messages_without_repeats() {
        let mut t = Tracker::default();
        t.status("x", 1, "connecting", None);
        t.status("x", 2, "connecting", None);
        for i in 0..30 {
            t.status("x", 10 + i, "error", Some(&format!("try {i}")));
        }
        let r = t.report("x", 100);
        assert_eq!(r.recent.len(), RECENT);
        assert_eq!(r.recent[0].detail.as_deref(), Some("try 29"));
        assert_eq!(r.last_error.unwrap().text, "try 29");
    }
}

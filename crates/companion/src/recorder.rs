//! Action recorder: while it runs, every key press is added to one new multi-step key,
//! with the pauses between presses kept as wait steps.

use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde_json::{Map, json};

use crate::model::Action;

#[derive(Default)]
pub struct Recorder {
    state: Mutex<Option<(Vec<Action>, Option<Instant>)>>,
}

const MAX_STEPS: usize = crate::model::MAX_ACTIONS;

impl Recorder {
    pub fn start(&self) {
        *self.state.lock() = Some((Vec::new(), None));
    }

    pub fn active(&self) -> bool {
        self.state.lock().is_some()
    }

    pub fn record(&self, actions: &[Action]) {
        let mut st = self.state.lock();
        let Some((steps, last)) = st.as_mut() else { return };
        if let Some(prev) = last {
            let gap = prev.elapsed().min(Duration::from_secs(60));
            if gap >= Duration::from_millis(150) && steps.len() < MAX_STEPS {
                let mut s = Map::new();
                s.insert("ms".into(), json!(gap.as_millis() as u64 / 50 * 50));
                steps.push(Action { kind: "core.wait".into(), settings: s, review: false });
            }
        }
        for a in actions.iter().filter(|a| !a.review) {
            if steps.len() >= MAX_STEPS {
                break;
            }
            steps.push(a.clone());
        }
        *last = Some(Instant::now());
    }

    pub fn stop(&self) -> Vec<Action> {
        self.state.lock().take().map(|(s, _)| s).unwrap_or_default()
    }
}

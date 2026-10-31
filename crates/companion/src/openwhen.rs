//! Opens pages on phones when a variable changes to the value a page asks for, like the OBS
//! scene switching to "BRB". Only changes count, so people can still move to other pages.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;
use tokio::sync::broadcast::error::RecvError;

use crate::actions::Nav;
use crate::app::App;
use crate::model::Profile;
use crate::state::Change;

pub fn spawn(app: Arc<App>) {
    tokio::spawn(async move {
        let mut rx = app.engine.lock().subscribe();
        let mut seen: HashMap<String, String> = HashMap::new();
        loop {
            match rx.recv().await {
                Ok(Change::Vars | Change::Layout | Change::Profile) | Err(RecvError::Lagged(_)) => {}
                Ok(_) => continue,
                Err(RecvError::Closed) => break,
            }
            let open = {
                let e = app.engine.lock();
                e.profile(e.active_profile()).and_then(|p| page_to_open(p, &mut seen, |v| e.var(v)))
            };
            if let Some(page) = open {
                app.navigate("*", Nav::Page(page));
            }
        }
    });
}

/// Records the current values, and returns the page whose value one of them just changed to.
fn page_to_open(profile: &Profile, seen: &mut HashMap<String, String>, get: impl Fn(&str) -> Option<Value>) -> Option<String> {
    let rules: Vec<_> = profile.pages.iter().filter_map(|pg| pg.open_when.as_ref().map(|w| (pg, w))).collect();
    let mut changed: HashMap<&str, String> = HashMap::new();
    for (_, w) in &rules {
        let now = match get(&w.var) {
            Some(Value::String(s)) => s,
            Some(Value::Null) | None => String::new(),
            Some(v) => v.to_string(),
        };
        // First sight of a variable only records it, so starting Keyside doesn't jump pages.
        if seen.get(&w.var).is_some_and(|before| *before != now) {
            changed.insert(&w.var, now.clone());
        }
        seen.insert(w.var.clone(), now);
    }
    rules.iter().find(|(_, w)| changed.get(w.var.as_str()) == Some(&w.value)).map(|(pg, _)| pg.id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn profile() -> Profile {
        serde_json::from_value(json!({ "id": "main", "name": "Main", "homePage": "p1", "pages": [
            { "id": "p1", "name": "Main", "cols": 3, "rows": 1, "keys": [] },
            { "id": "brb", "name": "BRB", "cols": 3, "rows": 1, "keys": [], "openWhen": { "var": "obs.scene", "value": "BRB" } },
            { "id": "game", "name": "Game", "cols": 3, "rows": 1, "keys": [], "openWhen": { "var": "obs.scene", "value": "Gameplay" } }
        ]}))
        .unwrap()
    }

    #[test]
    fn opens_a_page_when_the_scene_changes_to_it() {
        let p = profile();
        let mut seen = HashMap::new();
        let scene = |s: &'static str| move |_: &str| Some(json!(s));
        assert_eq!(page_to_open(&p, &mut seen, scene("BRB")), None, "already on BRB when Keyside starts");
        assert_eq!(page_to_open(&p, &mut seen, scene("Gameplay")).as_deref(), Some("game"));
        assert_eq!(page_to_open(&p, &mut seen, scene("Gameplay")), None, "no change, no jump");
        assert_eq!(page_to_open(&p, &mut seen, scene("Chatting")), None);
        assert_eq!(page_to_open(&p, &mut seen, scene("BRB")).as_deref(), Some("brb"));
    }
}

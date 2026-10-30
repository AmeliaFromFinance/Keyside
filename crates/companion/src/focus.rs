//! Switches profiles to match the focused app, and back to the default profile after.
//! Physical Stream Decks pick among their own layouts the same way, see `deck_layout`.

use std::sync::Arc;

use crate::app::App;
use crate::model::LayoutFile;

/// Whether a program name from an app list means the focused process (case-insensitive,
/// with or without .exe or .app).
pub fn same_app(listed: &str, process: &str) -> bool {
    let norm = |s: &str| s.trim().trim_end_matches(".exe").trim_end_matches(".app").to_ascii_lowercase();
    let want = norm(process);
    !want.is_empty() && norm(listed) == want
}

/// The layout a physical deck shows: one of its layouts that lists the focused app, otherwise
/// its default one (the first without apps, or the first of all). None when it has none.
pub fn deck_layout(layout: &LayoutFile, serial: &str, focused: &str) -> Option<String> {
    let mine: Vec<_> = layout.profiles.iter().filter(|p| p.deck.as_ref().is_some_and(|d| d.serial == serial)).collect();
    mine.iter().find(|p| p.apps.iter().any(|a| same_app(a, focused))).or_else(|| mine.iter().find(|p| p.apps.is_empty())).or(mine.first()).map(|p| p.id.clone())
}

pub fn spawn(app: Arc<App>) {
    #[cfg(feature = "focus")]
    tokio::spawn(async move {
        let mut by_focus = false;
        let mut last = String::new();
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            if !app.settings.read().follow_focus {
                continue;
            }
            let name = tokio::task::spawn_blocking(|| {
                active_win_pos_rs::get_active_window().ok().and_then(|w| w.process_path.file_name().map(|n| n.to_string_lossy().to_string()).or(Some(w.app_name)))
            })
            .await
            .ok()
            .flatten()
            .unwrap_or_default();
            if name == last {
                continue;
            }
            last = name.clone();
            app.engine.lock().set_var("core.app", serde_json::Value::String(name.clone()));
            let target = app.engine.lock().profile_for_app(&name);
            let mut e = app.engine.lock();
            match target {
                Some(p) => {
                    e.set_active_profile(&p);
                    by_focus = true;
                }
                None if by_focus => {
                    let d = e.layout().default_profile.clone();
                    e.set_active_profile(&d);
                    by_focus = false;
                }
                None => {}
            }
        }
    });
    #[cfg(not(feature = "focus"))]
    let _ = app;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout() -> LayoutFile {
        serde_json::from_value(serde_json::json!({
            "version": 1, "defaultProfile": "main",
            "profiles": [
                { "id": "main", "name": "Main", "homePage": "p", "apps": ["obs64.exe"], "pages": [{ "id": "p", "name": "P", "cols": 3, "rows": 2, "keys": [] }] },
                { "id": "game", "name": "Game", "homePage": "p", "apps": ["Game.exe", "other"], "deck": { "serial": "S1", "model": "Stream Deck +", "cols": 4, "rows": 2 }, "pages": [{ "id": "p", "name": "P", "cols": 4, "rows": 3, "keys": [] }] },
                { "id": "base", "name": "Base", "homePage": "p", "deck": { "serial": "S1", "model": "Stream Deck +", "cols": 4, "rows": 2 }, "pages": [{ "id": "p", "name": "P", "cols": 4, "rows": 3, "keys": [] }] },
                { "id": "mini", "name": "Mini", "homePage": "p", "deck": { "serial": "S2", "model": "Stream Deck Mini", "cols": 3, "rows": 2 }, "pages": [{ "id": "p", "name": "P", "cols": 3, "rows": 2, "keys": [] }] }
            ]
        }))
        .unwrap()
    }

    #[test]
    fn decks_pick_their_layout_by_focused_app() {
        let l = layout();
        assert_eq!(deck_layout(&l, "S1", "game"), Some("game".into()), "case and .exe don't matter");
        assert_eq!(deck_layout(&l, "S1", "obs64.exe"), Some("base".into()), "apps of normal layouts don't count");
        assert_eq!(deck_layout(&l, "S1", ""), Some("base".into()), "the one without apps is the default");
        assert_eq!(deck_layout(&l, "S2", "Game.exe"), Some("mini".into()), "a single layout always shows");
        assert_eq!(deck_layout(&l, "S3", "Game.exe"), None);

        // Without a layout free of apps, the first one is the default.
        let mut l = layout();
        l.profiles.retain(|p| p.id != "base");
        assert_eq!(deck_layout(&l, "S1", "notepad.exe"), Some("game".into()));
    }

    #[test]
    fn normal_switching_skips_deck_layouts() {
        let dir = tempfile::tempdir().unwrap();
        let app = crate::app::App::new(crate::app::Options { data_dir: Some(dir.path().into()), bundled_plugins: None }).unwrap();
        app.engine.lock().replace_layout(layout()).unwrap();
        let e = app.engine.lock();
        assert_eq!(e.profile_for_app("OBS64"), Some("main".into()));
        assert_eq!(e.profile_for_app("game.exe"), None);
    }
}

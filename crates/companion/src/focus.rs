//! Switches profiles to match the focused app, and back to the default profile after.

use std::sync::Arc;

use crate::app::App;

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

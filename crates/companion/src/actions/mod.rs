//! Runs the actions behind a key press.

pub mod core;
pub mod input;
#[cfg(feature = "midi")]
pub mod midi;
#[cfg(feature = "sound")]
pub mod sound;
pub mod timers;

use std::sync::Arc;

use crate::app::App;
use crate::model::{Action, Tone};
use crate::plugins::Step;
use crate::state::instance_id;

/// Which key was pressed, and by whom.
#[derive(Debug, Clone)]
pub struct Ctx {
    pub device: String,
    pub profile: String,
    pub page: String,
    pub key: String,
    /// True when the press came from the local editor or hardware deck.
    pub local: bool,
}

/// Effects a step can have on the device that pressed the key.
#[derive(Debug, Clone)]
pub enum Nav {
    Page(String),
    Profile(String),
}

/// Built-ins that can start programs. Only the desktop editor may add them.
pub const ADMIN_ONLY: &[&str] = &["core.launch"];

/// Built-ins that stay enabled when a layout is imported. Everything else waits for review.
pub const SAFE_ON_IMPORT: &[&str] = &["core.wait", "core.page", "core.profile", "core.timer", "core.var"];

pub fn is_admin_only(a: &Action) -> bool {
    ADMIN_ONLY.contains(&a.kind.as_str())
}

/// Runs a list of steps in order. Back-to-back steps for the same plugin go out together,
/// so plugins like OBS can batch them.
pub async fn run_steps(app: Arc<App>, ctx: Ctx, actions: Vec<Action>, off: bool) {
    let mut i = 0;
    while i < actions.len() {
        let a = &actions[i];
        if a.review {
            app.engine.lock().toast("This key has imported steps waiting for review on the PC", Tone::Warning);
            return;
        }
        match a.plugin() {
            Some((plugin, _)) => {
                let plugin = plugin.to_string();
                let mut steps = Vec::new();
                while i < actions.len() && actions[i].plugin().is_some_and(|(p, _)| p == plugin) && !actions[i].review {
                    let act = &actions[i];
                    steps.push(Step {
                        instance: instance_id(&ctx.profile, &ctx.page, &ctx.key, off, i),
                        action: act.plugin().map(|(_, id)| id.to_string()).unwrap_or_default(),
                        settings: act.settings.clone(),
                    });
                    i += 1;
                }
                if let Err(e) = app.plugins.run(&plugin, steps, "press", &ctx.device).await {
                    app.engine.lock().toast(e, Tone::Danger);
                    return;
                }
            }
            None => {
                let step = a.clone();
                i += 1;
                match core::run(&app, &ctx, &step).await {
                    Ok(Some(nav)) => app.navigate(&ctx.device, nav),
                    Ok(None) => {}
                    Err(e) => {
                        tracing::info!(action = %step.kind, "step failed: {e}");
                        app.engine.lock().toast(e, Tone::Danger);
                        return;
                    }
                }
            }
        }
    }
}

/// Key released after it fired. Only plugins get release events (push-to-talk and similar).
pub async fn release(app: Arc<App>, ctx: Ctx, actions: Vec<Action>) {
    let mut by_plugin: Vec<(String, Vec<Step>)> = Vec::new();
    for (i, a) in actions.iter().enumerate() {
        let Some((plugin, id)) = a.plugin() else { continue };
        if a.review {
            continue;
        }
        let step = Step { instance: instance_id(&ctx.profile, &ctx.page, &ctx.key, false, i), action: id.to_string(), settings: a.settings.clone() };
        match by_plugin.iter_mut().find(|(p, _)| p == plugin) {
            Some((_, v)) => v.push(step),
            None => by_plugin.push((plugin.to_string(), vec![step])),
        }
    }
    for (plugin, steps) in by_plugin {
        let _ = app.plugins.run(&plugin, steps, "release", &ctx.device).await;
    }
}

//! Runs the actions behind a key press.

#[cfg(any(target_os = "linux", all(windows, feature = "appaudio")))]
pub mod appaudio;
pub mod core;
pub mod input;
#[cfg(feature = "midi")]
pub mod midi;
#[cfg(feature = "nowplaying")]
pub mod nowplaying;
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
pub const SAFE_ON_IMPORT: &[&str] = &["core.wait", "core.page", "core.profile", "core.timer", "core.var", IF, ELSE, END];

/// Steps that decide which other steps run.
pub const IF: &str = "core.if";
pub const ELSE: &str = "core.else";
pub const END: &str = "core.end";

fn is_flow(a: &Action) -> bool {
    matches!(a.kind.as_str(), IF | ELSE | END)
}

/// From just after an "if" (or "otherwise"), finds the step that closes it: its "otherwise"
/// when `to_else`, else its "end". A missing end means the end of the list.
fn closing(actions: &[Action], from: usize, to_else: bool) -> usize {
    let mut depth = 0;
    for (i, a) in actions.iter().enumerate().skip(from) {
        match a.kind.as_str() {
            IF => depth += 1,
            ELSE if depth == 0 && to_else => return i,
            END if depth == 0 => return i,
            END => depth -= 1,
            _ => {}
        }
    }
    actions.len()
}

/// Checks an "if" step's condition against the current variables.
fn holds(app: &App, a: &Action) -> bool {
    let Ok(c) = serde_json::from_value::<crate::model::Condition>(serde_json::Value::Object(a.settings.clone())) else { return false };
    let e = app.engine.lock();
    crate::state::view::eval(&c, |n| e.var(n))
}

pub fn is_admin_only(a: &Action) -> bool {
    ADMIN_ONLY.contains(&a.kind.as_str())
}

/// Runs a list of steps in order. Back-to-back steps for the same plugin go out together,
/// so plugins like OBS can batch them.
pub async fn run_steps(app: Arc<App>, ctx: Ctx, actions: Vec<Action>, off: bool) {
    let flow = actions.iter().any(is_flow);
    let mut ran = Vec::new();
    let mut i = 0;
    while i < actions.len() {
        let a = &actions[i];
        if a.review {
            app.engine.lock().toast("This key has imported steps waiting for review on the PC", Tone::Warning);
            return;
        }
        match a.kind.as_str() {
            IF => {
                // True: carry on into the branch. False: jump past its "otherwise" or "end".
                i = if holds(&app, a) { i + 1 } else { closing(&actions, i + 1, true) + 1 };
                continue;
            }
            // Reached from a branch that ran: skip the other one.
            ELSE => {
                i = closing(&actions, i + 1, false) + 1;
                continue;
            }
            END => {
                i += 1;
                continue;
            }
            _ => {}
        }
        if flow && !off {
            ran.push(i);
            app.ran.lock().insert(crate::state::key_path(&ctx.profile, &ctx.page, &ctx.key), ran.clone());
        }
        match a.plugin() {
            Some((plugin, _)) => {
                let plugin = plugin.to_string();
                let mut steps = Vec::new();
                while i < actions.len() && actions[i].plugin().is_some_and(|(p, _)| p == plugin) && !actions[i].review {
                    let act = &actions[i];
                    if flow && !off && ran.last() != Some(&i) {
                        ran.push(i);
                    }
                    steps.push(Step {
                        instance: instance_id(&ctx.profile, &ctx.page, &ctx.key, off, i),
                        action: act.plugin().map(|(_, id)| id.to_string()).unwrap_or_default(),
                        settings: act.settings.clone(),
                    });
                    i += 1;
                }
                if flow && !off {
                    app.ran.lock().insert(crate::state::key_path(&ctx.profile, &ctx.page, &ctx.key), ran.clone());
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

/// Runs a key's steps, then again every `repeat` ms for as long as the key is held.
pub async fn run_key(app: Arc<App>, ctx: Ctx, actions: Vec<Action>, off: bool, repeat: Option<u32>) {
    let path = crate::state::key_path(&ctx.profile, &ctx.page, &ctx.key);
    loop {
        run_steps(app.clone(), ctx.clone(), actions.clone(), off).await;
        let Some(ms) = repeat.filter(|ms| *ms >= 100) else { break };
        if !app.held.lock().contains(&path) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(ms as u64)).await;
        if !app.held.lock().contains(&path) {
            break;
        }
    }
}

/// Key released after it fired. Only plugins get release events (push-to-talk and similar),
/// and with if/otherwise steps, only the steps that ran.
pub async fn release(app: Arc<App>, ctx: Ctx, actions: Vec<Action>) {
    let ran = if actions.iter().any(is_flow) { Some(app.ran.lock().remove(&crate::state::key_path(&ctx.profile, &ctx.page, &ctx.key)).unwrap_or_default()) } else { None };
    let mut by_plugin: Vec<(String, Vec<Step>)> = Vec::new();
    for (i, a) in actions.iter().enumerate() {
        let Some((plugin, id)) = a.plugin() else { continue };
        if a.review || ran.as_ref().is_some_and(|r| !r.contains(&i)) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn step(kind: &str, settings: serde_json::Value) -> Action {
        serde_json::from_value(json!({ "type": kind, "settings": settings })).unwrap()
    }

    fn set(name: &str, value: &str) -> Action {
        step("core.var", json!({ "name": name, "op": "set", "value": value }))
    }

    #[tokio::test]
    async fn if_otherwise_and_nesting() {
        let dir = tempfile::tempdir().unwrap();
        let app = App::new(crate::app::Options { data_dir: Some(dir.path().into()), bundled_plugins: None }).unwrap();
        let ctx = Ctx { device: "d".into(), profile: "p".into(), page: "g".into(), key: "k".into(), local: true };
        let steps = vec![
            step(IF, json!({ "var": "user.live", "op": "truthy" })),
            set("user.said", "live"),
            step(IF, json!({ "var": "user.mode", "op": "eq", "value": "chill" })),
            set("user.inner", "chill"),
            step(ELSE, json!({})),
            set("user.inner", "other"),
            step(END, json!({})),
            step(ELSE, json!({})),
            set("user.said", "offline"),
            step(END, json!({})),
            set("user.after", "yes"),
        ];
        let get = |n: &str| app.engine.lock().var(n).and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default();

        run_steps(app.clone(), ctx.clone(), steps.clone(), false).await;
        assert_eq!((get("user.said"), get("user.inner"), get("user.after")), ("offline".into(), "".into(), "yes".into()));

        app.engine.lock().set_var("user.live", json!(true));
        app.engine.lock().set_var("user.mode", json!("chill"));
        run_steps(app.clone(), ctx.clone(), steps.clone(), false).await;
        assert_eq!((get("user.said"), get("user.inner")), ("live".into(), "chill".into()));

        app.engine.lock().set_var("user.mode", json!("hype"));
        run_steps(app.clone(), ctx, steps, false).await;
        assert_eq!(get("user.inner"), "other");
        // Only the steps that ran are remembered for the release.
        assert_eq!(app.ran.lock().get("p/g/k").cloned(), Some(vec![1, 5, 10]));
    }

    #[test]
    fn a_missing_end_closes_at_the_end() {
        let steps = vec![step(IF, json!({ "var": "x", "op": "truthy" })), set("user.a", "1")];
        assert_eq!(closing(&steps, 1, true), 2);
    }
}

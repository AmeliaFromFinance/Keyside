//! Requests from the editor and paired devices. Each op checks who is asking first.

use std::sync::Arc;

use base64::Engine as _;
use serde_json::{Map, Value, json};

use super::catalog;
use super::conn::Who;
use crate::actions::{self, Ctx};
use crate::app::App;
use crate::devices::{Scope, valid_pin};
use crate::model::{self, Action, LayoutFile, Tone};
use crate::plugins::{Source, manifest};
use crate::state::{Change, all_keys};
use crate::tokens::ApiScope;

pub struct OpError {
    pub code: &'static str,
    pub message: String,
}

impl OpError {
    pub fn forbidden() -> Self {
        Self { code: "forbidden", message: "This device is not allowed to do that".into() }
    }
    pub fn invalid(m: impl Into<String>) -> Self {
        Self { code: "invalid", message: m.into() }
    }
    fn not_found(m: impl Into<String>) -> Self {
        Self { code: "not_found", message: m.into() }
    }
    fn conflict(m: impl Into<String>) -> Self {
        Self { code: "conflict", message: m.into() }
    }
    fn failed(m: impl std::fmt::Display) -> Self {
        Self { code: "failed", message: m.to_string() }
    }
}

type R = Result<Value, OpError>;

fn s<'a>(a: &'a Value, k: &str) -> Result<&'a str, OpError> {
    a.get(k).and_then(Value::as_str).ok_or_else(|| OpError::invalid(format!("missing {k}")))
}

fn need_edit(who: &Who) -> Result<(), OpError> {
    if who.can_edit() { Ok(()) } else { Err(OpError::forbidden()) }
}

fn need_admin(who: &Who) -> Result<(), OpError> {
    if who.admin { Ok(()) } else { Err(OpError::forbidden()) }
}

pub async fn handle(app: &Arc<App>, who: &Who, op: &str, a: Value) -> R {
    match op {
        "asset.image" => {
            let id = s(&a, "id")?;
            Ok(app.assets.data_url(id).map(Value::String).unwrap_or(Value::Null))
        }

        "layout.get" => {
            need_edit(who)?;
            Ok(serde_json::to_value(app.engine.lock().layout()).unwrap_or(Value::Null))
        }
        "layout.save" => {
            need_edit(who)?;
            save_layout(app, who, &a).await
        }
        "catalog.get" => {
            need_edit(who)?;
            let core: Vec<Value> = catalog::core_actions().as_array().cloned().unwrap_or_default().into_iter().filter(|c| who.admin || c.get("adminOnly").is_none()).collect();
            #[cfg(feature = "midi")]
            let midi = crate::actions::midi::ports();
            #[cfg(not(feature = "midi"))]
            let midi: Vec<String> = Vec::new();
            Ok(json!({
                "core": core,
                "plugins": app.plugins.catalog().into_iter().filter_map(|p| serde_json::to_value(p).ok()).chain(app.streamdeck.catalog()).collect::<Vec<_>>(),
                "assets": app.assets.list(),
                "midi": midi,
                "variables": all_variables(app),
            }))
        }
        "options.get" => {
            need_edit(who)?;
            let kind = s(&a, "type")?;
            let field = s(&a, "field")?;
            let settings = a.get("settings").and_then(Value::as_object).cloned().unwrap_or_default();
            #[cfg(target_os = "linux")]
            if field == "app" && (kind == crate::actions::appaudio::MUTE || kind == crate::actions::appaudio::VOLUME) {
                return Ok(json!(crate::actions::appaudio::names().into_iter().map(|n| json!({ "value": n, "label": n })).collect::<Vec<_>>()));
            }
            let (plugin, action) = kind.split_once('/').ok_or_else(|| OpError::invalid("not a plugin action"))?;
            app.plugins.options(plugin, action, field, &settings).await.map_err(OpError::failed)
        }
        "steps.test" => {
            need_edit(who)?;
            let list: Vec<Action> = serde_json::from_value(a.get("actions").cloned().unwrap_or(Value::Null)).map_err(|_| OpError::invalid("invalid steps"))?;
            if list.len() > 100 || list.iter().any(|x| !model::valid_action_type(&x.kind) || (actions::is_admin_only(x) && !who.admin)) {
                return Err(OpError::forbidden());
            }
            let ctx = Ctx { device: who.device.clone(), profile: "test".into(), page: "test".into(), key: "test".into(), local: who.admin };
            tokio::spawn(actions::run_steps(app.clone(), ctx, list.into_iter().map(|x| Action { review: false, ..x }).collect(), false));
            Ok(Value::Null)
        }
        "action.test" => {
            need_edit(who)?;
            let action: Action = serde_json::from_value(a.get("action").cloned().unwrap_or(Value::Null)).map_err(|_| OpError::invalid("invalid action"))?;
            if !model::valid_action_type(&action.kind) || (actions::is_admin_only(&action) && !who.admin) {
                return Err(OpError::forbidden());
            }
            let ctx = Ctx { device: who.device.clone(), profile: "test".into(), page: "test".into(), key: "test".into(), local: who.admin };
            tokio::spawn(actions::run_steps(app.clone(), ctx, vec![Action { review: false, ..action }], false));
            Ok(Value::Null)
        }
        "assets.put" => {
            need_edit(who)?;
            let data = base64::engine::general_purpose::STANDARD.decode(s(&a, "data")?).map_err(|_| OpError::invalid("data must be base64"))?;
            let meta = app.assets.put(a.get("name").and_then(Value::as_str).unwrap_or("file"), &data).map_err(OpError::failed)?;
            Ok(serde_json::to_value(meta).unwrap_or(Value::Null))
        }
        "assets.delete" => {
            need_admin(who)?;
            Ok(json!(app.assets.delete(s(&a, "id")?).map_err(OpError::failed)?))
        }
        "variables.list" => {
            need_edit(who)?;
            Ok(all_variables(app))
        }
        "share.export" => {
            need_edit(who)?;
            let profile = app.engine.lock().profile(s(&a, "profile")?).cloned().ok_or_else(|| OpError::not_found("profile not found"))?;
            let pages: Option<Vec<String>> = a.get("pages").and_then(|p| serde_json::from_value(p.clone()).ok());
            Ok(json!({ "code": crate::share::export(&profile, pages.as_deref()).map_err(OpError::failed)? }))
        }
        "share.import" => {
            need_edit(who)?;
            Ok(serde_json::to_value(crate::share::import(s(&a, "code")?).map_err(OpError::failed)?).unwrap_or(Value::Null))
        }
        "share.importJson" => {
            need_edit(who)?;
            let raw = a.get("profile").cloned().unwrap_or(Value::Null);
            if raw.to_string().len() > 4 * 1024 * 1024 {
                return Err(OpError::invalid("layout file is too large"));
            }
            let p: crate::model::Profile = serde_json::from_value(raw).map_err(|e| OpError::invalid(format!("not a Keyside layout: {e}")))?;
            Ok(serde_json::to_value(crate::share::import_profile(p).map_err(OpError::failed)?).unwrap_or(Value::Null))
        }
        "gallery.list" => {
            need_edit(who)?;
            let url = app.settings.read().registry_url.clone();
            let idx = app.registry.index(&app.http, &url).await.map_err(OpError::failed)?;
            Ok(json!({ "layouts": idx.layouts, "plugins": idx.plugins }))
        }
        "gallery.get" => {
            need_edit(who)?;
            let url = app.settings.read().registry_url.clone();
            let idx = app.registry.index(&app.http, &url).await.map_err(OpError::failed)?;
            let entry = idx.layouts.iter().find(|l| l.id == s(&a, "id").unwrap_or("")).ok_or_else(|| OpError::not_found("layout not found"))?;
            let code = app.registry.download_layout(&app.http, entry).await.map_err(OpError::failed)?;
            Ok(serde_json::to_value(crate::share::import(&code).map_err(OpError::failed)?).unwrap_or(Value::Null))
        }
        "recorder.start" => {
            need_edit(who)?;
            app.recorder.start();
            Ok(Value::Null)
        }
        "recorder.stop" => {
            need_edit(who)?;
            Ok(serde_json::to_value(app.recorder.stop()).unwrap_or(Value::Null))
        }
        "plugins.list" => {
            need_edit(who)?;
            Ok(serde_json::to_value(app.plugins.list()).unwrap_or(Value::Null))
        }

        // Everything below changes the PC itself and only works from the local editor.
        "companion.info" => {
            need_admin(who)?;
            let st = app.settings.read().clone();
            Ok(json!({
                "name": st.name,
                "version": env!("CARGO_PKG_VERSION"),
                "addresses": super::local_addresses(),
                "lanPort": st.lan_port,
                "adminPort": st.admin_port,
                "fingerprint": *app.tls_fingerprint.read(),
                "deno": crate::plugins::sandbox::find_deno().is_some(),
                "uptime": app.started.elapsed().as_secs(),
                "zoom": *app.editor_zoom.read(),
                "zoomAuto": st.editor_zoom.is_none(),
            }))
        }
        "settings.get" => {
            need_admin(who)?;
            Ok(serde_json::to_value(app.settings.read().clone()).unwrap_or(Value::Null))
        }
        "settings.set" => {
            need_admin(who)?;
            let mut merged = serde_json::to_value(app.settings.read().clone()).unwrap_or(Value::Null);
            if let (Some(m), Some(patch)) = (merged.as_object_mut(), a.as_object()) {
                for (k, v) in patch {
                    // Zoom has its own op, so an open settings form can't undo it.
                    if m.contains_key(k) && k != "editorZoom" {
                        m.insert(k.clone(), v.clone());
                    }
                }
            }
            let mut next: crate::config::Settings = serde_json::from_value(merged).map_err(|e| OpError::invalid(e.to_string()))?;
            next.name = crate::config::clean_name(&next.name, 40).ok_or_else(|| OpError::invalid("name can't be empty"))?;
            if !next.registry_url.starts_with("https://") {
                return Err(OpError::invalid("registry must be an https link"));
            }
            if next.admin_port == next.lan_port || next.admin_port < 1024 || next.lan_port < 1024 {
                return Err(OpError::invalid("ports must differ and be 1024 or higher"));
            }
            next.save(&app.paths).map_err(OpError::failed)?;
            *app.settings.write() = next;
            Ok(json!({ "restartNeeded": true }))
        }
        "editor.zoom" => {
            need_admin(who)?;
            // Like browsers: Ctrl + and - walk these steps.
            const STEPS: [f64; 13] = [0.5, 0.67, 0.75, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0];
            let saved = if let Some(v) = a.get("value") {
                v.as_f64().map(|z| z.clamp(STEPS[0], STEPS[STEPS.len() - 1]))
            } else {
                let cur = *app.editor_zoom.read();
                match a.get("step").and_then(Value::as_i64) {
                    Some(1) => Some(STEPS.into_iter().find(|z| *z > cur + 0.001).unwrap_or(STEPS[STEPS.len() - 1])),
                    Some(-1) => Some(STEPS.into_iter().rev().find(|z| *z < cur - 0.001).unwrap_or(STEPS[0])),
                    Some(0) => None,
                    _ => return Err(OpError::invalid("step must be -1, 0 or 1")),
                }
            };
            let applied = {
                let hook = app.zoom_hook.read();
                let hook = hook.as_ref().ok_or_else(|| OpError::invalid("zoom only works in the desktop app"))?;
                hook(saved)
            };
            *app.editor_zoom.write() = applied;
            let mut next = app.settings.read().clone();
            next.editor_zoom = saved;
            next.save(&app.paths).map_err(OpError::failed)?;
            *app.settings.write() = next;
            Ok(json!({ "zoom": applied, "auto": saved.is_none() }))
        }
        "pairing.start" => {
            need_admin(who)?;
            let scope: Scope = a.get("scope").cloned().and_then(|v| serde_json::from_value(v).ok()).unwrap_or(Scope::Deck);
            let pages: Vec<String> = a.get("pages").and_then(|p| serde_json::from_value(p.clone()).ok()).unwrap_or_default();
            if scope == Scope::Moderator && pages.is_empty() {
                return Err(OpError::invalid("pick at least one page for a moderator"));
            }
            if pages.len() > 50 || pages.iter().any(|p| p.len() > 90) {
                return Err(OpError::invalid("too many pages"));
            }
            let pin = a.get("pin").and_then(Value::as_str).filter(|p| !p.is_empty());
            if pin.is_some_and(|p| !valid_pin(p)) {
                return Err(OpError::invalid("PIN must be 4 to 12 digits"));
            }
            let (pid, code) = app.devices.lock().start_pairing(scope, pages, pin).map_err(OpError::failed)?;
            Ok(super::pairing_payload(app, &pid, &code))
        }
        "pairing.cancel" => {
            need_admin(who)?;
            app.devices.lock().cancel_pairing();
            Ok(Value::Null)
        }
        "pending.list" => {
            need_admin(who)?;
            Ok(serde_json::to_value(app.devices.lock().pending()).unwrap_or(Value::Null))
        }
        "pending.answer" => {
            need_admin(who)?;
            let pid = s(&a, "pid")?;
            let approve = a.get("approve").and_then(Value::as_bool).unwrap_or(false);
            let tx = app.approvals.lock().remove(pid).ok_or_else(|| OpError::not_found("that request has expired"))?;
            let _ = tx.send(approve);
            Ok(Value::Null)
        }
        "devices.list" => {
            need_admin(who)?;
            let online = |id: &str| app.is_online(id);
            Ok(serde_json::to_value(app.devices.lock().list(&online)).unwrap_or(Value::Null))
        }
        "devices.update" => {
            need_admin(who)?;
            let id = s(&a, "id")?;
            let scope = a.get("scope").cloned().and_then(|v| serde_json::from_value(v).ok());
            let pages = a.get("pages").and_then(|p| serde_json::from_value(p.clone()).ok());
            let pin = match a.get("pin") {
                Some(Value::Null) => Some(None),
                Some(Value::String(p)) if valid_pin(p) => Some(Some(p.as_str())),
                Some(_) => return Err(OpError::invalid("PIN must be 4 to 12 digits")),
                None => None,
            };
            let found = app.devices.lock().update(id, a.get("name").and_then(Value::as_str), scope, pages, pin).map_err(OpError::failed)?;
            if !found {
                return Err(OpError::not_found("device not found"));
            }
            // Reconnect so the new scope and PIN apply right away.
            app.notify(Change::Revoked(id.to_string()));
            Ok(Value::Null)
        }
        "devices.revoke" => {
            need_admin(who)?;
            let id = s(&a, "id")?;
            app.devices.lock().revoke(id).map_err(OpError::failed)?;
            app.notify(Change::Revoked(id.to_string()));
            app.notify(Change::Pending);
            Ok(Value::Null)
        }
        "tokens.list" => {
            need_admin(who)?;
            Ok(serde_json::to_value(app.tokens.lock().list()).unwrap_or(Value::Null))
        }
        "tokens.create" => {
            need_admin(who)?;
            let scopes: Vec<ApiScope> = a.get("scopes").and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default();
            if scopes.is_empty() {
                return Err(OpError::invalid("pick at least one permission"));
            }
            let (t, secret) = app.tokens.lock().create(a.get("name").and_then(Value::as_str).unwrap_or("Token"), scopes).map_err(OpError::failed)?;
            Ok(json!({ "token": t, "secret": secret }))
        }
        "tokens.revoke" => {
            need_admin(who)?;
            Ok(json!(app.tokens.lock().revoke(s(&a, "id")?).map_err(OpError::failed)?))
        }
        "review.list" => {
            need_admin(who)?;
            Ok(review_list(app))
        }
        "review.approve" => {
            need_admin(who)?;
            let only_profile = a.get("profile").and_then(Value::as_str).map(str::to_string);
            let mut l = app.engine.lock().layout().clone();
            for p in &mut l.profiles {
                if only_profile.as_ref().is_some_and(|o| *o != p.id) {
                    continue;
                }
                for pg in &mut p.pages {
                    for k in &mut pg.keys {
                        for act in k.actions.iter_mut().chain(k.off_actions.iter_mut()) {
                            act.review = false;
                        }
                    }
                }
            }
            let rev = app.engine.lock().replace_layout(l).map_err(OpError::failed)?;
            app.plugins.sync_all().await;
            Ok(json!({ "rev": rev }))
        }
        "plugins.stage" => {
            need_admin(who)?;
            let entry_id = s(&a, "registryId")?;
            let url = app.settings.read().registry_url.clone();
            let idx = app.registry.index(&app.http, &url).await.map_err(OpError::failed)?;
            let entry = idx.plugins.iter().find(|p| p.id == entry_id).ok_or_else(|| OpError::not_found("plugin not in the registry"))?;
            let pkg = app.registry.download_plugin(&app.http, entry).await.map_err(OpError::failed)?;
            Ok(stage(app, pkg, Source::Registry))
        }
        "plugins.install" => {
            need_admin(who)?;
            let id = s(&a, "stageId")?;
            let (pkg, source) = {
                let mut st = app.plugins.staged.lock();
                let (pkg, _) = st.remove(id).ok_or_else(|| OpError::not_found("upload expired, try again"))?;
                let src = if pkg.signer.is_some() && a.get("source").and_then(Value::as_str) == Some("registry") { Source::Registry } else { Source::File };
                (pkg, src)
            };
            let approve = a.get("approve").and_then(Value::as_bool).unwrap_or(false);
            app.plugins.install(&pkg, source, approve).map_err(OpError::failed)?;
            Ok(Value::Null)
        }
        "plugins.uninstall" => {
            need_admin(who)?;
            Ok(json!(app.plugins.uninstall(s(&a, "id")?).map_err(OpError::failed)?))
        }
        "plugins.enable" => {
            need_admin(who)?;
            app.plugins.set_enabled(s(&a, "id")?, a.get("enabled").and_then(Value::as_bool).unwrap_or(true)).map_err(OpError::failed)?;
            Ok(Value::Null)
        }
        "plugins.approve" => {
            need_admin(who)?;
            app.plugins.approve(s(&a, "id")?).map_err(OpError::failed)?;
            Ok(Value::Null)
        }
        "plugins.configure" => {
            need_admin(who)?;
            let values = a.get("values").and_then(Value::as_object).cloned().unwrap_or_default();
            // {"set": true} placeholders for secrets mean "leave unchanged".
            let values: Map<String, Value> = values.into_iter().filter(|(_, v)| !(v.is_object() && v.get("set").is_some())).collect();
            app.plugins.configure(s(&a, "id")?, &values, false).map_err(OpError::failed)?;
            Ok(Value::Null)
        }
        "plugins.restart" => {
            need_admin(who)?;
            app.plugins.restart(s(&a, "id")?);
            Ok(Value::Null)
        }
        "sound.stop" => {
            need_edit(who)?;
            #[cfg(feature = "sound")]
            app.sound.stop_all();
            Ok(Value::Null)
        }
        "streamdeck.list" | "streamdeck.install" | "streamdeck.uninstall" | "streamdeck.grant" | "streamdeck.inspector" => {
            need_admin(who)?;
            crate::streamdeck::op(app, op, &a).await.map_err(OpError::failed)
        }
        _ => Err(OpError::invalid(format!("unknown request {op}"))),
    }
}

pub fn stage(app: &App, pkg: crate::plugins::package::Package, source: Source) -> Value {
    let id = crate::ids::random_b64(12);
    let info = json!({
        "stageId": id,
        "source": source,
        "manifest": pkg.manifest,
        "signer": pkg.signer,
        "installed": app.plugins.get(&pkg.manifest.id).map(|i| json!({ "version": i.manifest.version, "granted": i.granted, "signer": i.signer })),
    });
    let mut st = app.plugins.staged.lock();
    st.retain(|_, (_, t)| t.elapsed() < std::time::Duration::from_secs(600));
    st.insert(id, (pkg, std::time::Instant::now()));
    info
}

fn all_variables(app: &App) -> Value {
    serde_json::to_value(app.engine.lock().vars_with_prefix("")).unwrap_or(Value::Null)
}

fn review_list(app: &App) -> Value {
    let l = app.engine.lock().layout().clone();
    let mut out = Vec::new();
    for (p, pg, k) in all_keys(&l) {
        for act in k.actions.iter().chain(&k.off_actions) {
            if act.review {
                out.push(json!({ "profile": p.name, "profileId": p.id, "page": pg.name, "key": k.label, "type": act.kind, "settings": act.settings }));
            }
        }
    }
    Value::Array(out)
}

/// Editors on other devices can rearrange anything, but can't add steps that start programs
/// or approve imported steps. Those need the desktop editor.
fn check_editor_changes(old: &LayoutFile, new: &LayoutFile) -> Result<(), OpError> {
    let old_actions: Vec<&Action> = all_keys(old).flat_map(|(_, _, k)| k.actions.iter().chain(&k.off_actions)).collect();
    for (_, _, k) in all_keys(new) {
        for a in k.actions.iter().chain(&k.off_actions) {
            if actions::is_admin_only(a) && !old_actions.iter().any(|o| o.same_as(a)) {
                return Err(OpError { code: "forbidden", message: format!("\"{}\" can only be added on the PC", k.label) });
            }
            if !a.review && !old_actions.iter().any(|o| o.same_as(a) && !o.review) && old_actions.iter().any(|o| o.same_as(a) && o.review) {
                return Err(OpError { code: "forbidden", message: "Imported steps can only be approved on the PC".into() });
            }
        }
    }
    Ok(())
}

/// Drops unknown or invalid settings so stored layouts only hold what each action defines.
fn clean_layout(app: &App, l: &mut LayoutFile) {
    for p in &mut l.profiles {
        for pg in &mut p.pages {
            for k in &mut pg.keys {
                for a in k.actions.iter_mut().chain(k.off_actions.iter_mut()) {
                    let schema = match a.plugin() {
                        Some((plugin, action)) => app.plugins.action_def(plugin, action).and_then(|d| d.settings),
                        None => catalog::core_schema(&a.kind),
                    };
                    if let Some(s) = schema {
                        a.settings = manifest::sanitize(Some(&s), &a.settings);
                    }
                }
            }
        }
    }
}

async fn save_layout(app: &Arc<App>, who: &Who, a: &Value) -> R {
    let mut new: LayoutFile = serde_json::from_value(a.get("layout").cloned().unwrap_or(Value::Null)).map_err(|e| OpError::invalid(format!("layout is invalid: {e}")))?;
    let base = a.get("baseRev").and_then(Value::as_u64);
    model::validate(&new).map_err(|e| OpError::invalid(e.0))?;
    clean_layout(app, &mut new);
    let rev = {
        let mut e = app.engine.lock();
        if base.is_some_and(|b| b != e.layout().rev) {
            return Err(OpError::conflict("The layout changed on another device. Reload to see the latest version."));
        }
        if !who.admin {
            check_editor_changes(e.layout(), &new)?;
        }
        crate::streamdeck::keep_settings(e.layout(), &mut new);
        let rev = e.replace_layout(new).map_err(OpError::failed)?;
        if !who.admin {
            e.toast(format!("Layout changed by {}", who.name), Tone::Default);
        }
        rev
    };
    app.plugins.sync_all().await;
    Ok(json!({ "rev": rev }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_editors_cannot_add_launch_steps() {
        let old = crate::state::default_layout();
        let mut new = old.clone();
        let mut s = Map::new();
        s.insert("path".into(), json!("/bin/sh"));
        new.profiles[0].pages[0].keys[0].actions.push(Action { kind: "core.launch".into(), settings: s, review: false });
        assert!(check_editor_changes(&old, &new).is_err());
        assert!(check_editor_changes(&new, &new).is_ok(), "existing steps can stay");
    }

    #[tokio::test]
    async fn zoom_steps_saves_and_goes_back_to_automatic() {
        let dir = tempfile::tempdir().unwrap();
        let app = App::new(crate::app::Options { data_dir: Some(dir.path().into()), bundled_plugins: None }).unwrap();
        let admin = Who { admin: true, device: String::new(), name: String::new(), scope: Scope::Editor, pages: None };
        let phone = Who { admin: false, ..admin.clone() };
        assert!(handle(&app, &admin, "editor.zoom", json!({ "step": 1 })).await.is_err(), "needs the desktop app");

        *app.zoom_hook.write() = Some(Box::new(|saved| saved.unwrap_or(1.25)));
        *app.editor_zoom.write() = 1.25;
        let z = |r: R| r.ok().unwrap()["zoom"].as_f64().unwrap();
        assert_eq!(z(handle(&app, &admin, "editor.zoom", json!({ "step": 1 })).await), 1.5);
        assert_eq!(app.settings.read().editor_zoom, Some(1.5));
        assert_eq!(z(handle(&app, &admin, "editor.zoom", json!({ "step": -1 })).await), 1.25);
        assert_eq!(z(handle(&app, &admin, "editor.zoom", json!({ "value": 9.0 })).await), 3.0);
        assert_eq!(z(handle(&app, &admin, "editor.zoom", json!({ "step": 0 })).await), 1.25);
        assert_eq!(app.settings.read().editor_zoom, None);
        assert!(handle(&app, &phone, "editor.zoom", json!({ "step": 1 })).await.is_err(), "only the local editor");

        // An open settings form can't undo it.
        *app.editor_zoom.write() = 1.25;
        assert!(handle(&app, &admin, "editor.zoom", json!({ "value": 1.1 })).await.is_ok());
        let mut form = serde_json::to_value(app.settings.read().clone()).unwrap();
        form["editorZoom"] = Value::Null;
        assert!(handle(&app, &admin, "settings.set", form).await.is_ok());
        assert_eq!(app.settings.read().editor_zoom, Some(1.1));
    }

    #[test]
    fn remote_editors_cannot_clear_review() {
        let mut old = crate::state::default_layout();
        old.profiles[0].pages[0].keys[0].actions[0].review = true;
        let mut new = old.clone();
        new.profiles[0].pages[0].keys[0].actions[0].review = false;
        assert!(check_editor_changes(&old, &new).is_err());
    }
}

//! Live state shared by every connection: layout, variables, plugin key state, feeds.

pub mod view;

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use anyhow::Result;
use serde::Serialize;
use serde_json::Value;
use tokio::sync::broadcast;

use crate::config::Paths;
use crate::model::{self, Action, FormFactor, Key, KeyKind, LayoutFile, Page, Profile, Tone};
use crate::store;
use view::{FeedItem, KeyView, PageView, ViewPatch};

const FEED_KEEP: usize = 50;
const MAX_VARS: usize = 5_000;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionView {
    pub id: String,
    pub name: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Pill {
    pub id: String,
    pub label: String,
    pub tone: Tone,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<u64>,
}

#[derive(Debug, Clone)]
pub enum Change {
    Layout,
    Keys,
    Vars,
    Status,
    Profile,
    Toast { text: String, tone: Tone },
    Revoked(String),
    Pending,
    Plugins,
    Pressed { profile: String, page: String, key: String, label: String, device: String },
}

pub struct Engine {
    paths: Paths,
    layout: LayoutFile,
    vars: HashMap<String, Value>,
    overrides: HashMap<String, ViewPatch>,
    /// Set through the inbound API, keyed by "profile/page/key".
    key_overrides: HashMap<String, ViewPatch>,
    toggles: HashMap<String, bool>,
    feeds: HashMap<String, VecDeque<FeedItem>>,
    connections: BTreeMap<String, ConnectionView>,
    pills: BTreeMap<String, Pill>,
    active_profile: String,
    installed: HashSet<String>,
    tx: broadcast::Sender<Change>,
}

/// Instance ids name one action on one key: "profile/page/key/n", or ".../o<n>" for off actions.
pub fn instance_id(profile: &str, page: &str, key: &str, off: bool, n: usize) -> String {
    format!("{profile}/{page}/{key}/{}{n}", if off { "o" } else { "" })
}

pub fn key_path(profile: &str, page: &str, key: &str) -> String {
    format!("{profile}/{page}/{key}")
}

#[derive(Debug, Clone, Serialize)]
pub struct Instance {
    pub id: String,
    pub action: String,
    pub settings: serde_json::Map<String, Value>,
    /// The key's own label and icon, so plugins can pick fitting variants (mic -> mic-off).
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

impl Engine {
    pub fn load(paths: Paths) -> Result<(Self, broadcast::Receiver<Change>)> {
        let layout = match store::read_json::<LayoutFile>(&paths.file("layout.json"))? {
            Some(l) if model::validate(&l).is_ok() => l,
            Some(_) => {
                let bad = paths.file("layout.invalid.json");
                let _ = std::fs::rename(paths.file("layout.json"), &bad);
                tracing::warn!("stored layout was invalid; kept it as {} and started fresh", bad.display());
                default_layout()
            }
            None => default_layout(),
        };
        let user_vars: HashMap<String, Value> = store::read_json(&paths.file("variables.json"))?.unwrap_or_default();
        let (tx, rx) = broadcast::channel(1024);
        let active_profile = layout.default_profile.clone();
        Ok((
            Self {
                paths,
                layout,
                vars: user_vars.into_iter().filter(|(k, _)| k.starts_with("user.")).collect(),
                overrides: HashMap::new(),
                key_overrides: HashMap::new(),
                toggles: HashMap::new(),
                feeds: HashMap::new(),
                connections: BTreeMap::new(),
                pills: BTreeMap::new(),
                active_profile,
                installed: HashSet::new(),
                tx,
            },
            rx,
        ))
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Change> {
        self.tx.subscribe()
    }

    pub fn notify(&self, c: Change) {
        let _ = self.tx.send(c);
    }

    pub fn toast(&self, text: impl Into<String>, tone: Tone) {
        self.notify(Change::Toast { text: text.into(), tone });
    }

    pub fn layout(&self) -> &LayoutFile {
        &self.layout
    }

    /// Stores a validated layout. Callers check who may change what first.
    pub fn replace_layout(&mut self, mut new: LayoutFile) -> Result<u64> {
        new.rev = self.layout.rev + 1;
        store::write_json(&self.paths.file("layout.json"), &new)?;
        let keep: HashSet<String> = all_keys(&new).map(|(p, pg, k)| key_path(&p.id, &pg.id, &k.id)).collect();
        self.toggles.retain(|k, _| keep.contains(k));
        self.key_overrides.retain(|k, _| keep.contains(k));
        self.overrides.retain(|inst, _| inst.rsplit_once('/').is_some_and(|(k, _)| keep.contains(k)));
        if new.profiles.iter().all(|p| p.id != self.active_profile) {
            self.active_profile = new.default_profile.clone();
        }
        self.layout = new;
        self.notify(Change::Layout);
        Ok(self.layout.rev)
    }

    pub fn active_profile(&self) -> &str {
        &self.active_profile
    }

    pub fn set_active_profile(&mut self, id: &str) -> bool {
        if self.active_profile == id || self.profile(id).is_none() {
            return false;
        }
        self.active_profile = id.to_string();
        self.vars.insert("core.profile".into(), Value::String(self.profile(id).map(|p| p.name.clone()).unwrap_or_default()));
        self.notify(Change::Profile);
        true
    }

    pub fn profile(&self, id: &str) -> Option<&Profile> {
        self.layout.profiles.iter().find(|p| p.id == id)
    }

    /// Profile whose app list matches a process name (case-insensitive, with or without .exe).
    pub fn profile_for_app(&self, process: &str) -> Option<String> {
        let norm = |s: &str| s.trim().trim_end_matches(".exe").trim_end_matches(".app").to_ascii_lowercase();
        let want = norm(process);
        self.layout.profiles.iter().find(|p| p.apps.iter().any(|a| norm(a) == want)).map(|p| p.id.clone())
    }

    pub fn page(&self, profile: &str, page: &str) -> Option<&Page> {
        self.profile(profile)?.pages.iter().find(|p| p.id == page)
    }

    pub fn key(&self, profile: &str, page: &str, key: &str) -> Option<&Key> {
        self.page(profile, page)?.keys.iter().find(|k| k.id == key)
    }

    pub fn set_installed(&mut self, ids: HashSet<String>) {
        self.installed = ids;
        self.notify(Change::Keys);
    }

    pub fn var(&self, name: &str) -> Option<Value> {
        self.vars.get(name).cloned()
    }

    pub fn vars_with_prefix(&self, prefix: &str) -> BTreeMap<String, Value> {
        self.vars.iter().filter(|(k, _)| k.starts_with(prefix)).map(|(k, v)| (k.clone(), v.clone())).collect()
    }

    /// Sets a variable. Returns false when the store is full or the name is invalid.
    pub fn set_var(&mut self, name: &str, value: Value) -> bool {
        if !model::valid_var_name(name) || !(value.is_null() || value.is_string() || value.is_number() || value.is_boolean()) {
            return false;
        }
        let value = match value {
            Value::String(s) if s.chars().count() > 512 => Value::String(s.chars().take(512).collect()),
            v => v,
        };
        if !self.vars.contains_key(name) && self.vars.len() >= MAX_VARS {
            return false;
        }
        if self.vars.get(name) == Some(&value) {
            return true;
        }
        self.vars.insert(name.to_string(), value);
        if name.starts_with("user.") {
            let user: BTreeMap<_, _> = self.vars_with_prefix("user.");
            let _ = store::write_json(&self.paths.file("variables.json"), &user);
        }
        self.notify(Change::Vars);
        true
    }

    pub fn set_override(&mut self, instance: &str, patch: Option<ViewPatch>) {
        let changed = match patch {
            Some(p) => self.overrides.insert(instance.to_string(), p.clone()) != Some(p),
            None => self.overrides.remove(instance).is_some(),
        };
        if changed {
            self.notify(Change::Keys);
        }
    }

    pub fn set_key_override(&mut self, path: &str, patch: Option<ViewPatch>) {
        let changed = match patch {
            Some(p) => self.key_overrides.insert(path.to_string(), p.clone()) != Some(p),
            None => self.key_overrides.remove(path).is_some(),
        };
        if changed {
            self.notify(Change::Keys);
        }
    }

    pub fn clear_overrides_with_prefix(&mut self, owns: impl Fn(&str) -> bool) {
        let before = self.overrides.len();
        self.overrides.retain(|k, _| !owns(k));
        if self.overrides.len() != before {
            self.notify(Change::Keys);
        }
    }

    pub fn push_feed(&mut self, feed: &str, mut item: FeedItem) {
        item.at = crate::devices::now() * 1000;
        let q = self.feeds.entry(feed.to_string()).or_default();
        q.push_back(item);
        while q.len() > FEED_KEEP {
            q.pop_front();
        }
        self.notify(Change::Keys);
    }

    pub fn set_connection(&mut self, c: ConnectionView) {
        if self.connections.get(&c.id) != Some(&c) {
            self.connections.insert(c.id.clone(), c);
            self.notify(Change::Status);
        }
    }

    pub fn remove_connection(&mut self, id: &str) {
        let a = self.connections.remove(id).is_some();
        let prefix = format!("{id}#");
        let before = self.pills.len();
        self.pills.retain(|k, _| !k.starts_with(&prefix));
        let b = self.pills.len() != before;
        if a || b {
            self.notify(Change::Status);
        }
    }

    pub fn set_pill(&mut self, id: &str, pill: Option<Pill>) {
        self.set_pills(id, pill.into_iter().collect());
    }

    /// Replaces every status pill owned by `owner` (up to three).
    pub fn set_pills(&mut self, owner: &str, pills: Vec<Pill>) {
        let prefix = format!("{owner}#");
        let before: Vec<Pill> = self.pills.iter().filter(|(k, _)| k.starts_with(&prefix)).map(|(_, v)| v.clone()).collect();
        if before == pills {
            return;
        }
        self.pills.retain(|k, _| !k.starts_with(&prefix));
        for (i, p) in pills.into_iter().take(3).enumerate() {
            self.pills.insert(format!("{prefix}{i}"), p);
        }
        self.notify(Change::Status);
    }

    pub fn status(&self) -> (Vec<ConnectionView>, Vec<Pill>) {
        (self.connections.values().cloned().collect(), self.pills.values().cloned().collect())
    }

    /// Flips a toggle key and returns its new state.
    pub fn flip_toggle(&mut self, path: &str) -> bool {
        let v = self.toggles.entry(path.to_string()).or_insert(false);
        *v = !*v;
        let on = *v;
        self.notify(Change::Keys);
        on
    }

    pub fn toggle_state(&self, path: &str) -> bool {
        self.toggles.get(path).copied().unwrap_or(false)
    }

    /// Replaces the settings of one action, when its type starts with `prefix`. For settings
    /// owned by Stream Deck plugins: saved right away, without a new layout revision, so
    /// editors with unsaved changes don't run into a conflict.
    pub fn set_action_settings(&mut self, instance: &str, prefix: &str, settings: serde_json::Map<String, Value>) -> bool {
        let Some((key_path, n)) = instance.rsplit_once('/') else { return false };
        let Ok(n) = n.parse::<usize>() else { return false };
        let mut parts = key_path.splitn(3, '/');
        let (Some(p), Some(pg), Some(k)) = (parts.next(), parts.next(), parts.next()) else { return false };
        let Some(action) = self
            .layout
            .profiles
            .iter_mut()
            .find(|x| x.id == p)
            .and_then(|x| x.pages.iter_mut().find(|x| x.id == pg))
            .and_then(|x| x.keys.iter_mut().find(|x| x.id == k))
            .and_then(|x| x.actions.get_mut(n))
        else {
            return false;
        };
        if !action.kind.starts_with(prefix) {
            return false;
        }
        action.settings = settings;
        if let Err(e) = store::write_json(&self.paths.file("layout.json"), &self.layout) {
            tracing::warn!("could not save plugin settings: {e:#}");
        }
        true
    }

    /// Every placed action of one plugin, so the plugin can draw its keys.
    pub fn instances_for(&self, plugin: &str) -> Vec<Instance> {
        let mut out = Vec::new();
        for (p, pg, k) in all_keys(&self.layout) {
            for (off, list) in [(false, &k.actions), (true, &k.off_actions)] {
                for (n, a) in list.iter().enumerate() {
                    if a.plugin().is_some_and(|(id, _)| id == plugin) && !a.review {
                        out.push(Instance {
                            id: instance_id(&p.id, &pg.id, &k.id, off, n),
                            action: a.kind.clone(),
                            settings: a.settings.clone(),
                            label: k.label.clone(),
                            icon: k.icon.clone(),
                        });
                    }
                }
            }
        }
        out
    }

    /// Top-level pages a device may see, in order.
    pub fn visible_pages<'a>(&'a self, profile: &'a Profile, allowed: Option<&'a [String]>) -> impl Iterator<Item = &'a Page> + 'a {
        profile.pages.iter().filter(move |p| !p.folder && allowed.is_none_or(|a| a.contains(&p.id)))
    }

    pub fn may_see(&self, profile: &Profile, page: &str, allowed: Option<&[String]>) -> bool {
        let Some(pg) = profile.pages.iter().find(|p| p.id == page) else { return false };
        match allowed {
            None => true,
            Some(a) if a.contains(&pg.id) => true,
            // Folders opened from an allowed page are allowed too.
            Some(a) => pg.folder && profile.pages.iter().any(|p| a.contains(&p.id) && p.keys.iter().any(|k| k.target.as_deref() == Some(page))),
        }
    }

    pub fn page_view(&self, profile_id: &str, page_id: &str, ff: FormFactor, allowed: Option<&[String]>) -> Option<PageView> {
        let profile = self.profile(profile_id)?;
        if !self.may_see(profile, page_id, allowed) {
            return None;
        }
        let page = profile.pages.iter().find(|p| p.id == page_id)?;
        let tops: Vec<&Page> = self.visible_pages(profile, allowed).collect();
        let index = tops.iter().position(|p| p.id == page.id).unwrap_or(0);
        let (cols, rows, places) = view::placements(page, ff);
        let keys = places
            .into_iter()
            .map(|(i, pos)| self.key_view(profile, page, &page.keys[i], pos))
            .collect();
        Some(PageView {
            id: page.id.clone(),
            name: page.name.clone(),
            subtitle: page.subtitle.clone(),
            folder: page.folder,
            cols,
            rows,
            index,
            count: tops.len(),
            keys,
        })
    }

    fn key_view(&self, profile: &Profile, page: &Page, key: &Key, pos: [u8; 4]) -> KeyView {
        let folder_size = key.target.as_deref().and_then(|t| profile.pages.iter().find(|p| p.id == t)).map(|p| p.keys.len());
        let mut v = view::base_view(key, pos, folder_size);
        let get = |n: &str| self.var(n);

        if let Some(name) = key.actions.iter().find(|a| a.kind == "core.timer").and_then(|a| a.settings.get("name")).and_then(Value::as_str) {
            if let Some(t) = self.var(&format!("timer.{name}")) {
                v.meta = Some(view::display(&t));
            }
            if self.var(&format!("timer.{name}.running")).is_some_and(|r| view::truthy(&r)) {
                v.tone = Some(Tone::Accent);
            }
        }
        if key.kind == KeyKind::Toggle && self.toggle_state(&key_path(&profile.id, &page.id, &key.id)) {
            v.badge = Some("ON".into());
            v.tone = Some(Tone::Success);
        }
        let missing = key
            .actions
            .iter()
            .filter_map(Action::plugin)
            .find(|(id, _)| !self.installed.contains(*id));
        if missing.is_some() && !v.disabled {
            v.badge = Some("MISSING".into());
            v.tone = Some(Tone::Muted);
            v.disabled = true;
        }
        for (n, _) in key.actions.iter().enumerate() {
            if let Some(p) = self.overrides.get(&instance_id(&profile.id, &page.id, &key.id, false, n)) {
                view::apply_patch(&mut v, p);
            }
        }
        if let Some(p) = self.key_overrides.get(&key_path(&profile.id, &page.id, &key.id)) {
            view::apply_patch(&mut v, p);
        }
        for f in &key.feedback {
            if view::eval(&f.when, get) {
                view::apply_style(&mut v, &f.style);
            }
        }
        if let Some(w) = &key.widget {
            if let Some(feed) = &w.feed {
                let n = w.lines.unwrap_or(5) as usize;
                v.lines = Some(self.feeds.get(feed).map(|q| q.iter().rev().take(n).rev().cloned().collect()).unwrap_or_default());
            }
            if let Some(t) = &w.text {
                v.sub = Some(view::template(t, get));
            }
            if let Some(p) = &w.progress {
                v.progress = self.var(p).and_then(|x| x.as_f64()).map(|f| f.clamp(0.0, 1.0));
            }
        }
        v.label = view::template(&v.label, get);
        v.sub = v.sub.map(|s| view::template(&s, get));
        v.meta = v.meta.map(|s| view::template(&s, get));
        v.badge = v.badge.map(|s| view::template(&s, get));
        v
    }
}

pub fn all_keys(l: &LayoutFile) -> impl Iterator<Item = (&Profile, &Page, &Key)> {
    l.profiles.iter().flat_map(|p| p.pages.iter().flat_map(move |pg| pg.keys.iter().map(move |k| (p, pg, k))))
}

pub fn default_layout() -> LayoutFile {
    serde_json::from_str(include_str!("../../assets/default-layout.json")).expect("bundled default layout is valid")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> (tempfile::TempDir, Engine) {
        let t = tempfile::tempdir().unwrap();
        let (e, _) = Engine::load(Paths { root: t.path().to_path_buf() }).unwrap();
        (t, e)
    }

    #[test]
    fn default_layout_is_valid() {
        model::validate(&default_layout()).unwrap();
    }

    #[test]
    fn moderators_only_see_their_pages() {
        let (_t, e) = engine();
        let p = e.layout().default_profile.clone();
        let pages: Vec<String> = e.profile(&p).unwrap().pages.iter().filter(|p| !p.folder).map(|p| p.id.clone()).collect();
        assert!(pages.len() >= 2);
        let allowed = vec![pages[0].clone()];
        assert!(e.page_view(&p, &pages[0], FormFactor::PhonePortrait, Some(&allowed)).is_some());
        assert!(e.page_view(&p, &pages[1], FormFactor::PhonePortrait, Some(&allowed)).is_none());
        assert_eq!(e.page_view(&p, &pages[0], FormFactor::PhonePortrait, Some(&allowed)).unwrap().count, 1);
    }

    #[test]
    fn variables_reject_bad_names_and_objects() {
        let (_t, mut e) = engine();
        assert!(e.set_var("user.goal", Value::from(5)));
        assert!(!e.set_var("bad name", Value::from(5)));
        assert!(!e.set_var("user.obj", serde_json::json!({"a": 1})));
    }
}

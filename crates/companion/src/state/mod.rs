//! Live state shared by every connection: layout, variables, plugin key state, feeds.

pub mod view;

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use anyhow::Result;
use serde::Serialize;
use serde_json::Value;
use tokio::sync::broadcast;

use crate::config::Paths;
use crate::model::{self, Action, FormFactor, Key, KeyKind, LayoutFile, Page, Profile, StepList, Tone};
use crate::store;
use view::{FeedItem, FeedMatch, KeyView, PageView, ViewPatch};

const FEED_KEEP: usize = 50;
/// Lines a feed widget gets. Enough to fill a tall key; the client clips what doesn't fit.
const FEED_SHOWN: usize = 20;
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
    Toast {
        text: String,
        tone: Tone,
    },
    Revoked(String),
    /// A device's settings changed. It reconnects to pick them up.
    Updated(String),
    Pending,
    Plugins,
    /// Key templates changed. Every editor reloads them.
    Templates,
    Pressed {
        profile: String,
        page: String,
        key: String,
        label: String,
        device: String,
    },
}

pub struct Engine {
    paths: Paths,
    layout: LayoutFile,
    vars: HashMap<String, Value>,
    overrides: HashMap<String, ViewPatch>,
    /// Set through the inbound API, keyed by "profile/page/key".
    key_overrides: HashMap<String, ViewPatch>,
    toggles: HashMap<String, bool>,
    /// Multi-state keys: the state the next press runs, by key path.
    states: HashMap<String, usize>,
    /// Toggles or states changed since the last save.
    states_dirty: bool,
    feeds: HashMap<String, VecDeque<FeedItem>>,
    connections: BTreeMap<String, ConnectionView>,
    pills: BTreeMap<String, Pill>,
    active_profile: String,
    installed: HashSet<String>,
    tx: broadcast::Sender<Change>,
    /// User variables changed since the last save. Saved by `flush_vars`, off the lock.
    vars_dirty: bool,
    /// Uptime, crashes and recent status messages of each plugin.
    pub health: crate::health::Tracker,
}

/// Instance ids name one action on one key: "profile/page/key/n", or with a list tag before
/// the number for the other step lists: "o" for off, "s<state>." for states, "l" for long
/// press, "d" for double tap.
pub fn instance_id(profile: &str, page: &str, key: &str, list: StepList, n: usize) -> String {
    format!("{profile}/{page}/{key}/{}{n}", list.tag())
}

/// Toggle and multi-state positions, saved so keys come back the way they were left.
#[derive(Debug, Default, Serialize, serde::Deserialize)]
struct KeyStates {
    #[serde(default)]
    toggles: HashMap<String, bool>,
    #[serde(default)]
    states: HashMap<String, usize>,
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
        let layout = match store::read_json_or_reset::<LayoutFile>(&paths.file("layout.json"))? {
            Some(l) if model::validate(&l).is_ok() => l,
            Some(_) => {
                let bad = paths.file("layout.invalid.json");
                let _ = std::fs::rename(paths.file("layout.json"), &bad);
                tracing::warn!("stored layout was invalid; kept it as {} and started fresh", bad.display());
                default_layout()
            }
            None => default_layout(),
        };
        let user_vars: HashMap<String, Value> = store::read_json_or_reset(&paths.file("variables.json"))?.unwrap_or_default();
        let saved: KeyStates = store::read_json_or_reset(&paths.file("key-states.json"))?.unwrap_or_default();
        let (tx, rx) = broadcast::channel(1024);
        let active_profile = layout.default_profile.clone();
        Ok((
            Self {
                paths,
                layout,
                vars: user_vars.into_iter().filter(|(k, _)| k.starts_with("user.")).collect(),
                overrides: HashMap::new(),
                key_overrides: HashMap::new(),
                toggles: saved.toggles,
                states: saved.states,
                states_dirty: false,
                feeds: HashMap::new(),
                connections: BTreeMap::new(),
                pills: BTreeMap::new(),
                active_profile,
                installed: HashSet::new(),
                tx,
                vars_dirty: false,
                health: Default::default(),
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

    /// Reads toggle and multi-state positions again, after a restore.
    pub fn reload_key_states(&mut self) -> Result<()> {
        let saved: KeyStates = store::read_json_or_reset(&self.paths.file("key-states.json"))?.unwrap_or_default();
        self.toggles = saved.toggles;
        self.states = saved.states;
        self.states_dirty = false;
        self.notify(Change::Keys);
        Ok(())
    }

    pub fn layout(&self) -> &LayoutFile {
        &self.layout
    }

    /// Stores a validated layout. Callers check who may change what first.
    pub fn replace_layout(&mut self, mut new: LayoutFile) -> Result<u64> {
        new.rev = self.layout.rev + 1;
        store::write_json(&self.paths.file("layout.json"), &new)?;
        let keep: HashSet<String> = all_keys(&new).map(|(p, pg, k)| key_path(&p.id, &pg.id, &k.id)).collect();
        let before = self.toggles.len() + self.states.len();
        self.toggles.retain(|k, _| keep.contains(k));
        self.states.retain(|k, _| keep.contains(k));
        self.states_dirty |= self.toggles.len() + self.states.len() != before;
        self.key_overrides.retain(|k, _| keep.contains(k));
        self.overrides.retain(|inst, _| inst.rsplit_once('/').is_some_and(|(k, _)| keep.contains(k)));
        if new.profiles.iter().all(|p| p.id != self.active_profile) {
            self.active_profile = new.default_profile.clone();
        }
        self.layout = new;
        self.notify(Change::Layout);
        // Which plugin connections show depends on the layout.
        self.notify(Change::Status);
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
    /// Layouts of physical decks switch on their own and are skipped.
    pub fn profile_for_app(&self, process: &str) -> Option<String> {
        self.layout.profiles.iter().find(|p| p.deck.is_none() && p.apps.iter().any(|a| crate::focus::same_app(a, process))).map(|p| p.id.clone())
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
            self.vars_dirty = true;
        }
        self.notify(Change::Vars);
        true
    }

    /// Toggle and multi-state positions to save, if any changed.
    pub fn take_unsaved_states(&mut self) -> Option<(std::path::PathBuf, Value)> {
        if !self.states_dirty {
            return None;
        }
        self.states_dirty = false;
        let all = KeyStates { toggles: self.toggles.clone(), states: self.states.clone() };
        Some((self.paths.file("key-states.json"), serde_json::to_value(all).unwrap_or(Value::Null)))
    }

    /// The user variables to save, if any changed. Writing happens outside the engine lock,
    /// since sliders can change a variable many times a second.
    pub fn take_unsaved_vars(&mut self) -> Option<(std::path::PathBuf, BTreeMap<String, Value>)> {
        if !self.vars_dirty {
            return None;
        }
        self.vars_dirty = false;
        Some((self.paths.file("variables.json"), self.vars_with_prefix("user.")))
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
        let q = self.feeds.entry(feed.to_string()).or_default();
        // Strictly increasing, since clients tell lines apart by it.
        item.at = crate::server::conn::now_ms().max(q.back().map_or(0, |last| last.at + 1));
        q.push_back(item);
        while q.len() > FEED_KEEP {
            q.pop_front();
        }
        self.notify(Change::Keys);
    }

    /// Takes lines off a feed, such as chat messages deleted by a moderator.
    pub fn remove_feed(&mut self, feed: &str, m: &FeedMatch) {
        let Some(q) = self.feeds.get_mut(feed) else { return };
        let before = q.len();
        q.retain(|i| !m.matches(i));
        if q.len() != before {
            self.notify(Change::Keys);
        }
    }

    pub fn set_connection(&mut self, c: ConnectionView) {
        if self.connections.get(&c.id) != Some(&c) {
            self.health.status(&c.id, crate::server::conn::now_ms(), &c.status, c.detail.as_deref());
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
        // Each pill gets its own id, so clients can tell them apart.
        let pills: Vec<Pill> = pills.into_iter().take(3).enumerate().map(|(i, p)| Pill { id: format!("{prefix}{i}"), ..p }).collect();
        let before: Vec<Pill> = self.pills.iter().filter(|(k, _)| k.starts_with(&prefix)).map(|(_, v)| v.clone()).collect();
        if before == pills {
            return;
        }
        self.pills.retain(|k, _| !k.starts_with(&prefix));
        for p in pills {
            self.pills.insert(p.id.clone(), p);
        }
        self.notify(Change::Status);
    }

    /// Every plugin connection, used in the layout or not.
    pub fn connections(&self) -> Vec<ConnectionView> {
        self.connections.values().cloned().collect()
    }

    /// Connections of plugins the layout uses, and every status pill.
    pub fn status(&self) -> (Vec<ConnectionView>, Vec<Pill>) {
        debug_assert!(self.pills.iter().all(|(k, p)| *k == p.id));
        let used: HashSet<&str> = all_keys(&self.layout).flat_map(|(_, _, k)| k.all_actions()).filter_map(|a| a.plugin().map(|(id, _)| id)).collect();
        (self.connections.values().filter(|c| used.contains(c.id.as_str())).cloned().collect(), self.pills.values().cloned().collect())
    }

    /// Latest status of one plugin, whether or not the layout uses it.
    pub fn connection(&self, id: &str) -> Option<ConnectionView> {
        self.connections.get(id).cloned()
    }

    /// Takes over user variables from a restored backup.
    pub fn restore_vars(&mut self, user: HashMap<String, Value>) {
        self.vars.retain(|k, _| !k.starts_with("user."));
        self.vars.extend(user.into_iter().filter(|(k, _)| k.starts_with("user.")));
        self.vars_dirty = false;
        self.notify(Change::Vars);
    }

    /// Flips a toggle key and returns its new state.
    pub fn flip_toggle(&mut self, path: &str) -> bool {
        let v = self.toggles.entry(path.to_string()).or_insert(false);
        *v = !*v;
        let on = *v;
        self.states_dirty = true;
        self.notify(Change::Keys);
        on
    }

    /// The state a multi-state key shows. A followed variable wins when one of the states
    /// has its value; otherwise it's where the presses left the key.
    pub fn key_state(&self, path: &str, key: &Key) -> usize {
        let n = key.states.len().max(1);
        if let Some(var) = &key.follow {
            let cur = self.var(var).unwrap_or(Value::Null);
            if let Some(i) = key.states.iter().position(|s| s.value.as_ref().is_some_and(|v| view::loose_eq(&cur, v))) {
                return i;
            }
        }
        self.states.get(path).copied().unwrap_or(0) % n
    }

    /// A press of a multi-state key: returns the state whose steps run and moves on to the next.
    pub fn advance_state(&mut self, path: &str, key: &Key) -> usize {
        let cur = self.key_state(path, key);
        self.states.insert(path.to_string(), (cur + 1) % key.states.len().max(1));
        self.states_dirty = true;
        self.notify(Change::Keys);
        cur
    }

    pub fn toggle_state(&self, path: &str) -> bool {
        self.toggles.get(path).copied().unwrap_or(false)
    }

    /// Replaces the settings of one action, when its type starts with `prefix`. For settings
    /// owned by Stream Deck plugins: saved right away, without a new layout revision, so
    /// editors with unsaved changes don't run into a conflict.
    pub fn set_action_settings(&mut self, instance: &str, prefix: &str, settings: serde_json::Map<String, Value>) -> bool {
        let Some((key_path, n)) = instance.rsplit_once('/') else { return false };
        let Some((list, n)) = StepList::parse(n) else { return false };
        let mut parts = key_path.splitn(3, '/');
        let (Some(p), Some(pg), Some(k)) = (parts.next(), parts.next(), parts.next()) else { return false };
        let Some(action) = self
            .layout
            .profiles
            .iter_mut()
            .find(|x| x.id == p)
            .and_then(|x| x.pages.iter_mut().find(|x| x.id == pg))
            .and_then(|x| x.keys.iter_mut().find(|x| x.id == k))
            .and_then(|x| x.steps_mut(list))
            .and_then(|x| x.get_mut(n))
        else {
            return false;
        };
        if !action.kind.starts_with(prefix) {
            return false;
        }
        // Keep the layout valid, or it would be thrown away on the next start.
        if serde_json::to_vec(&settings).map(|v| v.len()).unwrap_or(usize::MAX) > model::MAX_ACTION_SETTINGS {
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
        self.instances_where(|a| a.plugin().is_some_and(|(id, _)| id == plugin))
    }

    /// Steps of one built-in kind, like "core.nowplaying".
    pub fn instances_of(&self, kind: &str) -> Vec<Instance> {
        self.instances_where(|a| a.kind == kind)
    }

    fn instances_where(&self, wanted: impl Fn(&Action) -> bool) -> Vec<Instance> {
        let mut out = Vec::new();
        for (p, pg, k) in all_keys(&self.layout) {
            for (list, steps) in k.step_lists() {
                for (n, a) in steps.iter().enumerate() {
                    if wanted(a) && !a.review {
                        out.push(Instance {
                            id: instance_id(&p.id, &pg.id, &k.id, list, n),
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
        let keys = places.into_iter().map(|(i, pos)| self.key_view(profile, page, &page.keys[i], pos)).collect();
        Some(PageView { id: page.id.clone(), name: page.name.clone(), subtitle: page.subtitle.clone(), folder: page.folder, cols, rows, index, count: tops.len(), keys })
    }

    fn key_view(&self, profile: &Profile, page: &Page, key: &Key, pos: [u8; 4]) -> KeyView {
        let folder_size = key.target.as_deref().and_then(|t| profile.pages.iter().find(|p| p.id == t)).map(|p| p.keys.len());
        let mut v = view::base_view(key, pos, folder_size);
        let get = |n: &str| self.var(n);
        let path = key_path(&profile.id, &page.id, &key.id);
        // The steps the key runs next, whose plugins may draw on it.
        let mut shown = StepList::Main;
        if key.kind == KeyKind::Multi && !key.states.is_empty() {
            let i = self.key_state(&path, key);
            let st = &key.states[i];
            shown = StepList::State(i);
            v.state = Some(i);
            v.label = st.label.clone();
            v.sub = st.sub.clone();
            v.icon = st.icon.clone().or_else(|| key.icon.clone());
            v.color = st.color.or(key.color);
            v.meta = Some(format!("{}/{}", i + 1, key.states.len()));
        }
        if key.kind == KeyKind::Toggle && self.toggle_state(&path) {
            v.badge = Some("ON".into());
            v.tone = Some(Tone::Success);
        }
        let missing = key.all_actions().filter_map(Action::plugin).find(|(id, _)| !self.installed.contains(*id));
        if missing.is_some() && !v.disabled {
            v.badge = Some("MISSING".into());
            v.tone = Some(Tone::Muted);
            v.disabled = true;
        }
        for (n, _) in key.steps(shown).iter().enumerate() {
            if let Some(p) = self.overrides.get(&instance_id(&profile.id, &page.id, &key.id, shown, n)) {
                view::apply_patch(&mut v, p);
            }
        }
        if let Some(p) = self.key_overrides.get(&path) {
            view::apply_patch(&mut v, p);
        }
        for id in &key.styles {
            if let Some(st) = self.layout.styles.iter().find(|s| s.id == *id)
                && !st.when.var.is_empty()
                && view::eval(&st.when, get)
            {
                view::apply_style(&mut v, &st.style);
            }
        }
        for f in &key.feedback {
            if view::eval(&f.when, get) {
                view::apply_style(&mut v, &f.style);
            }
        }
        if let Some(w) = &key.widget {
            if let Some(feed) = &w.feed {
                v.lines = Some(self.feeds.get(feed).map(|q| q.iter().skip(q.len().saturating_sub(FEED_SHOWN)).cloned().collect()).unwrap_or_default());
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
    fn status_pills_have_unique_ids() {
        let (_t, mut e) = engine();
        let pill = |label: &str| Pill { id: "obs".into(), label: label.into(), tone: Tone::Danger, since: None };
        e.set_pills("obs", vec![pill("REC"), pill("LIVE")]);
        let (_, pills) = e.status();
        assert_eq!(pills.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), ["obs#0", "obs#1"]);
    }

    fn with_keys(e: &mut Engine, keys: Value, styles: Value) {
        let l: LayoutFile = serde_json::from_value(serde_json::json!({
            "version": 1, "defaultProfile": "main", "styles": styles,
            "profiles": [{ "id": "main", "name": "Main", "homePage": "p1", "pages": [{ "id": "p1", "name": "One", "cols": 3, "rows": 1, "keys": keys }] }]
        }))
        .unwrap();
        model::validate(&l).unwrap();
        e.replace_layout(l).unwrap();
    }

    fn view(e: &Engine) -> KeyView {
        e.page_view("main", "p1", FormFactor::PhonePortrait, None).unwrap().keys.remove(0)
    }

    #[test]
    fn multi_state_keys_follow_a_variable_and_keep_their_state() {
        let t = tempfile::tempdir().unwrap();
        let paths = Paths { root: t.path().to_path_buf() };
        let (mut e, _) = Engine::load(paths.clone()).unwrap();
        let keys = serde_json::json!([{ "id": "m", "x": 0, "y": 0, "kind": "multi", "label": "Scene", "actions": [], "follow": "obs.scene", "states": [
            { "label": "Gameplay", "color": "green", "value": "Gameplay", "actions": [] },
            { "label": "Chatting", "icon": "message-circle", "value": "Chatting", "actions": [] },
            { "label": "Other", "actions": [] }
        ]}, { "id": "t", "x": 1, "y": 0, "kind": "toggle", "label": "T", "actions": [] }]);
        with_keys(&mut e, keys, serde_json::json!([]));
        let key = e.key("main", "p1", "m").cloned().unwrap();
        assert_eq!((view(&e).label, view(&e).state), ("Gameplay".into(), Some(0)));
        assert_eq!(view(&e).color, Some(model::KeyColor::Green));
        // Presses move the state along while the variable matches none of them.
        assert_eq!(e.advance_state("main/p1/m", &key), 0);
        assert_eq!(view(&e).label, "Chatting");
        // Changed elsewhere: the key shows what the variable says.
        e.set_var("obs.scene", Value::from("Gameplay"));
        assert_eq!(view(&e).label, "Gameplay");
        e.set_var("obs.scene", Value::from("Chatting"));
        assert_eq!((view(&e).label, view(&e).icon), ("Chatting".into(), Some("message-circle".into())));
        e.set_var("obs.scene", Value::from("Intro"));
        assert_eq!(e.advance_state("main/p1/m", &key), 1);
        assert!(e.flip_toggle("main/p1/t"));

        // Saved, and back after a restart.
        let (path, states) = e.take_unsaved_states().unwrap();
        store::write_json(&path, &states).unwrap();
        assert!(e.take_unsaved_states().is_none());
        let layout = e.layout().clone();
        let (mut again, _) = Engine::load(paths).unwrap();
        again.replace_layout(layout).unwrap();
        assert_eq!(view(&again).label, "Other");
        assert!(again.toggle_state("main/p1/t"));
    }

    #[test]
    fn styles_apply_before_the_keys_own_feedback() {
        let (_t, mut e) = engine();
        let styles = serde_json::json!([
            { "id": "live", "name": "Live", "when": { "var": "user.live", "op": "truthy" }, "style": { "color": "red", "badge": "LIVE", "icon": "radio" } },
            { "id": "busy", "name": "Busy", "when": { "var": "user.n", "op": "gt", "value": 3 }, "style": { "label": "Busy" } }
        ]);
        let keys = serde_json::json!([{ "id": "a", "x": 0, "y": 0, "kind": "button", "label": "A", "actions": [], "styles": ["live", "busy"],
            "feedback": [{ "when": { "var": "user.live", "op": "truthy" }, "style": { "badge": "ON AIR" } }] }]);
        with_keys(&mut e, keys, styles);
        assert_eq!((view(&e).badge, view(&e).color), (None, None));
        e.set_var("user.live", Value::from(true));
        let v = view(&e);
        assert_eq!((v.badge.as_deref(), v.color, v.icon.as_deref(), v.label.as_str()), (Some("ON AIR"), Some(model::KeyColor::Red), Some("radio"), "A"));
        e.set_var("user.n", Value::from(5));
        assert_eq!(view(&e).label, "Busy");
    }

    #[test]
    fn variables_reject_bad_names_and_objects() {
        let (_t, mut e) = engine();
        assert!(e.set_var("user.goal", Value::from(5)));
        assert!(!e.set_var("bad name", Value::from(5)));
        assert!(!e.set_var("user.obj", serde_json::json!({"a": 1})));
    }

    #[test]
    fn feed_widgets_show_the_latest_lines_and_drop_removed_ones() {
        let (_t, mut e) = engine();
        let keys = serde_json::json!([{ "id": "c", "x": 0, "y": 0, "kind": "widget", "label": "Chat", "actions": [], "widget": { "feed": "twitch.chat", "lines": 6 } }]);
        with_keys(&mut e, keys, serde_json::json!([]));
        let line = |n: usize, author: &str| FeedItem { text: format!("m{n}"), author: Some(author.into()), id: Some(format!("id{n}")), ..Default::default() };
        for n in 0..30 {
            e.push_feed("twitch.chat", line(n, if n % 2 == 0 { "moss" } else { "fern" }));
        }
        let texts = |e: &Engine| view(e).lines.unwrap().into_iter().map(|i| i.text).collect::<Vec<_>>();
        // The lines setting no longer limits what is sent.
        assert_eq!(texts(&e).len(), FEED_SHOWN);
        assert_eq!(texts(&e).last().map(String::as_str), Some("m29"));
        // Ids stay on the PC.
        assert!(serde_json::to_value(&view(&e).lines.unwrap()[0]).unwrap().get("id").is_none());

        let mut rx = e.subscribe();
        e.remove_feed("twitch.chat", &FeedMatch { ids: vec!["id29".into(), "nope".into()], ..Default::default() });
        assert_eq!(texts(&e).last().map(String::as_str), Some("m28"));
        assert!(matches!(rx.try_recv(), Ok(Change::Keys)));
        e.remove_feed("twitch.chat", &FeedMatch { author: Some("moss".into()), ..Default::default() });
        assert!(texts(&e).iter().all(|t| t[1..].parse::<usize>().unwrap() % 2 == 1));
        assert!(matches!(rx.try_recv(), Ok(Change::Keys)));
        // Nothing matched: no update.
        e.remove_feed("twitch.chat", &FeedMatch { ids: vec!["id29".into()], ..Default::default() });
        e.remove_feed("other.chat", &FeedMatch { all: true, ..Default::default() });
        assert!(rx.try_recv().is_err());
        e.remove_feed("twitch.chat", &FeedMatch { all: true, ..Default::default() });
        assert!(texts(&e).is_empty());
    }
}

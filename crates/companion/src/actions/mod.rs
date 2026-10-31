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

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;

use crate::app::App;
use crate::model::{Action, Key, KeyKind, StepList, Tone};
use crate::plugins::Step;
use crate::state::{Change, instance_id, key_path};

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

/// Built-ins that type on the keyboard, which can do anything a person at the PC can. Added
/// from another device, they wait for review on the PC like imported steps do.
pub const PC_REVIEW: &[&str] = &["core.keys", "core.text"];

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

/// Steps a remote editor can't run or approve on its own.
pub fn needs_pc(a: &Action) -> bool {
    is_admin_only(a) || PC_REVIEW.contains(&a.kind.as_str())
}

/// Runs a list of steps in order. Back-to-back steps for the same plugin go out together,
/// so plugins like OBS can batch them.
pub async fn run_steps(app: Arc<App>, ctx: Ctx, actions: Vec<Action>, list: StepList) {
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
        if flow {
            ran.push(i);
            app.ran.lock().insert(key_path(&ctx.profile, &ctx.page, &ctx.key), ran.clone());
        }
        match a.plugin() {
            Some((plugin, _)) => {
                let plugin = plugin.to_string();
                let mut steps = Vec::new();
                while i < actions.len() && actions[i].plugin().is_some_and(|(p, _)| p == plugin) && !actions[i].review {
                    let act = &actions[i];
                    if flow && ran.last() != Some(&i) {
                        ran.push(i);
                    }
                    steps.push(Step {
                        instance: instance_id(&ctx.profile, &ctx.page, &ctx.key, list, i),
                        action: act.plugin().map(|(_, id)| id.to_string()).unwrap_or_default(),
                        settings: act.settings.clone(),
                    });
                    i += 1;
                }
                if flow {
                    app.ran.lock().insert(key_path(&ctx.profile, &ctx.page, &ctx.key), ran.clone());
                }
                if let Err(e) = app.plugins.run(&plugin, steps, "press", &ctx.device).await {
                    app.engine.lock().failed(key_path(&ctx.profile, &ctx.page, &ctx.key), e);
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
                        app.engine.lock().failed(key_path(&ctx.profile, &ctx.page, &ctx.key), e);
                        return;
                    }
                }
            }
        }
    }
}

/// Runs a key's steps, then again every `repeat` ms for as long as the key is held.
pub async fn run_key(app: Arc<App>, ctx: Ctx, actions: Vec<Action>, list: StepList, repeat: Option<u32>) {
    let path = key_path(&ctx.profile, &ctx.page, &ctx.key);
    loop {
        run_steps(app.clone(), ctx.clone(), actions.clone(), list).await;
        let Some(ms) = repeat.filter(|ms| *ms >= 100) else { break };
        if !app.held.lock().contains(&path) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(ms as u64)).await;
        if !app.held.lock().contains(&path) {
            break;
        }
    }
}

/// Sends slider values for `path` one at a time. A value that arrives while one is on its
/// way waits, and a newer one replaces it, so only the latest goes out next.
pub fn slide_one_at_a_time<F, Fut>(app: &Arc<App>, path: String, value: f64, send: F)
where
    F: Fn(Arc<App>, f64) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send,
{
    {
        let mut busy = app.slides.lock();
        if let Some(next) = busy.get_mut(&path) {
            *next = Some(value);
            return;
        }
        busy.insert(path.clone(), None);
    }
    let app = app.clone();
    tokio::spawn(async move {
        let mut value = value;
        loop {
            send(app.clone(), value).await;
            let next = {
                let mut busy = app.slides.lock();
                let next = busy.get_mut(&path).and_then(Option::take);
                if next.is_none() {
                    busy.remove(&path);
                }
                next
            };
            match next {
                Some(v) => value = v,
                None => break,
            }
        }
    });
}

/// Marks a key as running until dropped, so a second press waits for the first to finish.
/// Dropping (even by a panic in a step) frees the key again.
pub struct Running {
    app: Arc<App>,
    path: String,
}

impl Running {
    /// None when the key is already running.
    pub fn claim(app: &Arc<App>, path: String) -> Option<Self> {
        app.running.lock().insert(path.clone()).then(|| Self { app: app.clone(), path })
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.app.running.lock().remove(&self.path);
    }
}

/// How a key was pressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gesture {
    Press,
    Long,
    Double,
}

/// Which steps a gesture on a key runs. Moves toggles and multi-state keys on to their next
/// state, so call it once per press.
pub fn pick(app: &App, ctx: &Ctx, key: &Key, gesture: Gesture) -> (StepList, Vec<Action>) {
    let path = key_path(&ctx.profile, &ctx.page, &ctx.key);
    match (gesture, key.kind) {
        (Gesture::Long, _) => (StepList::Long, key.long_actions.clone()),
        (Gesture::Double, _) => (StepList::Double, key.double_actions.clone()),
        (Gesture::Press, KeyKind::Toggle) => {
            let on = app.engine.lock().flip_toggle(&path);
            if on || key.off_actions.is_empty() { (StepList::Main, key.actions.clone()) } else { (StepList::Off, key.off_actions.clone()) }
        }
        (Gesture::Press, KeyKind::Multi) if !key.states.is_empty() => {
            let i = app.engine.lock().advance_state(&path, key);
            (StepList::State(i), key.states[i].actions.clone())
        }
        (Gesture::Press, _) => (StepList::Main, key.actions.clone()),
    }
}

/// Steps that started, kept for the release.
type Fired = (Ctx, StepList, Vec<Action>);

/// Runs what a gesture on a key does. `who` names the device for the activity log. When the
/// key is already up, its release goes out as soon as the steps finish; otherwise the steps
/// are returned so the release can follow later.
pub fn fire(app: &Arc<App>, ctx: &Ctx, who: &str, gesture: Gesture, up: bool) -> Option<Fired> {
    let key = app.engine.lock().key(&ctx.profile, &ctx.page, &ctx.key).cloned()?;
    let (list, steps) = pick(app, ctx, &key, gesture);
    app.recorder.record(&steps);
    app.notify(Change::Pressed { profile: ctx.profile.clone(), page: ctx.page.clone(), key: ctx.key.clone(), label: key.label.clone(), device: who.to_string() });
    if steps.is_empty() {
        return None;
    }
    let running = Running::claim(app, key_path(&ctx.profile, &ctx.page, &ctx.key))?;
    let repeat = if gesture == Gesture::Press { key.repeat } else { None };
    let (app, run_ctx, run_steps) = (app.clone(), ctx.clone(), steps.clone());
    tokio::spawn(async move {
        run_key(app.clone(), run_ctx.clone(), run_steps.clone(), list, repeat).await;
        if up {
            release(app, run_ctx, list, run_steps).await;
        }
        drop(running);
    });
    (!up).then(|| (ctx.clone(), list, steps))
}

/// What happens once a timer started by `Presses` runs out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Timer {
    /// Hold to confirm: the key was held long enough.
    Hold,
    /// Held past the long press time.
    Long,
}

/// A key that is down right now. Kept so the release matches the press even if the page
/// changed meanwhile.
struct Press {
    ctx: Ctx,
    /// Entry in `App::held` while a repeating key is down.
    held: Option<String>,
    /// Steps that started, for the release.
    fired: Option<Fired>,
    /// Timer running while the key is down, by generation.
    timer: Option<(u64, Timer)>,
    /// Double tap window when the key has double tap steps.
    double: Option<u32>,
    /// Page changed while a timer ran: the release does nothing.
    dropped: bool,
}

/// A key tapped once, waiting to see whether a second tap follows.
struct Tapped {
    ctx: Ctx,
    generation: u64,
}

/// Presses in flight on one device, by whatever that device calls a key (an id on phones, a
/// button number on Stream Decks). Handles hold to confirm, long press and double tap. Timers
/// come back through the receiver from `new`, to be handed to `timer`.
pub struct Presses<K: Hash + Eq> {
    who: String,
    down: HashMap<K, Press>,
    tapped: HashMap<K, Tapped>,
    timers: mpsc::UnboundedSender<(K, u64)>,
    generation: u64,
}

impl<K: Hash + Eq + Clone + Send + 'static> Presses<K> {
    /// `who` names the device in the activity log.
    pub fn new(who: impl Into<String>) -> (Self, mpsc::UnboundedReceiver<(K, u64)>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Self { who: who.into(), down: HashMap::new(), tapped: HashMap::new(), timers: tx, generation: 0 }, rx)
    }

    pub fn is_down(&self, k: &K) -> bool {
        self.down.contains_key(k)
    }

    fn start(&mut self, k: &K, ms: u32) -> u64 {
        self.generation += 1;
        let (tx, k, generation) = (self.timers.clone(), k.clone(), self.generation);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(ms as u64)).await;
            let _ = tx.send((k, generation));
        });
        generation
    }

    /// A key went down. Plain keys fire right away; the others wait for their timer or the
    /// release. A second press of a key that is already down is ignored.
    pub fn down(&mut self, app: &Arc<App>, k: K, ctx: Ctx, key: &Key) {
        if self.down.contains_key(&k) {
            return;
        }
        let path = key_path(&ctx.profile, &ctx.page, &ctx.key);
        let (long, double) = key.gestures();
        if let Some(t) = self.tapped.remove(&k) {
            if t.ctx.profile == ctx.profile && t.ctx.page == ctx.page && t.ctx.key == ctx.key && double.is_some() {
                let fired = fire(app, &ctx, &self.who, Gesture::Double, false);
                self.down.insert(k, Press { ctx, held: None, fired, timer: None, double: None, dropped: false });
                return;
            }
            // The button shows another key by now: the first tap was a plain press after all.
            fire(app, &t.ctx, &self.who, Gesture::Press, true);
        }
        let held = key.repeat.is_some().then(|| {
            app.held.lock().insert(path.clone());
            path
        });
        let timer = match (key.hold.filter(|h| *h > 0), long) {
            (Some(ms), _) => Some((self.start(&k, ms), Timer::Hold)),
            (None, Some(ms)) => Some((self.start(&k, ms), Timer::Long)),
            (None, None) => None,
        };
        let fired = if timer.is_none() && double.is_none() { fire(app, &ctx, &self.who, Gesture::Press, false) } else { None };
        self.down.insert(k, Press { ctx, held, fired, timer, double, dropped: false });
    }

    /// The key came back up, or the press was cancelled (the finger slid off, the app lost
    /// focus). True when a hold to confirm key was let go too early.
    pub fn up(&mut self, app: &Arc<App>, k: &K, cancel: bool) -> bool {
        let Some(p) = self.down.remove(k) else { return false };
        if let Some(path) = &p.held {
            app.held.lock().remove(path);
        }
        if let Some((ctx, list, steps)) = p.fired {
            tokio::spawn(release(app.clone(), ctx, list, steps));
            return false;
        }
        if p.dropped || cancel {
            return false;
        }
        match (p.timer, p.double) {
            (Some((_, Timer::Hold)), _) => true,
            // Let go before a long press: a tap.
            (_, Some(ms)) => {
                let generation = self.start(k, ms);
                self.tapped.insert(k.clone(), Tapped { ctx: p.ctx, generation });
                false
            }
            (_, None) => {
                fire(app, &p.ctx, &self.who, Gesture::Press, true);
                false
            }
        }
    }

    /// A timer from `new` ran out.
    pub fn timer(&mut self, app: &Arc<App>, k: K, generation: u64) {
        if let Some(p) = self.down.get_mut(&k)
            && let Some((g, what)) = p.timer
            && g == generation
        {
            p.timer = None;
            let gesture = if what == Timer::Hold { Gesture::Press } else { Gesture::Long };
            p.fired = fire(app, &p.ctx, &self.who, gesture, false);
            return;
        }
        if self.tapped.get(&k).is_some_and(|t| t.generation == generation)
            && let Some(t) = self.tapped.remove(&k)
        {
            fire(app, &t.ctx, &self.who, Gesture::Press, true);
        }
    }

    /// The device moved to another page: keys waiting to be held long enough don't fire.
    pub fn page_changed(&mut self) {
        for p in self.down.values_mut() {
            if p.timer.take().is_some() || (p.fired.is_none() && p.double.is_some()) {
                p.dropped = true;
            }
        }
    }

    /// The device went away: releases what's down. Taps waiting for a second one are dropped.
    pub fn release_all(&mut self, app: &Arc<App>) {
        self.tapped.clear();
        let keys: Vec<K> = self.down.keys().cloned().collect();
        for k in keys {
            self.up(app, &k, true);
        }
    }
}

/// Key released after it fired. Only plugins get release events (push-to-talk and similar),
/// and with if/otherwise steps, only the steps that ran.
pub async fn release(app: Arc<App>, ctx: Ctx, list: StepList, actions: Vec<Action>) {
    let ran = if actions.iter().any(is_flow) { Some(app.ran.lock().remove(&key_path(&ctx.profile, &ctx.page, &ctx.key)).unwrap_or_default()) } else { None };
    let mut by_plugin: Vec<(String, Vec<Step>)> = Vec::new();
    for (i, a) in actions.iter().enumerate() {
        let Some((plugin, id)) = a.plugin() else { continue };
        if a.review || ran.as_ref().is_some_and(|r| !r.contains(&i)) {
            continue;
        }
        let step = Step { instance: instance_id(&ctx.profile, &ctx.page, &ctx.key, list, i), action: id.to_string(), settings: a.settings.clone() };
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

        run_steps(app.clone(), ctx.clone(), steps.clone(), StepList::Main).await;
        assert_eq!((get("user.said"), get("user.inner"), get("user.after")), ("offline".into(), "".into(), "yes".into()));

        app.engine.lock().set_var("user.live", json!(true));
        app.engine.lock().set_var("user.mode", json!("chill"));
        run_steps(app.clone(), ctx.clone(), steps.clone(), StepList::Main).await;
        assert_eq!((get("user.said"), get("user.inner")), ("live".into(), "chill".into()));

        app.engine.lock().set_var("user.mode", json!("hype"));
        run_steps(app.clone(), ctx, steps, StepList::Main).await;
        assert_eq!(get("user.inner"), "other");
        // Only the steps that ran are remembered for the release.
        assert_eq!(app.ran.lock().get("p/g/k").cloned(), Some(vec![1, 5, 10]));
    }

    #[tokio::test]
    async fn a_burst_of_slider_values_sends_the_first_and_the_last() {
        let dir = tempfile::tempdir().unwrap();
        let app = App::new(crate::app::Options { data_dir: Some(dir.path().into()), bundled_plugins: None }).unwrap();
        let sent = Arc::new(parking_lot::Mutex::new(Vec::new()));
        let send = {
            let sent = sent.clone();
            move |_: Arc<App>, v: f64| {
                let sent = sent.clone();
                async move {
                    sent.lock().push(v);
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                }
            }
        };
        for i in 1..=20 {
            slide_one_at_a_time(&app, "p/g/k".into(), i as f64 / 20.0, send.clone());
        }
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        assert_eq!(*sent.lock(), vec![0.05, 1.0]);
        assert!(app.slides.lock().is_empty(), "nothing left waiting");
    }

    fn inc(name: &str) -> Action {
        step("core.var", json!({ "name": name, "op": "inc", "value": 1 }))
    }

    struct Deck {
        _dir: tempfile::TempDir,
        app: Arc<App>,
        presses: Presses<String>,
        timers: mpsc::UnboundedReceiver<(String, u64)>,
    }

    impl Deck {
        fn new(keys: serde_json::Value) -> Self {
            let dir = tempfile::tempdir().unwrap();
            let app = App::new(crate::app::Options { data_dir: Some(dir.path().into()), bundled_plugins: None }).unwrap();
            let layout = serde_json::from_value(json!({
                "version": 1, "defaultProfile": "main",
                "profiles": [{ "id": "main", "name": "Main", "homePage": "p1", "pages": [{ "id": "p1", "name": "One", "cols": 4, "rows": 2, "keys": keys }] }]
            }))
            .unwrap();
            crate::model::validate(&layout).unwrap();
            app.engine.lock().replace_layout(layout).unwrap();
            let (presses, timers) = Presses::new("Test deck");
            Self { _dir: dir, app, presses, timers }
        }

        fn key(&self, id: &str) -> Key {
            self.app.engine.lock().key("main", "p1", id).cloned().unwrap()
        }

        fn down(&mut self, id: &str) {
            let ctx = Ctx { device: "d".into(), profile: "main".into(), page: "p1".into(), key: id.into(), local: true };
            let key = self.key(id);
            self.presses.down(&self.app, id.to_string(), ctx, &key);
        }

        fn up(&mut self, id: &str) -> bool {
            self.presses.up(&self.app, &id.to_string(), false)
        }

        /// Lets `ms` pass, handing timers that ran out back to the tracker, and lets the
        /// steps they started finish.
        async fn wait(&mut self, ms: u64) {
            for _ in 0..ms {
                tokio::time::advance(Duration::from_millis(1)).await;
                for _ in 0..4 {
                    tokio::task::yield_now().await;
                }
                while let Ok((k, g)) = self.timers.try_recv() {
                    self.presses.timer(&self.app, k, g);
                }
            }
            for _ in 0..20 {
                tokio::task::yield_now().await;
            }
        }

        fn count(&self, name: &str) -> i64 {
            self.app.engine.lock().var(name).and_then(|v| v.as_f64()).unwrap_or(0.0) as i64
        }

        fn counts(&self) -> (i64, i64, i64) {
            (self.count("user.press"), self.count("user.long"), self.count("user.double"))
        }
    }

    fn gesture_key(id: &str, x: u8, extra: serde_json::Value) -> serde_json::Value {
        let mut k = json!({ "id": id, "x": x, "y": 0, "kind": "button", "label": id, "actions": [inc("user.press")] });
        for (f, v) in extra.as_object().unwrap() {
            k[f] = v.clone();
        }
        k
    }

    #[tokio::test(start_paused = true)]
    async fn plain_keys_fire_on_the_way_down() {
        let mut d = Deck::new(json!([gesture_key("a", 0, json!({}))]));
        d.down("a");
        d.wait(1).await;
        assert_eq!(d.counts(), (1, 0, 0));
        d.up("a");
        d.wait(1000).await;
        assert_eq!(d.counts(), (1, 0, 0));
    }

    #[tokio::test(start_paused = true)]
    async fn long_press_and_tap() {
        let mut d = Deck::new(json!([gesture_key("a", 0, json!({ "longActions": [inc("user.long")] }))]));
        // A tap fires the normal steps on release, not before.
        d.down("a");
        d.wait(100).await;
        assert_eq!(d.counts(), (0, 0, 0));
        d.up("a");
        d.wait(1).await;
        assert_eq!(d.counts(), (1, 0, 0));
        // Held past 500 ms: the long press steps run while still held, and nothing on release.
        d.down("a");
        d.wait(499).await;
        assert_eq!(d.counts(), (1, 0, 0));
        d.wait(2).await;
        assert_eq!(d.counts(), (1, 1, 0));
        d.up("a");
        d.wait(1000).await;
        assert_eq!(d.counts(), (1, 1, 0));
    }

    #[tokio::test(start_paused = true)]
    async fn custom_long_press_time() {
        let mut d = Deck::new(json!([gesture_key("a", 0, json!({ "longActions": [inc("user.long")], "longMs": 1200 }))]));
        d.down("a");
        d.wait(800).await;
        d.up("a");
        d.wait(1).await;
        assert_eq!(d.counts(), (1, 0, 0));
        d.down("a");
        d.wait(1201).await;
        assert_eq!(d.counts(), (1, 1, 0));
    }

    #[tokio::test(start_paused = true)]
    async fn double_tap_and_single_tap() {
        let mut d = Deck::new(json!([gesture_key("a", 0, json!({ "doubleActions": [inc("user.double")] }))]));
        // One tap: the normal steps run once the window has passed.
        d.down("a");
        d.wait(50).await;
        d.up("a");
        d.wait(299).await;
        assert_eq!(d.counts(), (0, 0, 0));
        d.wait(2).await;
        assert_eq!(d.counts(), (1, 0, 0));
        // Two taps within 300 ms: only the double tap steps, and right on the second tap.
        d.down("a");
        d.wait(50).await;
        d.up("a");
        d.wait(200).await;
        d.down("a");
        d.wait(1).await;
        assert_eq!(d.counts(), (1, 0, 1));
        d.up("a");
        d.wait(1000).await;
        assert_eq!(d.counts(), (1, 0, 1));
        // Taps further apart than the window are two presses.
        for _ in 0..2 {
            d.down("a");
            d.wait(30).await;
            d.up("a");
            d.wait(400).await;
        }
        assert_eq!(d.counts(), (3, 0, 1));
    }

    #[tokio::test(start_paused = true)]
    async fn long_press_and_double_tap_together() {
        let mut d = Deck::new(json!([gesture_key("a", 0, json!({ "longActions": [inc("user.long")], "doubleActions": [inc("user.double")], "doubleMs": 200 }))]));
        d.down("a");
        d.wait(600).await;
        d.up("a");
        d.wait(500).await;
        assert_eq!(d.counts(), (0, 1, 0));
        d.down("a");
        d.wait(40).await;
        d.up("a");
        d.wait(100).await;
        d.down("a");
        d.wait(40).await;
        d.up("a");
        d.wait(500).await;
        assert_eq!(d.counts(), (0, 1, 1));
        d.down("a");
        d.wait(40).await;
        d.up("a");
        d.wait(201).await;
        assert_eq!(d.counts(), (1, 1, 1));
    }

    #[tokio::test(start_paused = true)]
    async fn hold_to_confirm_and_repeat_work_as_before() {
        let mut d = Deck::new(json!([gesture_key("h", 0, json!({ "hold": 800 })), gesture_key("r", 1, json!({ "repeat": 200, "actions": [inc("user.r")] }))]));
        d.down("h");
        d.wait(500).await;
        assert!(d.up("h"), "let go too early");
        d.wait(1000).await;
        assert_eq!(d.count("user.press"), 0);
        d.down("h");
        d.wait(801).await;
        assert_eq!(d.count("user.press"), 1);
        assert!(!d.up("h"));

        d.down("r");
        d.wait(1).await;
        assert_eq!(d.count("user.r"), 1);
        d.wait(450).await;
        assert_eq!(d.count("user.r"), 3);
        d.up("r");
        d.wait(1000).await;
        assert_eq!(d.count("user.r"), 3);
    }

    #[tokio::test(start_paused = true)]
    async fn multi_state_keys_step_through_their_states() {
        let state = |label: &str, var: &str| json!({ "label": label, "actions": [inc(var)] });
        let mut d = Deck::new(json!([{ "id": "m", "x": 0, "y": 0, "kind": "multi", "label": "M", "actions": [],
            "states": [state("One", "user.one"), state("Two", "user.two"), state("Three", "user.three")],
            "doubleActions": [inc("user.double")] }]));
        let shown = |d: &Deck| d.app.engine.lock().page_view("main", "p1", crate::model::FormFactor::PhonePortrait, None).unwrap().keys[0].label.clone();
        assert_eq!(shown(&d), "One");
        for _ in 0..4 {
            d.down("m");
            d.wait(10).await;
            d.up("m");
            d.wait(400).await;
        }
        assert_eq!((d.count("user.one"), d.count("user.two"), d.count("user.three")), (2, 1, 1));
        assert_eq!(shown(&d), "Two");
        // A double tap runs its own steps and leaves the state alone.
        d.down("m");
        d.up("m");
        d.wait(100).await;
        d.down("m");
        d.up("m");
        d.wait(400).await;
        assert_eq!((d.count("user.double"), d.count("user.two")), (1, 1));
        assert_eq!(shown(&d), "Two");
    }

    #[test]
    fn a_missing_end_closes_at_the_end() {
        let steps = vec![step(IF, json!({ "var": "x", "op": "truthy" })), set("user.a", "1")];
        assert_eq!(closing(&steps, 1, true), 2);
    }
}

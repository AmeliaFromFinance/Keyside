//! Chat triggers: a chat command, a keyword or a channel point redemption presses a key.
//! Also counts chat messages for {core.chat_rate}.

use std::collections::{HashMap, VecDeque};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde::Deserialize;
use serde_json::Value;

use crate::actions::{self, Ctx};
use crate::app::App;
use crate::model::{Audience, ChatSource, KeyKind, Trigger, TriggerKind};
use crate::state::view::FeedItem;
use crate::state::{Change, key_path};

/// Chat messages in the last minute.
pub const RATE_VAR: &str = "core.chat_rate";
const RATE_WINDOW: Duration = Duration::from_secs(60);
/// The same message arriving twice this quickly (from Twitch and from Streamer.bot, say)
/// counts once.
const SAME_WINDOW: Duration = Duration::from_secs(3);

/// Extra fields a plugin may send with a feed item, so triggers know what happened and who
/// did it. Plugins that send none still work: see `Event::from_item`.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Details {
    pub kind: Option<String>,
    pub source: Option<String>,
    pub roles: Vec<String>,
    pub reward: Option<String>,
    pub input: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventKind {
    Chat,
    Redeem,
    Other,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Roles {
    pub broadcaster: bool,
    pub moderator: bool,
    pub vip: bool,
    pub subscriber: bool,
}

impl Roles {
    pub fn parse<S: AsRef<str>>(list: &[S]) -> Self {
        let mut r = Self::default();
        for s in list.iter().take(16) {
            match s.as_ref().trim().to_ascii_lowercase().as_str() {
                "broadcaster" | "owner" | "streamer" => r.broadcaster = true,
                "mod" | "moderator" => r.moderator = true,
                "vip" => r.vip = true,
                "sub" | "subscriber" | "member" | "founder" => r.subscriber = true,
                _ => {}
            }
        }
        r
    }

    fn allows(&self, who: Audience) -> bool {
        match who {
            Audience::Everyone => true,
            Audience::Subs => self.subscriber || self.moderator || self.broadcaster,
            Audience::Mods => self.moderator || self.broadcaster,
            Audience::Broadcaster => self.broadcaster,
        }
    }
}

/// One chat message, redemption or alert, as triggers see it.
#[derive(Debug, Clone)]
pub struct Event {
    pub kind: EventKind,
    /// "twitch", "youtube", "kick" or the prefix of the plugin that sent it.
    pub source: String,
    pub author: String,
    pub text: String,
    pub roles: Roles,
    /// Title of a redeemed reward.
    pub reward: String,
    /// What the viewer typed with a redemption.
    pub input: String,
}

fn clean(s: &str, max: usize) -> String {
    s.chars().filter(|c| !c.is_control()).take(max).collect()
}

impl Event {
    /// Plugins from before kinds and roles existed still count: lines in a feed called
    /// "chat" are chat messages from someone without roles.
    pub fn from_item(prefix: &str, feed: &str, item: &FeedItem, d: &Details) -> Self {
        let kind = match d.kind.as_deref() {
            Some("chat") => EventKind::Chat,
            Some("redeem") => EventKind::Redeem,
            Some(_) => EventKind::Other,
            None if feed == "chat" => EventKind::Chat,
            None => EventKind::Other,
        };
        let source = d
            .source
            .as_deref()
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|s| !s.is_empty() && s.len() <= 16 && s.bytes().all(|b| b.is_ascii_alphanumeric()))
            .unwrap_or_else(|| prefix.to_string());
        Self {
            kind,
            source,
            author: item.author.clone().unwrap_or_default(),
            text: item.text.clone(),
            roles: Roles::parse(&d.roles),
            reward: clean(d.reward.as_deref().unwrap_or(""), 100),
            input: clean(d.input.as_deref().unwrap_or(""), 500),
        }
    }

    fn fingerprint(&self) -> u64 {
        let mut h = DefaultHasher::new();
        (self.kind, &self.source, self.author.to_lowercase(), self.text.trim(), &self.reward).hash(&mut h);
        h.finish()
    }
}

/// Whether a trigger fires for an event. Gives what the steps get as {core.trigger.input}:
/// the words after a command, the whole message for a keyword, the viewer text for a reward.
pub fn matches(t: &Trigger, ev: &Event) -> Option<String> {
    if t.disabled || !(t.source == ChatSource::Any || t.source.as_str() == ev.source) || !ev.roles.allows(t.who) {
        return None;
    }
    let want = t.pattern.trim();
    match (t.kind, ev.kind) {
        (TriggerKind::Command, EventKind::Chat) => {
            let text = ev.text.trim();
            let (first, rest) = text.split_once(char::is_whitespace).unwrap_or((text, ""));
            // "brb" means "!brb". Other prefixes are kept as typed.
            let command = if want.starts_with(char::is_alphanumeric) { format!("!{want}") } else { want.to_string() };
            (first.to_lowercase() == command.to_lowercase()).then(|| rest.trim().to_string())
        }
        (TriggerKind::Keyword, EventKind::Chat) => has_phrase(&ev.text, want).then(|| ev.text.clone()),
        (TriggerKind::Redeem, EventKind::Redeem) => (ev.reward.trim().to_lowercase() == want.to_lowercase()).then(|| ev.input.clone()),
        _ => None,
    }
}

/// Case-insensitive search that respects word edges, so "hi" doesn't match "this".
fn has_phrase(text: &str, phrase: &str) -> bool {
    let (text, phrase) = (text.to_lowercase(), phrase.to_lowercase());
    if phrase.is_empty() {
        return false;
    }
    let word_start = phrase.starts_with(char::is_alphanumeric);
    let word_end = phrase.ends_with(char::is_alphanumeric);
    let edge = |c: Option<char>| c.is_none_or(|c| !c.is_alphanumeric());
    let mut from = 0;
    while let Some(i) = text[from..].find(&phrase) {
        let start = from + i;
        let end = start + phrase.len();
        if (!word_start || edge(text[..start].chars().next_back())) && (!word_end || edge(text[end..].chars().next())) {
            return true;
        }
        from = start + text[start..].chars().next().map_or(1, char::len_utf8);
    }
    false
}

#[derive(Default)]
pub struct Chat {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    /// When each trigger may fire again.
    cooldowns: HashMap<String, Instant>,
    recent: VecDeque<(Instant, u64)>,
    messages: VecDeque<Instant>,
}

impl Chat {
    /// False when the same event came in a moment ago.
    fn first_sighting(&self, ev: &Event, now: Instant) -> bool {
        let fp = ev.fingerprint();
        let mut s = self.inner.lock();
        while s.recent.front().is_some_and(|(t, _)| now.duration_since(*t) > SAME_WINDOW) {
            s.recent.pop_front();
        }
        if s.recent.iter().any(|(_, f)| *f == fp) {
            return false;
        }
        if s.recent.len() < 1000 {
            s.recent.push_back((now, fp));
        }
        true
    }

    fn count_message(&self, now: Instant) {
        let mut s = self.inner.lock();
        s.messages.push_back(now);
        // Plenty for any real chat. Keeps memory flat during a spam wave.
        if s.messages.len() > 10_000 {
            s.messages.pop_front();
        }
    }

    /// Chat messages in the last minute.
    pub fn rate(&self, now: Instant) -> usize {
        let mut s = self.inner.lock();
        while s.messages.front().is_some_and(|t| now.duration_since(*t) > RATE_WINDOW) {
            s.messages.pop_front();
        }
        s.messages.len()
    }

    /// Starts the cooldown and returns true, unless the trigger is still cooling down.
    fn ready(&self, id: &str, cooldown: u32, now: Instant) -> bool {
        let mut s = self.inner.lock();
        if s.cooldowns.get(id).is_some_and(|until| *until > now) {
            return false;
        }
        s.cooldowns.retain(|_, until| *until > now);
        if cooldown > 0 {
            s.cooldowns.insert(id.to_string(), now + Duration::from_secs(cooldown as u64));
        }
        true
    }
}

fn publish_rate(app: &App, now: Instant) {
    let n = app.chat.rate(now);
    app.engine.lock().set_var(RATE_VAR, Value::from(n as u64));
}

/// Keeps {core.chat_rate} falling when chat goes quiet.
pub fn spawn_ticker(app: Arc<App>) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(5));
        loop {
            tick.tick().await;
            publish_rate(&app, Instant::now());
        }
    });
}

/// Called for every feed item a plugin sends.
pub fn handle(app: &Arc<App>, ev: Event) {
    let now = Instant::now();
    if ev.kind == EventKind::Other || !app.chat.first_sighting(&ev, now) {
        return;
    }
    if ev.kind == EventKind::Chat {
        app.chat.count_message(now);
        publish_rate(app, now);
    }
    let hits: Vec<(Trigger, String)> = app.engine.lock().layout().triggers.iter().filter_map(|t| matches(t, &ev).map(|input| (t.clone(), input))).collect();
    for (t, input) in hits {
        if app.chat.ready(&t.id, t.cooldown, now) {
            fire(app, &t, &ev, input);
        }
    }
}

/// Presses the target key the way the inbound API does: toggles flip, steps run once.
fn fire(app: &Arc<App>, t: &Trigger, ev: &Event, input: String) {
    let tg = &t.target;
    let path = key_path(&tg.profile, &tg.page, &tg.key);
    let Some(key) = app.engine.lock().key(&tg.profile, &tg.page, &tg.key).cloned() else { return };
    if key.actions.is_empty() && key.off_actions.is_empty() {
        return;
    }
    let Some(running) = actions::Running::claim(app, path.clone()) else { return };
    let (list, off) = {
        let mut e = app.engine.lock();
        e.set_var("core.trigger.user", Value::String(ev.author.clone()));
        e.set_var("core.trigger.input", Value::String(input));
        e.set_var("core.trigger.source", Value::String(ev.source.clone()));
        let picked = if key.kind == KeyKind::Toggle {
            let on = e.flip_toggle(&path);
            if on || key.off_actions.is_empty() { (key.actions.clone(), false) } else { (key.off_actions.clone(), true) }
        } else {
            (key.actions.clone(), false)
        };
        e.notify(Change::Pressed { profile: tg.profile.clone(), page: tg.page.clone(), key: tg.key.clone(), label: key.label.clone(), device: format!("Chat: {}", ev.author) });
        picked
    };
    let ctx = Ctx { device: format!("trigger:{}", t.id), profile: tg.profile.clone(), page: tg.page.clone(), key: tg.key.clone(), local: false };
    let app = app.clone();
    tokio::spawn(async move {
        actions::run_key(app, ctx, list, off, None).await;
        drop(running);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn trigger(kind: &str, pattern: &str, extra: Value) -> Trigger {
        let mut v = json!({ "id": "t1", "kind": kind, "match": pattern, "target": { "profile": "main", "page": "p1", "key": "k1" } });
        for (k, x) in extra.as_object().unwrap() {
            v[k] = x.clone();
        }
        serde_json::from_value(v).unwrap()
    }

    fn chat(text: &str, roles: &[&str]) -> Event {
        Event { kind: EventKind::Chat, source: "twitch".into(), author: "fern".into(), text: text.into(), roles: Roles::parse(roles), reward: String::new(), input: String::new() }
    }

    #[test]
    fn commands_match_the_first_word() {
        let t = trigger("command", "brb", json!({}));
        assert_eq!(matches(&t, &chat("!brb", &[])).as_deref(), Some(""));
        assert_eq!(matches(&t, &chat("  !BRB back in 5 ", &[])).as_deref(), Some("back in 5"));
        assert_eq!(matches(&t, &chat("brb", &[])), None, "needs the !");
        assert_eq!(matches(&t, &chat("!brbx", &[])), None);
        assert_eq!(matches(&t, &chat("going !brb", &[])), None);
        let other = trigger("command", "?scene", json!({}));
        assert!(matches(&other, &chat("?scene 2", &[])).is_some());
        assert!(matches(&other, &chat("!scene", &[])).is_none());
    }

    #[test]
    fn keywords_respect_word_edges() {
        let t = trigger("keyword", "hi", json!({}));
        assert!(matches(&t, &chat("Hi there", &[])).is_some());
        assert!(matches(&t, &chat("oh, hi!", &[])).is_some());
        assert!(matches(&t, &chat("this is fine", &[])).is_none());
        assert!(matches(&trigger("keyword", "good game", json!({})), &chat("that was a GOOD GAME gg", &[])).is_some());
        assert!(matches(&trigger("keyword", "café", json!({})), &chat("le Café est bon", &[])).is_some());
        assert!(matches(&trigger("keyword", "<3", json!({})), &chat("love it<3", &[])).is_some(), "symbols match anywhere");
    }

    #[test]
    fn redemptions_match_the_reward_title() {
        let t = trigger("redeem", "Hydrate", json!({}));
        let mut ev = chat("redeemed Hydrate", &[]);
        assert!(matches(&t, &ev).is_none(), "chat is not a redemption");
        ev.kind = EventKind::Redeem;
        ev.reward = " hydrate ".into();
        ev.input = "please".into();
        assert_eq!(matches(&t, &ev).as_deref(), Some("please"));
        ev.reward = "Hydrate now".into();
        assert!(matches(&t, &ev).is_none());
        assert!(matches(&trigger("command", "hydrate", json!({})), &Event { reward: "hydrate".into(), ..ev }).is_none());
    }

    #[test]
    fn roles_and_sources() {
        let subs = trigger("command", "!so", json!({ "who": "subs" }));
        assert!(matches(&subs, &chat("!so", &[])).is_none());
        assert!(matches(&subs, &chat("!so", &["vip"])).is_none());
        assert!(matches(&subs, &chat("!so", &["subscriber"])).is_some());
        assert!(matches(&subs, &chat("!so", &["moderator"])).is_some(), "mods count as subs");
        let mods = trigger("command", "!so", json!({ "who": "mods" }));
        assert!(matches(&mods, &chat("!so", &["sub"])).is_none());
        assert!(matches(&mods, &chat("!so", &["mod"])).is_some());
        assert!(matches(&mods, &chat("!so", &["broadcaster"])).is_some());
        let me = trigger("command", "!so", json!({ "who": "broadcaster" }));
        assert!(matches(&me, &chat("!so", &["mod"])).is_none());
        assert!(matches(&me, &chat("!so", &["owner"])).is_some());

        let yt = trigger("command", "!so", json!({ "source": "youtube" }));
        assert!(matches(&yt, &chat("!so", &[])).is_none());
        assert!(matches(&yt, &Event { source: "youtube".into(), ..chat("!so", &[]) }).is_some());
        assert!(matches(&trigger("command", "!so", json!({ "disabled": true })), &chat("!so", &[])).is_none());
    }

    #[test]
    fn older_plugins_count_as_chat_without_roles() {
        let item = FeedItem { text: "!brb".into(), author: Some("fern".into()), ..Default::default() };
        let ev = Event::from_item("twitch", "chat", &item, &Details::default());
        assert_eq!(ev.kind, EventKind::Chat);
        assert_eq!(ev.source, "twitch");
        assert_eq!(ev.roles, Roles::default());
        assert_eq!(Event::from_item("twitch", "events", &item, &Details::default()).kind, EventKind::Other);

        let d: Details = serde_json::from_value(json!({ "kind": "redeem", "source": "Kick", "roles": ["mod", "nonsense"], "reward": "Hydrate\u{7}", "input": "hi" })).unwrap();
        let ev = Event::from_item("sb", "events", &item, &d);
        assert_eq!((ev.kind, ev.source.as_str(), ev.reward.as_str()), (EventKind::Redeem, "kick", "Hydrate"));
        assert!(ev.roles.moderator && !ev.roles.subscriber);
        let bad: Details = serde_json::from_value(json!({ "source": "../etc" })).unwrap();
        assert_eq!(Event::from_item("sb", "chat", &item, &bad).source, "sb");
    }

    #[test]
    fn cooldowns_and_repeats() {
        let c = Chat::default();
        let now = Instant::now();
        assert!(c.ready("t1", 30, now));
        assert!(!c.ready("t1", 30, now + Duration::from_secs(29)));
        assert!(c.ready("t1", 30, now + Duration::from_secs(31)));
        assert!(c.ready("t2", 0, now));
        assert!(c.ready("t2", 0, now), "no cooldown");

        let ev = chat("!brb", &[]);
        assert!(c.first_sighting(&ev, now));
        assert!(!c.first_sighting(&ev, now + Duration::from_secs(1)), "same message from a second plugin");
        assert!(c.first_sighting(&Event { author: "moss".into(), ..ev.clone() }, now + Duration::from_secs(1)));
        assert!(c.first_sighting(&ev, now + Duration::from_secs(5)));
    }

    #[test]
    fn chat_rate_counts_the_last_minute() {
        let c = Chat::default();
        let now = Instant::now();
        for _ in 0..3 {
            c.count_message(now);
        }
        c.count_message(now + Duration::from_secs(30));
        assert_eq!(c.rate(now + Duration::from_secs(30)), 4);
        assert_eq!(c.rate(now + Duration::from_secs(61)), 1);
        assert_eq!(c.rate(now + Duration::from_secs(120)), 0);
    }

    #[tokio::test]
    async fn a_command_presses_the_key() {
        let dir = tempfile::tempdir().unwrap();
        let app = App::new(crate::app::Options { data_dir: Some(dir.path().into()), bundled_plugins: None }).unwrap();
        let mut layout = app.engine.lock().layout().clone();
        let (p, pg) = (layout.profiles[0].id.clone(), layout.profiles[0].pages[0].id.clone());
        let key: crate::model::Key = serde_json::from_value(json!({ "id": "pause", "x": 0, "y": 0, "kind": "toggle", "label": "BRB",
            "actions": [{ "type": "core.var", "settings": { "name": "user.brb", "op": "set", "value": "on" } }] }))
        .unwrap();
        layout.profiles[0].pages[0].keys.retain(|k| (k.x, k.y) != (0, 0) && k.id != "pause");
        layout.profiles[0].pages[0].keys.push(key);
        layout.triggers = vec![trigger("command", "!brb", json!({ "cooldown": 60, "target": { "profile": p, "page": pg, "key": "pause" } }))];
        app.engine.lock().replace_layout(layout).unwrap();

        let mut rx = app.engine.lock().subscribe();
        handle(&app, chat("!brb now", &[]));
        let pressed = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if let Ok(Change::Pressed { key, device, .. }) = rx.recv().await {
                    return (key, device);
                }
            }
        })
        .await
        .unwrap();
        assert_eq!(pressed, ("pause".to_string(), "Chat: fern".to_string()));
        for _ in 0..100 {
            if app.engine.lock().var("user.brb").is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let e = app.engine.lock();
        assert_eq!(e.var("user.brb"), Some(json!("on")));
        assert_eq!(e.var("core.trigger.input"), Some(json!("now")));
        assert_eq!(e.var(RATE_VAR), Some(json!(1)));
        assert!(e.toggle_state(&key_path(&p, &pg, "pause")));
        drop(e);
        // Still cooling down: a different message with the same command does nothing.
        handle(&app, chat("!brb again", &["mod"]));
        assert!(app.engine.lock().toggle_state(&key_path(&p, &pg, "pause")));
    }
}

//! Layout model. Mirrors packages/protocol/src/model.ts.
//! Everything an editor sends passes `validate` before it is stored or used.

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ids::valid_id;

pub const MAX_GRID: u8 = 12;
pub const MAX_PROFILES: usize = 20;
pub const MAX_PAGES: usize = 50;
pub const MAX_ACTIONS: usize = 32;
pub const MAX_ACTION_SETTINGS: usize = 64 * 1024;
pub const MAX_STATES: usize = 8;
pub const MAX_STYLES: usize = 64;
/// Styles one key can use.
pub const MAX_KEY_STYLES: usize = 8;
pub const LONG_MS: u32 = 500;
pub const DOUBLE_MS: u32 = 300;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LayoutFile {
    pub version: u32,
    #[serde(default)]
    pub rev: u64,
    pub default_profile: String,
    pub profiles: Vec<Profile>,
    /// Named looks that keys can share, like "red while live".
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub styles: Vec<Style>,
    /// Chat commands, keywords and channel point redemptions that press a key.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub triggers: Vec<Trigger>,
}

/// A named rule: when the condition holds, keys using the style change their look.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Style {
    pub id: String,
    pub name: String,
    pub when: Condition,
    pub style: FeedbackStyle,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub apps: Vec<String>,
    pub home_page: String,
    pub pages: Vec<Page>,
    /// The physical Stream Deck this layout is for. That deck shows it instead of the active layout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deck: Option<DeckLink>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeckLink {
    pub serial: String,
    pub model: String,
    pub cols: u8,
    pub rows: u8,
    /// Screen brightness in percent. Not set: `DeckLink::BRIGHTNESS`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brightness: Option<u8>,
    /// Minutes without a press before the deck goes dark. Not set or 0: never.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sleep: Option<u16>,
}

impl DeckLink {
    pub const BRIGHTNESS: u8 = 70;
    pub const MAX_SLEEP: u16 = 600;

    /// Dials on this model. Their keys sit in the row right below the key rows, one per dial.
    pub fn dials(&self) -> u8 {
        model_dials(&self.model)
    }

    pub fn brightness(&self) -> u8 {
        self.brightness.unwrap_or(Self::BRIGHTNESS).min(100)
    }

    pub fn sleep_minutes(&self) -> u16 {
        self.sleep.unwrap_or(0)
    }
}

/// Number of dials by model name, as the companion names the decks.
pub fn model_dials(model: &str) -> u8 {
    match model {
        "Stream Deck +" => 4,
        "Stream Deck + XL" => 6,
        _ => 0,
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum FormFactor {
    PhonePortrait,
    PhoneLandscape,
    Tablet,
    Desktop,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub folder: bool,
    pub cols: u8,
    pub rows: u8,
    pub keys: Vec<Key>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub variants: BTreeMap<FormFactor, Variant>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Variant {
    pub cols: u8,
    pub rows: u8,
    pub at: BTreeMap<String, [u8; 4]>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum KeyKind {
    Button,
    Toggle,
    Folder,
    Back,
    Slider,
    Widget,
    /// Steps through 2 to 8 states, one per press.
    Multi,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum KeyColor {
    Default,
    Teal,
    Violet,
    Green,
    Amber,
    Red,
    Blue,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Key {
    pub id: String,
    pub x: u8,
    pub y: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub w: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub h: Option<u8>,
    pub kind: KeyKind,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sub: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<KeyColor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hold: Option<u32>,
    /// Runs the steps again every this many ms while the key is held.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repeat: Option<u32>,
    #[serde(default)]
    pub actions: Vec<Action>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub off_actions: Vec<Action>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub feedback: Vec<Feedback>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slider: Option<Slider>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub widget: Option<Widget>,
    /// Multi-state keys: each press runs the current state and moves on to the next.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub states: Vec<KeyState>,
    /// Multi-state keys: show the state whose value this variable has, whatever changed it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub follow: Option<String>,
    /// Run instead of the normal steps when the key is held for `long_ms`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub long_actions: Vec<Action>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub long_ms: Option<u32>,
    /// Run instead of the normal steps on two taps within `double_ms`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub double_actions: Vec<Action>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub double_ms: Option<u32>,
    /// Ids of layout styles this key uses, applied in order before its own feedback.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub styles: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct KeyState {
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sub: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<KeyColor>,
    #[serde(default)]
    pub actions: Vec<Action>,
    /// Value of the followed variable that means this state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
}

/// One of the step lists on a key. Plugins see it in instance ids.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StepList {
    Main,
    /// Toggles: when turned off.
    Off,
    /// Multi-state keys: the steps of one state.
    State(usize),
    Long,
    Double,
}

impl StepList {
    /// What goes before the step number in an instance id.
    pub fn tag(self) -> String {
        match self {
            StepList::Main => String::new(),
            StepList::Off => "o".into(),
            StepList::State(i) => format!("s{i}."),
            StepList::Long => "l".into(),
            StepList::Double => "d".into(),
        }
    }

    /// Reads "o3", "s1.0", "7" and so on back into a list and step number.
    pub fn parse(s: &str) -> Option<(StepList, usize)> {
        let num = |t: &str| t.parse::<usize>().ok().filter(|_| !t.is_empty() && t.bytes().all(|b| b.is_ascii_digit()));
        if let Some(rest) = s.strip_prefix('o') {
            return Some((StepList::Off, num(rest)?));
        }
        if let Some(rest) = s.strip_prefix('l') {
            return Some((StepList::Long, num(rest)?));
        }
        if let Some(rest) = s.strip_prefix('d') {
            return Some((StepList::Double, num(rest)?));
        }
        if let Some((i, n)) = s.strip_prefix('s').and_then(|r| r.split_once('.')) {
            return Some((StepList::State(num(i)?), num(n)?));
        }
        Some((StepList::Main, num(s)?))
    }
}

impl Key {
    pub fn w(&self) -> u8 {
        self.w.unwrap_or(1)
    }
    pub fn h(&self) -> u8 {
        self.h.unwrap_or(1)
    }

    pub fn steps(&self, list: StepList) -> &[Action] {
        match list {
            StepList::Main => &self.actions,
            StepList::Off => &self.off_actions,
            StepList::State(i) => self.states.get(i).map(|s| s.actions.as_slice()).unwrap_or(&[]),
            StepList::Long => &self.long_actions,
            StepList::Double => &self.double_actions,
        }
    }

    pub fn steps_mut(&mut self, list: StepList) -> Option<&mut Vec<Action>> {
        match list {
            StepList::Main => Some(&mut self.actions),
            StepList::Off => Some(&mut self.off_actions),
            StepList::State(i) => self.states.get_mut(i).map(|s| &mut s.actions),
            StepList::Long => Some(&mut self.long_actions),
            StepList::Double => Some(&mut self.double_actions),
        }
    }

    /// Every step list on the key, including empty ones.
    pub fn step_lists(&self) -> impl Iterator<Item = (StepList, &[Action])> {
        [(StepList::Main, self.actions.as_slice()), (StepList::Off, &self.off_actions), (StepList::Long, &self.long_actions), (StepList::Double, &self.double_actions)]
            .into_iter()
            .chain(self.states.iter().enumerate().map(|(i, s)| (StepList::State(i), s.actions.as_slice())))
    }

    pub fn all_actions(&self) -> impl Iterator<Item = &Action> {
        self.step_lists().flat_map(|(_, l)| l.iter())
    }

    pub fn all_actions_mut(&mut self) -> impl Iterator<Item = &mut Action> {
        self.actions
            .iter_mut()
            .chain(self.off_actions.iter_mut())
            .chain(self.long_actions.iter_mut())
            .chain(self.double_actions.iter_mut())
            .chain(self.states.iter_mut().flat_map(|s| s.actions.iter_mut()))
    }

    /// A normal press has a step that acts for as long as the key is held, like push-to-talk.
    /// Long press and double tap steps may: they fire while the key is still down.
    pub fn acts_while_held(&self) -> bool {
        self.step_lists().filter(|(l, _)| !matches!(l, StepList::Long | StepList::Double)).any(|(_, steps)| steps.iter().any(acts_while_held))
    }

    /// Hold to confirm, repeat and "while held" steps all need the key to fire the moment
    /// it goes down, so they rule out long press and double tap.
    pub fn needs_plain_press(&self) -> bool {
        self.hold.is_some_and(|h| h > 0) || self.repeat.is_some() || self.acts_while_held()
    }

    /// Long press and double tap, when this key can use them: (long press ms, double tap ms).
    pub fn gestures(&self) -> (Option<u32>, Option<u32>) {
        if !matches!(self.kind, KeyKind::Button | KeyKind::Toggle | KeyKind::Multi) || self.needs_plain_press() {
            return (None, None);
        }
        let long = (!self.long_actions.is_empty()).then(|| self.long_ms.unwrap_or(LONG_MS));
        let double = (!self.double_actions.is_empty()).then(|| self.double_ms.unwrap_or(DOUBLE_MS));
        (long, double)
    }
}

/// Plugin steps set to act while the key is held, like "Talk while held".
pub fn acts_while_held(a: &Action) -> bool {
    a.plugin().is_some() && ["mode", "op"].iter().any(|f| a.settings.get(*f).and_then(Value::as_str) == Some("hold"))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Slider {
    pub min: f64,
    pub max: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Widget {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feed: Option<String>,
    /// No longer used: feed widgets fill the key. Still read, so older layouts load.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lines: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<String>,
    /// Feed messages longer than this end in "…".
    #[serde(default, rename = "maxChars", skip_serializing_if = "Option::is_none")]
    pub max_chars: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Action {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub settings: serde_json::Map<String, Value>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub review: bool,
}

impl Action {
    /// Plugin id and action id for plugin actions, None for core actions.
    pub fn plugin(&self) -> Option<(&str, &str)> {
        self.kind.split_once('/')
    }

    /// Same action, ignoring the review flag.
    pub fn same_as(&self, other: &Action) -> bool {
        self.kind == other.kind && self.settings == other.settings
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CompareOp {
    Eq,
    Ne,
    Gt,
    Lt,
    Truthy,
    Falsy,
    Contains,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Condition {
    pub var: String,
    pub op: CompareOp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Feedback {
    pub when: Condition,
    pub style: FeedbackStyle,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct FeedbackStyle {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<KeyColor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sub: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub badge: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tone: Option<Tone>,
    #[serde(default, rename = "badgeTone", skip_serializing_if = "Option::is_none")]
    pub badge_tone: Option<Tone>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
}

pub const MAX_TRIGGERS: usize = 100;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Trigger {
    pub id: String,
    #[serde(default)]
    pub source: ChatSource,
    pub kind: TriggerKind,
    /// The command ("!brb"), the words to look for, or the reward title.
    #[serde(rename = "match")]
    pub pattern: String,
    #[serde(default)]
    pub who: Audience,
    /// Seconds before the trigger can fire again.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub cooldown: u32,
    pub target: TriggerTarget,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub disabled: bool,
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TriggerTarget {
    pub profile: String,
    pub page: String,
    pub key: String,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ChatSource {
    #[default]
    Any,
    Twitch,
    Youtube,
    Kick,
}

impl ChatSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Any => "any",
            Self::Twitch => "twitch",
            Self::Youtube => "youtube",
            Self::Kick => "kick",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TriggerKind {
    Command,
    Redeem,
    Keyword,
}

/// Who may set off a trigger. Each level includes the ones above it (mods count as subs).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Audience {
    #[default]
    Everyone,
    Subs,
    Mods,
    Broadcaster,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Tone {
    Default,
    Accent,
    Danger,
    Success,
    Warning,
    Info,
    Muted,
}

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct Invalid(pub String);

fn bad<T>(msg: impl Into<String>) -> Result<T, Invalid> {
    Err(Invalid(msg.into()))
}

fn text(field: &str, s: &str, max: usize) -> Result<(), Invalid> {
    if s.chars().count() > max {
        return bad(format!("{field} is longer than {max} characters"));
    }
    if s.chars().any(|c| c.is_control() && c != '\n') {
        return bad(format!("{field} has control characters"));
    }
    Ok(())
}

fn opt_text(field: &str, s: &Option<String>, max: usize) -> Result<(), Invalid> {
    s.as_deref().map_or(Ok(()), |s| text(field, s, max))
}

pub fn valid_icon(s: &str) -> bool {
    if let Some(hash) = s.strip_prefix("asset:") {
        return (16..=64).contains(&hash.len()) && hash.bytes().all(|b| b.is_ascii_hexdigit());
    }
    if let Some(rest) = s.strip_prefix("plugin:") {
        return rest.len() <= 200 && !rest.contains("..") && rest.bytes().all(|b| b.is_ascii_graphic() && b != b'\\');
    }
    !s.is_empty() && s.len() <= 48 && s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Action types: "core.<name>" or "<plugin id>/<action id>".
pub fn valid_action_type(s: &str) -> bool {
    if s.len() > 160 {
        return false;
    }
    let ok = |p: &str| !p.is_empty() && p.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'));
    match s.split_once('/') {
        Some((plugin, action)) => ok(plugin) && ok(action),
        None => s.starts_with("core.") && ok(s),
    }
}

pub fn valid_var_name(s: &str) -> bool {
    !s.is_empty() && s.len() <= 80 && s.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b'/'))
}

fn grid(n: u8) -> bool {
    (1..=MAX_GRID).contains(&n)
}

fn check_actions(where_: &str, actions: &[Action]) -> Result<(), Invalid> {
    if actions.len() > MAX_ACTIONS {
        return bad(format!("{where_}: more than {MAX_ACTIONS} steps"));
    }
    for a in actions {
        if !valid_action_type(&a.kind) {
            return bad(format!("{where_}: unknown action type"));
        }
        let size = serde_json::to_vec(&a.settings).map(|v| v.len()).unwrap_or(usize::MAX);
        if size > MAX_ACTION_SETTINGS {
            return bad(format!("{where_}: action settings are too large"));
        }
    }
    Ok(())
}

fn fits(cols: u8, rows: u8, x: u8, y: u8, w: u8, h: u8) -> bool {
    w >= 1 && h >= 1 && (x as u16 + w as u16) <= cols as u16 && (y as u16 + h as u16) <= rows as u16
}

pub fn validate(l: &LayoutFile) -> Result<(), Invalid> {
    if l.version != 1 {
        return bad("unsupported layout version");
    }
    if l.profiles.is_empty() || l.profiles.len() > MAX_PROFILES {
        return bad("a layout needs 1 to 20 profiles");
    }
    if l.styles.len() > MAX_STYLES {
        return bad(format!("at most {MAX_STYLES} key styles"));
    }
    let mut style_ids = HashSet::new();
    for st in &l.styles {
        if !valid_id(&st.id) || !style_ids.insert(st.id.as_str()) {
            return bad("style ids must be unique");
        }
        if st.name.trim().is_empty() {
            return bad("every style needs a name");
        }
        text("style name", &st.name, 40)?;
        // A style without a variable yet is kept but never applies.
        if !st.when.var.is_empty() {
            check_condition("style", &st.when)?;
        }
        if let Some(Value::String(v)) = &st.when.value {
            text("style value", v, 200)?;
        }
        check_look(&st.style)?;
    }
    let mut profile_ids = HashSet::new();
    for p in &l.profiles {
        if !valid_id(&p.id) || !profile_ids.insert(p.id.as_str()) {
            return bad("profile ids must be unique");
        }
        text("profile name", &p.name, 40)?;
        if p.apps.len() > 20 || p.apps.iter().any(|a| a.is_empty() || a.len() > 120 || a.chars().any(char::is_control)) {
            return bad("profile app list is invalid");
        }
        if p.pages.is_empty() || p.pages.len() > MAX_PAGES {
            return bad("a profile needs 1 to 50 pages");
        }
        if let Some(d) = &p.deck {
            text("Stream Deck serial", &d.serial, 64)?;
            text("Stream Deck model", &d.model, 40)?;
            if !(1..=16).contains(&d.cols) || !(1..=16).contains(&d.rows) {
                return bad("Stream Deck grid is invalid");
            }
            if d.brightness.is_some_and(|b| b > 100) || d.sleep.is_some_and(|m| m > DeckLink::MAX_SLEEP) {
                return bad("Stream Deck brightness or sleep time is invalid");
            }
            let dials = d.dials();
            // The row below the keys holds one single-cell key per dial, nothing else.
            for pg in &p.pages {
                for k in &pg.keys {
                    let in_dial_row = k.y + k.h() > d.rows;
                    if in_dial_row && dials > 0 && !(k.y == d.rows && k.w() == 1 && k.h() == 1 && k.x < dials) {
                        return bad(format!("key \"{}\" doesn't fit the dials of the {}", k.label, d.model));
                    }
                }
            }
        }
        let page_ids: HashSet<&str> = p.pages.iter().map(|pg| pg.id.as_str()).collect();
        if page_ids.len() != p.pages.len() {
            return bad("page ids must be unique");
        }
        if !page_ids.contains(p.home_page.as_str()) {
            return bad("home page does not exist");
        }
        for pg in &p.pages {
            validate_page(pg, &page_ids, &style_ids)?;
        }
    }
    if !profile_ids.contains(l.default_profile.as_str()) {
        return bad("default profile does not exist");
    }
    validate_triggers(&l.triggers)
}

/// Targets aren't checked here: a deleted key only makes its trigger do nothing.
fn validate_triggers(list: &[Trigger]) -> Result<(), Invalid> {
    if list.len() > MAX_TRIGGERS {
        return bad(format!("at most {MAX_TRIGGERS} chat triggers"));
    }
    let mut ids = HashSet::new();
    for t in list {
        if !valid_id(&t.id) || !ids.insert(t.id.as_str()) {
            return bad("chat trigger ids must be unique");
        }
        if t.pattern.trim().is_empty() {
            return bad("a chat trigger needs something to match");
        }
        text("chat trigger", &t.pattern, 100)?;
        if t.pattern.contains('\n') {
            return bad("chat trigger has a line break");
        }
        if t.cooldown > 86_400 {
            return bad("chat trigger cooldown can be at most a day");
        }
        let target = &t.target;
        if ![&target.profile, &target.page, &target.key].iter().all(|id| valid_id(id)) {
            return bad("chat trigger points at an invalid key");
        }
    }
    Ok(())
}

fn check_condition(what: &str, c: &Condition) -> Result<(), Invalid> {
    if !valid_var_name(&c.var) {
        return bad(format!("{what} variable is invalid"));
    }
    if c.value.as_ref().is_some_and(|v| !(v.is_string() || v.is_number() || v.is_boolean())) {
        return bad(format!("{what} value must be text, a number or true/false"));
    }
    Ok(())
}

fn check_look(s: &FeedbackStyle) -> Result<(), Invalid> {
    for (n, v) in [("label", &s.label), ("sub", &s.sub), ("badge", &s.badge), ("meta", &s.meta)] {
        opt_text(n, v, 64)?;
    }
    if s.icon.as_deref().is_some_and(|i| !valid_icon(i)) {
        return bad("invalid icon");
    }
    Ok(())
}

fn check_key_steps(k: &Key) -> Result<(), Invalid> {
    if k.long_ms.is_some_and(|ms| !(200..=5_000).contains(&ms)) {
        return bad("long press must be between 200 ms and 5 seconds");
    }
    if k.double_ms.is_some_and(|ms| !(100..=1_000).contains(&ms)) {
        return bad("double tap window must be between 100 ms and 1 second");
    }
    check_actions(&k.label, &k.long_actions)?;
    check_actions(&k.label, &k.double_actions)?;
    if (!k.long_actions.is_empty() || !k.double_actions.is_empty()) && k.needs_plain_press() {
        return bad(format!("key \"{}\": long press and double tap can't be combined with hold to confirm, repeat or steps that act while held", k.label));
    }
    if k.kind == KeyKind::Multi && !(2..=MAX_STATES).contains(&k.states.len()) {
        return bad(format!("key \"{}\" needs 2 to {MAX_STATES} states", k.label));
    }
    if k.states.len() > MAX_STATES {
        return bad(format!("at most {MAX_STATES} states per key"));
    }
    for st in &k.states {
        text("state label", &st.label, 64)?;
        opt_text("state second line", &st.sub, 64)?;
        if st.icon.as_deref().is_some_and(|i| !valid_icon(i)) {
            return bad("invalid icon");
        }
        check_actions(&k.label, &st.actions)?;
        if st.value.as_ref().is_some_and(|v| !(v.is_string() || v.is_number() || v.is_boolean())) {
            return bad("state value must be text, a number or true/false");
        }
        if let Some(Value::String(v)) = &st.value {
            text("state value", v, 200)?;
        }
    }
    if k.follow.as_deref().is_some_and(|f| !valid_var_name(f)) {
        return bad("followed variable is invalid");
    }
    Ok(())
}

pub(crate) fn validate_page(pg: &Page, page_ids: &HashSet<&str>, style_ids: &HashSet<&str>) -> Result<(), Invalid> {
    if !valid_id(&pg.id) {
        return bad("invalid page id");
    }
    text("page name", &pg.name, 40)?;
    opt_text("page subtitle", &pg.subtitle, 60)?;
    if !grid(pg.cols) || !grid(pg.rows) {
        return bad("grid must be 1 to 12 cells each way");
    }
    let mut used = vec![false; pg.cols as usize * pg.rows as usize];
    let mut key_ids = HashSet::new();
    for k in &pg.keys {
        if !valid_id(&k.id) || !key_ids.insert(k.id.as_str()) {
            return bad("key ids must be unique");
        }
        if !fits(pg.cols, pg.rows, k.x, k.y, k.w(), k.h()) {
            return bad(format!("key \"{}\" is outside the grid", k.label));
        }
        for dy in 0..k.h() {
            for dx in 0..k.w() {
                let i = (k.y + dy) as usize * pg.cols as usize + (k.x + dx) as usize;
                if std::mem::replace(&mut used[i], true) {
                    return bad(format!("key \"{}\" overlaps another key", k.label));
                }
            }
        }
        text("key label", &k.label, 64)?;
        opt_text("key subtitle", &k.sub, 64)?;
        if k.icon.as_deref().is_some_and(|i| !valid_icon(i)) {
            return bad("invalid icon");
        }
        if k.repeat.is_some_and(|r| !(100..=10_000).contains(&r)) {
            return bad("repeat must be between 100 ms and 10 seconds");
        }
        if k.hold.is_some_and(|h| h > 10_000) {
            return bad("hold can be at most 10 seconds");
        }
        check_actions(&k.label, &k.actions)?;
        check_actions(&k.label, &k.off_actions)?;
        check_key_steps(k)?;
        if k.styles.len() > MAX_KEY_STYLES {
            return bad(format!("at most {MAX_KEY_STYLES} styles per key"));
        }
        if k.styles.iter().any(|s| !style_ids.contains(s.as_str())) {
            return bad(format!("key \"{}\" uses a style that doesn't exist", k.label));
        }
        match k.kind {
            KeyKind::Folder => {
                if !k.target.as_deref().is_some_and(|t| page_ids.contains(t)) {
                    return bad(format!("folder \"{}\" points at a missing page", k.label));
                }
            }
            KeyKind::Slider => {
                let s = k.slider.as_ref().ok_or_else(|| Invalid("slider needs a range".into()))?;
                if !(s.min.is_finite() && s.max.is_finite() && s.min < s.max) || s.step.is_some_and(|st| !(st > 0.0 && st.is_finite())) {
                    return bad("slider range is invalid");
                }
            }
            KeyKind::Widget => {
                let w = k.widget.as_ref().ok_or_else(|| Invalid("widget needs content".into()))?;
                opt_text("widget text", &w.text, 200)?;
                if w.feed.as_deref().is_some_and(|f| !valid_var_name(f)) || w.progress.as_deref().is_some_and(|p| !valid_var_name(p)) {
                    return bad("widget source is invalid");
                }
                if w.lines.is_some_and(|n| n == 0 || n > 20) {
                    return bad("widget shows 1 to 20 lines");
                }
                if w.max_chars.is_some_and(|n| n == 0 || n > 500) {
                    return bad("max characters must be 1 to 500");
                }
            }
            _ => {}
        }
        if k.feedback.len() > 8 {
            return bad("at most 8 feedback rules per key");
        }
        for f in &k.feedback {
            check_condition("feedback", &f.when)?;
            check_look(&f.style)?;
        }
    }
    for v in pg.variants.values() {
        if !grid(v.cols) || !grid(v.rows) {
            return bad("variant grid must be 1 to 12 cells each way");
        }
        for (id, [x, y, w, h]) in &v.at {
            if !key_ids.contains(id.as_str()) || !fits(v.cols, v.rows, *x, *y, *w, *h) {
                return bad("variant places a missing key or goes outside its grid");
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> LayoutFile {
        serde_json::from_value(serde_json::json!({
            "version": 1, "rev": 0, "defaultProfile": "main",
            "profiles": [{ "id": "main", "name": "Main", "homePage": "p1", "pages": [
                { "id": "p1", "name": "One", "cols": 3, "rows": 2, "keys": [
                    { "id": "a", "x": 0, "y": 0, "kind": "button", "label": "A", "actions": [{ "type": "core.wait", "settings": { "ms": 10 } }] },
                    { "id": "b", "x": 1, "y": 0, "w": 2, "kind": "folder", "label": "Cams", "target": "p2", "actions": [] }
                ]},
                { "id": "p2", "name": "Cams", "folder": true, "cols": 2, "rows": 2, "keys": [] }
            ]}]
        }))
        .unwrap()
    }

    #[test]
    fn accepts_a_good_layout() {
        validate(&sample()).unwrap();
    }

    #[test]
    fn rejects_overlap_and_bad_targets() {
        let mut l = sample();
        l.profiles[0].pages[0].keys[1].x = 0;
        assert!(validate(&l).is_err());
        let mut l = sample();
        l.profiles[0].pages[0].keys[1].target = Some("nope".into());
        assert!(validate(&l).is_err());
        let mut l = sample();
        l.profiles[0].pages[0].keys[0].actions[0].kind = "rm -rf".into();
        assert!(validate(&l).is_err());
    }

    #[test]
    fn old_layouts_load_and_save_unchanged() {
        let l = sample();
        let back = serde_json::to_value(&l).unwrap();
        let k = &back["profiles"][0]["pages"][0]["keys"][0];
        for f in ["states", "follow", "longActions", "longMs", "doubleActions", "doubleMs", "styles"] {
            assert!(k.get(f).is_none(), "{f} stays out of old keys");
        }
        assert!(back.get("styles").is_none());
    }

    #[test]
    fn multi_state_keys_need_2_to_8_states() {
        let state = |n: usize| serde_json::json!({ "label": format!("S{n}"), "actions": [] });
        let mut l = sample();
        let k = &mut l.profiles[0].pages[0].keys[0];
        k.kind = KeyKind::Multi;
        assert!(validate(&l).is_err(), "no states");
        for n in 1..=8 {
            l.profiles[0].pages[0].keys[0].states = (0..n).map(|i| serde_json::from_value(state(i)).unwrap()).collect();
            assert_eq!(validate(&l).is_ok(), n >= 2, "{n} states");
        }
        l.profiles[0].pages[0].keys[0].states.push(serde_json::from_value(state(9)).unwrap());
        assert!(validate(&l).is_err(), "9 states");
        let mut l2 = sample();
        let k = &mut l2.profiles[0].pages[0].keys[0];
        k.kind = KeyKind::Multi;
        k.states = (0..2).map(|i| serde_json::from_value(state(i)).unwrap()).collect();
        k.follow = Some("bad name".into());
        assert!(validate(&l2).is_err());
        l2.profiles[0].pages[0].keys[0].follow = Some("obs.scene".into());
        l2.profiles[0].pages[0].keys[0].states[0].value = Some(serde_json::json!({ "x": 1 }));
        assert!(validate(&l2).is_err(), "values are text, numbers or true/false");
    }

    #[test]
    fn long_press_and_double_tap_rule_out_holding() {
        let step: Action = serde_json::from_value(serde_json::json!({ "type": "core.wait", "settings": { "ms": 1 } })).unwrap();
        let mut l = sample();
        l.profiles[0].pages[0].keys[0].long_actions = vec![step.clone()];
        validate(&l).unwrap();
        l.profiles[0].pages[0].keys[0].long_ms = Some(100);
        assert!(validate(&l).is_err(), "too short");
        l.profiles[0].pages[0].keys[0].long_ms = Some(800);
        l.profiles[0].pages[0].keys[0].double_actions = vec![step.clone()];
        l.profiles[0].pages[0].keys[0].double_ms = Some(2000);
        assert!(validate(&l).is_err(), "too long");
        l.profiles[0].pages[0].keys[0].double_ms = None;
        validate(&l).unwrap();
        for f in [|k: &mut Key| k.hold = Some(1000), |k: &mut Key| k.repeat = Some(500)] {
            let mut l = l.clone();
            f(&mut l.profiles[0].pages[0].keys[0]);
            assert!(validate(&l).is_err());
        }
        let mut l = l.clone();
        let ptt: Action = serde_json::from_value(serde_json::json!({ "type": "dev.keyside.discord/mute", "settings": { "mode": "hold" } })).unwrap();
        l.profiles[0].pages[0].keys[0].actions.push(ptt);
        assert!(validate(&l).is_err(), "talk while held");
        // Held steps are fine as the long press itself: it fires while the key is down.
        let mut l = sample();
        l.profiles[0].pages[0].keys[0].long_actions = l.profiles[0].pages[0].keys[0].actions.clone();
        l.profiles[0].pages[0].keys[0].long_actions[0] = serde_json::from_value(serde_json::json!({ "type": "dev.keyside.discord/mute", "settings": { "mode": "hold" } })).unwrap();
        validate(&l).unwrap();
    }

    #[test]
    fn styles_are_checked_and_must_exist() {
        let mut l = sample();
        l.styles = vec![
            serde_json::from_value(
                serde_json::json!({ "id": "live", "name": "Live", "when": { "var": "obs.streaming", "op": "truthy" }, "style": { "color": "red", "badge": "LIVE" } }),
            )
            .unwrap(),
        ];
        l.profiles[0].pages[0].keys[0].styles = vec!["live".into()];
        validate(&l).unwrap();
        l.profiles[0].pages[0].keys[0].styles = vec!["gone".into()];
        assert!(validate(&l).is_err());
        let mut l2 = l.clone();
        l2.profiles[0].pages[0].keys[0].styles.clear();
        l2.styles.push(l2.styles[0].clone());
        assert!(validate(&l2).is_err(), "duplicate id");
        l2.styles.pop();
        l2.styles[0].name = " ".into();
        assert!(validate(&l2).is_err(), "needs a name");
        l2.styles[0].name = "Unfinished".into();
        l2.styles[0].when.var = String::new();
        validate(&l2).unwrap();
    }

    #[test]
    fn step_lists_round_trip_through_instance_tags() {
        for (l, n) in [(StepList::Main, 3), (StepList::Off, 0), (StepList::State(7), 12), (StepList::Long, 1), (StepList::Double, 31)] {
            assert_eq!(StepList::parse(&format!("{}{n}", l.tag())), Some((l, n)));
        }
        assert_eq!(StepList::parse("s1"), None);
        assert_eq!(StepList::parse("+1"), None);
        assert_eq!(StepList::parse(""), None);
    }

    #[test]
    fn checks_chat_triggers() {
        let trigger = |id: &str, pattern: &str| -> Trigger {
            serde_json::from_value(serde_json::json!({ "id": id, "kind": "command", "match": pattern, "target": { "profile": "main", "page": "p1", "key": "gone" } })).unwrap()
        };
        let mut l = sample();
        l.triggers = vec![trigger("t1", "!brb"), trigger("t2", "hello there")];
        validate(&l).unwrap();
        assert_eq!(l.triggers[0].who, Audience::Everyone);
        assert_eq!(l.triggers[0].source, ChatSource::Any);
        l.triggers[1].id = "t1".into();
        assert!(validate(&l).is_err(), "duplicate id");
        l.triggers = vec![trigger("t1", "  ")];
        assert!(validate(&l).is_err(), "empty match");
        l.triggers = vec![trigger("t1", "a\nb")];
        assert!(validate(&l).is_err(), "line break");
        // Layouts without triggers stay as they were.
        assert!(!serde_json::to_string(&sample()).unwrap().contains("triggers"));
    }

    #[test]
    fn icons_cannot_escape() {
        assert!(valid_icon("monitor"));
        assert!(valid_icon("asset:0123456789abcdef"));
        assert!(!valid_icon("plugin:../../etc/passwd"));
        assert!(!valid_icon("javascript:alert(1)"));
    }
}

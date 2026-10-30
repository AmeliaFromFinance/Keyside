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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LayoutFile {
    pub version: u32,
    #[serde(default)]
    pub rev: u64,
    pub default_profile: String,
    pub profiles: Vec<Profile>,
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
}

impl Key {
    pub fn w(&self) -> u8 {
        self.w.unwrap_or(1)
    }
    pub fn h(&self) -> u8 {
        self.h.unwrap_or(1)
    }
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lines: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<String>,
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
            validate_page(pg, &page_ids)?;
        }
    }
    if !profile_ids.contains(l.default_profile.as_str()) {
        return bad("default profile does not exist");
    }
    Ok(())
}

fn validate_page(pg: &Page, page_ids: &HashSet<&str>) -> Result<(), Invalid> {
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
            }
            _ => {}
        }
        if k.feedback.len() > 8 {
            return bad("at most 8 feedback rules per key");
        }
        for f in &k.feedback {
            if !valid_var_name(&f.when.var) {
                return bad("feedback variable is invalid");
            }
            let s = &f.style;
            for (n, v) in [("label", &s.label), ("sub", &s.sub), ("badge", &s.badge), ("meta", &s.meta)] {
                opt_text(n, v, 64)?;
            }
            if s.icon.as_deref().is_some_and(|i| !valid_icon(i)) {
                return bad("invalid icon");
            }
            if f.when.value.as_ref().is_some_and(|v| !(v.is_string() || v.is_number() || v.is_boolean())) {
                return bad("feedback value must be text, a number or true/false");
            }
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
    fn icons_cannot_escape() {
        assert!(valid_icon("monitor"));
        assert!(valid_icon("asset:0123456789abcdef"));
        assert!(!valid_icon("plugin:../../etc/passwd"));
        assert!(!valid_icon("javascript:alert(1)"));
    }
}

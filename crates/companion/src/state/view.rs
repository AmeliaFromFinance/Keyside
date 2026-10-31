//! Turns layout keys into what clients draw.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::model::{CompareOp, Condition, FeedbackStyle, FormFactor, Key, KeyColor, KeyKind, Page, Tone};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FeedItem {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default)]
    pub at: u64,
    /// The message id from the chat service, so a deleted message can be taken off again.
    /// Clients don't need it.
    #[serde(default, skip_serializing)]
    pub id: Option<String>,
}

/// Which feed lines to take off. A line goes if it matches any part.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FeedMatch {
    pub ids: Vec<String>,
    pub author: Option<String>,
    pub all: bool,
}

impl FeedMatch {
    pub fn matches(&self, item: &FeedItem) -> bool {
        self.all || item.id.as_ref().is_some_and(|id| self.ids.contains(id)) || self.author.as_ref().is_some_and(|a| item.author.as_ref() == Some(a))
    }
}

/// Fields a plugin may set on its own keys.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ViewPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sub: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<KeyColor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub badge: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tone: Option<Tone>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub badge_tone: Option<Tone>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct KeyView {
    pub id: String,
    pub kind: KeyKind,
    pub x: u8,
    pub y: u8,
    pub w: u8,
    pub h: u8,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<KeyColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub badge: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tone: Option<Tone>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub badge_tone: Option<Tone>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hold: Option<u32>,
    /// Multi-state keys: which state shows, counted from 0.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<usize>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub disabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lines: Option<Vec<FeedItem>>,
    /// An app this key needs that isn't connected, by name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trouble: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PageView {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    pub folder: bool,
    pub cols: u8,
    pub rows: u8,
    pub index: usize,
    pub count: usize,
    pub keys: Vec<KeyView>,
}

/// Where each key sits for a form factor. Landscape phones get the page transposed
/// unless the page has its own arrangement.
pub fn placements(page: &Page, ff: FormFactor) -> (u8, u8, Vec<(usize, [u8; 4])>) {
    if let Some(v) = page.variants.get(&ff) {
        let keys = page.keys.iter().enumerate().filter_map(|(i, k)| v.at.get(&k.id).map(|p| (i, *p))).collect();
        return (v.cols, v.rows, keys);
    }
    let transpose = ff == FormFactor::PhoneLandscape && page.rows > page.cols;
    let keys = page.keys.iter().enumerate().map(|(i, k)| (i, if transpose { [k.y, k.x, k.h(), k.w()] } else { [k.x, k.y, k.w(), k.h()] })).collect();
    if transpose { (page.rows, page.cols, keys) } else { (page.cols, page.rows, keys) }
}

pub fn base_view(key: &Key, pos: [u8; 4], folder_size: Option<usize>) -> KeyView {
    let mut v = KeyView {
        id: key.id.clone(),
        kind: key.kind,
        x: pos[0],
        y: pos[1],
        w: pos[2],
        h: pos[3],
        label: key.label.clone(),
        sub: key.sub.clone(),
        icon: key.icon.clone(),
        color: key.color,
        badge: None,
        tone: None,
        badge_tone: None,
        meta: None,
        image: None,
        value: None,
        progress: None,
        since: None,
        hold: key.hold.filter(|h| *h > 0),
        state: None,
        disabled: false,
        lines: None,
        trouble: None,
    };
    match key.kind {
        KeyKind::Folder => {
            v.icon.get_or_insert_with(|| "folder".into());
            if v.sub.is_none() {
                v.sub = folder_size.map(|n| format!("Folder · {n}"));
            }
        }
        KeyKind::Back => {
            v.icon.get_or_insert_with(|| "arrow-left".into());
        }
        _ => {}
    }
    if key.actions.len() > 1 {
        v.meta = Some(format!("{} steps", key.actions.len()));
    }
    if key.all_actions().any(|a| a.review) {
        v.badge = Some("REVIEW".into());
        v.tone = Some(Tone::Warning);
        v.disabled = true;
    }
    v
}

pub fn apply_patch(v: &mut KeyView, p: &ViewPatch) {
    macro_rules! set {
        ($($f:ident),*) => { $( if let Some(x) = &p.$f { v.$f = Some(x.clone()); } )* };
    }
    if let Some(l) = &p.label {
        v.label = l.clone();
    }
    set!(sub, icon, color, badge, tone, badge_tone, meta, image, value, progress, since);
    if let Some(d) = p.disabled {
        v.disabled = d;
    }
}

pub fn apply_style(v: &mut KeyView, s: &FeedbackStyle) {
    if let Some(c) = s.color {
        v.color = Some(c);
    }
    if let Some(l) = &s.label {
        v.label = l.clone();
    }
    for (dst, src) in [(&mut v.sub, &s.sub), (&mut v.icon, &s.icon), (&mut v.badge, &s.badge), (&mut v.meta, &s.meta)] {
        if let Some(x) = src {
            *dst = Some(x.clone());
        }
    }
    if let Some(t) = s.tone {
        v.tone = Some(t);
    }
    if let Some(t) = s.badge_tone {
        v.badge_tone = Some(t);
    }
    if let Some(d) = s.disabled {
        v.disabled = d;
    }
}

pub fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.is_empty() && s != "0" && !s.eq_ignore_ascii_case("false"),
        _ => true,
    }
}

pub fn display(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::Bool(true) => "on".into(),
        Value::Bool(false) => "off".into(),
        Value::Number(n) => match n.as_f64() {
            Some(f) if f.fract() == 0.0 && f.abs() < 1e15 => format!("{}", f as i64),
            Some(f) => {
                let s = format!("{f:.2}");
                s.trim_end_matches('0').trim_end_matches('.').to_string()
            }
            None => n.to_string(),
        },
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn as_num(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        _ => None,
    }
}

pub fn eval(c: &Condition, get: impl Fn(&str) -> Option<Value>) -> bool {
    let cur = get(&c.var).unwrap_or(Value::Null);
    let want = c.value.clone().unwrap_or(Value::Null);
    match c.op {
        CompareOp::Truthy => truthy(&cur),
        CompareOp::Falsy => !truthy(&cur),
        CompareOp::Eq => loose_eq(&cur, &want),
        CompareOp::Ne => !loose_eq(&cur, &want),
        CompareOp::Gt => matches!((as_num(&cur), as_num(&want)), (Some(a), Some(b)) if a > b),
        CompareOp::Lt => matches!((as_num(&cur), as_num(&want)), (Some(a), Some(b)) if a < b),
        CompareOp::Contains => display(&cur).to_lowercase().contains(&display(&want).to_lowercase()),
    }
}

pub fn loose_eq(a: &Value, b: &Value) -> bool {
    if a == b {
        return true;
    }
    match (as_num(a), as_num(b)) {
        (Some(x), Some(y)) if !(a.is_string() && b.is_string()) => x == y,
        _ => display(a) == display(b),
    }
}

/// Replaces {name} with variable values. Unknown names become empty. Output is capped.
/// Cuts `s` to `max` characters, ending in "…" when anything was cut.
pub fn shorten(s: &mut String, max: usize) {
    if let Some((i, _)) = s.char_indices().nth(max) {
        s.truncate(i);
        s.truncate(s.trim_end().len());
        s.push('…');
    }
}

pub fn template(s: &str, get: impl Fn(&str) -> Option<Value>) -> String {
    fill(s, get, 128)
}

/// `template` with a chosen length cap, in characters.
pub fn fill(s: &str, get: impl Fn(&str) -> Option<Value>, cap: usize) -> String {
    if !s.contains('{') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('}') {
            Some(end) if crate::model::valid_var_name(&after[..end]) => {
                out.push_str(&get(&after[..end]).map(|v| display(&v)).unwrap_or_default());
                rest = &after[end + 1..];
            }
            _ => {
                out.push('{');
                rest = after;
            }
        }
        if out.len() > cap * 4 {
            break;
        }
    }
    out.push_str(rest);
    out.chars().take(cap).collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn shortens_long_messages() {
        let mut s = "hello there chat".to_string();
        super::shorten(&mut s, 6);
        assert_eq!(s, "hello…");
        let mut s = "héllo".to_string();
        super::shorten(&mut s, 5);
        assert_eq!(s, "héllo", "nothing to cut");
    }

    use super::*;
    use serde_json::json;

    fn vars(name: &str) -> Option<Value> {
        match name {
            "obs.scene" => Some(json!("Ending")),
            "viewers" => Some(json!(42)),
            "ratio" => Some(json!(0.5)),
            _ => None,
        }
    }

    #[test]
    fn templates() {
        assert_eq!(template("{viewers} watching", vars), "42 watching");
        assert_eq!(template("{missing}x", vars), "x");
        assert_eq!(template("{not a var} {ratio}", vars), "{not a var} 0.5");
    }

    #[test]
    fn conditions() {
        let c = |op, value: Value| Condition { var: "obs.scene".into(), op, value: Some(value) };
        assert!(eval(&c(CompareOp::Eq, json!("Ending")), vars));
        assert!(eval(&c(CompareOp::Contains, json!("end")), vars));
        assert!(!eval(&c(CompareOp::Ne, json!("Ending")), vars));
        let n = Condition { var: "viewers".into(), op: CompareOp::Gt, value: Some(json!("40")) };
        assert!(eval(&n, vars));
    }
}

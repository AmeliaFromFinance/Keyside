//! Imports Elgato Stream Deck profiles (.streamDeckProfile files) as Keyside layouts.
//!
//! The file is a zip with one `<id>.sdProfile` folder, at the top or under `Profiles/`.
//!
//! Stream Deck 6 and later: the folder holds `manifest.json` with `Name`, `Device.Model` and
//! `Pages` (`Pages.Pages` lists the pages in order), and `Profiles/<page>/manifest.json` per
//! page. A page lists its `Controllers`: `Keypad` for keys and `Encoder` for dials, each with
//! `Actions` keyed "column,row". Folders are pages the page list leaves out. Page folders are
//! named after the page UUID, or in some exports after a 27-letter short form of it.
//!
//! Stream Deck 4 and 5: the folder's `manifest.json` holds `Actions` and `DeviceModel` itself,
//! folders are `Profiles/<id>.sdProfile` inside it, and key images sit in
//! `<column,row>/CustomImages/`.
//!
//! Everything here comes from a file someone shared, so wrong types read as missing and
//! nothing from the zip is written to disk except key images, through the asset store.

use std::collections::{BTreeMap, HashMap, HashSet};

use anyhow::{Result, bail};
use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::model::{self, Action, DeckLink, Key, KeyKind, KeyState, MAX_ACTIONS, MAX_GRID, MAX_PAGES, MAX_STATES, Page, Profile};
use crate::plugins::package::read_zip;

pub const MAX_FILE: usize = 50 * 1024 * 1024;
const MAX_ENTRIES: usize = 5000;
const MAX_ENTRY: u64 = 8 * 1024 * 1024;
const MAX_UNPACKED: u64 = 128 * 1024 * 1024;
/// Multi actions inside multi actions, this deep at most.
const MAX_DEPTH: usize = 4;

const BUILT_IN: &str = "com.elgato.streamdeck.";
const OPEN_CHILD: &str = "com.elgato.streamdeck.profile.openchild";
const BACK: &str = "com.elgato.streamdeck.profile.backtoparent";
const SWITCH: &str = "com.elgato.streamdeck.multiactions.routine2";
/// Qt's code for "no key". Unused hotkey slots have it.
const QT_NO_KEY: i64 = 0x01FF_FFFF;
/// Digits of the short page ids: 5 bits each, no U.
const SHORT_ID: &[u8; 32] = b"0123456789ABCDEFGHIJKLMNOPQRSTVW";

/// What came over and what didn't.
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub keys: usize,
    /// Plugin actions whose Stream Deck plugin isn't installed in Keyside yet.
    pub plugins: Vec<Missing>,
    /// Elgato actions Keyside has nothing for. Their keys keep title and image.
    pub unsupported: Vec<Missing>,
    /// Dial actions left out: the layout isn't linked to a deck with dials.
    pub dials: usize,
    /// Key images that were missing, too large or not an image.
    pub images: usize,
    /// Multi actions cut down to the most steps a key can have.
    pub trimmed: usize,
    /// Model of the deck the layout was linked to.
    pub deck: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Missing {
    pub name: String,
    pub uuid: String,
    pub keys: usize,
}

#[derive(Debug)]
pub struct Imported {
    pub profile: Profile,
    pub report: Report,
}

/// What the import needs from the rest of Keyside.
pub trait Env {
    /// UUID of the installed Stream Deck plugin that has this action.
    fn plugin_of(&self, action: &str) -> Option<String>;
    /// The deck to link a layout for this model to, when Keyside knows exactly one.
    fn deck(&self, model: &str) -> Option<DeckLink>;
    /// Stores a key image and returns the icon value for keys.
    fn store_image(&mut self, name: &str, bytes: &[u8]) -> Option<String>;
}

/// Keyside model name, keys across and down, by Elgato model code.
fn model(code: &str) -> Option<(&'static str, u8, u8)> {
    Some(match code {
        "20GAA9901" | "20GAA9902" | "20GBA9901" => ("Stream Deck", 5, 3),
        "20GAI9901" | "20GAI9902" => ("Stream Deck Mini", 3, 2),
        "20GAT9901" | "20GAT9902" => ("Stream Deck XL", 8, 4),
        "20GBJ9901" => ("Stream Deck Neo", 4, 2),
        "20GBD9901" => ("Stream Deck +", 4, 2),
        "20GBX9901" => ("Stream Deck + XL", 9, 4),
        _ => return None,
    })
}

fn text<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(Value::as_str).unwrap_or("")
}

fn num(v: &Value, k: &str) -> Option<f64> {
    let x = v.get(k)?;
    x.as_f64().or_else(|| x.as_str().and_then(|s| s.trim().parse().ok())).filter(|n| n.is_finite())
}

fn list<'a>(v: &'a Value, k: &str) -> &'a [Value] {
    v.get(k).and_then(Value::as_array).map_or(&[], Vec::as_slice)
}

fn obj<'a>(v: &'a Value, k: &str) -> Option<&'a Map<String, Value>> {
    v.get(k).and_then(Value::as_object)
}

fn parse_json(bytes: &[u8]) -> Option<Value> {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    serde_json::from_slice::<Value>(bytes).ok().filter(Value::is_object)
}

/// One spelling for a page id, whichever way the file wrote it: lowercase hex of the UUID.
fn id_key(s: &str) -> String {
    let s = s.trim();
    let s = s.strip_suffix(".sdProfile").unwrap_or(s);
    let hex: String = s.chars().filter(|c| *c != '-').collect();
    if hex.len() == 32 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return hex.to_ascii_lowercase();
    }
    // 26 digits hold 130 bits; the UUID is the first 128. A Z ends the name.
    if let Some(short) = s.strip_suffix('Z').filter(|t| t.len() == 26) {
        let mut bits = 0u128;
        for (i, c) in short.bytes().enumerate() {
            let Some(d) = SHORT_ID.iter().position(|&a| a == c) else { return s.to_ascii_lowercase() };
            bits = if i < 25 { bits << 5 | d as u128 } else { bits << 3 | (d as u128) >> 2 };
        }
        return format!("{bits:032x}");
    }
    s.to_ascii_lowercase()
}

/// Titles often pad with empty lines to push text around the image. Keyside keeps the words.
fn clean_title(s: &str) -> String {
    let lines: Vec<String> = s.lines().map(|l| l.chars().filter(|c| !c.is_control()).collect::<String>().trim().to_string()).filter(|l| !l.is_empty()).collect();
    lines.join("\n").chars().take(64).collect::<String>().trim_end().to_string()
}

fn one_line(s: &str, max: usize) -> Option<String> {
    crate::config::clean_name(&clean_title(s).replace('\n', " "), max)
}

fn position(s: &str) -> Option<(u8, u8)> {
    let (x, y) = s.split_once(',')?;
    Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
}

fn action_uuid(a: &Value) -> &str {
    match text(a, "UUID") {
        "" => obj(a, "Plugin").and_then(|p| p.get("UUID")).and_then(Value::as_str).unwrap_or(""),
        u => u,
    }
}

fn step(kind: &str, settings: Value) -> Action {
    Action { kind: kind.into(), settings: settings.as_object().cloned().unwrap_or_default(), review: false }
}

fn blank_key(id: String, x: u8, y: u8) -> Key {
    Key {
        id,
        x,
        y,
        w: None,
        h: None,
        kind: KeyKind::Button,
        label: String::new(),
        sub: None,
        icon: None,
        color: None,
        hold: None,
        repeat: None,
        actions: Vec::new(),
        off_actions: Vec::new(),
        target: None,
        feedback: Vec::new(),
        slider: None,
        widget: None,
        states: Vec::new(),
        follow: None,
        long_actions: Vec::new(),
        long_ms: None,
        double_actions: Vec::new(),
        double_ms: None,
        styles: Vec::new(),
    }
}

// Hotkeys: Elgato stores Qt's key code, the Windows key code and modifier flags.

fn qt_key(code: i64) -> Option<String> {
    const F1: i64 = 0x0100_0030;
    let named = match code {
        0x30..=0x39 | 0x41..=0x5A | 0x61..=0x7A => return Some(char::from(code as u8).to_ascii_lowercase().to_string()),
        0x27 | 0x2C | 0x2D | 0x2E | 0x2F | 0x3B | 0x3D | 0x5B | 0x5C | 0x5D | 0x60 => return Some(char::from(code as u8).to_string()),
        F1..=0x0100_0043 => return Some(format!("f{}", code - F1 + 1)),
        0x20 => "space",
        0x0100_0000 => "esc",
        0x0100_0001 => "tab",
        0x0100_0003 => "backspace",
        0x0100_0004 | 0x0100_0005 => "enter",
        0x0100_0006 => "insert",
        0x0100_0007 => "delete",
        0x0100_0010 => "home",
        0x0100_0011 => "end",
        0x0100_0012 => "left",
        0x0100_0013 => "up",
        0x0100_0014 => "right",
        0x0100_0015 => "down",
        0x0100_0016 => "pageup",
        0x0100_0017 => "pagedown",
        0x0100_0024 => "capslock",
        _ => return None,
    };
    Some(named.into())
}

fn vk_key(code: i64) -> Option<String> {
    let named = match code {
        0x30..=0x39 | 0x41..=0x5A => return Some(char::from(code as u8).to_ascii_lowercase().to_string()),
        0x70..=0x83 => return Some(format!("f{}", code - 0x6F)),
        0xBA => ";",
        0xBB => "=",
        0xBC => ",",
        0xBD => "-",
        0xBE => ".",
        0xBF => "/",
        0xC0 => "`",
        0xDB => "[",
        0xDC => "\\",
        0xDD => "]",
        0xDE => "'",
        0x08 => "backspace",
        0x09 => "tab",
        0x0D => "enter",
        0x14 => "capslock",
        0x1B => "esc",
        0x20 => "space",
        0x21 => "pageup",
        0x22 => "pagedown",
        0x23 => "end",
        0x24 => "home",
        0x25 => "left",
        0x26 => "up",
        0x27 => "right",
        0x28 => "down",
        0x2D => "insert",
        0x2E => "delete",
        _ => return None,
    };
    Some(named.into())
}

fn media_key(qt: i64, vk: i64) -> Option<&'static str> {
    Some(match (qt, vk) {
        (0x0100_0070, _) | (_, 0xAE) => "volume-down",
        (0x0100_0071, _) | (_, 0xAD) => "mute",
        (0x0100_0072, _) | (_, 0xAF) => "volume-up",
        (0x0100_0080 | 0x0100_0085 | 0x0100_0086, _) | (_, 0xB3) => "play",
        (0x0100_0081, _) | (_, 0xB2) => "stop",
        (0x0100_0082, _) | (_, 0xB1) => "prev",
        (0x0100_0083, _) | (_, 0xB0) => "next",
        _ => return None,
    })
}

enum Hotkey {
    Keys(String),
    Media(&'static str),
    Empty,
    Unknown,
}

fn hotkey(h: &Value) -> Hotkey {
    let qt = num(h, "QTKeyCode").map_or(QT_NO_KEY, |n| n as i64);
    let vk = num(h, "VKeyCode").map_or(-1, |n| n as i64);
    let bits = num(h, "KeyModifiers").map_or(0, |n| n as i64);
    let flag = |k: &str| h.get(k).and_then(Value::as_bool).unwrap_or(false);
    let mods: Vec<&str> = [
        ("ctrl", flag("KeyCtrl") || bits & 2 != 0),
        ("shift", flag("KeyShift") || bits & 1 != 0),
        ("alt", flag("KeyOption") || bits & 4 != 0),
        ("super", flag("KeyCmd") || bits & 8 != 0),
    ]
    .into_iter()
    .filter_map(|(m, on)| on.then_some(m))
    .collect();
    let no_key = (qt == QT_NO_KEY || qt <= 0) && vk <= 0;
    if no_key {
        return if mods.is_empty() { Hotkey::Empty } else { Hotkey::Keys(mods.join("+")) };
    }
    if mods.is_empty()
        && let Some(m) = media_key(qt, vk)
    {
        return Hotkey::Media(m);
    }
    match qt_key(qt).or_else(|| vk_key(vk)) {
        Some(k) => Hotkey::Keys(mods.into_iter().map(str::to_string).chain([k]).collect::<Vec<_>>().join("+")),
        None => Hotkey::Unknown,
    }
}

/// Words of a command line. Double quotes keep spaces inside one word.
fn words(s: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for c in s.trim().chars() {
        match c {
            '"' => quoted = !quoted,
            c if c.is_whitespace() && !quoted => {
                if !cur.is_empty() {
                    parts.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        parts.push(cur);
    }
    parts
}

/// Splits `"C:\Program Files\x.exe" -a b` into the program and its arguments.
fn split_command(s: &str) -> (String, Vec<String>) {
    let mut it = words(s).into_iter();
    (it.next().unwrap_or_default(), it.collect())
}

/// An http or https link, with https:// added when the address has no scheme.
fn web_link(s: &str) -> Option<String> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let has_scheme = s.split('/').next().is_some_and(|head| head.contains(':'));
    let u = if has_scheme { s.to_string() } else { format!("https://{s}") };
    let parsed = url::Url::parse(&u).ok()?;
    (matches!(parsed.scheme(), "http" | "https") && u.len() <= 2048).then_some(u)
}

fn launch(path: &str, args: &[String]) -> Option<Action> {
    let path = path.trim();
    if path.is_empty() || path.len() > 1024 {
        return None;
    }
    let mut s = json!({ "path": path });
    if !args.is_empty() {
        s["args"] = Value::String(args.join("\n").chars().take(4096).collect());
    }
    Some(step("core.launch", s))
}

/// A page as found in the file.
struct RawPage {
    alias: String,
    dir: String,
    name: String,
    keys: Map<String, Value>,
    dials: Map<String, Value>,
}

impl RawPage {
    fn read(alias: String, dir: String, m: &Value, v1: bool) -> Self {
        let (mut keys, mut dials) = (Map::new(), Map::new());
        if v1 {
            keys = obj(m, "Actions").cloned().unwrap_or_default();
        }
        for c in list(m, "Controllers") {
            let actions = obj(c, "Actions").cloned().unwrap_or_default();
            match text(c, "Type") {
                "Encoder" => dials.extend(actions),
                "Keypad" | "" => keys.extend(actions),
                _ => {}
            }
        }
        Self { alias, dir, name: text(m, "Name").to_string(), keys, dials }
    }

    /// Pages that folder keys on this page open.
    fn folders(&self) -> impl Iterator<Item = (String, String)> + '_ {
        self.keys.values().filter(|a| action_uuid(a).eq_ignore_ascii_case(OPEN_CHILD)).filter_map(|a| {
            let target = obj(a, "Settings").and_then(|s| s.get("ProfileUUID")).and_then(Value::as_str).filter(|t| !t.is_empty())?;
            let title = list(a, "States").iter().map(|s| text(s, "Title")).find(|t| !t.trim().is_empty()).unwrap_or("");
            Some((id_key(target), title.to_string()))
        })
    }
}

/// Collected while converting one key, counted once per key in the report.
#[derive(Default)]
struct Notes {
    plugins: BTreeMap<String, String>,
    unsupported: BTreeMap<String, String>,
    trimmed: bool,
}

impl Notes {
    fn unsupported(&mut self, a: &Value) {
        let uuid = action_uuid(a);
        let name = one_line(text(a, "Name"), 60)
            .or_else(|| obj(a, "Plugin").and_then(|p| p.get("Name")).and_then(Value::as_str).and_then(|n| one_line(n, 60)))
            .unwrap_or_else(|| uuid.to_string());
        self.unsupported.insert(uuid.to_ascii_lowercase(), name);
    }
}

/// Where a key sits: the page folder in the zip and, for top-level pages, the page number.
struct Here<'a> {
    dir: &'a str,
    top: Option<usize>,
}

struct Run<'a> {
    env: &'a mut dyn Env,
    files: &'a HashMap<String, Vec<u8>>,
    v1: bool,
    /// Keyside ids of the top-level pages, in order.
    pages: Vec<String>,
    /// Keyside page id by file page id.
    ids: HashMap<String, String>,
    images: HashMap<String, Option<String>>,
    report: Report,
    plugins: BTreeMap<String, (String, usize)>,
    unsupported: BTreeMap<String, (String, usize)>,
}

impl Run<'_> {
    /// Stores the first of these paths that the zip has. None when there is none or it can't be used.
    fn image(&mut self, paths: &[String]) -> Option<String> {
        let Some(path) = paths.iter().find(|p| self.files.contains_key(*p)) else {
            self.report.images += 1;
            return None;
        };
        if let Some(done) = self.images.get(path) {
            return done.clone();
        }
        let name = path.rsplit('/').next().unwrap_or("image");
        let stored = self.env.store_image(name, &self.files[path]);
        if stored.is_none() {
            self.report.images += 1;
        }
        self.images.insert(path.clone(), stored.clone());
        stored
    }

    /// Title and image of one state, or of the current one when `state` is None.
    fn look(&mut self, here: &Here, pos: &str, a: &Value, state: Option<usize>) -> (String, Option<String>) {
        let states = list(a, "States");
        let idx = state.unwrap_or_else(|| num(a, "State").unwrap_or(0.0).max(0.0) as usize);
        let cur = states.get(idx);
        let pick = |f: &str| -> String {
            let own = cur.map(|s| text(s, f)).filter(|v| !v.trim().is_empty());
            let any = || if state.is_some() { None } else { states.iter().map(|s| text(s, f)).find(|v| !v.trim().is_empty()) };
            own.or_else(any).unwrap_or("").to_string()
        };
        let show = cur.and_then(|s| s.get("ShowTitle")).and_then(Value::as_bool).unwrap_or(true);
        let label = if show { clean_title(&pick("Title")) } else { String::new() };
        let mut image = pick("Image");
        if image.is_empty() {
            image = obj(a, "Encoder").and_then(|e| e.get("Icon")).and_then(Value::as_str).unwrap_or("").to_string();
        }
        let dir = here.dir;
        let icon = if !image.is_empty() {
            let mut paths = vec![format!("{dir}/{image}")];
            if self.v1 {
                paths.push(format!("{dir}/{pos}/CustomImages/{image}"));
            }
            self.image(&paths)
        } else if self.v1 {
            // Older versions name custom images after the state: stateN.png.
            let prefix = format!("{dir}/{pos}/CustomImages/state{idx}.");
            let mut found: Vec<String> = self.files.keys().filter(|n| n.starts_with(&prefix)).cloned().collect();
            found.sort();
            if found.is_empty() { None } else { self.image(&found) }
        } else {
            None
        };
        (label, icon)
    }

    fn plugin_step(&mut self, a: &Value, notes: &mut Notes) -> Option<Action> {
        let uuid = action_uuid(a);
        let installed = self.env.plugin_of(uuid);
        let named = obj(a, "Plugin").and_then(|p| p.get("UUID")).and_then(Value::as_str).filter(|p| !p.is_empty()).map(str::to_string);
        let plugin = installed.clone().or(named).or_else(|| uuid.rsplit_once('.').map(|(p, _)| p.to_string())).unwrap_or_default();
        let kind = format!("{}{plugin}/{uuid}", crate::streamdeck::PREFIX);
        let settings = obj(a, "Settings").cloned().unwrap_or_default();
        let size = serde_json::to_vec(&settings).map_or(usize::MAX, |v| v.len());
        if !crate::streamdeck::manifest::valid_uuid(&plugin, 60) || !model::valid_action_type(&kind) || size > model::MAX_ACTION_SETTINGS {
            notes.unsupported(a);
            return None;
        }
        if installed.is_none() {
            let name = obj(a, "Plugin").and_then(|p| p.get("Name")).and_then(Value::as_str).and_then(|n| one_line(n, 60)).unwrap_or_else(|| plugin.clone());
            notes.plugins.entry(plugin.to_ascii_lowercase()).or_insert(name);
        }
        Some(Action { kind, settings, review: false })
    }

    /// Steps of a multi action list, flattened.
    fn children(&mut self, items: &[Value], here: &Here, depth: usize, notes: &mut Notes) -> Vec<Action> {
        let mut out = Vec::new();
        for item in items {
            // Lists of steps are usually wrapped as {"Actions": [...]}; a bare action works too.
            if item.get("UUID").is_some() || item.get("Plugin").is_some() {
                out.extend(self.steps(item, here, depth + 1, notes));
            } else {
                for a in list(item, "Actions") {
                    out.extend(self.steps(a, here, depth + 1, notes));
                }
            }
        }
        out
    }

    /// Keyside steps for one Elgato action. Empty when there are none.
    fn steps(&mut self, a: &Value, here: &Here, depth: usize, notes: &mut Notes) -> Vec<Action> {
        let uuid = action_uuid(a).to_ascii_lowercase();
        let empty = Value::Object(Map::new());
        let s = a.get("Settings").filter(|s| s.is_object()).unwrap_or(&empty);
        let one = |x: Option<Action>| x.into_iter().collect::<Vec<_>>();
        match uuid.as_str() {
            "com.elgato.streamdeck.system.hotkey" => match list(s, "Hotkeys").first().map(hotkey).unwrap_or(Hotkey::Empty) {
                Hotkey::Keys(k) => vec![step("core.keys", json!({ "keys": k }))],
                Hotkey::Media(m) => vec![step("core.media", json!({ "key": m }))],
                Hotkey::Empty => Vec::new(),
                Hotkey::Unknown => {
                    notes.unsupported(a);
                    Vec::new()
                }
            },
            "com.elgato.streamdeck.system.text" => {
                let mut t = text(s, "pastedText").to_string();
                if s.get("isSendingEnter").and_then(Value::as_bool).unwrap_or(false) {
                    t.push('\n');
                }
                let t: String = t.chars().take(2000).collect();
                if t.is_empty() { Vec::new() } else { vec![step("core.text", json!({ "text": t }))] }
            }
            "com.elgato.streamdeck.system.website" => {
                let raw = [text(s, "path"), text(s, "url")].into_iter().find(|u| !u.trim().is_empty()).unwrap_or("");
                match web_link(raw) {
                    Some(url) => vec![step("core.open", json!({ "url": url }))],
                    None if raw.is_empty() => Vec::new(),
                    // Other schemes open apps, like steam:// or microsoft-edge:.
                    None => {
                        notes.unsupported(a);
                        Vec::new()
                    }
                }
            }
            "com.elgato.streamdeck.system.open" => {
                let raw = text(s, "path").trim();
                if raw.starts_with("http://") || raw.starts_with("https://") {
                    return one(web_link(raw).map(|url| step("core.open", json!({ "url": url }))));
                }
                let (path, args) = split_command(raw);
                one(launch(&path, &args))
            }
            "com.elgato.streamdeck.system.openapp" => {
                let path = [text(s, "exec"), text(s, "bundle_path")].into_iter().find(|p| !p.trim().is_empty()).unwrap_or("");
                one(launch(path, &words(text(s, "args"))))
            }
            "com.elgato.streamdeck.system.multimedia" => {
                // The order of Elgato's list. No value means Play / Pause.
                const KEYS: [&str; 7] = ["prev", "play", "next", "stop", "mute", "volume-up", "volume-down"];
                match KEYS.get(num(s, "actionIdx").unwrap_or(1.0).max(0.0) as usize) {
                    Some(k) => vec![step("core.media", json!({ "key": k }))],
                    None => {
                        notes.unsupported(a);
                        Vec::new()
                    }
                }
            }
            "com.elgato.streamdeck.system.delay" | "com.elgato.streamdeck.multiactions.delay" => {
                let ms = num(s, "delay").or_else(|| num(s, "Delay")).or_else(|| num(s, "duration")).unwrap_or(1000.0).clamp(0.0, 60_000.0);
                vec![step("core.wait", json!({ "ms": ms as u64 }))]
            }
            "com.elgato.streamdeck.multiactions" | "com.elgato.streamdeck.multiactions.routine" | "com.elgato.streamdeck.system.multiaction" | SWITCH => {
                if depth >= MAX_DEPTH {
                    notes.unsupported(a);
                    return Vec::new();
                }
                // A switch nested in a multi action runs its first list.
                let lists = list(a, "Actions");
                let first = if lists.first().is_some_and(|l| l.get("Actions").is_some()) { &lists[..1] } else { lists };
                self.children(first, here, depth, notes)
            }
            "com.elgato.streamdeck.page.next" | "com.elgato.streamdeck.page.previous" => {
                let (Some(i), n) = (here.top, self.pages.len()) else {
                    notes.unsupported(a);
                    return Vec::new();
                };
                let to = if uuid.ends_with("next") { (i + 1) % n } else { (i + n - 1) % n };
                vec![step("core.page", json!({ "page": self.pages[to] }))]
            }
            "com.elgato.streamdeck.page.goto" => {
                // PageIndex counts from 1.
                let at = num(s, "PageIndex").map(|n| n as i64 - 1);
                match at.and_then(|i| usize::try_from(i).ok()).and_then(|i| self.pages.get(i)) {
                    Some(p) => vec![step("core.page", json!({ "page": p }))],
                    None => {
                        notes.unsupported(a);
                        Vec::new()
                    }
                }
            }
            OPEN_CHILD => {
                let target = text(s, "ProfileUUID");
                match self.ids.get(&id_key(target)) {
                    Some(p) => vec![step("core.page", json!({ "page": p }))],
                    None => {
                        notes.unsupported(a);
                        Vec::new()
                    }
                }
            }
            u if u.starts_with(BUILT_IN) => {
                notes.unsupported(a);
                Vec::new()
            }
            "" => Vec::new(),
            _ => one(self.plugin_step(a, notes)),
        }
    }

    fn trim(list: &mut Vec<Action>, notes: &mut Notes) {
        if list.len() > MAX_ACTIONS {
            list.truncate(MAX_ACTIONS);
            notes.trimmed = true;
        }
    }

    fn key(&mut self, here: &Here, pos: &str, at: (u8, u8), a: &Value, dial: bool) -> Key {
        let id = if dial { format!("d{}", at.0) } else { format!("k{}_{}", at.0, at.1) };
        let mut key = blank_key(id, at.0, at.1);
        let mut notes = Notes::default();
        let (label, icon) = self.look(here, pos, a, None);
        key.label = label;
        key.icon = icon;
        let uuid = action_uuid(a).to_ascii_lowercase();
        let lists = list(a, "Actions");
        if dial {
            // Keyside dials press their key and move sliders. Elgato built-ins on dials turn.
            if uuid.starts_with(BUILT_IN) {
                notes.unsupported(a);
            } else {
                key.actions = self.plugin_step(a, &mut notes).into_iter().collect();
            }
        } else if uuid == OPEN_CHILD {
            let target = obj(a, "Settings").and_then(|s| s.get("ProfileUUID")).and_then(Value::as_str).unwrap_or("");
            match self.ids.get(&id_key(target)) {
                Some(p) => {
                    key.kind = KeyKind::Folder;
                    key.target = Some(p.clone());
                }
                None => notes.unsupported(a),
            }
        } else if uuid == BACK {
            key.kind = KeyKind::Back;
        } else if uuid == SWITCH && lists.len() >= 2 {
            key.kind = KeyKind::Multi;
            for (i, l) in lists.iter().take(MAX_STATES).enumerate() {
                let (label, icon) = self.look(here, pos, a, Some(i));
                let mut steps = self.children(std::slice::from_ref(l), here, 0, &mut notes);
                Self::trim(&mut steps, &mut notes);
                key.states.push(KeyState { label, sub: None, icon, color: None, actions: steps, value: None });
            }
            if key.label.is_empty() {
                key.label = key.states[0].label.clone();
            }
        } else {
            key.actions = self.steps(a, here, 0, &mut notes);
            Self::trim(&mut key.actions, &mut notes);
        }
        let plain = matches!(key.kind, KeyKind::Button | KeyKind::Multi);
        if key.label.is_empty() && key.icon.is_none() && plain {
            key.label = one_line(text(a, "Name"), 64).unwrap_or_default();
        }
        for (uuid, name) in notes.plugins {
            self.plugins.entry(uuid).or_insert((name, 0)).1 += 1;
        }
        for (uuid, name) in notes.unsupported {
            self.unsupported.entry(uuid).or_insert((name, 0)).1 += 1;
        }
        self.report.trimmed += usize::from(notes.trimmed);
        self.report.keys += 1;
        key
    }
}

/// The profile folder: the shallowest `*.sdProfile` with a manifest. One per file.
fn find_root(files: &HashMap<String, Vec<u8>>) -> Result<String> {
    let mut roots: Vec<&str> = files
        .keys()
        .filter_map(|n| n.strip_suffix("/manifest.json"))
        .filter(|dir| dir.ends_with(".sdProfile"))
        .filter(|dir| !dir.rsplit_once('/').is_some_and(|(above, _)| above.split('/').any(|c| c.ends_with(".sdProfile"))))
        .collect();
    roots.sort();
    match roots.as_slice() {
        [] => bail!("this is not a Stream Deck profile"),
        [one] => Ok(one.to_string()),
        _ => bail!("this file holds several profiles. Export them from the Stream Deck app one at a time"),
    }
}

/// Reads a .streamDeckProfile file into a Keyside profile. The profile still needs
/// `share::adopt` for its id, review flags and checks.
pub fn import(bytes: &[u8], env: &mut dyn Env) -> Result<Imported> {
    if bytes.len() > MAX_FILE {
        bail!("the profile is larger than {} MB", MAX_FILE / (1024 * 1024));
    }
    let files: HashMap<String, Vec<u8>> = read_zip(bytes, MAX_ENTRIES, MAX_ENTRY, MAX_UNPACKED)?.into_iter().filter(|(n, _)| !n.starts_with("__MACOSX/")).collect();
    let root = find_root(&files)?;
    let Some(manifest) = files.get(&format!("{root}/manifest.json")).and_then(|b| parse_json(b)) else {
        bail!("the profile manifest is damaged");
    };
    let v1 = manifest.get("Pages").is_none();
    let device = manifest.get("Device").cloned().unwrap_or(Value::Null);
    let code = [text(&device, "Model"), text(&manifest, "DeviceModel")].into_iter().find(|c| !c.is_empty()).unwrap_or("");

    // Every page and folder in the file, by page id.
    let mut raw: Vec<RawPage> = Vec::new();
    if v1 {
        raw.push(RawPage::read(id_key(root.rsplit('/').next().unwrap_or(&root)), root.clone(), &manifest, true));
    }
    let mut names: Vec<&String> = files.keys().filter(|n| n.starts_with(&format!("{root}/Profiles/")) && n.ends_with("/manifest.json")).collect();
    names.sort();
    for n in names {
        let dir = &n[..n.len() - "/manifest.json".len()];
        let last = dir.rsplit('/').next().unwrap_or(dir);
        let is_page = if v1 { last.ends_with(".sdProfile") } else { dir.matches('/').count() == root.matches('/').count() + 2 };
        if let Some(m) = is_page.then(|| parse_json(&files[n])).flatten() {
            raw.push(RawPage::read(id_key(last), dir.to_string(), &m, v1));
        }
    }

    // Top-level pages in order, then the folders they open, and folders in those.
    let listed: Vec<String> = if v1 {
        raw.first().map(|p| vec![p.alias.clone()]).unwrap_or_default()
    } else {
        let pages = manifest.get("Pages").unwrap_or(&Value::Null);
        let ids = if pages.is_array() { pages.as_array().map_or(&[][..], Vec::as_slice) } else { list(pages, "Pages") };
        ids.iter().filter_map(Value::as_str).map(id_key).filter(|k| raw.iter().any(|p| &p.alias == k)).collect()
    };
    let mut order: Vec<String> = Vec::new();
    for k in listed {
        if !order.contains(&k) {
            order.push(k);
        }
    }
    if order.is_empty() {
        let opened: HashSet<String> = raw.iter().flat_map(|p| p.folders().map(|(k, _)| k)).collect();
        order = raw.iter().filter(|p| !opened.contains(&p.alias)).map(|p| p.alias.clone()).collect();
    }
    let top = order.len();
    let mut folder_names: HashMap<String, String> = HashMap::new();
    let mut i = 0;
    while i < order.len() {
        let Some(p) = raw.iter().find(|p| p.alias == order[i]) else { break };
        for (k, title) in p.folders() {
            if !order.contains(&k) && raw.iter().any(|p| p.alias == k) {
                folder_names.insert(k.clone(), title);
                order.push(k);
            }
        }
        if order.len() > MAX_PAGES {
            bail!("the profile has more than {MAX_PAGES} pages and folders");
        }
        i += 1;
    }

    // Grid: the model's, grown to fit keys placed further out.
    let (model_name, mut cols, mut rows) = model(code).map_or(("", 1, 1), |(n, c, r)| (n, c, r));
    for p in raw.iter().filter(|p| order.contains(&p.alias)) {
        for (x, y) in p.keys.keys().filter_map(|s| position(s)) {
            if x < MAX_GRID && y < MAX_GRID {
                cols = cols.max(x + 1);
                rows = rows.max(y + 1);
            }
        }
    }
    let deck = (!model_name.is_empty()).then(|| env.deck(model_name)).flatten().filter(|d| d.cols >= cols && d.rows >= rows);
    if let Some(d) = &deck {
        (cols, rows) = (d.cols, d.rows);
    }
    let dials = deck.as_ref().map_or(0, DeckLink::dials);
    let dial_row = u8::from(dials > 0 && rows < MAX_GRID);

    let ids: HashMap<String, String> = order.iter().enumerate().map(|(i, k)| (k.clone(), if i < top { format!("p{}", i + 1) } else { format!("f{}", i - top + 1) })).collect();
    let mut run = Run {
        env,
        files: &files,
        v1,
        pages: order[..top].iter().map(|k| ids[k].clone()).collect(),
        ids,
        images: HashMap::new(),
        report: Report { deck: deck.as_ref().map(|d| d.model.clone()), ..Report::default() },
        plugins: BTreeMap::new(),
        unsupported: BTreeMap::new(),
    };

    let mut pages = Vec::new();
    for (i, alias) in order.iter().enumerate() {
        let p = raw.iter().find(|p| &p.alias == alias).expect("listed pages exist");
        let here = Here { dir: &p.dir, top: (i < top).then_some(i) };
        let mut entries: Vec<((u8, u8), &String, &Value)> = p.keys.iter().filter_map(|(pos, a)| Some((position(pos)?, pos, a))).collect();
        entries.sort_by_key(|((x, y), _, _)| (*y, *x));
        let mut keys = Vec::new();
        for ((x, y), pos, a) in entries {
            if x < cols && y < rows && a.is_object() {
                keys.push(run.key(&here, pos, (x, y), a, false));
            }
        }
        let mut dial_keys: Vec<(u8, &String, &Value)> = p.dials.iter().filter_map(|(pos, a)| Some((position(pos)?.0, pos, a))).collect();
        dial_keys.sort_by_key(|(x, _, _)| *x);
        for (x, pos, a) in dial_keys {
            if dial_row == 1 && x < dials && a.is_object() {
                keys.push(run.key(&here, pos, (x, rows), a, true));
            } else {
                run.report.dials += 1;
            }
        }
        let name = if i < top {
            one_line(&p.name, 40).unwrap_or_else(|| format!("Page {}", i + 1))
        } else {
            folder_names.get(alias).and_then(|t| one_line(t, 40)).or_else(|| one_line(&p.name, 40)).unwrap_or_else(|| "Folder".into())
        };
        let id = run.ids[alias].clone();
        pages.push(Page { id, name, subtitle: None, folder: i >= top, cols, rows: rows + dial_row, keys, variants: Default::default(), open_when: None });
    }
    if pages.is_empty() {
        pages.push(Page {
            id: "p1".into(),
            name: "Page 1".into(),
            subtitle: None,
            folder: false,
            cols,
            rows: rows + dial_row,
            keys: Vec::new(),
            variants: Default::default(),
            open_when: None,
        });
    }

    let missing = |m: BTreeMap<String, (String, usize)>| {
        let mut v: Vec<Missing> = m.into_iter().map(|(uuid, (name, keys))| Missing { name, uuid, keys }).collect();
        v.sort_by_key(|a| a.name.to_lowercase());
        v
    };
    let mut report = std::mem::take(&mut run.report);
    report.plugins = missing(std::mem::take(&mut run.plugins));
    report.unsupported = missing(std::mem::take(&mut run.unsupported));
    let name = one_line(text(&manifest, "Name"), 40).unwrap_or_else(|| "Stream Deck".into());
    let profile = Profile { id: "imported".into(), name, apps: Vec::new(), home_page: pages[0].id.clone(), pages, deck };
    Ok(Imported { profile, report })
}

/// The import as the editor uses it, against this companion: images go to the asset store,
/// plugin actions are matched to installed Stream Deck plugins.
pub struct AppEnv<'a> {
    pub app: &'a crate::app::App,
}

impl Env for AppEnv<'_> {
    fn plugin_of(&self, action: &str) -> Option<String> {
        self.app.streamdeck.plugin_with_action(action)
    }

    fn deck(&self, model: &str) -> Option<DeckLink> {
        let e = self.app.engine.lock();
        let decks: Vec<&DeckLink> = e.layout().profiles.iter().filter_map(|p| p.deck.as_ref()).filter(|d| d.model == model).collect();
        let first = decks.first()?;
        decks.iter().all(|d| d.serial == first.serial).then(|| (*first).clone())
    }

    fn store_image(&mut self, name: &str, bytes: &[u8]) -> Option<String> {
        let m = self.app.assets.put(name, bytes).ok()?;
        (m.kind == crate::assets::Kind::Image).then(|| format!("asset:{}", m.id))
    }
}

/// Reads the file and prepares the profile for the editor to add.
pub fn import_for(app: &crate::app::App, bytes: &[u8]) -> Result<Imported> {
    let mut env = AppEnv { app };
    let mut done = import(bytes, &mut env)?;
    done.profile = crate::share::adopt(done.profile).map_err(|e| anyhow::anyhow!("the profile could not be converted: {e}"))?;
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n0000";

    struct Fake {
        installed: Vec<(&'static str, &'static str)>,
        deck: Option<DeckLink>,
        stored: Vec<String>,
    }

    impl Fake {
        fn new() -> Self {
            Self { installed: Vec::new(), deck: None, stored: Vec::new() }
        }
    }

    impl Env for Fake {
        fn plugin_of(&self, action: &str) -> Option<String> {
            self.installed.iter().find(|(_, a)| *a == action).map(|(p, _)| p.to_string())
        }
        fn deck(&self, model: &str) -> Option<DeckLink> {
            self.deck.clone().filter(|d| d.model == model)
        }
        fn store_image(&mut self, name: &str, bytes: &[u8]) -> Option<String> {
            if crate::assets::sniff(bytes).is_none() || bytes.len() > crate::assets::MAX_IMAGE {
                return None;
            }
            self.stored.push(name.to_string());
            Some(format!("asset:{:016x}", self.stored.len()))
        }
    }

    fn zip(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = std::io::Cursor::new(Vec::new());
        let mut z = zip::ZipWriter::new(&mut out);
        for (n, b) in files {
            z.start_file(*n, zip::write::SimpleFileOptions::default()).unwrap();
            z.write_all(b).unwrap();
        }
        z.finish().unwrap();
        out.into_inner()
    }

    /// The short form some exports use for page folders.
    fn short_id(uuid: &str) -> String {
        let bits = u128::from_str_radix(&uuid.replace('-', ""), 16).unwrap();
        let mut s: String = (0..25).map(|i| SHORT_ID[((bits >> (123 - 5 * i)) & 31) as usize] as char).collect();
        s.push(SHORT_ID[((bits & 7) << 2) as usize] as char);
        s.push('Z');
        s
    }

    fn hotkey_action(ctrl: bool, shift: bool, qt: i64, vk: i64) -> Value {
        let empty =
            json!({ "KeyCmd": false, "KeyCtrl": false, "KeyModifiers": 0, "KeyOption": false, "KeyShift": false, "NativeCode": 146, "QTKeyCode": QT_NO_KEY, "VKeyCode": -1 });
        let mods = i64::from(ctrl) * 2 + i64::from(shift);
        json!({ "Name": "Hotkey", "UUID": "com.elgato.streamdeck.system.hotkey", "State": 0,
            "States": [{ "Title": "New\n\n\ntab", "Image": "Images/HOT.png", "ShowTitle": true, "FontSize": 12 }],
            "Settings": { "Coalesce": true, "Hotkeys": [
                { "KeyCmd": false, "KeyCtrl": ctrl, "KeyModifiers": mods, "KeyOption": false, "KeyShift": shift, "NativeCode": vk, "QTKeyCode": qt, "VKeyCode": vk },
                empty, empty, empty] } })
    }

    const PROFILE: &str = "Profiles/671B2ED2-7DD6-42F8-B994-C0A56BBFEB01.sdProfile";
    const PAGE1: &str = "43fd45d1-36cb-4f4d-ac2d-06e1dff4fb08";
    const PAGE2: &str = "5eb3b727-8b84-40b8-9af5-0c2522d64aaa";
    const FOLDER: &str = "b3155c01-9bd7-42fc-9676-05d857c43f99";

    /// A Stream Deck 6 style export: two pages, one folder.
    fn current_format(model: &str) -> Vec<u8> {
        let manifest = json!({ "Device": { "Model": model, "UUID": "" }, "Name": "Streaming", "Version": "3.0",
            "Pages": { "Current": PAGE1, "Default": "81c2a4d1-c2b0-4893-a9e0-ef70da059513", "Pages": [PAGE1, PAGE2] } });
        let page1 = json!({ "Name": "", "Controllers": [
            { "Type": "Keypad", "Actions": {
                "0,0": hotkey_action(true, true, 0x54, 0x54),
                "1,0": { "Name": "Create Folder", "UUID": OPEN_CHILD, "Plugin": { "Name": "Create Folder", "UUID": OPEN_CHILD, "Version": "1.0" },
                         "Settings": { "ProfileUUID": FOLDER }, "State": 0, "States": [{ "Title": "Apps" }] },
                "2,0": { "Name": "Multi Action", "UUID": "com.elgato.streamdeck.multiactions.routine", "State": 0, "States": [{ "Title": "Go live" }], "Settings": {},
                         "Actions": [{ "Actions": [
                            { "UUID": "com.elgato.streamdeck.system.text", "Settings": { "pastedText": "hi", "isSendingEnter": true } },
                            { "UUID": "com.elgato.streamdeck.multiactions.delay", "Settings": { "delay": 250 } },
                            { "UUID": "com.elgato.obsstudio.scene", "Plugin": { "Name": "OBS Studio", "UUID": "com.elgato.obsstudio", "Version": "2" },
                              "Settings": { "sceneName": "Live" } } ] }] },
                "3,0": { "Name": "Brightness", "UUID": "com.elgato.streamdeck.system.keybrightness", "Settings": { "actionIdx": 0 }, "State": 0,
                         "States": [{ "Image": "Images/BRIGHT.svg" }] },
                "4,2": { "Name": "Next Page", "UUID": "com.elgato.streamdeck.page.next", "Settings": {}, "State": 0, "States": [{}] },
                "0,1": { "Name": "Website", "UUID": "com.elgato.streamdeck.system.website", "Settings": { "openInBrowser": true, "path": "twitch.tv" }, "State": 0, "States": [{}] },
                "1,1": { "Name": "Multimedia", "UUID": "com.elgato.streamdeck.system.multimedia", "Settings": { "actionIdx": 2 }, "State": 0, "States": [{}] },
                "2,1": { "Name": "Mute", "UUID": "com.elgato.discord.mute", "Plugin": { "Name": "Discord", "UUID": "com.elgato.discord", "Version": "1" },
                         "Settings": { "mode": 1 }, "State": 1, "States": [{ "Title": "Muted" }, { "Title": "Live" }] }
            } },
            { "Type": "Encoder", "Actions": { "0,0": { "Name": "Volume", "UUID": "com.elgato.volume-controller.output", "Plugin": { "Name": "Volume Controller", "UUID": "com.elgato.volume-controller" },
                "Settings": {}, "State": 0, "States": [{ "Title": "Volume" }] } } }
        ] });
        let page2 = json!({ "Controllers": [{ "Type": "Keypad", "Actions": {
            "0,0": { "UUID": "com.elgato.streamdeck.page.previous", "Settings": {}, "States": [{ "Title": "Back to 1" }] } } }] });
        let folder = json!({ "Controllers": [{ "Type": "Keypad", "Actions": {
            "0,0": { "Name": "Parent Folder", "UUID": BACK, "Settings": {}, "State": 0, "States": [{}] },
            "1,0": { "Name": "Open", "UUID": "com.elgato.streamdeck.system.open", "Settings": { "path": "\"C:\\Program Files\\App\\app.exe\" --fast" }, "State": 0, "States": [{ "Image": "Images/APP.png" }] },
            "2,0": { "Name": "Switch", "UUID": SWITCH, "Settings": {}, "State": 0, "States": [{ "Title": "Cam on", "Image": "Images/ON.png" }, { "Title": "Cam off" }],
                     "Actions": [{ "Actions": [{ "UUID": "com.elgato.streamdeck.system.hotkey", "Settings": { "Hotkeys": [{ "KeyCtrl": true, "QTKeyCode": 0x0100_0031, "VKeyCode": 0x71 }] } }] },
                                 { "Actions": [{ "UUID": "com.elgato.streamdeck.system.hotkey", "Settings": { "Hotkeys": [{ "KeyCtrl": true, "QTKeyCode": 0x0100_0032, "VKeyCode": 0x72 }] } }] }] }
        } }] });
        let f = |s: String| s;
        zip(&[
            ("package.json", br#"{"AppVersion":"7.2.1","DeviceModel":"20GBA9901","FormatVersion":1}"#),
            (&f(format!("{PROFILE}/manifest.json")), manifest.to_string().as_bytes()),
            (&f(format!("{PROFILE}/Profiles/{}/manifest.json", PAGE1.to_uppercase())), page1.to_string().as_bytes()),
            (&f(format!("{PROFILE}/Profiles/{}/Images/HOT.png", PAGE1.to_uppercase())), PNG),
            (&f(format!("{PROFILE}/Profiles/{}/Images/BRIGHT.svg", PAGE1.to_uppercase())), b"<svg xmlns='http://www.w3.org/2000/svg'/>"),
            // Some exports name page folders with the short form.
            (&f(format!("{PROFILE}/Profiles/{}/manifest.json", short_id(PAGE2))), page2.to_string().as_bytes()),
            (&f(format!("{PROFILE}/Profiles/{}/manifest.json", FOLDER.to_uppercase())), folder.to_string().as_bytes()),
            (&f(format!("{PROFILE}/Profiles/{}/Images/APP.png", FOLDER.to_uppercase())), b"MZ not an image"),
            (&f(format!("{PROFILE}/Profiles/{}/Images/ON.png", FOLDER.to_uppercase())), b"\x89PNG\r\n\x1a\n1111"),
            // The default page, on no list and opened by no folder, stays out.
            (&f(format!("{PROFILE}/Profiles/81C2A4D1-C2B0-4893-A9E0-EF70DA059513/manifest.json")), br#"{"Controllers":[{"Type":"Keypad","Actions":null}]}"#),
        ])
    }

    fn key<'a>(p: &'a Profile, page: &str, id: &str) -> &'a Key {
        p.pages.iter().find(|pg| pg.id == page).unwrap().keys.iter().find(|k| k.id == id).unwrap_or_else(|| panic!("no key {id} on {page}"))
    }

    #[test]
    fn reads_the_current_format() {
        let mut env = Fake::new();
        let done = import(&current_format("20GBA9901"), &mut env).unwrap();
        let p = &done.profile;
        assert_eq!(p.name, "Streaming");
        let ids: Vec<(&str, bool, u8, u8)> = p.pages.iter().map(|pg| (pg.id.as_str(), pg.folder, pg.cols, pg.rows)).collect();
        assert_eq!(ids, [("p1", false, 5, 3), ("p2", false, 5, 3), ("f1", true, 5, 3)]);
        assert_eq!(p.pages[2].name, "Apps", "folders take the title of their key");
        assert!(p.deck.is_none());

        let hot = key(p, "p1", "k0_0");
        assert_eq!(hot.actions[0].kind, "core.keys");
        assert_eq!(hot.actions[0].settings["keys"], "ctrl+shift+t");
        assert_eq!(hot.label, "New\ntab");
        assert!(hot.icon.as_deref().is_some_and(|i| i.starts_with("asset:")), "image carried over");

        let folder = key(p, "p1", "k1_0");
        assert_eq!((folder.kind, folder.target.as_deref()), (KeyKind::Folder, Some("f1")));
        assert_eq!(key(p, "f1", "k0_0").kind, KeyKind::Back);

        let steps = &key(p, "p1", "k2_0").actions;
        assert_eq!(steps.iter().map(|a| a.kind.as_str()).collect::<Vec<_>>(), ["core.text", "core.wait", "sd.com.elgato.obsstudio/com.elgato.obsstudio.scene"]);
        assert_eq!(steps[0].settings["text"], "hi\n");
        assert_eq!(steps[1].settings["ms"], 250);
        assert_eq!(steps[2].settings["sceneName"], "Live", "plugin settings carried over");

        assert_eq!(key(p, "p1", "k0_1").actions[0].settings["url"], "https://twitch.tv");
        assert_eq!(key(p, "p1", "k1_1").actions[0].settings["key"], "next");
        assert_eq!(key(p, "p1", "k4_2").actions[0].settings["page"], "p2");
        assert_eq!(key(p, "p2", "k0_0").actions[0].settings["page"], "p1", "short page names resolve");
        let discord = key(p, "p1", "k2_1");
        assert_eq!(discord.label, "Live", "the current state shows");
        assert_eq!(discord.actions[0].kind, "sd.com.elgato.discord/com.elgato.discord.mute");

        let open = &key(p, "f1", "k1_0").actions[0];
        assert_eq!((open.kind.as_str(), open.settings["path"].as_str(), open.settings["args"].as_str()), ("core.launch", Some("C:\\Program Files\\App\\app.exe"), Some("--fast")));
        assert!(key(p, "f1", "k1_0").icon.is_none(), "not an image");

        let switch = key(p, "f1", "k2_0");
        assert_eq!(switch.kind, KeyKind::Multi);
        assert_eq!(switch.states.len(), 2);
        assert_eq!((switch.states[0].label.as_str(), switch.states[1].label.as_str()), ("Cam on", "Cam off"));
        assert!(switch.states[0].icon.is_some() && switch.states[1].icon.is_none());
        assert_eq!(switch.states[1].actions[0].settings["keys"], "ctrl+f3");

        // Unknown actions keep their key, title and image.
        let bright = key(p, "p1", "k3_0");
        assert!(bright.actions.is_empty() && bright.icon.is_some());

        let r = &done.report;
        assert_eq!(r.keys, 12);
        let plugins: Vec<(&str, usize)> = r.plugins.iter().map(|m| (m.name.as_str(), m.keys)).collect();
        assert_eq!(plugins, [("Discord", 1), ("OBS Studio", 1)]);
        let unsupported: Vec<&str> = r.unsupported.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(unsupported, ["Brightness"]);
        assert_eq!(r.dials, 1, "the MK.2 has no dials");
        assert_eq!(r.images, 1);
        assert_eq!(env.stored.len(), 3);

        // Review flags and checks: steps that control the PC wait, page changes don't.
        let p = crate::share::adopt(done.profile).unwrap();
        assert_ne!(p.id, "imported");
        let k = key(&p, "p1", "k2_0");
        assert_eq!(k.actions.iter().map(|a| a.review).collect::<Vec<_>>(), [true, false, true], "waits are safe");
        assert!(!key(&p, "p1", "k4_2").actions[0].review);
    }

    #[test]
    fn installed_plugins_are_not_reported() {
        let mut env = Fake::new();
        env.installed.push(("com.elgato.discord", "com.elgato.discord.mute"));
        let done = import(&current_format("20GBA9901"), &mut env).unwrap();
        assert_eq!(done.report.plugins.iter().map(|m| m.uuid.as_str()).collect::<Vec<_>>(), ["com.elgato.obsstudio"]);
    }

    #[test]
    fn dials_go_to_a_linked_stream_deck_plus() {
        let mut env = Fake::new();
        env.deck = Some(DeckLink { serial: "PL1".into(), model: "Stream Deck +".into(), cols: 4, rows: 2, brightness: None, sleep: None });
        // The keys of the test profile reach column 5: they don't fit a +, so it isn't linked.
        let done = import(&current_format("20GBD9901"), &mut env).unwrap();
        assert!(done.profile.deck.is_none());
        assert_eq!(done.report.dials, 1);

        let manifest = json!({ "Device": { "Model": "20GBD9901" }, "Name": "Plus", "Pages": { "Pages": [PAGE1] } });
        let page = json!({ "Controllers": [
            { "Type": "Keypad", "Actions": { "3,1": hotkey_action(false, false, 0x0100_0072, 0xAF) } },
            { "Type": "Encoder", "Actions": {
                "0,0": { "UUID": "com.elgato.volume-controller.output", "Plugin": { "Name": "Volume Controller", "UUID": "com.elgato.volume-controller" }, "Settings": { "app": "x" }, "States": [{ "Title": "Vol" }], "Encoder": { "Icon": "Images/DIAL.png" } },
                "1,0": { "Name": "Multimedia", "UUID": "com.elgato.streamdeck.system.multimedia", "Settings": { "actionIdx": 18 }, "States": [{ "Title": "System Volume" }] } } }
        ] });
        let bytes = zip(&[
            (&format!("{PROFILE}/manifest.json"), manifest.to_string().as_bytes()),
            (&format!("{PROFILE}/Profiles/{}/manifest.json", PAGE1.to_uppercase()), page.to_string().as_bytes()),
            (&format!("{PROFILE}/Profiles/{}/Images/DIAL.png", PAGE1.to_uppercase()), PNG),
        ]);
        let done = import(&bytes, &mut env).unwrap();
        let p = &done.profile;
        assert_eq!(done.report.deck.as_deref(), Some("Stream Deck +"));
        assert_eq!((p.pages[0].cols, p.pages[0].rows), (4, 3), "one row more for the dials");
        let vol = key(p, "p1", "d0");
        assert_eq!((vol.x, vol.y, vol.label.as_str()), (0, 2, "Vol"));
        assert!(vol.icon.is_some());
        assert_eq!(vol.actions[0].kind, "sd.com.elgato.volume-controller/com.elgato.volume-controller.output");
        assert!(key(p, "p1", "d1").actions.is_empty());
        assert_eq!(done.report.unsupported[0].name, "Multimedia");
        assert_eq!(key(p, "p1", "k3_1").actions[0].settings["key"], "volume-up", "media keys in hotkeys");
        crate::share::adopt(done.profile).unwrap();
    }

    #[test]
    fn reads_the_older_format() {
        let root = "8E8C4D5B-1111-4111-8111-111111111111.sdProfile";
        let child = "C0FFEE00-2222-4222-8222-222222222222";
        let manifest = json!({ "DeviceModel": "20GAI9901", "DeviceUUID": "", "Name": "Old one", "Version": "1.0", "Actions": {
            "0,0": { "Name": "Hotkey", "UUID": "com.elgato.streamdeck.system.hotkey", "State": 0, "States": [{ "Title": "Save" }],
                     "Settings": { "Hotkeys": [{ "KeyCmd": true, "KeyCtrl": false, "KeyModifiers": 8, "KeyOption": false, "KeyShift": false, "NativeCode": 190, "QTKeyCode": 46, "VKeyCode": 190 }] } },
            "1,0": { "Name": "Create Folder", "UUID": OPEN_CHILD, "Settings": { "ProfileUUID": child }, "State": 0, "States": [{ "Title": "Tools" }] },
            "2,1": { "Name": "Timer", "UUID": "com.elgato.streamdeck.system.timer", "Settings": {}, "State": 0, "States": [{}] }
        } });
        let folder = json!({ "Name": "", "Actions": { "0,0": { "UUID": BACK, "Settings": {}, "States": [{}] } } });
        let bytes = zip(&[
            (&format!("{root}/manifest.json"), manifest.to_string().as_bytes()),
            (&format!("{root}/0,0/CustomImages/state0.png"), PNG),
            (&format!("{root}/Profiles/{child}.sdProfile/manifest.json"), folder.to_string().as_bytes()),
        ]);
        let mut env = Fake::new();
        let done = import(&bytes, &mut env).unwrap();
        let p = &done.profile;
        assert_eq!(p.name, "Old one");
        assert_eq!(p.pages.iter().map(|pg| (pg.id.as_str(), pg.folder, pg.cols, pg.rows)).collect::<Vec<_>>(), [("p1", false, 3, 2), ("f1", true, 3, 2)]);
        let save = key(p, "p1", "k0_0");
        assert_eq!(save.actions[0].settings["keys"], "super+.");
        assert!(save.icon.is_some(), "custom image from the key folder");
        assert_eq!(key(p, "p1", "k1_0").target.as_deref(), Some("f1"));
        assert_eq!(key(p, "f1", "k0_0").kind, KeyKind::Back);
        assert_eq!(key(p, "p1", "k2_1").label, "Timer", "keys without title or image show the action name");
        assert_eq!(done.report.unsupported[0].name, "Timer");
        crate::share::adopt(done.profile).unwrap();
    }

    #[test]
    fn hotkeys_become_keyside_text() {
        let k = |v: Value| match hotkey(&v) {
            Hotkey::Keys(s) => s,
            Hotkey::Media(m) => format!("media:{m}"),
            Hotkey::Empty => "empty".into(),
            Hotkey::Unknown => "unknown".into(),
        };
        assert_eq!(k(json!({ "KeyModifiers": 3, "QTKeyCode": 84, "VKeyCode": 84 })), "ctrl+shift+t");
        assert_eq!(k(json!({ "KeyOption": true, "QTKeyCode": 0x0100_0033, "VKeyCode": 0x73 })), "alt+f4");
        assert_eq!(k(json!({ "KeyModifiers": 2, "QTKeyCode": 0x0100_0012, "VKeyCode": 0x25 })), "ctrl+left");
        // Qt names a shifted character; the Windows code still has the key.
        assert_eq!(k(json!({ "KeyModifiers": 1, "QTKeyCode": 0x2B, "VKeyCode": 0xBB })), "shift+=");
        assert_eq!(k(json!({ "QTKeyCode": 0x0100_0085, "VKeyCode": 0xB3 })), "media:play");
        assert_eq!(k(json!({ "QTKeyCode": QT_NO_KEY, "VKeyCode": -1 })), "empty");
        assert_eq!(k(json!({ "QTKeyCode": 0x0100_00FF, "VKeyCode": 0xFF })), "unknown");
        for combo in ["ctrl+shift+t", "alt+f4", "ctrl+left", "shift+=", "super+."] {
            crate::actions::input::parse_combo(combo).unwrap();
        }
    }

    #[test]
    fn short_page_names_decode() {
        for u in [PAGE1, PAGE2, FOLDER, "ffffffff-ffff-4fff-bfff-ffffffffffff"] {
            assert_eq!(id_key(&short_id(u)), u.replace('-', ""));
            assert_eq!(id_key(&u.to_uppercase()), u.replace('-', ""));
        }
        assert_eq!(id_key("NOT-A-PAGE"), "not-a-page");
    }

    #[test]
    fn rejects_junk_bombs_and_odd_paths() {
        let mut env = Fake::new();
        assert!(import(b"not a zip", &mut env).is_err());
        assert!(import(&zip(&[("readme.txt", b"hi")]), &mut env).is_err());
        assert!(import(&zip(&[("../x.sdProfile/manifest.json", b"{}")]), &mut env).is_err());
        let two = zip(&[("A.sdProfile/manifest.json", b"{\"Pages\":[]}"), ("B.sdProfile/manifest.json", b"{\"Pages\":[]}")]);
        assert!(import(&two, &mut env).unwrap_err().to_string().contains("several"));
        // A small zip that unpacks to more than any profile needs.
        let mut out = std::io::Cursor::new(Vec::new());
        let mut z = zip::ZipWriter::new(&mut out);
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        z.start_file("A.sdProfile/manifest.json", opts).unwrap();
        z.write_all(b"{\"Pages\":[]}").unwrap();
        z.start_file("A.sdProfile/Profiles/X/Images/big.png", opts).unwrap();
        z.write_all(&vec![0u8; MAX_ENTRY as usize + 1]).unwrap();
        z.finish().unwrap();
        let bomb = out.into_inner();
        assert!(bomb.len() < 100 * 1024);
        assert!(import(&bomb, &mut env).is_err());
        assert!(import(&vec![0u8; MAX_FILE + 1], &mut env).is_err());
    }

    #[test]
    fn oversize_images_and_long_multi_actions_are_reported() {
        let many: Vec<Value> = (0..40).map(|i| json!({ "UUID": "com.elgato.streamdeck.system.text", "Settings": { "pastedText": format!("{i}") } })).collect();
        let manifest = json!({ "Device": { "Model": "20GAI9901" }, "Name": "Big", "Pages": { "Pages": [PAGE1] } });
        let page = json!({ "Controllers": [{ "Type": "Keypad", "Actions": {
            "0,0": { "UUID": "com.elgato.streamdeck.multiactions.routine", "States": [{ "Image": "Images/BIG.png" }], "Actions": [{ "Actions": many }] } } }] });
        let big = [PNG, &vec![0u8; crate::assets::MAX_IMAGE]].concat();
        let bytes = zip(&[
            (&format!("{PROFILE}/manifest.json"), manifest.to_string().as_bytes()),
            (&format!("{PROFILE}/Profiles/{}/manifest.json", PAGE1.to_uppercase()), page.to_string().as_bytes()),
            (&format!("{PROFILE}/Profiles/{}/Images/BIG.png", PAGE1.to_uppercase()), &big),
        ]);
        let mut env = Fake::new();
        let done = import(&bytes, &mut env).unwrap();
        let k = key(&done.profile, "p1", "k0_0");
        assert_eq!(k.actions.len(), MAX_ACTIONS);
        assert!(k.icon.is_none());
        assert_eq!((done.report.trimmed, done.report.images), (1, 1));
        crate::share::adopt(done.profile).unwrap();
    }
}

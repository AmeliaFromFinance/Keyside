//! Plugin manifest parsing and checks. A manifest that fails here is never run.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const HOST_API: &str = "1.0.0";
const RESERVED_PREFIXES: &[&str] = &["core", "user", "timer", "sys", "sd", "phonedeck"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Field {
    String {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default: Option<String>,
        #[serde(default, rename = "enum", skip_serializing_if = "Option::is_none")]
        choices: Option<Vec<String>>,
        #[serde(default, rename = "enumNames", skip_serializing_if = "Option::is_none")]
        choice_names: Option<Vec<String>>,
        #[serde(default, rename = "maxLength", skip_serializing_if = "Option::is_none")]
        max_length: Option<usize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        format: Option<String>,
        #[serde(default, rename = "x-options", skip_serializing_if = "Option::is_none")]
        options: Option<String>,
    },
    Number {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        minimum: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        maximum: Option<f64>,
    },
    Integer {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        minimum: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        maximum: Option<f64>,
    },
    Boolean {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default: Option<bool>,
    },
    Object {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(rename = "additionalProperties")]
        additional: Value,
    },
}

impl Field {
    pub fn is_secret(&self) -> bool {
        matches!(self, Field::String { format: Some(f), .. } if f == "secret")
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Schema {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub properties: BTreeMap<String, Field>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActionDef {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<Schema>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Permissions {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub net: Vec<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub udp: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ipc: Vec<String>,
}

impl Permissions {
    /// True when `self` asks for nothing beyond `granted`.
    pub fn within(&self, granted: &Permissions) -> bool {
        self.net.iter().all(|n| granted.net.contains(n)) && (!self.udp || granted.udp) && self.ipc.iter().all(|i| granted.ipc.contains(i))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VarDef {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FeedDef {
    pub name: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    /// Short name for variables and feeds, e.g. "obs" gives {obs.scene}.
    pub prefix: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    pub host: String,
    pub entry: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default)]
    pub permissions: Permissions,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection: Option<Schema>,
    pub actions: Vec<ActionDef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variables: Vec<VarDef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub feeds: Vec<FeedDef>,
}

fn err<T>(m: impl Into<String>) -> Result<T, String> {
    Err(m.into())
}

pub fn valid_plugin_id(s: &str) -> bool {
    (3..=64).contains(&s.len())
        && s.contains('.')
        && !s.starts_with("sd.")
        && s.split('.').all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'))
}

fn word(s: &str, max: usize) -> bool {
    !s.is_empty() && s.len() <= max && s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

fn plain(field: &str, s: &str, max: usize) -> Result<(), String> {
    if s.chars().count() > max || s.chars().any(char::is_control) {
        return err(format!("{field} is too long or has control characters"));
    }
    Ok(())
}

fn check_schema(where_: &str, s: &Schema) -> Result<(), String> {
    if s.kind != "object" || s.properties.len() > 40 {
        return err(format!("{where_}: settings must be an object with at most 40 fields"));
    }
    for (name, f) in &s.properties {
        if !word(name, 40) && !name.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return err(format!("{where_}: invalid field name \"{name}\""));
        }
        if let Field::String { choices: Some(c), choice_names, format, .. } = f {
            if c.len() > 200 || choice_names.as_ref().is_some_and(|n| n.len() != c.len()) {
                return err(format!("{where_}: invalid choices for \"{name}\""));
            }
            if format.as_deref().is_some_and(|f| !["secret", "textarea", "color", "url", "hotkey", "file"].contains(&f)) {
                return err(format!("{where_}: unknown format for \"{name}\""));
            }
        }
        if let Field::Object { additional, .. } = f
            && additional.get("type").and_then(Value::as_str) != Some("string")
        {
            return err(format!("{where_}: \"{name}\" may only hold text values"));
        }
    }
    if s.required.iter().any(|r| !s.properties.contains_key(r)) {
        return err(format!("{where_}: required field is not defined"));
    }
    Ok(())
}

/// Net permission entries: host, host:port, or {field} templates filled from connection settings.
fn check_net(entry: &str, conn: Option<&Schema>) -> Result<(), String> {
    if entry.is_empty() || entry.len() > 255 || entry.contains('*') || entry.contains('/') || entry.contains(char::is_whitespace) {
        return err(format!("invalid network permission \"{entry}\""));
    }
    let mut rest = entry;
    while let Some(start) = rest.find('{') {
        let end = rest[start..].find('}').ok_or_else(|| format!("unclosed template in \"{entry}\""))? + start;
        let field = &rest[start + 1..end];
        if !conn.is_some_and(|c| c.properties.contains_key(field)) {
            return err(format!("network permission uses unknown setting \"{field}\""));
        }
        rest = &rest[end + 1..];
    }
    Ok(())
}

pub fn validate(m: &Manifest) -> Result<(), String> {
    if !valid_plugin_id(&m.id) {
        return err("plugin id must look like com.example.name");
    }
    if !word(&m.prefix, 16) || !m.prefix.as_bytes()[0].is_ascii_lowercase() || RESERVED_PREFIXES.contains(&m.prefix.as_str()) {
        return err("prefix must be 2 to 16 lowercase letters, digits or _ and not reserved");
    }
    semver::Version::parse(&m.version).map_err(|_| "version must be semver, like 1.2.3".to_string())?;
    let req = semver::VersionReq::parse(&m.host).map_err(|_| "host must be a version range, like ^1.0".to_string())?;
    if !req.matches(&semver::Version::parse(HOST_API).expect("valid")) {
        return err(format!("plugin needs host API {}, this is {HOST_API}", m.host));
    }
    plain("name", &m.name, 40)?;
    plain("author", &m.author, 60)?;
    if let Some(d) = &m.description {
        plain("description", d, 300)?;
    }
    if m.homepage.as_deref().is_some_and(|h| !h.starts_with("https://") || h.len() > 300) {
        return err("homepage must be an https link");
    }
    let entry = std::path::Path::new(&m.entry);
    if entry.is_absolute() || m.entry.contains("..") || m.entry.contains('\\') || !(m.entry.ends_with(".js") || m.entry.ends_with(".mjs")) {
        return err("entry must be a .js file inside the package");
    }
    if m.permissions.net.len() > 32 || m.permissions.ipc.iter().any(|i| i != "discord") {
        return err("unsupported permissions");
    }
    for n in &m.permissions.net {
        check_net(n, m.connection.as_ref())?;
    }
    if let Some(c) = &m.connection {
        check_schema("connection", c)?;
    }
    if m.actions.len() > 200 {
        return err("too many actions");
    }
    let mut seen = std::collections::HashSet::new();
    for a in &m.actions {
        if !word(&a.id, 40) || !seen.insert(&a.id) {
            return err(format!("action id \"{}\" is invalid or repeated", a.id));
        }
        plain("action name", &a.name, 40)?;
        if let Some(s) = &a.settings {
            check_schema(&a.id, s)?;
        }
    }
    if m.variables.len() > 200 || m.variables.iter().any(|v| !crate::model::valid_var_name(&v.name)) {
        return err("invalid variables list");
    }
    if m.feeds.len() > 10 || m.feeds.iter().any(|f| !word(&f.name, 24)) {
        return err("invalid feeds list");
    }
    Ok(())
}

/// Checks values against a schema and drops fields it doesn't define.
pub fn clean_values(schema: Option<&Schema>, values: &Map<String, Value>) -> Result<Map<String, Value>, String> {
    check_values(schema, values, true)
}

/// Like `clean_values`, but drops bad fields instead of failing. Used while a key is still being edited.
pub fn sanitize(schema: Option<&Schema>, values: &Map<String, Value>) -> Map<String, Value> {
    check_values(schema, values, false).unwrap_or_default()
}

fn check_values(schema: Option<&Schema>, values: &Map<String, Value>, strict: bool) -> Result<Map<String, Value>, String> {
    let Some(schema) = schema else { return Ok(Map::new()) };
    let mut out = Map::new();
    for (name, f) in &schema.properties {
        let Some(v) = values.get(name) else { continue };
        if v.is_null() {
            continue;
        }
        let ok = match f {
            Field::String { choices, max_length, .. } => {
                v.as_str().is_some_and(|s| s.chars().count() <= max_length.unwrap_or(4096).min(16_384) && choices.as_ref().is_none_or(|c| c.iter().any(|x| x == s)))
            }
            Field::Number { minimum, maximum, .. } | Field::Integer { minimum, maximum, .. } => {
                let int = matches!(f, Field::Integer { .. });
                v.as_f64().is_some_and(|n| n.is_finite() && minimum.is_none_or(|m| n >= m) && maximum.is_none_or(|m| n <= m) && (!int || n.fract() == 0.0))
            }
            Field::Boolean { .. } => v.is_boolean(),
            Field::Object { .. } => v.as_object().is_some_and(|o| o.len() <= 32 && o.iter().all(|(k, v)| k.len() <= 64 && v.as_str().is_some_and(|s| s.len() <= 1024))),
        };
        if !ok {
            if strict {
                return err(format!("\"{name}\" has an invalid value"));
            }
            continue;
        }
        out.insert(name.clone(), v.clone());
    }
    if !strict {
        return Ok(out);
    }
    for r in &schema.required {
        if !out.contains_key(r) && !matches!(schema.properties.get(r), Some(f) if f.is_secret()) {
            return err(format!("\"{r}\" is required"));
        }
    }
    Ok(out)
}

/// Fills {field} templates in net permissions from connection values. Rejects anything
/// that would widen access (wildcards, paths, spaces, empty hosts).
pub fn expand_net(entries: &[String], conn: &Map<String, Value>) -> Vec<String> {
    let mut out = Vec::new();
    for e in entries {
        let mut s = String::new();
        let mut rest = e.as_str();
        let mut ok = true;
        while let Some(start) = rest.find('{') {
            s.push_str(&rest[..start]);
            let Some(end) = rest[start..].find('}').map(|e| e + start) else {
                ok = false;
                break;
            };
            match conn.get(&rest[start + 1..end]) {
                Some(Value::String(v)) => s.push_str(v.trim()),
                Some(Value::Number(n)) => match n.as_f64() {
                    Some(f) if f.fract() == 0.0 && (0.0..=65535.0).contains(&f) => s.push_str(&(f as u32).to_string()),
                    _ => ok = false,
                },
                _ => ok = false,
            }
            rest = &rest[end + 1..];
        }
        s.push_str(rest);
        let host_ok = !s.is_empty()
            && s.len() <= 255
            && !s.starts_with(':')
            && s.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b':' | b'[' | b']'));
        if ok && host_ok && !out.contains(&s) {
            out.push(s);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn manifest() -> Manifest {
        serde_json::from_value(json!({
            "id": "dev.example.test", "name": "Test", "version": "1.0.0", "author": "Me", "prefix": "test",
            "host": "^1.0", "entry": "main.js",
            "permissions": { "net": ["{host}:{port}", "api.example.com"] },
            "connection": { "type": "object", "properties": {
                "host": { "type": "string", "default": "127.0.0.1" },
                "port": { "type": "integer", "minimum": 1, "maximum": 65535 },
                "password": { "type": "string", "format": "secret" } }, "required": ["host", "password"] },
            "actions": [{ "id": "go", "name": "Go", "settings": { "type": "object", "properties": {
                "mode": { "type": "string", "enum": ["a", "b"] } } } }]
        }))
        .unwrap()
    }

    #[test]
    fn accepts_good_manifest() {
        validate(&manifest()).unwrap();
    }

    #[test]
    fn rejects_dangerous_manifests() {
        let mut m = manifest();
        m.permissions.net.push("*".into());
        assert!(validate(&m).is_err());
        let mut m = manifest();
        m.entry = "../../evil.js".into();
        assert!(validate(&m).is_err());
        let mut m = manifest();
        m.prefix = "core".into();
        assert!(validate(&m).is_err());
        let mut m = manifest();
        m.permissions.net = vec!["{token}".into()];
        assert!(validate(&m).is_err());
        let mut m = manifest();
        m.id = "sd.com.elgato.x".into();
        assert!(validate(&m).is_err());
    }

    #[test]
    fn expands_templates_safely() {
        let conn = json!({ "host": "192.168.1.5", "port": 4455 }).as_object().unwrap().clone();
        assert_eq!(expand_net(&["{host}:{port}".into()], &conn), vec!["192.168.1.5:4455"]);
        let bad = json!({ "host": "evil.com,0.0.0.0", "port": 1 }).as_object().unwrap().clone();
        assert!(expand_net(&["{host}:{port}".into()], &bad).is_empty());
        let missing = Map::new();
        assert!(expand_net(&["{host}".into()], &missing).is_empty());
    }

    #[test]
    fn cleans_values() {
        let m = manifest();
        let s = m.actions[0].settings.as_ref();
        assert!(clean_values(s, json!({"mode": "c"}).as_object().unwrap()).is_err());
        let ok = clean_values(s, json!({"mode": "a", "extra": 1}).as_object().unwrap()).unwrap();
        assert_eq!(ok.len(), 1);
    }
}

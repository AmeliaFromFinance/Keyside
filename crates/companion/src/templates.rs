//! Key templates: keys saved under a name so any editor can place them again. One template
//! can hold several keys and keeps their arrangement.

use std::collections::HashSet;
use std::path::PathBuf;

use anyhow::{Result, bail};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::model::{self, Key, KeyKind, Page};
use crate::store;

const MAX_TEMPLATES: usize = 200;
const MAX_KEYS: usize = 48;
/// One template, as JSON.
const MAX_BYTES: usize = 64 * 1024;
/// The whole file, as JSON.
const MAX_TOTAL: usize = 2 * 1024 * 1024;
/// Templates are checked on a grid this big, so they fit any page Keyside allows.
const GRID: u8 = 12;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Template {
    pub id: String,
    pub name: String,
    /// Positions start at 0,0: the top-left key of the group.
    pub keys: Vec<Key>,
}

pub struct Templates {
    path: PathBuf,
    list: Mutex<Vec<Template>>,
}

fn clean_name(name: &str) -> Result<String> {
    crate::config::clean_name(name, 40).ok_or_else(|| anyhow::anyhow!("give the template a name"))
}

/// Moves the keys to start at 0,0 and checks them like keys on a page.
fn prepare(mut keys: Vec<Key>) -> Result<Vec<Key>> {
    if keys.is_empty() || keys.len() > MAX_KEYS {
        bail!("a template holds 1 to {MAX_KEYS} keys");
    }
    if keys.iter().any(|k| k.kind == KeyKind::Folder) {
        bail!("folders can't be saved as templates");
    }
    let (ox, oy) = (keys.iter().map(|k| k.x).min().unwrap_or(0), keys.iter().map(|k| k.y).min().unwrap_or(0));
    for k in &mut keys {
        k.x -= ox;
        k.y -= oy;
        // Styles belong to one layout, and templates go into any.
        k.styles.clear();
    }
    let page = Page { id: "template".into(), name: "Template".into(), subtitle: None, cols: GRID, rows: GRID, folder: false, keys, variants: Default::default(), open_when: None };
    model::validate_page(&page, &HashSet::new(), &HashSet::new()).map_err(|e| anyhow::anyhow!(e.0))?;
    Ok(page.keys)
}

impl Templates {
    pub fn open(path: PathBuf) -> Result<Self> {
        let list: Vec<Template> = store::read_json_or_reset(&path)?.unwrap_or_default();
        // Drops entries that no longer pass the checks, so a hand-edited file can't break the editor.
        let list = list.into_iter().filter_map(|t| Some(Template { keys: prepare(t.keys).ok()?, ..t })).collect();
        Ok(Self { path, list: Mutex::new(list) })
    }

    /// Reads the file again, after a restore.
    pub fn reload(&self) -> Result<()> {
        let fresh = Self::open(self.path.clone())?;
        *self.list.lock() = fresh.list.into_inner();
        Ok(())
    }

    pub fn list(&self) -> Vec<Template> {
        self.list.lock().clone()
    }

    fn save(&self, list: &[Template]) -> Result<()> {
        if serde_json::to_vec(list)?.len() > MAX_TOTAL {
            bail!("there are too many templates, delete some first");
        }
        store::write_json(&self.path, &list)
    }

    pub fn add(&self, name: &str, keys: Vec<Key>) -> Result<Template> {
        let t = Template { id: crate::ids::random_b64(9), name: clean_name(name)?, keys: prepare(keys)? };
        if serde_json::to_vec(&t)?.len() > MAX_BYTES {
            bail!("this template is too large");
        }
        let mut list = self.list.lock();
        if list.len() >= MAX_TEMPLATES {
            bail!("there can be at most {MAX_TEMPLATES} templates");
        }
        let mut next = list.clone();
        next.push(t.clone());
        self.save(&next)?;
        *list = next;
        Ok(t)
    }

    pub fn rename(&self, id: &str, name: &str) -> Result<bool> {
        let name = clean_name(name)?;
        let mut list = self.list.lock();
        let Some(i) = list.iter().position(|t| t.id == id) else { return Ok(false) };
        let mut next = list.clone();
        next[i].name = name;
        self.save(&next)?;
        *list = next;
        Ok(true)
    }

    pub fn delete(&self, id: &str) -> Result<bool> {
        let mut list = self.list.lock();
        let next: Vec<Template> = list.iter().filter(|t| t.id != id).cloned().collect();
        if next.len() == list.len() {
            return Ok(false);
        }
        self.save(&next)?;
        *list = next;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn key(id: &str, x: u8, y: u8) -> Key {
        serde_json::from_value(json!({ "id": id, "x": x, "y": y, "kind": "button", "label": id, "actions": [{ "type": "core.wait", "settings": { "ms": 100 } }] })).unwrap()
    }

    #[test]
    fn keeps_the_arrangement_from_the_top_left() {
        let dir = tempfile::tempdir().unwrap();
        let t = Templates::open(dir.path().join("templates.json")).unwrap();
        let saved = t.add("  Stream  ", vec![key("a", 3, 2), key("b", 4, 3)]).unwrap();
        assert_eq!(saved.name, "Stream");
        assert_eq!((saved.keys[0].x, saved.keys[0].y, saved.keys[1].x, saved.keys[1].y), (0, 0, 1, 1));
        assert_eq!(saved.keys[0].actions.len(), 1);
        // Survives a restart.
        let again = Templates::open(dir.path().join("templates.json")).unwrap();
        assert_eq!(again.list().len(), 1);
        assert!(again.rename(&saved.id, "Live").unwrap());
        assert_eq!(again.list()[0].name, "Live");
        assert!(again.delete(&saved.id).unwrap());
        assert!(!again.delete(&saved.id).unwrap());
        assert!(again.list().is_empty());
    }

    #[test]
    fn rejects_bad_templates() {
        let dir = tempfile::tempdir().unwrap();
        let t = Templates::open(dir.path().join("templates.json")).unwrap();
        assert!(t.add("", vec![key("a", 0, 0)]).is_err(), "needs a name");
        assert!(t.add("x", vec![]).is_err(), "needs keys");
        assert!(t.add("x", vec![key("a", 0, 0), key("a", 1, 0)]).is_err(), "unique ids");
        assert!(t.add("x", vec![key("a", 0, 0), key("b", 0, 0)]).is_err(), "no overlaps");
        let mut folder = key("f", 0, 0);
        folder.kind = KeyKind::Folder;
        folder.target = Some("p1".into());
        assert!(t.add("x", vec![folder]).is_err(), "no folders");
        let mut big = key("a", 0, 0);
        big.icon = Some("../../etc".into());
        assert!(t.add("x", vec![big]).is_err(), "icons are checked");
        let many: Vec<Key> = (0..MAX_KEYS + 1).map(|i| key(&format!("k{i}"), (i % 12) as u8, (i / 12) as u8)).collect();
        assert!(t.add("x", many).is_err(), "too many keys");
        assert!(t.list().is_empty());
    }

    #[test]
    fn ignores_broken_entries_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("templates.json");
        let bad = key("a", 0, 0);
        let good = key("b", 0, 0);
        std::fs::write(&path, serde_json::to_vec(&json!([{ "id": "x", "name": "Bad", "keys": [bad.clone(), bad] }, { "id": "y", "name": "Good", "keys": [good] }])).unwrap())
            .unwrap();
        let t = Templates::open(path).unwrap();
        assert_eq!(t.list().iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["Good"]);
    }
}

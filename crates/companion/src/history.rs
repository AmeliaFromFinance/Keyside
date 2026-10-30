//! The last pushed layouts, so a push can be undone from the editor.

use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::model::LayoutFile;
use crate::state::all_keys;
use crate::store;

pub const KEEP: usize = 20;
pub const FILE: &str = "history.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub rev: u64,
    /// When it was pushed (ms since 1970). 0 for the layout from before the first push.
    pub at: u64,
    /// Name of the device that pushed it.
    pub by: String,
    pub device: String,
    pub layout: LayoutFile,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub rev: u64,
    pub at: u64,
    pub by: String,
    pub device: String,
    pub layouts: usize,
    pub pages: usize,
    pub keys: usize,
}

fn read(root: &Path) -> Vec<Entry> {
    store::read_json_or_reset(&root.join(FILE)).ok().flatten().unwrap_or_default()
}

/// Adds a pushed layout. `before` is the layout it replaced, kept too when the history is
/// still empty so the very first push can be undone.
pub fn record(root: &Path, before: &LayoutFile, entry: Entry) -> Result<()> {
    let mut list = read(root);
    if list.is_empty() && before.rev != entry.rev {
        list.push(Entry { rev: before.rev, at: 0, by: String::new(), device: String::new(), layout: before.clone() });
    }
    list.retain(|e| e.rev != entry.rev);
    list.push(entry);
    let extra = list.len().saturating_sub(KEEP);
    list.drain(..extra);
    store::write_json(&root.join(FILE), &list)
}

/// Newest first.
pub fn list(root: &Path) -> Vec<Summary> {
    read(root)
        .into_iter()
        .rev()
        .map(|e| Summary {
            rev: e.rev,
            at: e.at,
            layouts: e.layout.profiles.len(),
            pages: e.layout.profiles.iter().map(|p| p.pages.len()).sum(),
            keys: all_keys(&e.layout).count(),
            by: e.by,
            device: e.device,
        })
        .collect()
}

pub fn get(root: &Path, rev: u64) -> Option<LayoutFile> {
    read(root).into_iter().find(|e| e.rev == rev).map(|e| e.layout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_last_twenty_and_the_layout_before_the_first_push() {
        let dir = tempfile::tempdir().unwrap();
        let base = crate::state::default_layout();
        let mut prev = base.clone();
        for rev in 1..=25 {
            let mut l = base.clone();
            l.rev = rev;
            l.profiles[0].name = format!("v{rev}");
            record(dir.path(), &prev, Entry { rev, at: rev * 1000, by: "This PC".into(), device: "local".into(), layout: l.clone() }).unwrap();
            if rev == 1 {
                assert_eq!(list(dir.path()).len(), 2, "first push keeps what it replaced");
                assert_eq!(get(dir.path(), 0).unwrap().profiles[0].name, base.profiles[0].name);
            }
            prev = l;
        }
        let all = list(dir.path());
        assert_eq!(all.len(), KEEP);
        assert_eq!(all[0].rev, 25, "newest first");
        assert_eq!(all.last().unwrap().rev, 6);
        assert_eq!(get(dir.path(), 25).unwrap().profiles[0].name, "v25");
        assert!(get(dir.path(), 3).is_none());
    }
}

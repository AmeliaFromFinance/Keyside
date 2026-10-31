//! Automatic backups: one a day into `backups/` in the data folder, newest 7 kept. They never
//! hold passwords or keys, since there is no passphrase to encrypt them with. A day with no
//! changes since the last one adds nothing.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::app::App;
use crate::backup::{self, Contents};
use crate::store;

pub const DIR: &str = "backups";
pub const KEEP: usize = 7;
const PREFIX: &str = "keyside-auto-";
const DAY_MS: u64 = 86_400_000;
/// Wait after start, so Keyside settles first.
const FIRST_CHECK: Duration = Duration::from_secs(60);
const CHECK_EVERY: Duration = Duration::from_secs(600);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    /// The file name, which is also how the editor refers to it.
    pub id: String,
    pub created: u64,
    pub size: u64,
    pub keyside: String,
    pub contents: Contents,
}

fn dir(app: &App) -> Result<PathBuf> {
    app.paths.dir(DIR)
}

/// `keyside-auto-YYYYMMDD-HHMMSS.zip`, the only names this folder lists.
fn valid_name(n: &str) -> bool {
    let Some(stamp) = n.strip_prefix(PREFIX).and_then(|r| r.strip_suffix(".zip")) else { return false };
    let b = stamp.as_bytes();
    b.len() == 15 && b[8] == b'-' && b.iter().enumerate().all(|(i, c)| i == 8 || c.is_ascii_digit())
}

fn file_name(ms: u64) -> String {
    let t = time::OffsetDateTime::from_unix_timestamp((ms / 1000) as i64).unwrap_or(time::OffsetDateTime::UNIX_EPOCH);
    format!("{PREFIX}{:04}{:02}{:02}-{:02}{:02}{:02}.zip", t.year(), t.month() as u8, t.day(), t.hour(), t.minute(), t.second())
}

/// Backup files in the folder, oldest first. The time in the name sorts them.
fn names(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .map(|it| it.flatten().filter(|e| e.file_type().is_ok_and(|t| t.is_file())).map(|e| e.file_name().to_string_lossy().to_string()).filter(|n| valid_name(n)).collect())
        .unwrap_or_default();
    v.sort();
    v
}

/// Automatic backups, newest first.
pub fn list(app: &App) -> Result<Vec<Entry>> {
    let d = dir(app)?;
    Ok(names(&d)
        .into_iter()
        .rev()
        .filter_map(|n| {
            let p = d.join(&n);
            let m = backup::peek(&p).ok()?;
            Some(Entry { size: p.metadata().map(|m| m.len()).unwrap_or(0), id: n, created: m.created, keyside: m.keyside, contents: m.contents })
        })
        .collect())
}

/// Path of an automatic backup. Only names that are in the folder listing count.
pub fn path_of(app: &App, id: &str) -> Result<PathBuf> {
    let d = dir(app)?;
    if !valid_name(id) || !names(&d).iter().any(|n| n == id) {
        bail!("that backup is no longer kept");
    }
    Ok(d.join(id))
}

/// Reads an automatic backup and stages it for the same review and restore as an uploaded one.
pub fn stage(app: &App, id: &str) -> Result<serde_json::Value> {
    let bytes = std::fs::read(path_of(app, id)?)?;
    let b = backup::read(&bytes)?;
    Ok(backup::stage(app, b))
}

/// Deletes all but the newest `KEEP`.
fn rotate(dir: &Path) {
    let all = names(dir);
    for n in all.iter().take(all.len().saturating_sub(KEEP)) {
        if let Err(e) = std::fs::remove_file(dir.join(n)) {
            tracing::warn!("could not delete the old backup {n}: {e}");
        }
    }
}

/// When the newest automatic backup was made, if there is one.
fn newest(dir: &Path) -> Option<(u64, Option<String>)> {
    let n = names(dir).pop()?;
    let m = backup::peek(&dir.join(n)).ok()?;
    Some((m.created, m.hash))
}

/// Makes a backup now, unless nothing changed since the newest one. Returns the new file name.
pub fn run(app: &App) -> Result<Option<String>> {
    let d = dir(app)?;
    let (files, devices) = backup::gather(app)?;
    let hash = backup::hash_files(&files);
    if newest(&d).is_some_and(|(_, h)| h.as_deref() == Some(hash.as_str())) {
        return Ok(None);
    }
    let bytes = backup::pack(app, &files, &devices, None, Some(hash))?;
    // A second one within the same second replaces the first.
    let name = file_name(crate::server::conn::now_ms());
    store::write_private(&d.join(&name), &bytes).context("saving the automatic backup")?;
    rotate(&d);
    Ok(Some(name))
}

/// Takes a backup a minute after start if the newest is a day old, then once a day.
pub fn spawn(app: Arc<App>) {
    tokio::spawn(async move {
        tokio::time::sleep(FIRST_CHECK).await;
        // Last time a check found nothing new, so an unchanged setup is looked at once a day.
        let mut checked: Option<u64> = None;
        loop {
            if app.settings.read().auto_backup {
                let now = crate::server::conn::now_ms();
                let last = dir(&app).ok().and_then(|d| newest(&d)).map(|(at, _)| at).max(checked).unwrap_or(0);
                if now.saturating_sub(last) >= DAY_MS || last > now + DAY_MS {
                    let a = app.clone();
                    match tokio::task::spawn_blocking(move || run(&a)).await {
                        Ok(Ok(Some(name))) => tracing::info!("automatic backup saved as {name}"),
                        Ok(Ok(None)) => checked = Some(now),
                        Ok(Err(e)) => {
                            tracing::warn!("automatic backup failed: {e:#}");
                            // Try again in an hour rather than every check.
                            checked = Some(now.saturating_sub(DAY_MS - 3_600_000));
                        }
                        Err(e) => tracing::warn!("automatic backup failed: {e}"),
                    }
                }
            }
            tokio::time::sleep(CHECK_EVERY).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(dir: &Path) -> Arc<App> {
        App::new(crate::app::Options { data_dir: Some(dir.into()), bundled_plugins: None }).unwrap()
    }

    #[test]
    fn names_are_checked() {
        assert!(valid_name("keyside-auto-20261031-120000.zip"));
        assert_eq!(file_name(0), "keyside-auto-19700101-000000.zip");
        for bad in [
            "keyside-auto-2026103-120000.zip",
            "../keyside-auto-20261031-120000.zip",
            "keyside-auto-20261031-120000.zip.tmp",
            "keyside-auto-20261031x120000.zip",
            "settings.json",
            "",
        ] {
            assert!(!valid_name(bad), "{bad}");
        }
    }

    #[test]
    fn skips_when_nothing_changed() {
        let t = tempfile::tempdir().unwrap();
        let a = app(t.path());
        assert!(run(&a).unwrap().is_some());
        assert!(run(&a).unwrap().is_none(), "unchanged");
        a.engine.lock().set_var("user.n", serde_json::json!(1));
        std::thread::sleep(Duration::from_millis(1100));
        assert!(run(&a).unwrap().is_some(), "a variable changed");
        let l = list(&a).unwrap();
        assert_eq!(l.len(), 2);
        assert!(l[0].created >= l[1].created, "newest first");
        assert!(l[0].size > 0 && l[0].contents.layouts == 1);
    }

    #[test]
    fn keeps_the_newest_seven() {
        let t = tempfile::tempdir().unwrap();
        let a = app(t.path());
        let d = dir(&a).unwrap();
        for day in 1..=9 {
            std::fs::write(d.join(format!("{PREFIX}200101{day:02}-120000.zip")), b"old").unwrap();
        }
        std::fs::write(d.join("notes.txt"), b"not ours").unwrap();
        let made = run(&a).unwrap().unwrap();
        let left = names(&d);
        assert_eq!(left.len(), KEEP);
        assert_eq!(left.last(), Some(&made));
        assert_eq!(left[0], format!("{PREFIX}20010104-120000.zip"), "the oldest went");
        assert!(d.join("notes.txt").exists(), "other files stay");
    }

    #[test]
    fn refuses_ids_outside_the_folder() {
        let t = tempfile::tempdir().unwrap();
        let a = app(t.path());
        let made = run(&a).unwrap().unwrap();
        assert!(path_of(&a, &made).is_ok());
        assert!(stage(&a, &made).unwrap()["id"].is_string());
        std::fs::write(t.path().join("keyside-auto-20261031-120000.zip"), b"x").unwrap();
        for bad in ["../keyside-auto-20261031-120000.zip", "../identity.json", "/etc/passwd", "keyside-auto-20991231-120000.zip", ""] {
            assert!(path_of(&a, bad).is_err(), "{bad}");
        }
    }
}

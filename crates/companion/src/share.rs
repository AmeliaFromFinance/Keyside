//! Share codes: a profile or some pages packed into one line of text ("PD1.").
//! Imported steps that can affect the PC wait for review on the PC before they run.

use std::io::{Read, Write};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::actions::SAFE_ON_IMPORT;
use crate::crypto::channel::{b64, unb64};
use crate::model::{self, LayoutFile, Page, Profile};

const PREFIX: &str = "PD1.";
const MAX_CODE: usize = 512 * 1024;
const MAX_JSON: u64 = 4 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
struct Shared {
    v: u32,
    profile: Profile,
}

pub fn export(profile: &Profile, pages: Option<&[String]>) -> Result<String> {
    let mut p = profile.clone();
    if let Some(keep) = pages {
        // Keep the chosen pages plus folders they open.
        let mut wanted: Vec<String> = keep.to_vec();
        for pg in &profile.pages {
            if keep.contains(&pg.id) {
                wanted.extend(pg.keys.iter().filter_map(|k| k.target.clone()));
            }
        }
        p.pages.retain(|pg| wanted.contains(&pg.id));
        if p.pages.is_empty() {
            bail!("no pages selected");
        }
        if !p.pages.iter().any(|pg| pg.id == p.home_page) {
            p.home_page = p.pages[0].id.clone();
        }
    }
    p.apps.clear();
    let json = serde_json::to_vec(&Shared { v: 1, profile: p })?;
    let mut z = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::best());
    z.write_all(&json)?;
    let code = format!("{PREFIX}{}", b64(&z.finish()?));
    if code.len() > MAX_CODE {
        bail!("this profile is too large to share as a code");
    }
    Ok(code)
}

/// Parses a code into a profile with a fresh id and review flags set. Not saved yet.
pub fn import(code: &str) -> Result<Profile> {
    let code = code.trim();
    if code.len() > MAX_CODE || !code.starts_with(PREFIX) {
        bail!("this is not a Keyside share code");
    }
    let raw = unb64(&code[PREFIX.len()..], None).map_err(|_| anyhow::anyhow!("share code is damaged"))?;
    let mut json = Vec::new();
    flate2::read::DeflateDecoder::new(raw.as_slice()).take(MAX_JSON + 1).read_to_end(&mut json).context("share code is damaged")?;
    if json.len() as u64 > MAX_JSON {
        bail!("share code is too large");
    }
    let shared: Shared = serde_json::from_slice(&json).context("share code is damaged")?;
    if shared.v != 1 {
        bail!("this share code needs a newer Keyside");
    }
    import_profile(shared.profile)
}

/// Takes a profile from outside (code or file): fresh id, review flags, full validation.
pub fn import_profile(mut p: Profile) -> Result<Profile> {
    p.id = crate::ids::short().replace(['-', '_'], "x");
    p.apps.clear();
    for pg in &mut p.pages {
        mark(pg);
    }
    let check = LayoutFile { version: 1, rev: 0, default_profile: p.id.clone(), profiles: vec![p.clone()] };
    model::validate(&check).map_err(|e| anyhow::anyhow!("share code has an invalid layout: {e}"))?;
    Ok(p)
}

fn mark(pg: &mut Page) {
    for k in &mut pg.keys {
        for a in k.actions.iter_mut().chain(k.off_actions.iter_mut()) {
            if !SAFE_ON_IMPORT.contains(&a.kind.as_str()) {
                a.review = true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_marks_review() {
        let l = crate::state::default_layout();
        let code = export(&l.profiles[0], None).unwrap();
        let p = import(&code).unwrap();
        assert_ne!(p.id, l.profiles[0].id);
        let flagged = p.pages.iter().flat_map(|pg| &pg.keys).flat_map(|k| &k.actions).filter(|a| a.review).count();
        assert!(flagged > 5);
        assert!(p.pages.iter().flat_map(|pg| &pg.keys).flat_map(|k| &k.actions).filter(|a| a.kind == "core.wait").all(|a| !a.review));
    }

    #[test]
    fn rejects_junk_and_bombs() {
        assert!(import("hello").is_err());
        assert!(import("PD1.!!!").is_err());
        let mut z = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::best());
        z.write_all(&vec![b' '; 8 * 1024 * 1024]).unwrap();
        let bomb = format!("PD1.{}", b64(&z.finish().unwrap()));
        assert!(import(&bomb).is_err());
    }
}

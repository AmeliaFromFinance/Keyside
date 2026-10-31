//! Asks GitHub once a day whether a newer Keyside is out, so the editor can say so.
//! Only the version request goes to GitHub. Off in Settings.

use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;

use crate::app::App;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Update {
    pub version: String,
    pub url: String,
}

const CURRENT: &str = env!("CARGO_PKG_VERSION");

pub fn spawn(app: Arc<App>) {
    tokio::spawn(async move {
        // Let start-up settle first.
        tokio::time::sleep(Duration::from_secs(20)).await;
        loop {
            if app.settings.read().update_check {
                match latest(&app.http).await {
                    Ok(found) => *app.update.write() = found.filter(|u| newer(&u.version, CURRENT)),
                    Err(e) => tracing::debug!("update check failed: {e}"),
                }
            }
            tokio::time::sleep(Duration::from_secs(24 * 3600)).await;
        }
    });
}

async fn latest(http: &reqwest::Client) -> anyhow::Result<Option<Update>> {
    let repo = env!("CARGO_PKG_REPOSITORY").trim_end_matches('/').trim_start_matches("https://github.com/");
    let r = http
        .get(format!("https://api.github.com/repos/{repo}/releases/latest"))
        .header("user-agent", format!("Keyside/{CURRENT}"))
        .header("accept", "application/vnd.github+json")
        .timeout(Duration::from_secs(15))
        .send()
        .await?;
    // No published release yet.
    if r.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let v: Value = r.error_for_status()?.json().await?;
    Ok(parse(&v))
}

fn parse(v: &Value) -> Option<Update> {
    let version = v.get("tag_name")?.as_str()?.trim_start_matches('v').to_string();
    let url = v.get("html_url")?.as_str()?;
    url.starts_with("https://github.com/").then(|| Update { version, url: url.to_string() })
}

fn newer(found: &str, current: &str) -> bool {
    match (semver::Version::parse(found), semver::Version::parse(current)) {
        (Ok(f), Ok(c)) => f > c,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn only_newer_releases_count() {
        assert!(newer("1.1.0", "1.0.0"));
        assert!(newer("1.0.1", "1.0.0"));
        assert!(!newer("1.0.0", "1.0.0"));
        assert!(!newer("0.9.0", "1.0.0"));
        assert!(!newer("nonsense", "1.0.0"));
    }

    #[test]
    fn reads_the_release() {
        let u = parse(&json!({ "tag_name": "v1.2.0", "html_url": "https://github.com/a/b/releases/tag/v1.2.0" })).unwrap();
        assert_eq!(u.version, "1.2.0");
        assert!(parse(&json!({ "tag_name": "v1.2.0", "html_url": "https://evil.example/x" })).is_none());
    }
}

//! Stream Deck plugin compatibility. Filled in below.

use anyhow::{Result, bail};
use serde_json::Value;

use crate::app::App;

pub async fn op(_app: &App, _op: &str, _args: &Value) -> Result<Value> {
    bail!("not available yet")
}

pub fn stage(_app: &App, _bytes: &[u8]) -> Result<Value> {
    bail!("not available yet")
}

pub fn inspector_origin(_app: &App) -> Option<String> {
    None
}

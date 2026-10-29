//! Line-delimited JSON-RPC 2.0 over a plugin's stdin and stdout.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, oneshot};

pub const MAX_LINE: usize = 1024 * 1024;
const RATE_PER_SEC: f64 = 200.0;
const BURST: f64 = 400.0;

#[derive(Debug)]
pub enum Incoming {
    /// A call from the plugin. Send the reply with `Peer::reply` when `id` is set.
    Call {
        id: Option<Value>,
        method: String,
        params: Value,
    },
    /// The plugin broke the protocol or limits and should be stopped.
    Violation(String),
    Closed,
}

pub struct Peer {
    out: mpsc::Sender<String>,
    pending: Mutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>,
    next: AtomicU64,
}

impl Peer {
    /// Starts reader and writer tasks. Incoming calls arrive on the returned receiver.
    pub fn start<R, W>(reader: R, writer: W) -> (Arc<Peer>, mpsc::Receiver<Incoming>)
    where
        R: tokio::io::AsyncRead + Unpin + Send + 'static,
        W: AsyncWrite + Unpin + Send + 'static,
    {
        let (out_tx, mut out_rx) = mpsc::channel::<String>(256);
        let (in_tx, in_rx) = mpsc::channel::<Incoming>(256);
        let peer = Arc::new(Peer { out: out_tx, pending: Mutex::new(HashMap::new()), next: AtomicU64::new(1) });

        let mut writer = writer;
        tokio::spawn(async move {
            while let Some(line) = out_rx.recv().await {
                if writer.write_all(line.as_bytes()).await.is_err() || writer.write_all(b"\n").await.is_err() || writer.flush().await.is_err() {
                    break;
                }
            }
        });

        let p = peer.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::with_capacity(64 * 1024, reader);
            let mut buf = Vec::with_capacity(4096);
            let mut tokens = BURST;
            let mut last = Instant::now();
            loop {
                buf.clear();
                match read_line_limited(&mut lines, &mut buf).await {
                    Ok(0) => break,
                    Ok(_) => {}
                    Err(e) => {
                        let _ = in_tx.send(Incoming::Violation(e)).await;
                        break;
                    }
                }
                let now = Instant::now();
                tokens = (tokens + now.duration_since(last).as_secs_f64() * RATE_PER_SEC).min(BURST);
                last = now;
                if tokens < 1.0 {
                    let _ = in_tx.send(Incoming::Violation("too many messages".into())).await;
                    break;
                }
                tokens -= 1.0;
                let Ok(msg) = serde_json::from_slice::<Value>(&buf) else {
                    let _ = in_tx.send(Incoming::Violation("sent something that is not JSON".into())).await;
                    break;
                };
                if let Some(method) = msg.get("method").and_then(Value::as_str) {
                    let call = Incoming::Call { id: msg.get("id").cloned(), method: method.to_string(), params: msg.get("params").cloned().unwrap_or(Value::Null) };
                    if in_tx.send(call).await.is_err() {
                        break;
                    }
                } else if let Some(id) = msg.get("id").and_then(Value::as_u64) {
                    let res = match msg.get("error") {
                        Some(e) => Err(e.get("message").and_then(Value::as_str).unwrap_or("plugin error").chars().take(300).collect()),
                        None => Ok(msg.get("result").cloned().unwrap_or(Value::Null)),
                    };
                    if let Some(tx) = p.pending.lock().remove(&id) {
                        let _ = tx.send(res);
                    }
                }
            }
            p.pending.lock().clear();
            let _ = in_tx.send(Incoming::Closed).await;
        });
        (peer, in_rx)
    }

    pub async fn request(&self, method: &str, params: Value, timeout: Duration) -> Result<Value, String> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().insert(id, tx);
        let line = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }).to_string();
        if self.out.send(line).await.is_err() {
            self.pending.lock().remove(&id);
            return Err("plugin is not running".into());
        }
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(r)) => r,
            Ok(Err(_)) => Err("plugin stopped".into()),
            Err(_) => {
                self.pending.lock().remove(&id);
                Err("plugin did not answer in time".into())
            }
        }
    }

    pub async fn notify(&self, method: &str, params: Value) {
        let _ = self.out.send(json!({ "jsonrpc": "2.0", "method": method, "params": params }).to_string()).await;
    }

    pub async fn reply(&self, id: Value, result: Result<Value, String>) {
        let msg = match result {
            Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
            Err(e) => json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32000, "message": e } }),
        };
        let _ = self.out.send(msg.to_string()).await;
    }
}

async fn read_line_limited<R: tokio::io::AsyncBufRead + Unpin>(r: &mut R, buf: &mut Vec<u8>) -> Result<usize, String> {
    loop {
        let available = r.fill_buf().await.map_err(|e| e.to_string())?;
        if available.is_empty() {
            return Ok(buf.len());
        }
        match available.iter().position(|b| *b == b'\n') {
            Some(i) => {
                buf.extend_from_slice(&available[..i]);
                r.consume(i + 1);
                if buf.len() > MAX_LINE {
                    return Err("message too large".into());
                }
                if buf.is_empty() {
                    continue;
                }
                return Ok(buf.len());
            }
            None => {
                let n = available.len();
                buf.extend_from_slice(available);
                r.consume(n);
                if buf.len() > MAX_LINE {
                    return Err("message too large".into());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn round_trip_and_limits() {
        let (host_side, plugin_side) = tokio::io::duplex(1 << 20);
        let (hr, hw) = tokio::io::split(host_side);
        let (peer, mut rx) = Peer::start(hr, hw);
        let (pr, mut pw) = tokio::io::split(plugin_side);

        // Plugin answers the first request and then sends a call.
        tokio::spawn(async move {
            let mut lines = BufReader::new(pr).lines();
            let req: Value = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
            let id = req["id"].clone();
            pw.write_all(format!("{}\n", json!({"jsonrpc":"2.0","id":id,"result":42})).as_bytes()).await.unwrap();
            pw.write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"log\",\"params\":{\"message\":\"hi\"}}\n").await.unwrap();
            pw.write_all(&vec![b'x'; MAX_LINE + 10]).await.unwrap();
            pw.write_all(b"\n").await.unwrap();
        });

        assert_eq!(peer.request("ping", json!({}), Duration::from_secs(2)).await.unwrap(), json!(42));
        assert!(matches!(rx.recv().await, Some(Incoming::Call { method, .. }) if method == "log"));
        assert!(matches!(rx.recv().await, Some(Incoming::Violation(_))));
    }
}

//! Bridge to the Discord desktop app's local IPC socket. Plugins can't open sockets or pipes
//! themselves, so the companion relays frames for plugins granted the "discord" bridge.

use std::sync::Arc;

use anyhow::{Context, Result};
use serde_json::{Value, json};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::mpsc;

use crate::app::App;

const MAX_FRAME: u32 = 1024 * 1024;

#[derive(Clone)]
pub struct Bridge {
    tx: mpsc::Sender<(u32, Value)>,
    /// Relay tasks. Dropping the last handle ends them, so a replaced or stopped bridge
    /// doesn't keep reading from Discord.
    _tasks: Arc<Tasks>,
}

struct Tasks(Vec<tokio::task::JoinHandle<()>>);

impl Drop for Tasks {
    fn drop(&mut self) {
        for t in &self.0 {
            t.abort();
        }
    }
}

impl Bridge {
    pub async fn open_discord(app: Arc<App>, plugin: String) -> Result<Self> {
        let (reader, writer) = connect().await.context("Discord is not running")?;
        let (tx, mut rx) = mpsc::channel::<(u32, Value)>(64);
        let writer_task = tokio::spawn(async move {
            let mut w = writer;
            while let Some((op, payload)) = rx.recv().await {
                let body = payload.to_string().into_bytes();
                let mut frame = Vec::with_capacity(8 + body.len());
                frame.extend_from_slice(&op.to_le_bytes());
                frame.extend_from_slice(&(body.len() as u32).to_le_bytes());
                frame.extend_from_slice(&body);
                if w.write_all(&frame).await.is_err() {
                    break;
                }
            }
        });
        let reader_task = tokio::spawn(async move {
            let mut r = reader;
            loop {
                let mut head = [0u8; 8];
                if r.read_exact(&mut head).await.is_err() {
                    break;
                }
                let op = u32::from_le_bytes(head[..4].try_into().expect("4 bytes"));
                let len = u32::from_le_bytes(head[4..].try_into().expect("4 bytes"));
                if len > MAX_FRAME {
                    break;
                }
                let mut body = vec![0u8; len as usize];
                if r.read_exact(&mut body).await.is_err() {
                    break;
                }
                let payload: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
                super::ipc_deliver(&app, &plugin, "ipc.message", json!({ "bridge": "discord", "data": { "op": op, "payload": payload } })).await;
            }
            super::ipc_deliver(&app, &plugin, "ipc.closed", json!({ "bridge": "discord" })).await;
        });
        Ok(Self { tx, _tasks: Arc::new(Tasks(vec![writer_task, reader_task])) })
    }

    /// `data` is {"op": number, "payload": object}. Doesn't wait: a Discord that stopped
    /// reading must not hold up the plugin's other calls.
    pub fn send(&self, data: Value) -> Result<()> {
        let op = data.get("op").and_then(Value::as_u64).filter(|o| *o <= 4).context("invalid op")? as u32;
        let payload = data.get("payload").cloned().unwrap_or(Value::Null);
        anyhow::ensure!(payload.to_string().len() < MAX_FRAME as usize, "payload too large");
        self.tx.try_send((op, payload)).map_err(|e| match e {
            mpsc::error::TrySendError::Full(_) => anyhow::anyhow!("Discord isn't keeping up"),
            mpsc::error::TrySendError::Closed(_) => anyhow::anyhow!("bridge closed"),
        })
    }
}

type Halves = (Box<dyn AsyncRead + Unpin + Send>, Box<dyn AsyncWrite + Unpin + Send>);

#[cfg(unix)]
async fn connect() -> Result<Halves> {
    let mut dirs: Vec<std::path::PathBuf> = ["XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"].iter().filter_map(std::env::var_os).map(Into::into).collect();
    dirs.push("/tmp".into());
    for d in dirs {
        for sub in ["", "app/com.discordapp.Discord", "snap.discord", ".flatpak/dev.vencord.Vesktop/xdg-run"] {
            for i in 0..10 {
                let p = d.join(sub).join(format!("discord-ipc-{i}"));
                if let Ok(s) = tokio::net::UnixStream::connect(&p).await {
                    let (r, w) = s.into_split();
                    return Ok((Box::new(r), Box::new(w)));
                }
            }
        }
    }
    anyhow::bail!("no Discord IPC socket found")
}

#[cfg(windows)]
async fn connect() -> Result<Halves> {
    for i in 0..10 {
        let name = format!(r"\\?\pipe\discord-ipc-{i}");
        if let Ok(p) = tokio::net::windows::named_pipe::ClientOptions::new().open(&name) {
            let (r, w) = tokio::io::split(p);
            return Ok((Box::new(r), Box::new(w)));
        }
    }
    anyhow::bail!("no Discord IPC pipe found")
}

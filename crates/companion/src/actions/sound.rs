//! Soundboard playback on the PC. rodio output handles are not Send, so playback lives on one thread.

use std::path::PathBuf;
use std::sync::mpsc;

enum Cmd {
    Play { path: PathBuf, volume: f32, stop_others: bool },
    StopAll,
}

pub struct Sound {
    tx: mpsc::Sender<Cmd>,
}

impl Sound {
    pub fn start() -> Self {
        let (tx, rx) = mpsc::channel();
        let _ = std::thread::Builder::new().name("sound".into()).spawn(move || run(rx));
        Self { tx }
    }

    pub fn play(&self, path: PathBuf, volume: f32, stop_others: bool) -> Result<(), String> {
        self.tx.send(Cmd::Play { path, volume, stop_others }).map_err(|_| "sound output stopped".to_string())
    }

    pub fn stop_all(&self) {
        let _ = self.tx.send(Cmd::StopAll);
    }
}

fn run(rx: mpsc::Receiver<Cmd>) {
    let mut sink: Option<rodio::MixerDeviceSink> = None;
    let mut players: Vec<rodio::Player> = Vec::new();
    while let Ok(cmd) = rx.recv() {
        players.retain(|p| !p.empty());
        match cmd {
            Cmd::StopAll => players.clear(),
            Cmd::Play { path, volume, stop_others } => {
                if stop_others {
                    players.clear();
                }
                if sink.is_none() {
                    match rodio::DeviceSinkBuilder::open_default_sink() {
                        Ok(mut s) => {
                            s.log_on_drop(false);
                            sink = Some(s);
                        }
                        Err(e) => {
                            tracing::warn!("no sound output: {e}");
                            continue;
                        }
                    }
                }
                let Some(s) = sink.as_ref() else { continue };
                let decoded = std::fs::File::open(&path).map_err(|e| e.to_string()).and_then(|f| rodio::Decoder::try_from(f).map_err(|e| e.to_string()));
                match decoded {
                    Ok(src) => {
                        let p = rodio::Player::connect_new(s.mixer());
                        p.set_volume(volume);
                        p.append(src);
                        players.push(p);
                        if players.len() > 16 {
                            players.remove(0);
                        }
                    }
                    Err(e) => tracing::warn!("could not play {}: {e}", path.display()),
                }
            }
        }
    }
}

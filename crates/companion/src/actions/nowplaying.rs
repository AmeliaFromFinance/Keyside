//! What's playing in any media player: MPRIS on Linux, the system media controls on Windows.
//! Publishes media.title, media.artist, media.album, media.player and media.playing, and
//! shows the track and cover on "Now playing" keys.

use std::sync::Arc;
use std::time::Duration;

use serde_json::{Map, Value, json};

use crate::app::App;
use crate::model::Tone;
use crate::state::Change;
use crate::state::view::ViewPatch;

pub const KIND: &str = "core.nowplaying";

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Track {
    pub player: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub playing: bool,
    /// Changes when the cover does: a URL on Linux, the track on Windows.
    pub art_key: String,
}

#[derive(Debug, Clone, Copy)]
pub enum Control {
    Toggle,
    Next,
    Previous,
    Stop,
}

#[cfg(target_os = "linux")]
#[path = "nowplaying/mpris.rs"]
mod backend;
#[cfg(windows)]
#[path = "nowplaying/smtc.rs"]
mod backend;
#[cfg(not(any(target_os = "linux", windows)))]
mod backend {
    use super::{Control, Track};
    pub struct Backend;
    impl Backend {
        pub async fn new() -> Option<Self> {
            None
        }
        pub async fn current(&mut self) -> Option<Track> {
            None
        }
        pub async fn control(&mut self, _: Control) -> Result<(), String> {
            Err("Now playing isn't available on this system yet".into())
        }
        pub async fn art(&mut self, _: &Track, _: &reqwest::Client) -> Option<Vec<u8>> {
            None
        }
    }
}

/// A key press for the watcher, which owns the connection to the system, and where to answer.
type Request = (Control, tokio::sync::oneshot::Sender<Result<(), String>>);
static CONTROL: std::sync::OnceLock<tokio::sync::mpsc::Sender<Request>> = std::sync::OnceLock::new();

pub async fn press(m: &Map<String, Value>) -> Result<(), String> {
    let c = match m.get("op").and_then(Value::as_str).unwrap_or("toggle") {
        "toggle" => Control::Toggle,
        "next" => Control::Next,
        "previous" => Control::Previous,
        _ => return Err("unknown media control".into()),
    };
    control(c).await
}

/// Sends a control to the current player.
pub async fn control(c: Control) -> Result<(), String> {
    let tx = CONTROL.get().ok_or("Now playing isn't available on this system yet")?;
    let (done, wait) = tokio::sync::oneshot::channel();
    tx.send((c, done)).await.map_err(|_| "media watcher stopped")?;
    wait.await.map_err(|_| "media watcher stopped")?
}

pub fn spawn(app: Arc<App>) {
    let (tx, mut rx) = tokio::sync::mpsc::channel(8);
    let _ = CONTROL.set(tx);
    tokio::spawn(async move {
        let Some(mut be) = backend::Backend::new().await else {
            tracing::info!("now playing is off: no media service found");
            app.engine.lock().set_var("media.available", Value::Bool(false));
            show(&app, None, None);
            return;
        };
        let mut layout = app.engine.lock().subscribe();
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        let mut last: Option<Track> = None;
        let mut art: Option<String> = None;
        let mut art_key = String::new();
        loop {
            tokio::select! {
                _ = tick.tick() => {}
                Some((c, done)) = rx.recv() => {
                    let _ = done.send(be.control(c).await);
                }
                r = layout.recv() => {
                    if matches!(r, Ok(Change::Layout)) {
                        show(&app, last.as_ref(), art.as_deref());
                    }
                    continue;
                }
            }
            let now = be.current().await;
            if now == last {
                continue;
            }
            if let Some(t) = &now
                && t.art_key != art_key
            {
                art_key = t.art_key.clone();
                art = match if art_key.is_empty() { None } else { be.art(t, &app.http).await } {
                    // Decoding a big cover takes a moment, so it runs off the async threads.
                    Some(bytes) => tokio::task::spawn_blocking(move || cover(&bytes)).await.ok().flatten(),
                    None => None,
                };
            }
            if now.is_none() {
                art = None;
                art_key.clear();
            }
            publish(&app, now.as_ref());
            show(&app, now.as_ref(), art.as_deref());
            last = now;
        }
    });
}

fn publish(app: &App, t: Option<&Track>) {
    let empty = Track::default();
    let t = t.unwrap_or(&empty);
    let mut e = app.engine.lock();
    e.set_var("media.title", json!(t.title));
    e.set_var("media.artist", json!(t.artist));
    e.set_var("media.album", json!(t.album));
    e.set_var("media.player", json!(t.player));
    e.set_var("media.playing", json!(t.playing));
}

/// Puts the track on every "Now playing" key.
fn show(app: &App, t: Option<&Track>, art: Option<&str>) {
    let mut e = app.engine.lock();
    for inst in e.instances_of(KIND) {
        let with_art = inst.settings.get("cover").and_then(Value::as_bool).unwrap_or(true);
        let patch = match t {
            None => ViewPatch { sub: Some("Nothing playing".into()), tone: Some(Tone::Muted), ..Default::default() },
            Some(t) => ViewPatch {
                label: Some(if t.title.is_empty() { t.player.clone() } else { t.title.clone() }),
                sub: Some(if t.artist.is_empty() { t.player.clone() } else { t.artist.clone() }),
                badge: (!t.playing).then(|| "PAUSED".into()),
                badge_tone: (!t.playing).then_some(Tone::Muted),
                icon: Some(if t.playing { "circle-pause" } else { "circle-play" }.into()),
                image: if with_art { art.map(str::to_string) } else { None },
                ..Default::default()
            },
        };
        e.set_override(&inst.id, Some(patch));
    }
}

/// Shrinks a cover to a small JPEG data URL for keys.
fn cover(bytes: &[u8]) -> Option<String> {
    use base64::Engine as _;
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format().ok()?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(64 * 1024 * 1024);
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    reader.limits(limits);
    let img = reader.decode().ok()?.thumbnail(200, 200).to_rgb8();
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 80).encode_image(&img).ok()?;
    Some(format!("data:image/jpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(out)))
}

/// Reads a cover from a local file or an https link, up to 5 MB.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) async fn fetch_art(url: &str, http: &reqwest::Client) -> Option<Vec<u8>> {
    const MAX: usize = 5 * 1024 * 1024;
    if url.starts_with("file://") {
        let path = url::Url::parse(url).ok()?.to_file_path().ok()?;
        let meta = tokio::fs::metadata(&path).await.ok()?;
        if !meta.is_file() || meta.len() as usize > MAX {
            return None;
        }
        return tokio::fs::read(&path).await.ok();
    }
    if !url.starts_with("https://") {
        return None;
    }
    let mut r = http.get(url).timeout(Duration::from_secs(5)).send().await.ok()?.error_for_status().ok()?;
    if r.content_length().is_some_and(|n| n as usize > MAX) {
        return None;
    }
    let mut out = Vec::new();
    while let Some(chunk) = r.chunk().await.ok()? {
        if out.len() + chunk.len() > MAX {
            return None;
        }
        out.extend_from_slice(&chunk);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn reads_covers_from_files_only_or_https() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("My Cover.jpg");
        std::fs::write(&path, b"img").unwrap();
        let url = url::Url::from_file_path(&path).unwrap().to_string();
        let _ = rustls::crypto::ring::default_provider().install_default();
        let http = reqwest::Client::new();
        assert_eq!(super::fetch_art(&url, &http).await.as_deref(), Some(&b"img"[..]));
        assert!(super::fetch_art("http://192.168.1.1/x.jpg", &http).await.is_none(), "plain http is refused");
        assert!(super::fetch_art("ftp://x/y", &http).await.is_none());
    }

    #[test]
    fn covers_become_small_jpegs() {
        let img = image::RgbImage::from_pixel(800, 600, image::Rgb([200, 30, 30]));
        let mut png = Vec::new();
        image::DynamicImage::ImageRgb8(img).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        let url = super::cover(&png).unwrap();
        assert!(url.starts_with("data:image/jpeg;base64,"));
        assert!(url.len() < 20_000);
        assert!(super::cover(b"not an image").is_none());
    }
}

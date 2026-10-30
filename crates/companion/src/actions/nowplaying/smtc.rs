//! The Windows system media controls, which Spotify, browsers and most players report to.

use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session, GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
};
use windows::Storage::Streams::DataReader;

use super::{Control, Track};

pub struct Backend {
    manager: Manager,
}

/// WinRT calls block, so they run off the async threads.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> windows::core::Result<T> + Send + 'static) -> Option<T> {
    tokio::task::spawn_blocking(f).await.ok()?.ok()
}

fn session(m: &Manager) -> windows::core::Result<Session> {
    m.GetCurrentSession()
}

impl Backend {
    pub async fn new() -> Option<Self> {
        let manager = blocking(|| Manager::RequestAsync()?.join()).await?;
        Some(Self { manager })
    }

    pub async fn current(&mut self) -> Option<Track> {
        let m = self.manager.clone();
        blocking(move || {
            let s = session(&m)?;
            let props = s.TryGetMediaPropertiesAsync()?.join()?;
            let playing = s.GetPlaybackInfo()?.PlaybackStatus()? == Status::Playing;
            let app = s.SourceAppUserModelId()?.to_string();
            // "Spotify.exe" or "Microsoft.ZuneMusic_8wekyb3d8bbwe!Microsoft.ZuneMusic": keep the readable part.
            let player = app.rsplit(['!', '\\']).next().unwrap_or(&app).trim_end_matches(".exe").to_string();
            let title = props.Title()?.to_string();
            let album = props.AlbumTitle()?.to_string();
            Ok(Track { art_key: format!("{player}\u{0}{title}\u{0}{album}"), player, title, artist: props.Artist()?.to_string(), album, playing })
        })
        .await
    }

    pub async fn control(&mut self, c: Control) -> Result<(), String> {
        let m = self.manager.clone();
        blocking(move || {
            let s = session(&m)?;
            match c {
                Control::Toggle => s.TryTogglePlayPauseAsync()?.join(),
                Control::Next => s.TrySkipNextAsync()?.join(),
                Control::Previous => s.TrySkipPreviousAsync()?.join(),
            }
        })
        .await
        .filter(|ok| *ok)
        .map(|_| ())
        .ok_or_else(|| "Nothing is playing, or the player didn't accept that".to_string())
    }

    pub async fn art(&mut self, _: &Track, _: &reqwest::Client) -> Option<Vec<u8>> {
        let m = self.manager.clone();
        blocking(move || {
            let props = session(&m)?.TryGetMediaPropertiesAsync()?.join()?;
            let stream = props.Thumbnail()?.OpenReadAsync()?.join()?;
            let size = stream.Size()?.min(5 * 1024 * 1024) as u32;
            let reader = DataReader::CreateDataReader(&stream)?;
            reader.LoadAsync(size)?.join()?;
            let mut buf = vec![0u8; size as usize];
            reader.ReadBytes(&mut buf)?;
            Ok(buf)
        })
        .await
    }
}

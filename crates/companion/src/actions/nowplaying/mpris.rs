//! MPRIS over the D-Bus session bus. Every Linux media player and browser speaks it.

use std::collections::HashMap;

use zbus::proxy::CacheProperties;
use zbus::zvariant::OwnedValue;

use super::{Control, Track};

const PREFIX: &str = "org.mpris.MediaPlayer2.";
const PATH: &str = "/org/mpris/MediaPlayer2";

pub struct Backend {
    conn: zbus::Connection,
    /// Bus name of the player the keys follow.
    current: Option<String>,
    art_url: String,
}

impl Backend {
    pub async fn new() -> Option<Self> {
        let conn = zbus::Connection::session().await.ok()?;
        Some(Self { conn, current: None, art_url: String::new() })
    }

    async fn proxy(&self, dest: &str, iface: &'static str) -> zbus::Result<zbus::Proxy<'static>> {
        zbus::proxy::Builder::new(&self.conn).destination(dest.to_string())?.path(PATH)?.interface(iface)?.cache_properties(CacheProperties::No).build().await
    }

    async fn players(&self) -> Vec<String> {
        let Ok(dbus) = zbus::fdo::DBusProxy::new(&self.conn).await else { return Vec::new() };
        let mut names: Vec<String> = dbus.list_names().await.unwrap_or_default().into_iter().map(|n| n.to_string()).filter(|n| n.starts_with(PREFIX)).collect();
        names.sort();
        names
    }

    async fn read(&self, name: &str) -> Option<(Track, String)> {
        let p = self.proxy(name, "org.mpris.MediaPlayer2.Player").await.ok()?;
        let status: String = p.get_property("PlaybackStatus").await.ok()?;
        let meta: HashMap<String, OwnedValue> = p.get_property("Metadata").await.unwrap_or_default();
        let text = |k: &str| meta.get(k).and_then(|v| String::try_from(v.try_clone().ok()?).ok()).unwrap_or_default();
        let artists = meta.get("xesam:artist").and_then(|v| Vec::<String>::try_from(v.try_clone().ok()?).ok()).unwrap_or_default();
        let identity = match self.proxy(name, "org.mpris.MediaPlayer2").await {
            Ok(root) => root.get_property::<String>("Identity").await.unwrap_or_default(),
            Err(_) => String::new(),
        };
        let player = if identity.is_empty() { name.trim_start_matches(PREFIX).split('.').next().unwrap_or_default().to_string() } else { identity };
        let art = text("mpris:artUrl");
        let t = Track { player, title: text("xesam:title"), artist: artists.join(", "), album: text("xesam:album"), playing: status == "Playing", art_key: art.clone() };
        (status != "Stopped" || !t.title.is_empty()).then_some((t, art))
    }

    /// The playing player, sticking with the last one while it's still around.
    pub async fn current(&mut self) -> Option<Track> {
        let mut found: Vec<(String, Track, String)> = Vec::new();
        for name in self.players().await {
            if let Some((t, art)) = self.read(&name).await {
                found.push((name, t, art));
            }
        }
        let pick = found
            .iter()
            .position(|(n, t, _)| t.playing && self.current.as_deref() == Some(n.as_str()))
            .or_else(|| found.iter().position(|(_, t, _)| t.playing))
            .or_else(|| found.iter().position(|(n, _, _)| self.current.as_deref() == Some(n.as_str())))
            .or_else(|| (!found.is_empty()).then_some(0))?;
        let (name, t, art) = found.swap_remove(pick);
        self.current = Some(name);
        self.art_url = art;
        Some(t)
    }

    pub async fn control(&mut self, c: Control) -> Result<(), String> {
        let name = self.current.clone().ok_or("Nothing is playing")?;
        let p = self.proxy(&name, "org.mpris.MediaPlayer2.Player").await.map_err(|e| e.to_string())?;
        let method = match c {
            Control::Toggle => "PlayPause",
            Control::Next => "Next",
            Control::Previous => "Previous",
        };
        p.call_method(method, &()).await.map(|_| ()).map_err(|e| format!("The player didn't accept that: {e}"))
    }

    pub async fn art(&mut self, _: &Track, http: &reqwest::Client) -> Option<Vec<u8>> {
        super::fetch_art(&self.art_url, http).await
    }
}

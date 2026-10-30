//! Keystrokes, typed text and media keys. Runs on its own thread because some
//! platforms tie input APIs to the thread that created them.

use std::sync::mpsc;

#[cfg_attr(not(feature = "input"), allow(dead_code))]
enum Cmd {
    Keys(Vec<KeyName>),
    Text(String),
    Media(Media),
}

/// A command and where to send its result, so a key that fails says why.
type Job = (Cmd, tokio::sync::oneshot::Sender<Result<(), String>>);

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Media {
    PlayPause,
    Next,
    Prev,
    Stop,
    VolumeUp,
    VolumeDown,
    Mute,
}

#[derive(Debug, Clone, PartialEq)]
pub enum KeyName {
    Ctrl,
    Shift,
    Alt,
    Meta,
    Char(char),
    F(u8),
    Named(&'static str),
}

pub struct Input {
    tx: Option<mpsc::Sender<Job>>,
}

/// Parses "ctrl+shift+f13" style combos. Modifiers first, one main key last.
pub fn parse_combo(s: &str) -> Result<Vec<KeyName>, String> {
    let parts: Vec<String> = s.split('+').map(|p| p.trim().to_ascii_lowercase()).collect();
    if parts.is_empty() || parts.len() > 5 || parts.iter().any(String::is_empty) {
        return Err("invalid key combination".into());
    }
    let mut out = Vec::new();
    for (i, p) in parts.iter().enumerate() {
        let last = i == parts.len() - 1;
        let k = match p.as_str() {
            "ctrl" | "control" => KeyName::Ctrl,
            "shift" => KeyName::Shift,
            "alt" | "option" => KeyName::Alt,
            "meta" | "win" | "cmd" | "super" => KeyName::Meta,
            _ if !last => return Err(format!("\"{p}\" is not a modifier")),
            p if p.chars().count() == 1 && p.chars().all(|c| c.is_ascii_alphanumeric() || "`-=[];',./\\".contains(c)) => KeyName::Char(p.chars().next().unwrap_or(' ')),
            p if p.starts_with('f') && p[1..].parse::<u8>().is_ok_and(|n| (1..=20).contains(&n)) => KeyName::F(p[1..].parse().unwrap_or(1)),
            p => match NAMED.iter().find(|n| **n == p) {
                Some(n) => KeyName::Named(n),
                None => return Err(format!("unknown key \"{p}\"")),
            },
        };
        out.push(k);
    }
    Ok(out)
}

const NAMED: &[&str] = &["enter", "tab", "space", "esc", "backspace", "delete", "up", "down", "left", "right", "home", "end", "pageup", "pagedown", "insert", "capslock"];

pub fn parse_media(s: &str) -> Result<Media, String> {
    Ok(match s {
        "play" | "play-pause" => Media::PlayPause,
        "next" => Media::Next,
        "prev" | "previous" => Media::Prev,
        "stop" if cfg!(target_os = "macos") => return Err("Macs have no media stop key. Use play or pause instead.".into()),
        "stop" => Media::Stop,
        "volume-up" => Media::VolumeUp,
        "volume-down" => Media::VolumeDown,
        "mute" => Media::Mute,
        _ => return Err("unknown media key".into()),
    })
}

impl Input {
    pub fn start() -> Self {
        #[cfg(feature = "input")]
        {
            let (tx, rx) = mpsc::channel();
            let ok = std::thread::Builder::new().name("input".into()).spawn(move || imp::run(rx)).is_ok();
            return Self { tx: ok.then_some(tx) };
        }
        #[allow(unreachable_code)]
        Self { tx: None }
    }

    async fn send(&self, c: Cmd) -> Result<(), String> {
        let (done, wait) = tokio::sync::oneshot::channel();
        self.tx.as_ref().ok_or("keyboard control is not available in this build")?.send((c, done)).map_err(|_| "keyboard control stopped".to_string())?;
        // Long text takes a few seconds to type.
        tokio::time::timeout(std::time::Duration::from_secs(60), wait)
            .await
            .map_err(|_| "keyboard control didn't answer".to_string())?
            .map_err(|_| "keyboard control stopped".to_string())?
    }

    pub async fn keys(&self, combo: &str) -> Result<(), String> {
        self.send(Cmd::Keys(parse_combo(combo)?)).await
    }

    pub async fn text(&self, t: &str) -> Result<(), String> {
        self.send(Cmd::Text(t.to_string())).await
    }

    pub async fn media(&self, m: &str) -> Result<(), String> {
        let m = parse_media(m)?;
        // Linux desktops differ in which keys reach players, so ask the player and the sound
        // server directly first.
        #[cfg(target_os = "linux")]
        if linux_media(m).await.is_ok() {
            return Ok(());
        }
        self.send(Cmd::Media(m)).await
    }
}

#[cfg(target_os = "linux")]
async fn linux_media(m: Media) -> Result<(), String> {
    #[cfg(feature = "nowplaying")]
    {
        use super::nowplaying::{Control, control};
        let c = match m {
            Media::PlayPause => Some(Control::Toggle),
            Media::Next => Some(Control::Next),
            Media::Prev => Some(Control::Previous),
            Media::Stop => Some(Control::Stop),
            _ => None,
        };
        if let Some(c) = c {
            return control(c).await;
        }
    }
    let (wpctl, pactl): (&[&str], &[&str]) = match m {
        Media::VolumeUp => (&["set-volume", "-l", "1.0", "@DEFAULT_AUDIO_SINK@", "5%+"], &["set-sink-volume", "@DEFAULT_SINK@", "+5%"]),
        Media::VolumeDown => (&["set-volume", "@DEFAULT_AUDIO_SINK@", "5%-"], &["set-sink-volume", "@DEFAULT_SINK@", "-5%"]),
        Media::Mute => (&["set-mute", "@DEFAULT_AUDIO_SINK@", "toggle"], &["set-sink-mute", "@DEFAULT_SINK@", "toggle"]),
        _ => return Err("no player".into()),
    };
    for (program, args) in [("wpctl", wpctl), ("pactl", pactl)] {
        let Some(path) = crate::programs::find(program) else { continue };
        let mut cmd = tokio::process::Command::new(path);
        cmd.args(args).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
        crate::programs::apply_tokio(&mut cmd);
        if cmd.status().await.is_ok_and(|s| s.success()) {
            return Ok(());
        }
    }
    Err("no sound server".into())
}

#[cfg(all(feature = "input", target_os = "linux"))]
#[path = "input/wayland.rs"]
mod wayland;

#[cfg(feature = "input")]
mod imp {
    use super::{Cmd, Job, KeyName, Media};
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};
    use std::sync::mpsc::Receiver;

    enum Backend {
        #[cfg(target_os = "linux")]
        Wayland(super::wayland::Keyboard),
        Enigo(Enigo),
    }

    /// Wayland desktops get the virtual keyboard. Elsewhere, or if the desktop refuses it,
    /// enigo (X11 on Linux, which on Wayland only reaches X11 apps).
    fn open() -> Result<Backend, String> {
        #[cfg(target_os = "linux")]
        let wayland_err = if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            match super::wayland::Keyboard::connect() {
                Ok(k) => return Ok(Backend::Wayland(k)),
                Err(e) => {
                    tracing::warn!("Wayland keyboard unavailable, trying X11: {e}");
                    Some(e)
                }
            }
        } else {
            None
        };
        #[cfg(not(target_os = "linux"))]
        let wayland_err: Option<String> = None;
        Enigo::new(&Settings::default()).map(Backend::Enigo).map_err(|e| match wayland_err {
            Some(w) => format!("Keyside can't send keys here: {w}"),
            None => format!("Keyside can't send keys here: {e}"),
        })
    }

    pub fn run(rx: Receiver<Job>) {
        let mut backend: Option<Backend> = None;
        while let Ok((cmd, done)) = rx.recv() {
            if backend.is_none() {
                match open() {
                    Ok(b) => backend = Some(b),
                    Err(e) => {
                        let _ = done.send(Err(e));
                        continue;
                    }
                }
            }
            let Some(b) = backend.as_mut() else { continue };
            let r = match b {
                #[cfg(target_os = "linux")]
                Backend::Wayland(k) => wayland(k, &cmd),
                Backend::Enigo(en) => match &cmd {
                    Cmd::Keys(keys) => combo(en, keys),
                    Cmd::Text(t) => en.text(t),
                    Cmd::Media(m) => en.key(media(*m), Direction::Click),
                }
                .map_err(|e| format!("couldn't send keys: {e}")),
            };
            if r.is_err() {
                // Reconnect next time, say after the desktop restarted.
                backend = None;
            }
            let _ = done.send(r);
        }
    }

    #[cfg(target_os = "linux")]
    fn wayland(k: &mut super::wayland::Keyboard, cmd: &Cmd) -> Result<(), String> {
        use super::wayland::{ALT, CTRL, SHIFT, SUPER, key_sym, media_sym};
        match cmd {
            Cmd::Keys(keys) => {
                let (last, mods) = keys.split_last().ok_or("no key")?;
                let mask = mods.iter().fold(0, |m, k| {
                    m | match k {
                        KeyName::Ctrl => CTRL,
                        KeyName::Shift => SHIFT,
                        KeyName::Alt => ALT,
                        KeyName::Meta => SUPER,
                        _ => 0,
                    }
                });
                k.combo(mask, &key_sym(last))
            }
            Cmd::Text(t) => k.text(t),
            Cmd::Media(m) => k.combo(0, media_sym(*m)),
        }
    }

    fn combo(en: &mut Enigo, keys: &[KeyName]) -> enigo::InputResult<()> {
        let mapped: Vec<Key> = keys.iter().map(key).collect();
        let (last, mods) = mapped.split_last().expect("parse_combo returns at least one key");
        for m in mods {
            en.key(*m, Direction::Press)?;
        }
        let r = en.key(*last, Direction::Click);
        for m in mods.iter().rev() {
            let _ = en.key(*m, Direction::Release);
        }
        r
    }

    fn key(k: &KeyName) -> Key {
        const F: [Key; 20] = [
            Key::F1,
            Key::F2,
            Key::F3,
            Key::F4,
            Key::F5,
            Key::F6,
            Key::F7,
            Key::F8,
            Key::F9,
            Key::F10,
            Key::F11,
            Key::F12,
            Key::F13,
            Key::F14,
            Key::F15,
            Key::F16,
            Key::F17,
            Key::F18,
            Key::F19,
            Key::F20,
        ];
        match k {
            KeyName::Ctrl => Key::Control,
            KeyName::Shift => Key::Shift,
            KeyName::Alt => Key::Alt,
            KeyName::Meta => Key::Meta,
            KeyName::Char(c) => Key::Unicode(*c),
            KeyName::F(n) => F.get(*n as usize - 1).copied().unwrap_or(Key::F12),
            KeyName::Named(n) => match *n {
                "enter" => Key::Return,
                "tab" => Key::Tab,
                "space" => Key::Space,
                "esc" => Key::Escape,
                "backspace" => Key::Backspace,
                "delete" => Key::Delete,
                "up" => Key::UpArrow,
                "down" => Key::DownArrow,
                "left" => Key::LeftArrow,
                "right" => Key::RightArrow,
                "home" => Key::Home,
                "end" => Key::End,
                "pageup" => Key::PageUp,
                "pagedown" => Key::PageDown,
                "capslock" => Key::CapsLock,
                #[cfg(not(target_os = "macos"))]
                "insert" => Key::Insert,
                _ => Key::Escape,
            },
        }
    }

    fn media(m: Media) -> Key {
        match m {
            Media::PlayPause => Key::MediaPlayPause,
            Media::Next => Key::MediaNextTrack,
            Media::Prev => Key::MediaPrevTrack,
            #[cfg(not(target_os = "macos"))]
            Media::Stop => Key::MediaStop,
            // Never sent on macOS: parse_media refuses "stop" there.
            #[cfg(target_os = "macos")]
            Media::Stop => Key::MediaPlayPause,
            Media::VolumeUp => Key::VolumeUp,
            Media::VolumeDown => Key::VolumeDown,
            Media::Mute => Key::VolumeMute,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combos() {
        assert_eq!(parse_combo("Ctrl+Shift+F13").unwrap(), vec![KeyName::Ctrl, KeyName::Shift, KeyName::F(13)]);
        assert_eq!(parse_combo("alt+tab").unwrap(), vec![KeyName::Alt, KeyName::Named("tab")]);
        assert!(parse_combo("a+b").is_err());
        assert!(parse_combo("ctrl+").is_err());
        assert!(parse_combo("ctrl+f25").is_err());
        assert!(parse_combo("ctrl+shift+alt+meta+ctrl+x").is_err());
    }
}

//! Keystrokes, typed text and media keys. Runs on its own thread because some
//! platforms tie input APIs to the thread that created them.

use std::sync::mpsc;

#[cfg_attr(not(feature = "input"), allow(dead_code))]
enum Cmd {
    Keys(Vec<KeyName>),
    Text(String),
    Media(Media),
}

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
    tx: Option<mpsc::Sender<Cmd>>,
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
            p if p.chars().count() == 1 && p.chars().all(|c| c.is_ascii_alphanumeric() || "`-=[];',./\\".contains(c)) => {
                KeyName::Char(p.chars().next().unwrap_or(' '))
            }
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

    fn send(&self, c: Cmd) -> Result<(), String> {
        self.tx.as_ref().ok_or("keyboard control is not available in this build")?.send(c).map_err(|_| "keyboard control stopped".to_string())
    }

    pub fn keys(&self, combo: &str) -> Result<(), String> {
        self.send(Cmd::Keys(parse_combo(combo)?))
    }

    pub fn text(&self, t: &str) -> Result<(), String> {
        self.send(Cmd::Text(t.to_string()))
    }

    pub fn media(&self, m: &str) -> Result<(), String> {
        self.send(Cmd::Media(parse_media(m)?))
    }
}

#[cfg(feature = "input")]
mod imp {
    use super::{Cmd, KeyName, Media};
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};
    use std::sync::mpsc::Receiver;

    pub fn run(rx: Receiver<Cmd>) {
        let mut enigo: Option<Enigo> = None;
        while let Ok(cmd) = rx.recv() {
            if enigo.is_none() {
                match Enigo::new(&Settings::default()) {
                    Ok(e) => enigo = Some(e),
                    Err(e) => {
                        tracing::warn!("keyboard control unavailable: {e}");
                        continue;
                    }
                }
            }
            let Some(en) = enigo.as_mut() else { continue };
            let r = match cmd {
                Cmd::Keys(keys) => combo(en, &keys),
                Cmd::Text(t) => en.text(&t),
                Cmd::Media(m) => en.key(media(m), Direction::Click),
            };
            if let Err(e) = r {
                tracing::warn!("keyboard action failed: {e}");
            }
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
        match k {
            KeyName::Ctrl => Key::Control,
            KeyName::Shift => Key::Shift,
            KeyName::Alt => Key::Alt,
            KeyName::Meta => Key::Meta,
            KeyName::Char(c) => Key::Unicode(*c),
            KeyName::F(n) => [
                Key::F1, Key::F2, Key::F3, Key::F4, Key::F5, Key::F6, Key::F7, Key::F8, Key::F9, Key::F10, Key::F11, Key::F12,
                Key::F13, Key::F14, Key::F15, Key::F16, Key::F17, Key::F18, Key::F19, Key::F20,
            ]
            .get(*n as usize - 1)
            .copied()
            .unwrap_or(Key::F12),
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
                _ => Key::Escape,
            },
        }
    }

    fn media(m: Media) -> Key {
        match m {
            Media::PlayPause => Key::MediaPlayPause,
            Media::Next => Key::MediaNextTrack,
            Media::Prev => Key::MediaPrevTrack,
            Media::Stop => Key::MediaStop,
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

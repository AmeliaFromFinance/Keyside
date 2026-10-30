//! Typing on Wayland through the virtual keyboard protocol (wlroots desktops like Hyprland and
//! Sway). Each batch of keys gets a small keymap holding exactly the symbols it needs, so any
//! character can be typed whatever keyboard layout is in use. No input method is involved, so
//! it never competes with fcitx5 or IBus.

use std::fs::File;
use std::io::Write;
use std::os::fd::{AsFd, FromRawFd};
use std::time::{Duration, Instant};

use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::{wl_registry, wl_seat};
use wayland_client::{Connection, Dispatch, EventQueue, QueueHandle};
use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1;
use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1;

/// Keycodes per keymap. XKB allows up to 255, and 8 are reserved.
const PER_KEYMAP: usize = 200;
const XKB_V1: u32 = 1;

pub const SHIFT: u32 = 1;
pub const CTRL: u32 = 4;
pub const ALT: u32 = 8;
pub const SUPER: u32 = 64;

struct State;

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
    fn event(_: &mut Self, _: &wl_registry::WlRegistry, _: wl_registry::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
}
wayland_client::delegate_noop!(State: ignore wl_seat::WlSeat);
wayland_client::delegate_noop!(State: ZwpVirtualKeyboardManagerV1);
wayland_client::delegate_noop!(State: ZwpVirtualKeyboardV1);

pub struct Keyboard {
    conn: Connection,
    queue: EventQueue<State>,
    kb: ZwpVirtualKeyboardV1,
    start: Instant,
    /// Symbols in the keymap the compositor has now, in keycode order.
    mapped: Vec<String>,
}

impl Keyboard {
    pub fn connect() -> Result<Self, String> {
        let conn = Connection::connect_to_env().map_err(|e| format!("couldn't reach the Wayland desktop: {e}"))?;
        let (globals, queue) = registry_queue_init::<State>(&conn).map_err(|e| format!("couldn't reach the Wayland desktop: {e}"))?;
        let qh = queue.handle();
        let seat: wl_seat::WlSeat = globals.bind(&qh, 1..=7, ()).map_err(|_| "the desktop has no keyboard seat".to_string())?;
        let manager: ZwpVirtualKeyboardManagerV1 = globals.bind(&qh, 1..=1, ()).map_err(|_| "this desktop doesn't let apps send keys (no virtual keyboard support)".to_string())?;
        let kb = manager.create_virtual_keyboard(&seat, &qh, ());
        Ok(Self { conn, queue, kb, start: Instant::now(), mapped: Vec::new() })
    }

    /// Presses one key with modifiers held, like Super+Shift+T.
    pub fn combo(&mut self, mods: u32, sym: &str) -> Result<(), String> {
        self.map(&[sym.to_string()])?;
        let code = self.code(sym);
        self.kb.modifiers(mods, 0, 0, 0);
        self.press(code);
        self.kb.modifiers(0, 0, 0, 0);
        self.flush()
    }

    pub fn text(&mut self, text: &str) -> Result<(), String> {
        let syms: Vec<String> = text.chars().filter_map(char_sym).collect();
        // Split long text so each part fits in one keymap.
        let mut start = 0;
        while start < syms.len() {
            let mut unique: Vec<String> = Vec::new();
            let mut end = start;
            while end < syms.len() && (unique.contains(&syms[end]) || unique.len() < PER_KEYMAP) {
                if !unique.contains(&syms[end]) {
                    unique.push(syms[end].clone());
                }
                end += 1;
            }
            self.map(&unique)?;
            for s in &syms[start..end] {
                let code = self.code(s);
                self.press(code);
                self.flush()?;
                // Some apps drop keys that arrive in the same instant.
                std::thread::sleep(Duration::from_millis(2));
            }
            start = end;
        }
        Ok(())
    }

    fn press(&self, code: u32) {
        let t = self.start.elapsed().as_millis() as u32;
        self.kb.key(t, code, 1);
        self.kb.key(t, code, 0);
    }

    /// Linux key code for a symbol in the current keymap. XKB codes start 8 above Linux ones.
    fn code(&self, sym: &str) -> u32 {
        self.mapped.iter().position(|m| m == sym).map_or(0, |i| i as u32 + 1)
    }

    /// Sends a keymap with these symbols, unless the current one already has them all.
    fn map(&mut self, syms: &[String]) -> Result<(), String> {
        if syms.iter().all(|s| self.mapped.contains(s)) {
            return Ok(());
        }
        let mut km = format!("xkb_keymap {{\nxkb_keycodes \"keyside\" {{ minimum = 8; maximum = {}; ", syms.len() + 8);
        for i in 0..syms.len() {
            km += &format!("<K{i}> = {}; ", i + 9);
        }
        km += "};\nxkb_types \"keyside\" { include \"complete\" };\nxkb_compatibility \"keyside\" { include \"complete\" };\nxkb_symbols \"keyside\" { ";
        for (i, s) in syms.iter().enumerate() {
            km += &format!("key <K{i}> {{ [ {s} ] }}; ");
        }
        km += "};\n};\n\0";

        // SAFETY: memfd_create returns a new descriptor that we own, or -1.
        let fd = unsafe { libc::memfd_create(c"keyside-keymap".as_ptr(), libc::MFD_CLOEXEC) };
        if fd < 0 {
            return Err("couldn't prepare the keymap".into());
        }
        // SAFETY: fd is valid and not owned by anything else.
        let mut file = unsafe { File::from_raw_fd(fd) };
        file.write_all(km.as_bytes()).map_err(|e| format!("couldn't prepare the keymap: {e}"))?;
        self.kb.keymap(XKB_V1, file.as_fd(), km.len() as u32);
        self.mapped = syms.to_vec();
        // Wait for the desktop to load the keymap before sending keys that use it.
        self.queue.roundtrip(&mut State).map_err(|e| format!("the Wayland desktop stopped answering: {e}"))?;
        Ok(())
    }

    fn flush(&self) -> Result<(), String> {
        self.conn.flush().map_err(|e| format!("the Wayland desktop stopped answering: {e}"))
    }
}

/// XKB symbol for a typed character. Uxxxx names cover all of Unicode.
fn char_sym(c: char) -> Option<String> {
    match c {
        '\n' => Some("Return".into()),
        '\t' => Some("Tab".into()),
        c if c.is_control() => None,
        c => Some(format!("U{:04X}", c as u32)),
    }
}

/// XKB symbol for a key name from a combo or media key.
pub fn key_sym(k: &super::KeyName) -> String {
    use super::KeyName;
    match k {
        KeyName::Char(c) => char_sym(*c).unwrap_or_else(|| "space".into()),
        KeyName::F(n) => format!("F{n}"),
        KeyName::Named(n) => match *n {
            "enter" => "Return",
            "tab" => "Tab",
            "space" => "space",
            "esc" => "Escape",
            "backspace" => "BackSpace",
            "delete" => "Delete",
            "up" => "Up",
            "down" => "Down",
            "left" => "Left",
            "right" => "Right",
            "home" => "Home",
            "end" => "End",
            "pageup" => "Prior",
            "pagedown" => "Next",
            "insert" => "Insert",
            "capslock" => "Caps_Lock",
            _ => "Escape",
        }
        .into(),
        KeyName::Ctrl => "Control_L".into(),
        KeyName::Shift => "Shift_L".into(),
        KeyName::Alt => "Alt_L".into(),
        KeyName::Meta => "Super_L".into(),
    }
}

pub fn media_sym(m: super::Media) -> &'static str {
    use super::Media;
    match m {
        Media::PlayPause => "XF86AudioPlay",
        Media::Next => "XF86AudioNext",
        Media::Prev => "XF86AudioPrev",
        Media::Stop => "XF86AudioStop",
        Media::VolumeUp => "XF86AudioRaiseVolume",
        Media::VolumeDown => "XF86AudioLowerVolume",
        Media::Mute => "XF86AudioMute",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbols() {
        assert_eq!(char_sym('a').unwrap(), "U0061");
        assert_eq!(char_sym('ü').unwrap(), "U00FC");
        assert_eq!(char_sym('😀').unwrap(), "U1F600");
        assert_eq!(char_sym('\n').unwrap(), "Return");
        assert_eq!(char_sym('\u{7}'), None);
    }
}

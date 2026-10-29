//! MIDI output. Connections are opened per message; MIDI steps are rare enough for that.

use midir::MidiOutput;

pub fn ports() -> Vec<String> {
    let Ok(out) = MidiOutput::new("PhoneDeck") else { return Vec::new() };
    out.ports().iter().filter_map(|p| out.port_name(p).ok()).collect()
}

pub fn send(port: &str, msg: &[u8]) -> Result<(), String> {
    let out = MidiOutput::new("PhoneDeck").map_err(|e| e.to_string())?;
    let p = out.ports().into_iter().find(|p| out.port_name(p).is_ok_and(|n| n == port)).ok_or_else(|| format!("MIDI port \"{port}\" not found"))?;
    let mut conn = out.connect(&p, "phonedeck").map_err(|e| e.to_string())?;
    conn.send(msg).map_err(|e| e.to_string())
}

//! Draws a key the way the web deck does, as an image for a physical Stream Deck.

use std::collections::HashMap;
use std::sync::OnceLock;

use ab_glyph::{Font, FontRef, PxScale, ScaleFont, point};
use base64::Engine;
use image::{DynamicImage, RgbaImage};
use resvg::tiny_skia::{Paint, Pixmap, Rect, Transform};
use resvg::usvg;

use crate::model::{KeyColor, KeyKind, Tone};
use crate::state::view::KeyView;

const FONT: &[u8] = include_bytes!("../../../assets/fonts/Inter-SemiBold.ttf");
const ICONS: &str = include_str!("../../../assets/icons.json");

const TEXT: [u8; 3] = [0xed, 0xeb, 0xf2];
const MUTED: [u8; 3] = [0xa8, 0xa3, 0xb5];

fn font() -> &'static FontRef<'static> {
    static F: OnceLock<FontRef<'static>> = OnceLock::new();
    F.get_or_init(|| FontRef::try_from_slice(FONT).expect("bundled font"))
}

fn icons() -> &'static HashMap<String, String> {
    static I: OnceLock<HashMap<String, String>> = OnceLock::new();
    I.get_or_init(|| serde_json::from_str(ICONS).unwrap_or_default())
}

fn hex(h: u32) -> [u8; 3] {
    [(h >> 16) as u8, (h >> 8) as u8, h as u8]
}

fn background(v: &KeyView) -> [u8; 3] {
    match v.tone {
        Some(Tone::Accent) => return hex(0x252e38),
        Some(Tone::Danger) => return hex(0x2e1f26),
        _ => {}
    }
    hex(match v.color.unwrap_or(KeyColor::Default) {
        KeyColor::Default => 0x24222d,
        KeyColor::Teal => 0x2c5566,
        KeyColor::Violet => 0x4a2f5c,
        KeyColor::Green => 0x2f4a38,
        KeyColor::Amber => 0x5c3a2b,
        KeyColor::Red => 0x5c2b31,
        KeyColor::Blue => 0x2b3d66,
    })
}

fn tone_color(t: Option<Tone>) -> [u8; 3] {
    hex(match t {
        Some(Tone::Accent) => 0x9ee6ff,
        Some(Tone::Danger) => 0xff8a8a,
        Some(Tone::Success) => 0x7ee2a8,
        Some(Tone::Warning) => 0xf2c879,
        Some(Tone::Info) => 0x9ec1ff,
        Some(Tone::Muted) => 0x6f6a7c,
        _ => 0xedebf2,
    })
}

fn paint(c: [u8; 3], a: u8) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color_rgba8(c[0], c[1], c[2], a);
    p.anti_alias = true;
    p
}

fn fill(pix: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, c: [u8; 3], a: u8) {
    if let Some(r) = Rect::from_xywh(x, y, w, h) {
        pix.fill_rect(r, &paint(c, a), Transform::identity(), None);
    }
}

fn width(text: &str, px: f32) -> f32 {
    let f = font().as_scaled(PxScale::from(px));
    text.chars().map(|c| f.h_advance(f.glyph_id(c))).sum()
}

/// Splits into at most `max` lines that fit, ending with an ellipsis if cut.
fn wrap(text: &str, px: f32, room: f32, max: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    for word in text.split_whitespace() {
        let next = if cur.is_empty() { word.to_string() } else { format!("{cur} {word}") };
        if width(&next, px) <= room || cur.is_empty() {
            cur = next;
        } else {
            lines.push(std::mem::take(&mut cur));
            cur = word.to_string();
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    if lines.len() > max {
        lines.truncate(max);
        lines[max - 1].push('…');
    }
    for l in &mut lines {
        while width(l, px) > room && l.chars().count() > 1 {
            l.pop();
            l.pop();
            l.push('…');
        }
    }
    lines
}

fn text(pix: &mut Pixmap, s: &str, px: f32, x: f32, baseline: f32, c: [u8; 3]) {
    let f = font();
    let sf = f.as_scaled(PxScale::from(px));
    let (pw, ph) = (pix.width() as i32, pix.height() as i32);
    let data = pix.data_mut();
    let mut caret = x;
    let mut prev = None;
    for ch in s.chars() {
        let id = f.glyph_id(ch);
        if let Some(p) = prev {
            caret += sf.kern(p, id);
        }
        let g = id.with_scale_and_position(px, point(caret, baseline));
        caret += sf.h_advance(id);
        prev = Some(id);
        let Some(o) = f.outline_glyph(g) else { continue };
        let b = o.px_bounds();
        o.draw(|gx, gy, cov| {
            let (px_, py) = (b.min.x as i32 + gx as i32, b.min.y as i32 + gy as i32);
            if px_ < 0 || py < 0 || px_ >= pw || py >= ph {
                return;
            }
            let i = ((py * pw + px_) * 4) as usize;
            let a = cov.clamp(0.0, 1.0);
            for k in 0..3 {
                data[i + k] = (c[k] as f32 * a + data[i + k] as f32 * (1.0 - a)) as u8;
            }
            data[i + 3] = 255;
        });
    }
}

fn svg(pix: &mut Pixmap, source: &str, x: f32, y: f32, size: f32) {
    let Ok(tree) = usvg::Tree::from_str(source, &usvg::Options::default()) else { return };
    let s = tree.size();
    let scale = (size / s.width()).min(size / s.height());
    let dx = x + (size - s.width() * scale) / 2.0;
    let dy = y + (size - s.height() * scale) / 2.0;
    resvg::render(&tree, Transform::from_scale(scale, scale).post_translate(dx, dy), &mut pix.as_mut());
}

fn icon(pix: &mut Pixmap, name: &str, x: f32, y: f32, size: f32, c: [u8; 3]) {
    let Some(inner) = icons().get(name) else { return };
    let source = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\" width=\"24\" height=\"24\" fill=\"none\" stroke=\"#{:02x}{:02x}{:02x}\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\">{inner}</svg>",
        c[0], c[1], c[2]
    );
    svg(pix, &source, x, y, size);
}

/// Draws a data URL image to cover the key.
fn picture(pix: &mut Pixmap, url: &str) -> bool {
    let Some((head, body)) = url.split_once(',') else { return false };
    let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(body) else { return false };
    let size = pix.width() as f32;
    if head.starts_with("data:image/svg+xml") {
        svg(pix, &String::from_utf8_lossy(&bytes), 0.0, 0.0, size);
        return true;
    }
    let Ok(img) = image::load_from_memory(&bytes) else { return false };
    let img = img.resize_to_fill(pix.width(), pix.height(), image::imageops::FilterType::Triangle).to_rgba8();
    let data = pix.data_mut();
    for (i, p) in img.pixels().enumerate() {
        let a = p[3] as f32 / 255.0;
        for k in 0..3 {
            data[i * 4 + k] = (p[k] as f32 * a + data[i * 4 + k] as f32 * (1.0 - a)) as u8;
        }
        data[i * 4 + 3] = 255;
    }
    true
}

fn clock(since: u64) -> String {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as u64;
    let s = now.saturating_sub(since) / 1000;
    if s >= 3600 { format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60) } else { format!("{}:{:02}", s / 60, s % 60) }
}

pub fn key(v: &KeyView, w: u32, h: u32) -> DynamicImage {
    let Some(mut pix) = Pixmap::new(w.max(1), h.max(1)) else { return DynamicImage::new_rgba8(w, h) };
    let s = w.min(h) as f32;
    let bg = background(v);
    fill(&mut pix, 0.0, 0.0, w as f32, h as f32, bg, 255);

    let has_picture = v.image.as_deref().is_some_and(|i| picture(&mut pix, i));
    let label = v.label.trim();
    let fg = if v.disabled { MUTED } else { TEXT };
    if has_picture {
        if !label.is_empty() {
            fill(&mut pix, 0.0, h as f32 - s * 0.3, w as f32, s * 0.3, [0, 0, 0], 150);
        }
    } else if let Some(name) = v.icon.as_deref().or(match v.kind {
        KeyKind::Folder => Some("folder"),
        KeyKind::Back => Some("arrow-left"),
        _ => None,
    }) {
        let size = if label.is_empty() { s * 0.5 } else { s * 0.34 };
        let y = if label.is_empty() { (h as f32 - size) / 2.0 } else { s * 0.19 };
        icon(&mut pix, name, (w as f32 - size) / 2.0, y, size, if v.tone == Some(Tone::Danger) { tone_color(Some(Tone::Danger)) } else { fg });
    }

    let px = s * 0.15;
    if !label.is_empty() {
        let lines = wrap(label, px, w as f32 - s * 0.12, 2);
        let line_h = px * 1.15;
        let mut base = h as f32 - s * 0.09 - line_h * (lines.len() as f32 - 1.0);
        for l in &lines {
            text(&mut pix, l, px, (w as f32 - width(l, px)) / 2.0, base, fg);
            base += line_h;
        }
    }

    let corner = v.badge.clone().or_else(|| v.since.map(clock)).or_else(|| v.meta.clone());
    if let Some(b) = corner.filter(|b| !b.is_empty()) {
        let bpx = s * 0.12;
        let b = wrap(&b, bpx, w as f32 * 0.8, 1).pop().unwrap_or_default();
        let bw = width(&b, bpx) + s * 0.08;
        let x = w as f32 - bw - s * 0.05;
        let tone = v.badge_tone.or(if v.badge.is_some() { v.tone } else { None });
        let filled = matches!(tone, Some(Tone::Accent | Tone::Danger)) && v.badge.is_some();
        if filled {
            fill(&mut pix, x, s * 0.05, bw, bpx * 1.35, tone_color(tone), 255);
        }
        let c = if filled { hex(0x121118) } else { tone_color(tone) };
        text(&mut pix, &b, bpx, x + s * 0.04, s * 0.05 + bpx * 1.05, c);
    }

    if let Some(p) = v.progress.or(if v.kind == KeyKind::Slider { v.value } else { None }) {
        let bar = s * 0.05;
        fill(&mut pix, 0.0, h as f32 - bar, w as f32, bar, hex(0x34313f), 255);
        fill(&mut pix, 0.0, h as f32 - bar, w as f32 * p.clamp(0.0, 1.0) as f32, bar, tone_color(Some(Tone::Accent)), 255);
    }
    if matches!(v.tone, Some(Tone::Accent | Tone::Danger)) {
        let c = tone_color(v.tone);
        let t = (s * 0.035).max(2.0);
        fill(&mut pix, 0.0, 0.0, w as f32, t, c, 255);
        fill(&mut pix, 0.0, h as f32 - t, w as f32, t, c, 255);
        fill(&mut pix, 0.0, 0.0, t, h as f32, c, 255);
        fill(&mut pix, w as f32 - t, 0.0, t, h as f32, c, 255);
    }

    let img = RgbaImage::from_raw(w, h, pix.take()).unwrap_or_else(|| RgbaImage::new(w, h));
    DynamicImage::ImageRgba8(img)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view() -> KeyView {
        KeyView {
            id: "k".into(),
            kind: KeyKind::Button,
            x: 0,
            y: 0,
            w: 1,
            h: 1,
            label: "Starting Soon".into(),
            sub: None,
            icon: Some("monitor".into()),
            color: None,
            badge: Some("ON AIR".into()),
            tone: Some(Tone::Accent),
            badge_tone: Some(Tone::Accent),
            meta: None,
            image: None,
            value: None,
            progress: None,
            since: None,
            hold: None,
            disabled: false,
            lines: None,
        }
    }

    #[test]
    fn draws_keys_at_device_sizes() {
        for size in [72, 80, 96, 120] {
            let img = key(&view(), size, size).to_rgba8();
            assert_eq!(img.dimensions(), (size, size));
            // Label text and icon strokes add near-white pixels to the dark key.
            assert!(img.pixels().filter(|p| p[0] > 200 && p[1] > 200 && p[2] > 200).count() > 20);
        }
    }

    #[test]
    fn writes_a_preview() {
        if let Ok(dir) = std::env::var("PHONEDECK_RENDER_PREVIEW") {
            let mut v = view();
            key(&v, 96, 96).save(format!("{dir}/key-accent.png")).unwrap();
            v.tone = None;
            v.badge = None;
            v.color = Some(KeyColor::Violet);
            v.label = "Desktop audio".into();
            v.icon = Some("volume-2".into());
            key(&v, 96, 96).save(format!("{dir}/key-violet.png")).unwrap();
            v.tone = Some(Tone::Danger);
            v.badge = Some("MUTED".into());
            v.badge_tone = Some(Tone::Danger);
            v.icon = Some("mic-off".into());
            v.label = "Mic".into();
            key(&v, 96, 96).save(format!("{dir}/key-danger.png")).unwrap();
        }
    }

    #[test]
    fn wraps_long_labels() {
        let lines = wrap("A very long label that keeps going and going", 12.0, 60.0, 2);
        assert_eq!(lines.len(), 2);
        assert!(lines[1].ends_with('…'));
    }
}

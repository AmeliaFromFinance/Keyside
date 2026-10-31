//! Look of an overlay, read from its address (?size=96&color=ffcc00…). Every value is checked
//! against a fixed list or pattern and rebuilt from the parsed value, so nothing from the
//! address reaches the stylesheet or the page as typed. Anything unknown keeps the default.

use std::collections::HashMap;
use std::fmt::Write;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    Timer,
    Text,
}

/// A colour as red, green, blue and alpha.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color([u8; 4]);

impl Color {
    const WHITE: Self = Self([255, 255, 255, 255]);
    const SHADOW: Self = Self([0, 0, 0, 0x99]);

    /// `#rgb`, `#rrggbb` or `#rrggbbaa` (the `#` is optional), or one of a few colour names.
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        let named = match s.to_ascii_lowercase().as_str() {
            "white" => Some("fff"),
            "black" => Some("000"),
            "red" => Some("f00"),
            "green" => Some("008000"),
            "blue" => Some("00f"),
            "yellow" => Some("ff0"),
            "orange" => Some("ffa500"),
            "purple" => Some("800080"),
            "pink" => Some("ffc0cb"),
            "gray" | "grey" => Some("808080"),
            "cyan" => Some("0ff"),
            "magenta" => Some("f0f"),
            _ => None,
        };
        let hex = named.unwrap_or_else(|| s.strip_prefix('#').unwrap_or(s));
        if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let nib = |i: usize| u8::from_str_radix(&hex[i..=i], 16).ok();
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
        match hex.len() {
            3 => Some(Self([nib(0)? * 17, nib(1)? * 17, nib(2)? * 17, 255])),
            6 => Some(Self([byte(0)?, byte(2)?, byte(4)?, 255])),
            8 => Some(Self([byte(0)?, byte(2)?, byte(4)?, byte(6)?])),
            _ => None,
        }
    }

    /// Hex without `#`, as used in links.
    fn hex(self) -> String {
        let [r, g, b, a] = self.0;
        if a == 255 { format!("{r:02x}{g:02x}{b:02x}") } else { format!("{r:02x}{g:02x}{b:02x}{a:02x}") }
    }

    fn css(self) -> String {
        format!("#{}", self.hex())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Font {
    Sans,
    Serif,
    Mono,
    Inter,
    JetBrainsMono,
    /// A font installed on the PC: letters, digits, spaces and `-` only.
    Installed(String),
}

impl Font {
    fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        Some(match s.to_ascii_lowercase().as_str() {
            "sans" => Self::Sans,
            "serif" => Self::Serif,
            "mono" => Self::Mono,
            "inter" => Self::Inter,
            "jetbrains-mono" | "jetbrains mono" => Self::JetBrainsMono,
            _ if (1..=40).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b' ' || b == b'-') => Self::Installed(s.to_string()),
            _ => return None,
        })
    }

    fn key(&self) -> &str {
        match self {
            Self::Sans => "sans",
            Self::Serif => "serif",
            Self::Mono => "mono",
            Self::Inter => "inter",
            Self::JetBrainsMono => "jetbrains-mono",
            Self::Installed(n) => n,
        }
    }

    fn family(&self) -> String {
        match self {
            Self::Sans => "system-ui, sans-serif".into(),
            Self::Serif => "ui-serif, Georgia, \"Times New Roman\", serif".into(),
            Self::Mono => "ui-monospace, \"Cascadia Mono\", Menlo, Consolas, monospace".into(),
            Self::Inter => "\"Inter Variable\", system-ui, sans-serif".into(),
            Self::JetBrainsMono => "\"JetBrains Mono\", ui-monospace, monospace".into(),
            // The name only holds letters, digits, spaces and -, so quoting it is enough.
            Self::Installed(n) => format!("\"{n}\", system-ui, sans-serif"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Align {
    Left,
    Center,
    Right,
}

impl Align {
    fn key(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Center => "center",
            Self::Right => "right",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shadow {
    pub color: Color,
    pub x: i32,
    pub y: i32,
    pub blur: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Style {
    pub kind: Kind,
    pub font: Font,
    pub size: i32,
    pub weight: i32,
    pub color: Color,
    pub bg: Option<Color>,
    pub align: Align,
    /// `None` keeps the default of a little space around the text that grows with its size.
    pub pad: Option<i32>,
    pub spacing: i32,
    pub upper: bool,
    pub shadow: Option<Shadow>,
    pub glow: Option<(Color, i32)>,
    pub outline: Option<(Color, i32)>,
}

const SHADOW: Shadow = Shadow { color: Color::SHADOW, x: 0, y: 2, blur: 6 };
const GLOW_SIZE: i32 = 10;
const OUTLINE_WIDTH: i32 = 2;

/// A whole number in range. Out-of-range values are clamped, anything else is `None`.
fn num(q: &HashMap<String, String>, key: &str, min: i32, max: i32) -> Option<i32> {
    let v: f64 = q.get(key)?.trim().parse().ok()?;
    v.is_finite().then(|| (v.round() as i32).clamp(min, max))
}

impl Style {
    pub fn new(kind: Kind) -> Self {
        Self {
            kind,
            font: Font::Sans,
            size: if kind == Kind::Timer { 120 } else { 48 },
            weight: 700,
            color: Color::WHITE,
            bg: None,
            align: if kind == Kind::Timer { Align::Center } else { Align::Left },
            pad: None,
            spacing: 0,
            upper: false,
            shadow: Some(SHADOW),
            glow: None,
            outline: None,
        }
    }

    pub fn from_query(kind: Kind, q: &HashMap<String, String>) -> Self {
        let mut s = Self::new(kind);
        let get = |k: &str| q.get(k).map(|v| v.trim());
        let color = |k: &str| get(k).and_then(Color::parse);
        if let Some(f) = get("font").and_then(Font::parse) {
            s.font = f;
        }
        if let Some(n) = num(q, "size", 8, 1000) {
            s.size = n;
        }
        match get("weight") {
            Some("normal") => s.weight = 400,
            Some("bold") => s.weight = 700,
            _ => {
                if let Some(n) = num(q, "weight", 100, 900) {
                    s.weight = (n + 50) / 100 * 100;
                }
            }
        }
        if let Some(c) = color("color").or_else(|| color("colour")) {
            s.color = c;
        }
        s.bg = color("bg");
        match get("align") {
            Some("left") => s.align = Align::Left,
            Some("center") => s.align = Align::Center,
            Some("right") => s.align = Align::Right,
            _ => {}
        }
        s.pad = num(q, "pad", 0, 200);
        s.spacing = num(q, "spacing", -20, 50).unwrap_or(0);
        s.upper = get("upper") == Some("1");
        match get("shadow") {
            Some("0" | "off" | "none") => s.shadow = None,
            v => {
                let base = v.and_then(Color::parse).unwrap_or(SHADOW.color);
                s.shadow = Some(Shadow {
                    color: base,
                    x: num(q, "shadow-x", -50, 50).unwrap_or(SHADOW.x),
                    y: num(q, "shadow-y", -50, 50).unwrap_or(SHADOW.y),
                    blur: num(q, "shadow-blur", 0, 100).unwrap_or(SHADOW.blur),
                });
            }
        }
        s.glow = color("glow").map(|c| (c, num(q, "glow-size", 1, 100).unwrap_or(GLOW_SIZE)));
        s.outline = color("outline").map(|c| (c, num(q, "outline-width", 1, 20).unwrap_or(OUTLINE_WIDTH)));
        s
    }

    /// The options that differ from the defaults, as a query string built from the parsed values.
    pub fn query(&self) -> String {
        let d = Self::new(self.kind);
        let mut out = vec![format!("kind={}", if self.kind == Kind::Timer { "timer" } else { "text" })];
        let mut add = |k: &str, v: String| out.push(format!("{k}={v}"));
        if self.font != d.font {
            add("font", self.font.key().replace(' ', "%20"));
        }
        if self.size != d.size {
            add("size", self.size.to_string());
        }
        if self.weight != d.weight {
            add("weight", self.weight.to_string());
        }
        if self.color != d.color {
            add("color", self.color.hex());
        }
        if let Some(c) = self.bg {
            add("bg", c.hex());
        }
        if self.align != d.align {
            add("align", self.align.key().into());
        }
        if let Some(p) = self.pad {
            add("pad", p.to_string());
        }
        if self.spacing != 0 {
            add("spacing", self.spacing.to_string());
        }
        if self.upper {
            add("upper", "1".into());
        }
        match self.shadow {
            None => add("shadow", "0".into()),
            Some(sh) if sh != SHADOW => {
                add("shadow", sh.color.hex());
                add("shadow-x", sh.x.to_string());
                add("shadow-y", sh.y.to_string());
                add("shadow-blur", sh.blur.to_string());
            }
            Some(_) => {}
        }
        if let Some((c, n)) = self.glow {
            add("glow", c.hex());
            add("glow-size", n.to_string());
        }
        if let Some((c, n)) = self.outline {
            add("outline", c.hex());
            add("outline-width", n.to_string());
        }
        out.join("&")
    }

    /// The whole stylesheet. `asset` finds the file of a bundled font by name.
    pub fn css(&self, asset: impl Fn(&str) -> Option<String>) -> String {
        let mut css = String::new();
        let faces: &[(&str, &str, &str)] = match self.font {
            Font::Inter => &[("Inter Variable", "inter-{}-wght-normal", "100 900")],
            Font::JetBrainsMono => &[("JetBrains Mono", "jetbrains-mono-{}-400-normal", "400")],
            _ => &[],
        };
        for (family, file, weight) in faces {
            for (subset, range) in SUBSETS {
                if let Some(path) = asset(&file.replace("{}", subset)) {
                    let _ = writeln!(
                        css,
                        "@font-face{{font-family:\"{family}\";font-style:normal;font-display:block;font-weight:{weight};src:url(\"/{path}\") format(\"woff2\");unicode-range:{range}}}"
                    );
                }
            }
        }
        let bg = self.bg.map_or("transparent".into(), Color::css);
        let _ = writeln!(css, "html,body{{margin:0;background:{bg};overflow:hidden}}");
        let pad = self.pad.map_or("0.1em 0.2em".into(), |p| format!("{p}px"));
        let mut shadows = Vec::new();
        if let Some((c, n)) = self.glow {
            let c = c.css();
            // Three soft layers of growing size read as a glow.
            shadows.extend([n as f32 / 2.0, n as f32, n as f32 * 2.0].map(|b| format!("0 0 {b}px {c}")));
        }
        if let Some(sh) = self.shadow {
            shadows.push(format!("{}px {}px {}px {}", sh.x, sh.y, sh.blur, sh.color.css()));
        }
        let shadow = if shadows.is_empty() { "none".into() } else { shadows.join(",") };
        let _ = write!(
            css,
            "#out{{color:{};font-family:{};font-weight:{};font-size:{}px;line-height:1.15;white-space:pre-line;text-shadow:{shadow};padding:{pad}",
            self.color.css(),
            self.font.family(),
            self.weight,
            self.size
        );
        if self.align != Align::Left {
            let _ = write!(css, ";text-align:{}", self.align.key());
        }
        if self.kind == Kind::Timer {
            css.push_str(";font-variant-numeric:tabular-nums");
        }
        if self.spacing != 0 {
            let _ = write!(css, ";letter-spacing:{}px", self.spacing);
        }
        if self.upper {
            css.push_str(";text-transform:uppercase");
        }
        if let Some((c, w)) = self.outline {
            // The stroke is centred on the letter edge and drawn under the fill, so twice the width shows the asked width outside.
            let _ = write!(css, ";-webkit-text-stroke:{}px {};paint-order:stroke fill", w * 2, c.css());
        }
        css.push_str("}\n");
        css
    }
}

/// Character ranges of the bundled font files, the same for both fonts.
const SUBSETS: [(&str, &str); 4] = [
    ("latin", "U+0000-00FF,U+0131,U+0152-0153,U+02BB-02BC,U+02C6,U+02DA,U+02DC,U+0304,U+0308,U+0329,U+2000-206F,U+20AC,U+2122,U+2191,U+2193,U+2212,U+2215,U+FEFF,U+FFFD"),
    (
        "latin-ext",
        "U+0100-02BA,U+02BD-02C5,U+02C7-02CC,U+02CE-02D7,U+02DD-02FF,U+0304,U+0308,U+0329,U+1D00-1DBF,U+1E00-1E9F,U+1EF2-1EFF,U+2020,U+20A0-20AB,U+20AD-20C0,U+2113,U+2C60-2C7F,U+A720-A7FF",
    ),
    ("cyrillic", "U+0301,U+0400-045F,U+0490-0491,U+04B0-04B1,U+2116"),
    ("greek", "U+0370-0377,U+037A-037F,U+0384-038A,U+038C,U+038E-03A1,U+03A3-03FF"),
];

/// How a timer shows its time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TimeFormat {
    /// Minutes and seconds, with hours in front once there are any.
    Auto,
    Hms,
    Ms,
    Secs,
}

impl TimeFormat {
    pub fn parse(s: Option<&String>) -> Self {
        match s.map(|s| s.trim()) {
            Some("hh:mm:ss") => Self::Hms,
            Some("mm:ss") => Self::Ms,
            Some("ss") => Self::Secs,
            _ => Self::Auto,
        }
    }

    /// Rewrites a timer value like `04:59` or `1:02:05`. Anything else is left as it is.
    pub fn apply(self, shown: &str) -> String {
        let parts: Option<Vec<u64>> = shown.split(':').map(|p| p.parse().ok()).collect();
        let secs = match parts.as_deref() {
            Some([m, s]) => m * 60 + s,
            Some([h, m, s]) => h * 3600 + m * 60 + s,
            _ => return shown.to_string(),
        };
        match self {
            Self::Auto => shown.to_string(),
            Self::Hms => format!("{:02}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60),
            Self::Ms => format!("{:02}:{:02}", secs / 60, secs % 60),
            Self::Secs => secs.to_string(),
        }
    }
}

/// Makes text safe to put inside an HTML attribute.
pub fn attr(s: &str) -> String {
    s.replace('&', "&amp;").replace('"', "&quot;").replace('<', "&lt;").replace('>', "&gt;").replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    fn no_fonts(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn defaults_keep_the_old_look() {
        let timer = Style::from_query(Kind::Timer, &q(&[]));
        assert_eq!(timer, Style::new(Kind::Timer));
        assert_eq!(timer.query(), "kind=timer");
        let css = timer.css(no_fonts);
        assert!(css.contains("background:transparent"));
        assert!(css.contains("#out{color:#ffffff;font-family:system-ui, sans-serif;font-weight:700;font-size:120px;line-height:1.15;white-space:pre-line;text-shadow:0px 2px 6px #00000099;padding:0.1em 0.2em;text-align:center;font-variant-numeric:tabular-nums}"));
        let text = Style::new(Kind::Text).css(no_fonts);
        assert!(text.contains("font-size:48px") && !text.contains("text-align") && !text.contains("tabular"));
    }

    #[test]
    fn reads_options() {
        let s = Style::from_query(
            Kind::Text,
            &q(&[
                ("font", "Open Sans"),
                ("size", "96"),
                ("weight", "640"),
                ("colour", "#FC0"),
                ("bg", "00000080"),
                ("align", "right"),
                ("pad", "12"),
                ("spacing", "-2"),
                ("upper", "1"),
                ("shadow", "ff0000"),
                ("shadow-blur", "3"),
                ("glow", "00ffff"),
                ("glow-size", "8"),
                ("outline", "000"),
                ("outline-width", "3"),
            ]),
        );
        assert_eq!(s.font, Font::Installed("Open Sans".into()));
        assert_eq!((s.size, s.weight, s.align, s.pad, s.spacing, s.upper), (96, 600, Align::Right, Some(12), -2, true));
        assert_eq!(s.color, Color([255, 204, 0, 255]));
        assert_eq!(s.bg, Some(Color([0, 0, 0, 128])));
        assert_eq!(s.shadow, Some(Shadow { color: Color([255, 0, 0, 255]), x: 0, y: 2, blur: 3 }));
        let css = s.css(no_fonts);
        assert!(css.contains("font-family:\"Open Sans\", system-ui, sans-serif"));
        assert!(css.contains("text-shadow:0 0 4px #00ffff,0 0 8px #00ffff,0 0 16px #00ffff,0px 2px 3px #ff0000"));
        assert!(css.contains("-webkit-text-stroke:6px #000000;paint-order:stroke fill"));
        assert!(css.contains("background:#00000080"));
        // What the page links to reads back as the same style.
        let again: HashMap<String, String> = url::form_urlencoded::parse(s.query().as_bytes()).into_owned().collect();
        assert_eq!(Style::from_query(Kind::Text, &again), s);
        assert_eq!(Style::from_query(Kind::Timer, &q(&[("weight", "bold"), ("font", "Inter"), ("shadow", "0")])).query(), "kind=timer&font=inter&shadow=0");
    }

    #[test]
    fn clamps_numbers() {
        let s = Style::from_query(
            Kind::Timer,
            &q(&[("size", "99999"), ("weight", "5"), ("pad", "-4"), ("glow", "fff"), ("glow-size", "1e9"), ("outline", "fff"), ("outline-width", "NaN")]),
        );
        assert_eq!((s.size, s.weight, s.pad), (1000, 100, Some(0)));
        assert_eq!(s.glow, Some((Color::WHITE, 100)));
        assert_eq!(s.outline, Some((Color::WHITE, OUTLINE_WIDTH)));
        assert_eq!(Style::from_query(Kind::Timer, &q(&[("size", "big"), ("size ", "9")])).size, 120);
    }

    #[test]
    fn ignores_anything_unsafe() {
        let bad = [
            ("font", "x\";}body{background:url(//evil)}"),
            ("font", "Comic</style><script>"),
            ("font", &"a".repeat(41)),
            ("color", "red;background:url(x)"),
            ("color", "#12345"),
            ("color", "expression(alert(1))"),
            ("bg", "url(x)"),
            ("align", "center;color:red"),
            ("shadow", "000 0 0 red"),
            ("glow", "inherit"),
            ("outline", "#ggg"),
            ("weight", "bolder"),
        ];
        let s = Style::from_query(Kind::Text, &q(&bad));
        assert_eq!(s, Style::new(Kind::Text));
        let css = s.css(no_fonts);
        assert!(!css.contains(['<', '>', '"', '\\']));
        assert!(!css.contains("evil") && !css.contains("url(") && !css.contains("script"));
        assert_eq!(attr("a&b\"<x>'"), "a&amp;b&quot;&lt;x&gt;&#39;");
    }

    #[test]
    fn bundled_fonts() {
        let found = |n: &str| (n == "inter-latin-wght-normal").then(|| "assets/inter-latin-wght-normal-Ab_1.woff2".to_string());
        let css = Style::from_query(Kind::Text, &q(&[("font", "inter")])).css(found);
        assert!(css.starts_with("@font-face{font-family:\"Inter Variable\""));
        assert!(css.contains("src:url(\"/assets/inter-latin-wght-normal-Ab_1.woff2\")"));
        assert_eq!(css.matches("@font-face").count(), 1);
        assert!(!Style::new(Kind::Text).css(found).contains("@font-face"));
    }

    #[test]
    fn time_formats() {
        assert_eq!(TimeFormat::parse(Some(&"hh:mm:ss".into())).apply("04:59"), "00:04:59");
        assert_eq!(TimeFormat::parse(Some(&"mm:ss".into())).apply("1:02:05"), "62:05");
        assert_eq!(TimeFormat::parse(Some(&"ss".into())).apply("01:05"), "65");
        assert_eq!(TimeFormat::parse(Some(&"nope".into())).apply("1:02:05"), "1:02:05");
        assert_eq!(TimeFormat::Hms.apply("soon"), "soon");
    }
}

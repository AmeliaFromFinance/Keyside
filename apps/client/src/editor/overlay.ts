// Builds OBS overlay links from the look picked in the editor. The companion checks every
// option again; this only leaves out the ones that match its defaults, to keep links short.

export type OverlayKind = "timer" | "text";

export interface Look {
  /** sans, serif, mono, inter, jetbrains-mono, or "custom" for `custom`. */
  font: string;
  custom: string;
  size: number;
  weight: number;
  color: string;
  bg: string;
  /** Background opacity in %; 0 means no background. */
  bgAlpha: number;
  align: "left" | "center" | "right";
  /** Space around the text in px; null keeps the default, which grows with the text. */
  pad: number | null;
  spacing: number;
  upper: boolean;
  shadow: boolean;
  shadowColor: string;
  shadowAlpha: number;
  shadowX: number;
  shadowY: number;
  shadowBlur: number;
  glow: boolean;
  glowColor: string;
  glowSize: number;
  outline: boolean;
  outlineColor: string;
  outlineWidth: number;
  /** Timers only: auto, hh:mm:ss, mm:ss or ss. */
  format: string;
  hide: boolean;
}

export const FONTS = ["sans", "serif", "mono", "inter", "jetbrains-mono", "custom"] as const;
export const FORMATS = ["auto", "hh:mm:ss", "mm:ss", "ss"] as const;

export function defaults(kind: OverlayKind): Look {
  return {
    font: "sans",
    custom: "",
    size: kind === "timer" ? 120 : 48,
    weight: 700,
    color: "#ffffff",
    bg: "#000000",
    bgAlpha: 0,
    align: kind === "timer" ? "center" : "left",
    pad: null,
    spacing: 0,
    upper: false,
    shadow: true,
    shadowColor: "#000000",
    shadowAlpha: 60,
    shadowX: 0,
    shadowY: 2,
    shadowBlur: 6,
    glow: false,
    glowColor: "#ffcc00",
    glowSize: 10,
    outline: false,
    outlineColor: "#000000",
    outlineWidth: 2,
    format: "auto",
    hide: false,
  };
}

const hex = (c: string) => c.replace(/^#/, "").toLowerCase();
const withAlpha = (c: string, pct: number) => hex(c) + (pct >= 100 ? "" : Math.round((pct * 255) / 100).toString(16).padStart(2, "0"));

/** The options that differ from the defaults, in the order the companion documents them. */
export function options(kind: OverlayKind, look: Look): Array<[string, string]> {
  const d = defaults(kind);
  const out: Array<[string, string]> = [];
  const add = (k: string, v: string | number) => out.push([k, String(v)]);
  const font = look.font === "custom" ? look.custom.trim() : look.font;
  if (font && font !== d.font && /^[A-Za-z0-9 -]{1,40}$/.test(font)) add("font", font);
  if (look.size !== d.size) add("size", look.size);
  if (look.weight !== d.weight) add("weight", look.weight);
  if (hex(look.color) !== hex(d.color)) add("color", hex(look.color));
  if (look.bgAlpha > 0) add("bg", withAlpha(look.bg, look.bgAlpha));
  if (look.align !== d.align) add("align", look.align);
  if (look.pad !== null) add("pad", look.pad);
  if (look.spacing) add("spacing", look.spacing);
  if (look.upper) add("upper", 1);
  if (!look.shadow) add("shadow", 0);
  else if (hex(look.shadowColor) !== hex(d.shadowColor) || look.shadowAlpha !== d.shadowAlpha || look.shadowX !== d.shadowX || look.shadowY !== d.shadowY || look.shadowBlur !== d.shadowBlur) {
    add("shadow", withAlpha(look.shadowColor, look.shadowAlpha));
    add("shadow-x", look.shadowX);
    add("shadow-y", look.shadowY);
    add("shadow-blur", look.shadowBlur);
  }
  if (look.glow) {
    add("glow", hex(look.glowColor));
    add("glow-size", look.glowSize);
  }
  if (look.outline) {
    add("outline", hex(look.outlineColor));
    add("outline-width", look.outlineWidth);
  }
  if (kind === "timer" && look.format !== "auto") add("format", look.format);
  if (look.hide) add("hide", 1);
  return out;
}

/** The full link. `what` is the timer name or the text with {variables}. */
export function overlayLink(origin: string, kind: OverlayKind, what: string, look: Look): string {
  const q = new URLSearchParams(kind === "text" ? [["t", what], ...options(kind, look)] : options(kind, look));
  const base = kind === "timer" ? `${origin}/overlay/timer/${what}` : `${origin}/overlay/text`;
  const s = q.toString();
  return s ? `${base}?${s}` : base;
}

/** The last look used, or the defaults. Saved looks from older versions get the new fields filled in. */
export function loadLook(kind: OverlayKind): Look {
  try {
    const saved = JSON.parse(localStorage.getItem(`overlayLook.${kind}`) ?? "null");
    if (saved && typeof saved === "object") return { ...defaults(kind), ...saved };
  } catch {
    // Private mode or a broken entry: start from the defaults.
  }
  return defaults(kind);
}

export function saveLook(kind: OverlayKind, look: Look) {
  try {
    localStorage.setItem(`overlayLook.${kind}`, JSON.stringify(look));
  } catch {
    // Storage full or blocked; the link still works.
  }
}

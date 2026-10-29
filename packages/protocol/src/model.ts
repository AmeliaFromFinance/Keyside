// Layout and key model. Mirrored by crates/companion/src/model.rs.

export type Tone = "default" | "accent" | "danger" | "success" | "warning" | "info" | "muted";
export type FormFactor = "phone-portrait" | "phone-landscape" | "tablet" | "desktop";
export type KeyKind = "button" | "toggle" | "folder" | "back" | "slider" | "widget";
export type Scope = "deck" | "moderator" | "editor";

export interface LayoutFile {
  version: 1;
  /** Bumped by the companion on every saved change. */
  rev: number;
  defaultProfile: string;
  profiles: Profile[];
}

export interface Profile {
  id: string;
  name: string;
  /** Process names that switch to this profile while focused, e.g. "obs64.exe". */
  apps?: string[];
  homePage: string;
  pages: Page[];
}

export interface Page {
  id: string;
  name: string;
  subtitle?: string;
  /** Folder pages are opened from a folder key and are not part of the page dots. */
  folder?: boolean;
  cols: number;
  rows: number;
  keys: Key[];
  /** Optional per-device arrangements. Without one, landscape phones get the page transposed. */
  variants?: Partial<Record<FormFactor, Variant>>;
}

export interface Variant {
  cols: number;
  rows: number;
  /** keyId -> [x, y, w, h]. Keys left out are hidden on that form factor. */
  at: Record<string, [number, number, number, number]>;
}

export interface Key {
  id: string;
  x: number;
  y: number;
  w?: number;
  h?: number;
  kind: KeyKind;
  label: string;
  sub?: string;
  /** Icon name from the built-in set, or "asset:<id>" for an uploaded image. */
  icon?: string;
  color?: KeyColor;
  /** Milliseconds the key must be held before it fires. */
  hold?: number;
  actions: Action[];
  /** Toggle keys: run when switching off. */
  offActions?: Action[];
  /** Folder keys: page to open. */
  target?: string;
  feedback?: Feedback[];
  slider?: { min: number; max: number; step?: number };
  widget?: Widget;
}

export type KeyColor = "default" | "teal" | "violet" | "green" | "amber" | "red" | "blue";

export interface Widget {
  /** Text with {variable} placeholders. */
  text?: string;
  /** Feed to show, e.g. "dev.phonedeck.twitch/chat". */
  feed?: string;
  /** Lines kept from the feed. */
  lines?: number;
  /** Variable holding a 0..1 value, drawn as a bar. */
  progress?: string;
}

export interface Action {
  /** "core.<id>" for built-ins, "<pluginId>/<actionId>" for plugin actions. */
  type: string;
  settings: Record<string, unknown>;
  /** Set on imported actions that can affect the PC. Only the desktop editor can clear it. */
  review?: boolean;
}

export type CompareOp = "eq" | "ne" | "gt" | "lt" | "truthy" | "falsy" | "contains";

export interface Condition {
  var: string;
  op: CompareOp;
  value?: string | number | boolean;
}

export interface Feedback {
  when: Condition;
  style: Partial<Pick<KeyView, "label" | "sub" | "icon" | "badge" | "tone" | "badgeTone" | "meta" | "disabled">>;
}

/** What a client draws for one key. */
export interface KeyView {
  id: string;
  kind: KeyKind;
  x: number;
  y: number;
  w: number;
  h: number;
  label: string;
  sub?: string;
  icon?: string;
  color?: KeyColor;
  badge?: string;
  /** Colours the key: accent and danger change its border and background. */
  tone?: Tone;
  /** Colours only the badge. accent and danger draw a filled pill, the rest coloured text. */
  badgeTone?: Tone;
  /** Small text in the top right, e.g. "3 steps". */
  meta?: string;
  /** Plugin image as a data URL (png, jpeg, webp or svg). */
  image?: string;
  /** 0..1 */
  value?: number;
  /** 0..1 */
  progress?: number;
  /** Epoch ms a running timer counts from. Clients tick it locally. */
  since?: number;
  hold?: number;
  disabled?: boolean;
  /** Widget lines, newest last. */
  lines?: FeedItem[];
}

export interface FeedItem {
  /** Plain text only. Clients must render it as text, never as HTML. */
  text: string;
  author?: string;
  /** #rrggbb */
  color?: string;
  at: number;
}

export type VarValue = string | number | boolean | null;

export interface PageView {
  id: string;
  name: string;
  subtitle?: string;
  folder: boolean;
  cols: number;
  rows: number;
  /** Position among the top-level pages, for the page dots. */
  index: number;
  count: number;
  keys: KeyView[];
}

export interface ConnectionView {
  id: string;
  name: string;
  status: "connected" | "connecting" | "offline" | "error";
  detail?: string;
}

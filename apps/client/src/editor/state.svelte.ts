// Editor state: a draft of the layout that only reaches devices on "Push to phones".

import type { Action, FormFactor, Key, KeyKind, KeyView, LayoutFile, Page, PageView, Profile } from "@keyside/protocol";
import { req, listen, deck } from "../lib/deck.svelte.ts";
import { t } from "../lib/i18n.svelte.ts";

export interface FieldSchema {
  type: "string" | "number" | "integer" | "boolean" | "object";
  title?: string;
  description?: string;
  default?: unknown;
  enum?: string[];
  enumNames?: string[];
  maxLength?: number;
  minimum?: number;
  maximum?: number;
  format?: string;
  "x-options"?: string;
  /** Shows the field only while another field has one of these values. */
  "x-showWhen"?: Record<string, string[]>;
}

export interface Schema {
  type: "object";
  properties: Record<string, FieldSchema>;
  required?: string[];
}

export interface ActionDef {
  id: string;
  name: string;
  group?: string;
  icon?: string;
  description?: string;
  kind?: "button" | "toggle" | "slider";
  settings?: Schema;
  adminOnly?: boolean;
  /** Only makes sense as a step in a list, like "If". */
  stepOnly?: boolean;
}

export interface CatalogPlugin {
  id: string;
  name: string;
  prefix: string;
  icon?: string;
  running: boolean;
  actions: ActionDef[];
}

export interface Catalog {
  core: ActionDef[];
  plugins: CatalogPlugin[];
  assets: Array<{ id: string; kind: "image" | "sound"; name: string; mime: string }>;
  midi: string[];
  variables: Record<string, unknown>;
}

const HISTORY = 50;

export const ed = $state({
  layout: null as LayoutFile | null,
  baseRev: 0,
  dirty: false,
  savedAt: 0,
  profile: "",
  page: "",
  key: null as string | null,
  ff: "phone-portrait" as FormFactor,
  catalog: { core: [], plugins: [], assets: [], midi: [], variables: {} } as Catalog,
  live: null as PageView | null,
  armed: null as string | null,
  moving: null as string | null,
  error: "",
  pushing: false,
  conflict: false,
});

let past: string[] = [];
let future: string[] = [];
/** How many steps undo and redo have, so the toolbar buttons follow along. */
const history = $state({ undo: 0, redo: 0 });
function counted() {
  history.undo = past.length;
  history.redo = future.length;
}
/** The layout as the companion has it, to tell plugin overrides from unsaved edits. */
let saved: LayoutFile | null = null;

function draftKey() {
  return `keyside.draft.${deck.companion?.id ?? "local"}`;
}

export async function load(keepDraft = true) {
  const [layout, catalog] = await Promise.all([req<LayoutFile>("layout.get"), req<Catalog>("catalog.get")]);
  ed.catalog = catalog;
  ed.baseRev = layout.rev;
  let draft: LayoutFile | null = null;
  if (keepDraft) {
    try {
      const saved = JSON.parse(localStorage.getItem(draftKey()) ?? "null");
      if (saved?.base === layout.rev) draft = saved.layout;
    } catch {
      /* storage blocked */
    }
  }
  saved = layout;
  ed.layout = draft ?? layout;
  ed.dirty = !!draft;
  ed.conflict = false;
  past = [];
  future = [];
  counted();
  if (!ed.layout.profiles.some((p) => p.id === ed.profile)) ed.profile = ed.layout.defaultProfile;
  const prof = profile();
  if (!prof?.pages.some((p) => p.id === ed.page)) ed.page = prof?.homePage ?? "";
  ed.key = null;
}

export async function reloadCatalog() {
  ed.catalog = await req<Catalog>("catalog.get");
}

export function profile(): Profile | undefined {
  return ed.layout?.profiles.find((p) => p.id === ed.profile);
}

export function page(): Page | undefined {
  return profile()?.pages.find((p) => p.id === ed.page);
}

export function selectedKey(): Key | undefined {
  return page()?.keys.find((k) => k.id === ed.key);
}

/** Every change goes through here so undo, the dirty flag and the local draft stay right. */
export function change(fn: (l: LayoutFile) => void) {
  if (!ed.layout) return;
  past.push(JSON.stringify(ed.layout));
  if (past.length > HISTORY) past.shift();
  future = [];
  fn(ed.layout);
  ed.dirty = true;
  persist();
}

function persist() {
  counted();
  try {
    localStorage.setItem(draftKey(), JSON.stringify({ base: ed.baseRev, layout: ed.layout }));
    ed.savedAt = Date.now();
  } catch {
    /* storage full or blocked */
  }
}

export function undo() {
  const prev = past.pop();
  if (!prev || !ed.layout) return;
  future.push(JSON.stringify(ed.layout));
  ed.layout = JSON.parse(prev);
  ed.dirty = true;
  persist();
}

export function redo() {
  const next = future.pop();
  if (!next || !ed.layout) return;
  past.push(JSON.stringify(ed.layout));
  ed.layout = JSON.parse(next);
  ed.dirty = true;
  persist();
}

export const canUndo = () => history.undo > 0;
export const canRedo = () => history.redo > 0;

export async function push() {
  if (!ed.layout) return;
  ed.pushing = true;
  ed.error = "";
  try {
    const pushed = $state.snapshot(ed.layout) as LayoutFile;
    const r = await req<{ rev: number }>("layout.save", { layout: pushed, baseRev: ed.baseRev });
    saved = pushed;
    ed.baseRev = r.rev;
    ed.layout.rev = r.rev;
    ed.dirty = false;
    localStorage.removeItem(draftKey());
  } catch (e: any) {
    ed.error = e.message;
    ed.conflict = e.code === "conflict";
  } finally {
    ed.pushing = false;
  }
}

export function discard() {
  try {
    localStorage.removeItem(draftKey());
  } catch {
    /* storage blocked */
  }
  void load(false);
}

export function newId(prefix: string, taken: Iterable<string>): string {
  const used = new Set(taken);
  for (;;) {
    const id = `${prefix}${Math.random().toString(36).slice(2, 7)}`;
    if (!used.has(id)) return id;
  }
}

export function allActions(): ActionDef[] {
  return [...ed.catalog.core, ...ed.catalog.plugins.flatMap((p) => p.actions.map((a) => ({ ...a, id: `${p.id}/${a.id}`, group: p.name, icon: a.icon ?? p.icon })))];
}

export function actionDef(type: string): ActionDef | undefined {
  return allActions().find((a) => a.id === type);
}

export function defaults(def: ActionDef | undefined): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const [k, f] of Object.entries(def?.settings?.properties ?? {})) if (f.default !== undefined) out[k] = f.default;
  return out;
}

export function newAction(type: string): Action {
  return { type, settings: defaults(actionDef(type)) };
}

function occupied(p: Page, skip?: string | string[]): Set<string> {
  const skipped = new Set(Array.isArray(skip) ? skip : skip ? [skip] : []);
  const s = new Set<string>();
  for (const k of p.keys) {
    if (skipped.has(k.id)) continue;
    for (let dy = 0; dy < (k.h ?? 1); dy++) for (let dx = 0; dx < (k.w ?? 1); dx++) s.add(`${k.x + dx},${k.y + dy}`);
  }
  return s;
}

export function fits(p: Page, x: number, y: number, w: number, h: number, skip?: string | string[]): boolean {
  if (x < 0 || y < 0 || x + w > p.cols || y + h > p.rows) return false;
  const occ = occupied(p, skip);
  for (let dy = 0; dy < h; dy++) for (let dx = 0; dx < w; dx++) if (occ.has(`${x + dx},${y + dy}`)) return false;
  return true;
}

export function addKey(x: number, y: number, type: string | null, kind: KeyKind = "button") {
  const p = page();
  if (!p || !fits(p, x, y, 1, 1)) return;
  const def = type ? actionDef(type) : undefined;
  const id = newId("k", p.keys.map((k) => k.id));
  const k: Key = {
    id,
    x,
    y,
    kind: def?.kind === "slider" ? "slider" : def?.kind === "toggle" ? "toggle" : kind,
    label: def?.name ?? "New key",
    icon: def?.icon,
    actions: type ? [newAction(type)] : [],
  };
  if (k.kind === "slider") {
    k.slider = { min: 0, max: 1 };
    if (fits(p, x, y, 1, 2)) k.h = 2;
  }
  change(() => p.keys.push(k));
  ed.key = id;
}

export function moveKey(id: string, x: number, y: number): boolean {
  const p = page();
  const k = p?.keys.find((q) => q.id === id);
  if (!p || !k || !fits(p, x, y, k.w ?? 1, k.h ?? 1, id)) return false;
  change(() => {
    k.x = x;
    k.y = y;
  });
  return true;
}

export function deleteKey(id: string) {
  const p = page();
  if (!p) return;
  const label = p.keys.find((k) => k.id === id)?.label;
  change(() => {
    p.keys = p.keys.filter((k) => k.id !== id);
    for (const v of Object.values(p.variants ?? {})) if (v) delete v.at[id];
  });
  if (ed.key === id) ed.key = null;
  notify(t("Deleted {name}", { name: label ?? t("key") }), { label: t("Undo"), run: undo });
}

/** Short message under the canvas, with an optional button such as Undo. */
export const notice = $state({ text: "", action: null as { label: string; run: () => void } | null, at: 0 });
let noticeTimer: ReturnType<typeof setTimeout> | null = null;

export function notify(text: string, action: { label: string; run: () => void } | null = null) {
  notice.text = text;
  notice.action = action;
  notice.at = Date.now();
  if (noticeTimer) clearTimeout(noticeTimer);
  noticeTimer = setTimeout(() => (notice.text = ""), action ? 6000 : 3000);
}

/** Swaps two keys of the same size. Keys of different sizes swap when both still fit. */
export function swapKeys(a: string, b: string): boolean {
  const p = page();
  const ka = p?.keys.find((k) => k.id === a);
  const kb = p?.keys.find((k) => k.id === b);
  if (!p || !ka || !kb || a === b) return false;
  const [ax, ay, bx, by] = [ka.x, ka.y, kb.x, kb.y];
  // Check each in the other's spot, with both lifted off the grid.
  const aFits = fits(p, bx, by, ka.w ?? 1, ka.h ?? 1, [a, b]);
  const bFits = fits(p, ax, ay, kb.w ?? 1, kb.h ?? 1, [a, b]);
  const overlap = bx < ax + (kb.w ?? 1) && ax < bx + (ka.w ?? 1) && by < ay + (kb.h ?? 1) && ay < by + (ka.h ?? 1);
  if (!aFits || !bFits || (overlap && ((ka.w ?? 1) !== (kb.w ?? 1) || (ka.h ?? 1) !== (kb.h ?? 1)))) return false;
  change(() => {
    [ka.x, ka.y, kb.x, kb.y] = [bx, by, ax, ay];
  });
  return true;
}

/** First free spot on the page for a key of this size, reading left to right. */
function freeSpot(p: Page, w: number, h: number, from = 0): [number, number] | null {
  for (let i = from; i < p.cols * p.rows; i++) {
    const [x, y] = [i % p.cols, Math.floor(i / p.cols)];
    if (fits(p, x, y, w, h)) return [x, y];
  }
  return from ? freeSpot(p, w, h, 0) : null;
}

/** A copy with fresh ids, so it can live next to the original. */
function cloneKey(k: Key, p: Page, x: number, y: number): Key {
  const copy = JSON.parse(JSON.stringify($state.snapshot(k))) as Key;
  copy.id = newId("k", p.keys.map((q) => q.id));
  copy.x = x;
  copy.y = y;
  // A folder copy shares the original's sub-page rather than a duplicate of it.
  return copy;
}

export function duplicateKey(id: string) {
  const p = page();
  const k = p?.keys.find((q) => q.id === id);
  if (!p || !k) return;
  const at = freeSpot(p, k.w ?? 1, k.h ?? 1, k.y * p.cols + k.x + 1);
  if (!at) return notify(t("No free space on this page"));
  const copy = cloneKey(k, p, ...at);
  change(() => p.keys.push(copy));
  ed.key = copy.id;
}

/** Copied or cut key, to paste on any page. Kept for this session only. */
export const clipboard = $state({ key: null as Key | null });

export function copyKey(id: string, cut = false) {
  const k = page()?.keys.find((q) => q.id === id);
  if (!k) return;
  clipboard.key = JSON.parse(JSON.stringify($state.snapshot(k)));
  if (cut) {
    deleteKey(id);
    notify(t("Cut {name}", { name: k.label }), { label: t("Undo"), run: undo });
  } else notify(t("Copied {name}", { name: k.label }));
}

/** Pastes at a cell, or in the first free spot that fits. */
export function pasteKey(x?: number, y?: number) {
  const p = page();
  const k = clipboard.key;
  if (!p || !k) return;
  const at: [number, number] | null = x !== undefined && y !== undefined && fits(p, x, y, k.w ?? 1, k.h ?? 1) ? [x, y] : freeSpot(p, k.w ?? 1, k.h ?? 1);
  if (!at) return notify(t("No free space on this page"));
  const copy = cloneKey(k, p, ...at);
  change(() => p.keys.push(copy));
  ed.key = copy.id;
}

/** Moves a key one cell with the arrow keys. */
export function nudgeKey(id: string, dx: number, dy: number) {
  const p = page();
  const k = p?.keys.find((q) => q.id === id);
  if (k) moveKey(id, k.x + dx, k.y + dy);
}

export function addPage(folder = false): string | undefined {
  const pr = profile();
  if (!pr) return;
  const id = newId("p", pr.pages.map((p) => p.id));
  const base = page();
  change(() =>
    pr.pages.push({
      id,
      name: folder ? "Folder" : `Page ${pr.pages.filter((p) => !p.folder).length + 1}`,
      cols: base?.cols ?? 3,
      rows: base?.rows ?? 5,
      folder: folder || undefined,
      keys: folder ? [{ id: "back", x: 0, y: (base?.rows ?? 5) - 1, kind: "back", label: "Back", actions: [] }] : [],
    }),
  );
  if (!folder) ed.page = id;
  return id;
}

export function deletePage(id: string) {
  const pr = profile();
  if (!pr || pr.pages.filter((p) => !p.folder).length <= 1) return;
  change(() => {
    pr.pages = pr.pages.filter((p) => p.id !== id);
    for (const p of pr.pages) p.keys = p.keys.filter((k) => !(k.kind === "folder" && k.target === id));
    if (pr.homePage === id) pr.homePage = pr.pages.find((p) => !p.folder)!.id;
  });
  if (ed.page === id) ed.page = pr.homePage;
}

/** Draws a key from the draft, then adds live state (badges, timers, values) from the companion. */
export function previewView(k: Key, live: KeyView | undefined, pos?: [number, number, number, number]): KeyView {
  const [x, y, w, h] = pos ?? [k.x, k.y, k.w ?? 1, k.h ?? 1];
  const v: KeyView = { id: k.id, kind: k.kind, x, y, w, h, label: k.label, sub: k.sub, icon: k.icon, color: k.color, hold: k.hold };
  if (k.kind === "folder") {
    v.icon ??= "folder";
    const target = profile()?.pages.find((p) => p.id === k.target);
    v.sub ??= target ? `Folder · ${target.keys.length}` : undefined;
  }
  if (k.kind === "back") v.icon ??= "arrow-left";
  if (k.actions.length > 1) v.meta = `${k.actions.length} steps`;
  if ([...k.actions, ...(k.offActions ?? [])].some((a) => a.review)) {
    v.badge = "REVIEW";
    v.tone = "warning";
    v.disabled = true;
    return v;
  }
  if (live && live.kind === k.kind) {
    for (const f of ["badge", "tone", "badgeTone", "since", "value", "progress", "image", "disabled", "lines"] as const) {
      if (live[f] !== undefined) (v as any)[f] = live[f];
    }
    if (live.meta && !v.meta) v.meta = live.meta;
    // Live text only counts when a plugin changed it, not when the draft differs from what was pushed.
    const was = saved?.profiles.find((p) => p.id === ed.profile)?.pages.find((p) => p.id === ed.page)?.keys.find((x) => x.id === k.id) ?? k;
    if (live.sub && live.sub !== was.sub && k.kind !== "folder") v.sub = live.sub;
    if (live.label !== was.label && !was.label.includes("{")) v.label = live.label;
  }
  return v;
}

export function placements(p: Page, ff: FormFactor): { cols: number; rows: number; at: Map<string, [number, number, number, number]> } {
  const variant = p.variants?.[ff];
  if (variant) return { cols: variant.cols, rows: variant.rows, at: new Map(Object.entries(variant.at)) };
  const transpose = ff === "phone-landscape" && p.rows > p.cols;
  const at = new Map<string, [number, number, number, number]>();
  for (const k of p.keys) at.set(k.id, transpose ? [k.y, k.x, k.h ?? 1, k.w ?? 1] : [k.x, k.y, k.w ?? 1, k.h ?? 1]);
  return transpose ? { cols: p.rows, rows: p.cols, at } : { cols: p.cols, rows: p.rows, at };
}

let poll: ReturnType<typeof setInterval> | null = null;

/** Keeps live key state for the page being edited. */
export function startLive() {
  const fetchLive = async () => {
    if (!ed.page) return;
    try {
      ed.live = await req<PageView | null>("deck.preview", { profile: ed.profile, page: ed.page, formFactor: "phone-portrait" });
    } catch {
      /* not connected right now */
    }
  };
  void fetchLive();
  poll = setInterval(fetchLive, 1000);
  const off = listen((m) => {
    if (m.t === "event" && m.name === "admin.changed") void reloadCatalog().catch(() => {});
  });
  return () => {
    if (poll) clearInterval(poll);
    off();
  };
}

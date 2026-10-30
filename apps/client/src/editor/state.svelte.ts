// Editor state: a draft of the layout that only reaches devices on "Push to phones".

import type { Action, Condition, FormFactor, Key, KeyKind, KeyView, LayoutFile, Page, PageView, Profile } from "@keyside/protocol";
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
  /** Keys selected together. Holds ed.key too while more than one is selected. */
  picked: [] as string[],
  /** Touch screens: taps add to the selection until Done. */
  picking: false,
  /** Plugin to open in the Plugins panel. */
  setup: null as string | null,
  /** Opens the key styles manager. */
  styling: false,
  /** Current variable values, while the layout has styles or keys that follow a variable. */
  vars: {} as Record<string, unknown>,
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
    const sent = JSON.stringify(pushed);
    const r = await req<{ rev: number }>("layout.save", { layout: pushed, baseRev: ed.baseRev });
    saved = pushed;
    ed.baseRev = r.rev;
    // Edits made while the push was on its way still need pushing.
    const later = JSON.stringify({ ...($state.snapshot(ed.layout) as LayoutFile), rev: pushed.rev }) !== sent;
    ed.layout.rev = r.rev;
    if (later) persist();
    else {
      ed.dirty = false;
      localStorage.removeItem(draftKey());
    }
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

/** Selected keys on the current page. `ed.key` is the one shown in the inspector. */
export function selection(): string[] {
  const p = page();
  if (!p || !ed.key) return [];
  const here = new Set(p.keys.map((k) => k.id));
  if (ed.picked.length > 1 && ed.picked.includes(ed.key)) return ed.picked.filter((id) => here.has(id));
  return here.has(ed.key) ? [ed.key] : [];
}

/** Selects a key. With `add`, it joins or leaves the selection instead. */
export function pick(id: string, add = false) {
  if (!add) {
    ed.key = id;
    ed.picked = [];
    return;
  }
  const cur = selection();
  if (cur.includes(id)) {
    ed.picked = cur.filter((x) => x !== id);
    ed.key = ed.picked.at(-1) ?? null;
  } else {
    ed.picked = [...cur, id];
    ed.key = id;
  }
}

export function selectAll() {
  const p = page();
  if (!p?.keys.length) return;
  ed.picked = p.keys.map((k) => k.id);
  ed.key = ed.picked.at(-1)!;
}

export function clearSelection() {
  ed.key = null;
  ed.picked = [];
  ed.picking = false;
}

const keyName = (n: number, label?: string) => (n === 1 && label ? label : t("{n} keys", { n }));

export function deleteKeys(ids: string[]) {
  const p = page();
  if (!p || !ids.length) return;
  const label = p.keys.find((k) => k.id === ids[0])?.label;
  change(() => {
    p.keys = p.keys.filter((k) => !ids.includes(k.id));
    for (const v of Object.values(p.variants ?? {})) if (v) for (const id of ids) delete v.at[id];
  });
  clearSelection();
  notify(t("Deleted {name}", { name: keyName(ids.length, label) }), { label: t("Undo"), run: undo });
}

export const deleteKey = (id: string) => deleteKeys([id]);

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

/**
 * The arrangement shown for the current screen, and where to write changes. A screen with
 * its own arrangement keeps positions in its variant. Sideways phones without one show the
 * main layout turned, so their positions are swapped back before saving.
 */
function arrangement() {
  const p = page();
  if (!p) return null;
  const v = p.variants?.[ed.ff];
  const turned = !v && ed.ff === "phone-landscape" && p.rows > p.cols;
  const cols = v ? v.cols : p.cols;
  const rows = v ? v.rows : p.rows;
  const boxes = new Map<string, [number, number, number, number]>(
    v ? Object.entries(v.at) : p.keys.map((k) => [k.id, [k.x, k.y, k.w ?? 1, k.h ?? 1]] as [string, [number, number, number, number]]),
  );
  return { p, v, turned, cols, rows, boxes };
}

/** Where a key is on screen right now: [x, y, w, h]. */
export function onScreen(id: string): [number, number, number, number] | undefined {
  const p = page();
  return p ? placements(p, ed.ff).at.get(id) : undefined;
}

/**
 * Moves keys to on-screen positions, all or none. Returns false when any of them would
 * leave the grid or overlap another key. With `dry`, only checks.
 */
export function relocate(moves: Array<[string, number, number]>, dry = false): boolean {
  const a = arrangement();
  if (!a || !moves.length) return false;
  const moving = new Set(moves.map(([id]) => id));
  const taken = new Set<string>();
  const mark = (x: number, y: number, w: number, h: number) => {
    for (let dy = 0; dy < h; dy++)
      for (let dx = 0; dx < w; dx++) {
        const c = `${x + dx},${y + dy}`;
        if (taken.has(c)) return false;
        taken.add(c);
      }
    return true;
  };
  for (const [id, [x, y, w, h]] of a.boxes) if (!moving.has(id)) mark(x, y, w, h);
  const targets: Array<[string, number, number]> = [];
  for (const [id, sx, sy] of moves) {
    const box = a.boxes.get(id);
    if (!box) return false;
    const [x, y] = a.turned ? [sy, sx] : [sx, sy];
    if (x < 0 || y < 0 || x + box[2] > a.cols || y + box[3] > a.rows || !mark(x, y, box[2], box[3])) return false;
    targets.push([id, x, y]);
  }
  if (dry) return true;
  change(() => {
    for (const [id, x, y] of targets) {
      if (a.v) {
        const [, , w, h] = a.v.at[id];
        a.v.at[id] = [x, y, w, h];
      } else {
        const k = a.p.keys.find((q) => q.id === id)!;
        k.x = x;
        k.y = y;
      }
    }
  });
  return true;
}

/** Shifts keys by whole cells on screen. */
export function shiftKeys(ids: string[], dx: number, dy: number, dry = false): boolean {
  const moves: Array<[string, number, number]> = [];
  for (const id of ids) {
    const at = onScreen(id);
    if (!at) return false;
    moves.push([id, at[0] + dx, at[1] + dy]);
  }
  return relocate(moves, dry);
}

/** Two keys trade places. Keys of different sizes trade when both still fit. */
export function swapKeys(a: string, b: string, dry = false): boolean {
  const [pa, pb] = [onScreen(a), onScreen(b)];
  return !!pa && !!pb && a !== b && relocate([[a, pb[0], pb[1]], [b, pa[0], pa[1]]], dry);
}

export function moveKey(id: string, x: number, y: number): boolean {
  return relocate([[id, x, y]]);
}

export const nudgeKey = (id: string, dx: number, dy: number) => shiftKeys(selection().includes(id) ? selection() : [id], dx, dy);

/** First free spot for a key of this size, reading left to right from `from`. */
function freeSpot(p: Page, w: number, h: number, from = 0): [number, number] | null {
  for (let i = from; i < p.cols * p.rows; i++) {
    const [x, y] = [i % p.cols, Math.floor(i / p.cols)];
    if (fits(p, x, y, w, h)) return [x, y];
  }
  return from ? freeSpot(p, w, h, 0) : null;
}

/** Plain copies of keys, top-left first. */
function snapshot(p: Page, ids: string[]): Key[] {
  return p.keys
    .filter((k) => ids.includes(k.id))
    .map((k) => JSON.parse(JSON.stringify($state.snapshot(k))) as Key)
    .sort((a, b) => a.y - b.y || a.x - b.x);
}

/**
 * Places copies of keys on a page. With an anchor, the group keeps its shape there when it
 * fits. Otherwise each key goes in the first free spot. Returns the new keys, or null when
 * there isn't room for all of them. Doesn't change anything itself.
 */
function placeCopies(p: Page, keys: Key[], anchor?: [number, number], keepIds = false): Key[] | null {
  const scratch = { ...p, keys: [...p.keys] } as Page;
  const out: Key[] = [];
  const add = (k: Key, x: number, y: number) => {
    const id = keepIds && !scratch.keys.some((q) => q.id === k.id) ? k.id : newId("k", scratch.keys.map((q) => q.id));
    const copy = { ...k, id, x, y };
    scratch.keys.push(copy);
    out.push(copy);
  };
  if (anchor && keys.length) {
    const [ox, oy] = [Math.min(...keys.map((k) => k.x)), Math.min(...keys.map((k) => k.y))];
    const fit = keys.every((k, i) => {
      const [x, y] = [anchor[0] + k.x - ox, anchor[1] + k.y - oy];
      // Check against the keys placed so far, too.
      if (!fits(scratch, x, y, k.w ?? 1, k.h ?? 1)) return false;
      scratch.keys.push({ ...k, id: ` ${i}`, x, y });
      return true;
    });
    scratch.keys = [...p.keys];
    if (fit) {
      for (const k of keys) add(k, anchor[0] + k.x - ox, anchor[1] + k.y - oy);
      return out;
    }
  }
  for (const k of keys) {
    const at = freeSpot(scratch, k.w ?? 1, k.h ?? 1);
    if (!at) return null;
    add(k, ...at);
  }
  return out;
}

export function duplicateKeys(ids: string[]) {
  const p = page();
  if (!p || !ids.length) return;
  const made = placeCopies(p, snapshot(p, ids));
  if (!made) return notify(t("No free space on this page"));
  change(() => p.keys.push(...made));
  ed.picked = made.map((k) => k.id);
  ed.key = made.at(-1)!.id;
}

export const duplicateKey = (id: string) => duplicateKeys([id]);

/** Copied or cut keys, to paste on any page. Kept for this session only. */
export const clipboard = $state({ keys: [] as Key[] });

export function copyKeys(ids: string[], cut = false) {
  const p = page();
  if (!p || !ids.length) return;
  const keys = snapshot(p, ids);
  clipboard.keys = keys;
  const name = keyName(keys.length, keys[0]?.label);
  if (cut) {
    deleteKeys(ids);
    notify(t("Cut {name}", { name }), { label: t("Undo"), run: undo });
  } else notify(t("Copied {name}", { name }));
}

export const copyKey = (id: string, cut = false) => copyKeys([id], cut);

/** Pastes at a cell, keeping the copied keys' arrangement when it fits there. */
export function pasteKey(x?: number, y?: number) {
  const p = page();
  if (!p || !clipboard.keys.length) return;
  const made = placeCopies(p, clipboard.keys, x !== undefined && y !== undefined ? [x, y] : undefined);
  if (!made) return notify(t("No free space on this page"));
  change(() => p.keys.push(...made));
  ed.picked = made.map((k) => k.id);
  ed.key = made.at(-1)!.id;
}

/** Moves keys to another page of the same layout, into its free spots. */
export function moveToPage(ids: string[], target: string, dry = false): boolean {
  const p = page();
  const dest = profile()?.pages.find((pg) => pg.id === target);
  if (!p || !dest || dest === p || !ids.length) return false;
  const keys = snapshot(p, ids);
  if (keys.some((k) => k.kind === "folder" && k.target === dest.id)) {
    if (!dry) notify(t("A folder can't go inside itself"));
    return false;
  }
  const made = placeCopies(dest, keys, undefined, true);
  if (!made) {
    if (!dry) notify(t("{page} has no room for {name}", { page: dest.name, name: keyName(keys.length, keys[0]?.label) }));
    return false;
  }
  if (dry) return true;
  change(() => {
    p.keys = p.keys.filter((k) => !ids.includes(k.id));
    for (const v of Object.values(p.variants ?? {})) if (v) for (const id of ids) delete v.at[id];
    dest.keys.push(...made);
  });
  clearSelection();
  notify(t("Moved {name} to {page}", { name: keyName(keys.length, keys[0]?.label), page: dest.name }), { label: t("Undo"), run: undo });
  return true;
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
  const target = pr?.pages.find((p) => p.id === id);
  // A layout keeps at least one normal page. Folders can always go.
  if (!pr || !target || (!target.folder && pr.pages.filter((p) => !p.folder).length <= 1)) return;
  change(() => {
    pr.pages = pr.pages.filter((p) => p.id !== id);
    for (const p of pr.pages) p.keys = p.keys.filter((k) => !(k.kind === "folder" && k.target === id));
    if (pr.homePage === id) pr.homePage = pr.pages.find((p) => !p.folder)!.id;
  });
  if (ed.page === id) ed.page = pr.homePage;
}

/** Every step on a key, in all of its lists. */
export function keySteps(k: Key): Action[] {
  return [...k.actions, ...(k.offActions ?? []), ...(k.longActions ?? []), ...(k.doubleActions ?? []), ...(k.states ?? []).flatMap((s) => s.actions)];
}

/** Plugin steps that act for as long as the key is held, like "Talk while held". */
export function actsWhileHeld(a: Action): boolean {
  return a.type.includes("/") && (a.settings.mode === "hold" || a.settings.op === "hold");
}

/** Why a key can't have long press or double tap steps, or "" when it can. */
export function plainPressReason(k: Key): string {
  if (k.hold) return t("Hold to confirm is on.");
  if (k.repeat) return t("Repeat while held is on.");
  const press = [...k.actions, ...(k.offActions ?? []), ...(k.states ?? []).flatMap((s) => s.actions)];
  if (press.some(actsWhileHeld)) return t("A step acts while the key is held.");
  return "";
}

function num(v: unknown): number | undefined {
  if (typeof v === "number") return v;
  if (typeof v === "boolean") return v ? 1 : 0;
  if (typeof v === "string" && v.trim() !== "" && !Number.isNaN(Number(v))) return Number(v);
  return undefined;
}

function shown(v: unknown): string {
  if (v === null || v === undefined) return "";
  if (v === true) return "on";
  if (v === false) return "off";
  return String(v);
}

function truthy(v: unknown): boolean {
  if (typeof v === "string") return v !== "" && v !== "0" && v.toLowerCase() !== "false";
  if (typeof v === "number") return v !== 0;
  return !!v;
}

/** Same as the companion: loose equality, numbers compared as numbers. */
export function looseEq(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  const x = num(a);
  const y = num(b);
  if (x !== undefined && y !== undefined && !(typeof a === "string" && typeof b === "string")) return x === y;
  return shown(a) === shown(b);
}

export function matches(c: Condition, vars: Record<string, unknown>): boolean {
  const cur = vars[c.var];
  switch (c.op) {
    case "truthy":
      return truthy(cur);
    case "falsy":
      return !truthy(cur);
    case "eq":
      return looseEq(cur, c.value);
    case "ne":
      return !looseEq(cur, c.value);
    case "gt":
    case "lt": {
      const a = num(cur);
      const b = num(c.value);
      return a !== undefined && b !== undefined && (c.op === "gt" ? a > b : a < b);
    }
    case "contains":
      return shown(cur).toLowerCase().includes(shown(c.value).toLowerCase());
  }
}

/** Deletes a style and takes it off every key that used it. */
export function deleteStyle(id: string) {
  change((l) => {
    l.styles = (l.styles ?? []).filter((s) => s.id !== id);
    if (!l.styles.length) delete l.styles;
    for (const p of l.profiles)
      for (const pg of p.pages)
        for (const k of pg.keys) {
          if (!k.styles) continue;
          k.styles = k.styles.filter((s) => s !== id);
          if (!k.styles.length) delete k.styles;
        }
  });
}

/** Draws a key from the draft, then adds live state (badges, timers, values) from the companion. */
export function previewView(k: Key, live: KeyView | undefined, pos?: [number, number, number, number]): KeyView {
  const [x, y, w, h] = pos ?? [k.x, k.y, k.w ?? 1, k.h ?? 1];
  const v: KeyView = { id: k.id, kind: k.kind, x, y, w, h, label: k.label, sub: k.sub, icon: k.icon, color: k.color, hold: k.hold };
  const states = k.kind === "multi" ? (k.states ?? []) : [];
  if (states.length) {
    const followed = k.follow ? states.findIndex((s) => s.value !== undefined && s.value !== "" && looseEq(ed.vars[k.follow!], s.value)) : -1;
    const i = Math.min(states.length - 1, live?.state ?? (followed >= 0 ? followed : 0));
    const st = states[i];
    Object.assign(v, { state: i, label: st.label, sub: st.sub, icon: st.icon ?? k.icon, color: st.color ?? k.color, meta: `${i + 1}/${states.length}` });
  }
  if (k.kind === "folder") {
    v.icon ??= "folder";
    const target = profile()?.pages.find((p) => p.id === k.target);
    v.sub ??= target ? `Folder · ${target.keys.length}` : undefined;
  }
  if (k.kind === "back") v.icon ??= "arrow-left";
  if (k.actions.length > 1) v.meta = `${k.actions.length} steps`;
  for (const id of k.styles ?? []) {
    const st = ed.layout?.styles?.find((s) => s.id === id);
    if (!st?.when.var || !matches(st.when, ed.vars)) continue;
    const look = st.style;
    if (look.label) v.label = look.label;
    for (const f of ["sub", "icon", "color", "badge", "tone", "badgeTone", "meta"] as const) if (look[f]) (v as any)[f] = look[f];
  }
  if (keySteps(k).some((a) => a.review)) {
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
    if (live.label !== was.label && !was.label.includes("{") && !states.length && !k.styles?.length) v.label = live.label;
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
  let tick = 0;
  const fetchLive = async () => {
    if (!ed.page) return;
    try {
      ed.live = await req<PageView | null>("deck.preview", { profile: ed.profile, page: ed.page, formFactor: "phone-portrait" });
      // Variables only matter for drawing styles and followed states, so they're fetched less often.
      const wanted = !!ed.layout?.styles?.length || !!page()?.keys.some((k) => k.follow);
      if (wanted && tick++ % 3 === 0) ed.vars = await req<Record<string, unknown>>("variables.list");
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

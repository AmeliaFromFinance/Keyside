// What changed between the layout on the companion and the draft, for the review before pushing.

import type { Key, LayoutFile, Page, Profile } from "@keyside/protocol";

/** Parts of a key that can change, in the order they are listed. */
export type KeyPart = "label" | "sub" | "icon" | "color" | "type" | "size" | "steps" | "feedback" | "settings";

export type Item =
  | { kind: "added" | "removed"; label: string }
  | { kind: "moved"; label: string; from?: string }
  | { kind: "edited"; label: string; parts: KeyPart[] };

export type PagePart = "name" | "grid" | "arrangement" | "home";

export interface PageChange {
  id: string;
  name: string;
  status: "added" | "removed" | "changed";
  parts: PagePart[];
  items: Item[];
  /** Added and removed pages: how many keys they hold. */
  keys?: number;
}

export interface LayoutChange {
  id: string;
  name: string;
  status: "added" | "removed" | "changed";
  renamed?: string;
  pages: PageChange[];
}

const same = (a: unknown, b: unknown) => JSON.stringify(a ?? null) === JSON.stringify(b ?? null);

function keyParts(a: Key, b: Key): KeyPart[] {
  const out: KeyPart[] = [];
  if (a.label !== b.label) out.push("label");
  if ((a.sub ?? "") !== (b.sub ?? "")) out.push("sub");
  if (a.icon !== b.icon) out.push("icon");
  if (a.color !== b.color) out.push("color");
  if (a.kind !== b.kind || a.target !== b.target) out.push("type");
  if ((a.w ?? 1) !== (b.w ?? 1) || (a.h ?? 1) !== (b.h ?? 1)) out.push("size");
  if (!same(a.actions, b.actions) || !same(a.offActions ?? [], b.offActions ?? [])) out.push("steps");
  if (!same(a.feedback ?? [], b.feedback ?? [])) out.push("feedback");
  if (!same([a.hold, a.repeat, a.slider, a.widget], [b.hold, b.repeat, b.slider, b.widget])) out.push("settings");
  return out;
}

/** Changes to the keys of one page. `elsewhere` holds keys that left another page of the layout. */
function pageItems(old: Page, cur: Page, elsewhere: Map<string, string>, arrived: Set<string>): Item[] {
  const before = new Map(old.keys.map((k) => [k.id, k]));
  const after = new Set(cur.keys.map((k) => k.id));
  const items: Item[] = [];
  for (const k of cur.keys) {
    const was = before.get(k.id);
    if (!was) {
      const from = elsewhere.get(k.id);
      items.push(from !== undefined ? { kind: "moved", label: k.label, from } : { kind: "added", label: k.label });
      continue;
    }
    if (was.x !== k.x || was.y !== k.y) items.push({ kind: "moved", label: k.label });
    const parts = keyParts(was, k);
    if (parts.length) items.push({ kind: "edited", label: was.label === k.label ? k.label : was.label, parts });
  }
  // Keys moved to another page are listed there.
  for (const k of old.keys) if (!after.has(k.id) && !arrived.has(k.id)) items.push({ kind: "removed", label: k.label });
  return items;
}

function pageParts(old: Page, cur: Page, oldHome: boolean, curHome: boolean): PagePart[] {
  const out: PagePart[] = [];
  if (old.name !== cur.name) out.push("name");
  if (old.cols !== cur.cols || old.rows !== cur.rows) out.push("grid");
  if (!same(old.variants ?? {}, cur.variants ?? {})) out.push("arrangement");
  if (oldHome !== curHome) out.push("home");
  return out;
}

function profilePages(old: Profile, cur: Profile): PageChange[] {
  const oldPages = new Map(old.pages.map((p) => [p.id, p]));
  const curPages = new Set(cur.pages.map((p) => p.id));
  // Key ids that left a page that still exists, and the pages ids turned up on.
  const left = new Map<string, string>();
  const arrived = new Set<string>();
  for (const p of cur.pages) {
    const was = oldPages.get(p.id);
    if (!was) continue;
    const now = new Set(p.keys.map((k) => k.id));
    for (const k of was.keys) if (!now.has(k.id)) left.set(k.id, p.name);
  }
  for (const p of cur.pages) {
    const before = new Set(oldPages.get(p.id)?.keys.map((k) => k.id));
    for (const k of p.keys) if (!before.has(k.id) && left.has(k.id)) arrived.add(k.id);
  }
  const out: PageChange[] = [];
  for (const p of cur.pages) {
    const was = oldPages.get(p.id);
    if (!was) {
      const items: Item[] = p.keys.filter((k) => arrived.has(k.id)).map((k) => ({ kind: "moved", label: k.label, from: left.get(k.id) }));
      out.push({ id: p.id, name: p.name, status: "added", parts: [], items, keys: p.keys.length });
      continue;
    }
    const parts = pageParts(was, p, old.homePage === p.id, cur.homePage === p.id);
    const items = pageItems(was, p, left, arrived);
    if (parts.length || items.length) out.push({ id: p.id, name: p.name, status: "changed", parts, items });
  }
  for (const p of old.pages) if (!curPages.has(p.id)) out.push({ id: p.id, name: p.name, status: "removed", parts: [], items: [], keys: p.keys.length });
  return out;
}

/** Changes from `old` (what devices have) to `cur` (the draft), per layout and page. */
export function diffLayouts(old: LayoutFile | null, cur: LayoutFile | null): LayoutChange[] {
  if (!cur) return [];
  const before = new Map((old?.profiles ?? []).map((p) => [p.id, p]));
  const after = new Set(cur.profiles.map((p) => p.id));
  const out: LayoutChange[] = [];
  for (const p of cur.profiles) {
    const was = before.get(p.id);
    if (!was) {
      out.push({ id: p.id, name: p.name, status: "added", pages: [] });
      continue;
    }
    const pages = profilePages(was, p);
    const renamed = was.name !== p.name ? was.name : undefined;
    if (pages.length || renamed) out.push({ id: p.id, name: p.name, status: "changed", renamed, pages });
  }
  for (const p of old?.profiles ?? []) if (!after.has(p.id)) out.push({ id: p.id, name: p.name, status: "removed", pages: [] });
  return out;
}

/** How many separate changes there are, for the heading. */
export function countChanges(list: LayoutChange[]): number {
  return list.reduce((n, l) => n + (l.status !== "changed" ? 1 : (l.renamed ? 1 : 0) + l.pages.reduce((m, p) => m + (p.status !== "changed" ? 1 : p.parts.length + p.items.length), 0)), 0);
}

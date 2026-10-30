// Short summary of what restoring an older layout would change in the draft.

import type { LayoutFile } from "@keyside/protocol";

export interface LayoutDiff {
  /** Layouts (profiles) by name. */
  layoutsAdded: string[];
  layoutsRemoved: string[];
  /** Pages as "Layout › Page". */
  pagesAdded: string[];
  pagesRemoved: string[];
  /** Layouts or pages whose name, grid or other settings differ. */
  renamed: number;
  keysAdded: number;
  keysRemoved: number;
  keysChanged: number;
}

/**
 * JSON with sorted keys, leaving out empty values, so copies that went through the companion
 * and copies edited here compare equal.
 */
export function canonical(v: unknown): string {
  return JSON.stringify(v, (_, x) => {
    if (!x || typeof x !== "object" || Array.isArray(x)) return x;
    const out: Record<string, unknown> = {};
    for (const k of Object.keys(x).sort()) {
      const y = (x as Record<string, unknown>)[k];
      if (y === undefined || y === null || y === false || (Array.isArray(y) && !y.length) || (typeof y === "object" && !Array.isArray(y) && !Object.keys(y as object).length)) continue;
      out[k] = y;
    }
    return out;
  });
}

/** What changes when `to` replaces `from`. Things are matched by id. */
export function diffLayouts(from: LayoutFile, to: LayoutFile): LayoutDiff {
  const d: LayoutDiff = { layoutsAdded: [], layoutsRemoved: [], pagesAdded: [], pagesRemoved: [], renamed: 0, keysAdded: 0, keysRemoved: 0, keysChanged: 0 };
  const pageName = (p: string, pg: string) => `${p} › ${pg}`;
  for (const p of to.profiles) {
    const old = from.profiles.find((x) => x.id === p.id);
    if (!old) {
      d.layoutsAdded.push(p.name);
      continue;
    }
    const bare = (x: object) => canonical({ ...x, pages: undefined, keys: undefined });
    if (bare(old) !== bare(p)) d.renamed++;
    for (const pg of p.pages) {
      const was = old.pages.find((x) => x.id === pg.id);
      if (!was) {
        d.pagesAdded.push(pageName(p.name, pg.name));
        continue;
      }
      if (bare(was) !== bare(pg)) d.renamed++;
      for (const k of pg.keys) {
        const prev = was.keys.find((x) => x.id === k.id);
        if (!prev) d.keysAdded++;
        else if (canonical(prev) !== canonical(k)) d.keysChanged++;
      }
      d.keysRemoved += was.keys.filter((k) => !pg.keys.some((x) => x.id === k.id)).length;
    }
    for (const pg of old.pages) if (!p.pages.some((x) => x.id === pg.id)) d.pagesRemoved.push(pageName(old.name, pg.name));
  }
  for (const p of from.profiles) if (!to.profiles.some((x) => x.id === p.id)) d.layoutsRemoved.push(p.name);
  if (from.defaultProfile !== to.defaultProfile) d.renamed++;
  return d;
}

export function isSame(d: LayoutDiff): boolean {
  return !d.layoutsAdded.length && !d.layoutsRemoved.length && !d.pagesAdded.length && !d.pagesRemoved.length && !d.renamed && !d.keysAdded && !d.keysRemoved && !d.keysChanged;
}

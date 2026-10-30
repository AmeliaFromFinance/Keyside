// Finds keys in every layout and page by their text, steps and plugins.

import type { Key, LayoutFile } from "@keyside/protocol";

export interface KeyHit {
  profile: string;
  profileName: string;
  page: string;
  pageName: string;
  folder: boolean;
  key: string;
  label: string;
  sub?: string;
  icon?: string;
  /** Names of the steps, for showing under the label. */
  steps: string[];
}

/** Names for a step type: the action name and its plugin, in English and translated. */
export type Describe = (type: string) => { name: string; group?: string; words?: string[] };

const split = (s: string) => s.toLowerCase().split(/[^\p{L}\p{N}]+/u).filter(Boolean);

/**
 * Every search word has to start a word somewhere on the key. Words in the label count most,
 * then the second line, then step and plugin names. Returns the best matches first.
 */
export function searchKeys(layout: LayoutFile | null, query: string, describe: Describe, limit = 60): KeyHit[] {
  const words = split(query);
  if (!layout || !words.length) return [];
  const out: Array<KeyHit & { score: number; order: number }> = [];
  let order = 0;
  for (const pr of layout.profiles)
    for (const pg of pr.pages)
      for (const k of pg.keys) {
        order++;
        const steps = [...k.actions, ...(k.offActions ?? [])];
        const named = steps.map((a) => describe(a.type));
        const fields: Array<[string[], number]> = [
          [split(k.label), 4],
          [split(k.sub ?? ""), 2],
          [named.flatMap((d) => [...split(d.name), ...(d.words ?? []).flatMap(split)]), 1],
          [named.flatMap((d) => split(d.group ?? "")), 1],
        ];
        let score = 0;
        const all = words.every((w) => {
          let best = 0;
          for (const [tokens, weight] of fields) if (tokens.some((x) => x.startsWith(w))) best = Math.max(best, tokens.includes(w) ? weight + 0.5 : weight);
          score += best;
          return best > 0;
        });
        if (!all) continue;
        out.push({
          profile: pr.id,
          profileName: pr.name,
          page: pg.id,
          pageName: pg.name,
          folder: !!pg.folder,
          key: k.id,
          label: k.label,
          sub: k.sub,
          icon: k.icon,
          steps: [...new Set(named.map((d) => d.name))],
          score,
          order,
        });
      }
  out.sort((a, b) => b.score - a.score || a.order - b.order);
  return out.slice(0, limit).map(({ score: _s, order: _o, ...hit }) => hit);
}

/** Plain copies of keys for the clipboard or a template. Folder keys only work in their own layout. */
export function portable(keys: Key[], pages: Iterable<string>): { keys: Key[]; dropped: number } {
  const here = new Set(pages);
  const ok = keys.filter((k) => k.kind !== "folder" || (k.target !== undefined && here.has(k.target)));
  return { keys: ok, dropped: keys.length - ok.length };
}

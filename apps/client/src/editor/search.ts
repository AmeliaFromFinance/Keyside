// Action search for the palette and the step picker.

import { t } from "../lib/i18n.svelte.ts";
import type { ActionDef } from "./state.svelte.ts";

export const searchWords = (q: string) => q.trim().toLowerCase().split(/\s+/).filter(Boolean);

/** Every word has to appear in the plugin name, action name or description, in English or the editor's language. */
export function actionMatches(a: ActionDef, words: string[]): boolean {
  if (!words.length) return true;
  const group = a.group ?? "";
  const desc = a.description ?? "";
  const text = [a.name, t(a.name), group, group && t(group), desc, desc && t(desc)].join(" ").toLowerCase();
  return words.every((w) => text.includes(w));
}

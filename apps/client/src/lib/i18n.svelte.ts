// English text is the key. Other languages map English to their own text.
// test/i18n.test.ts checks every string used in the app has a translation.

import de from "../i18n/de.ts";
import es from "../i18n/es.ts";
import fr from "../i18n/fr.ts";

export const languages = { en: "English", de: "Deutsch", es: "Español", fr: "Français" } as const;
export type Lang = keyof typeof languages;

const dicts: Record<Lang, Record<string, string>> = { en: {}, de, es, fr };

function detect(): Lang {
  try {
    const saved = localStorage.getItem("lang");
    if (saved && saved in languages) return saved as Lang;
  } catch {
    /* storage blocked */
  }
  const nav = (navigator.language ?? "en").slice(0, 2);
  return (nav in languages ? nav : "en") as Lang;
}

const state = $state({ lang: detect() });

export function lang(): Lang {
  return state.lang;
}

export function setLang(l: Lang) {
  state.lang = l;
  document.documentElement.lang = l;
  try {
    localStorage.setItem("lang", l);
  } catch {
    /* storage blocked */
  }
}

export function t(text: string, params?: Record<string, string | number>): string {
  const out = dicts[state.lang][text] ?? text;
  return params ? out.replace(/\{(\w+)\}/g, (m, k) => (k in params ? String(params[k]) : m)) : out;
}

/** Marks text for translation where t() runs later (lists defined up front). */
export const mark = (text: string) => text;

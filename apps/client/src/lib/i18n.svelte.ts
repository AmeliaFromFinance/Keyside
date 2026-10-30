// English text is the key. Other languages map English to their own text.
// test/i18n.test.ts checks every string used in the app has a translation.

export const languages = { en: "English", de: "Deutsch", es: "Español", fr: "Français" } as const;
export type Lang = keyof typeof languages;

// Each language is its own file, loaded when needed, so a phone only downloads its own.
const loaders: Record<Exclude<Lang, "en">, () => Promise<{ default: Record<string, string> }>> = {
  de: () => import("../i18n/de.ts"),
  es: () => import("../i18n/es.ts"),
  fr: () => import("../i18n/fr.ts"),
};
let dicts = $state.raw<Partial<Record<Lang, Record<string, string>>>>({ en: {} });

/** Loads a language's text. main.ts waits for it before showing anything. */
export async function loadLang(l: Lang) {
  if (dicts[l] || l === "en") return;
  const d = (await loaders[l]()).default;
  dicts = { ...dicts, [l]: d };
}

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
  void loadLang(l);
  state.lang = l;
  document.documentElement.lang = l;
  try {
    localStorage.setItem("lang", l);
  } catch {
    /* storage blocked */
  }
}

export function t(text: string, params?: Record<string, string | number>): string {
  const out = dicts[state.lang]?.[text] ?? text;
  return params ? out.replace(/\{(\w+)\}/g, (m, k) => (k in params ? String(params[k]) : m)) : out;
}

/** Marks text for translation where t() runs later (lists defined up front). */
export const mark = (text: string) => text;

// Lists every English string the app passes through t() or mark(), plus the built-in
// action names and settings from the companion. Run it to see what needs translating:
//   node scripts/i18n-keys.ts [de]   (with a language, prints only the missing ones)

import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const here = fileURLToPath(new URL("../", import.meta.url));

function files(dir: string): string[] {
  return readdirSync(dir).flatMap((n) => {
    const p = join(dir, n);
    if (statSync(p).isDirectory()) return n === "i18n" ? [] : files(p);
    return /\.(svelte|ts)$/.test(n) ? [p] : [];
  });
}

const unescape = (s: string) => JSON.parse(`"${s}"`) as string;

export function keys(): Set<string> {
  const out = new Set<string>();
  for (const f of files(join(here, "src"))) {
    for (const m of readFileSync(f, "utf8").matchAll(/\b(?:t|mark)\(\s*"((?:[^"\\]|\\.)*)"/g)) out.add(unescape(m[1]));
  }
  const catalog = readFileSync(join(here, "../../crates/companion/src/server/catalog.rs"), "utf8");
  for (const m of catalog.matchAll(/"(?:group|name|title|description)": "((?:[^"\\]|\\.)*)"/g)) out.add(unescape(m[1]));
  for (const m of catalog.matchAll(/"enumNames": \[([^\]]*)\]/g)) {
    for (const s of m[1].matchAll(/"((?:[^"\\]|\\.)*)"/g)) out.add(unescape(s[1]));
  }
  out.delete("");
  return out;
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  const all = [...keys()].sort();
  const lang = process.argv[2];
  if (!lang) for (const k of all) console.log(k);
  else {
    const dict = (await import(`../src/i18n/${lang}.ts`)).default as Record<string, string>;
    for (const k of all) if (!(k in dict)) console.log(JSON.stringify(k));
  }
}

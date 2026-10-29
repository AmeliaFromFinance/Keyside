import { test } from "node:test";
import assert from "node:assert/strict";
import { keys } from "../scripts/i18n-keys.ts";
import de from "../src/i18n/de.ts";
import es from "../src/i18n/es.ts";
import fr from "../src/i18n/fr.ts";

const used = keys();
const params = (s: string) => [...s.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort().join(",");

for (const [lang, dict] of Object.entries({ de, es, fr })) {
  test(`${lang} translates every string the app uses`, () => {
    const missing = [...used].filter((k) => !(k in dict));
    assert.deepEqual(missing, [], `${missing.length} missing in ${lang}`);
  });

  test(`${lang} has no leftover strings and keeps placeholders`, () => {
    const stale = Object.keys(dict).filter((k) => !used.has(k));
    assert.deepEqual(stale, [], `${stale.length} unused in ${lang}`);
    for (const [en, tr] of Object.entries(dict)) assert.equal(params(tr), params(en), `${lang}: "${en}" lost or changed a {placeholder}`);
  });
}

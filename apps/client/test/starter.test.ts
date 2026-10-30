import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { STARTER_APPS, LOOKUPS, buildProfile, lookupKey } from "../src/editor/starter.ts";

const manifests = new Map<string, { actions: Array<{ id: string; settings?: { properties: Record<string, unknown> } }> }>();
for (const a of STARTER_APPS) manifests.set(a.plugin, JSON.parse(readFileSync(new URL(`../../../plugins/${a.plugin.split(".").pop()}/manifest.json`, import.meta.url), "utf8")));

const id = /^[A-Za-z0-9_-]{1,40}$/;

test("every starter key uses a real action with known settings and fits its page", () => {
  const p = buildProfile(
    STARTER_APPS.map((a) => a.id),
    {},
    "",
    [],
  );
  assert.equal(p.pages.length, STARTER_APPS.length);
  assert.match(p.id, id);
  for (const pg of p.pages) {
    assert.match(pg.id, id);
    assert.ok(pg.keys.length > 0, `${pg.name} has keys`);
    const cells = new Set<string>();
    for (const k of pg.keys) {
      assert.ok(k.x + (k.w ?? 1) <= pg.cols && k.y + (k.h ?? 1) <= pg.rows, `${pg.name}/${k.label} fits`);
      for (let dy = 0; dy < (k.h ?? 1); dy++)
        for (let dx = 0; dx < (k.w ?? 1); dx++) {
          const c = `${k.x + dx},${k.y + dy}`;
          assert.ok(!cells.has(c), `${pg.name}/${k.label} overlaps`);
          cells.add(c);
        }
      if (k.kind === "slider") assert.ok(k.slider);
      for (const a of k.actions) {
        const [plugin, action] = a.type.split("/");
        const def = manifests.get(plugin)?.actions.find((x) => x.id === action);
        assert.ok(def, `${a.type} exists`);
        for (const field of Object.keys(a.settings)) assert.ok(field in (def!.settings?.properties ?? {}), `${a.type} has setting ${field}`);
      }
    }
  }
});

test("lists read from the apps become keys, and taken ids are skipped", () => {
  const found = {
    [lookupKey(LOOKUPS.obs[0])]: [
      { value: "Gameplay", label: "Gameplay" },
      { value: "BRB", label: "BRB" },
    ],
    [lookupKey(LOOKUPS.obs[1])]: [{ value: "Mic (Shure)", label: "Mic (Shure)" }],
  };
  const p = buildProfile(["obs", "discord"], found, "Stream", ["setup"], (s) => (s === "Scene" ? "Szene" : s));
  assert.equal(p.id, "setup-2");
  assert.equal(p.name, "Stream");
  assert.deepEqual(
    p.pages.map((x) => x.id),
    ["obs", "discord"],
  );
  const obs = p.pages[0];
  assert.deepEqual(obs.keys[0].actions[0], { type: "dev.keyside.obs/scene", settings: { scene: "Gameplay", target: "auto", thumbnail: false } });
  assert.equal(obs.keys[0].sub, "Szene", "labels are translated");
  assert.equal(obs.keys.find((k) => k.label === "Mic")?.actions[0].settings.input, "Mic (Shure)");
});

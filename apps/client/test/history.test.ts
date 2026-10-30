import { test } from "node:test";
import assert from "node:assert/strict";
import type { LayoutFile } from "@keyside/protocol";
import { diffLayouts, isSame } from "../src/editor/history.ts";

const base = (): LayoutFile => ({
  version: 1,
  rev: 3,
  defaultProfile: "main",
  profiles: [
    {
      id: "main",
      name: "Main",
      homePage: "a",
      pages: [
        { id: "a", name: "A", cols: 3, rows: 5, keys: [{ id: "k1", x: 0, y: 0, kind: "button", label: "One", actions: [] }] },
        { id: "b", name: "B", cols: 3, rows: 5, keys: [] },
      ],
    },
  ],
});

test("an identical layout counts as the same, even with empty extras", () => {
  const a = base();
  const b = base();
  b.profiles[0].pages[0].keys[0].offActions = [];
  assert.ok(isSame(diffLayouts(a, b)));
});

test("lists added, removed and changed parts", () => {
  const a = base();
  const b = base();
  b.profiles[0].pages[0].keys[0].label = "Uno";
  b.profiles[0].pages[0].keys.push({ id: "k2", x: 1, y: 0, kind: "button", label: "Two", actions: [] });
  b.profiles[0].pages.pop();
  b.profiles.push({ id: "x", name: "Extra", homePage: "p", pages: [{ id: "p", name: "P", cols: 1, rows: 1, keys: [] }] });
  const d = diffLayouts(a, b);
  assert.deepEqual(d.layoutsAdded, ["Extra"]);
  assert.deepEqual(d.pagesRemoved, ["Main › B"]);
  assert.equal(d.keysChanged, 1);
  assert.equal(d.keysAdded, 1);
  const r = base();
  r.profiles[0].pages[1].name = "Renamed";
  assert.equal(diffLayouts(a, r).renamed, 1);
});

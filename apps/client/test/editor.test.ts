import { test } from "node:test";
import assert from "node:assert/strict";
import type { Key, LayoutFile } from "@keyside/protocol";
import { searchKeys, portable } from "../src/editor/keysearch.ts";
import { diffLayouts, countChanges } from "../src/editor/changes.ts";

const key = (id: string, x: number, label: string, extra: Partial<Key> = {}): Key => ({ id, x, y: 0, kind: "button", label, actions: [], ...extra });

function layout(): LayoutFile {
  return {
    version: 1,
    rev: 1,
    defaultProfile: "main",
    profiles: [
      {
        id: "main",
        name: "Main",
        homePage: "p1",
        pages: [
          { id: "p1", name: "Home", cols: 3, rows: 3, keys: [key("a", 0, "Mute mic", { sub: "Discord" }), key("b", 1, "Scene", { actions: [{ type: "dev.obs/scene", settings: {} }] }), key("f", 2, "More", { kind: "folder", target: "p2" })] },
          { id: "p2", name: "Folder", folder: true, cols: 3, rows: 3, keys: [key("c", 0, "Lights")] },
        ],
      },
      { id: "game", name: "Games", homePage: "g1", pages: [{ id: "g1", name: "Play", cols: 3, rows: 3, keys: [key("a", 0, "Mute game")] }] },
    ],
  };
}

const describe = (type: string) => (type === "dev.obs/scene" ? { name: "Switch scene", group: "OBS Studio" } : { name: type });
const found = (q: string) => searchKeys(layout(), q, describe).map((h) => `${h.profile}/${h.page}/${h.key}`);

test("finds keys in every layout by label, second line, step and plugin", () => {
  assert.deepEqual(found("mute"), ["main/p1/a", "game/g1/a"]);
  assert.deepEqual(found("discord"), ["main/p1/a"]);
  assert.deepEqual(found("switch"), ["main/p1/b"]);
  assert.deepEqual(found("obs stu"), ["main/p1/b"]);
  assert.deepEqual(found("lights"), ["main/p2/c"]);
  assert.deepEqual(found("mute game"), ["game/g1/a"]);
  assert.deepEqual(found("  "), []);
  assert.deepEqual(found("nothing"), []);
});

test("label matches come before step matches", () => {
  const l = layout();
  l.profiles[0].pages[0].keys.push(key("s", 0, "Switch", { y: 1 } as Partial<Key>));
  assert.deepEqual(searchKeys(l, "switch", describe).map((h) => h.key), ["s", "b"]);
});

test("folder keys only travel within their own layout", () => {
  const keys = layout().profiles[0].pages[0].keys;
  assert.equal(portable(keys, ["p1", "p2"]).dropped, 0);
  const other = portable(keys, ["g1"]);
  assert.equal(other.dropped, 1);
  assert.deepEqual(other.keys.map((k) => k.id), ["a", "b"]);
});

test("lists what changed per layout and page", () => {
  const old = layout();
  const cur = layout();
  const home = cur.profiles[0].pages[0];
  home.keys[0].label = "Mute";
  home.keys[0].icon = "mic-off";
  home.keys[1].x = 1;
  home.keys[1].y = 1;
  home.keys.push(key("n", 0, "New", { y: 2 } as Partial<Key>));
  // Lights moves from the folder to the home page.
  const lights = cur.profiles[0].pages[1].keys.pop()!;
  home.keys.push({ ...lights, y: 2, x: 1 });
  cur.profiles[1].pages = [{ id: "g2", name: "Chat", cols: 2, rows: 2, keys: [key("z", 0, "Hi")] }];
  cur.profiles[1].homePage = "g2";
  cur.profiles.push({ id: "new", name: "Work", homePage: "w", pages: [{ id: "w", name: "Work", cols: 2, rows: 2, keys: [] }] });

  const d = diffLayouts(old, cur);
  assert.deepEqual(
    d.map((l) => [l.name, l.status]),
    [
      ["Main", "changed"],
      ["Games", "changed"],
      ["Work", "added"],
    ],
  );
  const homeChanges = d[0].pages.find((p) => p.id === "p1")!;
  assert.deepEqual(homeChanges.items, [
    { kind: "edited", label: "Mute mic", parts: ["label", "icon"] },
    { kind: "moved", label: "Scene" },
    { kind: "added", label: "New" },
    { kind: "moved", label: "Lights", from: "Folder" },
  ]);
  assert.equal(d[0].pages.find((p) => p.id === "p2"), undefined, "the folder lists nothing once Lights moved out");
  assert.deepEqual(
    d[1].pages.map((p) => [p.name, p.status, p.keys]),
    [
      ["Chat", "added", 1],
      ["Play", "removed", 1],
    ],
  );
  assert.equal(countChanges(d), 7);
  assert.deepEqual(diffLayouts(old, layout()), []);
});

test("keys moved to a new page are listed there", () => {
  const old = layout();
  const cur = layout();
  const moved = cur.profiles[0].pages[0].keys.splice(1, 1)[0];
  cur.profiles[0].pages.push({ id: "p3", name: "Extra", cols: 3, rows: 3, keys: [moved] });
  const d = diffLayouts(old, cur);
  assert.deepEqual(
    d[0].pages.map((p) => [p.name, p.status, p.items]),
    [["Extra", "added", [{ kind: "moved", label: "Scene", from: "Home" }]]],
  );
});

test("notices removed layouts, renames and page settings", () => {
  const old = layout();
  const cur = layout();
  cur.profiles.pop();
  cur.profiles[0].name = "Stream";
  cur.profiles[0].pages[0].cols = 4;
  cur.profiles[0].pages[0].keys.shift();
  const d = diffLayouts(old, cur);
  assert.equal(d[0].renamed, "Main");
  assert.deepEqual(d[0].pages[0].parts, ["grid"]);
  assert.deepEqual(d[0].pages[0].items, [{ kind: "removed", label: "Mute mic" }]);
  assert.deepEqual([d[1].name, d[1].status], ["Games", "removed"]);
});

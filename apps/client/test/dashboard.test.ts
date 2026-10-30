import { test } from "node:test";
import assert from "node:assert/strict";
import { dashboardPage } from "../src/editor/dashboard.ts";

const same = (s: string) => s;

test("makes tiles only for connected apps", () => {
  assert.equal(dashboardPage([], "p1", same), null);
  assert.equal(dashboardPage([{ id: "dev.keyside.hue", prefix: "hue" }], "p1", same), null, "nothing to show for lights");

  const page = dashboardPage(
    [
      { id: "dev.keyside.twitch", prefix: "twitch", feeds: [{ name: "chat" }, { name: "events" }] },
      { id: "dev.keyside.obs", prefix: "obs" },
    ],
    "p9",
    (s) => `[${s}]`,
  )!;
  assert.equal(page.id, "p9");
  assert.equal(page.name, "[Dashboard]");
  // OBS comes first.
  assert.deepEqual(page.keys[0].actions, [{ type: "dev.keyside.obs/stats", settings: { show: "live" } }]);
  assert.equal(page.keys[0].label, "[Live]");
  const texts = page.keys.map((k) => k.widget?.text).filter(Boolean);
  assert.deepEqual(texts, ["{twitch.viewers}", "{twitch.followers}", "{twitch.subs}", "{core.chat_rate}"]);
  const chat = page.keys.find((k) => k.widget?.feed)!;
  assert.deepEqual([chat.widget!.feed, chat.w, chat.h, chat.x], ["twitch.chat", page.cols, 2, 0]);
});

test("keys fit the grid without overlapping", () => {
  const page = dashboardPage(
    ["obs", "twitch", "youtube", "streamlabs"].map((n) => ({ id: `dev.keyside.${n}`, prefix: n === "streamlabs" ? "slobs" : n, feeds: n === "youtube" ? [{ name: "chat" }] : [] })),
    "p1",
    same,
  )!;
  const used = new Set<string>();
  for (const k of page.keys) {
    for (let dy = 0; dy < (k.h ?? 1); dy++)
      for (let dx = 0; dx < (k.w ?? 1); dx++) {
        const cell = `${k.x + dx},${k.y + dy}`;
        assert.ok(k.x + dx < page.cols && k.y + dy < page.rows, `${k.label} is outside`);
        assert.ok(!used.has(cell), `${k.label} overlaps`);
        used.add(cell);
      }
  }
  assert.ok(page.rows <= 12);
  assert.equal(new Set(page.keys.map((k) => k.id)).size, page.keys.length);
});

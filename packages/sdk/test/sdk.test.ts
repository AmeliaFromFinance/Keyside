import { createTestHost } from "../src/testing.ts";
import { test } from "node:test";
import assert from "node:assert/strict";
import { definePlugin, setting } from "../src/index.ts";

const plugin = definePlugin({
  async start(host) {
    host.status("connected", undefined, { label: "LIVE", tone: "danger" });
    host.setVar("ready", true);
  },
  actions: {
    count: {
      press({ key, settings }) {
        const n = setting(settings, "step", 1);
        key.set({ label: "old" });
        key.set({ label: `+${n}`, badge: "ON" });
      },
      options: (field) => (field === "scene" ? [{ value: "a", label: "Scene A" }] : []),
    },
    fail: {
      press() {
        throw new Error("nope");
      },
    },
  },
});

test("runs actions, batches key updates and answers options", async () => {
  const h = createTestHost(plugin);
  await h.start();
  await h.sync([{ id: "k1", action: "count", settings: { step: 2 } }]);
  await h.press("k1", "count", { step: 2 });
  await h.settle();
  assert.deepEqual(h.view("k1"), { label: "+2", badge: "ON" });
  assert.equal(h.sent.filter((s) => s.method === "key.set").length, 1, "two updates in one tick go out once, the last one wins");
  assert.equal(h.variable("ready"), true);
  assert.deepEqual(await h.options("count", "scene"), [{ value: "a", label: "Scene A" }]);
  await assert.rejects(h.press("k1", "fail"), /nope/);
  await h.stop();
});

test("keys that were removed can't be updated", async () => {
  const h = createTestHost(plugin);
  await h.start();
  await h.sync([{ id: "k1", action: "count" }]);
  await h.sync([]);
  await h.press("k1", "count");
  await h.settle();
  assert.equal(h.view("k1"), undefined);
  await h.stop();
});

test("unfeed sends ids in batches of 100", async () => {
  let host: any;
  const h = createTestHost(definePlugin({ start: (x) => void (host = x), actions: {} }));
  await h.start();
  host.unfeed("chat", { ids: Array.from({ length: 150 }, (_, i) => `m${i}`), author: "moss" });
  host.unfeed("chat", { all: true });
  host.unfeed("chat", { ids: [] });
  await h.settle();
  const sent = h.unfeeds("chat");
  assert.deepEqual(sent.map((m) => [m.ids?.length, m.author, m.all]), [[100, "moss", undefined], [50, undefined, undefined], [undefined, undefined, true]]);
  await h.stop();
});

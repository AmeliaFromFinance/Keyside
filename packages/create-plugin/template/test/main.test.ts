import { createTestHost } from "@phonedeck/sdk/testing";
import { test } from "node:test";
import assert from "node:assert/strict";
import plugin from "../src/main.ts";

test("hello counts presses", async () => {
  const h = createTestHost(plugin);
  await h.start();
  await h.sync([{ id: "k1", action: "hello" }]);
  await h.press("k1", "hello", { message: "Hi" });
  await h.settle();
  assert.deepEqual(h.view("k1"), { badge: "1", sub: "Hi" });
  await h.stop();
});

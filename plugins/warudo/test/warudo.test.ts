import { createTestHost } from "@keyside/sdk/testing";
import { test } from "node:test";
import assert from "node:assert/strict";
import plugin from "../src/main.ts";
import { fakeServer, eventually } from "../../_lib/fake.ts";

test("sends action and data as JSON", async () => {
  const srv = await fakeServer(() => {});
  const h = createTestHost(plugin, { connection: { host: "127.0.0.1", port: srv.port } });
  await h.start();
  await h.sync([{ id: "k", action: "action", settings: { action: "Spin" } }]);
  await eventually(() => h.status()?.state === "connected");
  await h.press("k", "action", { action: "Spin", data: "fast" });
  assert.deepEqual(await srv.until((m) => m.action), { action: "Spin", data: "fast" });
  await srv.close();
  await eventually(() => h.view("k")?.disabled === true);
  await h.stop();
});

import { createTestHost } from "@keyside/sdk/testing";
import { test } from "node:test";
import assert from "node:assert/strict";
import plugin from "../src/main.ts";
import { fakeServer, eventually } from "../../_lib/fake.ts";

test("sends the command text to /vnyan", async () => {
  let path = "";
  const srv = await fakeServer(() => {}, (_ws, url) => (path = url));
  const h = createTestHost(plugin, { connection: { host: "127.0.0.1", port: srv.port } });
  await h.start();
  await eventually(() => h.status()?.state === "connected");
  await h.press("k", "command", { command: "wave" });
  assert.equal(await srv.until((m) => m === "wave"), "wave");
  assert.equal(path, "/vnyan");
  await h.stop();
  await srv.close();
});

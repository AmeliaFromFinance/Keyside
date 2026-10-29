import { createTestHost } from "@keyside/sdk/testing";
import { test } from "node:test";
import assert from "node:assert/strict";
import plugin, { endpoints, hexToXy } from "../src/main.ts";
import { eventually } from "../../_lib/fake.ts";
import { fakeHttp } from "../../_lib/fakehttp.ts";

test("pairs with the bridge button and controls a room", async () => {
  let presses = 0;
  const room = { on: false };
  const bridge = await fakeHttp((h) => {
    if (h.path === "/api" && h.method === "POST") return { json: presses++ === 0 ? [{ error: { type: 101, description: "link button not pressed" } }] : [{ success: { username: "u1" } }] };
    if (!h.path.startsWith("/api/u1/")) return { json: [{ error: { type: 1, description: "unauthorized user" } }] };
    const p = h.path.slice("/api/u1".length);
    if (p === "/groups") return { json: { "1": { name: "Office", type: "Room", state: { any_on: room.on }, action: { bri: 127 } } } };
    if (p === "/lights") return { json: { "3": { name: "Desk", state: { on: room.on, bri: 127, reachable: true } } } };
    if (p === "/scenes") return { json: { abc: { name: "Focus", group: "1" } } };
    if (p === "/groups/1/action") {
      if (typeof h.body.on === "boolean") room.on = h.body.on;
      return { json: [{ success: {} }] };
    }
  });
  endpoints.base = () => bridge.url;
  const h = createTestHost(plugin, { connection: { bridge: "192.168.1.2" } });
  await h.start();
  await h.sync([
    { id: "p", action: "power", settings: { target: "group:1" } },
    { id: "s", action: "scene", settings: { scene: "abc" } },
  ]);
  await eventually(() => h.status()?.state === "connected", 8000);
  assert.equal(h.sent.find((s) => s.method === "connection.update")?.params.values.username, "u1");
  assert.deepEqual(await h.options("power", "target"), [
    { value: "group:1", label: "Office (room)" },
    { value: "group:0", label: "All lights" },
    { value: "light:3", label: "Desk" },
  ]);
  await h.press("p", "power", { target: "group:1", mode: "toggle" });
  assert.equal(room.on, true);
  await eventually(() => h.view("p")?.badge === "ON");
  await h.press("s", "scene", { scene: "abc" });
  assert.equal(bridge.hits.at(-1)?.body.scene, "abc");
  await assert.rejects(h.press("p", "power", { target: "group:../../config" }), /Pick a room/);
  await h.stop();
  await bridge.close();
});

test("converts colours to Hue xy", () => {
  assert.deepEqual(hexToXy("#ff0000"), [0.7006, 0.2993]);
  assert.throws(() => hexToXy("red"));
});

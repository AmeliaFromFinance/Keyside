import { createTestHost } from "@keyside/sdk/testing";
import { test } from "node:test";
import assert from "node:assert/strict";
import plugin, { endpoints } from "../src/main.ts";
import { eventually } from "../../_lib/fake.ts";
import { fakeHttp } from "../../_lib/fakehttp.ts";

test("reads and sets the light", async () => {
  const state = { on: 0, brightness: 40, temperature: 200 };
  const light = await fakeHttp((h) => {
    if (h.method === "PUT") Object.assign(state, h.body.lights[0]);
    return { json: { numberOfLights: 1, lights: [state] } };
  });
  endpoints.base = () => light.url;
  const h = createTestHost(plugin, { connection: { light1: "192.168.1.20" } });
  await h.start();
  await h.sync([
    { id: "p", action: "power", settings: { light: "all" } },
    { id: "b", action: "brightness", settings: { light: "1" } },
  ]);
  await eventually(() => h.view("b")?.meta === "40%");
  assert.equal(h.view("p")?.icon, "lightbulb-off");
  await h.press("p", "power", { light: "all", mode: "toggle" });
  assert.equal(state.on, 1);
  await eventually(() => h.view("p")?.badge === "40%");
  await h.press("p", "preset", { light: "all", brightness: 80, kelvin: 5000 });
  assert.deepEqual(state, { on: 1, brightness: 80, temperature: 200 });
  await h.stop();
  await light.close();
});

test("asks for an address", async () => {
  const h = createTestHost(plugin, { connection: {} });
  await h.start();
  await eventually(() => /address/.test(String(h.status()?.detail)));
  await assert.rejects(h.press("p", "power", { light: "2" }), /no address/);
  await h.stop();
});

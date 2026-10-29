import { createTestHost } from "@keyside/sdk/testing";
import { test } from "node:test";
import assert from "node:assert/strict";
import plugin from "../src/main.ts";
import { fakeServer, eventually } from "../../_lib/fake.ts";

test("authenticates, shows states and calls services", async () => {
  const srv = await fakeServer(
    (ws, m) => {
      const ok = (result: unknown = null) => ws.send(JSON.stringify({ id: m.id, type: "result", success: true, result }));
      if (m.type === "auth") return ws.send(JSON.stringify({ type: m.access_token === "tok" ? "auth_ok" : "auth_invalid" }));
      if (m.type === "get_states")
        return ok([
          { entity_id: "light.desk", state: "off", attributes: { friendly_name: "Desk" } },
          { entity_id: "sensor.temp", state: "21.5", attributes: { unit_of_measurement: "°C" } },
          { entity_id: "scene.movie", state: "scening", attributes: {} },
        ]);
      ok();
    },
    (ws) => ws.send(JSON.stringify({ type: "auth_required" })),
  );
  const h = createTestHost(plugin, { connection: { host: "127.0.0.1", port: srv.port, token: "tok", watch: "sensor.temp" } });
  await h.start();
  await h.sync([
    { id: "t", action: "toggle", settings: { entity: "light.desk" } },
    { id: "s", action: "state", settings: { entity: "sensor.temp" } },
  ]);
  await eventually(() => h.status()?.state === "connected");
  await eventually(() => h.view("s")?.sub === "21.5 °C");
  assert.equal(h.variable("sensor.temp"), "21.5 °C");

  await h.press("t", "toggle", { entity: "light.desk", mode: "toggle" });
  const call = await srv.until((m) => m.type === "call_service");
  assert.deepEqual([call.domain, call.service, call.target], ["homeassistant", "turn_on", { entity_id: "light.desk" }]);

  srv.broadcast({ type: "event", event: { data: { entity_id: "light.desk", new_state: { state: "on", attributes: { brightness: 128 } } } } });
  await eventually(() => h.view("t")?.badge === "ON");

  await assert.rejects(h.press("x", "service", { service: "light.turn_on", data: "[1]" }), /JSON object/);
  await assert.rejects(h.press("x", "toggle", { entity: "scene.movie" }), /can't be used/);
  await h.stop();
  await srv.close();
});

test("a bad token is reported", async () => {
  const srv = await fakeServer((ws) => ws.send(JSON.stringify({ type: "auth_invalid" })), (ws) => ws.send(JSON.stringify({ type: "auth_required" })));
  const h = createTestHost(plugin, { connection: { host: "127.0.0.1", port: srv.port, token: "bad" } });
  await h.start();
  await eventually(() => h.status()?.state === "error");
  await h.stop();
  await srv.close();
});

import { createTestHost } from "@keyside/sdk/testing";
import { test } from "node:test";
import assert from "node:assert/strict";
import plugin, { endpoints } from "../src/main.ts";
import { fakeServer, eventually } from "../../_lib/fake.ts";
import { fakeHttp } from "../../_lib/fakehttp.ts";

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

test("thermostat, light colour, media player with cover and camera snapshots", async () => {
  const jpeg = new Uint8Array([0xff, 0xd8, 0xff, 0xe0, 1, 2, 3]);
  const web = await fakeHttp((x) => (x.path.startsWith("/api/") && x.headers.authorization === "Bearer tok" ? { bytes: jpeg, type: "image/jpeg" } : { status: 401, json: {} }));
  endpoints.http = web.url;
  const states = [
    { entity_id: "climate.lounge", state: "heat", attributes: { temperature: 20, current_temperature: 18.5, min_temp: 10, max_temp: 30, target_temp_step: 0.5 } },
    { entity_id: "light.desk", state: "on", attributes: { color_temp_kelvin: 4250, min_color_temp_kelvin: 2000, max_color_temp_kelvin: 6500 } },
    { entity_id: "media_player.tv", state: "playing", attributes: { media_title: "Intro", media_artist: "The xx", entity_picture: "/api/media_player_proxy/media_player.tv?token=x" } },
    { entity_id: "camera.door", state: "idle", attributes: {} },
  ];
  const srv = await fakeServer(
    (ws, m) => {
      const ok = (result: unknown = null) => ws.send(JSON.stringify({ id: m.id, type: "result", success: true, result }));
      if (m.type === "auth") return ws.send(JSON.stringify({ type: "auth_ok" }));
      if (m.type === "get_states") return ok(states);
      ok();
    },
    (ws) => ws.send(JSON.stringify({ type: "auth_required" })),
  );
  const h = createTestHost(plugin, { connection: { host: "127.0.0.1", port: srv.port, token: "tok" } });
  await h.start();
  await h.sync([
    { id: "th", action: "temperature", settings: { entity: "climate.lounge" } },
    { id: "ct", action: "color-temp", settings: { entity: "light.desk" } },
    { id: "tv", action: "media", settings: { entity: "media_player.tv" } },
    { id: "cam", action: "camera", settings: { entity: "camera.door", seconds: 5 } },
  ]);
  await eventually(() => h.status()?.state === "connected");
  await eventually(() => h.view("th")?.meta === "20°" && h.view("th")?.sub === "Now 18.5°");
  assert.equal(h.view("th")?.value, 0.5);
  await eventually(() => h.view("ct")?.meta === "4250 K");
  await eventually(() => String(h.view("tv")?.image).startsWith("data:image/jpeg;base64,") && h.view("tv")?.label === "Intro");
  await eventually(() => String(h.view("cam")?.image).startsWith("data:image/jpeg;base64,"));
  assert.ok(web.hits.some((x) => x.path === "/api/camera_proxy/camera.door" && x.query.get("width") === "320"));

  await h.slide("th", "temperature", 0.76, { entity: "climate.lounge" });
  const set = await srv.until((m) => m.type === "call_service" && m.service === "set_temperature");
  assert.equal(set.service_data.temperature, 25, "10 + 0.76 * 20, on 0.5 steps");

  await h.slide("ct", "color-temp", 0, { entity: "light.desk" });
  assert.equal((await srv.until((m) => m.type === "call_service" && m.service_data?.color_temp_kelvin !== undefined)).service_data.color_temp_kelvin, 2000);

  await h.press("tv", "media", { entity: "media_player.tv", op: "next" });
  assert.equal((await srv.until((m) => m.service === "media_next_track")).target.entity_id, "media_player.tv");
  await h.stop();
  await srv.close();
  await web.close();
  endpoints.http = "";
});

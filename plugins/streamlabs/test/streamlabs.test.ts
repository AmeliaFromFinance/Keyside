import { createTestHost } from "@phonedeck/sdk/testing";
import { test } from "node:test";
import assert from "node:assert/strict";
import plugin from "../src/main.ts";
import { fakeServer, eventually } from "../../_lib/fake.ts";

test("authenticates, mirrors state and sends commands", async () => {
  let path = "";
  const srv = await fakeServer(
    (ws, m) => {
      const ok = (result: unknown) => ws.send(JSON.stringify({ jsonrpc: "2.0", id: m.id, result }));
      const { resource, args } = m.params;
      if (m.method === "auth") return args[0] === "tok" ? ok(true) : ws.send(JSON.stringify({ jsonrpc: "2.0", id: m.id, error: { message: "bad" } }));
      if (m.method === "getModel") return ok({ streamingStatus: "offline", recordingStatus: "recording", recordingStatusTime: new Date().toISOString(), replayBufferStatus: "offline" });
      if (m.method === "getScenes") return ok([{ id: "s1", name: "Live" }, { id: "s2", name: "BRB" }]);
      if (m.method === "activeSceneId") return ok("s1");
      if (m.method === "getSources") return ok([{ sourceId: "a1", name: "Mic", muted: false, fader: { deflection: 0.8 } }]);
      return ok({ _type: "SUBSCRIPTION", resourceId: `${resource}.${m.method}` });
    },
    (_ws, url) => (path = url),
  );
  const h = createTestHost(plugin, { connection: { host: "127.0.0.1", port: srv.port, token: "tok" } });
  await h.start();
  await h.sync([
    { id: "live", action: "scene", settings: { scene: "Live" } },
    { id: "mic", action: "mute", settings: { source: "Mic" } },
    { id: "rec", action: "record" },
  ]);
  await eventually(() => h.view("live")?.badge === "ON AIR");
  assert.equal(path, "/api/websocket");
  assert.equal(h.view("rec")?.label, "Stop recording");
  await eventually(() => (h.status()?.pills as any[])?.[0]?.label === "REC");

  await h.press("mic", "mute", { source: "Mic", mode: "toggle" });
  const mute = await srv.until((m) => m.method === "setMuted");
  assert.equal(mute.params.resource, 'AudioSource["a1"]');
  assert.deepEqual(mute.params.args, [true]);

  srv.broadcast({ jsonrpc: "2.0", id: null, result: { _type: "EVENT", resourceId: "ScenesService.sceneSwitched", emitter: "STREAM", data: { id: "s2", name: "BRB" } } });
  await eventually(() => h.variable("scene") === "BRB");
  await h.stop();
  await srv.close();
});

test("a bad token is reported and not retried", async () => {
  const srv = await fakeServer((ws, m) => ws.send(JSON.stringify({ jsonrpc: "2.0", id: m.id, error: { message: "bad" } })));
  const h = createTestHost(plugin, { connection: { host: "127.0.0.1", port: srv.port, token: "nope" } });
  await h.start();
  await eventually(() => h.status()?.state === "error");
  await h.stop();
  await srv.close();
});

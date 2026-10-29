import { createTestHost } from "@phonedeck/sdk/testing";
import { test } from "node:test";
import assert from "node:assert/strict";
import plugin from "../src/main.ts";
import { fakeServer, eventually } from "../../_lib/fake.ts";

test("finds the state node, shows the current state and switches", async () => {
  let name = "";
  let current = "idle";
  const srv = await fakeServer(
    (ws, _m, raw) => {
      const m = JSON.parse(raw.slice(raw.indexOf(":") + 1));
      const out = (o: object) => ws.send(`nodes:${JSON.stringify(o)}`);
      if (m.event === "list") return out({ event: "list", entries: [{ type: "stateEvents", id: "mini", name: "avatar state" }] });
      if (m.event !== "payload") return;
      const p = m.payload;
      const reply = (payload: object) => out({ event: "payload", type: "stateEvents", id: "mini", name: "avatar state", payload });
      if (p.event === "list") reply({ event: "list", states: [{ id: "idle", name: "idle" }, { id: "happy", name: "happy" }] });
      if (p.event === "set") current = p.state;
      if (p.event === "peek") reply({ event: "peek", state: current });
      if (p.event === "thumb") reply({ event: "thumb", state: p.state, png: "iVBORw0KGgo=", width: 128, height: 128 });
    },
    (_ws, url) => (name = url),
  );
  const h = createTestHost(plugin, { connection: { host: "127.0.0.1", port: srv.port } });
  await h.start();
  await h.sync([
    { id: "a", action: "state", settings: { state: "idle" } },
    { id: "b", action: "state", settings: { state: "happy", op: "set" } },
  ]);
  await eventually(() => h.view("a")?.badge === "ON");
  assert.equal(name, "/?n=PhoneDeck");
  await eventually(() => String(h.view("b")?.image).startsWith("data:image/png;base64,"));
  await h.press("b", "state", { state: "happy", op: "set" });
  await eventually(() => h.view("b")?.badge === "ON");
  assert.equal(h.variable("state"), "happy");
  await h.stop();
  await srv.close();
});

import { createTestHost } from "@keyside/sdk/testing";
import { test } from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import plugin, { varName } from "../src/main.ts";
import { fakeServer, eventually } from "../../_lib/fake.ts";

const sha = (s: string) => createHash("sha256").update(s).digest("base64");

async function fakeBot(password: string | null) {
  return fakeServer(
    (ws, m) => {
      const ok = (extra: object = {}) => ws.send(JSON.stringify({ id: m.id, status: "ok", ...extra }));
      switch (m.request) {
        case "Authenticate":
          return m.authentication === sha(sha(`${password}salt`) + "chal") ? ok() : ws.send(JSON.stringify({ id: m.id, status: "error", error: "bad" }));
        case "GetActions":
          return ok({ actions: [{ id: "g1", name: "Hydrate", enabled: true }, { id: "g2", name: "Old", enabled: false }] });
        case "GetGlobals":
          return ok({ variables: { "Death Count": { name: "Death Count", value: 7 } } });
        default:
          return ok();
      }
    },
    (ws) => ws.send(JSON.stringify({ request: "Hello", info: { version: "1.0.0" }, ...(password ? { authentication: { salt: "salt", challenge: "chal" } } : {}) })),
  );
}

test("runs actions by name and shows their state", async () => {
  const sb = await fakeBot("pw");
  const h = createTestHost(plugin, { connection: { host: "127.0.0.1", port: sb.port, password: "pw" } });
  await h.start();
  await h.sync([
    { id: "a", action: "run", settings: { actionName: "Hydrate" } },
    { id: "b", action: "run", settings: { actionName: "Old" } },
    { id: "c", action: "run", settings: { actionName: "Missing" } },
  ]);
  await eventually(() => h.status()?.state === "connected");
  await h.settle();
  assert.equal(h.view("a"), null);
  assert.equal(h.view("b")?.disabled, true);
  assert.match(String(h.view("c")?.sub), /Not in/);
  assert.equal(h.variable("death_count"), 7);

  await h.press("a", "run", { actionName: "Hydrate", args: { size: "big" } });
  const sent = await sb.until((m) => m.request === "DoAction");
  assert.deepEqual(sent.action, { id: "g1", name: "Hydrate" });
  assert.deepEqual(sent.args, { size: "big" });

  sb.broadcast({ event: { source: "General", type: "Custom" }, data: { keyside: { var: "hype", value: 3 } } });
  sb.broadcast({ event: { source: "Twitch", type: "ChatMessage" }, data: { message: { displayName: "Fern", message: "hi!", color: "#ff0000" } } });
  await eventually(() => h.variable("hype") === 3);
  await eventually(() => h.sent.some((s) => s.method === "feed.push" && s.params.item.text === "hi!"));
  await h.stop();
  await sb.close();
});

test("a wrong password stops and explains", async () => {
  const sb = await fakeBot("right");
  const h = createTestHost(plugin, { connection: { host: "127.0.0.1", port: sb.port, password: "nope" } });
  await h.start();
  await eventually(() => h.status()?.state === "error");
  assert.match(String(h.status()?.detail), /password/);
  await h.stop();
  await sb.close();
});

test("variable names are made safe", () => {
  assert.equal(varName("Death Count"), "death_count");
  assert.equal(varName("__x__"), "x");
  assert.equal(varName("9lives"), null);
  assert.equal(varName("{evil}.name"), "evil_name");
});

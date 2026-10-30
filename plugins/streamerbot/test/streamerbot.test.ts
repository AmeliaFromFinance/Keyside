import { createTestHost } from "@keyside/sdk/testing";
import { test } from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import plugin, { nextValue, varName } from "../src/main.ts";
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
          return ok({ actions: [{ id: "g1", name: "Hydrate", enabled: true }, { id: "g2", name: "Old", enabled: false }, { id: "g3", name: "Keyside: Set global", enabled: true }] });
        case "GetGlobal":
          return m.variable === "Death Count" ? ok({ variable: "Death Count", value: 9 }) : ws.send(JSON.stringify({ id: m.id, status: "error", error: "not found" }));
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

test("sets globals through the helper action", async () => {
  const sb = await fakeBot(null);
  const h = createTestHost(plugin, { connection: { host: "127.0.0.1", port: sb.port } });
  await h.start();
  await eventually(() => h.status()?.state === "connected");
  assert.deepEqual(await h.options("set-global", "name"), [{ value: "Death Count", label: "Death Count" }]);

  await h.press("k", "set-global", { name: "Death Count", op: "add", value: "1" });
  let sent = await sb.until((m) => m.request === "DoAction");
  assert.deepEqual(sent.action, { id: "g3", name: "Keyside: Set global" });
  assert.deepEqual(sent.args, { keysideName: "Death Count", keysideValue: 10, keysidePersisted: true }, "adds to the value Streamer.bot has now");
  await eventually(() => h.variable("death_count") === 10);

  sb.received.length = 0;
  await h.press("k", "set-global", { name: "mood", value: "hype", persisted: false });
  sent = await sb.until((m) => m.request === "DoAction");
  assert.deepEqual(sent.args, { keysideName: "mood", keysideValue: "hype", keysidePersisted: false });
  await assert.rejects(h.press("k", "set-global", { name: "x", helper: "Missing helper" }), /Add the "Missing helper" action/);
  await h.stop();
  await sb.close();
});

test("works out the next value", () => {
  assert.equal(nextValue("toggle", "True", ""), false);
  assert.equal(nextValue("toggle", 0, ""), true);
  assert.equal(nextValue("add", "4", ""), 5);
  assert.equal(nextValue("add", undefined, "2.5"), 2.5);
  assert.throws(() => nextValue("add", 1, "lots"), /number/);
  assert.equal(nextValue("set", 1, " 42 "), 42);
  assert.equal(nextValue("set", 1, "007 agent"), "007 agent");
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

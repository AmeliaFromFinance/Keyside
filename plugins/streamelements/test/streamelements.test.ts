import { createTestHost } from "@keyside/sdk/testing";
import { test } from "node:test";
import assert from "node:assert/strict";
import plugin, { describe, endpoints } from "../src/main.ts";
import { fakeServer, eventually } from "../../_lib/fake.ts";
import { fakeHttp } from "../../_lib/fakehttp.ts";

const CHANNEL = "5ad23dcc18fff500d78c5348";
const OLD_TIP = "65ce2539b1585c5f909a91e1";
const NEW_FOLLOW = "65ce2539b1585c5f909a91e2";

async function fakes(opts: { badToken?: boolean } = {}) {
  const api = await fakeHttp((h) => {
    if (h.headers.authorization !== "Bearer jwt-1") return { status: 401, json: { message: "Unauthorized" } };
    if (h.path === "/channels/me") return { json: { _id: CHANNEL, username: "fern", displayName: "Fern" } };
    if (h.path === `/activities/${CHANNEL}` && h.method === "GET") return { json: [{ _id: OLD_TIP, type: "tip", data: { username: "kit", amount: 5, currency: "EUR" } }] };
    if (h.method === "PUT" && h.path.startsWith(`/overlays/${CHANNEL}/action/`)) return { json: {} };
    if (h.method === "POST" && /^\/activities\/[0-9a-f]+\/[0-9a-f]+\/replay$/.test(h.path)) return { json: {} };
  });
  const astro = await fakeServer(
    (ws, m) => {
      if (m.type !== "subscribe") return;
      const error = opts.badToken || m.data.token !== "jwt-1" ? "err_unauthorized" : undefined;
      ws.send(JSON.stringify({ type: "response", nonce: m.nonce, error, data: { topic: m.data.topic, room: CHANNEL, message: error ? "nope" : "ok" } }));
    },
    (ws) => ws.send(JSON.stringify({ type: "welcome", data: { client_id: "c1" } })),
  );
  Object.assign(endpoints, { api: api.url, astro: `ws://127.0.0.1:${astro.port}` });
  return { api, astro };
}

const message = (topic: string, data: unknown) => ({ id: "01J6VTDB25SX14GDBDFZWDX3M9", ts: "2026-10-30T01:00:00Z", type: "message", topic, room: CHANNEL, data });

test("shows alerts in the feed and controls the overlay", async () => {
  const { api, astro } = await fakes();
  const h = createTestHost(plugin, { connection: { token: "jwt-1" } });
  await h.start();
  await h.sync([
    { id: "p", action: "pause" },
    { id: "r", action: "replay", settings: { which: "tip" } },
  ]);
  await eventually(() => astro.received.filter((m) => m.type === "subscribe").length === 2);
  assert.equal(h.status()?.state, "connected");
  assert.deepEqual(astro.received.map((m) => m.data.topic).sort(), ["channel.activities", "channel.overlay.action"]);

  astro.broadcast(message("channel.activities", { _id: NEW_FOLLOW, type: "follower", provider: "twitch", data: { username: "jtv", displayName: "JTV" } }));
  astro.broadcast(message("channel.activities", { _id: "65ce2539b1585c5f909a91e3", type: "tip", data: { username: "ana", amount: 4.2, currency: "USD", message: "gg" } }));
  await eventually(() => h.variable("last_tip") === "ana $4.20");
  assert.equal(h.variable("last_follower"), "JTV");
  const feed = h.sent.filter((s) => s.method === "feed.push").map((s) => `${s.params.item.author} ${s.params.item.text}`);
  assert.deepEqual(feed, ["JTV followed", "ana tipped $4.20: gg"]);

  await h.press("r", "replay", { which: "tip" });
  assert.equal(api.hits.at(-1)?.path, `/activities/${CHANNEL}/65ce2539b1585c5f909a91e3/replay`);

  await h.press("p", "pause");
  assert.equal(api.hits.at(-1)?.path, `/overlays/${CHANNEL}/action/pause`);
  assert.equal(api.hits.at(-1)?.method, "PUT");
  await eventually(() => h.view("p")?.badge === "PAUSED");
  // Resumed from the dashboard.
  astro.broadcast(message("channel.overlay.action", { action: "unpause" }));
  await eventually(() => h.view("p") === null);

  await h.press("s", "skip");
  assert.equal(api.hits.at(-1)?.path, `/overlays/${CHANNEL}/action/skip`);
  await h.stop();
  await astro.close();
  await api.close();
});

test("replays alerts from before Keyside started", async () => {
  const { api, astro } = await fakes();
  const h = createTestHost(plugin, { connection: { token: "jwt-1" } });
  await h.start();
  await eventually(() => h.status()?.state === "connected");
  await eventually(() => astro.received.length === 2);
  await h.press("r", "replay", { which: "last" });
  assert.equal(api.hits.at(-1)?.path, `/activities/${CHANNEL}/${OLD_TIP}/replay`);
  await h.stop();
  await astro.close();
  await api.close();
});

test("says when the token is wrong", async () => {
  const { api, astro } = await fakes();
  const h = createTestHost(plugin, { connection: { token: "old" } });
  await h.start();
  await eventually(() => h.status()?.state === "error");
  assert.match(String(h.status()?.detail), /JWT token/);
  await assert.rejects(h.press("s", "skip"), /isn't connected/);
  await h.stop();
  await astro.close();
  await api.close();
});

test("describes each kind of alert", () => {
  assert.deepEqual(describe({ type: "subscriber", data: { username: "a", amount: 3, message: "hi" } }), { who: "a", text: "subscribed for 3 months: hi" });
  assert.deepEqual(describe({ type: "subscriber", data: { username: "a", gifted: true, sender: "b" } }), { who: "b", text: "gifted a sub to a" });
  assert.deepEqual(describe({ type: "cheer", data: { username: "c", amount: 100 } }), { who: "c", text: "cheered 100 bits" });
  assert.deepEqual(describe({ type: "tip", data: { username: "d", amount: 1, currency: "nope" } }), { who: "d", text: "tipped 1 nope" });
  assert.equal(describe({}), null);
});

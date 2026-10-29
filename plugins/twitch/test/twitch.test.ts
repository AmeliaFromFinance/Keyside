import { createTestHost } from "@phonedeck/sdk/testing";
import { test } from "node:test";
import assert from "node:assert/strict";
import plugin, { endpoints } from "../src/main.ts";
import { fakeServer, eventually } from "../../_lib/fake.ts";
import { fakeHttp } from "../../_lib/fakehttp.ts";

async function fakeTwitch() {
  const state = { token: "good", refreshed: 0, devicePolls: 0 };
  const id = await fakeHttp((h) => {
    if (h.path === "/device") return { json: { device_code: "dev", user_code: "WXYZ-1234", verification_uri: "https://www.twitch.tv/activate", interval: 1, expires_in: 60 } };
    if (h.path === "/token" && h.body.grant_type === "refresh_token") {
      state.refreshed++;
      state.token = "fresh";
      return { json: { access_token: "fresh", refresh_token: "r2", expires_in: 3600 } };
    }
    if (h.path === "/token") {
      if (state.devicePolls++ === 0) return { status: 400, json: { status: 400, message: "authorization_pending" } };
      return { json: { access_token: "good", refresh_token: "r1", expires_in: 3600, scope: ["user:read:chat"] } };
    }
  });
  const api = await fakeHttp((h) => {
    if (h.headers.authorization !== `Bearer ${state.token}`) return { status: 401, json: { message: "Invalid OAuth token" } };
    switch (`${h.method} ${h.path}`) {
      case "GET /users":
        return { json: { data: [{ id: "42", login: "fern", display_name: "Fern" }] } };
      case "GET /streams":
        return { json: { data: [{ viewer_count: 17 }] } };
      case "GET /channels/followers":
        return { json: { total: 1200, data: [] } };
      case "GET /channels":
        return { json: { data: [{ title: "Cozy", game_name: "Just Chatting" }] } };
      case "GET /chat/settings":
        return { json: { data: [{ emote_mode: false, slow_mode: false }] } };
      case "PATCH /chat/settings":
        return { json: { data: [{ emote_mode: true }] } };
      default:
        return { json: { data: [{}] } };
    }
  });
  const es = await fakeServer(() => {}, (ws) => ws.send(JSON.stringify({ metadata: { message_type: "session_welcome" }, payload: { session: { id: "sess", keepalive_timeout_seconds: 30 } } })));
  Object.assign(endpoints, { id: id.url, api: api.url, eventsub: `ws://127.0.0.1:${es.port}` });
  return { state, id, api, es, close: () => Promise.all([id.close(), api.close(), es.close()]) };
}

const session = (access: string) => JSON.stringify({ access, refresh: "r1", expires: Date.now() + 3_600_000, client: "abc" });

test("loads the channel, subscribes to events and runs actions", async () => {
  const tw = await fakeTwitch();
  const h = createTestHost(plugin, { connection: { clientId: "abc", session: session("good") }, prefix: "twitch" });
  await h.start();
  await h.sync([
    { id: "m", action: "marker" },
    { id: "e", action: "chat-mode", settings: { mode: "emote", op: "toggle" } },
  ]);
  await eventually(() => h.status()?.state === "connected");
  // These load in parallel, so wait for each.
  await eventually(() => h.variable("viewers") === 17 && h.variable("followers") === 1200 && h.variable("title") === "Cozy");
  await eventually(() => tw.api.hits.filter((x) => x.path === "/eventsub/subscriptions").length >= 10);
  const sub = tw.api.hits.find((x) => x.body?.type === "channel.chat.message")!;
  assert.deepEqual(sub.body.transport, { method: "websocket", session_id: "sess" });
  assert.equal(sub.headers["client-id"], "abc");

  tw.es.broadcast({ metadata: { message_type: "notification", subscription_type: "channel.chat.message" }, payload: { event: { chatter_user_name: "Quiet", message: { text: "gg" }, color: "#00FF00" } } });
  await eventually(() => h.sent.some((s) => s.method === "feed.push" && s.params.item.text === "gg"));

  await h.press("e", "chat-mode", { mode: "emote", op: "toggle" });
  const patch = tw.api.hits.find((x) => x.method === "PATCH")!;
  assert.deepEqual(patch.body, { emote_mode: true });
  await eventually(() => h.view("e")?.badge === "ON");

  await h.press("m", "marker", { description: "great play" });
  assert.equal(tw.api.hits.find((x) => x.path === "/streams/markers")?.body.user_id, "42");

  await h.stop();
  await tw.close();
});

test("refreshes an expired token once and saves it", async () => {
  const tw = await fakeTwitch();
  const h = createTestHost(plugin, { connection: { clientId: "abc", session: session("stale") } });
  tw.state.token = "fresh";
  await h.start();
  await eventually(() => h.status()?.state === "connected");
  assert.equal(tw.state.refreshed, 1);
  const saved = h.sent.find((s) => s.method === "connection.update");
  assert.equal(JSON.parse(saved!.params.values.session).access, "fresh");
  await h.stop();
  await tw.close();
});

test("signs in with a device code", async () => {
  const tw = await fakeTwitch();
  const h = createTestHost(plugin, { connection: { clientId: "abc" } });
  await h.start();
  await eventually(() => /WXYZ-1234/.test(String(h.status()?.detail)));
  await eventually(() => h.status()?.state === "connected", 8000);
  assert.ok(tw.state.devicePolls >= 2);
  assert.ok(h.sent.some((s) => s.method === "browser.open"));
  await h.stop();
  await tw.close();
});

test("asks for a client id first", async () => {
  const h = createTestHost(plugin, { connection: {} });
  await h.start();
  assert.match(String(h.status()?.detail), /Client ID/);
  await assert.rejects(h.press("x", "clip"), /signed in/);
  await h.stop();
});

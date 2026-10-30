import { createTestHost } from "@keyside/sdk/testing";
import { test } from "node:test";
import assert from "node:assert/strict";
import plugin, { SCOPES, endpoints, roles } from "../src/main.ts";
import { fakeServer, eventually } from "../../_lib/fake.ts";
import { fakeHttp } from "../../_lib/fakehttp.ts";

async function fakeTwitch() {
  const state = { token: "good", refreshed: 0, devicePolls: 0, rewards: [] as object[] };
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
      case "GET /subscriptions":
        return { json: { total: 85, data: [] } };
      case "GET /channels":
        return { json: { data: [{ title: "Cozy", game_name: "Just Chatting" }] } };
      case "GET /chat/settings":
        return { json: { data: [{ emote_mode: false, slow_mode: false }] } };
      case "PATCH /chat/settings":
        return { json: { data: [{ emote_mode: true }] } };
      case "PUT /moderation/shield_mode":
        return { json: { data: [{ is_active: h.body.is_active }] } };
      case "GET /polls":
      case "GET /predictions":
        return { json: { data: [] } };
      case "POST /polls":
        return { json: { data: [{ id: "p1", title: h.body.title, duration: h.body.duration, started_at: new Date().toISOString(), choices: h.body.choices.map((c: any, i: number) => ({ id: `c${i}`, title: c.title, votes: 0 })) }] } };
      case "POST /predictions":
        return { json: { data: [{ id: "d1", title: h.body.title, prediction_window: h.body.prediction_window, created_at: new Date().toISOString(), outcomes: h.body.outcomes.map((o: any, i: number) => ({ id: `o${i}`, title: o.title })) }] } };
      case "GET /channel_points/custom_rewards":
        return { json: { data: state.rewards } };
      case "POST /channel_points/custom_rewards": {
        const r = { id: "r1", title: h.body.title, cost: h.body.cost, is_enabled: true, is_paused: false };
        state.rewards.push(r);
        return { json: { data: [r] } };
      }
      case "PATCH /channel_points/custom_rewards": {
        const r: any = state.rewards[0];
        Object.assign(r, h.body);
        return { json: { data: [r] } };
      }
      default:
        return { json: { data: [{}] } };
    }
  });
  const es = await fakeServer(() => {}, (ws) => ws.send(JSON.stringify({ metadata: { message_type: "session_welcome" }, payload: { session: { id: "sess", keepalive_timeout_seconds: 30 } } })));
  Object.assign(endpoints, { id: id.url, api: api.url, eventsub: `ws://127.0.0.1:${es.port}` });
  return { state, id, api, es, close: () => Promise.all([id.close(), api.close(), es.close()]) };
}

const session = (access: string, scope = SCOPES) => JSON.stringify({ access, refresh: "r1", expires: Date.now() + 3_600_000, client: "abc", scope });

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

test("shield mode, polls, predictions and Keyside's own rewards", async () => {
  const tw = await fakeTwitch();
  const h = createTestHost(plugin, { connection: { clientId: "abc", session: session("good") }, prefix: "twitch" });
  await h.start();
  await h.sync([
    { id: "s", action: "shield" },
    { id: "p", action: "poll" },
    { id: "d", action: "prediction", settings: { op: "resolve", winner: "2" } },
    { id: "r", action: "reward", settings: { title: "Hydrate" } },
  ]);
  await eventually(() => h.status()?.state === "connected");
  // Let the start-up reads finish, or they could overwrite what the presses below change.
  await eventually(() => h.variable("subs") === 85 && tw.api.hits.some((x) => x.path === "/moderation/shield_mode"));
  await h.settle();

  await h.press("s", "shield", { op: "toggle" });
  assert.equal(tw.api.hits.find((x) => x.method === "PUT")?.body.is_active, true);
  await eventually(() => h.view("s")?.badge === "SHIELD");

  await assert.rejects(h.press("p", "poll", { question: "Best snack?", choices: "Chips" }), /2 to 5 choices/);
  await h.press("p", "poll", { question: "Best snack?", choices: "Chips\nGrapes\n", seconds: 60 });
  assert.deepEqual(tw.api.hits.find((x) => x.method === "POST" && x.path === "/polls")?.body.choices, [{ title: "Chips" }, { title: "Grapes" }]);
  tw.es.broadcast({ metadata: { message_type: "notification", subscription_type: "channel.poll.progress" }, payload: { event: { id: "p1", title: "Best snack?", ends_at: new Date(Date.now() + 60_000).toISOString(), choices: [{ id: "c0", title: "Chips", votes: 2 }, { id: "c1", title: "Grapes", votes: 5 }] } } });
  await eventually(() => h.view("p")?.sub === "Grapes 5");
  await assert.rejects(h.press("p", "poll", { question: "Again?", choices: "a\nb" }), /already running/);

  await assert.rejects(h.press("d", "prediction", { op: "resolve", winner: "2" }), /No prediction/);
  await h.press("d", "prediction", { op: "start", question: "Win this round?", outcomes: "Yes\nNo" });
  await h.press("d", "prediction", { op: "resolve", winner: "2" });
  const resolve = tw.api.hits.find((x) => x.method === "PATCH" && x.path === "/predictions")!;
  assert.deepEqual([resolve.body.status, resolve.body.winning_outcome_id], ["RESOLVED", "o1"]);

  await h.press("r", "reward", { title: "Hydrate", cost: 100 });
  assert.equal(tw.api.hits.find((x) => x.method === "POST" && x.path === "/channel_points/custom_rewards")?.body.cost, 100);
  await eventually(() => h.view("r")?.badge === "ON");
  await h.press("r", "reward", { title: "Hydrate" });
  assert.deepEqual(tw.api.hits.find((x) => x.method === "PATCH" && x.path === "/channel_points/custom_rewards")?.body, { is_enabled: false });
  await eventually(() => h.view("r")?.badge === "OFF");
  await h.stop();
  await tw.close();
});

test("asks to sign in again when the saved sign-in lacks newer permissions", async () => {
  const tw = await fakeTwitch();
  const h = createTestHost(plugin, { connection: { clientId: "abc", session: session("good", "user:read:chat") } });
  await h.start();
  await eventually(() => /WXYZ-1234/.test(String(h.status()?.detail)));
  await h.stop();
  await tw.close();
});

test("tags chat with roles and passes channel point redemptions on", async () => {
  const tw = await fakeTwitch();
  const h = createTestHost(plugin, { connection: { clientId: "abc", session: session("good") }, prefix: "twitch" });
  await h.start();
  await eventually(() => h.variable("subs") === 85);
  await eventually(() => tw.api.hits.some((x) => x.body?.type === "channel.channel_points_custom_reward_redemption.add"));
  const note = (type: string, event: object) => tw.es.broadcast({ metadata: { message_type: "notification", subscription_type: type }, payload: { event } });

  note("channel.chat.message", { broadcaster_user_id: "42", chatter_user_id: "7", chatter_user_name: "Moss", message: { text: "!brb" }, badges: [{ set_id: "moderator" }, { set_id: "subscriber" }] });
  await eventually(() => h.feed("chat").length === 1);
  assert.deepEqual(h.feed("chat")[0], { author: "Moss", text: "!brb", kind: "chat", source: "twitch", roles: ["mod", "sub"] });

  note("channel.channel_points_custom_reward_redemption.add", { user_name: "Quiet", user_input: "with ice", reward: { title: "Hydrate", cost: 100 } });
  await eventually(() => h.feed("events").length === 1);
  assert.deepEqual(h.feed("events")[0], { author: "Quiet", text: "redeemed Hydrate", kind: "redeem", source: "twitch", reward: "Hydrate", input: "with ice" });
  await eventually(() => h.variable("last_redeem") === "Quiet: Hydrate");

  await h.stop();
  await tw.close();
});

test("works out roles from badges", () => {
  assert.deepEqual(roles({ badges: [{ set_id: "vip" }, { set_id: "founder" }] }), ["vip", "sub"]);
  assert.deepEqual(roles({ badges: [], chatter_user_id: "42", broadcaster_user_id: "42" }), ["broadcaster"]);
  assert.deepEqual(roles({}), []);
});

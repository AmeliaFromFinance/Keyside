import { createTestHost } from "@keyside/sdk/testing";
import { test } from "node:test";
import assert from "node:assert/strict";
import plugin, { cleanTitle, endpoints } from "../src/main.ts";
import { eventually } from "../../_lib/fake.ts";
import { fakeHttp } from "../../_lib/fakehttp.ts";

test("finds the live broadcast, reads new chat and sends messages", async () => {
  let chatCalls = 0;
  const api = await fakeHttp((h) => {
    if (h.headers.authorization !== "Bearer good") return { status: 401, json: { error: { message: "bad" } } };
    if (h.path === "/liveBroadcasts") return { json: { items: [{ id: "v1", snippet: { liveChatId: "c1", title: "Speedrun" } }] } };
    if (h.path === "/videos") return { json: { items: [{ liveStreamingDetails: { concurrentViewers: "33" }, statistics: { likeCount: "5" } }] } };
    if (h.path === "/liveChat/messages" && h.method === "GET") {
      chatCalls++;
      const items = [{ snippet: { type: "textMessageEvent", displayMessage: chatCalls === 1 ? "old" : "new" }, authorDetails: { displayName: "Ana" } }];
      return { json: { items, nextPageToken: `p${chatCalls}`, pollingIntervalMillis: 1 } };
    }
    if (h.path === "/liveChat/messages") return { json: {} };
  });
  Object.assign(endpoints, { api: api.url });
  const session = JSON.stringify({ access: "good", refresh: "r", expires: Date.now() + 3_600_000, client: "cid" });
  const h = createTestHost(plugin, { connection: { clientId: "cid", clientSecret: "s", session } });
  await h.start();
  await eventually(() => h.variable("viewers") === 33);
  assert.equal(h.variable("title"), "Speedrun");
  assert.equal(h.variable("live"), true);
  await h.press("k", "send", { message: "hello" });
  const post = api.hits.find((x) => x.method === "POST")!;
  assert.equal(post.body.snippet.textMessageDetails.messageText, "hello");
  assert.equal(chatCalls, 1);
  assert.ok(!h.sent.some((s) => s.method === "feed.push" && s.params.item.text === "old"), "history isn't replayed");
  await h.stop();
  await api.close();
});

test("changes the title and shows stream health", async () => {
  let health = "good";
  const snippet = { liveChatId: "c1", title: "Old", description: "Keep me", scheduledStartTime: "2026-10-30T00:00:00Z" };
  const api = await fakeHttp((h) => {
    if (h.path === "/liveBroadcasts" && h.method === "GET") return { json: { items: [{ id: "v1", snippet, contentDetails: { boundStreamId: "s1" } }] } };
    if (h.path === "/liveBroadcasts" && h.method === "PUT") return { json: h.body };
    if (h.path === "/liveStreams")
      return h.query.get("id") === "s1"
        ? { json: { items: [{ status: { streamStatus: "active", healthStatus: { status: health, configurationIssues: health === "bad" ? [{ severity: "error", reason: "Bitrate too low" }] : [] } } }] } }
        : { status: 404 };
    if (h.path === "/videos") return { json: { items: [] } };
    if (h.path === "/liveChat/messages") return { json: { items: [], pollingIntervalMillis: 60000 } };
  });
  Object.assign(endpoints, { api: api.url });
  const session = JSON.stringify({ access: "good", refresh: "r", expires: Date.now() + 3_600_000, client: "cid" });
  const h = createTestHost(plugin, { connection: { clientId: "cid", clientSecret: "s", session } });
  await h.start();
  await h.sync([{ id: "hk", action: "health" }]);
  await eventually(() => h.variable("health") === "Good");
  assert.equal(h.view("hk")?.badge, "GOOD");
  assert.equal(h.view("hk")?.sub, "Receiving video");

  health = "bad";
  await h.press("hk", "health");
  await eventually(() => h.view("hk")?.badge === "POOR");
  assert.equal(h.view("hk")?.sub, "Bitrate too low");

  await h.press("t", "title", { title: "  New   title " });
  const put = api.hits.find((x) => x.method === "PUT")!;
  assert.equal(put.query.get("part"), "snippet");
  assert.deepEqual(put.body, { id: "v1", snippet: { title: "New title", description: "Keep me", scheduledStartTime: "2026-10-30T00:00:00Z" } });
  await eventually(() => h.variable("title") === "New title");
  await assert.rejects(h.press("t", "title", { title: "<script>" }), /can't contain/);
  await h.stop();
  await api.close();
});

test("checks titles", () => {
  assert.throws(() => cleanTitle(""), /empty/);
  assert.throws(() => cleanTitle("x".repeat(101)), /100/);
});

test("needs a client id and secret", async () => {
  const h = createTestHost(plugin, { connection: { clientId: "x" } });
  await h.start();
  assert.match(String(h.status()?.detail), /secret/);
  await assert.rejects(h.press("k", "send", { message: "x" }), /signed in/);
  await h.stop();
});

import { createTestHost } from "@phonedeck/sdk/testing";
import { test } from "node:test";
import assert from "node:assert/strict";
import plugin, { endpoints } from "../src/main.ts";
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

test("needs a client id and secret", async () => {
  const h = createTestHost(plugin, { connection: { clientId: "x" } });
  await h.start();
  assert.match(String(h.status()?.detail), /secret/);
  await assert.rejects(h.press("k", "send", { message: "x" }), /signed in/);
  await h.stop();
});

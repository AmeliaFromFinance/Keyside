import { createTestHost } from "@keyside/sdk/testing";
import { test } from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import plugin, { endpoints, queueUri, toUri } from "../src/main.ts";
import { eventually } from "../../_lib/fake.ts";
import { fakeHttp } from "../../_lib/fakehttp.ts";

test("signs in with PKCE on first press, then controls playback", async () => {
  let challenge = "";
  const accounts = await fakeHttp((h) => {
    if (h.path === "/api/token" && h.body.grant_type === "authorization_code") {
      const ok = createHash("sha256").update(h.body.code_verifier).digest("base64url") === challenge && h.body.code === "c0de" && h.body.redirect_uri === "http://127.0.0.1:7442/oauth/callback";
      return ok ? { json: { access_token: "a", refresh_token: "r", expires_in: 3600 } } : { status: 400, json: { error: "invalid_grant" } };
    }
  });
  let playing = false;
  const api = await fakeHttp((h) => {
    if (h.headers.authorization !== "Bearer a") return { status: 401, json: { error: { message: "no" } } };
    if (h.path === "/me/player" && h.method === "GET")
      return { json: { is_playing: playing, item: { id: "t1", name: "Song", artists: [{ name: "Band" }], album: { images: [] } }, device: { volume_percent: 40 }, shuffle_state: false, repeat_state: "off" } };
    if (h.path === "/me/tracks/contains") return { json: [true] };
    if (h.path === "/me/player/play") playing = true;
    return { status: 204 };
  });
  Object.assign(endpoints, { accounts: accounts.url, api: api.url });
  const h = createTestHost(plugin, {
    connection: { clientId: "cid" },
    respond(s) {
      if (s.method !== "oauth.authorize") return undefined;
      challenge = new URL(s.params.url).searchParams.get("code_challenge") ?? "";
      return { code: "c0de", redirectUri: "http://127.0.0.1:7442/oauth/callback" };
    },
  });
  await h.start();
  await h.sync([
    { id: "pp", action: "play-pause" },
    { id: "np", action: "now-playing" },
    { id: "like", action: "like" },
  ]);
  await h.settle();
  assert.equal(h.view("pp")?.sub, "Press to sign in");
  await h.press("pp", "play-pause");
  assert.ok(challenge, "the authorize URL carries a PKCE challenge");
  await eventually(() => h.view("np")?.label === "Song");
  assert.equal(h.view("like")?.badge, "LIKED");
  assert.equal(h.variable("artist"), "Band");
  assert.ok(api.hits.some((x) => x.path === "/me/player/play" && x.method === "PUT"));
  await eventually(() => h.view("pp")?.icon === "pause");
  await h.stop();
  await Promise.all([accounts.close(), api.close()]);
});

test("queues songs and moves playback to another device", async () => {
  const api = await fakeHttp((h) => {
    if (h.path === "/me/player" && h.method === "GET") return { json: { is_playing: true, item: { id: "t1", name: "Song", artists: [], album: { images: [] } }, device: { name: "Desk PC" } } };
    if (h.path === "/me/player/devices") return { json: { devices: [{ id: "d1", name: "Desk PC" }, { id: "d2", name: "Kitchen" }, { id: "d3", name: "Locked", is_restricted: true }] } };
    if (h.path === "/me/tracks/contains") return { json: [false] };
    return { status: 204 };
  });
  Object.assign(endpoints, { api: api.url });
  const session = JSON.stringify({ client: "cid", access: "a", refresh: "r", expires: Date.now() + 3600_000 });
  const h = createTestHost(plugin, { connection: { clientId: "cid", session } });
  await h.start();
  await h.sync([
    { id: "k", action: "device", settings: { device: "Kitchen" } },
    { id: "d", action: "device", settings: { device: "desk pc" } },
  ]);
  await eventually(() => h.view("d")?.badge === "PLAYING");
  assert.equal(h.view("k"), null);
  assert.equal(h.variable("device"), "Desk PC");
  assert.deepEqual(await h.options("device", "device"), [
    { value: "Desk PC", label: "Desk PC" },
    { value: "Kitchen", label: "Kitchen" },
  ]);

  await h.press("k", "device", { device: "Kitchen" });
  const moved = api.hits.find((x) => x.path === "/me/player" && x.method === "PUT");
  assert.deepEqual(moved?.body, { device_ids: ["d2"], play: true });
  await assert.rejects(h.press("k", "device", { device: "Garage" }), /isn't available/);

  await h.press("q", "queue", { uri: "https://open.spotify.com/track/4uLU6hMCjMI75M1A2tKUQC" });
  const queued = api.hits.find((x) => x.path === "/me/player/queue");
  assert.equal(queued?.query.get("uri"), "spotify:track:4uLU6hMCjMI75M1A2tKUQC");
  await assert.rejects(h.press("q", "queue", { uri: "spotify:playlist:abc" }), /Only songs/);
  await h.stop();
  await api.close();
});

test("reads Spotify links and URIs only", () => {
  assert.equal(queueUri("spotify:episode:abc"), "spotify:episode:abc");
  assert.equal(toUri("https://open.spotify.com/playlist/37i9dQZF1DX?si=abc"), "spotify:playlist:37i9dQZF1DX");
  assert.equal(toUri("spotify:album:abc123"), "spotify:album:abc123");
  assert.throws(() => toUri("https://evil.example/playlist/x"));
});

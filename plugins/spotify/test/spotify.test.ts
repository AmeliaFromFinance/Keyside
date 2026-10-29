import { createTestHost } from "@phonedeck/sdk/testing";
import { test } from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import plugin, { endpoints, toUri } from "../src/main.ts";
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

test("reads Spotify links and URIs only", () => {
  assert.equal(toUri("https://open.spotify.com/playlist/37i9dQZF1DX?si=abc"), "spotify:playlist:37i9dQZF1DX");
  assert.equal(toUri("spotify:album:abc123"), "spotify:album:abc123");
  assert.throws(() => toUri("https://evil.example/playlist/x"));
});

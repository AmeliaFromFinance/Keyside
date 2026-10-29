import { createTestHost, type Harness } from "@keyside/sdk/testing";
import { test } from "node:test";
import assert from "node:assert/strict";
import plugin, { endpoints } from "../src/main.ts";
import { eventually } from "../../_lib/fake.ts";
import { fakeHttp } from "../../_lib/fakehttp.ts";

test("authorizes through the Discord app and mirrors mute state", async () => {
  const token = await fakeHttp((x) => (x.body.code === "auth-code" && x.body.client_secret === "shh" ? { json: { access_token: "acc", refresh_token: "ref", expires_in: 604800 } } : { status: 400, json: { error: "invalid_grant" } }));
  endpoints.token = `${token.url}/api/oauth2/token`;
  const sentToDiscord: any[] = [];
  let h: Harness;
  const reply = (payload: object) => setTimeout(() => void h.emit("ipc.message", { bridge: "discord", data: { op: 1, payload } }), 1);
  const voice = { mute: false, deaf: false, input: { volume: 50 }, output: { volume: 100 } };
  h = createTestHost(plugin, {
    connection: { clientId: "123", clientSecret: "shh" },
    respond(s) {
      if (s.method !== "ipc.send") return undefined;
      const { op, payload: p } = s.params.data;
      sentToDiscord.push(s.params.data);
      if (op === 0) reply({ cmd: "DISPATCH", evt: "READY", data: {} });
      else if (p.cmd === "AUTHORIZE") reply({ cmd: "AUTHORIZE", nonce: p.nonce, data: { code: "auth-code" } });
      else if (p.cmd === "AUTHENTICATE") reply({ cmd: "AUTHENTICATE", nonce: p.nonce, data: p.args.access_token === "acc" ? { user: { username: "fern" } } : {} });
      else if (p.cmd === "GET_VOICE_SETTINGS") reply({ cmd: p.cmd, nonce: p.nonce, data: voice });
      else if (p.cmd === "SET_VOICE_SETTINGS") reply({ cmd: p.cmd, nonce: p.nonce, data: Object.assign(voice, p.args) });
      else reply({ cmd: p.cmd, nonce: p.nonce, data: {} });
      return null;
    },
  });
  await h.start();
  await h.sync([
    { id: "m", action: "mute" },
    { id: "d", action: "deafen" },
    { id: "v", action: "input-volume" },
  ]);
  await eventually(() => h.status()?.state === "connected");
  assert.equal(h.status()?.detail, "fern");
  assert.equal(JSON.parse(h.sent.find((s) => s.method === "connection.update")!.params.values.session).access, "acc");
  await eventually(() => h.view("v")?.value === 0.5);

  await h.press("m", "mute", { mode: "toggle" });
  assert.equal(sentToDiscord.at(-1).payload.args.mute, true);
  await eventually(() => h.view("m")?.badge === "MUTED");

  await h.emit("ipc.message", { bridge: "discord", data: { op: 1, payload: { cmd: "DISPATCH", evt: "VOICE_SETTINGS_UPDATE", data: { mute: false, deaf: true } } } });
  await eventually(() => h.view("d")?.badge === "DEAF");
  assert.equal(h.variable("deafened"), true);
  await h.stop();
  await token.close();
});

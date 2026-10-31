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
  await eventually(() => sentToDiscord.some((m) => m.payload?.cmd === "SET_VOICE_SETTINGS" && m.payload.args.mute === true));
  await eventually(() => h.view("m")?.badge === "MUTED");

  await h.emit("ipc.message", { bridge: "discord", data: { op: 1, payload: { cmd: "DISPATCH", evt: "VOICE_SETTINGS_UPDATE", data: { mute: false, deaf: true } } } });
  await eventually(() => h.view("d")?.badge === "DEAF");
  assert.equal(h.variable("deafened"), true);
  await h.stop();
  await token.close();
});

test("switches push to talk through Discord", async () => {
  const token = await fakeHttp(() => ({ json: { access_token: "acc", refresh_token: "ref", expires_in: 604800 } }));
  endpoints.token = `${token.url}/api/oauth2/token`;
  const sets: any[] = [];
  let h: Harness;
  const reply = (payload: object) => setTimeout(() => void h.emit("ipc.message", { bridge: "discord", data: { op: 1, payload } }), 1);
  const voice: any = { mute: false, deaf: false, mode: { type: "VOICE_ACTIVITY" }, noise_suppression: true, echo_cancellation: true, automatic_gain_control: false };
  h = createTestHost(plugin, {
    connection: { clientId: "123", clientSecret: "shh" },
    respond(s) {
      if (s.method !== "ipc.send") return undefined;
      const { op, payload: p } = s.params.data;
      if (op === 0) reply({ cmd: "DISPATCH", evt: "READY", data: {} });
      else if (p.cmd === "AUTHORIZE") reply({ cmd: p.cmd, nonce: p.nonce, data: { code: "c" } });
      else if (p.cmd === "AUTHENTICATE") reply({ cmd: p.cmd, nonce: p.nonce, data: { user: { username: "fern" } } });
      else if (p.cmd === "GET_VOICE_SETTINGS") reply({ cmd: p.cmd, nonce: p.nonce, data: voice });
      else if (p.cmd === "SET_VOICE_SETTINGS") (sets.push(p.args), reply({ cmd: p.cmd, nonce: p.nonce, data: Object.assign(voice, p.args) }));
      else reply({ cmd: p.cmd, nonce: p.nonce, data: {} });
      return null;
    },
  });
  await h.start();
  await h.sync([
    { id: "p", action: "voice-mode" },
    { id: "n", action: "processing", settings: { which: "noise" } },
  ]);
  await eventually(() => h.view("p")?.badge === "VOICE");
  await eventually(() => h.view("n")?.badge === "ON");
  await h.press("n", "processing", { which: "noise", mode: "toggle" });
  assert.deepEqual(sets.at(-1), { noise_suppression: false });
  await eventually(() => h.view("n")?.badge === "OFF");
  await h.press("p", "voice-mode", { mode: "toggle" });
  assert.deepEqual(sets.at(-1), { mode: { type: "PUSH_TO_TALK" } });
  await eventually(() => h.view("p")?.badge === "PTT");
  await h.stop();
  await token.close();
});

test("says so when Equibop's Rich Presence answers instead of Discord", async () => {
  let h: Harness;
  h = createTestHost(plugin, {
    connection: { clientId: "123", clientSecret: "shh" },
    respond(s) {
      if (s.method === "ipc.send" && s.params.data.op === 0) {
        setTimeout(() => void h.emit("ipc.message", { bridge: "discord", data: { op: 1, payload: { cmd: "DISPATCH", evt: "READY", data: { user: { id: "1045800378228281345", username: "arrpc" } } } } }), 1);
      }
      return null;
    },
  });
  await h.start();
  await eventually(() => h.status()?.state === "error");
  assert.match(String(h.status()?.detail), /Equibop/);
  await h.stop();
});

test("controls Equibop with its toggle commands and falls back to Flatpak", async () => {
  const ran: string[][] = [];
  const h = createTestHost(plugin, {
    connection: { app: "equibop" },
    respond(s) {
      if (s.method !== "run") return undefined;
      ran.push(s.params.argv);
      if (s.params.argv[0] === "equibop") throw new Error("not installed");
      return { code: 0 };
    },
  });
  await h.start();
  await h.sync([
    { id: "m", action: "mute" },
    { id: "d", action: "deafen" },
    { id: "j", action: "join", settings: { channel: "123456789" } },
  ]);
  await eventually(() => h.status()?.state === "connected");
  await eventually(() => h.view("j")?.disabled === true && h.view("j")?.sub === "Discord app only");

  await h.press("m", "mute", { mode: "toggle" });
  assert.deepEqual(ran, [
    ["equibop", "--toggle-mic"],
    ["flatpak", "run", "io.github.equicord.equibop", "--toggle-mic"],
  ]);
  await eventually(() => h.view("m")?.badge === "MUTED");

  // Already muted: nothing to send.
  await h.press("m", "mute", { mode: "on" });
  assert.equal(ran.length, 2);

  await h.press("d", "deafen", { mode: "toggle" });
  assert.deepEqual(ran.at(-1), ["flatpak", "run", "io.github.equicord.equibop", "--toggle-deafen"]);
  await eventually(() => h.view("d")?.badge === "DEAF");
  await assert.rejects(h.press("j", "join", { channel: "123456789" }), /Discord app/);
  await h.stop();
});

test("reports a missing Equibop", async () => {
  const h = createTestHost(plugin, {
    connection: { app: "equibop" },
    respond(s) {
      if (s.method === "run") throw new Error("not installed");
      return undefined;
    },
  });
  await h.start();
  await h.sync([{ id: "m", action: "mute" }]);
  await assert.rejects(h.press("m", "mute", { mode: "toggle" }), /wasn't found/);
  assert.equal(h.status()?.state, "error");
  assert.notEqual(h.view("m")?.badge, "MUTED");
  await h.stop();
});

test("tells a crashing Equibop apart from a missing one", async () => {
  const h = createTestHost(plugin, {
    connection: { app: "equibop", install: "native" },
    respond(s) {
      if (s.method === "run") return { code: 127 };
      return undefined;
    },
  });
  await h.start();
  await h.sync([{ id: "m", action: "mute" }]);
  await assert.rejects(h.press("m", "mute", { mode: "toggle" }), /stopped with error 127/);
  await h.stop();
});

test("says where to authorize and what to do when it's cancelled", async () => {
  let h: Harness;
  const reply = (payload: object) => setTimeout(() => void h.emit("ipc.message", { bridge: "discord", data: { op: 1, payload } }), 1);
  h = createTestHost(plugin, {
    connection: { clientId: "123", clientSecret: "shh" },
    respond(s) {
      if (s.method !== "ipc.send") return undefined;
      const { op, payload: p } = s.params.data;
      if (op === 0) reply({ cmd: "DISPATCH", evt: "READY", data: {} });
      else if (p.cmd === "AUTHORIZE") reply({ cmd: p.cmd, evt: "ERROR", nonce: p.nonce, data: { code: 5000, message: "OAuth2 Error: access_denied" } });
      return null;
    },
  });
  await h.start();
  await eventually(() => h.status()?.state === "error");
  assert.ok(h.sent.some((s) => s.method === "toast" && /Authorize/.test(s.params.text)));
  assert.match(String(h.status()?.detail), /cancelled in Discord. Press Restart/);
  await h.stop();
});

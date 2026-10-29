import { createTestHost } from "@phonedeck/sdk/testing";
import { test } from "node:test";
import assert from "node:assert/strict";
import plugin from "../src/main.ts";
import { fakeServer, eventually } from "../../_lib/fake.ts";

function fakeVts(opts: { deny?: boolean } = {}) {
  const exp = { "smile.exp3.json": false };
  return fakeServer((ws, m) => {
    const reply = (messageType: string, data: object) => ws.send(JSON.stringify({ apiName: "VTubeStudioPublicAPI", requestID: m.requestID, messageType, data }));
    switch (m.messageType) {
      case "AuthenticationTokenRequest":
        return opts.deny ? reply("APIError", { errorID: 50, message: "User has denied API access" }) : reply("AuthenticationTokenResponse", { authenticationToken: "tok" });
      case "AuthenticationRequest":
        return reply("AuthenticationResponse", { authenticated: m.data.authenticationToken === "tok" });
      case "CurrentModelRequest":
        return reply("CurrentModelResponse", { modelLoaded: true, modelName: "Fern", modelID: "m1" });
      case "HotkeysInCurrentModelRequest":
        return reply("HotkeysInCurrentModelResponse", { availableHotkeys: [{ name: "Wave", hotkeyID: "h1" }] });
      case "ExpressionStateRequest":
        return reply("ExpressionStateResponse", { expressions: Object.entries(exp).map(([file, active]) => ({ file, name: "Smile", active })) });
      case "ExpressionActivationRequest":
        exp[m.data.expressionFile as "smile.exp3.json"] = m.data.active;
        return reply("ExpressionActivationResponse", {});
      default:
        return reply(m.messageType.replace("Request", "Response"), {});
    }
  });
}

test("asks for access once, keeps the token and lights up the model", async () => {
  const vts = await fakeVts();
  const h = createTestHost(plugin, { connection: { host: "127.0.0.1", port: vts.port } });
  await h.start();
  await h.sync([
    { id: "m", action: "model", settings: { model: "m1" } },
    { id: "e", action: "expression", settings: { expression: "smile.exp3.json", mode: "hold" } },
    { id: "x", action: "hotkey", settings: { hotkey: "gone" } },
  ]);
  await eventually(() => h.status()?.state === "connected");
  assert.equal(h.sent.find((s) => s.method === "connection.update")?.params.values.token, "tok");
  await eventually(() => h.view("m")?.badge === "LOADED");
  assert.equal(h.variable("model"), "Fern");
  assert.equal(h.view("x")?.disabled, true);

  await h.press("e", "expression", { expression: "smile.exp3.json", mode: "hold" });
  await eventually(() => h.view("e")?.badge === "ON");
  await h.release("e", "expression", { expression: "smile.exp3.json", mode: "hold" });
  await eventually(() => h.view("e") === null);
  await h.stop();
  await vts.close();
});

test("stops asking after access is denied", async () => {
  const vts = await fakeVts({ deny: true });
  const h = createTestHost(plugin, { connection: { host: "127.0.0.1", port: vts.port } });
  await h.start();
  await eventually(() => h.status()?.state === "error");
  assert.match(String(h.status()?.detail), /denied/);
  await h.stop();
  await vts.close();
});

import { createTestHost } from "@keyside/sdk/testing";
import { test } from "node:test";
import assert from "node:assert/strict";
import plugin from "../src/main.ts";
import { fakeServer, eventually } from "../../_lib/fake.ts";

function fakeVts(opts: { deny?: boolean; seen?: any[] } = {}) {
  const exp = { "smile.exp3.json": false };
  const scene: Array<{ fileName: string; instanceID: string }> = [];
  return fakeServer((ws, m) => {
    opts.seen?.push(m);
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
      case "ItemListRequest":
        return reply("ItemListResponse", { itemInstancesInScene: scene, availableItemFiles: [{ fileName: "hat.png" }, { fileName: "cat.gif" }] });
      case "ItemLoadRequest":
        scene.push({ fileName: m.data.fileName, instanceID: "i1" });
        return reply("ItemLoadResponse", { instanceID: "i1" });
      case "ItemUnloadRequest":
        scene.splice(0, scene.length, ...scene.filter((i) => !m.data.instanceIDs.includes(i.instanceID)));
        return reply("ItemUnloadResponse", {});
      case "InputParameterListRequest":
        return reply("InputParameterListResponse", { defaultParameters: [{ name: "FaceAngleZ", min: -30, max: 30, defaultValue: 0 }], customParameters: [] });
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

test("shows and hides items, and holds a parameter until it's slid back", async () => {
  const seen: any[] = [];
  const vts = await fakeVts({ seen });
  const h = createTestHost(plugin, { connection: { host: "127.0.0.1", port: vts.port, token: "tok" } });
  await h.start();
  await h.sync([
    { id: "i", action: "item", settings: { item: "hat.png" } },
    { id: "p", action: "parameter", settings: { parameter: "FaceAngleZ" } },
  ]);
  await eventually(() => h.status()?.state === "connected");
  assert.deepEqual(await h.options("item", "item"), [
    { value: "hat.png", label: "hat" },
    { value: "cat.gif", label: "cat" },
  ]);
  await h.press("i", "item", { item: "hat.png", x: 0.5, size: 0.2 });
  const load = seen.find((m) => m.messageType === "ItemLoadRequest");
  assert.deepEqual([load.data.fileName, load.data.positionX, load.data.size], ["hat.png", 0.5, 0.2]);
  await eventually(() => h.view("i")?.badge === "SHOWN");
  await h.press("i", "item", { item: "hat.png" });
  assert.deepEqual(seen.find((m) => m.messageType === "ItemUnloadRequest").data.instanceIDs, ["i1"]);
  await eventually(() => h.view("i") === null);

  await h.slide("p", "parameter", 0.75, { parameter: "FaceAngleZ" });
  const inject = () => seen.filter((m) => m.messageType === "InjectParameterDataRequest");
  assert.deepEqual(inject().at(-1).data.parameterValues, [{ id: "FaceAngleZ", value: 15 }]);
  await eventually(() => h.view("p")?.meta === "15.00");
  // Kept up so tracking doesn't take it back.
  const before = inject().length;
  await eventually(() => inject().length >= before + 2, 2000);
  await h.slide("p", "parameter", 0.5, { parameter: "FaceAngleZ" });
  await eventually(() => h.view("p")?.meta === "Tracking");
  const after = inject().length;
  await new Promise((r) => setTimeout(r, 1200));
  assert.equal(inject().length, after, "no more injections once handed back");
  await h.stop();
  await vts.close();
});

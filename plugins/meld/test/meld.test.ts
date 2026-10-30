import { createTestHost } from "@keyside/sdk/testing";
import { test } from "node:test";
import assert from "node:assert/strict";
import plugin from "../src/main.ts";
import { fakeServer, eventually } from "../../_lib/fake.ts";

const id = (n: number) => n.toString(16).toUpperCase().padStart(32, "0");
const [S1, S2, L1, E1, T1] = [1, 2, 3, 4, 5].map(id);

const METHODS = ["showScene", "setStagedScene", "showStagedScene", "toggleLayer", "toggleEffect", "toggleMute", "toggleMonitor", "setGain", "toggleRecord", "toggleStream", "sendCommand", "registerTrackObserver"];

/** Stands in for Meld: a Qt WebChannel server sharing one "meld" object. */
async function fakeMeld() {
  const state: any = {
    isStreaming: false,
    isRecording: false,
    session: {
      items: {
        [S1]: { type: "scene", index: 0, name: "Main", current: true, staged: false },
        [S2]: { type: "scene", index: 1, name: "BRB", current: false, staged: true },
        [L1]: { type: "layer", parent: S1, index: 0, name: "Webcam", visible: true },
        [E1]: { type: "effect", parent: L1, name: "Blur", enabled: false },
        [T1]: { type: "track", name: "Mic", muted: false, monitoring: false },
      },
    },
  };
  const props = ["isStreaming", "isRecording", "session"];
  const calls: Array<[string, unknown[]]> = [];
  let idle = false;
  let dirty = false;
  const fake = await fakeServer((ws, m) => {
    const flush = () => {
      if (!idle || !dirty) return;
      idle = dirty = false;
      ws.send(JSON.stringify({ type: 2, data: [{ object: "meld", signals: {}, properties: Object.fromEntries(props.map((p, i) => [i, state[p]])) }] }));
    };
    const it = (k: string) => state.session.items[k];
    switch (m.type) {
      case 3: {
        // Qt lists overloads too. The plain name comes first.
        const methods = METHODS.flatMap((name, i) => [[name, 100 + i], [`${name}(QString)`, 200 + i]]);
        const properties = props.map((p, i) => [i, p, [`${p}Changed`, 50 + i], state[p]]);
        ws.send(JSON.stringify({ type: 10, id: m.id, data: { meld: { methods, properties, signals: [["gainUpdated", 60]], enums: {} } } }));
        return;
      }
      case 4:
        idle = true;
        return flush();
      case 6: {
        const name = METHODS[m.method - 100];
        calls.push([name, m.args]);
        const [a, b, c] = m.args;
        if (name === "showScene") for (const [k, v] of Object.entries<any>(state.session.items)) if (v.type === "scene") v.current = k === a;
        if (name === "toggleRecord") state.isRecording = !state.isRecording;
        if (name === "toggleLayer") it(b).visible = !it(b).visible;
        if (name === "toggleEffect") it(c).enabled = !it(c).enabled;
        if (name === "toggleMute") it(a).muted = !it(a).muted;
        if (name === "registerTrackObserver") ws.send(JSON.stringify({ type: 1, object: "meld", signal: 60, args: [b, 0.25, false] }));
        if (name === "setGain") ws.send(JSON.stringify({ type: 1, object: "meld", signal: 60, args: [a, b, false] }));
        ws.send(JSON.stringify({ type: 10, id: m.id, data: null }));
        dirty = true;
        return flush();
      }
    }
  });
  return Object.assign(fake, { calls });
}

test("follows Meld and controls scenes, layers, effects and audio", async () => {
  const meld = await fakeMeld();
  const h = createTestHost(plugin, { connection: { host: "127.0.0.1", port: meld.port } });
  await h.start();
  await h.sync([
    { id: "s1", action: "scene", settings: { scene: S1 } },
    { id: "s2", action: "scene", settings: { scene: S2 } },
    { id: "l", action: "layer", settings: { layer: L1 } },
    { id: "e", action: "effect", settings: { effect: E1 } },
    { id: "m", action: "mute", settings: { track: T1 } },
    { id: "g", action: "gain", settings: { track: T1 } },
    { id: "r", action: "record" },
    { id: "x", action: "scene", settings: { scene: id(99) } },
  ]);
  await eventually(() => h.status()?.state === "connected");
  await eventually(() => h.view("s1")?.badge === "ON AIR");
  assert.equal(h.view("s2")?.badge, "STAGED");
  assert.equal(h.view("l")?.badge, "SHOWN");
  assert.equal(h.view("x")?.disabled, true);
  assert.equal(h.variable("scene"), "Main");
  await eventually(() => h.view("g")?.meta === "25%");

  assert.deepEqual(await h.options("layer", "layer"), [{ value: L1, label: "Main › Webcam" }]);
  assert.deepEqual(
    (await h.options("scene", "scene")).map((o) => o.label),
    ["Main", "BRB"],
  );

  await h.press("s2", "scene", { scene: S2 });
  await eventually(() => h.variable("scene") === "BRB");
  await eventually(() => h.view("s2")?.badge === "ON AIR");

  await h.press("l", "layer", { layer: L1, mode: "on" });
  assert.equal(meld.calls.filter(([n]) => n === "toggleLayer").length, 0, "already shown");
  await h.press("l", "layer", { layer: L1 });
  assert.deepEqual(meld.calls.at(-1), ["toggleLayer", [S1, L1]]);
  await eventually(() => h.view("l") === null);

  await h.press("e", "effect", { effect: E1 });
  assert.deepEqual(meld.calls.at(-1), ["toggleEffect", [S1, L1, E1]]);
  await eventually(() => h.view("e")?.badge === "ON");

  await h.press("m", "mute", { track: T1 });
  await eventually(() => h.view("m")?.badge === "MUTED");

  await h.slide("g", "gain", 0.8, { track: T1 });
  await eventually(() => h.view("g")?.meta === "80%");

  await h.press("r", "record", { mode: "on" });
  await eventually(() => h.view("r")?.badge === "REC");
  assert.equal(h.variable("recording"), true);
  await h.stop();
  await meld.close();
});

test("only acts on things in the session", async () => {
  const meld = await fakeMeld();
  const h = createTestHost(plugin, { connection: { host: "127.0.0.1", port: meld.port } });
  await h.start();
  await eventually(() => h.status()?.state === "connected");
  const before = meld.calls.length;
  await assert.rejects(h.press("a", "scene", { scene: L1 }), /scene isn't/);
  await assert.rejects(h.press("a", "layer", { layer: "__proto__" }), /layer isn't/);
  await assert.rejects(h.press("a", "command", { command: "constructor" }), /Unknown command/);
  assert.equal(meld.calls.length, before);
  await h.press("a", "command", { command: "clip" });
  assert.deepEqual(meld.calls.at(-1), ["sendCommand", ["meld.recordClip"]]);
  await h.stop();
  await meld.close();
});

import { createTestHost } from "@keyside/sdk/testing";
import { test } from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import plugin from "../src/main.ts";
import { fakeServer, eventually } from "../../_lib/fake.ts";

const sha = (s: string) => createHash("sha256").update(s).digest("base64");

function fakeObs(password: string) {
  const state = { program: "Live", preview: "", muted: true, recording: false, monitor: "OBS_MONITORING_TYPE_NONE", streaming: false, bytes: 0 };
  const answer = (requestType: string, d: any): any => {
    switch (requestType) {
      case "GetSceneList":
        return { currentProgramSceneName: state.program, currentPreviewSceneName: null, scenes: [{ sceneName: "Live" }, { sceneName: "BRB" }] };
      case "GetRecordStatus":
        return { outputActive: state.recording, outputPaused: false, outputDuration: 0 };
      case "GetInputList":
        return { inputs: [{ inputName: "Mic" }, { inputName: "Music" }] };
      case "GetInputMute":
        return { inputMuted: d.inputName === "Mic" ? state.muted : false };
      case "GetInputVolume":
        return { inputVolumeMul: 0.125, inputVolumeDb: -18 };
      case "GetReplayBufferStatus":
        return null;
      case "GetRecordDirectory":
        return { recordDirectory: "/home/fern/Videos/" };
      case "GetInputAudioMonitorType":
        return { monitorType: state.monitor };
      case "SetInputAudioMonitorType":
        state.monitor = d.monitorType;
        return {};
      case "GetStats":
        return { activeFps: 60, cpuUsage: 12.5, renderSkippedFrames: 0, renderTotalFrames: 1000, outputSkippedFrames: 30, outputTotalFrames: 1000 };
      case "GetStreamStatus":
        // About 1000 kb/s at one poll every 2 seconds.
        return state.streaming ? { outputActive: true, outputDuration: 65_000, outputBytes: (state.bytes += 250_000), outputSkippedFrames: 0, outputTotalFrames: 100 } : { outputActive: false };
      case "SetInputMute":
      case "SetCurrentProgramScene":
      case "ToggleRecord":
        return {};
      default:
        return {};
    }
  };
  return fakeServer(
    (ws, m) => {
      if (m.op === 1) {
        if (m.d.authentication !== sha(sha(password + "salt") + "challenge")) return ws.close(4009, "Authentication failed.");
        return ws.send(JSON.stringify({ op: 2, d: { negotiatedRpcVersion: 1 } }));
      }
      if (m.op === 6) {
        const r = answer(m.d.requestType, m.d.requestData ?? {});
        ws.send(JSON.stringify({ op: 7, d: { requestType: m.d.requestType, requestId: m.d.requestId, requestStatus: { result: r !== null, code: r ? 100 : 604 }, responseData: r ?? undefined } }));
      }
      if (m.op === 8) {
        const results = m.d.requests.map((q: any) => {
          const r = answer(q.requestType, q.requestData ?? {});
          return { requestType: q.requestType, requestStatus: { result: r !== null, code: r ? 100 : 604 }, responseData: r ?? undefined };
        });
        ws.send(JSON.stringify({ op: 9, d: { requestId: m.d.requestId, results } }));
      }
    },
    (ws) => ws.send(JSON.stringify({ op: 0, d: { obsWebSocketVersion: "5.5.0", rpcVersion: 1, authentication: { challenge: "challenge", salt: "salt" } } })),
  ).then((f) => ({ ...f, state }));
}

test("signs in, mirrors OBS state on keys and sends presses", async () => {
  const obs = await fakeObs("hunter2");
  const h = createTestHost(plugin, { connection: { host: "127.0.0.1", port: obs.port, password: "hunter2" }, prefix: "obs" });
  await h.start();
  await h.sync([
    { id: "live", action: "scene", settings: { scene: "Live" } },
    { id: "brb", action: "scene", settings: { scene: "BRB" } },
    { id: "gone", action: "scene", settings: { scene: "Deleted" } },
    { id: "mic", action: "mute", settings: { input: "Mic", mode: "toggle" } },
    { id: "vol", action: "volume", settings: { input: "Music" } },
    { id: "rec", action: "record", settings: { mode: "toggle" } },
  ]);
  await eventually(() => h.status()?.state === "connected");
  await eventually(() => (h.view("mic") as any)?.badge === "MUTED");
  assert.equal(h.view("live")?.badge, "ON AIR");
  assert.equal(h.view("gone")?.disabled, true);
  assert.equal(h.view("mic")?.icon, "mic-off");
  assert.equal(h.view("vol")?.value, 0.5);
  assert.equal(h.variable("scene"), "Live");

  await h.press("brb", "scene", { scene: "BRB" });
  const sent = await obs.until((m) => m.op === 6 && m.d.requestType === "SetCurrentProgramScene");
  assert.equal(sent.d.requestData.sceneName, "BRB");
  obs.broadcast({ op: 5, d: { eventType: "CurrentProgramSceneChanged", eventData: { sceneName: "BRB" } } });
  await eventually(() => h.view("brb")?.badge === "ON AIR");
  assert.equal(h.view("live"), null, "the old scene goes back to the layout look");

  await h.press("mic", "mute", { input: "Mic", mode: "toggle" });
  const mute = await obs.until((m) => m.op === 6 && m.d.requestType === "SetInputMute");
  assert.equal(mute.d.requestData.inputMuted, false, "toggle unmutes a muted input");

  obs.broadcast({ op: 5, d: { eventType: "RecordStateChanged", eventData: { outputActive: true, outputState: "OBS_WEBSOCKET_OUTPUT_STARTED" } } });
  await eventually(() => h.view("rec")?.label === "Stop recording");
  assert.equal(h.view("rec")?.tone, "danger");
  await eventually(() => (h.status()?.pills as any[])?.[0]?.label === "REC");

  await h.slide("vol", "volume", 1, { input: "Music" });
  const vol = await obs.until((m) => m.op === 6 && m.d.requestType === "SetInputVolume");
  assert.equal(vol.d.requestData.inputVolumeMul, 1);

  await h.stop();
  await obs.close();
});

test("multi-step keys go to OBS as one batch", async () => {
  const obs = await fakeObs("pw");
  const h = createTestHost(plugin, { connection: { host: "127.0.0.1", port: obs.port, password: "pw" } });
  await h.start();
  await h.sync([{ id: "all", action: "mute", settings: { input: "Mic", mode: "on" } }]);
  await eventually(() => h.status()?.state === "connected");
  await h.run([
    { instance: "all", action: "mute", settings: { input: "Mic", mode: "on" } },
    { instance: "all", action: "mute", settings: { input: "Music", mode: "on" } },
  ]);
  const b = await obs.until((m) => m.op === 8 && m.d.requests.some((r: any) => r.requestType === "SetInputMute"));
  assert.deepEqual(
    b.d.requests.map((r: any) => [r.requestData.inputName, r.requestData.inputMuted]),
    [["Mic", true], ["Music", true]],
  );
  await h.stop();
  await obs.close();
});

test("a wrong password stops retrying and says why", async () => {
  const obs = await fakeObs("right");
  const h = createTestHost(plugin, { connection: { host: "127.0.0.1", port: obs.port, password: "wrong" } });
  await h.start();
  await eventually(() => h.status()?.state === "error");
  assert.match(String(h.status()?.detail), /password/i);
  const hellos = obs.received.filter((m) => m.op === 1).length;
  await new Promise((r) => setTimeout(r, 1500));
  assert.equal(obs.received.filter((m) => m.op === 1).length, hellos);
  await h.stop();
  await obs.close();
});

test("screenshots, audio monitoring and stream health", async () => {
  const obs = await fakeObs("pw");
  const h = createTestHost(plugin, { connection: { host: "127.0.0.1", port: obs.port, password: "pw" }, prefix: "obs" });
  await h.start();
  await h.sync([
    { id: "shot", action: "screenshot" },
    { id: "mon", action: "monitor", settings: { input: "Music" } },
    { id: "health", action: "stats" },
    { id: "split", action: "split" },
  ]);
  await eventually(() => h.status()?.state === "connected");
  await eventually(() => h.view("mon")?.badge === "OFF");
  assert.equal(h.view("split")?.disabled, true, "split needs a recording");

  await h.press("shot", "screenshot", {});
  const shot = await obs.until((m) => m.op === 6 && m.d.requestType === "SaveSourceScreenshot");
  assert.equal(shot.d.requestData.sourceName, "Live");
  assert.match(shot.d.requestData.imageFilePath, /^\/home\/fern\/Videos\/Screenshot Live \d{4}-\d\d-\d\d \d\d-\d\d-\d\d\.png$/);

  await h.press("mon", "monitor", { input: "Music", mode: "toggle" });
  const mon = await obs.until((m) => m.op === 6 && m.d.requestType === "SetInputAudioMonitorType");
  assert.equal(mon.d.requestData.monitorType, "OBS_MONITORING_TYPE_MONITOR_AND_OUTPUT");
  obs.broadcast({ op: 5, d: { eventType: "InputAudioMonitorTypeChanged", eventData: { inputName: "Music", monitorType: "OBS_MONITORING_TYPE_MONITOR_AND_OUTPUT" } } });
  await eventually(() => h.view("mon")?.badge === "ON");

  // 30 of 1000 encoded frames skipped: 3%.
  await eventually(() => h.view("health")?.badge === "DROPPING", 5000);
  assert.equal(h.view("health")?.sub, "Encoding 3.0% dropped");
  await h.stop();
  await obs.close();
});

test("stream dashboard figures: bitrate, time live and variables", async () => {
  const obs = await fakeObs("pw");
  obs.state.streaming = true;
  const h = createTestHost(plugin, { connection: { host: "127.0.0.1", port: obs.port, password: "pw" }, prefix: "obs" });
  await h.start();
  await h.sync([
    { id: "rate", action: "stats", settings: { show: "bitrate" } },
    { id: "live", action: "stats", settings: { show: "live" } },
    { id: "rec", action: "stats", settings: { show: "record" } },
  ]);
  await eventually(() => h.status()?.state === "connected");
  await eventually(() => h.view("live")?.tone === "danger");
  const since = Number(h.view("live")?.since);
  assert.ok(Math.abs(Date.now() - 65_000 - since) < 3000, "counts from when the stream started");
  assert.equal(h.view("rec")?.sub, "Not recording");
  await eventually(() => /kb\/s$/.test(String(h.view("rate")?.sub)), 9000);
  const kbps = Number(h.variable("bitrate"));
  assert.ok(kbps > 700 && kbps < 1400, `bitrate ${kbps}`);
  assert.equal(h.variable("cpu"), 12.5);
  assert.equal(h.variable("fps"), 60);
  assert.equal(h.variable("dropped"), 3);
  // Pressing a figure does nothing.
  await h.press("rate", "stats", { show: "bitrate" });
  await h.stop();
  await obs.close();
});

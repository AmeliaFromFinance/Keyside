// Streamlabs Desktop remote control API: JSON-RPC 2.0 over the raw WebSocket of its SockJS server.

import { definePlugin, setting, type Host, type Key, type KeyView, type Pill } from "@phonedeck/sdk";
import { Reconnecting, Waiters } from "../../_lib/socket.ts";
import { clamp01, want } from "../../_lib/util.ts";

let host: Host;
let sock: Reconnecting;
const waiting = new Waiters<any>();
let nextId = 1;
let ready = false;
let audioTimer: ReturnType<typeof setTimeout> | null = null;

const st = {
  scenes: [] as Array<{ id: string; name: string }>,
  active: "",
  audio: new Map<string, { id: string; name: string; muted: boolean; level: number }>(),
  streaming: "offline",
  streamingSince: 0,
  recording: "offline",
  recordingSince: 0,
  replay: "offline",
};

function call(resource: string, method: string, args: unknown[] = []): Promise<any> {
  const id = nextId++;
  const p = waiting.wait(String(id));
  sock.send(JSON.stringify({ jsonrpc: "2.0", id, method, params: { resource, args } }));
  return p;
}

function onMessage(raw: string) {
  for (const line of raw.split("\n")) {
    if (!line.trim()) continue;
    const m = JSON.parse(line);
    if (m.id !== null && m.id !== undefined) {
      if (m.error) waiting.reject(String(m.id), new Error(m.error.message ?? "Streamlabs refused the request"));
      else waiting.resolve(String(m.id), m.result);
    } else if (m.result?._type === "EVENT") onEvent(String(m.result.resourceId), m.result.data);
  }
}

function onEvent(id: string, data: any) {
  switch (id) {
    case "ScenesService.sceneSwitched":
      st.active = data?.id ?? st.active;
      void loadAudio();
      break;
    case "ScenesService.sceneAdded":
    case "ScenesService.sceneRemoved":
      void loadScenes();
      return;
    case "StreamingService.streamingStatusChange":
      st.streamingSince = data === "live" && st.streaming !== "live" ? Date.now() : st.streamingSince;
      st.streaming = String(data);
      break;
    case "StreamingService.recordingStatusChange":
      st.recordingSince = data === "recording" && st.recording !== "recording" ? Date.now() : st.recordingSince;
      st.recording = String(data);
      break;
    case "StreamingService.replayBufferStatusChange":
      st.replay = String(data);
      break;
    case "SourcesService.sourceUpdated":
      // Mute and volume changes arrive here; fetch the audio list once things settle.
      audioTimer ??= setTimeout(() => ((audioTimer = null), void loadAudio()), 150);
      return;
    default:
      return;
  }
  render();
}

async function loadScenes() {
  const [scenes, active] = await Promise.all([call("ScenesService", "getScenes"), call("ScenesService", "activeSceneId")]);
  st.scenes = (scenes ?? []).map((s: any) => ({ id: s.id, name: s.name }));
  st.active = active ?? "";
  render();
}

async function loadAudio() {
  const list = await call("AudioService", "getSources").catch(() => []);
  st.audio.clear();
  for (const a of list ?? []) st.audio.set(a.name, { id: a.sourceId, name: a.name, muted: !!a.muted, level: Number(a.fader?.deflection ?? 0) });
  render();
}

async function sync() {
  const model = await call("StreamingService", "getModel");
  const since = (t: unknown) => (t ? Date.parse(String(t)) || Date.now() : Date.now());
  st.streaming = model?.streamingStatus ?? "offline";
  st.recording = model?.recordingStatus ?? "offline";
  st.replay = model?.replayBufferStatus ?? "offline";
  st.streamingSince = st.streaming === "live" ? since(model?.streamingStatusTime) : 0;
  st.recordingSince = st.recording === "recording" ? since(model?.recordingStatusTime) : 0;
  await Promise.all([loadScenes(), loadAudio()]);
  for (const [res, ev] of [
    ["ScenesService", "sceneSwitched"],
    ["ScenesService", "sceneAdded"],
    ["ScenesService", "sceneRemoved"],
    ["StreamingService", "streamingStatusChange"],
    ["StreamingService", "recordingStatusChange"],
    ["StreamingService", "replayBufferStatusChange"],
    ["SourcesService", "sourceUpdated"],
  ]) {
    await call(res, ev).catch(() => {});
  }
}

const recording = () => st.recording === "recording";
const live = () => st.streaming === "live" || st.streaming === "starting";

function pills(): Pill[] {
  const out: Pill[] = [];
  if (recording()) out.push({ label: "REC", tone: "danger", since: st.recordingSince });
  if (st.streaming === "live") out.push({ label: "LIVE", tone: "danger", since: st.streamingSince });
  return out;
}

function view(k: Key): KeyView | null {
  if (!ready) return { disabled: true };
  const s = k.settings;
  switch (k.action) {
    case "scene": {
      const scene = st.scenes.find((x) => x.name === s.scene || x.id === s.scene);
      if (!scene) return { disabled: true, sub: "Not in Streamlabs" };
      return scene.id === st.active ? { tone: "accent", badge: "ON AIR", badgeTone: "accent" } : null;
    }
    case "mute":
    case "volume": {
      const a = st.audio.get(String(s.source ?? ""));
      if (!a) return { disabled: true, sub: "Not in Streamlabs" };
      const v: KeyView = k.action === "volume" ? { value: clamp01(a.level) } : {};
      if (a.muted) Object.assign(v, { tone: "danger", badge: "MUTED", badgeTone: "danger", icon: k.action === "mute" && (k.icon ?? "mic") === "mic" ? "mic-off" : undefined });
      else if (k.action === "mute") Object.assign(v, { badge: "ON", badgeTone: "success" });
      return v;
    }
    case "record":
      return recording() ? { label: "Stop recording", sub: "Recording", tone: "danger", since: st.recordingSince, icon: "circle-stop" } : null;
    case "stream":
      return live() ? { label: "End stream", badge: "LIVE", badgeTone: "danger", tone: "danger", since: st.streamingSince } : null;
    case "replay":
      if ((s.mode ?? "save") === "save") return st.replay === "running" ? { sub: "Buffer on" } : { disabled: true, sub: "Buffer off" };
      return st.replay === "running" ? { tone: "accent", badge: "RUNNING", badgeTone: "info" } : null;
    default:
      return null;
  }
}

let lastPills = "";
function render() {
  for (const k of host.keys()) {
    const v = view(k);
    if (v) k.set(v);
    else k.clear();
  }
  if (!ready) return;
  host.setVar("scene", st.scenes.find((s) => s.id === st.active)?.name ?? "");
  host.setVar("recording", recording());
  host.setVar("streaming", live());
  const p = JSON.stringify(pills());
  if (p !== lastPills) {
    lastPills = p;
    host.status("connected", undefined, pills());
  }
}

function connect() {
  sock?.stop();
  ready = false;
  sock = new Reconnecting(
    () => {
      const h = String(setting(host.connection, "host", "127.0.0.1")).trim();
      return h ? `ws://${h.includes(":") && !h.startsWith("[") ? `[${h}]` : h}:${Number(setting(host.connection, "port", 59650))}/api/websocket` : null;
    },
    {
      async open() {
        const token = String(host.connection.token ?? "");
        if (!token) {
          host.status("error", "Add the Streamlabs API token in the plugin settings.");
          sock.stop();
          return;
        }
        try {
          await call("TcpServerService", "auth", [token]);
        } catch {
          host.status("error", "Streamlabs didn't accept the API token.");
          sock.stop();
          return;
        }
        ready = true;
        sock.healthy();
        lastPills = "";
        await sync();
        render();
      },
      message: onMessage,
      close(reason) {
        ready = false;
        waiting.failAll(new Error("Streamlabs disconnected"));
        host.status("offline", `Can't reach Streamlabs Desktop (${reason}). Turn on Remote Control in its settings.`, null);
        render();
      },
    },
  );
  sock.start();
}

function connected() {
  if (!ready) throw new Error("Streamlabs isn't connected");
}

function audio(name: unknown) {
  const a = st.audio.get(String(name ?? ""));
  if (!a) throw new Error(`No audio source called "${name}"`);
  return a;
}

const list = (names: string[]) => names.map((n) => ({ value: n, label: n }));

export default definePlugin({
  start(h) {
    host = h;
    connect();
  },
  connectionChanged: connect,
  stop: () => sock?.stop(),
  actions: {
    scene: {
      async press({ settings }) {
        connected();
        const scene = st.scenes.find((x) => x.name === settings.scene || x.id === settings.scene);
        if (!scene) throw new Error(`No scene called "${settings.scene}"`);
        await call("ScenesService", "makeSceneActive", [scene.id]);
      },
      options: async () => (connected(), await loadScenes(), list(st.scenes.map((s) => s.name))),
      appear: render,
    },
    mute: {
      async press({ settings }) {
        connected();
        const a = audio(settings.source);
        const muted = want(settings.mode ?? "toggle", a.muted);
        await call(`AudioSource["${a.id}"]`, "setMuted", [muted]);
        a.muted = muted;
        render();
      },
      options: async () => (connected(), await loadAudio(), list([...st.audio.keys()])),
      appear: render,
    },
    volume: {
      async slide({ settings, value }) {
        connected();
        const a = audio(settings.source);
        a.level = clamp01(value);
        await call(`AudioSource["${a.id}"]`, "setDeflection", [a.level]);
      },
      options: async () => (connected(), await loadAudio(), list([...st.audio.keys()])),
      appear: render,
    },
    record: { press: async () => (connected(), await call("StreamingService", "toggleRecording")), appear: render },
    stream: { press: async () => (connected(), await call("StreamingService", "toggleStreaming")), appear: render },
    replay: {
      async press({ settings }) {
        connected();
        if ((settings.mode ?? "save") === "save") {
          if (st.replay !== "running") throw new Error("Start the replay buffer first");
          await call("StreamingService", "saveReplay");
          host.toast("Replay saved", "success");
        } else await call("StreamingService", st.replay === "running" ? "stopReplayBuffer" : "startReplayBuffer");
      },
      appear: render,
    },
  },
});

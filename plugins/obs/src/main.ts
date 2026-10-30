// OBS Studio over obs-websocket 5 (built into OBS 28 and later).

import { definePlugin, setting, type Host, type Key, type KeyView, type Pill, type Step } from "@keyside/sdk";
import { Reconnecting, Waiters } from "../../_lib/socket.ts";
import { b64, clamp01, randomId, sha256, want } from "../../_lib/util.ts";

const OP = { Hello: 0, Identify: 1, Identified: 2, Event: 5, Request: 6, Response: 7, Batch: 8, BatchResponse: 9 } as const;
// General, config, scenes, inputs, transitions, filters, outputs, scene items, media, UI.
const EVENTS = 1 | 2 | 4 | 8 | 16 | 32 | 64 | 128 | 256 | 1024;
const AUTH_FAILED = 4009;
const THUMB_EVERY = 5000;
const STATS_EVERY = 2000;

interface Req {
  requestType: string;
  requestData?: Record<string, unknown>;
}

const st = {
  ready: false,
  version: "",
  program: "",
  preview: "",
  studio: false,
  scenes: [] as string[],
  inputs: new Set<string>(),
  muted: new Map<string, boolean>(),
  volume: new Map<string, number>(),
  record: { active: false, paused: false, since: 0 },
  stream: { active: false, since: 0 },
  replay: null as boolean | null,
  vcam: false,
  items: new Map<string, { id: number; enabled: boolean }>(),
  filters: new Map<string, boolean>(),
  media: new Map<string, string>(),
  collection: "",
  profile: "",
  transition: "",
  thumbs: new Map<string, string>(),
  monitor: new Map<string, string>(),
  recordDir: "",
  stats: null as null | { fps: number; cpu: number; render: number; encode: number; network: number; kbps: number | null },
};

const MONITOR_OFF = "OBS_MONITORING_TYPE_NONE";
const MONITOR_ON = "OBS_MONITORING_TYPE_MONITOR_AND_OUTPUT";

let host: Host;
let sock: Reconnecting;
let authFailed = false;
const waiting = new Waiters<any>();
let thumbTimer: ReturnType<typeof setInterval> | null = null;
let statsTimer: ReturnType<typeof setInterval> | null = null;

const pair = (a: string, b: string) => `${a}\u0000${b}`;

function url(): string | null {
  const h = String(setting(host.connection, "host", "127.0.0.1")).trim();
  const port = Number(setting(host.connection, "port", 4455));
  if (!h || authFailed) return null;
  return `ws://${h.includes(":") && !h.startsWith("[") ? `[${h}]` : h}:${port}`;
}

function request(requestType: string, requestData?: Record<string, unknown>): Promise<any> {
  const requestId = randomId();
  return waiting.request(requestId, () => sock.send(JSON.stringify({ op: OP.Request, d: { requestType, requestId, requestData } })));
}

/** Runs requests in order as one message. Resolves with each result (failed ones are null). */
function batch(requests: Req[]): Promise<Array<any | null>> {
  if (!requests.length) return Promise.resolve([]);
  const requestId = randomId();
  return waiting.request(requestId, () => sock.send(JSON.stringify({ op: OP.Batch, d: { requestId, haltOnFailure: false, executionType: 0, requests } })));
}

function onMessage(text: string) {
  const m = JSON.parse(text);
  switch (m.op) {
    case OP.Hello:
      void identify(m.d);
      break;
    case OP.Identified:
      sock.healthy();
      void sync();
      break;
    case OP.Response: {
      const s = m.d.requestStatus;
      if (s?.result) waiting.resolve(m.d.requestId, m.d.responseData ?? {});
      else waiting.reject(m.d.requestId, new Error(s?.comment || `OBS refused ${m.d.requestType}`));
      break;
    }
    case OP.BatchResponse:
      waiting.resolve(
        m.d.requestId,
        (m.d.results ?? []).map((r: any) => (r.requestStatus?.result ? (r.responseData ?? {}) : null)),
      );
      break;
    case OP.Event:
      onEvent(m.d.eventType, m.d.eventData ?? {});
      break;
  }
}

async function identify(hello: any) {
  st.version = hello.obsWebSocketVersion ?? "";
  const d: Record<string, unknown> = { rpcVersion: 1, eventSubscriptions: EVENTS };
  if (hello.authentication) {
    const password = String(host.connection.password ?? "");
    if (!password) {
      host.status("error", "OBS asks for a password. Add it in the OBS plugin settings.");
      authFailed = true;
      sock.stop();
      return;
    }
    const secret = b64(await sha256(password + hello.authentication.salt));
    d.authentication = b64(await sha256(secret + hello.authentication.challenge));
  }
  sock.send(JSON.stringify({ op: OP.Identify, d }));
}

/** obs-websocket lists scenes bottom-up. Put them in the order OBS shows. */
function sceneNames(list: unknown): string[] {
  return (Array.isArray(list) ? [...list] : [])
    .sort((a: any, b: any) => Number(b.sceneIndex ?? 0) - Number(a.sceneIndex ?? 0))
    .map((s: any) => String(s.sceneName));
}

/** Full state refresh after connecting or switching scene collection. */
async function sync() {
  const [scenes, studio, rec, stream, replay, vcam, colls, profiles, trans, inputs, dir] = await batch([
    { requestType: "GetSceneList" },
    { requestType: "GetStudioModeEnabled" },
    { requestType: "GetRecordStatus" },
    { requestType: "GetStreamStatus" },
    { requestType: "GetReplayBufferStatus" },
    { requestType: "GetVirtualCamStatus" },
    { requestType: "GetSceneCollectionList" },
    { requestType: "GetProfileList" },
    { requestType: "GetSceneTransitionList" },
    { requestType: "GetInputList" },
    { requestType: "GetRecordDirectory" },
  ]);
  if (scenes) {
    st.scenes = sceneNames(scenes.scenes);
    st.program = scenes.currentProgramSceneName ?? "";
    st.preview = scenes.currentPreviewSceneName ?? "";
  }
  st.studio = !!studio?.studioModeEnabled;
  st.record = { active: !!rec?.outputActive, paused: !!rec?.outputPaused, since: rec?.outputActive ? Date.now() - Number(rec.outputDuration ?? 0) : 0 };
  st.stream = { active: !!stream?.outputActive, since: stream?.outputActive ? Date.now() - Number(stream.outputDuration ?? 0) : 0 };
  st.replay = replay ? !!replay.outputActive : null;
  st.vcam = !!vcam?.outputActive;
  st.collection = colls?.currentSceneCollectionName ?? "";
  st.profile = profiles?.currentProfileName ?? "";
  st.transition = trans?.currentSceneTransitionName ?? "";
  st.inputs = new Set((inputs?.inputs ?? []).map((i: any) => i.inputName));
  st.recordDir = dir?.recordDirectory ?? "";
  st.muted.clear();
  st.volume.clear();
  st.items.clear();
  st.filters.clear();
  st.media.clear();
  st.monitor.clear();
  st.ready = true;
  await watchKeys(host.keys());
  lastPills = JSON.stringify(pills());
  host.status("connected", st.version ? `obs-websocket ${st.version}` : undefined, pills());
  render();
}

/** Fetches the state keys depend on that the initial sync doesn't cover. */
async function watchKeys(keys: Key[]) {
  if (!st.ready || !sock.ready) return;
  const reqs: Req[] = [];
  const after: Array<(r: any) => void> = [];
  const add = (r: Req, fn: (res: any) => void) => (reqs.push(r), after.push(fn));
  const seen = new Set<string>();
  for (const k of keys) {
    const s = k.settings;
    const input = String(s.input ?? "");
    if ((k.action === "mute" || k.action === "volume") && input && !seen.has(`a:${input}`)) {
      seen.add(`a:${input}`);
      add({ requestType: "GetInputMute", requestData: { inputName: input } }, (r) => r && st.muted.set(input, !!r.inputMuted));
      add({ requestType: "GetInputVolume", requestData: { inputName: input } }, (r) => r && st.volume.set(input, Number(r.inputVolumeMul)));
    }
    if (k.action === "monitor" && input && !seen.has(`o:${input}`)) {
      seen.add(`o:${input}`);
      add({ requestType: "GetInputAudioMonitorType", requestData: { inputName: input } }, (r) => r && st.monitor.set(input, r.monitorType));
    }
    if (k.action === "media" && input && !seen.has(`m:${input}`)) {
      seen.add(`m:${input}`);
      add({ requestType: "GetMediaInputStatus", requestData: { inputName: input } }, (r) => r && st.media.set(input, r.mediaState));
    }
    if (k.action === "filter" && s.source && s.filter) {
      const source = String(s.source);
      const filter = String(s.filter);
      add({ requestType: "GetSourceFilter", requestData: { sourceName: source, filterName: filter } }, (r) => r && st.filters.set(pair(source, filter), !!r.filterEnabled));
    }
    if (k.action === "source" && s.scene && s.source) {
      const scene = String(s.scene);
      const source = String(s.source);
      const id = await request("GetSceneItemId", { sceneName: scene, sourceName: source }).catch(() => null);
      if (id) add({ requestType: "GetSceneItemEnabled", requestData: { sceneName: scene, sceneItemId: id.sceneItemId } }, (r) => r && st.items.set(pair(scene, source), { id: id.sceneItemId, enabled: !!r.sceneItemEnabled }));
    }
  }
  const res = await batch(reqs);
  res.forEach((r, i) => after[i](r));
  render();
}

function onEvent(type: string, d: any) {
  switch (type) {
    case "CurrentProgramSceneChanged":
      st.program = d.sceneName;
      break;
    case "CurrentPreviewSceneChanged":
      st.preview = d.sceneName;
      break;
    case "StudioModeStateChanged":
      st.studio = !!d.studioModeEnabled;
      if (!st.studio) st.preview = "";
      else void request("GetCurrentPreviewScene").then((r) => ((st.preview = r.currentPreviewSceneName ?? r.sceneName ?? ""), render()), () => {});
      break;
    case "SceneListChanged":
      st.scenes = sceneNames(d.scenes);
      break;
    case "SceneNameChanged":
      st.scenes = st.scenes.map((s) => (s === d.oldSceneName ? d.sceneName : s));
      if (st.program === d.oldSceneName) st.program = d.sceneName;
      break;
    case "InputCreated":
      st.inputs.add(d.inputName);
      watchSoon();
      break;
    case "InputRemoved":
      st.inputs.delete(d.inputName);
      break;
    case "InputNameChanged":
      st.inputs.delete(d.oldInputName);
      st.inputs.add(d.inputName);
      break;
    case "InputMuteStateChanged":
      st.muted.set(d.inputName, !!d.inputMuted);
      break;
    case "InputVolumeChanged":
      st.volume.set(d.inputName, Number(d.inputVolumeMul));
      break;
    case "RecordStateChanged":
      onOutput(st.record, d.outputState, d.outputActive);
      st.record.paused = d.outputState === "OBS_WEBSOCKET_OUTPUT_PAUSED";
      break;
    case "StreamStateChanged":
      onOutput(st.stream, d.outputState, d.outputActive);
      break;
    case "ReplayBufferStateChanged":
      st.replay = !!d.outputActive;
      break;
    case "ReplayBufferSaved":
      host.toast("Replay saved", "success");
      break;
    case "VirtualcamStateChanged":
      st.vcam = !!d.outputActive;
      break;
    case "SceneItemEnableStateChanged":
      for (const [k, v] of st.items) if (k.startsWith(`${d.sceneName}\u0000`) && v.id === d.sceneItemId) v.enabled = !!d.sceneItemEnabled;
      break;
    case "SourceFilterEnableStateChanged":
      st.filters.set(pair(d.sourceName, d.filterName), !!d.filterEnabled);
      break;
    case "MediaInputPlaybackStarted":
      st.media.set(d.inputName, "OBS_MEDIA_STATE_PLAYING");
      break;
    case "MediaInputPlaybackEnded":
      st.media.set(d.inputName, "OBS_MEDIA_STATE_ENDED");
      break;
    case "MediaInputActionTriggered":
      void request("GetMediaInputStatus", { inputName: d.inputName }).then((r) => (st.media.set(d.inputName, r.mediaState), render()), () => {});
      break;
    case "CurrentSceneTransitionChanged":
      st.transition = d.transitionName;
      break;
    case "CurrentProfileChanged":
      st.profile = d.profileName;
      // Each profile has its own recording folder.
      void request("GetRecordDirectory").then((r) => (st.recordDir = r.recordDirectory ?? ""), () => {});
      break;
    case "InputAudioMonitorTypeChanged":
      st.monitor.set(d.inputName, d.monitorType);
      break;
    case "CurrentSceneCollectionChanged":
      st.collection = d.sceneCollectionName;
      void sync();
      return;
    case "ExitStarted":
      sock.drop("OBS is closing");
      return;
    default:
      return;
  }
  render();
}

function onOutput(o: { active: boolean; since: number }, state: string, active: boolean) {
  if (state === "OBS_WEBSOCKET_OUTPUT_STARTED") o.since = Date.now();
  o.active = !!active;
  if (!o.active) o.since = 0;
}

function pills(): Pill[] {
  const out: Pill[] = [];
  if (st.record.active) out.push({ label: st.record.paused ? "PAUSED" : "REC", tone: st.record.paused ? "warning" : "danger", since: st.record.paused ? undefined : st.record.since });
  if (st.stream.active) out.push({ label: "LIVE", tone: "danger", since: st.stream.since });
  return out;
}

const dB = (mul: number) => (mul <= 0 ? "-inf dB" : `${(20 * Math.log10(mul)).toFixed(1)} dB`);
const mutedIcon = (icon?: string) => (icon === "mic" || !icon ? "mic-off" : icon.startsWith("volume") || icon === "speaker" ? "volume-x" : icon === "headphones" ? "volume-x" : icon);

function view(k: Key): KeyView | null {
  const s = k.settings;
  const mode = String(s.mode ?? "toggle");
  switch (k.action) {
    case "scene": {
      const scene = String(s.scene ?? "");
      const thumb = s.thumbnail ? st.thumbs.get(scene) : undefined;
      if (!st.scenes.includes(scene)) return { disabled: true, sub: "Not in OBS" };
      if (st.program === scene) return { tone: "accent", badge: "ON AIR", badgeTone: "accent", image: thumb };
      if (st.studio && st.preview === scene) return { badge: "PREVIEW", badgeTone: "info", image: thumb };
      return thumb ? { image: thumb } : null;
    }
    case "mute":
    case "volume": {
      const input = String(s.input ?? "");
      if (!st.inputs.has(input)) return { disabled: true, sub: "Not in OBS" };
      const muted = st.muted.get(input);
      const vol = st.volume.get(input);
      const v: KeyView = {};
      if (k.action === "volume" && vol !== undefined) {
        v.value = clamp01(Math.cbrt(vol));
        v.meta = dB(vol);
      }
      if (muted) Object.assign(v, { tone: "danger", badge: "MUTED", badgeTone: "danger", icon: k.action === "mute" ? mutedIcon(k.icon) : undefined });
      else if (muted === false && k.action === "mute") Object.assign(v, { badge: "ON", badgeTone: "success" });
      return v;
    }
    case "record":
      if (!st.record.active) return mode === "stop" || mode === "pause" ? { disabled: true } : null;
      if (st.record.paused) return { tone: "warning", sub: "Paused", icon: mode === "pause" ? "circle-play" : undefined, badge: "PAUSED", badgeTone: "warning" };
      return mode === "toggle" || mode === "stop"
        ? { label: "Stop recording", sub: "Recording", tone: "danger", since: st.record.since, icon: "circle-stop" }
        : { tone: "danger", since: st.record.since, sub: "Recording" };
    case "stream":
      if (!st.stream.active) return mode === "stop" ? { disabled: true } : null;
      return { badge: "LIVE", badgeTone: "danger", since: st.stream.since, label: mode === "toggle" ? "End stream" : undefined, tone: mode === "toggle" ? "danger" : undefined };
    case "replay":
      if (st.replay === null) return { disabled: true, sub: "Replay buffer not set up" };
      if (mode === "save") return st.replay ? { sub: "Buffer on" } : { disabled: true, sub: "Buffer off" };
      return st.replay ? { badge: "RUNNING", badgeTone: "info", tone: "accent" } : null;
    case "virtualcam":
      return st.vcam ? { tone: "accent", badge: "ON", badgeTone: "accent" } : null;
    case "studio-mode":
      return st.studio ? { tone: "accent", badge: "ON", badgeTone: "accent" } : null;
    case "transition":
      return st.studio ? (st.preview ? { sub: st.preview } : null) : { disabled: true, sub: "Studio mode off" };
    case "transition-type":
      return st.transition === s.transition ? { tone: "accent" } : null;
    case "source": {
      const item = st.items.get(pair(String(s.scene ?? ""), String(s.source ?? "")));
      if (!item) return { disabled: true, sub: "Not in OBS" };
      return item.enabled ? { tone: "accent", badge: "SHOWN", badgeTone: "success" } : { badge: "HIDDEN", badgeTone: "muted", icon: "eye-off" };
    }
    case "filter": {
      const on = st.filters.get(pair(String(s.source ?? ""), String(s.filter ?? "")));
      if (on === undefined) return { disabled: true, sub: "Not in OBS" };
      return on ? { tone: "accent", badge: "ON", badgeTone: "success" } : { badge: "OFF", badgeTone: "muted" };
    }
    case "media": {
      const state = st.media.get(String(s.input ?? ""));
      if (state === undefined) return st.inputs.has(String(s.input ?? "")) ? null : { disabled: true, sub: "Not in OBS" };
      return state === "OBS_MEDIA_STATE_PLAYING" ? { badge: "PLAYING", badgeTone: "success", icon: (s.op ?? "play-pause") === "play-pause" ? "circle-pause" : undefined } : null;
    }
    case "chapter":
    case "split":
      return st.record.active ? null : { disabled: true, sub: "Not recording" };
    case "screenshot":
      return st.recordDir ? null : { disabled: true, sub: "No recording folder" };
    case "monitor": {
      const type = st.monitor.get(String(s.input ?? ""));
      if (type === undefined) return st.inputs.has(String(s.input ?? "")) ? null : { disabled: true, sub: "Not in OBS" };
      return type === MONITOR_OFF ? { badge: "OFF", badgeTone: "muted", icon: "headphone-off" } : { tone: "accent", badge: type === MONITOR_ON ? "ON" : "ONLY", badgeTone: "accent" };
    }
    case "stats":
      return statsView(String(s.show ?? "dropped"));
    case "collection":
      return st.collection === s.collection ? { tone: "accent", badge: "ACTIVE", badgeTone: "accent" } : null;
    case "profile":
      return st.profile === s.profile ? { tone: "accent", badge: "ACTIVE", badgeTone: "accent" } : null;
    default:
      return null;
  }
}

/** One figure from OBS's stats. Warns when frames are being lost. */
function statsView(show: string): KeyView {
  // Clients count up from `since` on their own.
  if (show === "live") return st.stream.active ? { since: st.stream.since, tone: "danger" } : { sub: "Offline" };
  if (show === "record") {
    if (!st.record.active) return { sub: "Not recording" };
    return st.record.paused ? { sub: "Paused", badge: "PAUSED", badgeTone: "warning" } : { since: st.record.since, tone: "danger" };
  }
  const x = st.stats;
  if (!x) return { sub: "Waiting for OBS" };
  const pct = (n: number) => `${n < 10 ? n.toFixed(1) : Math.round(n)}%`;
  switch (show) {
    case "fps":
      return { sub: `${x.fps.toFixed(0)} fps` };
    case "bitrate":
      if (!st.stream.active) return { sub: "Offline" };
      if (x.kbps === null) return { sub: "Measuring" };
      return { sub: x.kbps >= 10_000 ? `${(x.kbps / 1000).toFixed(1)} Mb/s` : `${x.kbps} kb/s`, ...(x.kbps === 0 ? { badge: "NO DATA", badgeTone: "danger" as const } : {}) };
    case "cpu":
      return { sub: `CPU ${pct(x.cpu)}`, ...(x.cpu >= 90 ? { badge: "HIGH", badgeTone: "warning" as const } : {}) };
    default: {
      // The worst of render lag, encoder lag and network drops.
      const [what, n] = [["Render", x.render], ["Encoding", x.encode], ["Network", x.network]].reduce((a, b) => ((b[1] as number) > (a[1] as number) ? b : a)) as [string, number];
      if (n < 0.1) return { sub: "No dropped frames", badge: "OK", badgeTone: "success" };
      return { sub: `${what} ${pct(n)} dropped`, tone: n >= 1 ? "danger" : undefined, badge: n >= 1 ? "DROPPING" : "SOME", badgeTone: n >= 1 ? "danger" : "warning" };
    }
  }
}

/** Bytes sent at the last poll, to work out the bitrate. */
let sent: { bytes: number; at: number } | null = null;

/** Polls OBS only while a Stream health key is placed (a stream dashboard has several). */
async function stats() {
  if (!st.ready || !host.keys("stats").length) return;
  const [g, net] = await batch([{ requestType: "GetStats" }, { requestType: "GetStreamStatus" }]).catch(() => [null, null]);
  if (!g) return;
  const share = (part: number, total: number) => (total > 0 ? (100 * part) / total : 0);
  let kbps: number | null = null;
  if (net?.outputActive) {
    const now = { bytes: Number(net.outputBytes ?? 0), at: Date.now() };
    if (sent && now.bytes >= sent.bytes && now.at > sent.at) kbps = Math.round(((now.bytes - sent.bytes) * 8) / (now.at - sent.at));
    sent = now;
  } else sent = null;
  st.stats = {
    fps: Number(g.activeFps ?? 0),
    cpu: Number(g.cpuUsage ?? 0),
    render: share(Number(g.renderSkippedFrames ?? 0), Number(g.renderTotalFrames ?? 0)),
    encode: share(Number(g.outputSkippedFrames ?? 0), Number(g.outputTotalFrames ?? 0)),
    network: net?.outputActive ? share(Number(net.outputSkippedFrames ?? 0), Number(net.outputTotalFrames ?? 0)) : 0,
    kbps,
  };
  const round1 = (n: number) => Math.round(n * 10) / 10;
  host.setVar("bitrate", kbps ?? 0);
  host.setVar("dropped", round1(Math.max(st.stats.render, st.stats.encode, st.stats.network)));
  host.setVar("cpu", round1(st.stats.cpu));
  host.setVar("fps", Math.round(st.stats.fps));
  render();
}

function render() {
  if (!st.ready) return;
  for (const k of host.keys()) {
    const v = view(k);
    if (v) k.set(v);
    else k.clear();
  }
  host.setVar("scene", st.program);
  host.setVar("preview", st.preview);
  host.setVar("recording", st.record.active);
  host.setVar("streaming", st.stream.active);
  host.setVar("replay", !!st.replay);
  host.setVar("studio", st.studio);
  host.setVar("collection", st.collection);
  host.setVar("profile", st.profile);
  const p = JSON.stringify(pills());
  if (p !== lastPills) {
    lastPills = p;
    host.status("connected", st.version ? `obs-websocket ${st.version}` : undefined, pills());
  }
}

let lastPills = "";

function offline(reason: string) {
  st.ready = false;
  lastPills = "";
  waiting.failAll(new Error("OBS disconnected"));
  for (const k of host.keys()) k.set({ disabled: true });
  host.setVar("recording", false);
  host.setVar("streaming", false);
  if (authFailed) host.status("error", reason);
  else host.status("offline", `Can't reach OBS (${reason}). Is the WebSocket server on?`, null);
}

async function thumbnails() {
  if (!st.ready) return;
  const scenes = [...new Set(host.keys("scene").filter((k) => k.settings.thumbnail).map((k) => String(k.settings.scene)))].filter((s) => st.scenes.includes(s));
  if (!scenes.length) return;
  const res = await batch(scenes.map((sourceName) => ({ requestType: "GetSourceScreenshot", requestData: { sourceName, imageFormat: "jpg", imageWidth: 240, imageCompressionQuality: 60 } }))).catch(() => []);
  res.forEach((r, i) => {
    if (typeof r?.imageData === "string" && r.imageData.length < 300_000) st.thumbs.set(scenes[i], r.imageData.replace("data:image/jpg;", "data:image/jpeg;"));
  });
  render();
}

/** What a press should send, worked out from the current state. Null means nothing to do. */
function plan(action: string, s: Record<string, unknown>, phase: "press" | "release"): Req[] | null {
  const mode = String(s.mode ?? "toggle");
  if (phase === "release") {
    if (action === "mute" && mode === "hold") return [{ requestType: "SetInputMute", requestData: { inputName: s.input, inputMuted: true } }];
    return [];
  }
  switch (action) {
    case "scene": {
      const target = String(s.target ?? "auto");
      const preview = target === "preview" || (target === "auto" && st.studio);
      if (preview && !st.studio) throw new Error("Studio mode is off");
      return [{ requestType: preview ? "SetCurrentPreviewScene" : "SetCurrentProgramScene", requestData: { sceneName: s.scene } }];
    }
    case "mute": {
      const input = String(s.input ?? "");
      const muted = want(mode === "hold" ? "off" : mode, st.muted.get(input) ?? false);
      return [{ requestType: "SetInputMute", requestData: { inputName: input, inputMuted: muted } }];
    }
    case "record":
      if (mode === "pause") return st.record.active ? [{ requestType: st.record.paused ? "ResumeRecord" : "PauseRecord" }] : [];
      return [{ requestType: mode === "start" ? "StartRecord" : mode === "stop" ? "StopRecord" : "ToggleRecord" }];
    case "stream":
      return [{ requestType: mode === "start" ? "StartStream" : mode === "stop" ? "StopStream" : "ToggleStream" }];
    case "replay": {
      const m = String(s.mode ?? "save");
      if (m === "save") {
        if (!st.replay) throw new Error("Start the replay buffer first");
        return [{ requestType: "SaveReplayBuffer" }];
      }
      return [{ requestType: m === "start" ? "StartReplayBuffer" : m === "stop" ? "StopReplayBuffer" : "ToggleReplayBuffer" }];
    }
    case "virtualcam":
      return [{ requestType: want(mode, st.vcam) ? "StartVirtualCam" : "StopVirtualCam" }];
    case "studio-mode":
      return [{ requestType: "SetStudioModeEnabled", requestData: { studioModeEnabled: want(mode, st.studio) } }];
    case "transition":
      if (!st.studio) throw new Error("Studio mode is off");
      return [{ requestType: "TriggerStudioModeTransition" }];
    case "transition-type": {
      const out: Req[] = [{ requestType: "SetCurrentSceneTransition", requestData: { transitionName: s.transition } }];
      if (typeof s.duration === "number") out.push({ requestType: "SetCurrentSceneTransitionDuration", requestData: { transitionDuration: s.duration } });
      return out;
    }
    case "source": {
      const item = st.items.get(pair(String(s.scene), String(s.source)));
      if (!item) throw new Error(`"${s.source}" isn't in scene "${s.scene}"`);
      return [{ requestType: "SetSceneItemEnabled", requestData: { sceneName: s.scene, sceneItemId: item.id, sceneItemEnabled: want(mode, item.enabled) } }];
    }
    case "filter":
      return [{ requestType: "SetSourceFilterEnabled", requestData: { sourceName: s.source, filterName: s.filter, filterEnabled: want(mode, st.filters.get(pair(String(s.source), String(s.filter))) ?? false) } }];
    case "media": {
      const op = String(s.op ?? "play-pause");
      const playing = st.media.get(String(s.input)) === "OBS_MEDIA_STATE_PLAYING";
      const name = op === "play-pause" ? (playing ? "PAUSE" : "PLAY") : op.toUpperCase();
      return [{ requestType: "TriggerMediaInputAction", requestData: { inputName: s.input, mediaAction: `OBS_WEBSOCKET_MEDIA_INPUT_ACTION_${name}` } }];
    }
    case "text":
      return [{ requestType: "SetInputSettings", requestData: { inputName: s.input, inputSettings: { text: String(s.text ?? "") }, overlay: true } }];
    case "chapter":
      return [{ requestType: "CreateRecordChapter", requestData: s.name ? { chapterName: s.name } : {} }];
    case "split":
      if (!st.record.active) throw new Error("Not recording");
      return [{ requestType: "SplitRecordFile" }];
    case "screenshot": {
      if (!st.recordDir) throw new Error("OBS has no recording folder set");
      const source = String(s.capture || st.program);
      const stamp = new Date().toISOString().slice(0, 19).replace("T", " ").replace(/:/g, "-");
      const safe = source.replace(/[\\/:*?"<>|]/g, "_").slice(0, 60);
      const path = `${st.recordDir.replace(/[\\/]+$/, "")}/Screenshot ${safe} ${stamp}.png`;
      return [{ requestType: "SaveSourceScreenshot", requestData: { sourceName: source, imageFormat: "png", imageFilePath: path } }];
    }
    case "monitor": {
      const input = String(s.input ?? "");
      const on = want(mode, (st.monitor.get(input) ?? MONITOR_OFF) !== MONITOR_OFF);
      return [{ requestType: "SetInputAudioMonitorType", requestData: { inputName: input, monitorType: on ? MONITOR_ON : MONITOR_OFF } }];
    }
    case "stats":
      return [];
    case "collection":
      return [{ requestType: "SetCurrentSceneCollection", requestData: { sceneCollectionName: s.collection } }];
    case "profile":
      return [{ requestType: "SetCurrentProfile", requestData: { profileName: s.profile } }];
    case "hotkey":
      return [{ requestType: "TriggerHotkeyByName", requestData: { hotkeyName: s.hotkey } }];
    default:
      return null;
  }
}

let watchTimer: ReturnType<typeof setTimeout> | null = null;
function watchSoon() {
  watchTimer ??= setTimeout(() => {
    watchTimer = null;
    void watchKeys(host.keys());
  }, 100);
}

function connected() {
  if (!sock.ready || !st.ready) throw new Error("OBS isn't connected");
}

async function runOne(step: Step, phase: "press" | "release") {
  connected();
  const reqs = plan(step.key.action, step.settings, phase) ?? [];
  if (reqs.length === 1) await request(reqs[0].requestType, reqs[0].requestData);
  else if (reqs.length) await batch(reqs);
}

const press = (step: Step) => runOne(step, "press");

async function options(field: string, s: Record<string, unknown>) {
  connected();
  const list = (xs: unknown[]) => xs.map((x) => ({ value: String(x), label: String(x) }));
  switch (field) {
    case "scene":
      return list(st.scenes);
    case "input":
      return list([...st.inputs].sort());
    case "capture":
      return [{ value: "", label: "Live scene" }, ...list(st.scenes), ...list([...st.inputs].sort())];
    case "source": {
      if (!s.scene) {
        const scenes = list(st.scenes);
        return [...scenes, ...list([...st.inputs].sort())];
      }
      const r = await request("GetSceneItemList", { sceneName: s.scene });
      return list((r.sceneItems ?? []).map((i: any) => i.sourceName));
    }
    case "filter": {
      if (!s.source) return [];
      const r = await request("GetSourceFilterList", { sourceName: s.source });
      return list((r.filters ?? []).map((f: any) => f.filterName));
    }
    case "transition": {
      const r = await request("GetSceneTransitionList");
      return list((r.transitions ?? []).map((t: any) => t.transitionName));
    }
    case "collection":
      return list((await request("GetSceneCollectionList")).sceneCollections ?? []);
    case "profile":
      return list((await request("GetProfileList")).profiles ?? []);
    case "hotkey":
      return list((await request("GetHotkeyList")).hotkeys ?? []);
    default:
      return [];
  }
}

function connect() {
  authFailed = false;
  sock?.stop();
  sock = new Reconnecting(url, {
    open() {
      host.status("connecting", "Signing in to OBS");
    },
    message: onMessage,
    close(reason, code) {
      if (code === AUTH_FAILED) {
        authFailed = true;
        offline("Wrong OBS password. Check the OBS plugin settings.");
        return "stop";
      }
      offline(reason);
    },
  });
  sock.start();
}

const actions = Object.fromEntries(
  ["scene", "mute", "volume", "record", "stream", "replay", "virtualcam", "studio-mode", "transition", "transition-type", "source", "filter", "media", "text", "chapter", "split", "screenshot", "monitor", "stats", "collection", "profile", "hotkey"].map(
    (id) => [id, { press, release: (step: Step) => runOne(step, "release"), options, appear: watchSoon }],
  ),
);

actions.volume = {
  ...actions.volume,
  press: async () => {},
  async slide({ settings, value }: { settings: Record<string, unknown>; value: number }) {
    connected();
    const mul = clamp01(value) ** 3;
    st.volume.set(String(settings.input), mul);
    await request("SetInputVolume", { inputName: settings.input, inputVolumeMul: mul });
  },
} as any;

export default definePlugin({
  start(h) {
    host = h;
    connect();
    thumbTimer = setInterval(() => void thumbnails(), THUMB_EVERY);
    statsTimer = setInterval(() => void stats(), STATS_EVERY);
  },
  connectionChanged() {
    connect();
  },
  stop() {
    if (thumbTimer) clearInterval(thumbTimer);
    if (statsTimer) clearInterval(statsTimer);
    sock?.stop();
  },
  actions,
  async batch(steps, phase) {
    connected();
    const reqs: Req[] = [];
    for (const s of steps) {
      const p = plan(s.action, s.settings, phase);
      if (p === null) return false;
      reqs.push(...p);
    }
    const res = await batch(reqs);
    const failed = res.filter((r) => r === null).length;
    if (failed) throw new Error(`OBS refused ${failed} of ${res.length} steps`);
    return true;
  },
});


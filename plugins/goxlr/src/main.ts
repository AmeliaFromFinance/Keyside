// GoXLR through the GoXLR Utility's WebSocket API. The Utility sends a JSON Patch for every
// change, so keys follow the hardware buttons too.

import { definePlugin, setting, type Host, type Key, type KeyView } from "@keyside/sdk";
import { Reconnecting, Waiters } from "../../_lib/socket.ts";
import { clamp01, want } from "../../_lib/util.ts";

const FADERS = ["A", "B", "C", "D"];
const CHANNELS = ["Mic", "Chat", "Music", "Game", "Console", "LineIn", "System", "Sample", "Headphones", "MicMonitor", "LineOut"];
const INPUTS = ["Microphone", "Chat", "Music", "Game", "Console", "LineIn", "System", "Samples"];
const OUTPUTS = ["BroadcastMix", "Headphones", "ChatMic", "Sampler", "LineOut", "StreamMix2"];
const BANKS = ["A", "B", "C"];
const BUTTONS = ["TopLeft", "TopRight", "BottomLeft", "BottomRight"];
const PRESETS = ["Preset1", "Preset2", "Preset3", "Preset4", "Preset5", "Preset6"];
const EFFECTS: Record<string, [command: string, read: (fx: any) => boolean]> = {
  fx: ["SetFXEnabled", (fx) => !!fx.is_enabled],
  megaphone: ["SetMegaphoneEnabled", (fx) => !!fx.current?.megaphone?.is_enabled],
  robot: ["SetRobotEnabled", (fx) => !!fx.current?.robot?.is_enabled],
  hardtune: ["SetHardTuneEnabled", (fx) => !!fx.current?.hard_tune?.is_enabled],
};
const NAMES: Record<string, string> = { LineIn: "Line in", Sample: "Samples", MicMonitor: "Mic monitor", LineOut: "Line out" };
const UNSAFE = new Set(["__proto__", "constructor", "prototype"]);

let host: Host;
let sock: Reconnecting;
const waiting = new Waiters<any>();
let nextId = 1;
let status: any = null;
let renderTimer: ReturnType<typeof setTimeout> | null = null;
let refetchTimer: ReturnType<typeof setTimeout> | null = null;

/** Only lets known values through, so a shared layout can't send anything odd. */
function pick(v: unknown, allowed: string[], fallback: string): string {
  const s = String(v ?? fallback);
  if (!allowed.includes(s)) throw new Error(`Unknown setting: ${s.slice(0, 40)}`);
  return s;
}

function request(data: unknown, ms = 5000): Promise<any> {
  const id = nextId++;
  const p = waiting.wait(String(id), ms);
  sock.send(JSON.stringify({ id, data }));
  return p;
}

/** The GoXLR the keys control: the one set in the settings, or the first. */
function mixer(): [string, any] | null {
  const mixers = status?.mixers ?? {};
  const want = String(setting(host.connection, "serial", "")).trim();
  const serial = want || Object.keys(mixers)[0];
  return serial && mixers[serial] ? [serial, mixers[serial]] : null;
}

function device(): [string, any] {
  if (!status) throw new Error("The GoXLR Utility isn't connected");
  const m = mixer();
  if (!m) throw new Error("No GoXLR found. Is it plugged in?");
  return m;
}

async function command(cmd: Record<string, unknown>) {
  const [serial] = device();
  await request({ Command: [serial, cmd] });
}

/** Applies the Utility's RFC 6902 patch. Anything unusual fetches the whole status instead. */
export function applyPatch(doc: any, ops: any[]): boolean {
  for (const op of ops) {
    if (typeof op?.path !== "string" || !op.path.startsWith("/")) return false;
    const parts = op.path
      .slice(1)
      .split("/")
      .map((p: string) => p.replace(/~1/g, "/").replace(/~0/g, "~"));
    if (parts.some((p: string) => UNSAFE.has(p))) return false;
    const last = parts.pop()!;
    let parent = doc;
    for (const p of parts) {
      if (parent === null || typeof parent !== "object" || !Object.hasOwn(parent, p)) return false;
      parent = parent[p];
    }
    if (parent === null || typeof parent !== "object") return false;
    const list = Array.isArray(parent);
    if (op.op === "replace" || op.op === "add") {
      if (list && last === "-") parent.push(op.value);
      else if (list && op.op === "add") parent.splice(Number(last), 0, op.value);
      else parent[last] = op.value;
    } else if (op.op === "remove") {
      if (list) parent.splice(Number(last), 1);
      else delete parent[last];
    } else if (op.op !== "test") {
      return false;
    }
  }
  return true;
}

async function refetch() {
  refetchTimer = null;
  try {
    status = await request("GetStatus");
  } catch {
    return;
  }
  changed();
}

function changed() {
  const m = mixer()?.[1];
  host.setVar("profile", m?.profile_name ?? "");
  host.setVar("mic_profile", m?.mic_profile_name ?? "");
  if (!renderTimer) renderTimer = setTimeout(render, 40);
}

function onMessage(raw: string) {
  const m = JSON.parse(raw);
  const d = m?.data;
  if (d && typeof d === "object" && Array.isArray(d.Patch)) {
    // A patch we can't apply means our copy is off. Fetch it again, once things settle.
    if (!status || !applyPatch(status, d.Patch)) refetchTimer ??= setTimeout(refetch, 200);
    else changed();
    return;
  }
  const id = String(m?.id);
  if (d === "Ok") waiting.resolve(id, null);
  else if (d && typeof d === "object" && "Error" in d) waiting.reject(id, new Error(`GoXLR: ${String(d.Error).slice(0, 200)}`));
  else if (d && typeof d === "object" && "Status" in d) waiting.resolve(id, d.Status);
  else waiting.resolve(id, d);
}

const on = (v: KeyView): KeyView => ({ tone: "accent", badge: "ON", badgeTone: "accent", ...v });

function view(k: Key): KeyView | null {
  if (!status) return { disabled: true };
  const m = mixer()?.[1];
  if (!m) return { disabled: true, sub: "No GoXLR found" };
  const s = k.settings;
  const full = (fn: (fx: any, sampler: any) => KeyView | null) => (m.effects && m.sampler ? fn(m.effects, m.sampler) : { disabled: true, sub: "Full-size GoXLR only" });
  switch (k.action) {
    case "fader-mute": {
      const f = m.fader_status?.[String(s.fader ?? "A")];
      if (!f) return null;
      const meta = NAMES[f.channel] ?? f.channel;
      return f.mute_state === "Unmuted" ? { meta } : { meta, tone: "danger", badge: "MUTED", badgeTone: "danger", icon: "volume-x" };
    }
    case "cough":
      return m.cough_button?.state === "Unmuted" ? null : { tone: "danger", badge: "MUTED", badgeTone: "danger", icon: "mic-off" };
    case "volume": {
      const v = Number(m.levels?.volumes?.[String(s.channel ?? "Music")] ?? 0) / 255;
      return { value: clamp01(v), meta: `${Math.round(v * 100)}%` };
    }
    case "effect": {
      const e = EFFECTS[String(s.effect ?? "fx")];
      return full((fx) => (e && e[1](fx) ? on({}) : null));
    }
    case "preset":
      return full((fx) => {
        const p = String(s.preset ?? "");
        const meta = fx.preset_names?.[p];
        return fx.active_preset === p ? on({ badge: "ACTIVE", meta }) : { meta };
      });
    case "sample":
      return full((_, sampler) => (sampler.banks?.[String(s.bank ?? "A")]?.[String(s.button ?? "TopLeft")]?.is_playing ? on({ badge: "PLAYING" }) : null));
    case "route":
      return m.router?.[String(s.input ?? "Music")]?.[String(s.output ?? "BroadcastMix")] ? on({}) : null;
    case "profile": {
      const current = s.kind === "mic" ? m.mic_profile_name : m.profile_name;
      return s.profile && current === s.profile ? on({ badge: "LOADED" }) : null;
    }
  }
  return null;
}

function render() {
  renderTimer = null;
  for (const k of host.keys()) {
    const v = view(k);
    if (v) k.set(v);
    else k.clear();
  }
}

function connect() {
  sock?.stop();
  status = null;
  sock = new Reconnecting(
    () => {
      const h = String(setting(host.connection, "host", "127.0.0.1")).trim();
      return h ? `ws://${h.includes(":") && !h.startsWith("[") ? `[${h}]` : h}:${Number(setting(host.connection, "port", 14564))}/api/websocket` : null;
    },
    {
      async open() {
        status = await request("GetStatus");
        sock.healthy();
        const m = mixer();
        host.status("connected", m ? (m[1].hardware?.device_type === "Mini" ? "GoXLR Mini" : "GoXLR") : "No GoXLR plugged in");
        changed();
      },
      message: onMessage,
      close(reason) {
        status = null;
        waiting.failAll(new Error("The GoXLR Utility disconnected"));
        host.status("offline", `Can't reach the GoXLR Utility (${reason}). Is it running?`);
        render();
      },
    },
  );
  sock.start();
}

async function setEffect(name: string, enabled: boolean) {
  const e = EFFECTS[name];
  if (!e) throw new Error("Unknown effect");
  if (!device()[1].effects) throw new Error("Effects need a full-size GoXLR");
  await command({ [e[0]]: enabled });
}

async function setCough(to: unknown, muted: boolean) {
  await command({ SetCoughMuteState: muted ? (to === "all" ? "MutedToAll" : "MutedToX") : "Unmuted" });
}

export default definePlugin({
  start(h) {
    host = h;
    connect();
  },
  connectionChanged: connect,
  stop() {
    if (renderTimer) clearTimeout(renderTimer);
    if (refetchTimer) clearTimeout(refetchTimer);
    sock?.stop();
  },
  actions: {
    "fader-mute": {
      async press({ settings }) {
        const fader = pick(settings.fader, FADERS, "A");
        const muted = device()[1].fader_status?.[fader]?.mute_state !== "Unmuted";
        const state = want(settings.mode ?? "toggle", muted) ? (settings.to === "all" ? "MutedToAll" : "MutedToX") : "Unmuted";
        await command({ SetFaderMuteState: [fader, state] });
      },
      appear: render,
    },
    cough: {
      press({ settings }) {
        if (settings.mode === "hold") return setCough(settings.to, true);
        return setCough(settings.to, want(settings.mode ?? "toggle", device()[1].cough_button?.state !== "Unmuted"));
      },
      release({ settings }) {
        if (settings.mode === "hold") return setCough(settings.to, false);
      },
      appear: render,
    },
    volume: {
      async slide({ settings, value }) {
        const ch = pick(settings.channel, CHANNELS, "Music");
        const v = Math.round(clamp01(value) * 255);
        await command({ SetVolume: [ch, v] });
      },
      appear: render,
    },
    effect: {
      press({ settings }) {
        const name = String(settings.effect ?? "fx");
        if (settings.mode === "hold") return setEffect(name, true);
        const fx = device()[1].effects;
        return setEffect(name, want(settings.mode ?? "toggle", !!fx && !!EFFECTS[name]?.[1](fx)));
      },
      release({ settings }) {
        if (settings.mode === "hold") return setEffect(String(settings.effect ?? "fx"), false);
      },
      appear: render,
    },
    preset: {
      async press({ settings }) {
        const p = pick(settings.preset, PRESETS, "");
        if (!device()[1].effects) throw new Error("Effects need a full-size GoXLR");
        await command({ SetActiveEffectPreset: p });
      },
      options() {
        const names = mixer()?.[1].effects?.preset_names ?? {};
        return PRESETS.map((p, i) => ({ value: p, label: names[p] ? `${i + 1}: ${names[p]}` : `Preset ${i + 1}` }));
      },
      appear: render,
    },
    sample: {
      async press({ settings }) {
        const bank = pick(settings.bank, BANKS, "A");
        const button = pick(settings.button, BUTTONS, "TopLeft");
        const sampler = device()[1].sampler;
        if (!sampler) throw new Error("Samples need a full-size GoXLR");
        const op = String(settings.op ?? "toggle");
        const stop = op === "stop" || (op === "toggle" && !!sampler.banks?.[bank]?.[button]?.is_playing);
        await command(stop ? { StopSamplePlayback: [bank, button] } : { PlayNextSample: [bank, button] });
      },
      appear: render,
    },
    route: {
      async press({ settings }) {
        const input = pick(settings.input, INPUTS, "Music");
        const output = pick(settings.output, OUTPUTS, "BroadcastMix");
        const enabled = want(settings.mode ?? "toggle", !!device()[1].router?.[input]?.[output]);
        await command({ SetRouter: [input, output, enabled] });
      },
      appear: render,
    },
    profile: {
      async press({ settings }) {
        const mic = settings.kind === "mic";
        const name = String(settings.profile ?? "");
        const known: string[] = (mic ? status?.files?.mic_profiles : status?.files?.profiles) ?? [];
        device();
        if (!known.includes(name)) throw new Error(`There's no ${mic ? "mic " : ""}profile called ${name.slice(0, 60) || "that"}`);
        await command({ [mic ? "LoadMicProfile" : "LoadProfile"]: [name, true] });
      },
      options(_field, settings) {
        if (!status) throw new Error("The GoXLR Utility isn't connected");
        const list: string[] = (settings.kind === "mic" ? status.files?.mic_profiles : status.files?.profiles) ?? [];
        return [...list].sort().map((p) => ({ value: p, label: p }));
      },
      appear: render,
    },
  },
});

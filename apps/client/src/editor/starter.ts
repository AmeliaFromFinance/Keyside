// Starter layouts for the setup wizard: one page per app, with keys that work right away.
// Lists like OBS scenes come from the app when its plugin can reach it.

import type { Action, Key, Page, Profile } from "@keyside/protocol";

// Key labels go through the editor's translation when the layout is made.
const mark = (s: string) => s;

export interface StarterApp {
  id: string;
  name: string;
  plugin: string;
  icon: string;
}

export const STARTER_APPS: StarterApp[] = [
  { id: "obs", name: "OBS Studio", plugin: "dev.keyside.obs", icon: "monitor-play" },
  { id: "streamerbot", name: "Streamer.bot", plugin: "dev.keyside.streamerbot", icon: "bot" },
  { id: "streamlabs", name: "Streamlabs Desktop", plugin: "dev.keyside.streamlabs", icon: "monitor" },
  { id: "meld", name: "Meld Studio", plugin: "dev.keyside.meld", icon: "clapperboard" },
  { id: "discord", name: "Discord", plugin: "dev.keyside.discord", icon: "headset" },
  { id: "spotify", name: "Spotify", plugin: "dev.keyside.spotify", icon: "music" },
  { id: "twitch", name: "Twitch", plugin: "dev.keyside.twitch", icon: "tv" },
  { id: "youtube", name: "YouTube Live", plugin: "dev.keyside.youtube", icon: "tv-minimal" },
  { id: "streamelements", name: "StreamElements", plugin: "dev.keyside.streamelements", icon: "bell" },
  { id: "vtubestudio", name: "VTube Studio", plugin: "dev.keyside.vtubestudio", icon: "scan-face" },
  { id: "veadotube", name: "veadotube", plugin: "dev.keyside.veadotube", icon: "smile" },
  { id: "goxlr", name: "GoXLR", plugin: "dev.keyside.goxlr", icon: "sliders-vertical" },
  { id: "voicemeeter", name: "Voicemeeter", plugin: "dev.keyside.voicemeeter", icon: "audio-lines" },
  { id: "homeassistant", name: "Home Assistant", plugin: "dev.keyside.homeassistant", icon: "house" },
  { id: "hue", name: "Philips Hue", plugin: "dev.keyside.hue", icon: "lightbulb" },
  { id: "keylight", name: "Elgato Key Light", plugin: "dev.keyside.keylight", icon: "lamp-desk" },
];

export const app = (id: string) => STARTER_APPS.find((a) => a.id === id);

/** A list the editor can read from the app, like the scenes of OBS. */
export interface Lookup {
  type: string;
  field: string;
}
export type Option = { value: string; label: string };
/** Options found, by `${type}#${field}`. */
export type Found = Record<string, Option[]>;
export const lookupKey = (l: Lookup) => `${l.type}#${l.field}`;

const P = (id: string) => app(id)!.plugin;
const L = (id: string, action: string, field: string): Lookup => ({ type: `${P(id)}/${action}`, field });

export const LOOKUPS: Record<string, Lookup[]> = {
  obs: [L("obs", "scene", "scene"), L("obs", "mute", "input")],
  streamlabs: [L("streamlabs", "scene", "scene")],
  meld: [L("meld", "scene", "scene")],
  streamerbot: [L("streamerbot", "run", "actionName")],
  vtubestudio: [L("vtubestudio", "hotkey", "hotkey")],
  veadotube: [L("veadotube", "state", "state")],
  homeassistant: [L("homeassistant", "toggle", "entity"), L("homeassistant", "activate", "entity")],
  hue: [L("hue", "scene", "scene")],
};

/** Apps whose page lists scenes or similar from the app, so they can be filled in after setup. */
export const fillable = (id: string) => (LOOKUPS[id] ?? []).length > 0;

interface Spec {
  label: string;
  sub?: string;
  icon: string;
  kind?: Key["kind"];
  w?: number;
  h?: number;
  hold?: number;
  actions?: Action[];
  widget?: Key["widget"];
}

const COLS = 3;
const ROWS = 5;

function act(appId: string, action: string, settings: Record<string, unknown> = {}): Action {
  return { type: `${P(appId)}/${action}`, settings };
}

/** Buttons for each option, or one button to finish setting up when the app gave none. */
function each(found: Option[] | undefined, max: number, make: (o: Option) => Spec, placeholder: Spec): Spec[] {
  const list = (found ?? []).slice(0, max);
  return list.length ? list.map(make) : [placeholder];
}

function specs(id: string, f: Found): Spec[] {
  const get = (action: string, field: string) => f[lookupKey(L(id, action, field))];
  const slider = (label: string, icon: string, action: string, settings: Record<string, unknown> = {}): Spec => ({ label, icon, kind: "slider", h: 2, actions: [act(id, action, settings)] });
  switch (id) {
    case "obs": {
      const inputs = get("mute", "input") ?? [];
      const mic = inputs.find((o) => /mic/i.test(o.value))?.value ?? "Mic/Aux";
      const desktop = inputs.find((o) => /desktop/i.test(o.value))?.value ?? "Desktop Audio";
      const scenes = (get("scene", "scene") ?? []).slice(0, 6);
      return [
        ...scenes.map((s) => ({ label: s.label, sub: mark("Scene"), icon: "monitor", actions: [act(id, "scene", { scene: s.value, target: "auto", thumbnail: false })] })),
        { label: mark("Mic"), sub: mic, icon: "mic", actions: [act(id, "mute", { input: mic, mode: "toggle" })] },
        { label: mark("Desktop audio"), sub: desktop, icon: "volume-2", actions: [act(id, "mute", { input: desktop, mode: "toggle" })] },
        { label: mark("Record"), icon: "circle-dot", actions: [act(id, "record", { mode: "toggle" })] },
        { label: mark("Stream"), sub: mark("Hold 1 s"), icon: "radio", hold: 1000, actions: [act(id, "stream", { mode: "toggle" })] },
        { label: mark("Save replay"), icon: "rotate-ccw", actions: [act(id, "replay", { mode: "save" })] },
      ];
    }
    case "streamlabs":
      return [
        ...(get("scene", "scene") ?? []).slice(0, 9).map((s) => ({ label: s.label, sub: mark("Scene"), icon: "monitor", actions: [act(id, "scene", { scene: s.value })] })),
        { label: mark("Record"), icon: "circle-dot", actions: [act(id, "record")] },
        { label: mark("Go live"), sub: mark("Hold 1 s"), icon: "radio", hold: 1000, actions: [act(id, "stream")] },
        { label: mark("Save replay"), icon: "rotate-ccw", actions: [act(id, "replay", { mode: "save" })] },
      ];
    case "meld":
      return [
        ...(get("scene", "scene") ?? []).slice(0, 8).map((s) => ({ label: s.label, sub: mark("Scene"), icon: "monitor", actions: [act(id, "scene", { scene: s.value, mode: "show" })] })),
        { label: mark("Record"), icon: "circle-dot", actions: [act(id, "record", { mode: "toggle" })] },
        { label: mark("Stream"), sub: mark("Hold 1 s"), icon: "radio", hold: 1000, actions: [act(id, "stream", { mode: "toggle" })] },
        { label: mark("Screenshot"), icon: "camera", actions: [act(id, "command", { command: "screenshot" })] },
        { label: mark("Clip"), icon: "scissors", actions: [act(id, "command", { command: "clip" })] },
      ];
    case "streamerbot":
      return each(
        get("run", "actionName"),
        15,
        (o) => ({ label: o.label, sub: "Streamer.bot", icon: "bot", actions: [act(id, "run", { actionName: o.value })] }),
        { label: mark("Pick an action"), sub: "Streamer.bot", icon: "bot", actions: [act(id, "run", { actionName: "" })] },
      );
    case "discord":
      return [
        { label: mark("Mute"), icon: "mic-off", actions: [act(id, "mute", { mode: "toggle" })] },
        { label: mark("Deafen"), icon: "headphone-off", actions: [act(id, "deafen", { mode: "toggle" })] },
        { label: mark("Push to talk"), icon: "hand", actions: [act(id, "voice-mode", { mode: "toggle" })] },
        { label: mark("Noise suppression"), icon: "audio-waveform", actions: [act(id, "processing", { which: "noise", mode: "toggle" })] },
        { label: mark("Leave voice"), icon: "log-out", actions: [act(id, "leave")] },
        slider(mark("Mic volume"), "mic", "input-volume"),
        slider(mark("Speaker volume"), "volume-2", "output-volume"),
      ];
    case "spotify":
      return [
        { label: mark("Now playing"), icon: "disc-3", w: 3, actions: [act(id, "now-playing")] },
        { label: mark("Previous"), icon: "skip-back", actions: [act(id, "previous")] },
        { label: mark("Play or pause"), icon: "play", actions: [act(id, "play-pause")] },
        { label: mark("Next"), icon: "skip-forward", actions: [act(id, "next")] },
        { label: mark("Shuffle"), icon: "shuffle", actions: [act(id, "shuffle")] },
        { label: mark("Repeat"), icon: "repeat", actions: [act(id, "repeat")] },
        { label: mark("Like"), icon: "heart", actions: [act(id, "like")] },
        slider(mark("Volume"), "volume-2", "volume"),
      ];
    case "twitch":
      return [
        { label: mark("Chat"), icon: "message-square", kind: "widget", w: 3, h: 2, widget: { feed: "twitch.chat" } },
        { label: mark("Viewers"), icon: "users", kind: "widget", widget: { text: "{twitch.viewers}" } },
        { label: mark("Marker"), icon: "bookmark", actions: [act(id, "marker", { description: "" })] },
        { label: mark("Clip"), icon: "scissors", actions: [act(id, "clip")] },
        { label: mark("Run ad"), sub: mark("Hold 1 s"), icon: "tv", hold: 1000, actions: [act(id, "ad", { length: 60 })] },
        { label: mark("Emote only"), icon: "smile", actions: [act(id, "chat-mode", { mode: "emote", op: "toggle" })] },
        { label: mark("Slow mode"), icon: "hourglass", actions: [act(id, "chat-mode", { mode: "slow", op: "toggle", seconds: 10 })] },
        { label: mark("Shield mode"), icon: "shield", actions: [act(id, "shield", { op: "toggle" })] },
      ];
    case "youtube":
      return [
        { label: mark("Chat"), icon: "message-square", kind: "widget", w: 3, h: 2, widget: { feed: "youtube.chat" } },
        { label: mark("Viewers"), icon: "users", kind: "widget", widget: { text: "{youtube.viewers}" } },
        { label: mark("Likes"), icon: "thumbs-up", kind: "widget", widget: { text: "{youtube.likes}" } },
        { label: mark("Stream health"), icon: "activity", actions: [act(id, "health")] },
      ];
    case "streamelements":
      return [
        { label: mark("Skip alert"), icon: "skip-forward", actions: [act(id, "skip")] },
        { label: mark("Pause alerts"), icon: "pause", actions: [act(id, "pause", { mode: "toggle" })] },
        { label: mark("Mute alert sounds"), icon: "volume-x", actions: [act(id, "mute", { mode: "toggle" })] },
        { label: mark("Replay alert"), icon: "rotate-ccw", actions: [act(id, "replay", { which: "last" })] },
      ];
    case "vtubestudio":
      return each(
        get("hotkey", "hotkey"),
        15,
        (o) => ({ label: o.label, sub: "VTube Studio", icon: "sparkles", actions: [act(id, "hotkey", { hotkey: o.value })] }),
        { label: mark("Pick a hotkey"), sub: "VTube Studio", icon: "sparkles", actions: [act(id, "hotkey", { hotkey: "" })] },
      );
    case "veadotube":
      return [
        ...each(
          get("state", "state"),
          14,
          (o) => ({ label: o.label, icon: "smile", actions: [act(id, "state", { state: o.value, op: "set", thumbnail: true })] }),
          { label: mark("Pick a state"), icon: "smile", actions: [act(id, "state", { state: "", op: "set", thumbnail: true })] },
        ),
        { label: mark("Clear states"), icon: "rotate-ccw", actions: [act(id, "clear")] },
      ];
    case "goxlr":
      return [
        ...["A", "B", "C", "D"].map((fader) => ({ label: `${fader}`, sub: mark("Fader mute"), icon: "volume-x", actions: [act(id, "fader-mute", { fader, to: "fader", mode: "toggle" })] })),
        { label: mark("Cough"), icon: "mic-off", actions: [act(id, "cough", { to: "button", mode: "hold" })] },
        { label: mark("Voice effects"), icon: "sparkles", actions: [act(id, "effect", { effect: "fx", mode: "toggle" })] },
        slider(mark("Mic"), "mic", "volume", { channel: "Mic" }),
        slider(mark("Music"), "music", "volume", { channel: "Music" }),
        slider(mark("Chat"), "headset", "volume", { channel: "Chat" }),
      ];
    case "voicemeeter":
      return [
        { label: mark("Input 1"), sub: mark("Mute"), icon: "mic-off", actions: [act(id, "mute", { target: "Strip", index: 1, mode: "toggle" })] },
        { label: mark("Input 2"), sub: mark("Mute"), icon: "mic-off", actions: [act(id, "mute", { target: "Strip", index: 2, mode: "toggle" })] },
        { label: mark("Output A1"), sub: mark("Mute"), icon: "volume-x", actions: [act(id, "mute", { target: "Bus", index: 1, mode: "toggle" })] },
        slider(mark("Input 1"), "mic", "gain", { target: "Strip", index: 1 }),
        slider(mark("Output A1"), "volume-2", "gain", { target: "Bus", index: 1 }),
      ];
    case "homeassistant": {
      const toggles = (get("toggle", "entity") ?? []).slice(0, 6).map((o) => ({ label: o.label, icon: "power", actions: [act(id, "toggle", { entity: o.value, mode: "toggle" })] }));
      const scenes = (get("activate", "entity") ?? []).slice(0, 6).map((o) => ({ label: o.label, icon: "sparkles", actions: [act(id, "activate", { entity: o.value })] }));
      return toggles.length + scenes.length ? [...toggles, ...scenes] : [{ label: mark("Pick a device"), icon: "house", actions: [act(id, "toggle", { entity: "", mode: "toggle" })] }];
    }
    case "hue":
      return each(
        get("scene", "scene"),
        15,
        (o) => ({ label: o.label, sub: mark("Scene"), icon: "lightbulb", actions: [act(id, "scene", { scene: o.value })] }),
        { label: mark("Pick a scene"), icon: "lightbulb", actions: [act(id, "scene", { scene: "" })] },
      );
    case "keylight":
      return [
        { label: mark("Light"), icon: "lamp-desk", actions: [act(id, "power", { light: "all", mode: "toggle" })] },
        { label: mark("Daylight"), sub: "4500 K", icon: "sun", actions: [act(id, "preset", { light: "all", brightness: 50, kelvin: 4500 })] },
        { label: mark("Warm"), sub: "3200 K", icon: "lightbulb", actions: [act(id, "preset", { light: "all", brightness: 40, kelvin: 3200 })] },
        slider(mark("Brightness"), "sun", "brightness", { light: "all" }),
        slider(mark("Colour"), "thermometer", "temperature", { light: "all" }),
      ];
  }
  return [];
}

/** Places keys left to right, top to bottom, each in the first spot it fits. Drops what doesn't fit. */
function pack(list: Spec[], tr: (s: string) => string): Key[] {
  const used = new Set<string>();
  const free = (x: number, y: number, w: number, h: number) => {
    if (x + w > COLS || y + h > ROWS) return false;
    for (let dy = 0; dy < h; dy++) for (let dx = 0; dx < w; dx++) if (used.has(`${x + dx},${y + dy}`)) return false;
    return true;
  };
  const keys: Key[] = [];
  for (const s of list) {
    const [w, h] = [s.w ?? 1, s.h ?? 1];
    let spot: [number, number] | null = null;
    for (let i = 0; i < COLS * ROWS && !spot; i++) if (free(i % COLS, Math.floor(i / COLS), w, h)) spot = [i % COLS, Math.floor(i / COLS)];
    if (!spot) continue;
    for (let dy = 0; dy < h; dy++) for (let dx = 0; dx < w; dx++) used.add(`${spot[0] + dx},${spot[1] + dy}`);
    const k: Key = { id: `k${keys.length + 1}`, x: spot[0], y: spot[1], kind: s.kind ?? "button", label: tr(s.label).slice(0, 64), icon: s.icon, actions: s.actions ?? [] };
    if (w > 1) k.w = w;
    if (h > 1) k.h = h;
    if (s.sub) k.sub = tr(s.sub).slice(0, 64);
    if (s.hold) k.hold = s.hold;
    if (s.widget) k.widget = s.widget;
    if (k.kind === "slider") k.slider = { min: 0, max: 1 };
    keys.push(k);
  }
  return keys;
}

export function buildPage(id: string, found: Found, tr: (s: string) => string = (s) => s): Page {
  return { id, name: app(id)?.name ?? id, cols: COLS, rows: ROWS, keys: pack(specs(id, found), tr) };
}

/** A new layout with a page for each picked app, in the order of STARTER_APPS. */
export function buildProfile(apps: string[], found: Found, name: string, taken: string[], tr: (s: string) => string = (s) => s): Profile {
  const picked = STARTER_APPS.filter((a) => apps.includes(a.id));
  const pages = picked.map((a) => buildPage(a.id, found, tr));
  let id = "setup";
  for (let n = 2; taken.includes(id); n++) id = `setup-${n}`;
  if (!pages.length) pages.push({ id: "main", name: tr(mark("Main")), cols: COLS, rows: ROWS, keys: [] });
  return { id, name: name.trim().slice(0, 40) || tr(mark("My setup")), homePage: pages[0].id, pages };
}

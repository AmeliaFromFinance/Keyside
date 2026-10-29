// Philips Hue Bridge, local API v1. The bridge hands out a username after its link button
// is pressed; it's kept in the keychain.

import { definePlugin, setting, type Host, type Key, type KeyView } from "@phonedeck/sdk";
import { clamp01, errorText, http, sleep, want } from "../../_lib/util.ts";

const POLL_EVERY = 3000;
const PAIR_FOR = 90_000;

/** Overridden in tests. */
export const endpoints = { base: (bridge: string) => `http://${bridge.includes(":") && !bridge.startsWith("[") ? `[${bridge}]` : bridge}` };

interface Target {
  name: string;
  on: boolean;
  bri: number;
}

let host: Host;
let generation = 0;
let timer: ReturnType<typeof setInterval> | null = null;
let user = "";
const groups = new Map<string, Target & { kind: string }>();
const lights = new Map<string, Target>();
const scenes = new Map<string, { name: string; group: string }>();

const bridge = () => String(setting(host.connection, "bridge", "")).trim();
const url = (path: string) => `${endpoints.base(bridge())}/api/${encodeURIComponent(user)}${path}`;

/** The v1 API answers 200 with [{error: ...}] on failure. */
async function call(method: string, path: string, json?: unknown) {
  const r = await http(url(path), { method, json, timeout: 4000 });
  const err = Array.isArray(r) ? r.find((x) => x?.error)?.error : r?.[0]?.error;
  if (err) throw Object.assign(new Error(String(err.description ?? "Hue Bridge refused the request")), { hueType: err.type });
  return r;
}

async function pair(gen: number) {
  host.status("connecting", "Press the round button on the Hue Bridge");
  host.toast("Press the button on your Hue Bridge");
  const end = Date.now() + PAIR_FOR;
  while (Date.now() < end && gen === generation) {
    const r = await http(`${endpoints.base(bridge())}/api`, { json: { devicetype: "phonedeck#desktop" }, timeout: 4000 }).catch(() => null);
    const name = Array.isArray(r) ? r[0]?.success?.username : undefined;
    if (name) {
      user = String(name);
      await host.saveConnection({ username: user });
      host.toast("Paired with the Hue Bridge", "success");
      return true;
    }
    await sleep(3000);
  }
  if (gen === generation) host.status("error", "The bridge button wasn't pressed in time. Save the settings to try again.");
  return false;
}

async function poll() {
  try {
    const [g, l] = await Promise.all([call("GET", "/groups"), call("GET", "/lights")]);
    groups.clear();
    lights.clear();
    for (const [id, x] of Object.entries<any>(g ?? {})) groups.set(id, { name: x.name, kind: x.type, on: !!x.state?.any_on, bri: Number(x.action?.bri ?? 0) });
    for (const [id, x] of Object.entries<any>(l ?? {})) lights.set(id, { name: x.name, on: !!x.state?.on && x.state?.reachable !== false, bri: Number(x.state?.bri ?? 0) });
    const all = [...lights.values()];
    groups.set("0", { name: "All lights", kind: "All", on: all.some((x) => x.on), bri: Math.max(0, ...all.map((x) => x.bri)) });
    host.status("connected", `${lights.size} lights`);
  } catch (e) {
    if ((e as any).hueType === 1) {
      user = "";
      await host.saveConnection({ username: "" });
      return void begin();
    }
    host.status("offline", `Can't reach the Hue Bridge (${errorText(e)})`);
    groups.clear();
    lights.clear();
  }
  render();
}

function target(t: unknown): Target | undefined {
  const [kind, id] = String(t ?? "").split(":");
  return kind === "group" ? groups.get(id) : kind === "light" ? lights.get(id) : undefined;
}

function view(k: Key): KeyView | null {
  if (!lights.size) return { disabled: true };
  if (k.action === "scene") return scenes.size && !scenes.has(String(k.settings.scene)) ? { disabled: true, sub: "Not on the bridge" } : null;
  const t = target(k.settings.target);
  if (!t) return { disabled: true, sub: "Not on the bridge" };
  if (k.action === "power") return t.on ? { tone: "accent", badge: "ON", badgeTone: "accent" } : { icon: (k.icon ?? "lightbulb") === "lightbulb" ? "lightbulb-off" : undefined };
  if (k.action === "brightness") return { value: t.on ? clamp01(t.bri / 254) : 0, meta: t.on ? `${Math.round((t.bri / 254) * 100)}%` : "Off" };
  return null;
}

function render() {
  for (const k of host.keys()) {
    const v = view(k);
    if (v) k.set(v);
    else k.clear();
  }
}

/** sRGB hex to CIE xy, the colour space Hue lamps take. */
export function hexToXy(hex: string): [number, number] {
  const m = /^#?([0-9a-f]{6})$/i.exec(hex.trim());
  if (!m) throw new Error("Colour must look like #ff8800");
  const n = parseInt(m[1], 16);
  const lin = (c: number) => (c > 0.04045 ? ((c + 0.055) / 1.055) ** 2.4 : c / 12.92);
  const [r, g, b] = [(n >> 16) & 255, (n >> 8) & 255, n & 255].map((c) => lin(c / 255));
  const X = r * 0.664511 + g * 0.154324 + b * 0.162028;
  const Y = r * 0.283881 + g * 0.668433 + b * 0.047685;
  const Z = r * 0.000088 + g * 0.07231 + b * 0.986039;
  const sum = X + Y + Z;
  if (sum === 0) return [0.3127, 0.329];
  return [Math.round((X / sum) * 10000) / 10000, Math.round((Y / sum) * 10000) / 10000];
}

async function set(t: unknown, state: Record<string, unknown>) {
  const [kind, id] = String(t ?? "").split(":");
  if (!/^\d{1,4}$/.test(id ?? "") || (kind !== "group" && kind !== "light")) throw new Error("Pick a room, zone or light");
  await call("PUT", kind === "group" ? `/groups/${id}/action` : `/lights/${id}/state`, state);
  const cur = target(t);
  if (cur && typeof state.on === "boolean") cur.on = state.on;
  if (cur && typeof state.bri === "number") cur.bri = state.bri;
  render();
}

async function begin() {
  const gen = ++generation;
  if (timer) clearInterval(timer);
  timer = null;
  user = String(setting(host.connection, "username", ""));
  if (!bridge()) return host.status("offline", "Add the Hue Bridge address in the plugin settings.");
  if (!user && !(await pair(gen))) return;
  if (gen !== generation) return;
  await poll();
  const sc = await call("GET", "/scenes").catch(() => ({}));
  scenes.clear();
  for (const [id, s] of Object.entries<any>(sc ?? {})) scenes.set(id, { name: s.name, group: s.group ?? "0" });
  render();
  timer = setInterval(() => void poll(), POLL_EVERY);
}

const targetOptions = () => [
  ...[...groups].filter(([, g]) => ["Room", "Zone", "LightGroup"].includes(g.kind)).map(([id, g]) => ({ value: `group:${id}`, label: `${g.name} (${g.kind === "Zone" ? "zone" : "room"})` })),
  { value: "group:0", label: "All lights" },
  ...[...lights].map(([id, l]) => ({ value: `light:${id}`, label: l.name })),
];

export default definePlugin({
  start(h) {
    host = h;
    void begin();
  },
  connectionChanged: () => void begin(),
  stop() {
    generation++;
    if (timer) clearInterval(timer);
  },
  actions: {
    power: { press: ({ settings }) => set(settings.target, { on: want(settings.mode ?? "toggle", target(settings.target)?.on ?? false) }), options: targetOptions, appear: render },
    brightness: {
      slide: ({ settings, value }) => (value <= 0.01 ? set(settings.target, { on: false }) : set(settings.target, { on: true, bri: Math.max(1, Math.round(clamp01(value) * 254)) })),
      options: targetOptions,
      appear: render,
    },
    scene: {
      async press({ settings }) {
        const s = scenes.get(String(settings.scene));
        if (!s) throw new Error("That scene isn't on the bridge");
        await call("PUT", `/groups/${encodeURIComponent(s.group)}/action`, { scene: String(settings.scene) });
      },
      options: () => [...scenes].map(([id, s]) => ({ value: id, label: `${s.name} (${groups.get(s.group)?.name ?? "all"})` })),
      appear: render,
    },
    color: { press: ({ settings }) => set(settings.target, { on: true, xy: hexToXy(String(settings.color ?? "")) }), options: targetOptions, appear: render },
  },
});

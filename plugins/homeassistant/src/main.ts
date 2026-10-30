// Home Assistant over its WebSocket API with a long-lived access token.

import { definePlugin, setting, type Host, type Key, type KeyView } from "@keyside/sdk";
import { Reconnecting, Waiters } from "../../_lib/socket.ts";
import { b64, clamp01, want } from "../../_lib/util.ts";

/** Overridden in tests: where images are fetched from. Empty means the configured address. */
export const endpoints = { http: "" };

const ENTITY = /^[a-z0-9_]+\.[a-z0-9_]+$/;
const ON = new Set(["on", "open", "opening", "unlocked", "playing", "home", "heat", "cool", "auto", "active"]);
const DOMAINS: Record<string, string[]> = {
  toggle: ["light", "switch", "fan", "input_boolean", "lock", "cover", "media_player", "automation", "siren", "humidifier", "climate"],
  activate: ["scene", "script", "button", "input_button", "automation"],
  brightness: ["light"],
  "color-temp": ["light"],
  temperature: ["climate"],
  media: ["media_player"],
  "media-volume": ["media_player"],
  camera: ["camera"],
};
const MEDIA_OPS: Record<string, string> = {
  "play-pause": "media_play_pause",
  next: "media_next_track",
  previous: "media_previous_track",
  "volume-up": "volume_up",
  "volume-down": "volume_down",
};
const MAX_IMAGE = 300_000;

interface Entity {
  state: string;
  name: string;
  unit: string;
  brightness: number | null;
  attrs: Record<string, any>;
}

let host: Host;
let sock: Reconnecting;
const waiting = new Waiters<any>();
let nextId = 1;
let ready = false;
const states = new Map<string, Entity>();

function send(msg: Record<string, unknown>): Promise<any> {
  const id = nextId++;
  return waiting.request(String(id), () => sock.send(JSON.stringify({ id, ...msg })));
}

function toEntity(s: any): Entity {
  return {
    state: String(s?.state ?? "unknown"),
    name: String(s?.attributes?.friendly_name ?? ""),
    unit: String(s?.attributes?.unit_of_measurement ?? ""),
    brightness: typeof s?.attributes?.brightness === "number" ? s.attributes.brightness : null,
    attrs: s?.attributes && typeof s.attributes === "object" ? s.attributes : {},
  };
}

const watched = () =>
  new Set([
    ...String(setting(host.connection, "watch", ""))
      .split(/[\s,]+/)
      .filter((e) => ENTITY.test(e)),
    ...host.keys().map((k) => String(k.settings.entity ?? "")).filter((e) => ENTITY.test(e)),
  ]);

function publish(id: string) {
  const e = states.get(id);
  if (e && watched().has(id)) host.setVar(id, e.unit ? `${e.state} ${e.unit}` : e.state);
}

function onMessage(raw: string) {
  const m = JSON.parse(raw);
  switch (m.type) {
    case "auth_required":
      sock.send(JSON.stringify({ type: "auth", access_token: String(host.connection.token ?? "") }));
      break;
    case "auth_ok":
      void onReady();
      break;
    case "auth_invalid":
      host.status("error", "Home Assistant didn't accept the access token.");
      sock.stop();
      break;
    case "result":
      if (m.success) waiting.resolve(String(m.id), m.result);
      else waiting.reject(String(m.id), new Error(m.error?.message ?? "Home Assistant refused the request"));
      break;
    case "event": {
      const d = m.event?.data;
      if (d?.entity_id) {
        if (d.new_state) states.set(d.entity_id, toEntity(d.new_state));
        else states.delete(d.entity_id);
        publish(d.entity_id);
        renderFor(d.entity_id);
        void loadArt(d.entity_id);
      }
      break;
    }
  }
}

async function onReady() {
  sock.healthy();
  const list = await send({ type: "get_states" });
  states.clear();
  for (const s of list ?? []) states.set(s.entity_id, toEntity(s));
  await send({ type: "subscribe_events", event_type: "state_changed" });
  ready = true;
  host.status("connected", `${states.size} entities`);
  for (const id of watched()) publish(id);
  render();
  for (const k of host.keys("media")) void loadArt(String(k.settings.entity ?? ""));
  void snapshots();
}

// Covers for media players and snapshots for cameras, as data URLs.
const art = new Map<string, { from: string; image: string }>();
const lastShot = new Map<string, number>();

function httpBase(): string {
  if (endpoints.http) return endpoints.http;
  const h = String(setting(host.connection, "host", "homeassistant.local")).trim();
  const scheme = setting(host.connection, "secure", false) ? "https" : "http";
  return `${scheme}://${h.includes(":") && !h.startsWith("[") ? `[${h}]` : h}:${Number(setting(host.connection, "port", 8123))}`;
}

/** Fetches an image from Home Assistant. Paths only, so it can't be pointed elsewhere. */
async function image(path: string): Promise<string | null> {
  if (!path.startsWith("/api/")) return null;
  const r = await fetch(`${httpBase()}${path}`, { headers: { authorization: `Bearer ${String(host.connection.token ?? "")}` } }).catch(() => null);
  if (!r?.ok) return null;
  const type = (r.headers.get("content-type") ?? "").split(";")[0].trim();
  if (!/^image\/(jpeg|png|webp|gif)$/.test(type)) return null;
  const bytes = new Uint8Array(await r.arrayBuffer());
  return bytes.length && bytes.length <= MAX_IMAGE ? `data:${type};base64,${b64(bytes)}` : null;
}

async function loadArt(id: string) {
  if (!id.startsWith("media_player.") || !host.keys("media").some((k) => k.settings.entity === id)) return;
  const pic = String(states.get(id)?.attrs.entity_picture ?? "");
  if (art.get(id)?.from === pic) return;
  if (!pic) {
    art.delete(id);
    return renderFor(id);
  }
  const img = await image(pic);
  if (img) art.set(id, { from: pic, image: img });
  else art.delete(id);
  renderFor(id);
}

/** Refreshes camera snapshots as often as each key asks, small enough for a key. */
async function snapshots() {
  if (!ready) return;
  const now = Date.now();
  for (const k of host.keys("camera")) {
    const id = String(k.settings.entity ?? "");
    if (!id.startsWith("camera.")) continue;
    const every = Math.max(2, Number(k.settings.seconds ?? 10)) * 1000;
    if (now - (lastShot.get(id) ?? 0) < every) continue;
    lastShot.set(id, now);
    const img = await image(`/api/camera_proxy/${id}?width=320`);
    if (img) art.set(id, { from: "snapshot", image: img });
    renderFor(id);
  }
}

function view(k: Key): KeyView | null {
  if (!ready) return { disabled: true };
  const id = String(k.settings.entity ?? "");
  if (k.action === "service") return null;
  const e = states.get(id);
  if (!e) return { disabled: true, sub: "Not in Home Assistant" };
  if (e.state === "unavailable") return { disabled: true, sub: "Unavailable" };
  switch (k.action) {
    case "toggle":
      return ON.has(e.state) ? { tone: "accent", badge: "ON", badgeTone: "accent" } : null;
    case "brightness":
      return { value: e.state === "on" ? clamp01((e.brightness ?? 255) / 255) : 0, meta: e.state === "on" ? `${Math.round(((e.brightness ?? 255) / 255) * 100)}%` : "Off" };
    case "color-temp": {
      const [lo, hi] = [Number(e.attrs.min_color_temp_kelvin ?? 2000), Number(e.attrs.max_color_temp_kelvin ?? 6500)];
      const k = Number(e.attrs.color_temp_kelvin);
      if (e.state !== "on" || !Number.isFinite(k)) return { value: 0, meta: e.state === "on" ? "Colour" : "Off" };
      return { value: clamp01((k - lo) / (hi - lo || 1)), meta: `${Math.round(k)} K` };
    }
    case "temperature": {
      const [lo, hi] = [Number(e.attrs.min_temp ?? 7), Number(e.attrs.max_temp ?? 35)];
      const target = Number(e.attrs.temperature);
      const now = e.attrs.current_temperature;
      return {
        value: Number.isFinite(target) ? clamp01((target - lo) / (hi - lo || 1)) : 0,
        meta: Number.isFinite(target) ? `${target}°` : e.state,
        sub: typeof now === "number" ? `Now ${now}°` : undefined,
      };
    }
    case "media": {
      const a = e.attrs;
      const cover = k.settings.cover !== false ? art.get(id)?.image : undefined;
      if (e.state === "off" || e.state === "standby") return { sub: "Off", tone: "muted" };
      const title = String(a.media_title ?? "");
      const v: KeyView = { image: cover, sub: String(a.media_artist ?? a.app_name ?? e.state).slice(0, 64) };
      if (title) v.label = title.slice(0, 64);
      if (e.state === "playing") Object.assign(v, { tone: "accent" });
      else if (e.state === "paused") Object.assign(v, { badge: "PAUSED", badgeTone: "muted" });
      if (a.is_volume_muted) Object.assign(v, { badge: "MUTED", badgeTone: "danger" });
      return v;
    }
    case "media-volume": {
      const vol = Number(e.attrs.volume_level);
      return Number.isFinite(vol) ? { value: clamp01(vol), meta: `${Math.round(vol * 100)}%` } : { value: 0, meta: e.state };
    }
    case "camera":
      return { image: art.get(id)?.image, sub: art.has(id) ? undefined : "Loading" };
    case "state":
      return { sub: (e.unit ? `${e.state} ${e.unit}` : e.state).slice(0, 64) };
    default:
      return null;
  }
}

function render() {
  for (const k of host.keys()) {
    const v = view(k);
    if (v) k.set(v);
    else k.clear();
  }
}

function renderFor(id: string) {
  for (const k of host.keys()) {
    if (k.settings.entity !== id) continue;
    const v = view(k);
    if (v) k.set(v);
    else k.clear();
  }
}

let shotTimer: ReturnType<typeof setInterval> | null = null;

function connect() {
  sock?.stop();
  ready = false;
  art.clear();
  lastShot.clear();
  sock = new Reconnecting(
    () => {
      const h = String(setting(host.connection, "host", "homeassistant.local")).trim();
      if (!h || !host.connection.token) return null;
      const scheme = setting(host.connection, "secure", false) ? "wss" : "ws";
      return `${scheme}://${h.includes(":") && !h.startsWith("[") ? `[${h}]` : h}:${Number(setting(host.connection, "port", 8123))}/api/websocket`;
    },
    {
      open() {
        host.status("connecting");
      },
      message: onMessage,
      close(reason) {
        ready = false;
        waiting.failAll(new Error("Home Assistant disconnected"));
        host.status("offline", host.connection.token ? `Can't reach Home Assistant (${reason})` : "Add a Home Assistant access token in the plugin settings.");
        render();
      },
    },
  );
  sock.start();
}

function entity(s: Record<string, unknown>, action?: string): string {
  const id = String(s.entity ?? "").trim();
  if (!ENTITY.test(id)) throw new Error("Pick an entity");
  if (action && DOMAINS[action] && !DOMAINS[action].includes(id.split(".")[0])) throw new Error(`${id} can't be used here`);
  return id;
}

function service(full: string, entityId: string | null, data: Record<string, unknown> = {}) {
  if (!ready) throw new Error("Home Assistant isn't connected");
  const [domain, svc] = full.split(".");
  return send({ type: "call_service", domain, service: svc, service_data: data, ...(entityId ? { target: { entity_id: entityId } } : {}) });
}

function toggleService(id: string, on: boolean): string {
  const domain = id.split(".")[0];
  if (domain === "lock") return on ? "lock.unlock" : "lock.lock";
  if (domain === "cover") return on ? "cover.open_cover" : "cover.close_cover";
  return on ? "homeassistant.turn_on" : "homeassistant.turn_off";
}

const options = (action: string) => () => {
  if (!ready) throw new Error("Home Assistant isn't connected");
  const domains = DOMAINS[action];
  return [...states]
    .filter(([id]) => !domains || domains.includes(id.split(".")[0]))
    .sort(([a], [b]) => a.localeCompare(b))
    .slice(0, 1000)
    .map(([id, e]) => ({ value: id, label: e.name ? `${e.name} (${id})` : id }));
};

/** A slider value (0 to 1) as a number between an entity's limits, on its step. */
function between(e: Entity | undefined, value: number, lo: string, hi: string, dflt: [number, number], step: number): number {
  const [a, b] = [Number(e?.attrs[lo] ?? dflt[0]), Number(e?.attrs[hi] ?? dflt[1])];
  return Math.round((a + clamp01(value) * (b - a)) / step) * step;
}

export default definePlugin({
  start(h) {
    host = h;
    connect();
    shotTimer = setInterval(() => void snapshots(), 1000);
  },
  connectionChanged: connect,
  stop: () => {
    if (shotTimer) clearInterval(shotTimer);
    sock?.stop();
  },
  actions: {
    toggle: {
      async press({ settings }) {
        const id = entity(settings, "toggle");
        await service(toggleService(id, want(settings.mode ?? "toggle", ON.has(states.get(id)?.state ?? ""))), id);
      },
      options: options("toggle"),
      appear: render,
    },
    activate: {
      async press({ settings }) {
        const id = entity(settings, "activate");
        const domain = id.split(".")[0];
        const svc = domain === "scene" || domain === "script" ? `${domain}.turn_on` : domain === "automation" ? "automation.trigger" : `${domain}.press`;
        await service(svc, id);
      },
      options: options("activate"),
      appear: render,
    },
    brightness: {
      async slide({ settings, value }) {
        const id = entity(settings, "brightness");
        const pct = Math.round(clamp01(value) * 100);
        await (pct === 0 ? service("light.turn_off", id) : service("light.turn_on", id, { brightness_pct: pct }));
      },
      options: options("brightness"),
      appear: render,
    },
    "color-temp": {
      async slide({ settings, value }) {
        const id = entity(settings, "color-temp");
        await service("light.turn_on", id, { color_temp_kelvin: between(states.get(id), value, "min_color_temp_kelvin", "max_color_temp_kelvin", [2000, 6500], 50) });
      },
      options: options("color-temp"),
      appear: render,
    },
    temperature: {
      async slide({ settings, value }) {
        const id = entity(settings, "temperature");
        const step = Number(states.get(id)?.attrs.target_temp_step ?? 0.5) || 0.5;
        await service("climate.set_temperature", id, { temperature: between(states.get(id), value, "min_temp", "max_temp", [7, 35], step) });
      },
      options: options("temperature"),
      appear: render,
    },
    media: {
      async press({ settings }) {
        const id = entity(settings, "media");
        const op = String(settings.op ?? "play-pause");
        if (op === "mute") return service("media_player.volume_mute", id, { is_volume_muted: !states.get(id)?.attrs.is_volume_muted });
        if (!MEDIA_OPS[op]) throw new Error("Unknown media control");
        await service(`media_player.${MEDIA_OPS[op]}`, id);
      },
      options: options("media"),
      appear: (k) => (render(), void loadArt(String(k.settings.entity ?? ""))),
    },
    "media-volume": {
      async slide({ settings, value }) {
        await service("media_player.volume_set", entity(settings, "media-volume"), { volume_level: Math.round(clamp01(value) * 100) / 100 });
      },
      options: options("media-volume"),
      appear: render,
    },
    camera: {
      // Pressing takes a new snapshot straight away.
      press({ settings }) {
        lastShot.delete(entity(settings, "camera"));
        return snapshots();
      },
      options: options("camera"),
      appear: (k) => (lastShot.delete(String(k.settings.entity ?? "")), render()),
    },
    state: { options: options("state"), appear: render },
    service: {
      async press({ settings }) {
        const full = String(settings.service ?? "").trim();
        if (!ENTITY.test(full)) throw new Error("Service should look like light.turn_on");
        const raw = String(settings.data ?? "").trim();
        let data: unknown = {};
        if (raw) {
          try {
            data = JSON.parse(raw);
          } catch {
            throw new Error("Data must be valid JSON");
          }
          if (!data || typeof data !== "object" || Array.isArray(data)) throw new Error("Data must be a JSON object");
        }
        const target = String(settings.entity ?? "").trim();
        if (target && !target.split(",").every((t) => ENTITY.test(t.trim()))) throw new Error("Entity should look like light.kitchen");
        await service(full, target || null, data as Record<string, unknown>);
      },
    },
  },
});

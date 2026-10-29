// Home Assistant over its WebSocket API with a long-lived access token.

import { definePlugin, setting, type Host, type Key, type KeyView } from "@phonedeck/sdk";
import { Reconnecting, Waiters } from "../../_lib/socket.ts";
import { clamp01, want } from "../../_lib/util.ts";

const ENTITY = /^[a-z0-9_]+\.[a-z0-9_]+$/;
const ON = new Set(["on", "open", "opening", "unlocked", "playing", "home", "heat", "cool", "auto", "active"]);
const DOMAINS: Record<string, string[]> = {
  toggle: ["light", "switch", "fan", "input_boolean", "lock", "cover", "media_player", "automation", "siren", "humidifier", "climate"],
  activate: ["scene", "script", "button", "input_button", "automation"],
  brightness: ["light"],
};

interface Entity {
  state: string;
  name: string;
  unit: string;
  brightness: number | null;
}

let host: Host;
let sock: Reconnecting;
const waiting = new Waiters<any>();
let nextId = 1;
let ready = false;
const states = new Map<string, Entity>();

function send(msg: Record<string, unknown>): Promise<any> {
  const id = nextId++;
  const p = waiting.wait(String(id));
  sock.send(JSON.stringify({ id, ...msg }));
  return p;
}

function toEntity(s: any): Entity {
  return {
    state: String(s?.state ?? "unknown"),
    name: String(s?.attributes?.friendly_name ?? ""),
    unit: String(s?.attributes?.unit_of_measurement ?? ""),
    brightness: typeof s?.attributes?.brightness === "number" ? s.attributes.brightness : null,
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

function connect() {
  sock?.stop();
  ready = false;
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

export default definePlugin({
  start(h) {
    host = h;
    connect();
  },
  connectionChanged: connect,
  stop: () => sock?.stop(),
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

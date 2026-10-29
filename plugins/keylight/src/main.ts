// Elgato lights over their local HTTP API on port 9123.

import { definePlugin, type Host, type Key, type KeyView } from "@phonedeck/sdk";
import { clamp01, errorText, http, want } from "../../_lib/util.ts";

const POLL_EVERY = 5000;
const PORT = 9123;
// The API takes temperature as 143..344 (about 1,000,000 / kelvin).
const toKelvin = (t: number) => Math.round(1_000_000 / t / 50) * 50;
const fromKelvin = (k: number) => Math.min(344, Math.max(143, Math.round(1_000_000 / k)));

interface Light {
  on: boolean;
  brightness: number;
  temperature: number;
}

let host: Host;
let timer: ReturnType<typeof setInterval> | null = null;
const lights = new Map<string, Light | null>();
/** Overridden in tests. */
export const endpoints = { base: (addr: string) => `http://${addr.includes(":") && !addr.startsWith("[") ? `[${addr}]` : addr}:${PORT}` };

function addresses(): Array<[string, string]> {
  return ["1", "2", "3", "4"].map((n) => [n, String(host.connection[`light${n}`] ?? "").trim()] as [string, string]).filter(([, a]) => a);
}

function pick(which: unknown): Array<[string, string]> {
  const all = addresses();
  if (!which || which === "all") return all;
  const one = all.filter(([n]) => n === String(which));
  if (!one.length) throw new Error(`Light ${which} has no address yet`);
  return one;
}

async function poll() {
  const all = addresses();
  if (!all.length) {
    host.status("offline", "Add a light address in the plugin settings.");
    return;
  }
  await Promise.all(
    all.map(async ([n, addr]) => {
      try {
        const r = await http(`${endpoints.base(addr)}/elgato/lights`, { timeout: 3000 });
        const l = r?.lights?.[0];
        lights.set(n, l ? { on: !!l.on, brightness: Number(l.brightness), temperature: Number(l.temperature) } : null);
      } catch {
        lights.set(n, null);
      }
    }),
  );
  const down = all.filter(([n]) => !lights.get(n)).length;
  if (down === all.length) host.status("offline", "Can't reach the lights. Check the addresses.");
  else host.status("connected", down ? `${down} of ${all.length} lights not responding` : `${all.length} light${all.length > 1 ? "s" : ""}`);
  render();
}

function summary(which: unknown): Light | null {
  try {
    const list = pick(which)
      .map(([n]) => lights.get(n))
      .filter((l): l is Light => !!l);
    if (!list.length) return null;
    return { on: list.some((l) => l.on), brightness: list[0].brightness, temperature: list[0].temperature };
  } catch {
    return null;
  }
}

function view(k: Key): KeyView | null {
  const l = summary(k.settings.light);
  if (!l) return { disabled: true };
  switch (k.action) {
    case "power":
      return l.on ? { tone: "accent", badge: `${l.brightness}%`, badgeTone: "accent" } : { icon: (k.icon ?? "lightbulb") === "lightbulb" ? "lightbulb-off" : undefined };
    case "brightness":
      return { value: clamp01((l.brightness - 3) / 97), meta: `${l.brightness}%` };
    case "temperature":
      return { value: clamp01((344 - l.temperature) / (344 - 143)), meta: `${toKelvin(l.temperature)} K` };
    case "preset":
      return l.on && l.brightness === Number(k.settings.brightness ?? 50) && Math.abs(toKelvin(l.temperature) - Number(k.settings.kelvin ?? 4500)) <= 100 ? { tone: "accent" } : null;
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

async function apply(which: unknown, change: (l: Light) => Partial<Light>) {
  const targets = pick(which);
  await Promise.all(
    targets.map(async ([n, addr]) => {
      const cur = lights.get(n) ?? { on: false, brightness: 50, temperature: 213 };
      const next = { ...cur, ...change(cur) };
      await http(`${endpoints.base(addr)}/elgato/lights`, {
        method: "PUT",
        json: { numberOfLights: 1, lights: [{ on: next.on ? 1 : 0, brightness: next.brightness, temperature: next.temperature }] },
        timeout: 3000,
      }).catch((e) => {
        throw new Error(`Light ${n}: ${errorText(e)}`);
      });
      lights.set(n, next);
    }),
  );
  render();
}

export default definePlugin({
  start(h) {
    host = h;
    void poll();
    timer = setInterval(() => void poll(), POLL_EVERY);
  },
  connectionChanged: () => void poll(),
  stop() {
    if (timer) clearInterval(timer);
  },
  actions: {
    power: {
      press: ({ settings }) => {
        const on = want(settings.mode ?? "toggle", summary(settings.light)?.on ?? false);
        return apply(settings.light, () => ({ on }));
      },
      appear: render,
    },
    brightness: { slide: ({ settings, value }) => apply(settings.light, () => ({ on: true, brightness: Math.round(3 + clamp01(value) * 97) })), appear: render },
    temperature: { slide: ({ settings, value }) => apply(settings.light, () => ({ temperature: Math.round(344 - clamp01(value) * (344 - 143)) })), appear: render },
    preset: {
      press: ({ settings }) => apply(settings.light, () => ({ on: true, brightness: Number(settings.brightness ?? 50), temperature: fromKelvin(Number(settings.kelvin ?? 4500)) })),
      appear: render,
    },
  },
});

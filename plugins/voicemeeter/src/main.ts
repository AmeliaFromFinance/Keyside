// Voicemeeter over VBAN-TEXT (UDP). VBAN text can't read state back, so toggles follow what
// Keyside last sent.

import { definePlugin, setting, type Host, type Key, type KeyView } from "@keyside/sdk";
import { clamp01, want } from "../../_lib/util.ts";

const MIN_DB = -60;
const MAX_DB = 12;

let host: Host;
let frame = 0;
const known = new Map<string, boolean | number>();

/** Builds one VBAN text packet (28-byte header, UTF-8 body). */
export function packet(stream: string, text: string, counter: number): Uint8Array {
  const body = new TextEncoder().encode(text);
  const out = new Uint8Array(28 + body.length);
  out.set([0x56, 0x42, 0x41, 0x4e], 0); // "VBAN"
  out[4] = 0x40 | 18; // text sub-protocol, 256 kbps
  out[5] = 0;
  out[6] = 0;
  out[7] = 0x10; // UTF-8
  out.set(new TextEncoder().encode(stream.slice(0, 16)), 8);
  new DataView(out.buffer).setUint32(24, counter >>> 0, true);
  out.set(body, 28);
  return out;
}

/** Keeps scripts to Voicemeeter's command syntax so a key can't smuggle anything else. */
export function cleanScript(s: string): string {
  const text = s.replace(/[\r\n]+/g, ";").trim();
  if (!text) throw new Error("The script is empty");
  if (text.length > 1000 || /[^\x20-\x7e]/.test(text)) throw new Error("Scripts can only use plain ASCII and up to 1000 characters");
  return text;
}

async function send(text: string) {
  const h = String(setting(host.connection, "host", "127.0.0.1")).trim();
  const port = Number(setting(host.connection, "port", 6980));
  const stream = String(setting(host.connection, "stream", "Command1"));
  await host.udp(h, port, packet(stream, text, frame++));
}

const ch = (s: Record<string, unknown>) => {
  const target = s.target === "Bus" ? "Bus" : "Strip";
  const index = Math.min(8, Math.max(1, Math.round(Number(s.index ?? 1)))) - 1;
  return `${target}[${index}]`;
};

/** A preset file path, quoted for Voicemeeter. Quotes and command separators aren't allowed in it. */
export function presetCommand(path: string): string {
  const p = path.trim();
  if (!p) throw new Error("Set the preset file first");
  if (p.length > 260 || /[";\x00-\x1f]/.test(p)) throw new Error("That path can't contain quotes, ; or control characters");
  if (!/\.xml$/i.test(p)) throw new Error("Voicemeeter presets are .xml files");
  return `Command.Load="${p}";`;
}

const button = (s: Record<string, unknown>) => {
  const n = Math.round(Number(s.button ?? 1));
  if (!(n >= 1 && n <= 80)) throw new Error("Macro buttons are numbered 1 to 80");
  return `Command.Button[${n - 1}].State`;
};

const RECORDER: Record<string, string> = { play: "Recorder.Play=1;", stop: "Recorder.Stop=1;", record: "Recorder.Record=1;", pause: "Recorder.Pause=1;" };

function view(k: Key): KeyView | null {
  const s = k.settings;
  if (k.action === "macro") {
    try {
      return known.get(button(s)) === true ? { tone: "accent", badge: "ON", badgeTone: "accent" } : null;
    } catch {
      return { disabled: true, sub: "Set a button number" };
    }
  }
  if (k.action === "mute") return known.get(`${ch(s)}.Mute`) === true ? { tone: "danger", badge: "MUTED", badgeTone: "danger", icon: (k.icon ?? "mic") === "mic" ? "mic-off" : undefined } : null;
  if (k.action === "route") return known.get(`${ch({ index: s.index })}.${s.output ?? "A1"}`) === true ? { tone: "accent", badge: "ON", badgeTone: "accent" } : null;
  if (k.action === "gain") {
    const db = known.get(`${ch(s)}.Gain`);
    return typeof db === "number" ? { value: (db - MIN_DB) / (MAX_DB - MIN_DB), meta: `${db.toFixed(1)} dB` } : null;
  }
  return null;
}

function render() {
  for (const k of host.keys()) {
    const v = view(k);
    if (v) k.set(v);
    else k.clear();
  }
}

async function setFlag(param: string, mode: unknown) {
  const on = want(mode ?? "toggle", known.get(param) === true);
  await send(`${param}=${on ? 1 : 0};`);
  known.set(param, on);
  render();
}

export default definePlugin({
  start(h) {
    host = h;
    host.status("connected", "VBAN text");
  },
  actions: {
    mute: { press: ({ settings }) => setFlag(`${ch(settings)}.Mute`, settings.mode), appear: render },
    route: {
      press: ({ settings }) => {
        const out = String(settings.output ?? "A1");
        if (!/^(A[1-5]|B[1-3])$/.test(out)) throw new Error("Unknown output");
        return setFlag(`${ch({ index: settings.index })}.${out}`, settings.mode);
      },
      appear: render,
    },
    gain: {
      async slide({ settings, value }) {
        const db = Math.round((MIN_DB + clamp01(value) * (MAX_DB - MIN_DB)) * 10) / 10;
        await send(`${ch(settings)}.Gain=${db};`);
        known.set(`${ch(settings)}.Gain`, db);
        render();
      },
      appear: render,
    },
    script: { press: ({ settings }) => send(cleanScript(String(settings.script ?? ""))) },
    macro: {
      async press({ settings }) {
        const b = button(settings);
        const mode = String(settings.mode ?? "press");
        if (mode === "press") {
          // Like a click: down, then up.
          await send(`${b}=1;`);
          await new Promise((r) => setTimeout(r, 120));
          await send(`${b}=0;`);
          return;
        }
        await setFlag(b, mode);
      },
      appear: render,
    },
    preset: { press: ({ settings }) => send(presetCommand(String(settings.file ?? ""))) },
    recorder: {
      press({ settings }) {
        const cmd = RECORDER[String(settings.op ?? "play")];
        if (!cmd) throw new Error("Unknown recorder control");
        return send(cmd);
      },
    },
  },
});

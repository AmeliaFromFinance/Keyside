// Meld Studio through its WebChannel API on port 13376. Meld shares one object, "meld", whose
// session lists every scene, layer, effect and audio track by a 32-character id.

import { definePlugin, setting, type Host, type Key, type KeyView } from "@keyside/sdk";
import { Reconnecting } from "../../_lib/socket.ts";
import { clamp01, want } from "../../_lib/util.ts";
import { WebChannel } from "./webchannel.ts";

const COMMANDS: Record<string, string> = {
  screenshot: "meld.screenshot",
  "screenshot-vertical": "meld.screenshot.vertical",
  clip: "meld.recordClip",
  "replay-show": "meld.replay.show",
  "replay-dismiss": "meld.replay.dismiss",
  vcam: "meld.toggleVirtualCameraAction",
};
const ID = /^[0-9A-Fa-f]{32}$/;

interface Item {
  type: string;
  name?: string;
  parent?: string;
  index?: number;
  current?: boolean;
  staged?: boolean;
  visible?: boolean;
  enabled?: boolean;
  muted?: boolean;
  monitoring?: boolean;
}

let host: Host;
let sock: Reconnecting;
let ch: WebChannel | null = null;
let ready = false;
/** Track volumes, which Meld sends as signals rather than in the session. */
const gains = new Map<string, number>();
const observed = new Set<string>();
let renderTimer: ReturnType<typeof setTimeout> | null = null;

const meld = () => ch?.objects.get("meld")?.props ?? {};
const items = (): Record<string, Item> => ((meld().session as any)?.items ?? {}) as Record<string, Item>;

function connected(): WebChannel {
  if (!ready || !ch) throw new Error("Meld Studio isn't connected");
  return ch;
}

/** Looks up an item by id and checks its type, so a shared layout can only name real things. */
function item(id: unknown, type: string): [string, Item] {
  connected();
  const key = String(id ?? "");
  const it = ID.test(key) ? items()[key] : undefined;
  if (!it || it.type !== type) throw new Error(`That ${type} isn't in this Meld session`);
  return [key, it];
}

const call = (method: string, ...args: unknown[]) => connected().invoke("meld", method, args);

function changed() {
  const m = meld();
  const live = Object.values(items()).find((i) => i.type === "scene" && i.current);
  host.setVar("scene", live?.name ?? "");
  host.setVar("streaming", !!m.isStreaming);
  host.setVar("recording", !!m.isRecording);
  if (!renderTimer) renderTimer = setTimeout(render, 30);
}

const on = (badge = "ON"): KeyView => ({ tone: "accent", badge, badgeTone: "accent" });

function view(k: Key): KeyView | null {
  if (!ready) return { disabled: true };
  const s = k.settings;
  const all = items();
  const find = (id: unknown, type: string) => {
    const it = all[String(id ?? "")];
    return it?.type === type ? it : undefined;
  };
  const missing = (what: string): KeyView => ({ disabled: true, sub: `${what} not found` });
  switch (k.action) {
    case "scene": {
      const it = find(s.scene, "scene");
      if (!it) return missing("Scene");
      if (it.current) return on("ON AIR");
      return it.staged ? { badge: "STAGED", badgeTone: "info" } : null;
    }
    case "record":
      return meld().isRecording ? { tone: "danger", badge: "REC", badgeTone: "danger" } : null;
    case "stream":
      return meld().isStreaming ? { tone: "danger", badge: "LIVE", badgeTone: "danger" } : null;
    case "layer": {
      const it = find(s.layer, "layer");
      if (!it) return missing("Layer");
      return it.visible ? on("SHOWN") : null;
    }
    case "effect": {
      const it = find(s.effect, "effect");
      if (!it) return missing("Effect");
      return it.enabled ? on() : null;
    }
    case "mute": {
      const it = find(s.track, "track");
      if (!it) return missing("Track");
      return it.muted ? { tone: "danger", badge: "MUTED", badgeTone: "danger", icon: "mic-off" } : null;
    }
    case "monitor": {
      const it = find(s.track, "track");
      if (!it) return missing("Track");
      return it.monitoring ? on() : null;
    }
    case "gain": {
      const id = String(s.track ?? "");
      if (!find(id, "track")) return missing("Track");
      watchGain(id);
      const g = gains.get(id);
      return g === undefined ? null : { value: clamp01(g), meta: `${Math.round(g * 100)}%` };
    }
  }
  return null;
}

/** Meld only reports a track's volume once asked to watch it. */
function watchGain(id: string) {
  if (observed.has(id) || !ready) return;
  observed.add(id);
  call("registerTrackObserver", "keyside", id).catch(() => observed.delete(id));
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
  ready = false;
  gains.clear();
  observed.clear();
  sock = new Reconnecting(
    () => {
      const h = String(setting(host.connection, "host", "127.0.0.1")).trim();
      return h ? `ws://${h.includes(":") && !h.startsWith("[") ? `[${h}]` : h}:${Number(setting(host.connection, "port", 13376))}` : null;
    },
    {
      async open() {
        const c = new WebChannel((t) => sock.send(t));
        c.onChange = (o) => o === "meld" && changed();
        c.onSignal = (o, signal, args) => {
          if (o !== "meld" || signal !== "gainUpdated") return;
          const [track, gain] = args;
          if (typeof track === "string" && typeof gain === "number") {
            gains.set(track, gain);
            if (!renderTimer) renderTimer = setTimeout(render, 30);
          }
        };
        ch = c;
        await c.init();
        if (!c.objects.has("meld")) throw new Error("this isn't Meld Studio");
        c.listen("meld", "gainUpdated");
        ready = true;
        sock.healthy();
        host.status("connected");
        changed();
      },
      message: (raw) => ch?.message(raw),
      close(reason) {
        ready = false;
        ch?.failAll(new Error("Meld Studio disconnected"));
        gains.clear();
        observed.clear();
        host.status("offline", `Can't reach Meld Studio (${reason}). Is it open?`);
        render();
      },
    },
  );
  sock.start();
}

/** Picker entries for one type, named with their scene (and layer) so duplicates are clear. */
function choices(type: string) {
  connected();
  const all = items();
  const path = (id: string | undefined, depth = 0): string[] => {
    const it = id && depth < 4 ? all[id] : undefined;
    return it ? [...path(it.parent, depth + 1), it.name ?? "?"] : [];
  };
  const list = Object.entries(all)
    .filter(([, it]) => it.type === type)
    .map(([id, it]) => ({ value: id, label: path(id).join(" › ") || id, index: it.index ?? 0 }));
  // Scenes keep Meld's order. The rest read best sorted by scene and name.
  list.sort((a, b) => (type === "scene" ? a.index - b.index : a.label.localeCompare(b.label)));
  return list.map(({ value, label }) => ({ value, label }));
}

async function toggleTo(mode: unknown, now: boolean, toggle: () => Promise<unknown>) {
  if (want(mode ?? "toggle", now) !== now) await toggle();
}

export default definePlugin({
  start(h) {
    host = h;
    connect();
  },
  connectionChanged: connect,
  stop() {
    if (renderTimer) clearTimeout(renderTimer);
    sock?.stop();
  },
  actions: {
    scene: {
      async press({ settings }) {
        const [id] = item(settings.scene, "scene");
        await call(settings.mode === "stage" ? "setStagedScene" : "showScene", id);
      },
      options: () => choices("scene"),
      appear: render,
    },
    "go-live": {
      async press() {
        await call("showStagedScene");
      },
    },
    record: {
      press({ settings }) {
        connected();
        return toggleTo(settings.mode, !!meld().isRecording, () => call("toggleRecord"));
      },
      appear: render,
    },
    stream: {
      press({ settings }) {
        connected();
        return toggleTo(settings.mode, !!meld().isStreaming, () => call("toggleStream"));
      },
      appear: render,
    },
    layer: {
      async press({ settings }) {
        const [id, it] = item(settings.layer, "layer");
        await toggleTo(settings.mode, !!it.visible, () => call("toggleLayer", it.parent ?? "", id));
      },
      options: () => choices("layer"),
      appear: render,
    },
    effect: {
      async press({ settings }) {
        const [id, it] = item(settings.effect, "effect");
        const layer = items()[it.parent ?? ""];
        if (!layer) throw new Error("That effect's layer is gone");
        await toggleTo(settings.mode, !!it.enabled, () => call("toggleEffect", layer.parent ?? "", it.parent ?? "", id));
      },
      options: () => choices("effect"),
      appear: render,
    },
    mute: {
      async press({ settings }) {
        const [id, it] = item(settings.track, "track");
        await toggleTo(settings.mode, !!it.muted, () => call("toggleMute", id));
      },
      options: () => choices("track"),
      appear: render,
    },
    monitor: {
      async press({ settings }) {
        const [id, it] = item(settings.track, "track");
        await toggleTo(settings.mode, !!it.monitoring, () => call("toggleMonitor", id));
      },
      options: () => choices("track"),
      appear: render,
    },
    gain: {
      async slide({ settings, value }) {
        const [id] = item(settings.track, "track");
        const v = clamp01(value);
        gains.set(id, v);
        await call("setGain", id, v);
      },
      options: () => choices("track"),
      appear: render,
    },
    command: {
      async press({ settings }) {
        const name = String(settings.command ?? "screenshot");
        if (!Object.hasOwn(COMMANDS, name)) throw new Error("Unknown command");
        await call("sendCommand", COMMANDS[name]);
      },
    },
  },
});

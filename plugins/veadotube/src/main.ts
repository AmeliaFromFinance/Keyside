// veadotube over its WebSocket API ("nodes" channel, stateEvents node).

import { definePlugin, setting, type Host, type Key } from "@keyside/sdk";
import { Reconnecting } from "../../_lib/socket.ts";

let host: Host;
let sock: Reconnecting;
let node: string | null = null;
let ready = false;
let current = "";
let states: Array<{ id: string; name: string }> = [];
const thumbs = new Map<string, string>();
let peekTimer: ReturnType<typeof setInterval> | null = null;
let pendingOptions: ((list: Array<{ id: string; name: string }>) => void) | null = null;

function send(msg: unknown) {
  sock.send(`nodes:${JSON.stringify(msg)}`);
}

function toNode(payload: Record<string, unknown>) {
  if (!node) throw new Error("veadotube has no avatar states to control");
  send({ event: "payload", type: "stateEvents", id: node, payload });
}

function onMessage(raw: string) {
  const i = raw.indexOf(":");
  if (i < 0 || raw.slice(0, i).trim() !== "nodes") return;
  const m = JSON.parse(raw.slice(i + 1));
  if (m.event === "list" && Array.isArray(m.entries)) {
    const n = m.entries.find((e: any) => e.type === "stateEvents");
    if (n && n.id !== node) {
      node = String(n.id);
      toNode({ event: "listen", token: "keyside" });
      toNode({ event: "list" });
      toNode({ event: "peek" });
    }
    if (!n) host.status("error", "This veadotube instance has no avatar states");
    return;
  }
  if (m.event !== "payload" || m.type !== "stateEvents") return;
  const p = m.payload ?? {};
  if (p.event === "list" && Array.isArray(p.states)) {
    states = p.states.map((s: any) => ({ id: String(s.id), name: String(s.name ?? s.id) }));
    pendingOptions?.(states);
    pendingOptions = null;
    for (const s of states) if (!thumbs.has(s.id) && wantsThumb(s.id)) toNode({ event: "thumb", state: s.id });
  } else if (p.event === "peek") {
    current = String(p.state ?? "");
    host.setVar("state", states.find((s) => s.id === current)?.name ?? current);
  } else if (p.event === "thumb" && typeof p.png === "string" && p.png.length < 300_000 && /^[A-Za-z0-9+/=]+$/.test(p.png)) {
    thumbs.set(String(p.state), `data:image/png;base64,${p.png}`);
  }
  render();
}

const wantsThumb = (id: string) => host.keys("state").some((k) => k.settings.state === id && k.settings.thumbnail !== false);

function view(k: Key) {
  if (!ready) return { disabled: true };
  if (k.action !== "state") return null;
  const id = String(k.settings.state ?? "");
  if (states.length && !states.some((s) => s.id === id)) return { disabled: true, sub: "Not in veadotube" };
  const image = k.settings.thumbnail !== false ? thumbs.get(id) : undefined;
  return id === current ? { tone: "accent" as const, badge: "ON", badgeTone: "accent" as const, image } : image ? { image } : null;
}

function render() {
  for (const k of host.keys()) {
    const v = view(k);
    if (v) k.set(v);
    else k.clear();
  }
}

function connect() {
  sock?.stop();
  ready = false;
  node = null;
  sock = new Reconnecting(
    () => {
      const h = String(setting(host.connection, "host", "127.0.0.1")).trim();
      const port = Number(host.connection.port ?? 0);
      return h && port ? `ws://${h.includes(":") && !h.startsWith("[") ? `[${h}]` : h}:${port}?n=Keyside` : null;
    },
    {
      open() {
        ready = true;
        sock.healthy();
        host.status("connected");
        send({ event: "listen", token: "keyside" });
        send({ event: "list" });
      },
      message: onMessage,
      close(reason) {
        ready = false;
        node = null;
        host.status("offline", host.connection.port ? `Can't reach veadotube (${reason})` : "Add veadotube's WebSocket port in the plugin settings.");
        render();
      },
    },
  );
  sock.start();
}

function connected() {
  if (!ready || !node) throw new Error("veadotube isn't connected");
}

export default definePlugin({
  start(h) {
    host = h;
    connect();
    // State changes made in veadotube itself don't always arrive as events, so check now and then.
    peekTimer = setInterval(() => ready && node && toNode({ event: "peek" }), 2000);
  },
  connectionChanged: connect,
  stop() {
    if (peekTimer) clearInterval(peekTimer);
    sock?.stop();
  },
  actions: {
    state: {
      press({ settings }) {
        connected();
        const op = String(settings.op ?? "set");
        toNode({ event: op === "hold" ? "push" : op, state: String(settings.state) });
        toNode({ event: "peek" });
      },
      release({ settings }) {
        if (settings.op !== "hold" || !ready || !node) return;
        toNode({ event: "pop", state: String(settings.state) });
        toNode({ event: "peek" });
      },
      options() {
        connected();
        return new Promise((resolve) => {
          pendingOptions = (list) => resolve(list.map((s) => ({ value: s.id, label: s.name })));
          toNode({ event: "list" });
          setTimeout(() => resolve(states.map((s) => ({ value: s.id, label: s.name }))), 3000);
        });
      },
      appear(k) {
        const id = String(k.settings.state ?? "");
        if (ready && node && k.settings.thumbnail !== false && id && !thumbs.has(id)) toNode({ event: "thumb", state: id });
        render();
      },
    },
    clear: {
      press() {
        connected();
        toNode({ event: "clear" });
        toNode({ event: "peek" });
      },
    },
  },
});

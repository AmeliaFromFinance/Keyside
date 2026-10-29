// Streamer.bot over its WebSocket server.
//
// Globals show up as {sb.<name>}. A Streamer.bot action can also push a value with the
// "WebSocket Server Broadcast" sub-action: {"phonedeck":{"var":"hype","value":3}} sets {sb.hype}.

import { definePlugin, setting, type Host, type Key } from "@phonedeck/sdk";
import { Reconnecting, Waiters } from "../../_lib/socket.ts";
import { b64, randomId, sha256 } from "../../_lib/util.ts";

const GLOBALS_EVERY = 15_000;

let host: Host;
let sock: Reconnecting;
const waiting = new Waiters<any>();
let actions: Array<{ id: string; name: string; enabled: boolean }> = [];
let ready = false;
let globalsTimer: ReturnType<typeof setInterval> | null = null;

function url(): string | null {
  const h = String(setting(host.connection, "host", "127.0.0.1")).trim();
  const port = Number(setting(host.connection, "port", 8080));
  let path = String(setting(host.connection, "endpoint", "/")).trim();
  if (!path.startsWith("/")) path = `/${path}`;
  if (!h) return null;
  return `ws://${h.includes(":") && !h.startsWith("[") ? `[${h}]` : h}:${port}${encodeURI(path)}`;
}

function request(name: string, body: Record<string, unknown> = {}): Promise<any> {
  const id = randomId();
  const p = waiting.wait(id);
  sock.send(JSON.stringify({ request: name, id, ...body }));
  return p;
}

/** Streamer.bot variable names become {sb.name}. Anything unusual is folded to _. */
export function varName(name: string): string | null {
  const clean = name
    .toLowerCase()
    .replace(/[^a-z0-9_]+/g, "_")
    .replace(/^_+|_+$/g, "")
    .slice(0, 48);
  return /^[a-z][a-z0-9_]*$/.test(clean) ? clean : null;
}

function setGlobal(name: string, value: unknown) {
  const n = varName(name);
  if (!n || n === "connected") return;
  const v = typeof value === "string" || typeof value === "number" || typeof value === "boolean" ? value : value == null ? null : JSON.stringify(value).slice(0, 500);
  host.setVar(n, typeof v === "string" ? v.slice(0, 500) : v);
}

async function loadGlobals() {
  if (!ready) return;
  const r = await request("GetGlobals", { persisted: true }).catch(() => null);
  const vars = r?.variables ?? {};
  for (const [name, entry] of Object.entries<any>(vars)) setGlobal(entry?.name ?? name, entry?.value ?? entry);
}

async function loadActions() {
  const r = await request("GetActions");
  actions = (r.actions ?? []).map((a: any) => ({ id: String(a.id), name: String(a.name), enabled: a.enabled !== false }));
  render();
}

function find(s: Record<string, unknown>) {
  const name = String(s.actionName ?? "");
  return actions.find((a) => a.id === name) ?? actions.find((a) => a.name === name);
}

function render() {
  for (const k of host.keys("run")) {
    if (!ready) k.set({ disabled: true });
    else {
      const a = find(k.settings);
      if (!a) k.set({ disabled: true, sub: "Not in Streamer.bot" });
      else if (!a.enabled) k.set({ disabled: true, sub: "Turned off in Streamer.bot" });
      else k.clear();
    }
  }
  for (const k of host.keys("trigger")) (ready ? k.clear() : k.set({ disabled: true }));
}

const text = (v: unknown) => (typeof v === "string" ? v : "");

function onEvent(source: string, type: string, d: any) {
  if (source === "General" && type === "Custom") {
    const p = d?.phonedeck ?? d?.data?.phonedeck;
    if (p && typeof p.var === "string") setGlobal(p.var, p.value);
    return;
  }
  if (source === "Misc" && /GlobalVariable/.test(type)) {
    if (d?.name !== undefined) setGlobal(String(d.name), d.newValue ?? d.value);
    else void loadGlobals();
    return;
  }
  if (!setting(host.connection, "chat", true)) return;
  const user = d?.user ?? {};
  const msg = d?.message ?? {};
  const who = text(msg.displayName) || text(user.name) || text(user.login) || text(d?.displayName) || text(d?.userName) || text(d?.user_name);
  if (type === "ChatMessage" || (source === "YouTube" && type === "Message") || (source === "Kick" && type === "ChatMessage")) {
    const body = text(msg.message) || text(d?.text) || text(d?.message);
    if (body) host.feed("chat", { author: who, text: body.slice(0, 500), color: /^#[0-9a-f]{6}$/i.test(msg.color ?? "") ? msg.color : undefined });
    return;
  }
  const alerts: Record<string, string> = {
    Follow: "followed",
    Sub: "subscribed",
    ReSub: "resubscribed",
    GiftSub: "gifted a sub",
    GiftBomb: "gifted subs",
    Cheer: "cheered",
    Raid: "raided",
    NewSubscriber: "subscribed",
    SuperChat: "sent a Super Chat",
    NewSponsor: "became a member",
  };
  if (alerts[type] && who) {
    const extra = d?.bits ? ` ${d.bits} bits` : d?.viewers ? ` with ${d.viewers}` : d?.gifts ? ` (${d.gifts})` : "";
    host.feed("events", { author: who, text: `${alerts[type]}${extra}` });
  }
}

function onMessage(raw: string) {
  const m = JSON.parse(raw);
  if (m.request === "Hello") {
    void hello(m);
    return;
  }
  if (m.id !== undefined && m.status !== undefined) {
    if (m.status === "ok") waiting.resolve(String(m.id), m);
    else waiting.reject(String(m.id), new Error(m.error || "Streamer.bot refused the request"));
    return;
  }
  if (m.event) onEvent(String(m.event.source ?? ""), String(m.event.type ?? ""), m.data ?? {});
}

async function hello(m: any) {
  try {
    if (m.authentication) {
      const password = String(host.connection.password ?? "");
      if (!password) throw new Error("Streamer.bot asks for a password. Add it in the plugin settings.");
      const secret = b64(await sha256(password + m.authentication.salt));
      await request("Authenticate", { authentication: b64(await sha256(secret + m.authentication.challenge)) }).catch(() => {
        throw new Error("Wrong Streamer.bot password.");
      });
    }
    ready = true;
    sock.healthy();
    await loadActions();
    const events: Record<string, string[]> = { General: ["Custom"], Misc: ["GlobalVariableUpdated"] };
    if (setting(host.connection, "chat", true)) {
      Object.assign(events, {
        Twitch: ["ChatMessage", "Follow", "Sub", "ReSub", "GiftSub", "GiftBomb", "Cheer", "Raid"],
        YouTube: ["Message", "NewSubscriber", "SuperChat", "NewSponsor"],
      });
    }
    await request("Subscribe", { events }).catch(() => request("Subscribe", { events: { General: ["Custom"] } }));
    await loadGlobals();
    host.setVar("connected", true);
    host.status("connected", m.info?.version ? `Streamer.bot ${m.info.version}` : undefined);
  } catch (e) {
    ready = false;
    host.status("error", e instanceof Error ? e.message : String(e));
    sock.stop();
  }
}

async function run(s: Record<string, unknown>) {
  if (!ready) throw new Error("Streamer.bot isn't connected");
  const a = find(s);
  if (!a) throw new Error(`No action named "${s.actionName}" in Streamer.bot`);
  await request("DoAction", { action: { id: a.id, name: a.name }, args: s.args ?? {} });
}

function connect() {
  sock?.stop();
  ready = false;
  sock = new Reconnecting(url, {
    open() {
      host.status("connecting");
    },
    message: onMessage,
    close(reason) {
      ready = false;
      waiting.failAll(new Error("Streamer.bot disconnected"));
      host.setVar("connected", false);
      host.status("offline", `Can't reach Streamer.bot (${reason}). Is its WebSocket server on?`);
      render();
    },
  });
  sock.start();
}

export default definePlugin({
  start(h) {
    host = h;
    connect();
    globalsTimer = setInterval(() => void loadGlobals(), GLOBALS_EVERY);
  },
  connectionChanged: connect,
  stop() {
    if (globalsTimer) clearInterval(globalsTimer);
    sock?.stop();
  },
  actions: {
    run: {
      press: ({ settings }) => run(settings),
      async options() {
        if (!ready) throw new Error("Streamer.bot isn't connected");
        await loadActions();
        return actions.map((a) => ({ value: a.name, label: a.enabled ? a.name : `${a.name} (off)` }));
      },
      appear: (_k: Key) => render(),
    },
    trigger: {
      async press({ settings }) {
        if (!ready) throw new Error("Streamer.bot isn't connected");
        await request("ExecuteCodeTrigger", { triggerName: settings.triggerName, args: settings.args ?? {} });
      },
      appear: () => render(),
    },
  },
});

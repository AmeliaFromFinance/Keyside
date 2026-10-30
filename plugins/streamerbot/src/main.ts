// Streamer.bot over its WebSocket server.
//
// Globals show up as {sb.<name>}. A Streamer.bot action can also push a value with the
// "WebSocket Server Broadcast" sub-action: {"keyside":{"var":"hype","value":3}} sets {sb.hype}.

import { definePlugin, setting, type Host, type Key } from "@keyside/sdk";
import { Reconnecting, Waiters } from "../../_lib/socket.ts";
import { b64, randomId, sha256 } from "../../_lib/util.ts";

const GLOBALS_EVERY = 15_000;

let host: Host;
let sock: Reconnecting;
const waiting = new Waiters<any>();
let actions: Array<{ id: string; name: string; enabled: boolean }> = [];
let ready = false;
let greeted = false;
let globalsTimer: ReturnType<typeof setInterval> | null = null;
/** Globals by their Streamer.bot name, for add and toggle. */
const globals = new Map<string, unknown>();

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
  return waiting.request(id, () => sock.send(JSON.stringify({ request: name, id, ...body })));
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

/** `pushed` values come from a broadcast, not from Streamer.bot's globals. */
function setGlobal(name: string, value: unknown, pushed = false) {
  if (!pushed && (globals.size < 1000 || globals.has(name))) globals.set(name, value);
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

type Role = "broadcaster" | "mod" | "vip" | "sub";

/**
 * Chat roles from a Streamer.bot event, for chat triggers. Twitch sends a role number
 * (2 VIP, 3 moderator, 4 broadcaster), badges and a subscribed flag; YouTube and Kick send flags.
 */
export function roles(d: any): Role[] {
  const out = new Set<Role>();
  const user = d?.user ?? {};
  const msg = typeof d?.message === "object" && d.message ? d.message : {};
  const role = Number(user.role ?? msg.role ?? 0);
  if (role >= 4) out.add("broadcaster");
  else if (role === 3) out.add("mod");
  else if (role === 2) out.add("vip");
  const yes = (...v: unknown[]) => v.some((x) => x === true);
  if (yes(user.subscribed, msg.subscriber, user.isSubscribed, user.isSubscriber, user.isSponsor, user.isMember)) out.add("sub");
  if (yes(user.isModerator, msg.isModerator, user.isMod)) out.add("mod");
  if (yes(user.isBroadcaster, user.isOwner, msg.isBroadcaster)) out.add("broadcaster");
  if (yes(user.isVip, msg.isVip)) out.add("vip");
  for (const b of [...(Array.isArray(user.badges) ? user.badges : []), ...(Array.isArray(msg.badges) ? msg.badges : [])]) {
    const name = String(b?.name ?? b?.setId ?? b ?? "").toLowerCase();
    if (name === "broadcaster") out.add("broadcaster");
    else if (name === "moderator") out.add("mod");
    else if (name === "vip") out.add("vip");
    else if (name === "subscriber" || name === "founder") out.add("sub");
  }
  return [...out];
}

const PLATFORMS = new Set(["twitch", "youtube", "kick"]);

function onEvent(source: string, type: string, d: any) {
  if (source === "General" && type === "Custom") {
    const p = d?.keyside ?? d?.data?.keyside;
    if (p && typeof p.var === "string") setGlobal(p.var, p.value, true);
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
  const who =
    text(msg.displayName) ||
    text(user.name) ||
    text(user.login) ||
    text(d?.displayName) ||
    text(d?.userName) ||
    text(d?.user_name) ||
    text(d?.from_broadcaster_user_name);
  const platform = PLATFORMS.has(source.toLowerCase()) ? source.toLowerCase() : undefined;
  if (type === "ChatMessage" || (source === "YouTube" && type === "Message")) {
    const body = text(msg.message) || text(d?.text) || text(d?.message);
    if (body) host.feed("chat", { author: who, text: body.slice(0, 500), color: /^#[0-9a-f]{6}$/i.test(msg.color ?? "") ? msg.color : undefined, kind: "chat", source: platform, roles: roles(d) });
    return;
  }
  if (type === "RewardRedemption") {
    const title = (text(d?.reward?.title) || text(d?.rewardTitle) || text(d?.title)).slice(0, 100);
    const input = text(d?.user_input) || text(d?.userInput) || text(d?.input);
    if (title && who) host.feed("events", { author: who, text: `redeemed ${title}`, kind: "redeem", source: platform, reward: title, input: input.slice(0, 500) });
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
    Subscription: "subscribed",
    Resubscription: "resubscribed",
    GiftSubscription: "gifted a sub",
    MassGiftSubscription: "gifted subs",
  };
  if (alerts[type] && who) {
    const extra = d?.bits ? ` ${d.bits} bits` : d?.viewers ? ` with ${d.viewers}` : d?.gifts ? ` (${d.gifts})` : "";
    const kind = type === "Follow" ? "follow" : type === "Cheer" ? "cheer" : type === "Raid" ? "raid" : type === "SuperChat" ? "alert" : "sub";
    host.feed("events", { author: who, text: `${alerts[type]}${extra}`, kind, source: platform });
  }
}

function onMessage(raw: string) {
  const m = JSON.parse(raw);
  if (m.request === "Hello") {
    if (!greeted) {
      greeted = true;
      void hello(m);
    }
    return;
  }
  if (m.id !== undefined && m.status !== undefined) {
    if (m.status === "ok") waiting.resolve(String(m.id), m);
    else waiting.reject(String(m.id), new Error(m.error || "Streamer.bot refused the request"));
    return;
  }
  if (m.event) onEvent(String(m.event.source ?? ""), String(m.event.type ?? ""), m.data ?? {});
}

/** Streamer.bot 0.2.4 and older don't send Hello. Ask for the info instead. */
async function helloFallback(s: Reconnecting) {
  if (greeted || s !== sock || !sock.ready) return;
  const r = await request("GetInfo").catch(() => null);
  if (greeted || s !== sock || !r?.info) return;
  greeted = true;
  await hello({ info: r.info });
}

async function hello(m: any) {
  try {
    if (m.authentication) {
      const password = String(host.connection.password ?? "");
      if (password) {
        const secret = b64(await sha256(password + m.authentication.salt));
        await request("Authenticate", { authentication: b64(await sha256(secret + m.authentication.challenge)) }).catch(() => {
          throw new Error("Wrong Streamer.bot password.");
        });
      } else {
        // Auth may be on without "Enforce". Then ordinary requests still work.
        await request("GetInfo").catch(() => {
          throw new Error("Streamer.bot asks for a password. Add it in the plugin settings.");
        });
      }
    }
    ready = true;
    sock.healthy();
    await loadActions();
    const events: Record<string, string[]> = { General: ["Custom"], Misc: ["GlobalVariableUpdated"] };
    if (setting(host.connection, "chat", true)) {
      Object.assign(events, {
        Twitch: ["ChatMessage", "Follow", "Sub", "ReSub", "GiftSub", "GiftBomb", "Cheer", "Raid", "RewardRedemption"],
        YouTube: ["Message", "NewSubscriber", "SuperChat", "NewSponsor"],
        Kick: ["ChatMessage", "Follow", "Subscription", "Resubscription", "GiftSubscription", "MassGiftSubscription"],
      });
    }
    // One request per source, so an event this version doesn't know can't block the rest.
    for (const [source, types] of Object.entries(events)) {
      await request("Subscribe", { events: { [source]: types } }).catch(() => {});
    }
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

/** The value a "Set global variable" key writes. */
export function nextValue(op: unknown, current: unknown, input: string): string | number | boolean {
  if (op === "toggle") return !(current === true || current === "true" || current === "True" || (typeof current === "number" && current !== 0));
  if (op === "add") {
    const n = Number(input.trim() || "1");
    if (!Number.isFinite(n)) throw new Error("Add needs a number");
    const now = Number(current ?? 0);
    return (Number.isFinite(now) ? now : 0) + n;
  }
  const t = input.trim();
  if (t === "true" || t === "false") return t === "true";
  return /^-?\d+(\.\d+)?$/.test(t) ? Number(t) : input;
}

async function setRemote(s: Record<string, unknown>) {
  if (!ready) throw new Error("Streamer.bot isn't connected");
  const name = String(s.name ?? "").trim();
  if (!name || name.length > 100) throw new Error("Pick a global variable");
  const helperName = String(s.helper ?? "Keyside: Set global").trim();
  const helper = actions.find((a) => a.name === helperName);
  if (!helper) throw new Error(`Add the "${helperName}" action to Streamer.bot first. The plugin setup guide has it.`);
  if (s.op === "add" || s.op === "toggle") {
    const r = await request("GetGlobal", { variable: name, persisted: s.persisted !== false }).catch(() => null);
    // Streamer.bot answers { variables: { "<name>": { name, value, lastWrite } } }.
    const entry = r?.variables?.[name] ?? Object.values<any>(r?.variables ?? {})[0];
    if (entry && "value" in entry) globals.set(name, entry.value);
    else if (r && "value" in r) globals.set(name, r.value);
  }
  const value = nextValue(s.op, globals.get(name), String(s.value ?? ""));
  await request("DoAction", { action: { id: helper.id, name: helper.name }, args: { keysideName: name, keysideValue: value, keysidePersisted: s.persisted !== false } });
  setGlobal(name, value);
}

function connect() {
  sock?.stop();
  ready = false;
  globals.clear();
  sock = new Reconnecting(url, {
    open() {
      greeted = false;
      host.status("connecting");
      const s = sock;
      setTimeout(() => void helloFallback(s), 1500);
    },
    message: onMessage,
    close(reason) {
      ready = false;
      greeted = false;
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
    "set-global": {
      press: ({ settings }) => setRemote(settings),
      async options() {
        if (!ready) throw new Error("Streamer.bot isn't connected");
        await loadGlobals();
        return [...globals.keys()].sort().map((n) => ({ value: n, label: n }));
      },
    },
  },
});

// StreamElements: alerts arrive over its Astro WebSocket, and the overlay is controlled through
// its REST API. Both use the channel's JWT token.

import { definePlugin, setting, type Host, type Key, type KeyView } from "@keyside/sdk";
import { Reconnecting } from "../../_lib/socket.ts";
import { HttpError, errorText, http, randomId, want } from "../../_lib/util.ts";

export const endpoints = {
  api: "https://api.streamelements.com/kappa/v2",
  astro: "wss://astro.streamelements.com",
};

const TOPICS = ["channel.activities", "channel.overlay.action"];
const KEEP = 20;
const ID = /^[0-9a-f]{24}$/i;

interface Activity {
  id: string;
  type: string;
}

let host: Host;
let sock: Reconnecting | null = null;
let generation = 0;
let channel = "";
let reconnectToken = "";
/** Newest first, for replay keys. */
let recent: Activity[] = [];
/** Overlay state, as far as we've seen it change. */
const overlay = { paused: false, muted: false };

const token = () => String(setting(host.connection, "token", "")).trim();

async function api(method: string, path: string): Promise<any> {
  try {
    return await http(`${endpoints.api}${path}`, { method, headers: { authorization: `Bearer ${token()}`, accept: "application/json" } });
  } catch (e) {
    if (e instanceof HttpError && (e.status === 401 || e.status === 403)) throw new Error("StreamElements refused the JWT token. Copy it again from the dashboard.");
    throw e;
  }
}

function money(amount: unknown, currency: unknown): string {
  const n = Number(amount);
  if (!Number.isFinite(n)) return "";
  try {
    return new Intl.NumberFormat("en", { style: "currency", currency: String(currency || "USD") }).format(n);
  } catch {
    return `${n} ${String(currency ?? "")}`.trim();
  }
}

const clip = (s: unknown, n = 200) => String(s ?? "").slice(0, n);

/** One activity from the feed or the API: shows it and remembers it for replay. */
export function describe(a: any): { who: string; text: string } | null {
  const d = a?.data ?? {};
  const who = clip(d.displayName || d.username || d.name, 60);
  const msg = d.message ? `: ${clip(d.message, 300)}` : "";
  switch (a?.type) {
    case "follow":
    case "follower":
      return { who, text: "followed" };
    case "subscriber":
    case "sponsor":
      if (d.gifted && d.sender) return { who: clip(d.sender, 60), text: `gifted a sub to ${who}` };
      return { who, text: Number(d.amount) > 1 ? `subscribed for ${Number(d.amount)} months${msg}` : `subscribed${msg}` };
    case "communityGiftPurchase":
      return { who, text: Number(d.amount) > 0 ? `gifted ${Number(d.amount)} subs` : "gifted subs" };
    case "tip":
    case "superchat":
      return { who, text: `${a.type === "tip" ? "tipped" : "sent a Super Chat"} ${money(d.amount, d.currency)}${msg}` };
    case "cheer":
      return { who, text: `cheered ${Number(d.amount) || 0} bits${msg}` };
    case "raid":
    case "host":
      return { who, text: `${a.type === "raid" ? "raided" : "hosted"} with ${Number(d.amount) || 0} viewers` };
    case "redemption":
    case "channelPointsRedemption":
      return { who, text: `redeemed ${clip(d.redemption || d.item || d.reward?.title || "a reward", 80)}` };
    case "merch":
    case "purchase":
      return { who, text: "bought merch" };
    default:
      return a?.type ? { who, text: clip(a.type, 40) } : null;
  }
}

function remember(a: any) {
  const id = String(a?._id ?? a?.activityId ?? "");
  if (!ID.test(id) || recent.some((r) => r.id === id)) return;
  recent = [{ id, type: String(a.type) }, ...recent].slice(0, KEEP);
}

function onActivity(a: any) {
  remember(a);
  const shown = describe(a);
  if (!shown) return;
  host.feed("events", { author: shown.who, text: shown.text });
  const d = a.data ?? {};
  if (a.type === "tip") host.setVar("last_tip", `${shown.who} ${money(d.amount, d.currency)}`.trim());
  if (a.type === "follower" || a.type === "follow") host.setVar("last_follower", shown.who);
  if (a.type === "subscriber" || a.type === "sponsor") host.setVar("last_sub", shown.who);
  if (a.type === "cheer") host.setVar("last_cheer", `${shown.who} ${Number(d.amount) || 0}`);
  render();
}

function onOverlay(action: unknown) {
  if (action === "pause" || action === "unpause") overlay.paused = action === "pause";
  else if (action === "mute" || action === "unmute") overlay.muted = action === "mute";
  else return;
  render();
}

function onMessage(raw: string) {
  const m = JSON.parse(raw);
  switch (m?.type) {
    case "welcome":
      for (const topic of TOPICS) sock?.send(JSON.stringify({ type: "subscribe", nonce: randomId(), data: { topic, token: token(), token_type: "jwt" } }));
      return;
    case "response":
      if (m.error) host.status("error", m.error === "err_unauthorized" ? "StreamElements refused the JWT token. Copy it again from the dashboard." : `StreamElements: ${clip(m.data?.message || m.error, 120)}`);
      return;
    case "reconnect":
      // The server is restarting. Its token brings our subscriptions back on the next server.
      reconnectToken = String(m.data?.reconnect_token ?? "");
      sock?.drop("server restart");
      return;
    case "message":
      if (m.topic === "channel.activities") onActivity(m.data);
      else if (m.topic === "channel.overlay.action") onOverlay(m.data?.action);
      return;
  }
}

function view(k: Key): KeyView | null {
  if (!channel) return { disabled: true };
  if (k.action === "pause" && overlay.paused) return { tone: "accent", badge: "PAUSED", badgeTone: "accent" };
  if (k.action === "mute" && overlay.muted) return { tone: "danger", badge: "MUTED", badgeTone: "danger" };
  if (k.action === "replay") {
    const which = k.settings.which === "tip" ? recent.find((r) => r.type === "tip") : recent[0];
    return which ? null : { sub: "Nothing to replay yet" };
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

async function start(gen: number) {
  sock?.stop();
  sock = null;
  channel = "";
  recent = [];
  render();
  if (!token()) {
    host.status("offline", "Paste your StreamElements JWT token in the plugin settings.");
    return;
  }
  try {
    const me = await api("GET", "/channels/me");
    if (gen !== generation) return;
    if (!ID.test(String(me?._id ?? ""))) throw new Error("StreamElements didn't say which channel this is");
    channel = String(me._id);
    host.status("connected", clip(me.displayName || me.username, 60) || undefined);
    // Recent alerts, so replay works straight away.
    const past = await api("GET", `/activities/${channel}?limit=${KEEP}&after=${new Date(Date.now() - 7 * 864e5).toISOString()}&before=${new Date().toISOString()}`).catch(() => []);
    if (gen !== generation) return;
    for (const a of (Array.isArray(past) ? past : []).slice().reverse()) remember(a);
  } catch (e) {
    if (gen === generation) host.status("error", errorText(e));
    return;
  }
  sock = new Reconnecting(
    () => {
      const t = reconnectToken;
      reconnectToken = "";
      return t ? `${endpoints.astro}/?reconnect_token=${encodeURIComponent(t)}` : endpoints.astro;
    },
    {
      open() {
        sock?.healthy();
        host.status("connected");
      },
      message: onMessage,
      close(reason) {
        if (gen === generation) host.status("connecting", `Reconnecting to StreamElements (${reason})`);
      },
    },
    { maxDelay: 30_000 },
  );
  sock.start();
  render();
}

function restart() {
  void start(++generation);
}

function ready() {
  if (!channel) throw new Error("StreamElements isn't connected");
  return channel;
}

async function overlayAction(action: string) {
  await api("PUT", `/overlays/${ready()}/action/${action}`);
}

export default definePlugin({
  start(h) {
    host = h;
    restart();
  },
  connectionChanged: restart,
  stop() {
    generation++;
    sock?.stop();
  },
  actions: {
    skip: {
      press: () => overlayAction("skip"),
    },
    pause: {
      async press({ settings }) {
        const paused = want(settings.mode ?? "toggle", overlay.paused);
        await overlayAction(paused ? "pause" : "unpause");
        onOverlay(paused ? "pause" : "unpause");
      },
      appear: render,
    },
    mute: {
      async press({ settings }) {
        const muted = want(settings.mode ?? "toggle", overlay.muted);
        await overlayAction(muted ? "mute" : "unmute");
        onOverlay(muted ? "mute" : "unmute");
      },
      appear: render,
    },
    replay: {
      async press({ settings }) {
        const ch = ready();
        const a = settings.which === "tip" ? recent.find((r) => r.type === "tip") : recent[0];
        if (!a) throw new Error(settings.which === "tip" ? "No tips to replay yet" : "No alerts to replay yet");
        await api("POST", `/activities/${ch}/${a.id}/replay`);
      },
      appear: render,
    },
  },
});

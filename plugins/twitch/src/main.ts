// Twitch through Helix and EventSub. Signs in with the device code flow, so no client
// secret or redirect server is needed.

import { definePlugin, setting, type Host, type Key, type KeyView, type Step } from "@keyside/sdk";
import { Reconnecting } from "../../_lib/socket.ts";
import { Session, fromResponse, pollDeviceCode } from "../../_lib/session.ts";
import { HttpError, errorText, firstTime, http, want } from "../../_lib/util.ts";

/** Overridden in tests. */
export const endpoints = {
  id: "https://id.twitch.tv/oauth2",
  api: "https://api.twitch.tv/helix",
  eventsub: "wss://eventsub.wss.twitch.tv/ws",
};

export const SCOPES = [
  "user:read:chat",
  "user:write:chat",
  "channel:manage:broadcast",
  "clips:edit",
  "channel:edit:commercial",
  "channel:read:ads",
  "channel:manage:ads",
  "moderator:manage:chat_settings",
  "moderator:manage:announcements",
  "moderator:manage:chat_messages",
  "moderator:manage:shoutouts",
  "moderator:read:followers",
  "channel:read:subscriptions",
  "channel:manage:raids",
  "bits:read",
  "channel:manage:polls",
  "channel:manage:predictions",
  "channel:manage:redemptions",
  "moderator:manage:shield_mode",
].join(" ");

const POLL_EVERY = 60_000;

interface ChatSettings {
  emote_mode?: boolean;
  slow_mode?: boolean;
  slow_mode_wait_time?: number | null;
  follower_mode?: boolean;
  follower_mode_duration?: number | null;
  subscriber_mode?: boolean;
  unique_chat_mode?: boolean;
}

let host: Host;
let session: Session;
let me: { id: string; login: string; name: string } | null = null;
let events: Reconnecting | null = null;
let generation = 0;
let pollTimer: ReturnType<typeof setInterval> | null = null;
let adTimer: ReturnType<typeof setInterval> | null = null;
let watchdog: ReturnType<typeof setTimeout> | null = null;
let reconnectUrl: string | null = null;
let signInPage: string | null = null;

const live = { on: false, viewers: 0 };
let chat: ChatSettings = {};
const channel = { title: "", category: "" };
const ads = { until: 0, length: 0, next: 0, snoozes: 0 };
let shield = false;

interface Choice {
  id: string;
  title: string;
  votes: number;
}
/** The poll or prediction that's running, or just ended. */
let poll: { id: string; title: string; ends: number; choices: Choice[]; done: boolean } | null = null;
let prediction: { id: string; title: string; ends: number; state: "active" | "locked" | "done"; outcomes: Choice[] } | null = null;
/** Channel point rewards Keyside created, by title. Twitch only lets an app change its own rewards. */
const rewards = new Map<string, { id: string; enabled: boolean; paused: boolean }>();

const clientId = () => String(setting(host.connection, "clientId", "")).trim();

async function api(method: string, path: string, body?: unknown, retry = true): Promise<any> {
  const token = await session.token();
  try {
    return await http(`${endpoints.api}${path}`, { method, headers: { authorization: `Bearer ${token}`, "client-id": clientId() }, json: body });
  } catch (e) {
    if (e instanceof HttpError && e.status === 401 && retry) {
      await session.renew();
      return api(method, path, body, false);
    }
    throw e;
  }
}

/** Twitch sends ad times as Unix seconds in some fields and RFC3339 in others. Epoch ms, or 0. */
export function stamp(v: unknown): number {
  if (v === null || v === undefined || v === "" || v === 0 || v === "0") return 0;
  const n = Number(v);
  if (Number.isFinite(n)) return n < 1e12 ? n * 1000 : n;
  return Date.parse(String(v)) || 0;
}

const q = (o: Record<string, string | number>) => `?${new URLSearchParams(Object.entries(o).map(([k, v]) => [k, String(v)]))}`;

async function signIn(gen: number) {
  const client = clientId();
  const dev = await http(`${endpoints.id}/device`, { form: { client_id: client, scopes: SCOPES } });
  signInPage = String(dev.verification_uri ?? "https://www.twitch.tv/activate");
  host.status("connecting", `Sign in: open twitch.tv/activate and enter ${dev.user_code}`);
  host.toast(`Twitch code: ${dev.user_code}`);
  if (signInPage.startsWith("https://") && (await firstTime(host.dataDir, `twitch-${client}`))) await host.openBrowser(signInPage).catch(() => {});
  const tokens = await pollDeviceCode(
    () => http(`${endpoints.id}/token`, { form: { client_id: client, scopes: SCOPES, device_code: dev.device_code, grant_type: "urn:ietf:params:oauth:grant-type:device_code" } }),
    Number(dev.interval),
    Number(dev.expires_in),
    () => gen !== generation,
  );
  signInPage = null;
  await session.save(fromResponse(tokens, client));
  host.toast("Signed in to Twitch", "success");
}

async function start(gen: number) {
  stopAll();
  const client = clientId();
  if (!client) {
    host.status("offline", "Add your Twitch Client ID in the plugin settings.");
    return;
  }
  session = new Session(host, { refresh: (t) => http(`${endpoints.id}/token`, { form: { client_id: client, grant_type: "refresh_token", refresh_token: t.refresh ?? "" } }) });
  session.onSignedOut = () => {
    if (gen === generation) void begin();
  };
  try {
    if (session.load(client) && !session.covers(SCOPES)) {
      // Keyside asks for more than this sign-in allows (polls, predictions, rewards, shield mode).
      await session.clear();
      host.toast("Twitch needs a new sign-in for polls, predictions and rewards");
    }
    if (!session.signedIn) await signIn(gen);
    if (gen !== generation) return;
    const u = (await api("GET", "/users")).data?.[0];
    if (!u) throw new Error("Twitch didn't return your account");
    me = { id: u.id, login: u.login, name: u.display_name };
    host.status("connected", me.name);
    await Promise.all([refresh(), loadChannel(), loadChat(), loadExtras()]);
    pollTimer = setInterval(() => void refresh(), POLL_EVERY);
    connectEvents(gen);
    render();
  } catch (e) {
    if (gen !== generation) return;
    host.status("error", errorText(e));
  }
}

function begin() {
  generation++;
  return start(generation);
}

function stopAll() {
  if (pollTimer) clearInterval(pollTimer);
  if (adTimer) clearInterval(adTimer);
  if (voteTimer) clearInterval(voteTimer);
  voteTimer = null;
  if (watchdog) clearTimeout(watchdog);
  pollTimer = adTimer = watchdog = null;
  events?.stop();
  events = null;
  me = null;
}

async function refresh() {
  if (!me) return;
  const [stream, followers, schedule] = await Promise.all([
    api("GET", `/streams${q({ user_id: me.id })}`).catch(() => null),
    api("GET", `/channels/followers${q({ broadcaster_id: me.id, first: 1 })}`).catch(() => null),
    api("GET", `/channels/ads${q({ broadcaster_id: me.id })}`).catch(() => null),
  ]);
  if (stream) {
    const s = stream.data?.[0];
    live.on = !!s;
    live.viewers = s ? Number(s.viewer_count) : 0;
    host.setVar("live", live.on);
    host.setVar("viewers", live.viewers);
  }
  if (followers) host.setVar("followers", Number(followers.total ?? 0));
  const a = schedule?.data?.[0];
  if (a) {
    ads.next = stamp(a.next_ad_at);
    ads.snoozes = Number(a.snooze_count ?? 0);
  }
  render();
}

async function loadChannel() {
  if (!me) return;
  const c = (await api("GET", `/channels${q({ broadcaster_id: me.id })}`)).data?.[0];
  if (!c) return;
  channel.title = c.title ?? "";
  channel.category = c.game_name ?? "";
  host.setVar("title", channel.title);
  host.setVar("category", channel.category);
}

const choices = (list: any[] | undefined): Choice[] => (list ?? []).map((c) => ({ id: String(c.id), title: String(c.title ?? ""), votes: Number(c.votes ?? c.channel_points ?? 0) }));

/** Shield mode, the running poll and prediction, and Keyside's rewards. Some need Affiliate. */
async function loadExtras() {
  if (!me) return;
  const [sh, po, pr, rw] = await Promise.all([
    api("GET", `/moderation/shield_mode${q({ broadcaster_id: me.id, moderator_id: me.id })}`).catch(() => null),
    api("GET", `/polls${q({ broadcaster_id: me.id, first: 1 })}`).catch(() => null),
    api("GET", `/predictions${q({ broadcaster_id: me.id, first: 1 })}`).catch(() => null),
    api("GET", `/channel_points/custom_rewards${q({ broadcaster_id: me.id, only_manageable_rewards: "true" })}`).catch(() => null),
  ]);
  shield = !!sh?.data?.[0]?.is_active;
  const p = po?.data?.[0];
  poll = p && p.status === "ACTIVE" ? { id: p.id, title: p.title, ends: Date.parse(p.started_at) + Number(p.duration) * 1000, choices: choices(p.choices), done: false } : null;
  const d = pr?.data?.[0];
  prediction =
    d && (d.status === "ACTIVE" || d.status === "LOCKED")
      ? { id: d.id, title: d.title, ends: Date.parse(d.created_at) + Number(d.prediction_window) * 1000, state: d.status === "ACTIVE" ? "active" : "locked", outcomes: choices(d.outcomes) }
      : null;
  rewards.clear();
  for (const r of rw?.data ?? []) rewards.set(String(r.title).toLowerCase(), { id: r.id, enabled: !!r.is_enabled, paused: !!r.is_paused });
  if (poll || prediction) tickVotes();
}

async function loadChat() {
  if (!me) return;
  chat = (await api("GET", `/chat/settings${q({ broadcaster_id: me.id, moderator_id: me.id })}`)).data?.[0] ?? {};
}

const MODES: Record<string, keyof ChatSettings> = { emote: "emote_mode", slow: "slow_mode", followers: "follower_mode", subs: "subscriber_mode", unique: "unique_chat_mode" };

const mmss = (ms: number) => {
  const s = Math.max(0, Math.round(ms / 1000));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
};

function view(k: Key): KeyView | null {
  if (!me) return { disabled: true };
  const s = k.settings;
  switch (k.action) {
    case "marker":
    case "clip":
      return live.on ? null : { disabled: true, sub: "Offline" };
    case "ad": {
      const left = ads.until - Date.now();
      if (left > 0) return { tone: "warning", badge: "AD", badgeTone: "warning", meta: mmss(left), progress: Math.min(1, left / (ads.length * 1000 || 1)) };
      if (!live.on) return { disabled: true, sub: "Offline" };
      return ads.next > Date.now() ? { meta: `next ${Math.max(1, Math.round((ads.next - Date.now()) / 60_000))}m` } : null;
    }
    case "snooze-ad":
      return live.on && ads.snoozes > 0 ? { badge: String(ads.snoozes) } : { disabled: true };
    case "chat-mode": {
      const key = MODES[String(s.mode ?? "emote")];
      return key && chat[key] ? { tone: "accent", badge: "ON", badgeTone: "accent" } : null;
    }
    case "title":
      return s.title && s.title === channel.title ? { tone: "accent" } : null;
    case "shield":
      return shield ? { tone: "danger", badge: "SHIELD", badgeTone: "danger", icon: "shield-alert" } : null;
    case "poll": {
      if (!poll) return String(s.op ?? "start") === "end" ? { disabled: true, sub: "No poll running" } : null;
      const top = leader(poll.choices);
      return { tone: "accent", badge: poll.done ? "ENDED" : "POLL", badgeTone: poll.done ? "muted" : "accent", sub: top ? `${top.title} ${top.votes}` : poll.title, meta: poll.done ? undefined : mmss(poll.ends - Date.now()) };
    }
    case "prediction": {
      const op = String(s.op ?? "start");
      if (!prediction) return op === "start" ? null : { disabled: true, sub: "No prediction" };
      const top = leader(prediction.outcomes);
      const badge = prediction.state === "active" ? "OPEN" : prediction.state === "locked" ? "LOCKED" : "ENDED";
      return { tone: prediction.state === "done" ? undefined : "accent", badge, badgeTone: prediction.state === "active" ? "accent" : "muted", sub: top ? `${top.title} ${top.votes}` : prediction.title, meta: prediction.state === "active" ? mmss(prediction.ends - Date.now()) : undefined };
    }
    case "reward": {
      const r = rewards.get(String(s.title ?? "").trim().toLowerCase());
      if (!r) return { sub: "Not created yet" };
      return r.enabled && !r.paused ? { tone: "accent", badge: "ON", badgeTone: "accent" } : { badge: r.paused ? "PAUSED" : "OFF", badgeTone: "muted" };
    }
    default:
      return null;
  }
}

const leader = (list: Choice[]) => (list.length ? list.reduce((a, b) => (b.votes > a.votes ? b : a)) : null);

/** Keeps poll and prediction countdowns moving while one runs. */
let voteTimer: ReturnType<typeof setInterval> | null = null;
function tickVotes() {
  if (voteTimer) return;
  voteTimer = setInterval(() => {
    const running = (poll && !poll.done) || prediction?.state === "active";
    for (const k of [...host.keys("poll"), ...host.keys("prediction")]) {
      const v = view(k);
      if (v) k.set(v);
      else k.clear();
    }
    if (!running && voteTimer) {
      clearInterval(voteTimer);
      voteTimer = null;
    }
  }, 1000);
}

function render() {
  for (const k of host.keys()) {
    const v = view(k);
    if (v) k.set(v);
    else k.clear();
  }
}

function onEvent(type: string, e: any) {
  const who = String(e.user_name ?? e.chatter_user_name ?? e.from_broadcaster_user_name ?? "");
  const showChat = setting(host.connection, "chat", true);
  switch (type) {
    case "channel.chat.message":
      if (showChat) host.feed("chat", { author: who, text: String(e.message?.text ?? "").slice(0, 500), color: /^#[0-9a-f]{6}$/i.test(e.color ?? "") ? e.color : undefined });
      return;
    case "channel.chat_settings.update":
      chat = { ...chat, ...e };
      break;
    case "channel.follow":
      host.setVar("last_follower", who);
      if (showChat) host.feed("events", { author: who, text: "followed" });
      return;
    case "channel.subscribe":
      if (e.is_gift) return;
      host.setVar("last_sub", who);
      if (showChat) host.feed("events", { author: who, text: "subscribed" });
      return;
    case "channel.subscription.message":
      host.setVar("last_sub", who);
      if (showChat) host.feed("events", { author: who, text: `resubscribed (${e.cumulative_months ?? "?"} months)` });
      return;
    case "channel.subscription.gift":
      if (showChat) host.feed("events", { author: e.is_anonymous ? "Anonymous" : who, text: `gifted ${e.total ?? 1} sub${e.total === 1 ? "" : "s"}` });
      return;
    case "channel.cheer":
      host.setVar("last_cheer", `${e.is_anonymous ? "Anonymous" : who} (${e.bits})`);
      if (showChat) host.feed("events", { author: e.is_anonymous ? "Anonymous" : who, text: `cheered ${e.bits} bits` });
      return;
    case "channel.raid":
      host.setVar("last_raid", `${e.from_broadcaster_user_name} (${e.viewers})`);
      if (showChat) host.feed("events", { author: String(e.from_broadcaster_user_name ?? ""), text: `raided with ${e.viewers}` });
      return;
    case "stream.online":
    case "stream.offline":
      live.on = type === "stream.online";
      if (!live.on) live.viewers = 0;
      host.setVar("live", live.on);
      host.setVar("viewers", live.viewers);
      break;
    case "channel.update":
      channel.title = e.title ?? channel.title;
      channel.category = e.category_name ?? channel.category;
      host.setVar("title", channel.title);
      host.setVar("category", channel.category);
      break;
    case "channel.shield_mode.begin":
    case "channel.shield_mode.end":
      shield = type.endsWith("begin");
      break;
    case "channel.poll.begin":
    case "channel.poll.progress":
    case "channel.poll.end":
      poll = { id: e.id, title: e.title, ends: Date.parse(e.ends_at ?? e.ended_at ?? "") || Date.now(), choices: choices(e.choices), done: type.endsWith("end") };
      tickVotes();
      break;
    case "channel.prediction.begin":
    case "channel.prediction.progress":
    case "channel.prediction.lock":
    case "channel.prediction.end":
      prediction = {
        id: e.id,
        title: e.title,
        ends: Date.parse(e.locks_at ?? "") || Date.now(),
        state: type.endsWith("lock") ? "locked" : type.endsWith("end") ? "done" : "active",
        outcomes: choices(e.outcomes),
      };
      tickVotes();
      break;
    case "channel.ad_break.begin":
      ads.length = Number(e.duration_seconds ?? 0);
      ads.until = Date.parse(e.started_at ?? "") + ads.length * 1000 || Date.now() + ads.length * 1000;
      tickAds();
      break;
    default:
      return;
  }
  render();
}

function tickAds() {
  if (adTimer) return;
  adTimer = setInterval(() => {
    if (Date.now() > ads.until) {
      clearInterval(adTimer!);
      adTimer = null;
      void refresh();
    }
    for (const k of host.keys("ad")) {
      const v = view(k);
      if (v) k.set(v);
      else k.clear();
    }
  }, 1000);
}

function subscriptions(user: string): Array<[string, string, Record<string, string>]> {
  const b = { broadcaster_user_id: user };
  return [
    ["channel.chat.message", "1", { ...b, user_id: user }],
    ["channel.chat_settings.update", "1", { ...b, user_id: user }],
    ["channel.follow", "2", { ...b, moderator_user_id: user }],
    ["channel.subscribe", "1", b],
    ["channel.subscription.message", "1", b],
    ["channel.subscription.gift", "1", b],
    ["channel.cheer", "1", b],
    ["channel.raid", "1", { to_broadcaster_user_id: user }],
    ["stream.online", "1", b],
    ["stream.offline", "1", b],
    ["channel.update", "2", b],
    ["channel.ad_break.begin", "1", b],
    ["channel.shield_mode.begin", "1", { ...b, moderator_user_id: user }],
    ["channel.shield_mode.end", "1", { ...b, moderator_user_id: user }],
    ["channel.poll.begin", "1", b],
    ["channel.poll.progress", "1", b],
    ["channel.poll.end", "1", b],
    ["channel.prediction.begin", "1", b],
    ["channel.prediction.progress", "1", b],
    ["channel.prediction.lock", "1", b],
    ["channel.prediction.end", "1", b],
  ];
}

function connectEvents(gen: number) {
  let moved = false;
  const sock = new Reconnecting(
    () => {
      const u = reconnectUrl ?? `${endpoints.eventsub}?keepalive_timeout_seconds=30`;
      moved = !!reconnectUrl;
      reconnectUrl = null;
      return u;
    },
    {
      open() {},
      message(raw) {
        const m = JSON.parse(raw);
        const kind = m.metadata?.message_type;
        const timeout = Number(m.payload?.session?.keepalive_timeout_seconds ?? 30);
        if (watchdog) clearTimeout(watchdog);
        watchdog = setTimeout(() => sock.drop("EventSub went quiet"), (timeout + 10) * 1000);
        if (kind === "session_welcome") {
          sock.healthy();
          if (!moved && me && gen === generation) void subscribe(String(m.payload.session.id), me.id);
        } else if (kind === "session_reconnect") {
          const next = String(m.payload?.session?.reconnect_url ?? "");
          if (next.startsWith("wss://")) reconnectUrl = next;
          sock.drop("moving to a new EventSub server");
        } else if (kind === "notification") {
          onEvent(String(m.metadata.subscription_type), m.payload?.event ?? {});
        }
      },
      close() {},
    },
  );
  events = sock;
  sock.start();
}

async function subscribe(sessionId: string, user: string) {
  const chatOff = !setting(host.connection, "chat", true);
  const results = await Promise.allSettled(
    subscriptions(user)
      .filter(([type]) => !(chatOff && type === "channel.chat.message"))
      .map(([type, version, condition]) => api("POST", "/eventsub/subscriptions", { type, version, condition, transport: { method: "websocket", session_id: sessionId } })),
  );
  const failed = results.filter((r) => r.status === "rejected").length;
  if (failed) host.log.warn(`${failed} Twitch event subscriptions were refused (some need Affiliate or Partner)`);
}

function text(v: unknown, max: number, what: string): string {
  const t = String(v ?? "").trim();
  if (!t) throw new Error(`Set ${what} on this key first`);
  if (t.length > max) throw new Error(`Keep it to ${max} characters: "${t.slice(0, 20)}…"`);
  return t;
}

/** One entry per line, within Twitch's limits. */
function lines(v: unknown, min: number, max: number, len: number, what: string): string[] {
  const list = String(v ?? "")
    .split("\n")
    .map((l) => l.trim())
    .filter(Boolean);
  if (list.length < min || list.length > max) throw new Error(`Twitch needs ${min} to ${max} ${what}, one per line`);
  const long = list.find((l) => l.length > len);
  if (long) throw new Error(`"${long}" is longer than ${len} characters`);
  return list;
}

const clampInt = (v: unknown, lo: number, hi: number, dflt: number) => {
  const n = Math.round(Number(v ?? dflt));
  return Number.isFinite(n) ? Math.min(hi, Math.max(lo, n)) : dflt;
};

async function userId(login: string): Promise<string> {
  const name = login.trim().replace(/^@/, "").toLowerCase();
  if (!/^[a-z0-9_]{1,25}$/.test(name)) throw new Error("That isn't a valid Twitch channel name");
  const u = (await api("GET", `/users${q({ login: name })}`)).data?.[0];
  if (!u) throw new Error(`No Twitch channel called ${name}`);
  return u.id;
}

function signedIn() {
  if (!me) {
    if (signInPage) void host.openBrowser(signInPage).catch(() => {});
    throw new Error("Twitch isn't signed in yet");
  }
  return me;
}

const handlers: Record<string, (s: Record<string, unknown>) => Promise<unknown>> = {
  async marker(s) {
    const u = signedIn();
    await api("POST", "/streams/markers", { user_id: u.id, description: String(s.description ?? "").slice(0, 140) || undefined });
    host.toast("Marker added", "success");
  },
  async clip() {
    const u = signedIn();
    const r = await api("POST", `/clips${q({ broadcaster_id: u.id })}`);
    host.toast(r.data?.[0]?.id ? "Clip created" : "Clip requested", "success");
  },
  async ad(s) {
    const u = signedIn();
    const length = Math.min(180, Math.max(30, Number(s.length ?? 60)));
    const r = (await api("POST", "/channels/commercial", { broadcaster_id: u.id, length })).data?.[0];
    if (r?.length) {
      ads.length = Number(r.length);
      ads.until = Date.now() + ads.length * 1000;
      tickAds();
    }
  },
  async "snooze-ad"() {
    const u = signedIn();
    const r = (await api("POST", `/channels/ads/schedule/snooze${q({ broadcaster_id: u.id })}`)).data?.[0];
    if (r) {
      ads.snoozes = Number(r.snooze_count ?? 0);
      ads.next = stamp(r.next_ad_at) || ads.next;
      render();
    }
  },
  async "chat-mode"(s) {
    const u = signedIn();
    const mode = String(s.mode ?? "emote");
    const key = MODES[mode];
    if (!key) throw new Error("Unknown chat mode");
    const on = want(s.op ?? "toggle", !!chat[key]);
    const body: Record<string, unknown> = { [key]: on };
    if (mode === "slow" && on) body.slow_mode_wait_time = Number(s.seconds ?? 30);
    if (mode === "followers" && on) body.follower_mode_duration = Number(s.minutes ?? 0);
    const r = (await api("PATCH", `/chat/settings${q({ broadcaster_id: u.id, moderator_id: u.id })}`, body)).data?.[0];
    chat = r ?? { ...chat, [key]: on };
    render();
  },
  async send(s) {
    const u = signedIn();
    const r = (await api("POST", "/chat/messages", { broadcaster_id: u.id, sender_id: u.id, message: String(s.message ?? "").slice(0, 500) })).data?.[0];
    if (r && r.is_sent === false) throw new Error(r.drop_reason?.message ?? "Twitch didn't send the message");
  },
  async announce(s) {
    const u = signedIn();
    await api("POST", `/chat/announcements${q({ broadcaster_id: u.id, moderator_id: u.id })}`, { message: String(s.message ?? "").slice(0, 500), color: String(s.color ?? "primary") });
  },
  async title(s) {
    const u = signedIn();
    const body: Record<string, string> = {};
    if (s.title) body.title = String(s.title);
    if (s.category) {
      const g = (await api("GET", `/games${q({ name: String(s.category) })}`)).data?.[0];
      if (!g) throw new Error(`No Twitch category called "${s.category}"`);
      body.game_id = g.id;
    }
    if (!Object.keys(body).length) return;
    await api("PATCH", `/channels${q({ broadcaster_id: u.id })}`, body);
    await loadChannel();
    render();
  },
  async shoutout(s) {
    const u = signedIn();
    const to = await userId(String(s.user ?? ""));
    await api("POST", `/chat/shoutouts${q({ from_broadcaster_id: u.id, to_broadcaster_id: to, moderator_id: u.id })}`);
  },
  async raid(s) {
    const u = signedIn();
    if (s.op === "cancel") return api("DELETE", `/raids${q({ broadcaster_id: u.id })}`);
    const to = await userId(String(s.user ?? ""));
    await api("POST", `/raids${q({ from_broadcaster_id: u.id, to_broadcaster_id: to })}`);
    host.toast("Raid ready. It starts after 90 seconds or when you confirm on Twitch.");
  },
  async shield(s) {
    const u = signedIn();
    const on = want(s.op ?? "toggle", shield);
    const r = (await api("PUT", `/moderation/shield_mode${q({ broadcaster_id: u.id, moderator_id: u.id })}`, { is_active: on })).data?.[0];
    shield = r ? !!r.is_active : on;
    render();
  },
  async poll(s) {
    const u = signedIn();
    if (String(s.op ?? "start") === "end") {
      if (!poll || poll.done) throw new Error("No poll is running");
      await api("PATCH", "/polls", { broadcaster_id: u.id, id: poll.id, status: "TERMINATED" });
      return;
    }
    if (poll && !poll.done) throw new Error("A poll is already running");
    const options = lines(s.choices, 2, 5, 25, "choices");
    const r = (
      await api("POST", "/polls", {
        broadcaster_id: u.id,
        title: text(s.question, 60, "a question"),
        choices: options.map((title) => ({ title })),
        duration: clampInt(s.seconds, 15, 1800, 120),
      })
    ).data?.[0];
    if (r) poll = { id: r.id, title: r.title, ends: Date.parse(r.started_at) + Number(r.duration) * 1000, choices: choices(r.choices), done: false };
    tickVotes();
    render();
  },
  async prediction(s) {
    const u = signedIn();
    const op = String(s.op ?? "start");
    if (op === "start") {
      if (prediction && prediction.state !== "done") throw new Error("A prediction is already running");
      const outcomes = lines(s.outcomes, 2, 10, 25, "outcomes");
      const r = (
        await api("POST", "/predictions", {
          broadcaster_id: u.id,
          title: text(s.question, 45, "a question"),
          outcomes: outcomes.map((title) => ({ title })),
          prediction_window: clampInt(s.seconds, 30, 1800, 120),
        })
      ).data?.[0];
      if (r) prediction = { id: r.id, title: r.title, ends: Date.parse(r.created_at) + Number(r.prediction_window) * 1000, state: "active", outcomes: choices(r.outcomes) };
      tickVotes();
      render();
      return;
    }
    if (!prediction || prediction.state === "done") throw new Error("No prediction is running");
    const body: Record<string, unknown> = { broadcaster_id: u.id, id: prediction.id };
    if (op === "lock") body.status = "LOCKED";
    else if (op === "cancel") body.status = "CANCELED";
    else {
      // "resolve": the winner is an outcome's name or its number.
      const pick = String(s.winner ?? "").trim();
      const n = Number(pick);
      const win = prediction.outcomes.find((o, i) => o.title.toLowerCase() === pick.toLowerCase() || (Number.isInteger(n) && n === i + 1));
      if (!win) throw new Error("Set the winning outcome on this key: its name or number");
      body.status = "RESOLVED";
      body.winning_outcome_id = win.id;
    }
    await api("PATCH", "/predictions", body);
  },
  async reward(s) {
    const u = signedIn();
    const title = text(s.title, 45, "a reward name");
    const r = rewards.get(title.toLowerCase());
    const mode = String(s.mode ?? "enable");
    const on = want(s.op ?? "toggle", r ? (mode === "pause" ? !r.paused : r.enabled) : false);
    if (!r) {
      if (!on) return;
      // Twitch only lets an app change rewards it made, so Keyside makes its own.
      const made = (
        await api("POST", `/channel_points/custom_rewards${q({ broadcaster_id: u.id })}`, {
          title,
          cost: clampInt(s.cost, 1, 1_000_000, 500),
          prompt: String(s.prompt ?? "").slice(0, 200) || undefined,
          is_enabled: true,
        })
      ).data?.[0];
      if (made) rewards.set(title.toLowerCase(), { id: made.id, enabled: !!made.is_enabled, paused: !!made.is_paused });
      host.toast(`Created the "${title}" reward`, "success");
    } else {
      const body = mode === "pause" ? { is_paused: !on } : { is_enabled: on };
      const done = (await api("PATCH", `/channel_points/custom_rewards${q({ broadcaster_id: u.id, id: r.id })}`, body)).data?.[0];
      if (done) rewards.set(title.toLowerCase(), { id: r.id, enabled: !!done.is_enabled, paused: !!done.is_paused });
    }
    render();
  },
  async "clear-chat"() {
    const u = signedIn();
    await api("DELETE", `/moderation/chat${q({ broadcaster_id: u.id, moderator_id: u.id })}`);
  },
};

export default definePlugin({
  start(h) {
    host = h;
    void begin();
  },
  connectionChanged() {
    void begin();
  },
  stop() {
    generation++;
    stopAll();
  },
  actions: Object.fromEntries(Object.entries(handlers).map(([id, fn]) => [id, { press: (step: Step) => fn(step.settings), appear: (k: Key) => (view(k) ? k.set(view(k)!) : k.clear()) }])),
});

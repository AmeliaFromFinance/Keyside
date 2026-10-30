// YouTube Live through the Data API. Sign-in uses Google's device flow.
// The API has a daily quota, so chat is only read while a broadcast is live, and no faster
// than every 15 seconds.

import { definePlugin, setting, type Host } from "@keyside/sdk";
import { Session, fromResponse, pollDeviceCode } from "../../_lib/session.ts";
import { HttpError, errorText, firstTime, http } from "../../_lib/util.ts";

export const endpoints = {
  oauth: "https://oauth2.googleapis.com",
  api: "https://www.googleapis.com/youtube/v3",
};

const SCOPE = "https://www.googleapis.com/auth/youtube";
const IDLE_CHECK = 120_000;
const CHAT_MIN = 15_000;
const STATS_EVERY = 60_000;

let host: Host;
let session: Session;
let generation = 0;
let signedIn = false;
let broadcast: { id: string; chat: string; title: string; stream: string } | null = null;
/** What YouTube reports about the incoming video. */
let health: { status: string; ingest: string; issue: string } | null = null;
let page: string | undefined;
let timers: Array<ReturnType<typeof setTimeout>> = [];
let firstPage = true;

const cfg = () => ({ id: String(setting(host.connection, "clientId", "")).trim(), secret: String(setting(host.connection, "clientSecret", "")).trim() });

async function api(method: string, path: string, body?: unknown, retry = true): Promise<any> {
  const token = await session.token();
  try {
    return await http(`${endpoints.api}${path}`, { method, headers: { authorization: `Bearer ${token}` }, json: body });
  } catch (e) {
    if (e instanceof HttpError && e.status === 401 && retry) {
      await session.renew();
      return api(method, path, body, false);
    }
    if (e instanceof HttpError && e.status === 403 && /quota/i.test(e.message)) throw new Error("YouTube API quota used up for today");
    throw e;
  }
}

function later(fn: () => void, ms: number, gen: number) {
  timers.push(setTimeout(() => gen === generation && fn(), ms));
}

async function signIn(gen: number) {
  const { id, secret } = cfg();
  const dev = await http(`${endpoints.oauth}/device/code`, { form: { client_id: id, scope: SCOPE } });
  const where = String(dev.verification_url ?? "https://www.google.com/device");
  host.status("connecting", `Sign in: open ${where.replace(/^https:\/\//, "")} and enter ${dev.user_code}`);
  host.toast(`YouTube code: ${dev.user_code}`);
  if (where.startsWith("https://") && (await firstTime(host.dataDir, `youtube-${id}`))) await host.openBrowser(where).catch(() => {});
  const tokens = await pollDeviceCode(
    () => http(`${endpoints.oauth}/token`, { form: { client_id: id, client_secret: secret, device_code: dev.device_code, grant_type: "urn:ietf:params:oauth:grant-type:device_code" } }),
    Number(dev.interval),
    Number(dev.expires_in),
    () => gen !== generation,
  );
  await session.save(fromResponse(tokens, id));
  host.toast("Signed in to YouTube", "success");
}

async function start(gen: number) {
  const { id, secret } = cfg();
  signedIn = false;
  if (!id || !secret) {
    host.status("offline", "Add your Google OAuth client ID and secret in the plugin settings.");
    return;
  }
  session = new Session(host, { refresh: (t) => http(`${endpoints.oauth}/token`, { form: { client_id: id, client_secret: secret, refresh_token: t.refresh ?? "", grant_type: "refresh_token" } }) });
  session.onSignedOut = () => gen === generation && void restart();
  try {
    if (!session.load(id)) await signIn(gen);
    if (gen !== generation) return;
    signedIn = true;
    host.status("connected", "Waiting for a live broadcast");
    await findBroadcast(gen);
  } catch (e) {
    if (gen === generation) host.status("error", errorText(e));
  }
}

function restart() {
  generation++;
  for (const t of timers) clearTimeout(t);
  timers = [];
  broadcast = null;
  page = undefined;
  return start(generation);
}

async function findBroadcast(gen: number) {
  try {
    const r = await api("GET", "/liveBroadcasts?part=snippet,contentDetails&broadcastStatus=active&broadcastType=all&maxResults=1");
    const b = r.items?.[0];
    const was = broadcast?.id;
    broadcast = b?.snippet?.liveChatId ? { id: b.id, chat: b.snippet.liveChatId, title: b.snippet.title ?? "", stream: String(b.contentDetails?.boundStreamId ?? "") } : null;
    if (!broadcast) setHealth(null);
    host.setVar("live", !!broadcast);
    host.setVar("title", broadcast?.title ?? "");
    if (!broadcast) host.setVar("viewers", 0);
    host.status("connected", broadcast ? broadcast.title : "Waiting for a live broadcast");
    if (broadcast && broadcast.id !== was) {
      page = undefined;
      firstPage = true;
      void readChat(gen);
      void stats(gen);
    }
  } catch (e) {
    host.status("error", errorText(e));
  }
  later(() => void findBroadcast(gen), broadcast ? IDLE_CHECK * 2 : IDLE_CHECK, gen);
}

async function stats(gen: number) {
  if (!broadcast) return;
  const v = (await api("GET", `/videos?part=liveStreamingDetails,statistics&id=${encodeURIComponent(broadcast.id)}`).catch(() => null))?.items?.[0];
  if (v) {
    host.setVar("viewers", Number(v.liveStreamingDetails?.concurrentViewers ?? 0));
    host.setVar("likes", Number(v.statistics?.likeCount ?? 0));
  }
  await checkHealth();
  later(() => void stats(gen), STATS_EVERY, gen);
}

const HEALTH: Record<string, [label: string, tone: "success" | "warning" | "danger" | "muted"]> = {
  good: ["Good", "success"],
  ok: ["OK", "warning"],
  bad: ["Poor", "danger"],
  noData: ["No data", "muted"],
};

async function checkHealth() {
  const id = broadcast?.stream;
  if (!id) return setHealth(null);
  const s = (await api("GET", `/liveStreams?part=status&id=${encodeURIComponent(id)}`).catch(() => null))?.items?.[0]?.status;
  if (!s) return;
  const issue = (s.healthStatus?.configurationIssues ?? []).find((i: any) => i?.severity === "error") ?? s.healthStatus?.configurationIssues?.[0];
  setHealth({ status: String(s.healthStatus?.status ?? "noData"), ingest: String(s.streamStatus ?? ""), issue: String(issue?.reason ?? issue?.description ?? "").slice(0, 80) });
}

function setHealth(h: typeof health) {
  health = h;
  host.setVar("health", h ? (Object.hasOwn(HEALTH, h.status) ? HEALTH[h.status][0] : h.status) : "");
  for (const k of host.keys("health")) {
    if (!h) {
      k.set({ sub: "Not live", tone: "muted" });
      continue;
    }
    const [label, tone] = Object.hasOwn(HEALTH, h.status) ? HEALTH[h.status] : [h.status, "muted" as const];
    k.set({ badge: label.toUpperCase(), badgeTone: tone, tone: tone === "danger" ? "danger" : undefined, sub: h.issue || (h.ingest === "active" ? "Receiving video" : h.ingest || undefined) });
  }
}

/** The broadcast a title key changes: the live one, or else the next scheduled one. */
async function titleTarget(): Promise<any> {
  for (const status of ["active", "upcoming"]) {
    const b = (await api("GET", `/liveBroadcasts?part=snippet&broadcastStatus=${status}&broadcastType=all&maxResults=1`)).items?.[0];
    if (b?.id && b.snippet) return b;
  }
  throw new Error("No live or scheduled broadcast to rename");
}

export function cleanTitle(input: unknown): string {
  const t = String(input ?? "").replace(/\s+/g, " ").trim();
  if (!t) throw new Error("The title is empty");
  if (t.length > 100) throw new Error("YouTube titles can be up to 100 characters");
  if (/[<>]/.test(t)) throw new Error("YouTube titles can't contain < or >");
  return t;
}

async function readChat(gen: number) {
  const b = broadcast;
  if (!b) return;
  let wait = CHAT_MIN;
  try {
    const r = await api("GET", `/liveChat/messages?part=snippet,authorDetails&maxResults=200&liveChatId=${encodeURIComponent(b.chat)}${page ? `&pageToken=${encodeURIComponent(page)}` : ""}`);
    page = r.nextPageToken;
    wait = Math.max(CHAT_MIN, Number(r.pollingIntervalMillis ?? 0));
    // The first page is chat history from before Keyside connected.
    if (!firstPage) for (const m of r.items ?? []) onMessage(m);
    firstPage = false;
  } catch (e) {
    if (e instanceof HttpError && (e.status === 403 || e.status === 404)) {
      broadcast = null;
      return;
    }
    host.log.warn(`YouTube chat: ${errorText(e)}`);
  }
  if (broadcast?.id === b.id) later(() => void readChat(gen), wait, gen);
}

function onMessage(m: any) {
  const s = m.snippet ?? {};
  const who = String(m.authorDetails?.displayName ?? "");
  switch (s.type) {
    case "textMessageEvent":
      host.feed("chat", { author: who, text: String(s.displayMessage ?? "").slice(0, 500) });
      break;
    case "superChatEvent":
    case "superStickerEvent": {
      const amount = s.superChatDetails?.amountDisplayString ?? s.superStickerDetails?.amountDisplayString ?? "";
      host.setVar("last_superchat", `${who} ${amount}`.trim());
      host.feed("events", { author: who, text: `sent ${amount}${s.superChatDetails?.userComment ? `: ${s.superChatDetails.userComment}` : ""}`.slice(0, 500) });
      break;
    }
    case "newSponsorEvent":
    case "memberMilestoneChatEvent":
      host.setVar("last_member", who);
      host.feed("events", { author: who, text: s.type === "newSponsorEvent" ? "became a member" : "member milestone" });
      break;
    case "membershipGiftingEvent":
      host.feed("events", { author: who, text: `gifted ${s.membershipGiftingDetails?.giftMembershipsCount ?? ""} memberships` });
      break;
  }
}

export default definePlugin({
  start(h) {
    host = h;
    void restart();
  },
  connectionChanged() {
    void restart();
  },
  stop() {
    generation++;
    for (const t of timers) clearTimeout(t);
  },
  actions: {
    send: {
      async press({ settings }) {
        if (!signedIn) throw new Error("YouTube isn't signed in yet");
        if (!broadcast) throw new Error("No live broadcast right now");
        await api("POST", "/liveChat/messages?part=snippet", { snippet: { liveChatId: broadcast.chat, type: "textMessageEvent", textMessageDetails: { messageText: String(settings.message ?? "").slice(0, 200) } } });
      },
    },
    title: {
      async press({ settings }) {
        if (!signedIn) throw new Error("YouTube isn't signed in yet");
        const title = cleanTitle(settings.title);
        const b = await titleTarget();
        // YouTube clears any field left out of an update, so send the others back unchanged.
        const { description, scheduledStartTime, scheduledEndTime } = b.snippet;
        await api("PUT", "/liveBroadcasts?part=snippet", { id: b.id, snippet: { title, description, scheduledStartTime, scheduledEndTime } });
        if (broadcast && broadcast.id === b.id) {
          broadcast.title = title;
          host.setVar("title", title);
        }
        host.toast("YouTube title changed", "success");
      },
    },
    health: {
      async press() {
        if (!signedIn) throw new Error("YouTube isn't signed in yet");
        if (!broadcast) throw new Error("No live broadcast right now");
        await checkHealth();
      },
      appear: () => setHealth(health),
    },
  },
});

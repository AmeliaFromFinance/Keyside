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
let broadcast: { id: string; chat: string; title: string } | null = null;
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
    const r = await api("GET", "/liveBroadcasts?part=snippet&broadcastStatus=active&broadcastType=all&maxResults=1");
    const b = r.items?.[0];
    const was = broadcast?.id;
    broadcast = b?.snippet?.liveChatId ? { id: b.id, chat: b.snippet.liveChatId, title: b.snippet.title ?? "" } : null;
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
  later(() => void stats(gen), STATS_EVERY, gen);
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
  },
});

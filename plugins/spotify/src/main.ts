// Spotify Web API. Signs in with PKCE through Keyside's local redirect, so no client
// secret is stored.

import { definePlugin, setting, type Host, type Key, type KeyView } from "@keyside/sdk";
import { Session, fromResponse } from "../../_lib/session.ts";
import { HttpError, b64, b64url, clamp01, errorText, http, sha256 } from "../../_lib/util.ts";

export const endpoints = {
  accounts: "https://accounts.spotify.com",
  api: "https://api.spotify.com/v1",
};

const SCOPES = "user-read-playback-state user-modify-playback-state user-library-read user-library-modify";
const FAST = 3000;
const SLOW = 15_000;

interface Player {
  playing: boolean;
  track: string;
  artist: string;
  id: string;
  art: string;
  volume: number | null;
  shuffle: boolean;
  repeat: "off" | "context" | "track";
  liked: boolean;
  device: string;
}

let host: Host;
let session: Session;
let generation = 0;
let timer: ReturnType<typeof setTimeout> | null = null;
let signingIn: Promise<void> | null = null;
let player: Player | null = null;
let artFor = "";
let artData = "";
const artCache = new Map<string, string>();

const clientId = () => String(setting(host.connection, "clientId", "")).trim();

async function api(method: string, path: string, body?: unknown, retry = true): Promise<any> {
  const token = await session.token();
  try {
    return await http(`${endpoints.api}${path}`, { method, headers: { authorization: `Bearer ${token}` }, json: body });
  } catch (e) {
    if (e instanceof HttpError && e.status === 401 && retry) {
      await session.renew();
      return api(method, path, body, false);
    }
    if (e instanceof HttpError && e.status === 403) throw new Error("Spotify needs Premium for this");
    if (e instanceof HttpError && e.status === 404) throw new Error("Open Spotify on a device first");
    throw e;
  }
}

async function signIn() {
  const client = clientId();
  const verifier = b64url(crypto.getRandomValues(new Uint8Array(48)));
  const challenge = b64url(await sha256(verifier));
  const url = `${endpoints.accounts}/authorize?${new URLSearchParams({ client_id: client, response_type: "code", scope: SCOPES, code_challenge_method: "S256", code_challenge: challenge })}`;
  host.status("connecting", "Finish signing in to Spotify in your browser");
  const { code, redirectUri } = await host.authorize(url);
  const tokens = await http(`${endpoints.accounts}/api/token`, { form: { grant_type: "authorization_code", code, redirect_uri: redirectUri, client_id: client, code_verifier: verifier } });
  await session.save(fromResponse(tokens, client));
  host.toast("Signed in to Spotify", "success");
}

/** Signs in on demand, since it opens a browser on the PC. */
async function ensureSignedIn() {
  if (session?.signedIn) return;
  if (!clientId()) throw new Error("Add your Spotify Client ID in the plugin settings");
  signingIn ??= signIn()
    .then(() => schedule(0, generation))
    .finally(() => (signingIn = null));
  await signingIn;
}

function schedule(ms: number, gen: number) {
  if (timer) clearTimeout(timer);
  timer = setTimeout(() => gen === generation && void poll(gen), ms);
}

async function poll(gen: number) {
  try {
    const p = await api("GET", "/me/player");
    if (!p || !p.item) player = null;
    else {
      const id = String(p.item.id ?? "");
      const liked = player?.id === id ? player.liked : id ? !!(await api("GET", `/me/tracks/contains?ids=${encodeURIComponent(id)}`).catch(() => [false]))?.[0] : false;
      const images: Array<{ url: string; width: number }> = p.item.album?.images ?? p.item.images ?? [];
      const art = images.slice().sort((a, b) => a.width - b.width).find((i) => i.width >= 160)?.url ?? images[0]?.url ?? "";
      player = {
        playing: !!p.is_playing,
        track: String(p.item.name ?? ""),
        artist: (p.item.artists ?? []).map((a: any) => a.name).join(", ") || String(p.item.show?.name ?? ""),
        id,
        art,
        volume: typeof p.device?.volume_percent === "number" ? p.device.volume_percent / 100 : null,
        shuffle: !!p.shuffle_state,
        repeat: p.repeat_state ?? "off",
        liked,
        device: String(p.device?.name ?? ""),
      };
      if (art && art !== artFor) await loadArt(art);
    }
    host.status("connected", player ? `${player.playing ? "Playing" : "Paused"}: ${player.track}` : "Nothing playing");
  } catch (e) {
    host.status("error", errorText(e));
  }
  render();
  schedule(player?.playing ? FAST : SLOW, gen);
}

async function loadArt(url: string) {
  artFor = url;
  const cached = artCache.get(url);
  if (cached) {
    artData = cached;
    return;
  }
  if (!url.startsWith("https://i.scdn.co/") && !url.startsWith(endpoints.api)) return;
  try {
    const res = await fetch(url);
    const type = res.headers.get("content-type") ?? "";
    const bytes = new Uint8Array(await res.arrayBuffer());
    if (!res.ok || !/^image\/(jpeg|png|webp)$/.test(type) || bytes.length > 250_000) return;
    artData = `data:${type};base64,${b64(bytes)}`;
    artCache.clear();
    artCache.set(url, artData);
  } catch {
    artData = "";
  }
}

function view(k: Key): KeyView | null {
  if (!session?.signedIn) return clientId() ? { sub: "Press to sign in" } : { disabled: true, sub: "Not set up" };
  const p = player;
  switch (k.action) {
    case "now-playing":
      if (!p) return { sub: "Nothing playing" };
      return { label: p.track.slice(0, 64), sub: p.artist.slice(0, 64), image: artData || undefined, badge: p.playing ? undefined : "PAUSED", badgeTone: "muted" };
    case "play-pause":
      return p?.playing ? { icon: "pause", tone: "accent" } : null;
    case "volume":
      return p?.volume != null ? { value: clamp01(p.volume), meta: `${Math.round(p.volume * 100)}%` } : { disabled: true };
    case "shuffle":
      return p?.shuffle ? { tone: "accent", badge: "ON", badgeTone: "accent" } : null;
    case "repeat":
      return p && p.repeat !== "off" ? { tone: "accent", badge: p.repeat === "track" ? "ONE" : "ALL", badgeTone: "accent" } : null;
    case "like":
      return p?.liked ? { tone: "accent", badge: "LIKED", badgeTone: "accent" } : null;
    case "device":
      return p?.device && p.device.toLowerCase() === String(k.settings.device ?? "").toLowerCase() ? { tone: "accent", badge: "PLAYING", badgeTone: "accent" } : null;
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
  host.setVar("track", player?.track ?? "");
  host.setVar("artist", player?.artist ?? "");
  host.setVar("playing", !!player?.playing);
  host.setVar("device", player?.device ?? "");
}

async function devices(): Promise<Array<{ id: string; name: string }>> {
  const r = await api("GET", "/me/player/devices");
  return (r?.devices ?? []).filter((d: any) => d?.id && d?.name && !d.is_restricted).map((d: any) => ({ id: String(d.id), name: String(d.name).slice(0, 100) }));
}

/** Accepts spotify:album:x or an open.spotify.com link. */
export function toUri(input: string): string {
  const s = input.trim();
  if (/^spotify:(album|playlist|artist|show|track|episode):[A-Za-z0-9]+$/.test(s)) return s;
  const m = s.match(/^https:\/\/open\.spotify\.com\/(?:intl-[a-z-]+\/)?(album|playlist|artist|show|track|episode)\/([A-Za-z0-9]+)/);
  if (m) return `spotify:${m[1]}:${m[2]}`;
  throw new Error("That isn't a Spotify link");
}

/** Only single songs and episodes can be queued. */
export function queueUri(input: string): string {
  const uri = toUri(input);
  if (!/^spotify:(track|episode):/.test(uri)) throw new Error("Only songs and episodes can be queued");
  return uri;
}

async function act(fn: () => Promise<unknown>) {
  await ensureSignedIn();
  await fn();
  schedule(400, generation);
}

function start() {
  generation++;
  if (timer) clearTimeout(timer);
  player = null;
  const client = clientId();
  session = new Session(host, { refresh: (t) => http(`${endpoints.accounts}/api/token`, { form: { grant_type: "refresh_token", refresh_token: t.refresh ?? "", client_id: client } }) });
  session.onSignedOut = () => render();
  if (!client) host.status("offline", "Add your Spotify Client ID in the plugin settings.");
  else if (!session.load(client)) host.status("offline", "Press any Spotify key to sign in.");
  else schedule(0, generation);
  render();
}

export default definePlugin({
  start(h) {
    host = h;
    start();
  },
  connectionChanged: start,
  stop() {
    generation++;
    if (timer) clearTimeout(timer);
  },
  actions: {
    "now-playing": { press: () => act(() => api("PUT", player?.playing ? "/me/player/pause" : "/me/player/play")), appear: render },
    "play-pause": { press: () => act(() => api("PUT", player?.playing ? "/me/player/pause" : "/me/player/play")), appear: render },
    next: { press: () => act(() => api("POST", "/me/player/next")), appear: render },
    previous: { press: () => act(() => api("POST", "/me/player/previous")), appear: render },
    volume: {
      press: () => ensureSignedIn(),
      async slide({ value }) {
        await ensureSignedIn();
        const pct = Math.round(clamp01(value) * 100);
        if (player) player.volume = pct / 100;
        await api("PUT", `/me/player/volume?volume_percent=${pct}`);
      },
      appear: render,
    },
    shuffle: { press: () => act(() => api("PUT", `/me/player/shuffle?state=${!player?.shuffle}`)), appear: render },
    repeat: {
      press: () => act(() => api("PUT", `/me/player/repeat?state=${({ off: "context", context: "track", track: "off" } as const)[player?.repeat ?? "off"]}`)),
      appear: render,
    },
    like: {
      press: () =>
        act(async () => {
          if (!player?.id) throw new Error("Nothing playing");
          await api(player.liked ? "DELETE" : "PUT", `/me/tracks?ids=${encodeURIComponent(player.id)}`);
          player.liked = !player.liked;
        }),
      appear: render,
    },
    play: {
      press: ({ settings }) =>
        act(() => {
          const uri = toUri(String(settings.uri ?? ""));
          return api("PUT", "/me/player/play", /:(track|episode):/.test(uri) ? { uris: [uri] } : { context_uri: uri });
        }),
      appear: render,
    },
    queue: {
      press: ({ settings }) => act(() => api("POST", `/me/player/queue?uri=${encodeURIComponent(queueUri(String(settings.uri ?? "")))}`)),
    },
    device: {
      // Saved by name, since some devices get a new id each time they connect.
      press: ({ settings }) =>
        act(async () => {
          const name = String(settings.device ?? "").trim().toLowerCase();
          const d = (await devices()).find((x) => x.name.toLowerCase() === name);
          if (!d) throw new Error(`${String(settings.device ?? "That device").slice(0, 100)} isn't available. Open Spotify on it first.`);
          await api("PUT", "/me/player", { device_ids: [d.id], play: settings.play !== false });
        }),
      async options() {
        await ensureSignedIn();
        return (await devices()).map((d) => ({ value: d.name, label: d.name }));
      },
      appear: render,
    },
  },
});

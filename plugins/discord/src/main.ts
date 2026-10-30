// Discord over its desktop RPC: the companion opens Discord's local IPC socket and relays
// frames, this plugin handles sign-in and commands. Equibop (a Linux client) has no RPC for
// voice, so for it the plugin runs Equibop's --toggle-mic, --toggle-deafen and --toggle-vad.

import { definePlugin, setting, type Host, type Key, type KeyView } from "@keyside/sdk";
import { Session, fromResponse } from "../../_lib/session.ts";
import { clamp01, errorText, http, randomId, want } from "../../_lib/util.ts";
import { Waiters } from "../../_lib/socket.ts";

export const endpoints = { token: "https://discord.com/api/oauth2/token" };
const REDIRECT = "http://127.0.0.1";
const SCOPES = ["rpc", "rpc.voice.read", "rpc.voice.write"];
const RETRY = 10_000;
const OP = { Handshake: 0, Frame: 1, Close: 2, Ping: 3, Pong: 4 };

let host: Host;
let session: Session;
let generation = 0;
let ready = false;
let open = false;
let retryTimer: ReturnType<typeof setTimeout> | null = null;
const waiting = new Waiters<any>();
let readyWait: ((data: any) => void) | null = null;
const voice = { mute: false, deaf: false, ptt: false, input: 1, output: 1, channel: "", noise: false, echo: false, agc: false };

/** Voice processing switches, and their names in Discord's voice settings. */
const PROCESSING = { noise: "noise_suppression", echo: "echo_cancellation", agc: "automatic_gain_control" } as const;
type Processing = keyof typeof PROCESSING;

const cfg = () => ({ id: String(setting(host.connection, "clientId", "")).trim(), secret: String(setting(host.connection, "clientSecret", "")).trim() });
const equibop = () => String(setting(host.connection, "app", "discord")) === "equibop";

// Equibop's own Rich Presence server (arRPC) answers on Discord's socket but can't do voice.
const ARRPC_USER = "1045800378228281345";

/** How Equibop is started, tried in this order. Must match "run" in the manifest. */
function launchers(): string[][] {
  const how = String(setting(host.connection, "install", "auto"));
  const native = ["equibop"];
  const flatpak = ["flatpak", "run", "io.github.equicord.equibop"];
  return how === "native" ? [native] : how === "flatpak" ? [flatpak] : [native, flatpak];
}
let launcher: string[] | null = null;

/** Sends one toggle to the running Equibop. Starts Equibop if it isn't open. */
async function toggleInEquibop(flag: "--toggle-mic" | "--toggle-deafen" | "--toggle-vad") {
  let crashed: number | null = null;
  let other = "";
  for (const l of launcher ? [launcher] : launchers()) {
    try {
      const { code } = await host.run([...l, flag]);
      if (code !== null && code !== 0) {
        crashed = code;
        continue;
      }
      if (!launcher) host.status("connected", "Equibop");
      launcher = l;
      return;
    } catch (e) {
      const msg = errorText(e);
      if (!/not installed/.test(msg)) other = msg;
    }
  }
  launcher = null;
  const msg =
    crashed !== null
      ? `Equibop stopped with error ${crashed}. Check that \`equibop --toggle-mic\` works in a terminal.`
      : other || "Equibop wasn't found. Install it from the AUR or Flathub. For the AppImage, link it as `~/.local/bin/equibop`.";
  host.status("error", msg);
  throw new Error(msg);
}

function frame(op: number, payload: unknown) {
  return host.ipc.send("discord", { op, payload });
}

async function command(cmd: string, args: Record<string, unknown> = {}, evt?: string, ms = 10_000): Promise<any> {
  const nonce = randomId();
  const p = waiting.wait(nonce, ms);
  await frame(OP.Frame, { cmd, args, evt, nonce });
  return p;
}

function onFrame(data: any) {
  const { op, payload: p } = data ?? {};
  if (op === OP.Ping) return void frame(OP.Pong, p);
  if (op === OP.Close) {
    host.status("error", `Discord closed the connection: ${p?.message ?? p?.code ?? "unknown"}`);
    return;
  }
  if (op !== OP.Frame || !p) return;
  if (p.cmd === "DISPATCH" && p.evt === "READY") {
    readyWait?.(p.data ?? {});
    readyWait = null;
    return;
  }
  if (p.nonce) {
    if (p.evt === "ERROR") waiting.reject(p.nonce, new Error(p.data?.message ?? "Discord refused the request"));
    else waiting.resolve(p.nonce, p.data ?? {});
    return;
  }
  if (p.cmd === "DISPATCH" && p.evt === "VOICE_SETTINGS_UPDATE") applyVoice(p.data);
  if (p.cmd === "DISPATCH" && p.evt === "VOICE_CHANNEL_SELECT") {
    voice.channel = p.data?.channel_id ?? "";
    void channelName();
  }
}

function applyVoice(d: any) {
  if (!d) return;
  voice.mute = !!d.mute;
  voice.deaf = !!d.deaf;
  if (typeof d.mode?.type === "string") voice.ptt = d.mode.type === "PUSH_TO_TALK";
  for (const [k, field] of Object.entries(PROCESSING)) if (typeof d[field] === "boolean") voice[k as Processing] = d[field];
  if (typeof d.input?.volume === "number") voice.input = d.input.volume / 100;
  if (typeof d.output?.volume === "number") voice.output = d.output.volume / 200;
  render();
}

async function channelName() {
  if (!voice.channel) return host.setVar("channel", "");
  const c = await command("GET_CHANNEL", { channel_id: voice.channel }).catch(() => null);
  host.setVar("channel", c?.name ?? "");
}

async function connect(gen: number) {
  const { id, secret } = cfg();
  ready = false;
  if (equibop()) {
    // Equibop doesn't report its state, so keys show what Keyside last sent.
    ready = true;
    host.status("connected", "Equibop");
    render();
    return;
  }
  if (!id || !secret) {
    host.status("offline", "Add your Discord Client ID and Client secret in the plugin settings.");
    return;
  }
  try {
    if (!open) {
      await host.ipc.open("discord");
      open = true;
    }
    const got = new Promise<any>((r) => {
      readyWait = r;
      setTimeout(() => r(null), 5000);
    });
    await frame(OP.Handshake, { v: 1, client_id: id });
    const hello = await got;
    if (!hello) throw new Error("Discord didn't answer. Check the Client ID.");
    if (hello.user?.id === ARRPC_USER || hello.user?.username === "arrpc") {
      throw new Error("Found Equibop or Vesktop instead of Discord. For Equibop, choose it under App.");
    }
    if (!session.load(id)) {
      // Discord shows the prompt inside its own window, which may be behind others or in the tray.
      host.status("connecting", "Open the Discord app. It shows a Keyside window: press Authorize.");
      host.toast("Open Discord and press Authorize");
      const { code } = await command("AUTHORIZE", { client_id: id, scopes: SCOPES }, undefined, 120_000).catch((e) => {
        throw new Error(authorizeError(errorText(e)));
      });
      const t = await http(endpoints.token, { form: { client_id: id, client_secret: secret, grant_type: "authorization_code", code, redirect_uri: REDIRECT } }).catch((e) => {
        throw new Error(tokenError(errorText(e)));
      });
      await session.save(fromResponse(t, id));
    }
    const auth = await command("AUTHENTICATE", { access_token: await session.token() }).catch(async () => {
      await session.clear();
      throw new Error("Discord sign-in expired. Save the plugin settings to sign in again.");
    });
    if (gen !== generation) return;
    ready = true;
    host.status("connected", auth.user?.global_name ?? auth.user?.username);
    applyVoice(await command("GET_VOICE_SETTINGS"));
    await command("SUBSCRIBE", {}, "VOICE_SETTINGS_UPDATE");
    await command("SUBSCRIBE", {}, "VOICE_CHANNEL_SELECT");
    const ch = await command("GET_SELECTED_VOICE_CHANNEL").catch(() => null);
    voice.channel = ch?.id ?? "";
    host.setVar("channel", ch?.name ?? "");
  } catch (e) {
    if (gen !== generation) return;
    const msg = errorText(e);
    host.status(/not running|no Discord/i.test(msg) ? "offline" : "error", /not running|no Discord/i.test(msg) ? "Discord isn't running" : msg);
    if (/not running|no Discord|didn't answer/i.test(msg)) retryLater(gen);
  }
  render();
}

function authorizeError(msg: string) {
  if (/no answer in time/.test(msg)) return "Nobody pressed Authorize in Discord. Press Restart to ask again.";
  if (/cancel|denied|access_denied/i.test(msg)) return "Authorize was cancelled in Discord. Press Restart to ask again.";
  return msg;
}

function tokenError(msg: string) {
  if (/redirect/i.test(msg)) return "Add `http://127.0.0.1` under OAuth2 > Redirects in the Discord Developer Portal, then press Restart.";
  if (/invalid_client|unauthorized/i.test(msg)) return "Discord didn't accept the Client secret. Copy it again and save.";
  return msg;
}

/** One pending reconnect at most, so two failures can't start two sign-ins. */
function retryLater(gen: number) {
  if (retryTimer) clearTimeout(retryTimer);
  retryTimer = setTimeout(() => {
    retryTimer = null;
    if (gen === generation) void connect(gen);
  }, RETRY);
}

const discordOnly = new Set(["join", "leave", "input-volume", "output-volume", "processing"]);

function view(k: Key): KeyView | null {
  if (!ready) return { disabled: true };
  if (equibop() && discordOnly.has(k.action)) return { disabled: true, sub: "Discord app only" };
  switch (k.action) {
    case "mute":
      return voice.mute || voice.deaf ? { tone: "danger", badge: "MUTED", badgeTone: "danger", icon: "mic-off" } : { badge: "ON", badgeTone: "success" };
    case "deafen":
      return voice.deaf ? { tone: "danger", badge: "DEAF", badgeTone: "danger", icon: "volume-x" } : null;
    case "processing":
      return voice[processingOf(k.settings)] ? { tone: "accent", badge: "ON", badgeTone: "accent" } : { badge: "OFF", badgeTone: "muted" };
    case "voice-mode":
      return voice.ptt ? { tone: "accent", badge: "PTT", badgeTone: "accent" } : { badge: "VOICE", badgeTone: "success" };
    case "join":
      return voice.channel && voice.channel === k.settings.channel ? { tone: "accent", badge: "IN", badgeTone: "accent" } : null;
    case "leave":
      return voice.channel ? null : { disabled: true };
    case "input-volume":
      return { value: clamp01(voice.input), meta: `${Math.round(voice.input * 100)}%` };
    case "output-volume":
      return { value: clamp01(voice.output), meta: `${Math.round(voice.output * 200)}%` };
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
  host.setVar("muted", voice.mute || voice.deaf);
  host.setVar("deafened", voice.deaf);
}

function restart() {
  generation++;
  launcher = null;
  if (retryTimer) clearTimeout(retryTimer);
  retryTimer = null;
  waiting.failAll(new Error("restarting"));
  const { id, secret } = cfg();
  session = new Session(host, { refresh: (t) => http(endpoints.token, { form: { client_id: id, client_secret: secret, grant_type: "refresh_token", refresh_token: t.refresh ?? "" } }) });
  void connect(generation);
}

function connected() {
  if (!ready) throw new Error("Discord isn't connected");
}

const set = (args: Record<string, unknown>) => (connected(), command("SET_VOICE_SETTINGS", args).then(applyVoice));

/** Equibop only toggles, so it's sent only when the wanted state differs from the last one sent. */
async function equibopSet(which: "mute" | "deaf" | "ptt", on: boolean) {
  if (voice[which] === on) return;
  await toggleInEquibop(which === "mute" ? "--toggle-mic" : which === "deaf" ? "--toggle-deafen" : "--toggle-vad");
  voice[which] = on;
  render();
}

function setMute(on: boolean) {
  return equibop() ? equibopSet("mute", on) : set({ mute: on });
}

function processingOf(s: Record<string, unknown>): Processing {
  const w = String(s.which ?? "noise");
  return w in PROCESSING ? (w as Processing) : "noise";
}

function onlyDiscord() {
  if (equibop()) throw new Error("Equibop can't do this. It works with the Discord app.");
  connected();
}

export default definePlugin({
  start(h) {
    host = h;
    host.ipc.on("discord", onFrame, () => {
      open = false;
      if (equibop()) return;
      ready = false;
      waiting.failAll(new Error("Discord closed"));
      host.status("offline", "Discord isn't running");
      render();
      retryLater(generation);
    });
    restart();
  },
  connectionChanged: restart,
  stop() {
    generation++;
    if (retryTimer) clearTimeout(retryTimer);
  },
  actions: {
    mute: {
      press: ({ settings }) => setMute(settings.mode === "hold" ? false : want(settings.mode ?? "toggle", voice.mute)),
      release: ({ settings }) => (settings.mode === "hold" ? setMute(true) : undefined),
      appear: render,
    },
    deafen: {
      press: ({ settings }) => (equibop() ? equibopSet("deaf", want(settings.mode ?? "toggle", voice.deaf)) : set({ deaf: want(settings.mode ?? "toggle", voice.deaf) })),
      appear: render,
    },
    "voice-mode": {
      press({ settings }) {
        const ptt = want(settings.mode ?? "toggle", voice.ptt);
        return equibop() ? equibopSet("ptt", ptt) : set({ mode: { type: ptt ? "PUSH_TO_TALK" : "VOICE_ACTIVITY" } });
      },
      appear: render,
    },
    processing: {
      press({ settings }) {
        onlyDiscord();
        const which = processingOf(settings);
        return set({ [PROCESSING[which]]: want(settings.mode ?? "toggle", voice[which]) });
      },
      appear: render,
    },
    join: {
      async press({ settings }) {
        onlyDiscord();
        const id = String(settings.channel ?? "").trim();
        if (!/^\d{5,24}$/.test(id)) throw new Error("Channel ID should be a long number");
        await command("SELECT_VOICE_CHANNEL", { channel_id: id, force: true });
      },
      appear: render,
    },
    leave: { press: () => (onlyDiscord(), command("SELECT_VOICE_CHANNEL", { channel_id: null })), appear: render },
    "input-volume": { slide: ({ value }) => (onlyDiscord(), set({ input: { volume: Math.round(clamp01(value) * 100) } })), appear: render },
    "output-volume": { slide: ({ value }) => (onlyDiscord(), set({ output: { volume: Math.round(clamp01(value) * 200) } })), appear: render },
  },
});

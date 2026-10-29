// Discord desktop RPC. The companion opens Discord's local IPC socket for us and relays
// frames; this plugin handles sign-in and commands.

import { definePlugin, setting, type Host, type Key, type KeyView } from "@phonedeck/sdk";
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
let readyWait: ((ok: boolean) => void) | null = null;
const voice = { mute: false, deaf: false, input: 1, output: 1, channel: "" };

const cfg = () => ({ id: String(setting(host.connection, "clientId", "")).trim(), secret: String(setting(host.connection, "clientSecret", "")).trim() });

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
    readyWait?.(true);
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
  if (!id || !secret) {
    host.status("offline", "Add your Discord Application ID and Client secret in the plugin settings.");
    return;
  }
  try {
    if (!open) {
      await host.ipc.open("discord");
      open = true;
    }
    const got = new Promise<boolean>((r) => {
      readyWait = r;
      setTimeout(() => r(false), 5000);
    });
    await frame(OP.Handshake, { v: 1, client_id: id });
    if (!(await got)) throw new Error("Discord didn't answer. Check the Application ID.");
    if (!session.load(id)) {
      host.status("connecting", "Allow PhoneDeck in the Discord window");
      const { code } = await command("AUTHORIZE", { client_id: id, scopes: SCOPES }, undefined, 120_000);
      const t = await http(endpoints.token, { form: { client_id: id, client_secret: secret, grant_type: "authorization_code", code, redirect_uri: REDIRECT } });
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
    if (/not running|no Discord|didn't answer/i.test(msg)) retryTimer = setTimeout(() => gen === generation && void connect(gen), RETRY);
  }
  render();
}

function view(k: Key): KeyView | null {
  if (!ready) return { disabled: true };
  switch (k.action) {
    case "mute":
      return voice.mute || voice.deaf ? { tone: "danger", badge: "MUTED", badgeTone: "danger", icon: "mic-off" } : { badge: "ON", badgeTone: "success" };
    case "deafen":
      return voice.deaf ? { tone: "danger", badge: "DEAF", badgeTone: "danger", icon: "volume-x" } : null;
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
  if (retryTimer) clearTimeout(retryTimer);
  waiting.failAll(new Error("restarting"));
  const { id, secret } = cfg();
  session = new Session(host, { refresh: (t) => http(endpoints.token, { form: { client_id: id, client_secret: secret, grant_type: "refresh_token", refresh_token: t.refresh ?? "" } }) });
  void connect(generation);
}

function connected() {
  if (!ready) throw new Error("Discord isn't connected");
}

const set = (args: Record<string, unknown>) => (connected(), command("SET_VOICE_SETTINGS", args).then(applyVoice));

export default definePlugin({
  start(h) {
    host = h;
    host.ipc.on("discord", onFrame, () => {
      open = false;
      ready = false;
      waiting.failAll(new Error("Discord closed"));
      host.status("offline", "Discord isn't running");
      render();
      const gen = generation;
      retryTimer = setTimeout(() => gen === generation && void connect(gen), RETRY);
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
      press: ({ settings }) => set({ mute: settings.mode === "hold" ? false : want(settings.mode ?? "toggle", voice.mute) }),
      release: ({ settings }) => (settings.mode === "hold" ? set({ mute: true }) : undefined),
      appear: render,
    },
    deafen: { press: ({ settings }) => set({ deaf: want(settings.mode ?? "toggle", voice.deaf) }), appear: render },
    join: {
      async press({ settings }) {
        connected();
        const id = String(settings.channel ?? "").trim();
        if (!/^\d{5,24}$/.test(id)) throw new Error("Channel ID should be a long number");
        await command("SELECT_VOICE_CHANNEL", { channel_id: id, force: true });
      },
      appear: render,
    },
    leave: { press: () => (connected(), command("SELECT_VOICE_CHANNEL", { channel_id: null })), appear: render },
    "input-volume": { slide: ({ value }) => set({ input: { volume: Math.round(clamp01(value) * 100) } }), appear: render },
    "output-volume": { slide: ({ value }) => set({ output: { volume: Math.round(clamp01(value) * 200) } }), appear: render },
  },
});

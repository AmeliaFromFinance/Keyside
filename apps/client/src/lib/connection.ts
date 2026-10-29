// One connection to a companion: the local editor socket, or a paired device over the
// encrypted channel. Reconnects on its own and measures latency.

import { ClientHandshake, importPsk, type Session, type DeviceInfo } from "@phonedeck/protocol/channel";
import { fromB64u } from "@phonedeck/protocol";
import type { ClientMsg, ServerMsg } from "@phonedeck/protocol";
import { companions, type PairParams, type StoredCompanion } from "./companions.ts";
import { isNative } from "./native.ts";

export type Target = { kind: "admin" } | { kind: "device"; companion: StoredCompanion } | { kind: "pair"; params: PairParams; device: DeviceInfo };

export type ConnState =
  | { s: "connecting"; attempt: number }
  | { s: "waiting"; sas: string }
  | { s: "open" }
  | { s: "closed"; reason: string; retry: boolean };

type Handler = (m: ServerMsg | { t: "waiting"; sas: string } | { t: "error"; reason: string }) => void;

const HELLO_TIMEOUT = 8000;

export class Connection {
  #target: Target;
  #ws: WebSocket | null = null;
  #session: Session | null = null;
  #reqId = 0;
  #pending = new Map<number, { ok: (v: any) => void; fail: (e: Error) => void; timer: ReturnType<typeof setTimeout> }>();
  #retry = 0;
  #stopped = false;
  #pingTimer: ReturnType<typeof setInterval> | null = null;
  #retryTimer: ReturnType<typeof setTimeout> | null = null;
  onMessage: Handler = () => {};
  onState: (s: ConnState) => void = () => {};
  onPaired: (c: StoredCompanion) => void = () => {};
  latency = 0;

  constructor(target: Target) {
    this.#target = target;
  }

  get target() {
    return this.#target;
  }

  start() {
    this.#stopped = false;
    void this.#connect();
  }

  stop() {
    this.#stopped = true;
    if (this.#retryTimer) clearTimeout(this.#retryTimer);
    this.#teardown();
  }

  #teardown() {
    if (this.#pingTimer) clearInterval(this.#pingTimer);
    this.#pingTimer = null;
    const ws = this.#ws;
    this.#ws = null;
    this.#session = null;
    if (ws && ws.readyState <= 1) ws.close();
    for (const p of this.#pending.values()) {
      clearTimeout(p.timer);
      p.fail(new Error("disconnected"));
    }
    this.#pending.clear();
  }

  #urls(): string[] {
    const t = this.#target;
    if (t.kind === "admin") return [`ws://${location.host}/admin/ws`];
    const hosts = t.kind === "pair" ? t.params.hosts : t.companion.hosts;
    const port = t.kind === "pair" ? t.params.port : t.companion.port;
    // Browsers need wss (the page itself came from the companion over https).
    // The native app connects in plain ws; the channel is encrypted end to end anyway.
    if (!isNative() && location.protocol === "https:") {
      const here = location.hostname.replace(/^\[|\]$/g, "");
      const ordered = [...hosts].sort((a, b) => Number(b.replace(/^\[|\]$/g, "") === here) - Number(a.replace(/^\[|\]$/g, "") === here));
      return ordered.map((h) => `wss://${h}:${port}/ws`);
    }
    return hosts.map((h) => `ws://${h}:${port}/ws`);
  }

  async #open(url: string): Promise<WebSocket> {
    return new Promise((resolve, reject) => {
      const ws = new WebSocket(url);
      ws.binaryType = "arraybuffer";
      const timer = setTimeout(() => {
        ws.close();
        reject(new Error("timeout"));
      }, 4000);
      ws.onopen = () => {
        clearTimeout(timer);
        resolve(ws);
      };
      ws.onerror = () => {
        clearTimeout(timer);
        reject(new Error("unreachable"));
      };
    });
  }

  async #connect() {
    if (this.#stopped) return;
    this.onState({ s: "connecting", attempt: this.#retry });
    let ws: WebSocket | null = null;
    let usedHost = "";
    for (const url of this.#urls()) {
      try {
        ws = await this.#open(url);
        usedHost = new URL(url).hostname;
        break;
      } catch {
        /* try the next address */
      }
    }
    if (!ws) return this.#closed("unreachable", true);
    this.#ws = ws;
    try {
      if (this.#target.kind === "admin") {
        ws.onmessage = (e) => this.#deliver(JSON.parse(String(e.data)));
      } else {
        await this.#handshake(ws);
        if (this.#target.kind === "device") void companions.touch(this.#target.companion.cid);
        void usedHost;
      }
    } catch (e) {
      const reason = e instanceof Error ? e.message : String(e);
      return this.#closed(reason, !["pairing-expired", "unknown-device", "wrong-companion", "bad-proof", "denied"].includes(reason));
    }
    ws.onclose = () => {
      if (this.#ws === ws) this.#closed("disconnected", true);
    };
    this.#retry = 0;
    this.#pingTimer = setInterval(() => this.send({ t: "ping", ts: Math.round(performance.now()) }), 10_000);
    this.send({ t: "ping", ts: Math.round(performance.now()) });
  }

  #nextText(ws: WebSocket): Promise<any> {
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error("timeout")), HELLO_TIMEOUT);
      ws.onmessage = (e) => {
        clearTimeout(timer);
        if (typeof e.data !== "string") return reject(new Error("bad-proof"));
        const m = JSON.parse(e.data);
        if (m.t === "error") reject(new Error(m.reason));
        else resolve(m);
      };
      ws.onclose = () => {
        clearTimeout(timer);
        reject(new Error("disconnected"));
      };
    });
  }

  async #handshake(ws: WebSocket) {
    const t = this.#target;
    const opts =
      t.kind === "pair"
        ? { mode: "pair" as const, cid: t.params.cid, id: t.params.pid, psk: await importPsk(fromB64u(t.params.code, 16)), device: t.device }
        : t.kind === "device"
          ? { mode: "resume" as const, cid: t.companion.cid, id: t.companion.deviceId, psk: t.companion.psk }
          : null;
    if (!opts) throw new Error("bad target");
    const { hs, hello } = await ClientHandshake.start(opts);
    const reply = this.#nextText(ws);
    ws.send(JSON.stringify(hello));
    const res = await hs.finish(await reply);
    ws.send(JSON.stringify(res.confirm));
    this.#session = res.session;
    const session = res.session;
    ws.onmessage = async (e) => {
      if (typeof e.data === "string") {
        const m = JSON.parse(e.data);
        if (m.t === "error") this.#closed(m.reason, false);
        return;
      }
      let m: any;
      try {
        m = await session.open<any>(new Uint8Array(e.data as ArrayBuffer));
      } catch {
        this.#closed("bad-frame", true);
        return;
      }
      try {
        await this.#deviceMessage(m);
      } catch (err) {
        console.error("PhoneDeck: could not handle message", m?.t, err);
        if (m?.t === "paired") this.#closed("storage", false);
      }
    };
  }

  async #deviceMessage(m: any) {
    const t = this.#target;
    if (m.t === "waiting") {
      this.onState({ s: "waiting", sas: m.sas });
      this.onMessage(m);
      return;
    }
    if (m.t === "paired" && t.kind === "pair") {
      const stored: StoredCompanion = {
        cid: t.params.cid,
        name: m.companion?.name ?? t.params.name,
        hosts: [...t.params.hosts],
        port: t.params.port,
        deviceId: m.deviceId,
        psk: await importPsk(fromB64u(m.secret, 32)),
        pairedAt: Date.now(),
        lastUsed: Date.now(),
      };
      await companions.put(stored);
      this.#target = { kind: "device", companion: stored };
      this.onPaired(stored);
      return;
    }
    if (m.t === "bye") {
      this.#closed(m.reason, m.reason === "shutdown");
      return;
    }
    this.#deliver(m);
  }

  #deliver(m: any) {
    if (m.t === "welcome") this.onState({ s: "open" });
    if (m.t === "pong") {
      this.latency = Math.max(0, Math.round(performance.now() - m.ts));
    }
    if (m.t === "result") {
      const p = this.#pending.get(m.id);
      if (p) {
        clearTimeout(p.timer);
        this.#pending.delete(m.id);
        if (m.ok) p.ok(m.data);
        else p.fail(Object.assign(new Error(m.error?.message ?? "failed"), { code: m.error?.code }));
      }
      return;
    }
    this.onMessage(m);
  }

  #closed(reason: string, retry: boolean) {
    this.#teardown();
    if (this.#stopped) return;
    this.onState({ s: "closed", reason, retry });
    if (!retry || this.#target.kind === "pair") return;
    const delay = Math.min(10_000, 500 * 2 ** this.#retry++);
    this.#retryTimer = setTimeout(() => void this.#connect(), delay);
  }

  /** Retries right away, for example when the app comes back to the foreground. */
  nudge() {
    if (!this.#ws && !this.#stopped) {
      if (this.#retryTimer) clearTimeout(this.#retryTimer);
      this.#retry = 0;
      void this.#connect();
    }
  }

  send(m: ClientMsg) {
    const ws = this.#ws;
    if (!ws || ws.readyState !== 1) return;
    if (this.#target.kind === "admin") ws.send(JSON.stringify(m));
    else if (this.#session) void this.#session.seal(m).then((f) => ws.readyState === 1 && ws.send(f));
  }

  req<T = any>(op: string, args?: unknown, timeout = 30_000): Promise<T> {
    const id = ++this.#reqId;
    return new Promise<T>((ok, fail) => {
      const timer = setTimeout(() => {
        this.#pending.delete(id);
        fail(new Error("The companion did not answer in time"));
      }, timeout);
      this.#pending.set(id, { ok, fail, timer });
      this.send({ t: "req", id, op, args });
    });
  }
}

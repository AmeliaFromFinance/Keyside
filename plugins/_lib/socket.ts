// Reconnecting WebSocket for plugins that talk to an app on the PC or LAN.

export interface SocketEvents {
  open(): void | Promise<void>;
  message(data: string): void;
  /** Return "stop" to give up reconnecting (for example after a wrong password). */
  close(reason: string, code?: number): void | "stop";
}

export class Reconnecting {
  #url: () => string | null;
  #ev: SocketEvents;
  #protocols?: string | string[];
  #maxDelay: number;
  #ws: WebSocket | null = null;
  #timer: ReturnType<typeof setTimeout> | null = null;
  #attempt = 0;
  #stopped = true;

  constructor(url: () => string | null, ev: SocketEvents, opts: { protocols?: string | string[]; maxDelay?: number } = {}) {
    this.#url = url;
    this.#ev = ev;
    this.#protocols = opts.protocols;
    this.#maxDelay = opts.maxDelay ?? 15_000;
  }

  get ready() {
    return this.#ws?.readyState === 1;
  }

  start() {
    this.#stopped = false;
    this.#connect();
  }

  stop() {
    this.#stopped = true;
    if (this.#timer) clearTimeout(this.#timer);
    this.#timer = null;
    const ws = this.#ws;
    this.#ws = null;
    if (ws && ws.readyState <= 1) ws.close();
  }

  restart() {
    this.stop();
    this.#attempt = 0;
    this.start();
  }

  /** Drops the current socket and reconnects after the usual delay. */
  drop(reason: string) {
    const ws = this.#ws;
    if (!ws) return;
    this.#ws = null;
    if (ws.readyState <= 1) ws.close();
    this.#ev.close(reason);
    this.#schedule();
  }

  /** Call once the app accepted us, so the next drop retries quickly. */
  healthy() {
    this.#attempt = 0;
  }

  send(data: string) {
    if (!this.#ws || this.#ws.readyState !== 1) throw new Error("not connected");
    this.#ws.send(data);
  }

  #connect() {
    if (this.#stopped) return;
    const url = this.#url();
    if (!url) {
      this.#ev.close("not set up");
      return;
    }
    let ws: WebSocket;
    try {
      ws = new WebSocket(url, this.#protocols);
    } catch (e) {
      this.#ev.close(e instanceof Error ? e.message : String(e));
      this.#schedule();
      return;
    }
    ws.binaryType = "arraybuffer";
    this.#ws = ws;
    ws.onopen = () => {
      if (this.#ws === ws) void Promise.resolve(this.#ev.open()).catch((e) => this.drop(e instanceof Error ? e.message : String(e)));
    };
    ws.onmessage = (e) => {
      if (this.#ws !== ws) return;
      const text = typeof e.data === "string" ? e.data : new TextDecoder().decode(e.data as ArrayBuffer);
      try {
        this.#ev.message(text);
      } catch {
        /* a bad message shouldn't drop the connection */
      }
    };
    ws.onerror = () => {};
    ws.onclose = (e) => {
      if (this.#ws !== ws) return;
      this.#ws = null;
      if (this.#ev.close(e.reason || (e.code === 1006 ? "can't reach the app" : `closed (${e.code})`), e.code) === "stop") this.#stopped = true;
      else this.#schedule();
    };
  }

  #schedule() {
    if (this.#stopped || this.#timer) return;
    const delay = Math.min(this.#maxDelay, 1000 * 2 ** this.#attempt++);
    this.#timer = setTimeout(() => {
      this.#timer = null;
      this.#connect();
    }, delay);
  }
}

/** Matches replies to requests by id. */
export class Waiters<T = any> {
  #m = new Map<string, { ok: (v: T) => void; fail: (e: Error) => void; timer: ReturnType<typeof setTimeout> }>();

  wait(id: string, ms = 10_000): Promise<T> {
    return new Promise((ok, fail) => {
      const timer = setTimeout(() => {
        this.#m.delete(id);
        fail(new Error("no answer in time"));
      }, ms);
      this.#m.set(id, { ok, fail, timer });
    });
  }

  resolve(id: string, v: T) {
    const w = this.#m.get(id);
    if (!w) return;
    clearTimeout(w.timer);
    this.#m.delete(id);
    w.ok(v);
  }

  reject(id: string, e: Error) {
    const w = this.#m.get(id);
    if (!w) return;
    clearTimeout(w.timer);
    this.#m.delete(id);
    w.fail(e);
  }

  failAll(e: Error) {
    for (const id of [...this.#m.keys()]) this.reject(id, e);
  }
}

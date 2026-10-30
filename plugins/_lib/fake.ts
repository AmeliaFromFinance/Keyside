// Test helper: a local WebSocket server standing in for the app a plugin talks to.

import { WebSocketServer, type WebSocket } from "ws";
import type { AddressInfo } from "node:net";

export interface Fake {
  port: number;
  /** Every text message received, parsed as JSON when possible. */
  received: any[];
  clients: Set<WebSocket>;
  broadcast(msg: unknown): void;
  /** Resolves once a message matching `fn` arrives. */
  until(fn: (m: any) => boolean, ms?: number): Promise<any>;
  close(): Promise<void>;
}

export async function fakeServer(handler: (ws: WebSocket, msg: any, raw: string) => void, onConnect?: (ws: WebSocket, url: string) => void): Promise<Fake> {
  const wss = new WebSocketServer({ host: "127.0.0.1", port: 0 });
  await new Promise<void>((r) => wss.once("listening", () => r()));
  const received: any[] = [];
  const waiters: Array<{ fn: (m: any) => boolean; ok: (m: any) => void }> = [];
  wss.on("connection", (ws, req) => {
    onConnect?.(ws, req.url ?? "/");
    ws.on("message", (data) => {
      const raw = data.toString();
      let msg: any = raw;
      try {
        msg = JSON.parse(raw);
      } catch {
        /* plain text protocol */
      }
      received.push(msg);
      for (const w of [...waiters]) if (w.fn(msg)) (waiters.splice(waiters.indexOf(w), 1), w.ok(msg));
      handler(ws, msg, raw);
    });
  });
  return {
    port: (wss.address() as AddressInfo).port,
    received,
    clients: wss.clients,
    broadcast(msg) {
      for (const c of wss.clients) c.send(typeof msg === "string" ? msg : JSON.stringify(msg));
    },
    until(fn, ms = 3000) {
      const hit = received.find(fn);
      if (hit) return Promise.resolve(hit);
      return new Promise((ok, fail) => {
        const t = setTimeout(() => fail(new Error("timed out waiting for message")), ms);
        waiters.push({ fn, ok: (m) => (clearTimeout(t), ok(m)) });
      });
    },
    close: () =>
      new Promise((r) => {
        for (const c of wss.clients) c.terminate();
        wss.close(() => r());
      }),
  };
}

export async function eventually(check: () => boolean | unknown, ms = 5000) {
  const end = Date.now() + ms;
  for (;;) {
    let ok = false;
    try {
      ok = !!check();
    } catch {
      ok = false;
    }
    if (ok) return;
    if (Date.now() > end) {
      check();
      throw new Error("condition not met in time");
    }
    await new Promise((r) => setTimeout(r, 20));
  }
}

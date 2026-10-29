// Live deck state shared by the deck view and the editor.

import type { ConnectionView, KeyView, PageView, StatusPill, Tone, FormFactor } from "@phonedeck/protocol";
import { Connection, type ConnState, type Target } from "./connection.ts";
import type { StoredCompanion } from "./companions.ts";

export interface Toast {
  text: string;
  tone: Tone;
  at: number;
}

export interface Self {
  id: string;
  name: string;
  scope: "deck" | "moderator" | "editor";
  pages?: string[];
  admin?: boolean;
}

export const deck = $state({
  conn: { s: "connecting", attempt: 0 } as ConnState,
  companion: null as { id: string; name: string; version: string } | null,
  self: null as Self | null,
  locked: false,
  lockReason: "" as string,
  retryAfter: 0,
  profile: null as { id: string; name: string } | null,
  page: null as PageView | null,
  rev: 0,
  connections: [] as ConnectionView[],
  pills: [] as StatusPill[],
  toast: null as Toast | null,
  latency: 0,
  sas: "",
});

let conn: Connection | null = null;
const listeners = new Set<(m: any) => void>();

export function connection(): Connection | null {
  return conn;
}

/** Extra handler for messages the deck itself ignores (editor events). */
export function listen(fn: (m: any) => void): () => void {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

export function connect(target: Target, onPaired?: (c: StoredCompanion) => void) {
  conn?.stop();
  deck.page = null;
  deck.self = null;
  deck.companion = null;
  const c = new Connection(target);
  conn = c;
  c.onState = (s) => {
    deck.conn = s;
    if (s.s === "waiting") deck.sas = s.sas;
  };
  c.onPaired = (stored) => onPaired?.(stored);
  c.onMessage = (m) => {
    deck.latency = c.latency;
    handle(m);
    for (const l of listeners) l(m);
  };
  c.start();
  return c;
}

export function disconnect() {
  conn?.stop();
  conn = null;
}

function handle(m: any) {
  switch (m.t) {
    case "welcome":
      deck.companion = m.companion;
      deck.self = m.device;
      deck.locked = m.locked;
      sendViewport();
      break;
    case "locked":
      deck.locked = true;
      deck.lockReason = m.reason ?? "pin";
      deck.retryAfter = m.retryAfter ?? 0;
      break;
    case "unlocked":
      deck.locked = false;
      deck.lockReason = "";
      break;
    case "page":
      deck.page = m.page;
      deck.profile = m.profile;
      deck.rev = m.rev;
      break;
    case "keys":
      if (deck.page && deck.page.id === m.page) {
        for (const k of m.keys as KeyView[]) {
          const i = deck.page.keys.findIndex((x) => x.id === k.id);
          if (i >= 0) deck.page.keys[i] = k;
        }
      }
      deck.rev = m.rev;
      break;
    case "status":
      deck.connections = m.connections;
      deck.pills = m.pills;
      break;
    case "toast":
      deck.toast = { text: m.text, tone: m.tone, at: m.at ?? Date.now() };
      break;
  }
}

export function formFactor(): FormFactor {
  const w = innerWidth;
  const h = innerHeight;
  const short = Math.min(w, h);
  if (short >= 700 && w >= 1100) return "desktop";
  if (short >= 600) return "tablet";
  return w > h ? "phone-landscape" : "phone-portrait";
}

let lastFF = "";
export function sendViewport() {
  const ff = formFactor();
  if (ff === lastFF && deck.page) return;
  lastFF = ff;
  conn?.send({ t: "viewport", formFactor: ff, width: innerWidth, height: innerHeight });
}

export function press(key: string, phase: "down" | "up" | "cancel") {
  conn?.send({ t: "input", key, phase });
}

export function slide(key: string, value: number) {
  conn?.send({ t: "slide", key, value });
}

export function nav(to: "next" | "prev" | "back" | "home" | "page", page?: string) {
  conn?.send({ t: "nav", to, page });
}

export function unlock(pin: string) {
  conn?.send({ t: "unlock", pin });
}

export function req<T = any>(op: string, args?: unknown): Promise<T> {
  if (!conn) return Promise.reject(new Error("not connected"));
  return conn.req<T>(op, args);
}

/** Image assets are cached by id, they never change. */
const images = new Map<string, Promise<string | null>>();
export function assetImage(id: string): Promise<string | null> {
  let p = images.get(id);
  if (!p) {
    p = req<string | null>("asset.image", { id }).catch(() => null);
    images.set(id, p);
  }
  return p;
}

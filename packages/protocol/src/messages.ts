// Messages sent inside the secure channel (remote devices) or the local admin socket.

import type { ConnectionView, FormFactor, KeyView, PageView, Scope, Tone } from "./model.ts";

export interface StatusPill {
  id: string;
  label: string;
  tone: Tone;
  /** Epoch ms for a running clock, e.g. recording time. */
  since?: number;
}

export type ServerMsg =
  | { t: "welcome"; companion: { id: string; name: string; version: string }; device: DeviceSelf; locked: boolean }
  | { t: "paired"; deviceId: string; secret: string; companion: { id: string; name: string } }
  | { t: "locked"; retryAfter?: number; reason?: "pin" | "wrong-pin" | "too-many-tries" }
  | { t: "unlocked" }
  | { t: "page"; rev: number; profile: { id: string; name: string }; page: PageView; locked: boolean }
  | { t: "keys"; rev: number; page: string; keys: KeyView[] }
  | { t: "status"; connections: ConnectionView[]; pills: StatusPill[] }
  | { t: "toast"; text: string; tone: Tone; at: number }
  | { t: "pong"; ts: number }
  | { t: "result"; id: number; ok: true; data?: unknown }
  | { t: "result"; id: number; ok: false; error: { code: string; message: string } }
  | { t: "event"; name: string; data?: unknown }
  | { t: "bye"; reason: "revoked" | "shutdown" | "replaced" };

export interface DeviceSelf {
  id: string;
  name: string;
  scope: Scope;
  /** Moderator devices only see these pages. */
  pages?: string[];
}

export type ClientMsg =
  | { t: "viewport"; formFactor: FormFactor; width: number; height: number }
  | { t: "input"; key: string; phase: "down" | "up" | "cancel" }
  | { t: "slide"; key: string; value: number }
  | { t: "nav"; to: "next" | "prev" | "back" | "home" | "page"; page?: string }
  | { t: "ping"; ts: number }
  | { t: "unlock"; pin: string }
  | { t: "req"; id: number; op: string; args?: unknown };

/** Short, human readable error codes. */
export const ErrorCode = {
  Forbidden: "forbidden",
  NotFound: "not_found",
  Invalid: "invalid",
  Conflict: "conflict",
  RateLimited: "rate_limited",
  Unavailable: "unavailable",
  Internal: "internal",
} as const;

// Small helpers shared by the bundled plugins.

export function b64(bytes: Uint8Array): string {
  let s = "";
  for (const b of bytes) s += String.fromCharCode(b);
  return btoa(s);
}

export function b64url(bytes: Uint8Array): string {
  return b64(bytes).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

export async function sha256(data: string | Uint8Array): Promise<Uint8Array> {
  const bytes = typeof data === "string" ? new TextEncoder().encode(data) : data;
  return new Uint8Array(await crypto.subtle.digest("SHA-256", bytes as BufferSource));
}

export function randomId(n = 12): string {
  return b64url(crypto.getRandomValues(new Uint8Array(n)));
}

export const clamp01 = (v: number) => Math.min(1, Math.max(0, Number.isFinite(v) ? v : 0));

/** "toggle" | "on" | "off" against the current state. */
export function want(mode: unknown, current: boolean): boolean {
  return mode === "on" ? true : mode === "off" ? false : !current;
}

export function errorText(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

export class HttpError extends Error {
  readonly status: number;
  readonly body: unknown;
  constructor(status: number, message: string, body?: unknown) {
    super(message);
    this.status = status;
    this.body = body;
  }
}

/** fetch with a timeout and readable errors. Parses JSON replies. */
export async function http(
  url: string,
  init: { method?: string; headers?: Record<string, string>; json?: unknown; form?: Record<string, string>; timeout?: number; body?: string } = {},
): Promise<any> {
  const headers: Record<string, string> = { ...init.headers };
  let body: string | undefined = init.body;
  if (init.json !== undefined) {
    headers["content-type"] = "application/json";
    body = JSON.stringify(init.json);
  } else if (init.form) {
    headers["content-type"] = "application/x-www-form-urlencoded";
    body = new URLSearchParams(init.form).toString();
  }
  const ctl = new AbortController();
  const timer = setTimeout(() => ctl.abort(), init.timeout ?? 10_000);
  let res: Response;
  try {
    res = await fetch(url, { method: init.method ?? (body ? "POST" : "GET"), headers, body, signal: ctl.signal });
  } catch (e) {
    throw new Error(ctl.signal.aborted ? "request timed out" : `can't reach ${new URL(url).host}`);
  } finally {
    clearTimeout(timer);
  }
  const text = await res.text();
  let data: any = null;
  try {
    data = text ? JSON.parse(text) : null;
  } catch {
    data = text;
  }
  if (!res.ok) {
    const msg = (data && typeof data === "object" && (data.message || data.error_description || data.error?.message || data.error)) || res.statusText || "request failed";
    throw new HttpError(res.status, String(msg).slice(0, 200), data);
  }
  return data;
}

export const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

declare const Deno: any;

/** True the first time it's called for `key` (remembered in the plugin's data folder). */
export async function firstTime(dataDir: string, key: string): Promise<boolean> {
  if (typeof Deno === "undefined" || !dataDir) return true;
  const file = `${dataDir}/once-${key.replace(/[^A-Za-z0-9_-]/g, "_").slice(0, 80)}`;
  try {
    await Deno.stat(file);
    return false;
  } catch {
    await Deno.writeTextFile(file, String(Date.now())).catch(() => {});
    return true;
  }
}

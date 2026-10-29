// OAuth sessions for plugins that sign in to an online service. The session is saved in
// a hidden secret connection field, so it lands in the OS keychain.

import type { Host } from "@phonedeck/sdk";
import { HttpError, sleep } from "./util.ts";

export interface Tokens {
  access: string;
  refresh?: string;
  /** Epoch ms. */
  expires: number;
  /** Client id the tokens belong to. Changing the id signs out. */
  client: string;
  scope?: string;
}

export function fromResponse(r: any, client: string, prev?: Tokens): Tokens {
  return {
    access: String(r.access_token),
    refresh: r.refresh_token ? String(r.refresh_token) : prev?.refresh,
    expires: Date.now() + Number(r.expires_in ?? 3600) * 1000,
    client,
    scope: Array.isArray(r.scope) ? r.scope.join(" ") : (r.scope ?? prev?.scope),
  };
}

export class Session {
  #host: Host;
  #field: string;
  #refresh: (t: Tokens) => Promise<any>;
  #tokens: Tokens | null = null;
  #refreshing: Promise<string> | null = null;
  onSignedOut: () => void = () => {};

  constructor(host: Host, opts: { field?: string; refresh: (t: Tokens) => Promise<any> }) {
    this.#host = host;
    this.#field = opts.field ?? "session";
    this.#refresh = opts.refresh;
  }

  /** Loads the saved session if it belongs to this client id. */
  load(client: string): boolean {
    const raw = this.#host.connection[this.#field];
    this.#tokens = null;
    if (typeof raw !== "string" || !raw) return false;
    try {
      const t = JSON.parse(raw) as Tokens;
      if (t.client === client && t.access) this.#tokens = t;
    } catch {
      /* corrupted, sign in again */
    }
    return !!this.#tokens;
  }

  get signedIn() {
    return !!this.#tokens;
  }

  async save(t: Tokens) {
    this.#tokens = t;
    await this.#host.saveConnection({ [this.#field]: JSON.stringify(t) });
  }

  async clear() {
    this.#tokens = null;
    await this.#host.saveConnection({ [this.#field]: "" });
  }

  /** A valid access token, refreshed when it is about to expire. */
  async token(): Promise<string> {
    const t = this.#tokens;
    if (!t) throw new Error("not signed in");
    if (t.expires - Date.now() > 60_000) return t.access;
    return (this.#refreshing ??= this.#doRefresh(t).finally(() => (this.#refreshing = null)));
  }

  /** Call after a 401: forces a refresh once. */
  async renew(): Promise<string> {
    const t = this.#tokens;
    if (!t) throw new Error("not signed in");
    return (this.#refreshing ??= this.#doRefresh(t).finally(() => (this.#refreshing = null)));
  }

  async #doRefresh(t: Tokens): Promise<string> {
    if (!t.refresh) {
      await this.clear();
      this.onSignedOut();
      throw new Error("signed out, sign in again");
    }
    try {
      const r = await this.#refresh(t);
      const next = fromResponse(r, t.client, t);
      await this.save(next);
      return next.access;
    } catch (e) {
      if (e instanceof HttpError && (e.status === 400 || e.status === 401)) {
        await this.clear();
        this.onSignedOut();
        throw new Error("signed out, sign in again");
      }
      throw e;
    }
  }
}

/**
 * Polls a device-code token endpoint until the person approves the code on another screen.
 * `poll` should throw the provider's HttpError while approval is pending.
 */
export async function pollDeviceCode(poll: () => Promise<any>, intervalSec: number, expiresSec: number, cancelled: () => boolean): Promise<any> {
  let wait = Math.max(intervalSec || 5, 2) * 1000;
  const end = Date.now() + Math.min(expiresSec || 600, 1800) * 1000;
  while (Date.now() < end) {
    await sleep(wait);
    if (cancelled()) throw new Error("cancelled");
    try {
      return await poll();
    } catch (e) {
      const why = e instanceof HttpError ? JSON.stringify(e.body ?? e.message) : "";
      if (/authorization_pending/.test(why)) continue;
      if (/slow_down/.test(why)) {
        wait += 5000;
        continue;
      }
      throw e;
    }
  }
  throw new Error("The sign-in code expired. Save the plugin settings to get a new one.");
}

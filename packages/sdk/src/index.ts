// @phonedeck/sdk: write PhoneDeck plugins in TypeScript.
// Plugins run inside the companion's Deno sandbox and talk to it over stdin/stdout.

export type Tone = "default" | "accent" | "danger" | "success" | "warning" | "info" | "muted";
export type KeyColor = "default" | "teal" | "violet" | "green" | "amber" | "red" | "blue";
export type VarValue = string | number | boolean | null;
export type Settings = Record<string, unknown>;

/** What a plugin can change on one of its keys. Unset fields keep the layout's value. */
export interface KeyView {
  label?: string;
  sub?: string;
  icon?: string;
  color?: KeyColor;
  badge?: string;
  /** accent and danger change the key's border and background. */
  tone?: Tone;
  /** Badge colour. accent and danger draw a filled pill, the rest coloured text. */
  badgeTone?: Tone;
  meta?: string;
  /** png, jpeg, webp, gif or svg data URL, under 350 KB. */
  image?: string;
  /** 0..1, for sliders. */
  value?: number;
  /** 0..1, drawn as a bar. */
  progress?: number;
  /** Epoch ms a running clock starts from. Clients tick it themselves. */
  since?: number;
  disabled?: boolean;
}

export interface Pill {
  label: string;
  tone?: Tone;
  /** Epoch ms for a running clock. */
  since?: number;
}

export interface Option {
  value: string;
  label: string;
}

export interface Key {
  readonly id: string;
  readonly action: string;
  readonly settings: Settings;
  /** The label and icon set in the layout, for picking variants (mic -> mic-off). */
  readonly label: string;
  readonly icon?: string;
  /** Replaces this key's state. Fields you leave out show the layout's own values. */
  set(view: KeyView): void;
  /** Back to the layout's own look. */
  clear(): void;
}

export interface Step {
  key: Key;
  settings: Settings;
  /** Device id of whoever pressed the key. */
  device: string;
}

export interface ActionHandlers<S extends Settings = Settings> {
  press?(step: Step & { settings: S }): unknown | Promise<unknown>;
  release?(step: Step & { settings: S }): unknown | Promise<unknown>;
  /** Sliders: value is 0..1. */
  slide?(step: { key: Key; settings: S; value: number }): unknown | Promise<unknown>;
  /** Fill a dropdown field marked "x-options": "dynamic". */
  options?(field: string, settings: S): Option[] | Promise<Option[]>;
  /** A key using this action was added or its settings changed. */
  appear?(key: Key): void;
  disappear?(key: Key): void;
}

export interface PluginDefinition {
  /** Called once with the saved connection settings. */
  start?(host: Host): unknown | Promise<unknown>;
  /** Connection settings were edited (or saved by the plugin). */
  connectionChanged?(host: Host): unknown | Promise<unknown>;
  stop?(): unknown | Promise<unknown>;
  actions: Record<string, ActionHandlers<any>>;
  /**
   * Optional: runs several back-to-back steps of this plugin in one go (for example one
   * OBS request batch). Return false to fall back to running them one by one.
   */
  batch?(steps: Array<Step & { action: string }>, phase: "press" | "release"): boolean | Promise<boolean>;
}

export interface Host {
  readonly pluginId: string;
  readonly prefix: string;
  readonly locale: string;
  readonly dataDir: string;
  /** Current connection settings, secrets included. */
  readonly connection: Settings;
  /** Every placed key, optionally only those using one action. */
  keys(action?: string): Key[];
  key(id: string): Key | undefined;
  /** Publishes {prefix.name} for labels and conditions. */
  setVar(name: string, value: VarValue): void;
  /** Adds a line to a feed declared in the manifest (chat, alerts). Text only. */
  feed(name: string, item: { text: string; author?: string; color?: string }): void;
  /** Connection state shown in the deck header. `pills` adds up to three status chips, like REC. */
  status(state: "connected" | "connecting" | "offline" | "error", detail?: string, pills?: Pill[] | Pill | null): void;
  toast(text: string, tone?: "default" | "success" | "warning" | "danger"): void;
  log: Record<"debug" | "info" | "warn" | "error", (...args: unknown[]) => void>;
  /** Saves values into the connection settings (for example OAuth tokens). */
  saveConnection(values: Settings): Promise<void>;
  /** Opens an https page in the default browser on the PC. */
  openBrowser(url: string): Promise<void>;
  /**
   * Browser sign-in with a redirect back to PhoneDeck. Pass the provider's authorize URL;
   * PhoneDeck adds redirect_uri and state, then returns the code once the person signs in.
   */
  authorize(url: string): Promise<{ code: string; redirectUri: string }>;
  /** Sends one UDP datagram (needs the "udp" permission and the host in "net"). */
  udp(host: string, port: number, data: Uint8Array): Promise<void>;
  /** Local app bridges the companion opens for you (needs the "ipc" permission). */
  ipc: {
    open(bridge: "discord"): Promise<void>;
    send(bridge: "discord", data: unknown): Promise<void>;
    on(bridge: "discord", fn: (data: unknown) => void, closed?: () => void): void;
  };
}

/** Transport, swappable for tests. */
export interface Wire {
  write(line: string): void;
  lines(): AsyncIterable<string>;
}

interface Instance {
  id: string;
  action: string;
  settings: Settings;
  label?: string;
  icon?: string;
}

const FLUSH_MS = 50;

class Runtime implements Host {
  pluginId = "";
  prefix = "";
  locale = "en";
  dataDir = "";
  connection: Settings = {};
  #def: PluginDefinition;
  #wire: Wire;
  #instances = new Map<string, Instance>();
  #handles = new Map<string, Key>();
  #nextId = 1;
  #pending = new Map<number, { ok: (v: unknown) => void; fail: (e: Error) => void }>();
  #views = new Map<string, KeyView | null>();
  #vars = new Map<string, VarValue>();
  #timer: ReturnType<typeof setTimeout> | null = null;
  #ipc = new Map<string, { fn: (d: unknown) => void; closed?: () => void }>();

  constructor(def: PluginDefinition, wire: Wire) {
    this.#def = def;
    this.#wire = wire;
  }

  log = {
    debug: (...a: unknown[]) => this.#notify("log", { level: "debug", message: fmt(a) }),
    info: (...a: unknown[]) => this.#notify("log", { level: "info", message: fmt(a) }),
    warn: (...a: unknown[]) => this.#notify("log", { level: "warn", message: fmt(a) }),
    error: (...a: unknown[]) => this.#notify("log", { level: "error", message: fmt(a) }),
  };

  ipc = {
    open: (bridge: "discord") => this.#call("ipc.open", { bridge }).then(() => undefined),
    send: (bridge: "discord", data: unknown) => this.#call("ipc.send", { bridge, data }).then(() => undefined),
    on: (bridge: "discord", fn: (d: unknown) => void, closed?: () => void) => void this.#ipc.set(bridge, { fn, closed }),
  };

  #send(msg: unknown) {
    this.#wire.write(JSON.stringify(msg));
  }

  #notify(method: string, params: unknown) {
    this.#send({ jsonrpc: "2.0", method, params });
  }

  #call(method: string, params: unknown): Promise<unknown> {
    const id = this.#nextId++;
    this.#send({ jsonrpc: "2.0", id, method, params });
    return new Promise((ok, fail) => this.#pending.set(id, { ok, fail }));
  }

  #schedule() {
    this.#timer ??= setTimeout(() => this.#flush(), FLUSH_MS);
  }

  #flush() {
    this.#timer = null;
    for (const [instance, view] of this.#views) this.#notify("key.set", { instance, view });
    for (const [name, value] of this.#vars) this.#notify("var.set", { name, value });
    this.#views.clear();
    this.#vars.clear();
  }

  #handle(inst: Instance): Key {
    const existing = this.#handles.get(inst.id);
    if (existing && existing.action === (inst.action.split("/").pop() ?? inst.action)) {
      Object.assign(existing as object, { settings: inst.settings, label: inst.label ?? "", icon: inst.icon });
      return existing;
    }
    const rt = this;
    const k: Key = {
      id: inst.id,
      action: inst.action.split("/").pop() ?? inst.action,
      settings: inst.settings,
      label: inst.label ?? "",
      icon: inst.icon,
      set(view: KeyView) {
        if (!rt.#instances.has(inst.id)) return;
        const prev = rt.#views.get(inst.id);
        rt.#views.set(inst.id, prev ? { ...prev, ...view } : { ...view });
        rt.#schedule();
      },
      clear() {
        rt.#views.set(inst.id, null);
        rt.#schedule();
      },
    };
    this.#handles.set(inst.id, k);
    return k;
  }

  keys(action?: string): Key[] {
    const out: Key[] = [];
    for (const inst of this.#instances.values()) {
      const k = this.#handle(inst);
      if (!action || k.action === action) out.push(k);
    }
    return out;
  }

  key(id: string) {
    const inst = this.#instances.get(id);
    return inst ? this.#handle(inst) : undefined;
  }

  setVar(name: string, value: VarValue) {
    this.#vars.set(name, value);
    this.#schedule();
  }

  feed(name: string, item: { text: string; author?: string; color?: string }) {
    this.#notify("feed.push", { feed: name, item });
  }

  status(state: "connected" | "connecting" | "offline" | "error", detail?: string, pills?: Pill[] | Pill | null) {
    const params: Record<string, unknown> = { state, detail };
    if (pills !== undefined) params.pills = pills === null ? [] : Array.isArray(pills) ? pills : [pills];
    this.#notify("status.set", params);
  }

  toast(text: string, tone: "default" | "success" | "warning" | "danger" = "default") {
    this.#notify("toast", { text, tone });
  }

  async saveConnection(values: Settings) {
    await this.#call("connection.update", { values });
    Object.assign(this.connection, values);
  }

  async openBrowser(url: string) {
    await this.#call("browser.open", { url });
  }

  authorize(url: string) {
    return this.#call("oauth.authorize", { url }) as Promise<{ code: string; redirectUri: string }>;
  }

  async udp(host: string, port: number, data: Uint8Array) {
    let bin = "";
    for (const b of data) bin += String.fromCharCode(b);
    await this.#call("udp.send", { host, port, data: btoa(bin) });
  }

  #sync(list: Instance[]) {
    const next = new Map(list.map((i) => [i.id, i]));
    for (const [id, inst] of this.#instances) {
      if (!next.has(id)) {
        const k = this.#handle(inst);
        this.#instances.delete(id);
        this.#handles.delete(id);
        this.#views.delete(id);
        this.#def.actions[k.action]?.disappear?.(k);
      }
    }
    for (const [id, inst] of next) {
      const prev = this.#instances.get(id);
      this.#instances.set(id, inst);
      if (!prev || JSON.stringify(prev.settings) !== JSON.stringify(inst.settings) || prev.action !== inst.action) {
        const k = this.#handle(inst);
        this.#def.actions[k.action]?.appear?.(k);
      }
    }
  }

  async #run(p: { steps: Array<{ instance: string; action: string; settings: Settings }>; phase: "press" | "release"; device: string }) {
    const steps = p.steps.map((s) => {
      const key = this.#instances.has(s.instance) ? this.#handle(this.#instances.get(s.instance)!) : detachedKey(s.instance, s.action, s.settings);
      return { key, settings: s.settings, device: p.device, action: s.action };
    });
    if (steps.length > 1 && this.#def.batch && (await this.#def.batch(steps, p.phase))) return;
    for (const s of steps) {
      const h = this.#def.actions[s.action];
      if (!h) throw new Error(`unknown action ${s.action}`);
      await (p.phase === "press" ? h.press?.(s) : h.release?.(s));
    }
  }

  async #dispatch(method: string, params: any): Promise<unknown> {
    switch (method) {
      case "initialize":
        this.pluginId = params.pluginId;
        this.prefix = params.prefix;
        this.locale = params.locale ?? "en";
        this.dataDir = params.dataDir ?? "";
        this.connection = params.connection ?? {};
        await this.#def.start?.(this);
        return null;
      case "connection.changed":
        this.connection = params.connection ?? {};
        await this.#def.connectionChanged?.(this);
        return null;
      case "instances.sync":
        this.#sync(params.instances ?? []);
        return null;
      case "run":
        await this.#run(params);
        return null;
      case "slide": {
        const inst = this.#instances.get(params.instance);
        const key = inst ? this.#handle(inst) : detachedKey(params.instance, params.action, params.settings);
        await this.#def.actions[params.action]?.slide?.({ key, settings: params.settings, value: params.value });
        return null;
      }
      case "options": {
        const h = this.#def.actions[params.action];
        return (await h?.options?.(params.field, params.settings ?? {})) ?? [];
      }
      case "ipc.message":
        this.#ipc.get(params.bridge)?.fn(params.data);
        return null;
      case "ipc.closed":
        this.#ipc.get(params.bridge)?.closed?.();
        return null;
      case "shutdown":
        await this.#def.stop?.();
        return null;
      default:
        throw new Error(`unknown method ${method}`);
    }
  }

  async serve() {
    for await (const line of this.#wire.lines()) {
      if (!line.trim()) continue;
      let msg: any;
      try {
        msg = JSON.parse(line);
      } catch {
        continue;
      }
      if (msg.method) {
        const run = this.#dispatch(msg.method, msg.params ?? {});
        if (msg.id !== undefined) {
          run.then(
            (result) => this.#send({ jsonrpc: "2.0", id: msg.id, result: result ?? null }),
            (e) => this.#send({ jsonrpc: "2.0", id: msg.id, error: { code: -32000, message: errorText(e) } }),
          );
        } else {
          run.catch((e) => this.log.error(errorText(e)));
        }
      } else if (msg.id !== undefined) {
        const p = this.#pending.get(msg.id);
        this.#pending.delete(msg.id);
        if (msg.error) p?.fail(new Error(msg.error.message ?? "host error"));
        else p?.ok(msg.result);
      }
    }
    await this.#def.stop?.();
  }
}

function detachedKey(id: string, action: string, settings: Settings): Key {
  return { id, action, settings, label: "", set() {}, clear() {} };
}

function errorText(e: unknown): string {
  return (e instanceof Error ? e.message : String(e)).slice(0, 300);
}

function fmt(a: unknown[]): string {
  return a.map((x) => (typeof x === "string" ? x : x instanceof Error ? `${x.message}` : safeJson(x))).join(" ").slice(0, 2000);
}

function safeJson(x: unknown): string {
  try {
    return JSON.stringify(x);
  } catch {
    return String(x);
  }
}

declare const Deno: any;

/** stdin/stdout transport used inside the companion. */
function stdioWire(): Wire {
  const enc = new TextEncoder();
  return {
    write(line) {
      Deno.stdout.writeSync(enc.encode(line + "\n"));
    },
    async *lines() {
      const reader = Deno.stdin.readable.pipeThrough(new TextDecoderStream()).getReader();
      let buf = "";
      for (;;) {
        const { value, done } = await reader.read();
        if (done) break;
        buf += value;
        let i: number;
        while ((i = buf.indexOf("\n")) >= 0) {
          yield buf.slice(0, i);
          buf = buf.slice(i + 1);
        }
      }
    },
  };
}

/** stdout carries the protocol, so console output goes to stderr (the companion log). */
function quietConsole() {
  const err = (...a: unknown[]) => {
    try {
      Deno.stderr.writeSync(new TextEncoder().encode(fmt(a) + "\n"));
    } catch {
      /* stderr closed */
    }
  };
  for (const k of ["log", "info", "debug", "warn", "error", "trace", "dir", "table"] as const) {
    (console as any)[k] = err;
  }
}

/**
 * Declares the plugin. Inside the companion it starts serving right away.
 * In tests (see @phonedeck/sdk/testing) it only returns the definition.
 */
export function definePlugin(def: PluginDefinition): PluginDefinition {
  if (!(globalThis as any).__phonedeckTest && typeof Deno !== "undefined") {
    quietConsole();
    queueMicrotask(() => {
      void new Runtime(def, stdioWire()).serve().then(() => Deno.exit(0));
    });
  }
  return def;
}

/** Runs a definition over any transport. Used by the test helpers. */
export function serve(def: PluginDefinition, wire: Wire): Promise<void> {
  return new Runtime(def, wire).serve();
}

/** Small helper for reading typed settings with defaults. */
export function setting<T>(s: Settings, name: string, fallback: T): T {
  const v = s[name];
  return (v === undefined || v === null || v === "" ? fallback : v) as T;
}

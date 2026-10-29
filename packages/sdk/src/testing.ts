// Test helpers: run a plugin definition in memory and inspect what it sends.
// Import this module before your plugin module, so the plugin doesn't start on stdio.

(globalThis as any).__keysideTest = true;

import { serve, type PluginDefinition, type Settings, type Wire } from "./index.ts";

export interface Sent {
  method: string;
  params: any;
}

export interface Harness {
  /** Everything the plugin sent, in order. */
  sent: Sent[];
  start(): Promise<void>;
  sync(instances: Array<{ id: string; action: string; settings?: Settings }>): Promise<void>;
  press(instance: string, action: string, settings?: Settings): Promise<void>;
  release(instance: string, action: string, settings?: Settings): Promise<void>;
  slide(instance: string, action: string, value: number, settings?: Settings): Promise<void>;
  /** Several steps on one key, the way the host sends a multi-action key. */
  run(steps: Array<{ instance: string; action: string; settings?: Settings }>, phase?: "press" | "release"): Promise<void>;
  /** Sends any host notification, such as "ipc.message". */
  emit(method: string, params: unknown): Promise<void>;
  /** Latest connection status the plugin reported. */
  status(): { state: string; detail?: string; pills?: unknown[] } | undefined;
  options(action: string, field: string, settings?: Settings): Promise<Array<{ value: string; label: string }>>;
  setConnection(values: Settings): Promise<void>;
  /** Latest view sent for a key, merged across updates. */
  view(instance: string): Record<string, unknown> | null | undefined;
  variable(name: string): unknown;
  /** Waits for batched key and variable updates to go out. */
  settle(): Promise<void>;
  stop(): Promise<void>;
}

export function createTestHost(def: PluginDefinition, opts: { connection?: Settings; pluginId?: string; prefix?: string; respond?: (s: Sent) => unknown } = {}): Harness {
  const inbox: string[] = [];
  let wake: (() => void) | null = null;
  let closed = false;
  const sent: Sent[] = [];
  const replies = new Map<number, { ok: (v: any) => void; fail: (e: Error) => void }>();
  let nextId = 1;

  const wire: Wire = {
    write(line) {
      const m = JSON.parse(line);
      if (m.method) {
        sent.push({ method: m.method, params: m.params });
        if (m.id !== undefined) inbox.push(JSON.stringify({ jsonrpc: "2.0", id: m.id, result: opts.respond?.({ method: m.method, params: m.params }) ?? null }));
        wake?.();
      } else if (m.id !== undefined) {
        const r = replies.get(m.id);
        replies.delete(m.id);
        if (m.error) r?.fail(new Error(m.error.message));
        else r?.ok(m.result);
      }
    },
    async *lines() {
      for (;;) {
        while (inbox.length) yield inbox.shift()!;
        if (closed) return;
        await new Promise<void>((r) => (wake = r));
        wake = null;
      }
    },
  };
  const done = serve(def, wire);

  const call = (method: string, params: unknown) => {
    const id = nextId++;
    inbox.push(JSON.stringify({ jsonrpc: "2.0", id, method, params }));
    wake?.();
    return new Promise<any>((ok, fail) => replies.set(id, { ok, fail }));
  };
  const notify = async (method: string, params: unknown) => {
    inbox.push(JSON.stringify({ jsonrpc: "2.0", method, params }));
    wake?.();
    await new Promise((r) => setTimeout(r, 0));
  };
  const run = (instance: string, action: string, phase: string, settings: Settings = {}) =>
    call("run", { steps: [{ instance, action, settings }], phase, device: "test" });

  return {
    sent,
    start: () => call("initialize", { hostVersion: "1.0.0", pluginId: opts.pluginId ?? "dev.example.test", prefix: opts.prefix ?? "test", connection: opts.connection ?? {}, locale: "en", dataDir: "" }),
    sync: (instances) => notify("instances.sync", { instances: instances.map((i) => ({ settings: {}, ...i })) }),
    press: (i, a, s) => run(i, a, "press", s),
    release: (i, a, s) => run(i, a, "release", s),
    slide: (i, a, value, s = {}) => notify("slide", { instance: i, action: a, settings: s, value }),
    emit: (method, params) => notify(method, params),
    run: (steps, phase = "press") => call("run", { steps: steps.map((s) => ({ settings: {}, ...s })), phase, device: "test" }),
    status() {
      let last: any;
      for (const s of sent) if (s.method === "status.set") last = { ...(last ?? {}), ...s.params };
      return last;
    },
    options: (action, field, settings = {}) => call("options", { action, field, settings }),
    setConnection: (values) => notify("connection.changed", { connection: values }),
    view(instance) {
      let v: Record<string, unknown> | null | undefined;
      for (const s of sent) {
        if (s.method !== "key.set" || s.params.instance !== instance) continue;
        v = s.params.view === null ? null : { ...(v ?? {}), ...s.params.view };
      }
      return v;
    },
    variable(name) {
      let v: unknown;
      for (const s of sent) if (s.method === "var.set" && s.params.name === name) v = s.params.value;
      return v;
    },
    settle: () => new Promise((r) => setTimeout(r, 80)),
    async stop() {
      closed = true;
      wake?.();
      await done;
    },
  };
}

// Minimal Qt WebChannel client, the protocol Meld Studio speaks. Only what Keyside needs:
// read properties, call methods and listen to signals of the objects the app shares.

const Type = {
  Signal: 1,
  PropertyUpdate: 2,
  Init: 3,
  Idle: 4,
  InvokeMethod: 6,
  ConnectToSignal: 7,
  Response: 10,
} as const;

export interface Shared {
  methods: Map<string, number>;
  signals: Map<number, string>;
  signalIds: Map<string, number>;
  propertyNames: Map<number, string>;
  props: Record<string, unknown>;
}

export class WebChannel {
  readonly objects = new Map<string, Shared>();
  #send: (text: string) => void;
  #nextId = 1;
  #waiting = new Map<number, { ok: (v: unknown) => void; fail: (e: Error) => void; timer: ReturnType<typeof setTimeout> }>();
  /** Called after properties changed, with the object name. */
  onChange: (object: string) => void = () => {};
  onSignal: (object: string, signal: string, args: unknown[]) => void = () => {};

  constructor(send: (text: string) => void) {
    this.#send = send;
  }

  #call(msg: Record<string, unknown>, ms = 8000): Promise<unknown> {
    const id = this.#nextId++;
    return new Promise((ok, fail) => {
      const timer = setTimeout(() => {
        this.#waiting.delete(id);
        fail(new Error("no answer in time"));
      }, ms);
      this.#waiting.set(id, { ok, fail, timer });
      try {
        this.#send(JSON.stringify({ ...msg, id }));
      } catch (e) {
        clearTimeout(timer);
        this.#waiting.delete(id);
        fail(e instanceof Error ? e : new Error(String(e)));
      }
    });
  }

  /** Asks for the shared objects. Resolves once they're known. */
  async init() {
    const data = (await this.#call({ type: Type.Init })) as Record<string, any>;
    this.objects.clear();
    for (const [name, o] of Object.entries(data ?? {})) {
      const shared: Shared = { methods: new Map(), signals: new Map(), signalIds: new Map(), propertyNames: new Map(), props: Object.create(null) };
      for (const [method, idx] of o?.methods ?? []) if (!shared.methods.has(method)) shared.methods.set(String(method), Number(idx));
      for (const [signal, idx] of o?.signals ?? []) {
        shared.signals.set(Number(idx), String(signal));
        shared.signalIds.set(String(signal), Number(idx));
      }
      for (const [idx, prop, , value] of o?.properties ?? []) {
        shared.propertyNames.set(Number(idx), String(prop));
        shared.props[String(prop)] = value;
      }
      this.objects.set(name, shared);
    }
    // Tells the app we're ready for property updates.
    this.#send(JSON.stringify({ type: Type.Idle }));
  }

  invoke(object: string, method: string, args: unknown[] = []): Promise<unknown> {
    const o = this.objects.get(object);
    const idx = o?.methods.get(method);
    if (idx === undefined) return Promise.reject(new Error(`This version of the app can't ${method}`));
    return this.#call({ type: Type.InvokeMethod, object, method: idx, args });
  }

  listen(object: string, signal: string) {
    const idx = this.objects.get(object)?.signalIds.get(signal);
    if (idx !== undefined) this.#send(JSON.stringify({ type: Type.ConnectToSignal, object, signal: idx }));
  }

  /** Feed every text message from the socket here. */
  message(raw: string) {
    const m = JSON.parse(raw);
    switch (m?.type) {
      case Type.Response: {
        const w = this.#waiting.get(Number(m.id));
        if (!w) return;
        clearTimeout(w.timer);
        this.#waiting.delete(Number(m.id));
        w.ok(m.data);
        return;
      }
      case Type.Signal: {
        const o = this.objects.get(String(m.object));
        const name = o?.signals.get(Number(m.signal));
        if (name) this.onSignal(String(m.object), name, Array.isArray(m.args) ? m.args : []);
        return;
      }
      case Type.PropertyUpdate: {
        for (const u of Array.isArray(m.data) ? m.data : []) {
          const o = this.objects.get(String(u?.object));
          if (!o) continue;
          for (const [idx, value] of Object.entries(u.properties ?? {})) {
            const name = o.propertyNames.get(Number(idx));
            if (name) o.props[name] = value;
          }
          this.onChange(String(u.object));
        }
        this.#send(JSON.stringify({ type: Type.Idle }));
        return;
      }
    }
  }

  failAll(e: Error) {
    for (const [id, w] of this.#waiting) {
      clearTimeout(w.timer);
      this.#waiting.delete(id);
      w.fail(e);
    }
  }
}

// VTube Studio public API. The first connection asks for access inside VTube Studio;
// the token it grants is kept in the keychain.

import { definePlugin, setting, type Host, type Key } from "@phonedeck/sdk";
import { Reconnecting, Waiters } from "../../_lib/socket.ts";
import { errorText, randomId, want } from "../../_lib/util.ts";

const PLUGIN = { pluginName: "PhoneDeck", pluginDeveloper: "PhoneDeck" };
const EXPRESSIONS_EVERY = 4000;
const DENIED = 50;

let host: Host;
let sock: Reconnecting;
const waiting = new Waiters<any>();
let ready = false;
let denied = false;
let model = { id: "", name: "" };
let hotkeys: Array<{ id: string; name: string }> = [];
let models: Array<{ id: string; name: string }> = [];
const expressions = new Map<string, { name: string; active: boolean }>();
let timer: ReturnType<typeof setInterval> | null = null;

class VtsError extends Error {
  readonly id: number;
  constructor(message: string, id: number) {
    super(message);
    this.id = id;
  }
}

function call(messageType: string, data: Record<string, unknown> = {}, ms = 10_000): Promise<any> {
  const requestID = randomId();
  const p = waiting.wait(requestID, ms);
  sock.send(JSON.stringify({ apiName: "VTubeStudioPublicAPI", apiVersion: "1.0", requestID, messageType, data }));
  return p;
}

function onMessage(raw: string) {
  const m = JSON.parse(raw);
  if (m.messageType === "APIError") waiting.reject(m.requestID, new VtsError(m.data?.message ?? "VTube Studio refused the request", Number(m.data?.errorID)));
  else if (m.requestID) waiting.resolve(m.requestID, m.data ?? {});
  if (m.messageType === "ModelLoadedEvent") void refreshModel();
}

async function authenticate() {
  let token = String(host.connection.token ?? "");
  for (let attempt = 0; attempt < 2; attempt++) {
    if (!token) {
      host.status("connecting", "Click Allow in VTube Studio");
      // The person has to click Allow inside VTube Studio, so give them time.
      token = (await call("AuthenticationTokenRequest", PLUGIN, 120_000)).authenticationToken;
      await host.saveConnection({ token });
    }
    const r = await call("AuthenticationRequest", { ...PLUGIN, authenticationToken: token });
    if (r.authenticated) return;
    token = "";
  }
  throw new Error("VTube Studio didn't accept PhoneDeck");
}

async function refreshModel() {
  if (!ready) return;
  const m = await call("CurrentModelRequest");
  model = { id: m.modelLoaded ? m.modelID : "", name: m.modelLoaded ? m.modelName : "" };
  host.setVar("model", model.name);
  const hk = await call("HotkeysInCurrentModelRequest").catch(() => ({ availableHotkeys: [] }));
  hotkeys = (hk.availableHotkeys ?? []).map((h: any) => ({ id: h.hotkeyID, name: h.name || h.description || h.type }));
  await refreshExpressions();
}

async function refreshExpressions() {
  if (!ready || !model.id) return;
  const r = await call("ExpressionStateRequest", { details: false }).catch(() => null);
  if (!r) return;
  expressions.clear();
  for (const e of r.expressions ?? []) expressions.set(e.file, { name: e.name, active: !!e.active });
  render();
}

function view(k: Key) {
  if (!ready) return { disabled: true };
  const s = k.settings;
  if (k.action === "model") return s.model === model.id ? { tone: "accent" as const, badge: "LOADED", badgeTone: "accent" as const } : null;
  if (k.action === "expression") {
    const e = expressions.get(String(s.expression ?? ""));
    if (!e) return model.id ? { disabled: true, sub: "Not on this model" } : { disabled: true };
    return e.active ? { tone: "accent" as const, badge: "ON", badgeTone: "accent" as const } : null;
  }
  if (k.action === "hotkey" && hotkeys.length && !hotkeys.some((h) => h.id === s.hotkey)) return { disabled: true, sub: "Not on this model" };
  return null;
}

function render() {
  for (const k of host.keys()) {
    const v = view(k);
    if (v) k.set(v);
    else k.clear();
  }
}

function connect() {
  sock?.stop();
  ready = false;
  denied = false;
  sock = new Reconnecting(
    () => {
      const h = String(setting(host.connection, "host", "127.0.0.1")).trim();
      return h && !denied ? `ws://${h.includes(":") && !h.startsWith("[") ? `[${h}]` : h}:${Number(setting(host.connection, "port", 8001))}` : null;
    },
    {
      async open() {
        try {
          await authenticate();
          ready = true;
          sock.healthy();
          await call("EventSubscriptionRequest", { eventName: "ModelLoadedEvent", subscribe: true }).catch(() => {});
          await refreshModel();
          host.status("connected", model.name || undefined);
          render();
        } catch (e) {
          if (e instanceof VtsError && e.id === DENIED) {
            denied = true;
            await host.saveConnection({ token: "" }).catch(() => {});
            host.status("error", "Access was denied in VTube Studio. Save the plugin settings to ask again.");
            sock.stop();
            return;
          }
          throw e;
        }
      },
      message: onMessage,
      close(reason) {
        ready = false;
        waiting.failAll(new Error("VTube Studio disconnected"));
        if (!denied) host.status("offline", `Can't reach VTube Studio (${reason}). Is its API turned on?`);
        render();
      },
    },
  );
  sock.start();
}

function connected() {
  if (!ready) throw new Error("VTube Studio isn't connected");
}

async function setExpression(file: string, active: boolean) {
  connected();
  await call("ExpressionActivationRequest", { expressionFile: file, active });
  const e = expressions.get(file);
  if (e) e.active = active;
  render();
}

export default definePlugin({
  start(h) {
    host = h;
    connect();
    timer = setInterval(() => {
      if (host.keys("expression").length) void refreshExpressions();
    }, EXPRESSIONS_EVERY);
  },
  connectionChanged: connect,
  stop() {
    if (timer) clearInterval(timer);
    sock?.stop();
  },
  actions: {
    hotkey: {
      async press({ settings }) {
        connected();
        await call("HotkeyTriggerRequest", { hotkeyID: settings.hotkey });
      },
      async options() {
        connected();
        await refreshModel();
        return hotkeys.map((h) => ({ value: h.id, label: h.name }));
      },
      appear: render,
    },
    expression: {
      press({ settings }) {
        const file = String(settings.expression ?? "");
        const mode = String(settings.mode ?? "toggle");
        return setExpression(file, mode === "hold" ? true : want(mode, expressions.get(file)?.active ?? false));
      },
      release({ settings }) {
        if (settings.mode === "hold") return setExpression(String(settings.expression ?? ""), false);
      },
      async options() {
        connected();
        await refreshExpressions();
        return [...expressions].map(([file, e]) => ({ value: file, label: e.name }));
      },
      appear: render,
    },
    model: {
      async press({ settings }) {
        connected();
        await call("ModelLoadRequest", { modelID: settings.model }).catch((e) => {
          throw new Error(`Couldn't load the model: ${errorText(e)}`);
        });
      },
      async options() {
        connected();
        const r = await call("AvailableModelsRequest");
        models = (r.availableModels ?? []).map((m: any) => ({ id: m.modelID, name: m.modelName }));
        return models.map((m) => ({ value: m.id, label: m.name }));
      },
      appear: render,
    },
  },
});

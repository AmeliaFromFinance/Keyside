// VTube Studio public API. The first connection asks for access inside VTube Studio;
// the token it grants is kept in the keychain.

import { definePlugin, setting, type Host, type Key, type KeyView } from "@keyside/sdk";
import { Reconnecting, Waiters } from "../../_lib/socket.ts";
import { clamp01, errorText, randomId, want } from "../../_lib/util.ts";

const PLUGIN = { pluginName: "Keyside", pluginDeveloper: "Keyside" };
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
/** Item file name -> instance ids in the scene. */
const items = new Map<string, string[]>();
let itemFiles: string[] = [];
/** Tracking parameters and their ranges. */
const params = new Map<string, { min: number; max: number; dflt: number }>();
/** Parameters a slider has taken over, with the value to keep sending. */
const held = new Map<string, number>();
let holdTimer: ReturnType<typeof setInterval> | null = null;
// VTube Studio hands a parameter back to tracking a second after the last value.
const HOLD_EVERY = 500;

class VtsError extends Error {
  readonly id: number;
  constructor(message: string, id: number) {
    super(message);
    this.id = id;
  }
}

function call(messageType: string, data: Record<string, unknown> = {}, ms = 10_000): Promise<any> {
  const requestID = randomId();
  return waiting.request(requestID, () => sock.send(JSON.stringify({ apiName: "VTubeStudioPublicAPI", apiVersion: "1.0", requestID, messageType, data })), ms);
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
  throw new Error("VTube Studio didn't accept Keyside");
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

async function refreshItems() {
  if (!ready) return;
  const r = await call("ItemListRequest", { includeAvailableSpots: false, includeItemInstancesInScene: true, includeAvailableItemFiles: true }).catch(() => null);
  if (!r) return;
  items.clear();
  for (const i of r.itemInstancesInScene ?? []) items.set(i.fileName, [...(items.get(i.fileName) ?? []), i.instanceID]);
  itemFiles = (r.availableItemFiles ?? []).map((f: any) => String(f.fileName));
  render();
}

async function refreshParams() {
  const r = await call("InputParameterListRequest");
  params.clear();
  for (const p of [...(r.defaultParameters ?? []), ...(r.customParameters ?? [])]) params.set(p.name, { min: Number(p.min), max: Number(p.max), dflt: Number(p.defaultValue ?? 0) });
}

/** Keeps sending held values, so tracking doesn't take the parameter back. */
async function injectHeld() {
  if (!ready || !held.size) return;
  const parameterValues = [...held].map(([id, value]) => ({ id, value }));
  await call("InjectParameterDataRequest", { faceFound: false, mode: "set", parameterValues }).catch(() => {});
}

async function refreshExpressions() {
  if (!ready || !model.id) return;
  const r = await call("ExpressionStateRequest", { details: false }).catch(() => null);
  if (!r) return;
  expressions.clear();
  for (const e of r.expressions ?? []) expressions.set(e.file, { name: e.name, active: !!e.active });
  render();
}

function view(k: Key): KeyView | null {
  if (!ready) return { disabled: true };
  const s = k.settings;
  if (k.action === "item") return items.get(String(s.item ?? ""))?.length ? { tone: "accent", badge: "SHOWN", badgeTone: "accent" } : null;
  if (k.action === "parameter") {
    const id = String(s.parameter ?? "");
    const p = params.get(id);
    if (params.size && !p) return { disabled: true, sub: "Unknown parameter" };
    const v = held.get(id);
    if (v === undefined || !p) return { meta: "Tracking" };
    return { value: clamp01((v - p.min) / (p.max - p.min || 1)), meta: v.toFixed(2), tone: "accent" };
  }
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
          await Promise.all([refreshItems(), refreshParams().catch(() => {})]);
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
        held.clear();
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
      if (host.keys("item").length) void refreshItems();
    }, EXPRESSIONS_EVERY);
    holdTimer = setInterval(() => void injectHeld(), HOLD_EVERY);
  },
  connectionChanged: connect,
  stop() {
    if (timer) clearInterval(timer);
    if (holdTimer) clearInterval(holdTimer);
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
    item: {
      async press({ settings }) {
        connected();
        const file = String(settings.item ?? "");
        if (!file) throw new Error("Pick an item");
        const shown = items.get(file) ?? [];
        if (want(settings.mode ?? "toggle", shown.length > 0)) {
          if (shown.length) return;
          await call("ItemLoadRequest", {
            fileName: file,
            positionX: Math.max(-1, Math.min(1, Number(settings.x ?? 0))),
            positionY: Math.max(-1, Math.min(1, Number(settings.y ?? 0))),
            size: Math.max(0, Math.min(1, Number(settings.size ?? 0.32))),
            fadeTime: 0.3,
          });
        } else if (shown.length) {
          await call("ItemUnloadRequest", { instanceIDs: shown, unloadAllInScene: false, unloadAllLoadedByThisPlugin: false, allowUnloadingItemsLoadedByUserOrOtherPlugins: true });
        }
        await refreshItems();
      },
      async options() {
        connected();
        await refreshItems();
        return itemFiles.map((f) => ({ value: f, label: f.replace(/\.[a-z0-9]+$/i, "") }));
      },
      appear: render,
    },
    parameter: {
      // Moving the slider takes the parameter over. Back at its default, tracking gets it back.
      async slide({ settings, value }) {
        connected();
        const id = String(settings.parameter ?? "");
        if (!params.size) await refreshParams();
        const p = params.get(id);
        if (!p) throw new Error("Pick a parameter");
        const v = p.min + clamp01(value) * (p.max - p.min);
        if (Math.abs(v - p.dflt) <= (p.max - p.min) * 0.02) held.delete(id);
        else held.set(id, v);
        await injectHeld();
        render();
      },
      async options() {
        connected();
        await refreshParams();
        return [...params.keys()].sort().map((name) => ({ value: name, label: name }));
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

// Plugin manifest and the JSON-RPC contract between the companion and a plugin process.

import type { FeedItem, KeyView, VarValue } from "./model.ts";

export type FieldSchema =
  | {
      type: "string";
      title?: string;
      description?: string;
      default?: string;
      enum?: string[];
      enumNames?: string[];
      maxLength?: number;
      /** secret: stored in the OS keychain, never sent back to the editor. */
      format?: "secret" | "textarea" | "color" | "url" | "hotkey" | "file";
      /** Options are fetched from the plugin while editing. */
      "x-options"?: "dynamic";
      /** Kept by the plugin itself (such as a sign-in session) and hidden in the editor. */
      "x-hidden"?: boolean;
      /** {variables} in the value are filled in when the key runs. */
      "x-template"?: boolean;
      /** Shown only while other fields have one of these values. */
      "x-showWhen"?: Record<string, string[]>;
    }
  | { type: "number" | "integer"; title?: string; description?: string; default?: number; minimum?: number; maximum?: number }
  | { type: "boolean"; title?: string; description?: string; default?: boolean }
  | { type: "object"; title?: string; description?: string; additionalProperties: { type: "string" }; "x-template"?: boolean };

export interface SettingsSchema {
  type: "object";
  properties: Record<string, FieldSchema>;
  required?: string[];
}

export interface ActionDef {
  id: string;
  name: string;
  description?: string;
  icon?: string;
  /** Editor hint for which key kind fits best. */
  kind?: "button" | "toggle" | "slider";
  settings?: SettingsSchema;
}

export interface PluginPermissions {
  /**
   * Hosts the plugin may reach, as "host", "host:port" or a template using connection
   * settings such as "{host}:{port}". No wildcards.
   */
  net?: string[];
  /** Send UDP datagrams through the host. Still limited to the hosts in net. */
  udp?: boolean;
  /** Local app bridges the companion opens on the plugin's behalf. */
  ipc?: Array<"discord">;
  /** Exact commands the plugin may run, such as ["equibop", "--toggle-mic"]. Bundled plugins only. */
  run?: string[][];
}

export interface PluginManifest {
  id: string;
  name: string;
  version: string;
  author: string;
  /** Short name for variables and feeds: "obs" gives {obs.scene}. */
  prefix: string;
  description?: string;
  homepage?: string;
  license?: string;
  /** Supported host API, e.g. "^1.0". */
  host: string;
  /** Bundled JavaScript entry, relative to the package root. */
  entry: string;
  icon?: string;
  permissions: PluginPermissions;
  /** Settings for the plugin as a whole (address, password, OAuth client id). */
  connection?: SettingsSchema;
  /** Up to 8 setup steps shown above the settings. `[text](https://…)` makes a link, `` `text` `` a copy button. */
  setup?: string[];
  actions: ActionDef[];
  variables?: Array<{ name: string; title?: string; type?: "string" | "number" | "boolean" }>;
  feeds?: Array<{ name: string; title: string }>;
}

/** A placed key using one of the plugin's actions. */
export interface Instance {
  id: string;
  action: string;
  settings: Record<string, unknown>;
  /** Label and icon chosen in the editor. */
  label?: string;
  icon?: string;
}

export interface Option {
  value: string;
  label: string;
}

/** Host -> plugin requests and notifications. */
export interface HostToPlugin {
  initialize: { params: { hostVersion: string; pluginId: string; prefix: string; connection: Record<string, unknown>; locale: string; dataDir: string }; result: void };
  "connection.changed": { params: { connection: Record<string, unknown> }; result: void };
  "instances.sync": { params: { instances: Instance[] }; result: void };
  /** Consecutive actions of the same plugin on one key arrive together, so they can be batched. */
  run: { params: { steps: Array<{ instance: string; action: string; settings: Record<string, unknown> }>; phase: "press" | "release"; device: string }; result: void };
  slide: { params: { instance: string; action: string; settings: Record<string, unknown>; value: number }; result: void };
  options: { params: { action: string; field: string; settings: Record<string, unknown> }; result: Option[] };
  "ipc.message": { params: { bridge: "discord"; data: unknown }; result: void };
  "ipc.closed": { params: { bridge: "discord" }; result: void };
  shutdown: { params: Record<string, never>; result: void };
}

/** Plugin -> host calls. Every call is checked against the plugin's own instances and namespace. */
export interface PluginToHost {
  "key.set": { params: { instance: string; view: Partial<Omit<KeyView, "id" | "kind" | "x" | "y" | "w" | "h" | "lines">> | null }; result: void };
  "var.set": { params: { name: string; value: VarValue }; result: void };
  "feed.push": { params: { feed: string; item: Omit<FeedItem, "at"> }; result: void };
  "status.set": { params: { state: "connected" | "connecting" | "offline" | "error"; detail?: string; pills?: Array<{ label: string; tone?: string; since?: number }> }; result: void };
  toast: { params: { text: string; tone?: "default" | "success" | "warning" | "danger" }; result: void };
  log: { params: { level: "debug" | "info" | "warn" | "error"; message: string }; result: void };
  /** Save values (such as OAuth tokens) into the plugin's connection settings. Secret fields go to the keychain. */
  "connection.update": { params: { values: Record<string, unknown> }; result: void };
  "ipc.open": { params: { bridge: "discord" }; result: void };
  "ipc.send": { params: { bridge: "discord"; data: unknown }; result: void };
  /** Opens a URL in the default browser on the PC. https only. */
  "browser.open": { params: { url: string }; result: void };
  /** Browser sign-in. The host sets redirect_uri and state and returns the code. */
  "oauth.authorize": { params: { url: string }; result: { code: string; redirectUri: string } };
  "udp.send": { params: { host: string; port: number; data: string }; result: void };
  /** Runs one of the commands listed under permissions.run. `code` is null if it keeps running. */
  run: { params: { argv: string[] }; result: { code: number | null } };
}

// A ready-made "Stream dashboard" page: live figures for whatever apps are connected.
// Kept free of Svelte state so it can be tested on its own.

import type { Key, Page } from "@keyside/protocol";

export interface DashPlugin {
  id: string;
  prefix: string;
  feeds?: Array<{ name: string }>;
}

/** Marks tile names for translation. They are translated when the page is made. */
const mark = (text: string) => text;

const OBS = "dev.keyside.obs";
const TWITCH = "dev.keyside.twitch";
const YOUTUBE = "dev.keyside.youtube";
const STREAMLABS = "dev.keyside.streamlabs";
const COLS = 3;

type Tile = Omit<Key, "id" | "x" | "y">;

const figure = (label: string, icon: string, text: string): Tile => ({ kind: "widget", label, icon, actions: [], widget: { text } });
const obsStat = (label: string, icon: string, show: string): Tile => ({ kind: "button", label, icon, actions: [{ type: `${OBS}/stats`, settings: { show } }] });

/** Tiles for one connected plugin, with untranslated names. */
function tiles(p: DashPlugin): Tile[] {
  const v = (name: string) => `{${p.prefix}.${name}}`;
  switch (p.id) {
    case OBS:
      return [
        obsStat(mark("Live"), "radio", "live"),
        obsStat(mark("Recording"), "circle-dot", "record"),
        obsStat(mark("Bitrate"), "wifi", "bitrate"),
        obsStat(mark("Dropped frames"), "gauge", "dropped"),
        obsStat(mark("CPU"), "cpu", "cpu"),
      ];
    case TWITCH:
      return [figure(mark("Twitch viewers"), "eye", v("viewers")), figure(mark("Followers"), "user-plus", v("followers")), figure(mark("Subscribers"), "star", v("subs"))];
    case YOUTUBE:
      return [figure(mark("YouTube viewers"), "eye", v("viewers")), figure(mark("Likes"), "thumbs-up", v("likes"))];
    case STREAMLABS:
      return [figure(mark("Streaming"), "radio", v("streaming")), figure(mark("Recording"), "circle-dot", v("recording"))];
    default:
      return [];
  }
}

/**
 * Builds the page from connected plugins, or returns null when none of them has anything
 * to show. Plugins with a chat feed add the chat rate and the chat itself.
 */
export function dashboardPage(plugins: DashPlugin[], id: string, t: (text: string) => string): Page | null {
  // OBS first, since live time and stream health matter most.
  const order = [OBS, STREAMLABS, TWITCH, YOUTUBE];
  const sorted = [...plugins].sort((a, b) => (order.indexOf(a.id) + 1 || 99) - (order.indexOf(b.id) + 1 || 99));
  const list = sorted.flatMap(tiles);
  const chat = sorted.find((p) => p.feeds?.some((f) => f.name === "chat"));
  if (!list.length && !chat) return null;
  if (chat) list.push(figure(mark("Chat per minute"), "messages-square", "{core.chat_rate}"));

  const keys: Key[] = list.map((tile, i) => ({ ...tile, label: t(tile.label), id: `d${i}`, x: i % COLS, y: Math.floor(i / COLS) }));
  let rows = Math.ceil(keys.length / COLS);
  if (chat) {
    keys.push({ id: "chat", kind: "widget", label: t("Chat"), icon: "message-square", x: 0, y: rows, w: COLS, h: 2, actions: [], widget: { feed: `${chat.prefix}.chat` } });
    rows += 2;
  }
  return { id, name: t("Dashboard"), cols: COLS, rows: Math.min(12, Math.max(rows, 2)), keys };
}

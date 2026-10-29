// Warudo: sends {"action": name, "data": text} to blueprints with an On WebSocket Action node.

import { definePlugin, setting, type Host } from "@keyside/sdk";
import { Reconnecting } from "../../_lib/socket.ts";

let host: Host;
let sock: Reconnecting;

function connect() {
  sock?.stop();
  sock = new Reconnecting(
    () => {
      const h = String(setting(host.connection, "host", "127.0.0.1")).trim();
      return h ? `ws://${h.includes(":") && !h.startsWith("[") ? `[${h}]` : h}:${Number(setting(host.connection, "port", 19190))}` : null;
    },
    {
      open() {
        sock.healthy();
        host.setVar("connected", true);
        host.status("connected");
        for (const k of host.keys()) k.clear();
      },
      message() {},
      close(reason) {
        host.setVar("connected", false);
        host.status("offline", `Can't reach Warudo (${reason}). Is it running?`);
        for (const k of host.keys()) k.set({ disabled: true });
      },
    },
  );
  sock.start();
}

export default definePlugin({
  start(h) {
    host = h;
    connect();
  },
  connectionChanged: connect,
  stop: () => sock?.stop(),
  actions: {
    action: {
      press({ settings }) {
        if (!sock.ready) throw new Error("Warudo isn't connected");
        sock.send(JSON.stringify({ action: String(settings.action ?? ""), data: String(settings.data ?? "") }));
      },
      appear: (k) => (sock?.ready ? k.clear() : k.set({ disabled: true })),
    },
  },
});

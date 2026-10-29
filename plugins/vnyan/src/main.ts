// VNyan: each key sends a plain-text command that its WebSocket Command nodes listen for.

import { definePlugin, setting, type Host } from "@phonedeck/sdk";
import { Reconnecting } from "../../_lib/socket.ts";

let host: Host;
let sock: Reconnecting;

function connect() {
  sock?.stop();
  sock = new Reconnecting(
    () => {
      const h = String(setting(host.connection, "host", "127.0.0.1")).trim();
      return h ? `ws://${h.includes(":") && !h.startsWith("[") ? `[${h}]` : h}:${Number(setting(host.connection, "port", 8000))}/vnyan` : null;
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
        host.status("offline", `Can't reach VNyan (${reason}). Turn on its WebSocket server.`);
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
    command: {
      press({ settings }) {
        if (!sock.ready) throw new Error("VNyan isn't connected");
        sock.send(String(settings.command ?? ""));
      },
      appear: (k) => (sock?.ready ? k.clear() : k.set({ disabled: true })),
    },
  },
});

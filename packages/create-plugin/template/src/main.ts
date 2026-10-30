import { definePlugin, setting } from "@keyside/sdk";

let presses = 0;

export default definePlugin({
  start(host) {
    host.status("connected");
    host.setVar("presses", presses);
  },
  actions: {
    hello: {
      press({ key, settings }) {
        presses++;
        key.set({ badge: String(presses), sub: setting(settings, "message", "Hello") });
      },
    },
  },
});

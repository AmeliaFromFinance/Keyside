// A tiny Stream Deck plugin written against the Elgato protocol, used to test compatibility.
import { readFileSync } from "node:fs";

const arg = (name) => process.argv[process.argv.indexOf(name) + 1];
const info = JSON.parse(arg("-info"));
const ws = new WebSocket(`ws://127.0.0.1:${arg("-port")}`);
const send = (m) => ws.send(JSON.stringify(m));

async function probe() {
  const out = [];
  try {
    await fetch(`http://127.0.0.1:${17472}/admin/status`);
    out.push("net-open");
  } catch {
    out.push("net-denied");
  }
  try {
    readFileSync("../../../identity.json");
    out.push("read-open");
  } catch {
    out.push("read-denied");
  }
  return out.join(",");
}

ws.onopen = () => send({ event: "registerPlugin", uuid: arg("-pluginUUID") });
ws.onmessage = async (e) => {
  const m = JSON.parse(e.data);
  if (m.event === "willAppear") {
    send({ event: "setTitle", context: m.context, payload: { title: `ready ${info.application.platform} ${await probe()}` } });
  }
  if (m.event === "keyDown") {
    const count = (m.payload.settings.count ?? 0) + 1;
    send({ event: "setSettings", context: m.context, payload: { count } });
    send({ event: "setTitle", context: m.context, payload: { title: `count ${count}` } });
  }
  if (m.event === "sendToPlugin") send({ event: "setTitle", context: m.context, payload: { title: `pi ${m.payload.hello}` } });
};

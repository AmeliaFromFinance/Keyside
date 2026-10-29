import { createTestHost } from "@keyside/sdk/testing";
import { test } from "node:test";
import assert from "node:assert/strict";
import plugin, { cleanScript, packet } from "../src/main.ts";

const text = (b64: string) => Buffer.from(b64, "base64").subarray(28).toString();

test("builds VBAN text packets", () => {
  const p = packet("Command1", "Strip[0].Mute=1;", 7);
  assert.equal(Buffer.from(p.subarray(0, 4)).toString(), "VBAN");
  assert.equal(p[4] & 0xe0, 0x40);
  assert.equal(Buffer.from(p.subarray(8, 16)).toString(), "Command1");
  assert.equal(new DataView(p.buffer).getUint32(24, true), 7);
  assert.equal(Buffer.from(p.subarray(28)).toString(), "Strip[0].Mute=1;");
});

test("toggles follow what was sent and go through the host", async () => {
  const h = createTestHost(plugin, { connection: { host: "127.0.0.1", port: 6980, stream: "Command1" } });
  await h.start();
  await h.sync([{ id: "m", action: "mute", settings: { target: "Strip", index: 2 } }]);
  await h.press("m", "mute", { target: "Strip", index: 2, mode: "toggle" });
  await h.press("m", "mute", { target: "Strip", index: 2, mode: "toggle" });
  const sent = h.sent.filter((s) => s.method === "udp.send");
  assert.deepEqual(sent.map((s) => text(s.params.data)), ["Strip[1].Mute=1;", "Strip[1].Mute=0;"]);
  assert.equal(sent[0].params.port, 6980);
  await h.slide("m", "gain", 1, { target: "Bus", index: 1 });
  await h.settle();
  assert.equal(text(h.sent.filter((s) => s.method === "udp.send").at(-1)!.params.data), "Bus[0].Gain=12;");
  await h.stop();
});

test("scripts stay plain text", () => {
  assert.equal(cleanScript("Strip[0].Mute=1\nBus[0].Gain=-3"), "Strip[0].Mute=1;Bus[0].Gain=-3");
  assert.throws(() => cleanScript("x\u0000y"));
  assert.throws(() => cleanScript(""));
});

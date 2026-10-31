import { test } from "node:test";
import assert from "node:assert/strict";
import { defaults, overlayLink } from "../src/editor/overlay.ts";

const origin = "http://127.0.0.1:7442";

test("default looks give the plain links", () => {
  assert.equal(overlayLink(origin, "timer", "break", defaults("timer")), `${origin}/overlay/timer/break`);
  assert.equal(overlayLink(origin, "text", "{twitch.viewers} viewers", defaults("text")), `${origin}/overlay/text?t=%7Btwitch.viewers%7D+viewers`);
});

test("only changed options go into the link", () => {
  const look = { ...defaults("timer"), size: 96, color: "#FFCC00", align: "left" as const, shadowAlpha: 100, glow: true, format: "mm:ss", hide: true };
  assert.equal(
    overlayLink(origin, "timer", "break", look),
    `${origin}/overlay/timer/break?size=96&color=ffcc00&align=left&shadow=000000&shadow-x=0&shadow-y=2&shadow-blur=6&glow=ffcc00&glow-size=10&format=mm%3Ass&hide=1`,
  );
  const text = { ...defaults("text"), font: "custom", custom: "Bebas Neue", bgAlpha: 50, shadow: false };
  assert.equal(overlayLink(origin, "text", "Hi", text), `${origin}/overlay/text?t=Hi&font=Bebas+Neue&bg=00000080&shadow=0`);
});

test("leaves out an installed font name the companion would refuse", () => {
  const look = { ...defaults("text"), font: "custom", custom: 'x";}' };
  assert.equal(overlayLink(origin, "text", "Hi", look), `${origin}/overlay/text?t=Hi`);
});

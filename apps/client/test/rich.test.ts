import { test } from "node:test";
import assert from "node:assert/strict";
import { parts, plain } from "../src/lib/rich.ts";

test("finds links and things to copy", () => {
  assert.deepEqual(parts("Open [twitch.tv/activate](https://www.twitch.tv/activate?x=1) and enter `AB-12`."), [
    { text: "Open " },
    { text: "twitch.tv/activate", href: "https://www.twitch.tv/activate?x=1" },
    { text: " and enter " },
    { code: "AB-12" },
    { text: "." },
  ]);
  assert.equal(plain("Add `http://127.0.0.1` in the [portal](https://discord.com)"), "Add http://127.0.0.1 in the portal");
});

test("only https becomes a link", () => {
  assert.deepEqual(parts("[x](javascript:alert(1)) [y](http://a.b)"), [{ text: "[x](javascript:alert(1)) [y](http://a.b)" }]);
});

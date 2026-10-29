// Installs a Stream Deck plugin into a real companion and drives it like the Elgato app would.
// Needs Deno on PATH (or KEYSIDE_DENO) and a built companion.

import { test, before, after } from "node:test";
import assert from "node:assert/strict";
import { spawn, type ChildProcess } from "node:child_process";
import { mkdtempSync, readFileSync, existsSync, writeFileSync, mkdirSync, readdirSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, relative } from "node:path";
import { zip } from "./fixtures/zip.ts";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../../../", import.meta.url));
const bin = process.env.KEYSIDE_BIN ?? join(root, "target/debug/keyside");
const ADMIN = 17472;
const hasDeno = !!process.env.KEYSIDE_DENO || (process.env.PATH ?? "").split(process.platform === "win32" ? ";" : ":").some((d) => existsSync(join(d, process.platform === "win32" ? "deno.exe" : "deno")));
const origin = `http://127.0.0.1:${ADMIN}`;
let child: ChildProcess;
let dir: string;
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

before(() => {
  dir = mkdtempSync(join(tmpdir(), "keyside-sd-"));
  mkdirSync(join(dir, "data"));
  writeFileSync(join(dir, "data", "settings.json"), JSON.stringify({ adminPort: ADMIN, lanPort: ADMIN + 1, mdns: false, followFocus: false, lanEnabled: false }));
  if (!existsSync(bin) || !hasDeno) return;
  child = spawn(bin, ["--data-dir", join(dir, "data")], { env: { ...process.env, KEYSIDE_NO_KEYCHAIN: "1", KEYSIDE_LOG: "warn" }, stdio: ["ignore", "inherit", "inherit"] });
});

after(() => child?.kill());

function pluginZip() {
  const base = fileURLToPath(new URL("./fixtures/sdplugin/", import.meta.url));
  const files: Record<string, Uint8Array> = {};
  const walk = (d: string) => {
    for (const n of readdirSync(d)) {
      const p = join(d, n);
      if (statSync(p).isDirectory()) walk(p);
      else files[relative(base, p)] = readFileSync(p);
    }
  };
  walk(base);
  return zip(files);
}

async function session() {
  for (let i = 0; i < 50 && !existsSync(join(dir, "data", "identity.json")); i++) await sleep(100);
  let login: Response | null = null;
  for (let i = 0; i < 50; i++) {
    try {
      const token = JSON.parse(readFileSync(join(dir, "data", "identity.json"), "utf8")).admin_token;
      login = await fetch(`${origin}/admin/login`, { method: "POST", headers: { "content-type": "application/json", origin }, body: JSON.stringify({ token }) });
      if (login.ok) break;
    } catch {
      /* not up yet */
    }
    await sleep(200);
  }
  const cookie = (login!.headers.get("set-cookie") ?? "").split(";")[0];
  const ws = new WebSocket(`ws://127.0.0.1:${ADMIN}/admin/ws`, { headers: { cookie, origin } } as never);
  const msgs: any[] = [];
  let id = 0;
  ws.onmessage = (e) => msgs.push(JSON.parse(String(e.data)));
  await new Promise((r) => (ws.onopen = r));
  const req = async (op: string, args?: unknown) => {
    const n = ++id;
    ws.send(JSON.stringify({ t: "req", id: n, op, args }));
    for (let i = 0; i < 100; i++) {
      const r = msgs.find((m) => m.t === "result" && m.id === n);
      if (r) {
        if (!r.ok) throw new Error(r.error.message);
        return r.data;
      }
      await sleep(50);
    }
    throw new Error(`${op} timed out`);
  };
  return { ws, msgs, req, cookie };
}

async function until<T>(fn: () => T | undefined | null | false, what: string, ms = 15000): Promise<T> {
  const end = Date.now() + ms;
  while (Date.now() < end) {
    const v = fn();
    if (v) return v;
    await sleep(100);
  }
  throw new Error(`timed out waiting for ${what}`);
}

test("runs an unmodified Stream Deck plugin in the sandbox", { skip: (!existsSync(bin) && "companion not built") || (!hasDeno && "Deno not installed") }, async () => {
  const s = await session();
  const up = await fetch(`${origin}/admin/upload/streamdeck`, { method: "POST", headers: { cookie: s.cookie, origin, "x-keyside": "1" }, body: Buffer.from(pluginZip()) });
  assert.equal(up.status, 200, await up.clone().text());
  const staged = await up.json();
  assert.equal(staged.uuid, "com.keyside.test");
  assert.equal(staged.runtime, "node");
  await s.req("streamdeck.install", { token: staged.token });

  const layout = await s.req("layout.get");
  const profile = layout.profiles[0];
  profile.pages.push({ id: "sd", name: "SD", cols: 2, rows: 1, keys: [{ id: "c", x: 0, y: 0, kind: "button", label: "Count", actions: [{ type: "sd.com.keyside.test/com.keyside.test.count", settings: {} }] }] });
  await s.req("layout.save", { layout, baseRev: layout.rev });
  s.ws.send(JSON.stringify({ t: "nav", to: "page", page: "sd" }));

  const keyLabel = () => {
    for (let i = s.msgs.length - 1; i >= 0; i--) {
      const m = s.msgs[i];
      const keys = m.t === "page" && m.page.id === "sd" ? m.page.keys : m.t === "keys" && m.page === "sd" ? m.keys : null;
      if (keys) return keys.find((k: any) => k.id === "c");
    }
  };
  const ready = await until(() => keyLabel()?.label?.startsWith("ready") && keyLabel(), "the plugin to set its title");
  assert.match(ready.label, /net-denied/, "the plugin can't reach the editor port");
  assert.match(ready.label, /read-denied/, "the plugin can't read the companion's files");
  assert.match(String(ready.image), /^data:image\/svg\+xml;base64,/, "state image comes from the plugin folder");

  s.ws.send(JSON.stringify({ t: "input", key: "c", phase: "down" }));
  s.ws.send(JSON.stringify({ t: "input", key: "c", phase: "up" }));
  await until(() => keyLabel()?.label === "count 1", "the press to count");
  const saved = await until(() => {
    const l = JSON.parse(readFileSync(join(dir, "data", "layout.json"), "utf8"));
    return l.profiles[0].pages.find((p: any) => p.id === "sd")?.keys[0].actions[0].settings.count === 1 && l;
  }, "settings to be saved");
  assert.equal(saved.rev, layout.rev + 1, "plugin settings don't bump the layout revision");

  const pi = await s.req("streamdeck.inspector", { instance: `${profile.id}/sd/c/0` });
  const url = new URL(pi.url);
  assert.equal(url.hostname, "localhost");
  const page = await fetch(pi.url);
  assert.equal(page.status, 200);
  assert.match(page.headers.get("content-security-policy") ?? "", new RegExp(`frame-ancestors http://127.0.0.1:${ADMIN}`));
  assert.match(await page.text(), /__keyside\/boot\.js/);

  const params = JSON.parse(decodeURIComponent(url.hash.slice(1)));
  const foreign = new WebSocket(`ws://127.0.0.1:${params.port}`, { headers: { origin: "https://evil.example" } } as never);
  await assert.rejects(new Promise((ok, fail) => ((foreign.onopen = ok), (foreign.onerror = () => fail(new Error("refused"))))));
  const panel = new WebSocket(`ws://127.0.0.1:${params.port}`, { headers: { origin: url.origin } } as never);
  await new Promise((r) => (panel.onopen = r));
  panel.send(JSON.stringify({ event: "registerPropertyInspector", uuid: params.uuid }));
  panel.send(JSON.stringify({ event: "sendToPlugin", context: "someone/else/key/0", payload: { hello: "world" } }));
  await until(() => keyLabel()?.label === "pi world", "the panel message to reach the plugin");
  panel.close();
  s.ws.close();
});

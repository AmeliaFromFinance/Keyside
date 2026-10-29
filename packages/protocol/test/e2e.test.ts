// Runs the real companion and pairs a fake phone with it.
// Needs a built binary: cargo build -p keyside-companion (or set KEYSIDE_BIN).

import { test, before, after } from "node:test";
import assert from "node:assert/strict";
import { spawn, type ChildProcess } from "node:child_process";
import { request } from "node:http";
import { mkdtempSync, readFileSync, existsSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { ClientHandshake, importPsk, type Session } from "../src/channel.ts";
import { fromB64u } from "../src/bytes.ts";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../../../", import.meta.url));
const bin = process.env.KEYSIDE_BIN ?? join(root, "target/debug/keyside");
const ADMIN = 17442;
const LAN = 17443;
let child: ChildProcess;
let dir: string;
let cookie = "";

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

before(async () => {
  assert.ok(existsSync(bin), `companion binary missing at ${bin}`);
  dir = mkdtempSync(join(tmpdir(), "keyside-e2e-"));
  writeFileSync(join(dir, "settings.json"), JSON.stringify({ adminPort: ADMIN, lanPort: LAN, mdns: false, followFocus: false }));
  child = spawn(bin, ["--data-dir", dir], { env: { ...process.env, KEYSIDE_NO_KEYCHAIN: "1", KEYSIDE_LOG: "warn" }, stdio: ["ignore", "inherit", "inherit"] });
  for (let i = 0; i < 100; i++) {
    try {
      const r = await fetch(`http://127.0.0.1:${ADMIN}/admin/status`);
      if (r.ok) break;
    } catch {
      /* not up yet */
    }
    await sleep(100);
  }
  const token = JSON.parse(readFileSync(join(dir, "identity.json"), "utf8")).admin_token;
  const r = await fetch(`http://127.0.0.1:${ADMIN}/admin/login`, {
    method: "POST",
    headers: { "content-type": "application/json", origin: `http://127.0.0.1:${ADMIN}` },
    body: JSON.stringify({ token }),
  });
  assert.equal(r.status, 200);
  cookie = (r.headers.get("set-cookie") ?? "").split(";")[0];
});

after(() => {
  child?.kill();
});

/** Local editor socket with request helper. */
async function admin() {
  const ws = new WebSocket(`ws://127.0.0.1:${ADMIN}/admin/ws`, { headers: { cookie, origin: `http://127.0.0.1:${ADMIN}` } } as never);
  const queue: any[] = [];
  const waiters: Array<(m: any) => void> = [];
  ws.onmessage = (e) => {
    const m = JSON.parse(String(e.data));
    const w = waiters.shift();
    if (w) w(m);
    else queue.push(m);
  };
  await new Promise((res, rej) => ((ws.onopen = res), (ws.onerror = rej)));
  const next = () => new Promise<any>((res) => (queue.length ? res(queue.shift()) : waiters.push(res)));
  let id = 0;
  const req = async (op: string, args?: unknown) => {
    const myId = ++id;
    ws.send(JSON.stringify({ t: "req", id: myId, op, args }));
    for (;;) {
      const m = await next();
      if (m.t === "result" && m.id === myId) return m;
    }
  };
  return { ws, next, req };
}

/** Fake phone over the encrypted channel. */
async function device(hello: unknown, finish: (ch: any) => Promise<{ confirm: unknown; session: Session; sas: string }>) {
  const ws = new WebSocket(`ws://127.0.0.1:${LAN}/ws`);
  ws.binaryType = "arraybuffer";
  const inbox: any[] = [];
  const waiters: Array<(m: any) => void> = [];
  let session: Session | null = null;
  ws.onmessage = async (e) => {
    const m = typeof e.data === "string" ? JSON.parse(e.data) : await session!.open(new Uint8Array(e.data as ArrayBuffer));
    const w = waiters.shift();
    if (w) w(m);
    else inbox.push(m);
  };
  await new Promise((res, rej) => ((ws.onopen = res), (ws.onerror = rej)));
  const next = () => new Promise<any>((res) => (inbox.length ? res(inbox.shift()) : waiters.push(res)));
  ws.send(JSON.stringify(hello));
  const ch = await next();
  if (ch.t !== "challenge") return { ws, next, error: ch, session: null as Session | null, sas: "" };
  const r = await finish(ch);
  session = r.session;
  ws.send(JSON.stringify(r.confirm));
  const send = async (m: unknown) => ws.send(await session!.seal(m));
  return { ws, next, send, session, sas: r.sas };
}

async function until(next: () => Promise<any>, pred: (m: any) => boolean) {
  for (;;) {
    const m = await next();
    if (pred(m)) return m;
  }
}

test("pair, approve, use and resume", async () => {
  const a = await admin();
  await until(a.next, (m) => m.t === "page");
  const start = await a.req("pairing.start", { scope: "deck" });
  assert.equal(start.ok, true);
  const frag = new URLSearchParams(new URL(start.data.url).hash.slice(1));
  const cid = frag.get("c")!;
  const pid = frag.get("i")!;
  const code = fromB64u(frag.get("k")!);

  const { hs, hello } = await ClientHandshake.start({ mode: "pair", cid, id: pid, psk: await importPsk(code), device: { name: "Test phone", kind: "phone", app: "web" } });
  const d = await device(hello, (ch) => hs.finish(ch));
  const waiting = await d.next();
  assert.equal(waiting.t, "waiting");
  assert.equal(waiting.sas, d.sas);

  const pending = (await a.req("pending.list")).data;
  assert.equal(pending.length, 1);
  assert.equal(pending[0].sas, d.sas, "both screens show the same code");
  assert.equal((await a.req("pending.answer", { pid, approve: true })).ok, true);

  const paired = await d.next();
  assert.equal(paired.t, "paired");
  const welcome = await d.next();
  assert.equal(welcome.t, "welcome");
  assert.equal(welcome.device.scope, "deck");
  const page = await d.next();
  assert.equal(page.t, "page");
  assert.ok(page.page.keys.length > 0);

  // A deck device can't read or change the layout.
  await d.send!({ t: "req", id: 7, op: "layout.get" });
  const denied = await until(d.next, (m) => m.t === "result" && m.id === 7);
  assert.equal(denied.ok, false);
  assert.equal(denied.error.code, "forbidden");

  await d.send!({ t: "nav", to: "next" });
  const p2 = await until(d.next, (m) => m.t === "page");
  assert.equal(p2.page.index, 1);
  d.ws.close();

  // Resume with the stored secret.
  const secret = fromB64u(paired.secret, 32);
  const r = await ClientHandshake.start({ mode: "resume", cid, id: paired.deviceId, psk: await importPsk(secret) });
  const d2 = await device(r.hello, (ch) => r.hs.finish(ch));
  assert.equal((await d2.next()).t, "welcome");
  d2.ws.close();

  // Wrong secret is refused.
  const bad = await ClientHandshake.start({ mode: "resume", cid, id: paired.deviceId, psk: await importPsk(new Uint8Array(32)) });
  await assert.rejects(device(bad.hello, (ch) => bad.hs.finish(ch)));

  // The pairing code can't be reused.
  const again = await ClientHandshake.start({ mode: "pair", cid, id: pid, psk: await importPsk(code), device: { name: "Copy", kind: "phone", app: "web" } });
  const d3 = await device(again.hello, (ch) => again.hs.finish(ch));
  assert.equal(d3.error?.reason, "pairing-expired");

  // Revoking cuts the device off.
  const devs = (await a.req("devices.list")).data;
  assert.equal(devs.length, 1);
  await a.req("devices.revoke", { id: paired.deviceId });
  const r2 = await ClientHandshake.start({ mode: "resume", cid, id: paired.deviceId, psk: await importPsk(secret) });
  const d4 = await device(r2.hello, (ch) => r2.hs.finish(ch));
  assert.equal(d4.error?.reason, "unknown-device");
  a.ws.close();
});

test("editor endpoints refuse other origins and hosts", async () => {
  const wrongOrigin = await fetch(`http://127.0.0.1:${ADMIN}/admin/login`, {
    method: "POST",
    headers: { "content-type": "application/json", origin: "https://evil.example" },
    body: JSON.stringify({ token: "x" }),
  });
  assert.equal(wrongOrigin.status, 403);
  // fetch() ignores a custom Host header, so this uses node:http.
  const rebinding = await new Promise<number>((res, rej) =>
    request({ host: "127.0.0.1", port: ADMIN, path: "/admin/status", headers: { host: `evil.example:${ADMIN}` } }, (r) => res(r.statusCode ?? 0)).on("error", rej).end(),
  );
  assert.equal(rebinding, 421, "DNS rebinding guard");
  const noCookie = await fetch(`http://127.0.0.1:${ADMIN}/admin/upload/asset`, { method: "POST", headers: { origin: `http://127.0.0.1:${ADMIN}`, "x-keyside": "1" }, body: "x" });
  assert.equal(noCookie.status, 403);
  const api = await fetch(`http://127.0.0.1:${ADMIN}/api/v1/variables`);
  assert.equal(api.status, 401);
});

test("plain HTTP on the deck port only offers the socket", async () => {
  const r = await fetch(`http://127.0.0.1:${LAN}/`, { redirect: "manual" });
  assert.equal(r.status, 308);
  assert.match(r.headers.get("location") ?? "", /^https:\/\/127\.0\.0\.1:17443\//);
});

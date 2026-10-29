// Starts the companion with a probe plugin that tries to escape the Deno sandbox.
// Needs Deno on PATH (or PHONEDECK_DENO) and a built companion.

import { test, before, after } from "node:test";
import assert from "node:assert/strict";
import { spawn, type ChildProcess } from "node:child_process";
import { mkdtempSync, readFileSync, existsSync, writeFileSync, cpSync, mkdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const root = new URL("../../../", import.meta.url).pathname;
const bin = process.env.PHONEDECK_BIN ?? join(root, "target/debug/phonedeck");
const ADMIN = 17462;
const hasDeno = !!process.env.PHONEDECK_DENO || (process.env.PATH ?? "").split(process.platform === "win32" ? ";" : ":").some((d) => existsSync(join(d, process.platform === "win32" ? "deno.exe" : "deno")));
let child: ChildProcess;
let dir: string;

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

before(async () => {
  dir = mkdtempSync(join(tmpdir(), "phonedeck-sandbox-"));
  const plugins = join(dir, "bundled");
  cpSync(new URL("./fixtures/probe", import.meta.url).pathname, join(plugins, "probe"), { recursive: true });
  mkdirSync(join(dir, "data"));
  writeFileSync(join(dir, "data", "settings.json"), JSON.stringify({ adminPort: ADMIN, lanPort: ADMIN + 1, mdns: false, followFocus: false, lanEnabled: false }));
  if (!existsSync(bin) || !hasDeno) return;
  child = spawn(bin, ["--data-dir", join(dir, "data"), "--plugins", plugins], {
    env: { ...process.env, PHONEDECK_NO_KEYCHAIN: "1", PHONEDECK_LOG: "warn" },
    stdio: ["ignore", "inherit", "inherit"],
  });
});

after(() => child?.kill());

async function variables(): Promise<Record<string, unknown>> {
  const token = JSON.parse(readFileSync(join(dir, "data", "identity.json"), "utf8")).admin_token;
  const login = await fetch(`http://127.0.0.1:${ADMIN}/admin/login`, {
    method: "POST",
    headers: { "content-type": "application/json", origin: `http://127.0.0.1:${ADMIN}` },
    body: JSON.stringify({ token }),
  });
  const cookie = (login.headers.get("set-cookie") ?? "").split(";")[0];
  const ws = new WebSocket(`ws://127.0.0.1:${ADMIN}/admin/ws`, { headers: { cookie, origin: `http://127.0.0.1:${ADMIN}` } } as never);
  const result = new Promise<any>((res) => {
    ws.onmessage = (e) => {
      const m = JSON.parse(String(e.data));
      if (m.t === "result" && m.id === 1) res(m.data);
    };
  });
  await new Promise((r) => (ws.onopen = r));
  ws.send(JSON.stringify({ t: "req", id: 1, op: "variables.list" }));
  const v = await result;
  ws.close();
  return v;
}

test("a plugin can't reach anything it wasn't granted", { skip: (!existsSync(bin) && "companion not built") || (!hasDeno && "Deno not installed") }, async () => {
  let vars: Record<string, unknown> = {};
  for (let i = 0; i < 100; i++) {
    await sleep(200);
    try {
      vars = await variables();
      if (vars["probe.done"]) break;
    } catch {
      /* not up yet */
    }
  }
  assert.equal(vars["probe.done"], true, "probe plugin ran");
  for (const blocked of ["readSecrets", "readIdentity", "readEtc", "writeOutside", "writeOwnCode", "env", "run", "ffi", "sys", "remoteImport", "netOther", "netPublic"]) {
    assert.equal(vars[`probe.${blocked}`], "denied", `${blocked} must be denied`);
  }
  for (const allowed of ["netGranted", "writeOwnData", "readOwnCode"]) {
    assert.equal(vars[`probe.${allowed}`], "allowed", `${allowed} must work`);
  }
});

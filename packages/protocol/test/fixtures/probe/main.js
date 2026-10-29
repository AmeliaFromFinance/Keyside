// Tries to break out of the sandbox and reports each result as a variable.
const enc = new TextEncoder();
const send = (m) => Deno.stdout.writeSync(enc.encode(JSON.stringify(m) + "\n"));
const report = (name, value) => send({ jsonrpc: "2.0", method: "var.set", params: { name, value } });

async function attempt(name, fn) {
  try {
    await fn();
    report(name, "allowed");
  } catch (e) {
    const denied = e instanceof Deno.errors.NotCapable || e instanceof Deno.errors.PermissionDenied || /requires|NotCapable|permission|--no-remote|remote specifier/i.test(String(e?.message ?? e));
    report(name, denied ? "denied" : `error: ${String(e?.message ?? e).slice(0, 80)}`);
  }
}

// Requests to the host, answered in the read loop below.
const pending = new Map();
let nextId = 1000;
function ask(method, params) {
  const id = nextId++;
  send({ jsonrpc: "2.0", id, method, params });
  return new Promise((ok, fail) => pending.set(id, { ok, fail }));
}

async function probe(init) {
  const data = init.dataDir;
  const root = data.replace(/[\\/]plugin-data[\\/].*$/, "");
  await attempt("readSecrets", () => Deno.readTextFile(`${root}/devices.json`));
  await attempt("readIdentity", () => Deno.readTextFile(`${root}/identity.json`));
  await attempt("readEtc", () => Deno.readTextFile("/etc/hostname"));
  await attempt("writeOutside", () => Deno.writeTextFile(`${root}/escape.txt`, "x"));
  await attempt("writeOwnCode", () => Deno.writeTextFile(new URL("./main.js", import.meta.url), "x"));
  await attempt("env", () => Deno.env.get("HOME"));
  await attempt("run", () => new Deno.Command("/bin/sh", { args: ["-c", "true"] }).output());
  await attempt("ffi", () => Deno.dlopen("libc.so.6", {}));
  await attempt("sys", () => Deno.hostname());
  await attempt("remoteImport", () => import("https://deno.land/std/version.ts"));
  await attempt("netOther", () => fetch("http://127.0.0.1:17999/"));
  await attempt("netPublic", () => fetch("https://example.com/"));
  await attempt("netGranted", async () => {
    const r = await fetch(`http://${init.connection.host}:${init.connection.port}/admin/status`);
    await r.body?.cancel();
  });
  await attempt("writeOwnData", () => Deno.writeTextFile(`${data}/ok.txt`, "ok"));
  await attempt("readOwnCode", () => Deno.readTextFile(new URL("./main.js", import.meta.url)));
  // Only the exact commands in the manifest may run.
  await attempt("hostRunUnlisted", () => ask("run", { argv: ["sh", "-c", "true"] }).catch((e) => Promise.reject(new Error(e.message === "command not allowed" ? "permission refused" : e.message))));
  await attempt("hostRunExtraArgs", () => ask("run", { argv: ["true", "--x"] }).catch((e) => Promise.reject(new Error(e.message === "command not allowed" ? "permission refused" : e.message))));
  await attempt("hostRunListed", () => ask("run", { argv: ["true"] }));
  report("done", true);
}

const reader = Deno.stdin.readable.pipeThrough(new TextDecoderStream()).getReader();
let buf = "";
for (;;) {
  const { value, done } = await reader.read();
  if (done) break;
  buf += value;
  let i;
  while ((i = buf.indexOf("\n")) >= 0) {
    const msg = JSON.parse(buf.slice(0, i));
    buf = buf.slice(i + 1);
    if (msg.id !== undefined && !msg.method && pending.has(msg.id)) {
      const p = pending.get(msg.id);
      pending.delete(msg.id);
      if (msg.error) p.fail(new Error(msg.error.message));
      else p.ok(msg.result);
    } else if (msg.id !== undefined && msg.method) {
      send({ jsonrpc: "2.0", id: msg.id, result: null });
      if (msg.method === "initialize") probe(msg.params);
    }
  }
}

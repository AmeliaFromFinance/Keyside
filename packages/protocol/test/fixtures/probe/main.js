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
    if (msg.id !== undefined && msg.method) {
      send({ jsonrpc: "2.0", id: msg.id, result: null });
      if (msg.method === "initialize") probe(msg.params);
    }
  }
}

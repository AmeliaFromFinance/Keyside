// Writes schema/vectors/channel.json. The Rust companion must reproduce every value.
// Run with --new-keys to replace the fixed key pairs.

import { writeFileSync, readFileSync, existsSync } from "node:fs";
import { ClientHandshake, importPsk, ServerHandshake } from "../src/channel.ts";
import { fromB64u, randomBytes, toB64u } from "../src/bytes.ts";

const out = new URL("../../../schema/vectors/channel.json", import.meta.url);
const subtle = crypto.subtle;

async function keyPair(jwk?: JsonWebKey): Promise<{ pair: CryptoKeyPair; jwk: JsonWebKey }> {
  if (!jwk) {
    const p = (await subtle.generateKey({ name: "ECDH", namedCurve: "P-256" }, true, ["deriveBits"])) as CryptoKeyPair;
    jwk = await subtle.exportKey("jwk", p.privateKey);
  }
  const privateKey = await subtle.importKey("jwk", jwk, { name: "ECDH", namedCurve: "P-256" }, false, ["deriveBits"]);
  const { d: _d, ...pub } = jwk;
  const publicKey = await subtle.importKey("jwk", { ...pub, key_ops: [] }, { name: "ECDH", namedCurve: "P-256" }, true, []);
  return { pair: { privateKey, publicKey }, jwk };
}

const prev = existsSync(out) && !process.argv.includes("--new-keys") ? JSON.parse(readFileSync(out, "utf8")) : null;
const cases = [];

for (const [i, mode] of (["pair", "resume"] as const).entries()) {
  const p = prev?.cases?.[i];
  const client = await keyPair(p?.client_jwk);
  const server = await keyPair(p?.server_jwk);
  const psk = p ? fromB64u(p.psk) : randomBytes(mode === "pair" ? 16 : 32);
  const cid = p ? fromB64u(p.cid) : randomBytes(16);
  const id = p ? fromB64u(p.id) : randomBytes(mode === "pair" ? 8 : 16);
  const nc = p ? fromB64u(p.client_nonce) : randomBytes(16);
  const ns = p ? fromB64u(p.server_nonce) : randomBytes(16);

  const { hs, hello } = await ClientHandshake.start({ mode, cid: toB64u(cid), id: toB64u(id), psk: await importPsk(psk), fixed: { pair: client.pair, nonce: nc } });
  const srv = await ServerHandshake.respond(hello, { cid, psk: await importPsk(psk), fixed: { pair: server.pair, nonce: ns } });
  const cl = await hs.finish(srv.challenge);
  const ss = await srv.hs.accept(cl.confirm);
  const c2s = await cl.session.seal({ t: "ping", ts: 1 });
  const s2c = await ss.seal({ t: "pong", ts: 1 });

  cases.push({
    mode,
    psk: toB64u(psk),
    cid: toB64u(cid),
    id: toB64u(id),
    client_jwk: client.jwk,
    server_jwk: server.jwk,
    client_nonce: toB64u(nc),
    server_nonce: toB64u(ns),
    hello_e: hello.e,
    challenge_e: srv.challenge.e,
    challenge_mac: srv.challenge.mac,
    confirm_mac: cl.confirm.mac,
    sas: cl.sas,
    first_c2s: { plain: '{"t":"ping","ts":1}', frame: toB64u(c2s) },
    first_s2c: { plain: '{"t":"pong","ts":1}', frame: toB64u(s2c) },
  });
}

writeFileSync(out, JSON.stringify({ protocol: "keyside/1", cases }, null, 2) + "\n");
console.log(`wrote ${cases.length} cases`);

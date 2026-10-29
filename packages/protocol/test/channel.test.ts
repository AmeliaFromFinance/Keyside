import { test } from "node:test";
import assert from "node:assert/strict";
import { ClientHandshake, HandshakeError, importPsk, ServerHandshake } from "../src/channel.ts";
import { randomBytes, toB64u } from "../src/bytes.ts";

async function handshake(clientPsk: Uint8Array, serverPsk: Uint8Array) {
  const cid = randomBytes(16);
  const { hs, hello } = await ClientHandshake.start({
    mode: "pair",
    cid: toB64u(cid),
    id: toB64u(randomBytes(8)),
    psk: await importPsk(clientPsk),
    device: { name: "Test phone", kind: "phone", app: "web" },
  });
  const server = await ServerHandshake.respond(hello, { cid, psk: await importPsk(serverPsk) });
  const client = await hs.finish(server.challenge);
  const serverSession = await server.hs.accept(client.confirm);
  return { client, server, serverSession };
}

test("both sides agree and can talk", async () => {
  const psk = randomBytes(16);
  const { client, server, serverSession } = await handshake(psk, psk);
  assert.equal(client.sas, server.sas);
  assert.match(client.sas, /^\d{6}$/);
  const frame = await client.session.seal({ t: "input", key: "k1" });
  assert.deepEqual(await serverSession.open(frame), { t: "input", key: "k1" });
  const back = await serverSession.seal({ t: "delta", rev: 1 });
  assert.deepEqual(await client.session.open(back), { t: "delta", rev: 1 });
});

test("wrong key is rejected by the client", async () => {
  await assert.rejects(handshake(randomBytes(16), randomBytes(16)), HandshakeError);
});

test("tampered frames are rejected", async () => {
  const psk = randomBytes(32);
  const { client, serverSession } = await handshake(psk, psk);
  const frame = await client.session.seal({ a: 1 });
  frame[frame.length - 1] ^= 1;
  await assert.rejects(serverSession.open(frame));
});

test("replayed and reordered frames are rejected", async () => {
  const psk = randomBytes(32);
  const { client, serverSession } = await handshake(psk, psk);
  const f0 = await client.session.seal({ n: 0 });
  const f1 = await client.session.seal({ n: 1 });
  await assert.rejects(serverSession.open(f1), HandshakeError);
  const fresh = await handshake(psk, psk);
  const g0 = await fresh.client.session.seal({ n: 0 });
  assert.deepEqual(await fresh.serverSession.open(g0), { n: 0 });
  await assert.rejects(fresh.serverSession.open(g0), HandshakeError);
  void f0;
});

test("a frame from one direction can't be reflected back", async () => {
  const psk = randomBytes(32);
  const { client } = await handshake(psk, psk);
  const f = await client.session.seal({ n: 0 });
  await assert.rejects(client.session.open(f));
});

test("concurrent sends keep their order", async () => {
  const psk = randomBytes(16);
  const { client, serverSession } = await handshake(psk, psk);
  const frames = await Promise.all(Array.from({ length: 20 }, (_, i) => client.session.seal({ i })));
  for (let i = 0; i < 20; i++) assert.deepEqual(await serverSession.open(frames[i]), { i });
});

test("bad public keys are refused", async () => {
  const psk = randomBytes(16);
  const { hello } = await ClientHandshake.start({ mode: "pair", cid: toB64u(randomBytes(16)), id: toB64u(randomBytes(8)), psk: await importPsk(psk) });
  hello.e = toB64u(new Uint8Array(65).fill(4));
  await assert.rejects(ServerHandshake.respond(hello, { cid: randomBytes(16), psk: await importPsk(psk) }));
});

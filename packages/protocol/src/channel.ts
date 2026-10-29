// Secure channel used by every remote device.
// Both sides prove they hold a pre-shared key (the pairing code or the device secret)
// and agree on fresh AES-256-GCM keys through an ephemeral P-256 exchange.
// See docs/protocol.md for the byte layout.

import { concat, fromB64u, fromUtf8, lp, randomBytes, toB64u, u64be, utf8, type Bytes } from "./bytes.ts";

export const PROTOCOL = "keyside/1";
export const MAX_FRAME = 2 * 1024 * 1024;
const MAX_COUNTER = 2n ** 32n;
const DIR_C2S = 1;
const DIR_S2C = 2;

export type Mode = "pair" | "resume";
export type DeviceKind = "phone" | "tablet" | "desktop" | "browser";
export type DeviceApp = "web" | "android" | "ios" | "desktop";

export interface DeviceInfo {
  name: string;
  kind: DeviceKind;
  app: DeviceApp;
}

export interface HelloMsg {
  t: "hello";
  v: 1;
  mode: Mode;
  /** Companion id from the QR code. */
  cid: string;
  /** Pairing id (pair) or device id (resume). */
  id: string;
  e: string;
  n: string;
  device?: DeviceInfo;
}

export interface ChallengeMsg {
  t: "challenge";
  e: string;
  n: string;
  mac: string;
}

export interface ConfirmMsg {
  t: "confirm";
  mac: string;
}

export class HandshakeError extends Error {}

const subtle = () => globalThis.crypto.subtle;

/** Imports a pre-shared key so it can't be read back out of storage. */
export async function importPsk(raw: Uint8Array, extractable = false): Promise<CryptoKey> {
  if (raw.length < 16) throw new RangeError("psk too short");
  return subtle().importKey("raw", new Uint8Array(raw), { name: "HMAC", hash: "SHA-256" }, extractable, ["sign"]);
}

async function hmac(key: CryptoKey | Bytes, data: Bytes): Promise<Bytes> {
  const k = key instanceof Uint8Array
    ? await subtle().importKey("raw", key, { name: "HMAC", hash: "SHA-256" }, false, ["sign"])
    : key;
  return new Uint8Array(await subtle().sign("HMAC", k, data));
}

/** HKDF-SHA256 with the psk as salt. Same output as RFC 5869 with salt=psk, ikm=shared. */
async function hkdf(psk: CryptoKey, ikm: Bytes, info: Bytes, len: number): Promise<Bytes> {
  const prk = await hmac(psk, ikm);
  const out = new Uint8Array(len);
  let prev = new Uint8Array(0);
  for (let i = 1, o = 0; o < len; i++) {
    prev = await hmac(prk, concat(prev, info, Uint8Array.of(i)));
    out.set(prev.subarray(0, Math.min(prev.length, len - o)), o);
    o += prev.length;
  }
  return out;
}

export function transcript(mode: Mode, cid: Bytes, id: Bytes, ec: Bytes, nc: Bytes, es: Bytes, ns: Bytes): Bytes {
  return concat(lp(utf8(PROTOCOL)), lp(utf8(mode)), lp(cid), lp(id), lp(ec), lp(nc), lp(es), lp(ns));
}

interface Derived {
  th: Bytes;
  c2s: CryptoKey;
  s2c: CryptoKey;
  macS: CryptoKey;
  macC: CryptoKey;
  sas: string;
}

async function derive(psk: CryptoKey, shared: Bytes, transcriptBytes: Bytes): Promise<Derived> {
  const th = new Uint8Array(await subtle().digest("SHA-256", transcriptBytes));
  const okm = await hkdf(psk, shared, concat(lp(utf8(`${PROTOCOL} keys`)), th), 128);
  const aes = (b: Bytes) => subtle().importKey("raw", b, "AES-GCM", false, ["encrypt", "decrypt"]);
  const mac = (b: Bytes) => subtle().importKey("raw", b, { name: "HMAC", hash: "SHA-256" }, false, ["sign", "verify"]);
  const macS = await mac(okm.slice(64, 96));
  const sasBytes = await hmac(macS, concat(utf8("sas"), th));
  const sasNum = new DataView(sasBytes.buffer).getUint32(0) % 1_000_000;
  return {
    th,
    c2s: await aes(okm.slice(0, 32)),
    s2c: await aes(okm.slice(32, 64)),
    macS,
    macC: await mac(okm.slice(96, 128)),
    sas: String(sasNum).padStart(6, "0"),
  };
}

async function ecdh(priv: CryptoKey, peerRaw: Bytes): Promise<Bytes> {
  if (peerRaw.length !== 65 || peerRaw[0] !== 4) throw new HandshakeError("bad public key");
  const peer = await subtle().importKey("raw", peerRaw, { name: "ECDH", namedCurve: "P-256" }, false, []);
  return new Uint8Array(await subtle().deriveBits({ name: "ECDH", public: peer }, priv, 256));
}

async function newEphemeral(): Promise<CryptoKeyPair> {
  return subtle().generateKey({ name: "ECDH", namedCurve: "P-256" }, false, ["deriveBits"]) as Promise<CryptoKeyPair>;
}

const exportRaw = async (k: CryptoKey) => new Uint8Array(await subtle().exportKey("raw", k));

/** Encrypted, ordered, replay-proof message stream. */
export class Session {
  #sendKey: CryptoKey;
  #recvKey: CryptoKey;
  #sendDir: number;
  #recvDir: number;
  #sendCtr = 0n;
  #recvCtr = 0n;
  #sendTail: Promise<unknown> = Promise.resolve();
  #recvTail: Promise<unknown> = Promise.resolve();

  constructor(sendKey: CryptoKey, recvKey: CryptoKey, sendDir: number, recvDir: number) {
    this.#sendKey = sendKey;
    this.#recvKey = recvKey;
    this.#sendDir = sendDir;
    this.#recvDir = recvDir;
  }

  /** Encrypts in call order, even when called concurrently. */
  seal(msg: unknown): Promise<Bytes> {
    const ctr = this.#sendCtr++;
    if (ctr >= MAX_COUNTER) return Promise.reject(new HandshakeError("session exhausted"));
    const plain = utf8(JSON.stringify(msg));
    const job = this.#sendTail.then(async () => {
      const ct = await subtle().encrypt(
        { name: "AES-GCM", iv: nonce(ctr), additionalData: Uint8Array.of(this.#sendDir) },
        this.#sendKey,
        plain,
      );
      const frame = concat(u64be(ctr), new Uint8Array(ct));
      if (frame.length > MAX_FRAME) throw new RangeError("message too large");
      return frame;
    });
    this.#sendTail = job.catch(() => undefined);
    return job;
  }

  /** Decrypts in arrival order. Throws on tampering, replay or reordering; close the socket if it does. */
  open<T = unknown>(frame: Bytes): Promise<T> {
    const job = this.#recvTail.then(async () => {
      if (frame.length < 8 + 16 || frame.length > MAX_FRAME) throw new HandshakeError("bad frame");
      const ctr = new DataView(frame.buffer, frame.byteOffset, 8).getBigUint64(0);
      if (ctr !== this.#recvCtr) throw new HandshakeError("out of order frame");
      const pt = await subtle().decrypt(
        { name: "AES-GCM", iv: nonce(ctr), additionalData: Uint8Array.of(this.#recvDir) },
        this.#recvKey,
        frame.subarray(8),
      );
      this.#recvCtr++;
      return JSON.parse(fromUtf8(new Uint8Array(pt))) as T;
    });
    this.#recvTail = job.catch(() => undefined);
    return job;
  }
}

function nonce(ctr: bigint): Bytes {
  return concat(new Uint8Array(4), u64be(ctr));
}

export interface HandshakeResult {
  session: Session;
  /** Six digits both screens show while a new device waits for approval. */
  sas: string;
}

export interface ClientOptions {
  mode: Mode;
  cid: string;
  id: string;
  psk: CryptoKey;
  device?: DeviceInfo;
  /** Test vectors only. */
  fixed?: { pair: CryptoKeyPair; nonce: Bytes };
}

export class ClientHandshake {
  #o: ClientOptions;
  #pair!: CryptoKeyPair;
  #e!: Bytes;
  #n!: Bytes;

  private constructor(o: ClientOptions) {
    this.#o = o;
  }

  static async start(o: ClientOptions): Promise<{ hs: ClientHandshake; hello: HelloMsg }> {
    const hs = new ClientHandshake(o);
    hs.#pair = o.fixed?.pair ?? (await newEphemeral());
    hs.#e = await exportRaw(hs.#pair.publicKey);
    hs.#n = o.fixed?.nonce ?? randomBytes(16);
    const hello: HelloMsg = { t: "hello", v: 1, mode: o.mode, cid: o.cid, id: o.id, e: toB64u(hs.#e), n: toB64u(hs.#n) };
    if (o.device) hello.device = o.device;
    return { hs, hello };
  }

  async finish(ch: ChallengeMsg): Promise<HandshakeResult & { confirm: ConfirmMsg }> {
    if (ch?.t !== "challenge") throw new HandshakeError("unexpected message");
    const es = fromB64u(ch.e, 65);
    const ns = fromB64u(ch.n, 16);
    const shared = await ecdh(this.#pair.privateKey, es);
    const d = await derive(this.#o.psk, shared, transcript(this.#o.mode, fromB64u(this.#o.cid), fromB64u(this.#o.id), this.#e, this.#n, es, ns));
    const ok = await subtle().verify("HMAC", d.macS, fromB64u(ch.mac, 32), concat(utf8("server"), d.th));
    if (!ok) throw new HandshakeError("server could not prove the key");
    const mac = await hmac(d.macC, concat(utf8("client"), d.th));
    const keys = roleKeys(d, "client");
    return { confirm: { t: "confirm", mac: toB64u(mac) }, session: new Session(keys.send, keys.recv, DIR_C2S, DIR_S2C), sas: d.sas };
  }
}

export interface ServerOptions {
  cid: Bytes;
  psk: CryptoKey;
  fixed?: { pair: CryptoKeyPair; nonce: Bytes };
}

/** Reference server side. The companion implements the same steps in Rust. */
export class ServerHandshake {
  #d!: Derived;

  static async respond(hello: HelloMsg, o: ServerOptions): Promise<{ hs: ServerHandshake; challenge: ChallengeMsg; sas: string }> {
    if (hello?.t !== "hello" || hello.v !== 1) throw new HandshakeError("unsupported hello");
    const ec = fromB64u(hello.e, 65);
    const nc = fromB64u(hello.n, 16);
    const pair = o.fixed?.pair ?? (await newEphemeral());
    const es = await exportRaw(pair.publicKey);
    const ns = o.fixed?.nonce ?? randomBytes(16);
    const shared = await ecdh(pair.privateKey, ec);
    const hs = new ServerHandshake();
    hs.#d = await derive(o.psk, shared, transcript(hello.mode, o.cid, fromB64u(hello.id), ec, nc, es, ns));
    const mac = await hmac(hs.#d.macS, concat(utf8("server"), hs.#d.th));
    return { hs, challenge: { t: "challenge", e: toB64u(es), n: toB64u(ns), mac: toB64u(mac) }, sas: hs.#d.sas };
  }

  async accept(c: ConfirmMsg): Promise<Session> {
    if (c?.t !== "confirm") throw new HandshakeError("unexpected message");
    const ok = await subtle().verify("HMAC", this.#d.macC, fromB64u(c.mac, 32), concat(utf8("client"), this.#d.th));
    if (!ok) throw new HandshakeError("client could not prove the key");
    const keys = roleKeys(this.#d, "server");
    return new Session(keys.send, keys.recv, DIR_S2C, DIR_C2S);
  }
}

function roleKeys(d: Derived, role: "client" | "server") {
  return role === "client" ? { send: d.c2s, recv: d.s2c } : { send: d.s2c, recv: d.c2s };
}

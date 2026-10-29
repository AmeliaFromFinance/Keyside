/** Byte arrays backed by a plain ArrayBuffer, which Web Crypto requires. */
export type Bytes = Uint8Array<ArrayBuffer>;

const enc = new TextEncoder();
const dec = new TextDecoder("utf-8", { fatal: true });

export const utf8 = (s: string): Bytes => enc.encode(s);
export const fromUtf8 = (b: Bytes): string => dec.decode(b);

export function concat(...parts: Bytes[]): Bytes {
  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let o = 0;
  for (const p of parts) {
    out.set(p, o);
    o += p.length;
  }
  return out;
}

/** u16 big-endian length prefix, so concatenated fields can't be confused. */
export function lp(b: Bytes): Bytes {
  if (b.length > 0xffff) throw new RangeError("field too long");
  const out = new Uint8Array(2 + b.length);
  out[0] = b.length >> 8;
  out[1] = b.length & 0xff;
  out.set(b, 2);
  return out;
}

export function toB64u(b: Bytes): string {
  let s = "";
  for (const x of b) s += String.fromCharCode(x);
  return btoa(s).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

export function fromB64u(s: string, expectedLen?: number): Bytes {
  if (typeof s !== "string" || !/^[A-Za-z0-9_-]*$/.test(s)) throw new TypeError("bad base64url");
  const pad = s.length % 4 === 2 ? "==" : s.length % 4 === 3 ? "=" : s.length % 4 === 0 ? "" : null;
  if (pad === null) throw new TypeError("bad base64url length");
  const bin = atob(s.replace(/-/g, "+").replace(/_/g, "/") + pad);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  if (expectedLen !== undefined && out.length !== expectedLen) throw new RangeError("unexpected length");
  return out;
}

export function randomBytes(n: number): Bytes {
  return crypto.getRandomValues(new Uint8Array(n));
}

export function u64be(n: bigint): Bytes {
  const out = new Uint8Array(8);
  new DataView(out.buffer).setBigUint64(0, n);
  return out;
}

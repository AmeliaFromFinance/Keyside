// Downloads the Deno build PhoneDeck ships as its plugin runtime and checks it against the
// pinned hash in deno.json. Usage: node scripts/fetch-deno.ts <target triple> <out file>

import { createHash } from "node:crypto";
import { chmodSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname } from "node:path";
import { inflateRawSync } from "node:zlib";

const [target, out] = process.argv.slice(2);
const pins = JSON.parse(readFileSync(new URL("./deno.json", import.meta.url), "utf8"));
const expected: string | undefined = pins.sha256[target];
if (!target || !out || !expected) {
  console.error(`usage: fetch-deno.ts <${Object.keys(pins.sha256).join("|")}> <out file>`);
  process.exit(2);
}

const url = `https://github.com/denoland/deno/releases/download/v${pins.version}/deno-${target}.zip`;
const res = await fetch(url);
if (!res.ok) throw new Error(`download failed: ${res.status} ${url}`);
const zip = Buffer.from(await res.arrayBuffer());
const actual = createHash("sha256").update(zip).digest("hex");
if (actual !== expected) throw new Error(`checksum mismatch for ${target}: got ${actual}, expected ${expected}`);

// The archive holds one file. Read it through the central directory.
const end = zip.lastIndexOf(Buffer.from([0x50, 0x4b, 0x05, 0x06]));
const central = zip.readUInt32LE(end + 16);
const method = zip.readUInt16LE(central + 10);
const compressed = zip.readUInt32LE(central + 20);
const local = zip.readUInt32LE(central + 42);
const start = local + 30 + zip.readUInt16LE(local + 26) + zip.readUInt16LE(local + 28);
const raw = zip.subarray(start, start + compressed);
const body = method === 0 ? raw : inflateRawSync(raw);
mkdirSync(dirname(out), { recursive: true });
writeFileSync(out, body);
chmodSync(out, 0o755);
console.log(`deno ${pins.version} for ${target} -> ${out}`);

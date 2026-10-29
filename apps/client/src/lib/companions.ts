// Paired companions, kept in IndexedDB. The device secret is stored as a non-extractable
// key, so page scripts can use it for the handshake but can't read it back out.

export interface StoredCompanion {
  cid: string;
  name: string;
  hosts: string[];
  port: number;
  deviceId: string;
  psk: CryptoKey;
  pairedAt: number;
  lastUsed: number;
}

const DB = "keyside";
const STORE = "companions";

function open(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const req = indexedDB.open(DB, 1);
    req.onupgradeneeded = () => req.result.createObjectStore(STORE, { keyPath: "cid" });
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
}

async function tx<T>(mode: IDBTransactionMode, fn: (s: IDBObjectStore) => IDBRequest<T>): Promise<T> {
  const db = await open();
  return new Promise((resolve, reject) => {
    const r = fn(db.transaction(STORE, mode).objectStore(STORE));
    r.onsuccess = () => resolve(r.result);
    r.onerror = () => reject(r.error);
  });
}

export const companions = {
  all: () => tx<StoredCompanion[]>("readonly", (s) => s.getAll()).then((l) => l.sort((a, b) => b.lastUsed - a.lastUsed)),
  get: (cid: string) => tx<StoredCompanion | undefined>("readonly", (s) => s.get(cid)),
  put: (c: StoredCompanion) => tx("readwrite", (s) => s.put(c)),
  remove: (cid: string) => tx("readwrite", (s) => s.delete(cid)),
  async touch(cid: string, hosts?: string[]) {
    const c = await companions.get(cid);
    if (c) await companions.put({ ...c, lastUsed: Date.now(), hosts: [...(hosts ?? c.hosts)] });
  },
};

export interface PairParams {
  hosts: string[];
  port: number;
  cid: string;
  pid: string;
  code: string;
  name: string;
}

/** Reads the pairing fragment from a QR link: https://host:port/#pair&h=..&p=..&c=..&i=..&k=..&n=.. */
export function parsePairLink(text: string): PairParams | null {
  const hash = text.includes("#") ? text.slice(text.indexOf("#") + 1) : text;
  const q = new URLSearchParams(hash);
  if (!q.has("pair")) return null;
  const valid = (v: string | null, len?: number) => !!v && /^[A-Za-z0-9_-]+$/.test(v) && (!len || v.length === len);
  const hosts = (q.get("h") ?? "")
    .split(",")
    .map((h) => h.trim())
    .filter((h) => /^[A-Za-z0-9.:[\]-]{1,64}$/.test(h));
  const port = Number(q.get("p"));
  const cid = q.get("c");
  const pid = q.get("i");
  const code = q.get("k");
  if (!hosts.length || !(port > 0 && port < 65536) || !valid(cid, 22) || !valid(pid, 11) || !valid(code, 22)) return null;
  return { hosts, port, cid: cid!, pid: pid!, code: code!, name: (q.get("n") ?? "Keyside").slice(0, 40) };
}

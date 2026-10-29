// Test helper: a local HTTP server that answers like an online API.

import { createServer, type IncomingMessage } from "node:http";
import type { AddressInfo } from "node:net";

export interface Hit {
  method: string;
  path: string;
  query: URLSearchParams;
  headers: IncomingMessage["headers"];
  body: any;
}

export async function fakeHttp(route: (h: Hit) => { status?: number; json?: unknown } | undefined) {
  const hits: Hit[] = [];
  const server = createServer(async (req, res) => {
    let raw = "";
    for await (const c of req) raw += c;
    const url = new URL(req.url ?? "/", "http://x");
    let body: any = raw;
    if ((req.headers["content-type"] ?? "").includes("json") && raw) body = JSON.parse(raw);
    else if ((req.headers["content-type"] ?? "").includes("urlencoded")) body = Object.fromEntries(new URLSearchParams(raw));
    const hit = { method: req.method ?? "GET", path: url.pathname, query: url.searchParams, headers: req.headers, body };
    hits.push(hit);
    const r = route(hit) ?? { status: 404, json: { message: "not found" } };
    res.writeHead(r.status ?? 200, { "content-type": "application/json" });
    res.end(r.json === undefined ? "" : JSON.stringify(r.json));
  });
  await new Promise<void>((r) => server.listen(0, "127.0.0.1", () => r()));
  return {
    url: `http://127.0.0.1:${(server.address() as AddressInfo).port}`,
    hits,
    close: () => new Promise<void>((r) => (server.closeAllConnections(), server.close(() => r()))),
  };
}

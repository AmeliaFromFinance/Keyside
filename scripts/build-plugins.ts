// Bundles every plugins/<name>/src/main.ts into plugins/<name>/main.js.
// Plugins ship as one file because the sandbox blocks loading code from anywhere else.

import { build } from "esbuild";
import { existsSync, readdirSync } from "node:fs";
import { join } from "node:path";

const root = new URL("../plugins/", import.meta.url).pathname;
const only = process.argv.slice(2);
const names = readdirSync(root).filter((n) => existsSync(join(root, n, "src/main.ts")) && (only.length === 0 || only.includes(n)));

for (const name of names) {
  await build({
    entryPoints: [join(root, name, "src/main.ts")],
    outfile: join(root, name, "main.js"),
    bundle: true,
    format: "esm",
    platform: "browser",
    target: "es2022",
    conditions: ["deno", "browser", "import"],
    legalComments: "inline",
    banner: { js: `// ${name} plugin for Keyside. Built file, edit src/ instead.` },
    logLevel: "warning",
  });
  console.log(`built ${name}`);
}

// Copies the built first-party plugins (manifest.json and main.js only) to a folder the
// desktop app bundles. Usage: node scripts/stage-plugins.ts <out dir>

import { copyFileSync, existsSync, mkdirSync, readdirSync, rmSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const out = process.argv[2];
if (!out) {
  console.error("usage: stage-plugins.ts <out dir>");
  process.exit(2);
}
const root = fileURLToPath(new URL("../plugins/", import.meta.url));
rmSync(out, { recursive: true, force: true });
for (const name of readdirSync(root)) {
  const dir = join(root, name);
  if (name.startsWith("_") || !existsSync(join(dir, "manifest.json"))) continue;
  if (!existsSync(join(dir, "main.js"))) throw new Error(`${name} isn't built. Run pnpm build:plugins first.`);
  mkdirSync(join(out, name), { recursive: true });
  for (const f of ["manifest.json", "main.js"]) copyFileSync(join(dir, f), join(out, name, f));
}
console.log(`staged plugins in ${out}`);

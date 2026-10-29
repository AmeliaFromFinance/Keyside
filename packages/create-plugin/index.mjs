#!/usr/bin/env node
// npm create keyside-plugin <folder>

import { cpSync, existsSync, readFileSync, renameSync, writeFileSync, readdirSync, statSync } from "node:fs";
import { basename, join, resolve } from "node:path";

const target = process.argv[2];
if (!target) {
  console.error("Usage: npm create keyside-plugin <folder>");
  process.exit(1);
}
const dir = resolve(target);
if (existsSync(dir) && readdirSync(dir).length > 0) {
  console.error(`${dir} is not empty`);
  process.exit(1);
}
const slug = basename(dir).toLowerCase().replace(/[^a-z0-9-]/g, "-").replace(/^-+|-+$/g, "") || "my-plugin";
const prefix = slug.replace(/-/g, "_").replace(/^[^a-z]+/, "").slice(0, 16) || "plugin";

cpSync(new URL("./template", import.meta.url).pathname, dir, { recursive: true });

const fill = (p) => {
  if (statSync(p).isDirectory()) return readdirSync(p).forEach((n) => fill(join(p, n)));
  writeFileSync(p, readFileSync(p, "utf8").replaceAll("__SLUG__", slug).replaceAll("__PREFIX__", prefix));
};
fill(dir);
// npm drops .gitignore from published packages, so it ships as "gitignore".
if (existsSync(join(dir, "gitignore"))) renameSync(join(dir, "gitignore"), join(dir, ".gitignore"));

console.log(`Created ${dir}

Next:
  cd ${target}
  npm install
  npm run build        # bundles src/main.ts into main.js
  npm test

Install it from the editor: Plugins > Install from file, after packing it with
  keyside plugin pack .`);

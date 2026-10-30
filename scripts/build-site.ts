// Builds the project website (GitHub Pages) into site/dist: the landing page plus the
// Markdown docs as HTML. Usage: node scripts/build-site.ts

import { copyFileSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, posix, relative } from "node:path";
import { Marked } from "marked";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
const site = join(root, "site");
const out = join(site, "dist");
const repo = "https://github.com/AmeliaFromFinance/keyside";

const pages: Array<{ src: string; slug: string; title: string }> = [
  { src: "docs/plugins.md", slug: "plugins", title: "Plugin setup" },
  { src: "docs/stream-tools.md", slug: "stream-tools", title: "Stream tools" },
  { src: "docs/remote-access.md", slug: "remote-access", title: "Remote access" },
  { src: "docs/stream-deck-plugins.md", slug: "stream-deck-plugins", title: "Stream Deck plugins" },
  { src: "docs/plugin-guide.md", slug: "plugin-guide", title: "Writing plugins" },
  { src: "docs/inbound-api.md", slug: "inbound-api", title: "HTTP API" },
  { src: "docs/protocol.md", slug: "protocol", title: "Protocol" },
  { src: "PRIVACY.md", slug: "privacy", title: "Privacy" },
  { src: "SECURITY.md", slug: "security", title: "Security" },
  { src: "CONTRIBUTING.md", slug: "contributing", title: "Contributing" },
  { src: "CHANGELOG.md", slug: "changelog", title: "Changelog" },
];

const pageFor = new Map(pages.map((p) => [p.src, p]));
const read = (p: string) => readFileSync(join(root, p), "utf8");
const esc = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
const slugify = (s: string) =>
  s
    .toLowerCase()
    .replace(/<[^>]+>/g, "")
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "");

/** Points a link in a Markdown file at its page on the site, or at the repository. */
function link(href: string, from: string, here: string): string {
  if (/^([a-z]+:|#|mailto:)/i.test(href)) return href;
  const [path, hash] = href.split("#");
  const target = posix.normalize(posix.join(posix.dirname(from), path));
  const page = pageFor.get(target);
  if (page) return relative(here, `docs/${page.slug}/`).replace(/\\/g, "/") + "/" + (hash ? `#${hash}` : "");
  if (target.startsWith("docs/images/")) return relative(here, target.replace("docs/", "")).replace(/\\/g, "/");
  if (target === "README.md") return relative(here, "") || "./";
  return `${repo}/blob/main/${target}${hash ? `#${hash}` : ""}`;
}

function render(src: string, here: string): string {
  const md = new Marked({ gfm: true });
  md.use({
    renderer: {
      heading({ tokens, depth }) {
        const html = this.parser.parseInline(tokens);
        return `<h${depth} id="${slugify(html)}">${html}</h${depth}>\n`;
      },
    },
    walkTokens(token) {
      if (token.type === "link" || token.type === "image") token.href = link(token.href, src, here);
    },
  });
  // The first heading becomes the page title.
  return md.parse(read(src).replace(/^# .*\n/, "")) as string;
}

function layout(file: string, vars: Record<string, string>): string {
  return readFileSync(join(site, file), "utf8").replace(/\{\{(\w+)\}\}/g, (_, k) => vars[k] ?? "");
}

rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });

const base = (here: string) => (relative(here, "") || ".").replace(/\\/g, "/");
for (const p of pages) {
  const here = `docs/${p.slug}`;
  const nav = pages.map((q) => `<a href="${base(here)}/docs/${q.slug}/"${q.slug === p.slug ? ' aria-current="page"' : ""}>${esc(q.title)}</a>`).join("\n");
  const html = layout("doc.html", { title: esc(p.title), base: base(here), nav, content: render(p.src, here), source: `${repo}/blob/main/${p.src}` });
  mkdirSync(join(out, here), { recursive: true });
  writeFileSync(join(out, here, "index.html"), html);
}

writeFileSync(join(out, "index.html"), layout("index.html", { repo }));
writeFileSync(join(out, "404.html"), layout("404.html", {}));
for (const f of ["site.css", "deck.js", "CNAME"]) copyFileSync(join(site, f), join(out, f));

const assets: Array<[string, string]> = [
  ["apps/client/public/icon.svg", "icon.svg"],
  ["docs/images/phone.png", "images/phone.png"],
  ["docs/images/editor.png", "images/editor.png"],
  ["apps/client/node_modules/@fontsource-variable/inter/files/inter-latin-wght-normal.woff2", "fonts/inter.woff2"],
  ["apps/client/node_modules/@fontsource/jetbrains-mono/files/jetbrains-mono-latin-400-normal.woff2", "fonts/jetbrains-mono.woff2"],
];
for (const [from, to] of assets) {
  mkdirSync(dirname(join(out, to)), { recursive: true });
  copyFileSync(join(root, from), join(out, to));
}
// The plugin registry, so it can also be fetched from the site.
mkdirSync(join(out, "registry"), { recursive: true });
copyFileSync(join(root, "registry/index.json"), join(out, "registry/index.json"));

console.log(`site built in ${relative(root, out)}`);

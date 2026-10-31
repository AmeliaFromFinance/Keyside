// Builds the project website (GitHub Pages) into site/dist: the landing page plus the
// Markdown docs as HTML. Usage: node scripts/build-site.ts

import {
  copyFileSync,
  mkdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { dirname, join, posix, relative } from "node:path";
import { Marked } from "marked";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
const site = join(root, "site");
const out = join(site, "dist");
const repo = "https://github.com/AmeliaFromFinance/keyside";

const pages: Array<{
  src: string;
  slug: string;
  title: string;
  group: string;
}> = [
  {
    src: "docs/plugins.md",
    slug: "plugins",
    title: "Plugin setup",
    group: "Using Keyside",
  },
  {
    src: "docs/stream-tools.md",
    slug: "stream-tools",
    title: "Stream tools",
    group: "Using Keyside",
  },
  {
    src: "docs/remote-access.md",
    slug: "remote-access",
    title: "Remote access",
    group: "Using Keyside",
  },
  {
    src: "docs/stream-deck-plugins.md",
    slug: "stream-deck-plugins",
    title: "Stream Deck plugins",
    group: "Using Keyside",
  },
  {
    src: "docs/plugin-guide.md",
    slug: "plugin-guide",
    title: "Writing plugins",
    group: "Building on it",
  },
  {
    src: "docs/inbound-api.md",
    slug: "inbound-api",
    title: "HTTP API",
    group: "Building on it",
  },
  {
    src: "docs/protocol.md",
    slug: "protocol",
    title: "Protocol",
    group: "Building on it",
  },
  { src: "PRIVACY.md", slug: "privacy", title: "Privacy", group: "Project" },
  { src: "SECURITY.md", slug: "security", title: "Security", group: "Project" },
  {
    src: "CONTRIBUTING.md",
    slug: "contributing",
    title: "Contributing",
    group: "Project",
  },
  {
    src: "CHANGELOG.md",
    slug: "changelog",
    title: "Changelog",
    group: "Project",
  },
];

const pageFor = new Map(pages.map((p) => [p.src, p]));
const read = (p: string) => readFileSync(join(root, p), "utf8");
const esc = (s: string) =>
  s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
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
  if (page)
    return (
      relative(here, `docs/${page.slug}/`).replace(/\\/g, "/") +
      "/" +
      (hash ? `#${hash}` : "")
    );
  if (target.startsWith("docs/images/"))
    return relative(here, target.replace("docs/", "")).replace(/\\/g, "/");
  if (target === "README.md") return relative(here, "") || "./";
  return `${repo}/blob/main/${target}${hash ? `#${hash}` : ""}`;
}

/** The page as HTML, plus its h2 and h3 headings for "On this page". */
function render(
  src: string,
  here: string,
): { html: string; heads: Array<{ id: string; text: string; depth: number }> } {
  const md = new Marked({ gfm: true });
  const heads: Array<{ id: string; text: string; depth: number }> = [];
  md.use({
    renderer: {
      heading({ tokens, depth }) {
        const html = this.parser.parseInline(tokens);
        const id = slugify(html);
        if (depth === 2 || depth === 3)
          heads.push({ id, text: html.replace(/<[^>]+>/g, ""), depth });
        return `<h${depth} id="${id}">${html}</h${depth}>\n`;
      },
      code({ text, lang }) {
        const cls = lang ? ` class="language-${esc(lang)}"` : "";
        return `<div class="code"><pre><code${cls}>${esc(text)}</code></pre></div>\n`;
      },
    },
    walkTokens(token) {
      if (token.type === "link" || token.type === "image")
        token.href = link(token.href, src, here);
    },
  });
  // The first heading becomes the page title.
  const html = (md.parse(read(src).replace(/^# .*\n/, "")) as string)
    .replace(/<table>/g, '<div class="table"><table>')
    .replace(/<\/table>/g, "</table></div>");
  return { html, heads };
}

function layout(file: string, vars: Record<string, string>): string {
  return readFileSync(join(site, file), "utf8").replace(
    /\{\{(\w+)\}\}/g,
    (_, k) => vars[k] ?? "",
  );
}

rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });

const base = (here: string) => (relative(here, "") || ".").replace(/\\/g, "/");
const groups = [...new Set(pages.map((p) => p.group))];
pages.forEach((p, i) => {
  const here = `docs/${p.slug}`;
  const b = base(here);
  const href = (q: (typeof pages)[number]) => `${b}/docs/${q.slug}/`;
  const nav = groups
    .map(
      (g) =>
        `<h4>${esc(g)}</h4>\n<ul>\n${pages
          .filter((q) => q.group === g)
          .map(
            (q) =>
              `<li><a href="${href(q)}"${q.slug === p.slug ? ' aria-current="page"' : ""}>${esc(q.title)}</a></li>`,
          )
          .join("\n")}\n</ul>`,
    )
    .join("\n");
  const picker = pages
    .map(
      (q) =>
        `<option value="${href(q)}"${q.slug === p.slug ? " selected" : ""}>${esc(q.title)}</option>`,
    )
    .join("\n");
  const { html: content, heads } = render(p.src, here);
  const toc = heads.length
    ? `<h4>On this page</h4>\n<ul>\n${heads.map((h) => `<li><a href="#${h.id}"${h.depth === 3 ? ' class="subhead"' : ""}>${esc(h.text)}</a></li>`).join("\n")}\n</ul>`
    : "";
  const prev = pages[i - 1];
  const next = pages[i + 1];
  const pager = [
    prev &&
      `<a class="prev" href="${href(prev)}"><small>Previous</small>${esc(prev.title)}</a>`,
    next &&
      `<a class="next" href="${href(next)}"><small>Next</small>${esc(next.title)}</a>`,
  ]
    .filter(Boolean)
    .join("");
  const html = layout("doc.html", {
    title: esc(p.title),
    base: b,
    repo,
    nav,
    picker,
    toc,
    pager,
    content,
    source: `${repo}/blob/main/${p.src}`,
  });
  mkdirSync(join(out, here), { recursive: true });
  writeFileSync(join(out, here, "index.html"), html);
});

writeFileSync(join(out, "index.html"), layout("index.html", { repo }));
writeFileSync(join(out, "404.html"), layout("404.html", {}));
for (const f of ["site.css", "deck.js", "docs.js", "CNAME"])
  copyFileSync(join(site, f), join(out, f));

const assets: Array<[string, string]> = [
  ["apps/client/public/icon.svg", "icon.svg"],
  ...[
    "phone",
    "editor",
    "dashboard",
    "keys",
    "streamdeck",
    "triggers",
    "review",
    "search",
    "wizard",
    "pages",
    "overlay",
  ].map((n): [string, string] => [`docs/images/${n}.png`, `images/${n}.png`]),
];
for (const [from, to] of assets) {
  mkdirSync(dirname(join(out, to)), { recursive: true });
  copyFileSync(join(root, from), join(out, to));
}
// The plugin registry, so it can also be fetched from the site.
mkdirSync(join(out, "registry"), { recursive: true });
copyFileSync(
  join(root, "registry/index.json"),
  join(out, "registry/index.json"),
);

console.log(`site built in ${relative(root, out)}`);

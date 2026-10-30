// Light markup for plugin hints: [text](https://…) is a link, `text` is something to copy.

export type Part = { text: string } | { text: string; href: string } | { code: string };

const MARK = /\[([^\]\n]{1,100})\]\((https:\/\/[^\s)]{1,500})\)|`([^`\n]{1,300})`/g;

export function parts(s: string): Part[] {
  const out: Part[] = [];
  let last = 0;
  for (const m of s.matchAll(MARK)) {
    if (m.index > last) out.push({ text: s.slice(last, m.index) });
    out.push(m[3] !== undefined ? { code: m[3] } : { text: m[1], href: m[2] });
    last = m.index + m[0].length;
  }
  if (last < s.length) out.push({ text: s.slice(last) });
  return out;
}

/** The same text without markup, for tooltips. */
export const plain = (s: string) => s.replace(MARK, (_, text, _href, code) => code ?? text);

export async function copyText(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    // Older webviews: fall back to a hidden text box.
    const box = document.createElement("textarea");
    box.value = text;
    box.style.cssText = "position:fixed;opacity:0";
    document.body.append(box);
    box.select();
    const ok = document.execCommand("copy");
    box.remove();
    return ok;
  }
}

<script lang="ts">
  import Icon from "../components/Icon.svelte";
  import { parts, copyText } from "../lib/rich.ts";
  import { req } from "../lib/deck.svelte.ts";
  import { t } from "../lib/i18n.svelte.ts";

  let { text }: { text: string } = $props();

  let copied = $state<string | null>(null);
  let timer: ReturnType<typeof setTimeout> | undefined;

  async function copy(e: MouseEvent, code: string) {
    // Inside a <label>, a click would also focus the field.
    e.preventDefault();
    if (!(await copyText(code))) return;
    copied = code;
    clearTimeout(timer);
    timer = setTimeout(() => (copied = null), 1500);
  }

  function open(e: MouseEvent, href: string) {
    // The desktop window can't open tabs, so the companion opens the browser.
    if (!(window as { __KEYSIDE_DESKTOP__?: boolean }).__KEYSIDE_DESKTOP__) return;
    e.preventDefault();
    void req("open.link", { url: href }).catch(() => {});
  }
</script>

{#each parts(text) as p, i (i)}{#if "code" in p}<button type="button" class="code" title={t("Copy")} onclick={(e) => copy(e, p.code)}><span class="mono">{p.code}</span><Icon name={copied === p.code ? "check" : "copy"} size={14} /></button>{:else if "href" in p}<a href={p.href} target="_blank" rel="noreferrer" onclick={(e) => open(e, p.href)}>{p.text}<Icon name="external-link" size={13} /></a>{:else}{p.text}{/if}{/each}<span class="sr-only" aria-live="polite">{copied ? t("Copied") : ""}</span>

<style>
  .code {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    max-width: 100%;
    margin: 2px 0;
    padding: 1px 8px;
    border: 1px solid var(--line);
    border-radius: 6px;
    background: var(--raised);
    color: var(--text);
    font: inherit;
    vertical-align: baseline;
    cursor: copy;
  }
  .code .mono {
    overflow-wrap: anywhere;
    text-align: left;
    user-select: all;
  }
  .code:hover {
    border-color: var(--accent);
  }
  a {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    color: var(--accent-text);
    text-decoration: underline;
    text-underline-offset: 2px;
  }
</style>

<script lang="ts">
  // The last pushed layouts. Restoring one puts it in the draft; pushing makes it live again.
  import type { LayoutFile } from "@keyside/protocol";
  import Icon from "../components/Icon.svelte";
  import { ed, change } from "./state.svelte.ts";
  import { diffLayouts, isSame, type LayoutDiff } from "./history.ts";
  import { req } from "../lib/deck.svelte.ts";
  import { t } from "../lib/i18n.svelte.ts";
  import { ago } from "../lib/format.ts";

  interface Entry {
    rev: number;
    at: number;
    by: string;
    device: string;
    layouts: number;
    pages: number;
    keys: number;
  }

  let { onrestored }: { onrestored?: () => void } = $props();

  let list = $state<Entry[] | null>(null);
  let picked = $state<number | null>(null);
  let version = $state<LayoutFile | null>(null);
  let diff = $state<LayoutDiff | null>(null);
  let error = $state("");
  const now = Date.now();

  $effect(() => {
    req<Entry[]>("history.list").then(
      (l) => (list = l),
      (e) => (error = e.message),
    );
  });

  async function pick(rev: number) {
    if (picked === rev) {
      picked = null;
      return;
    }
    picked = rev;
    version = null;
    diff = null;
    error = "";
    try {
      const l = await req<LayoutFile>("history.get", { rev });
      if (picked !== rev) return;
      version = l;
      if (ed.layout) diff = diffLayouts($state.snapshot(ed.layout) as LayoutFile, l);
    } catch (e: any) {
      error = e.message;
    }
  }

  function restore() {
    const l = version;
    if (!l || !ed.layout) return;
    change((d) => {
      d.profiles = $state.snapshot(l.profiles) as LayoutFile["profiles"];
      d.defaultProfile = l.defaultProfile;
    });
    if (!l.profiles.some((p) => p.id === ed.profile)) ed.profile = l.defaultProfile;
    const prof = l.profiles.find((p) => p.id === ed.profile);
    if (!prof?.pages.some((p) => p.id === ed.page)) ed.page = prof?.homePage ?? "";
    ed.key = null;
    onrestored?.();
  }

  const when = (e: Entry) => (e.at ? `${new Date(e.at).toLocaleString([], { dateStyle: "medium", timeStyle: "short" })} · ${t("{time} ago", { time: ago(e.at, now) })}` : t("Before the first push"));
  const names = (l: string[]) => (l.length > 3 ? `${l.slice(0, 3).join(", ")} +${l.length - 3}` : l.join(", "));
</script>

{#if error}<p class="bad" role="alert">{error}</p>{/if}
<p class="muted">{t("The last 20 pushes are kept on this PC. Restoring one loads it into your draft; push it to make it live.")}</p>

{#if list && !list.length}
  <p class="muted">{t("Nothing pushed yet.")}</p>
{/if}
{#each list ?? [] as e (e.rev)}
  <div class="entry" class:open={picked === e.rev}>
    <button class="head" onclick={() => pick(e.rev)} aria-expanded={picked === e.rev}>
      <Icon name="history" size={18} />
      <span class="what">
        <b>{when(e)}</b>
        <span class="muted">{#if e.by}{t("by {name}", { name: e.by })}{" · "}{/if}{t("Layouts: {layouts} · Pages: {pages} · Keys: {keys}", { layouts: e.layouts, pages: e.pages, keys: e.keys })}</span>
      </span>
      {#if e.rev === ed.baseRev}<span class="tag">{t("Live now")}</span>{/if}
    </button>
    {#if picked === e.rev}
      <div class="body">
        {#if !diff}
          <p class="muted">{t("Loading…")}</p>
        {:else if isSame(diff)}
          <p class="muted">{t("Same as your draft.")}</p>
        {:else}
          <p>{t("Restoring this version changes your draft:")}</p>
          <ul>
            {#if diff.layoutsAdded.length}<li>{t("Brings back layouts: {names}", { names: names(diff.layoutsAdded) })}</li>{/if}
            {#if diff.layoutsRemoved.length}<li>{t("Removes layouts: {names}", { names: names(diff.layoutsRemoved) })}</li>{/if}
            {#if diff.pagesAdded.length}<li>{t("Brings back pages: {names}", { names: names(diff.pagesAdded) })}</li>{/if}
            {#if diff.pagesRemoved.length}<li>{t("Removes pages: {names}", { names: names(diff.pagesRemoved) })}</li>{/if}
            {#if diff.renamed}<li>{t("Names or grids changed: {n}", { n: diff.renamed })}</li>{/if}
            {#if diff.keysChanged || diff.keysAdded || diff.keysRemoved}
              <li>{t("Keys: {changed} changed, {added} added, {removed} removed", { changed: diff.keysChanged, added: diff.keysAdded, removed: diff.keysRemoved })}</li>
            {/if}
          </ul>
        {/if}
        <div class="row">
          <button class="btn primary" disabled={!version || (!!diff && isSame(diff))} onclick={restore}><Icon name="undo-2" size={18} />{t("Restore")}</button>
        </div>
      </div>
    {/if}
  </div>
{/each}

<style>
  .entry {
    border-radius: var(--radius);
    background: var(--bg);
    border: 1px solid var(--line-soft);
  }
  .entry.open {
    border-color: var(--line);
  }
  .head {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    min-height: 56px;
    padding: 8px 16px;
    border: none;
    background: none;
    text-align: left;
    color: inherit;
  }
  .what {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .tag {
    font-size: 0.75rem;
    padding: 2px 8px;
    border-radius: 6px;
    background: var(--accent-bg);
    color: var(--accent-text);
  }
  .body {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 0 16px 16px;
  }
  ul {
    margin: 0;
    padding-left: 1.2em;
  }
  p {
    margin: 0;
  }
  .row {
    display: flex;
    gap: 8px;
  }
  .muted {
    color: var(--muted);
  }
  .bad {
    color: var(--danger-text);
  }
</style>

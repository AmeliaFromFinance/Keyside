<script lang="ts">
  // Every page of the open layout: open, rename, reorder, copy and delete them.
  import type { Page } from "@keyside/protocol";
  import Icon from "../components/Icon.svelte";
  import { ed, change, profile, addPage, deletePage, duplicatePage, movePage, clearSelection } from "./state.svelte.ts";
  import { t } from "../lib/i18n.svelte.ts";

  let { onopen }: { onopen: () => void } = $props();
  let deleting = $state<string | null>(null);

  const pr = $derived(profile());
  const pages = $derived(pr?.pages.filter((p) => !p.folder) ?? []);
  const folders = $derived(pr?.pages.filter((p) => p.folder) ?? []);
  const fixed = $derived(!!pr?.deck);

  /** The folder keys that open a folder page, as "Page › Key". */
  function openedFrom(id: string): string {
    const from = (pr?.pages ?? []).flatMap((p) => p.keys.filter((k) => k.kind === "folder" && k.target === id).map((k) => `${p.name} › ${k.label || t("Untitled key")}`));
    return from.length ? from.join(", ") : t("No folder key opens it");
  }
  function open(id: string) {
    ed.page = id;
    clearSelection();
    onopen();
  }
  function rename(p: Page, name: string) {
    change(() => (p.name = name));
  }
  function add() {
    const id = addPage();
    if (id) open(id);
  }
  function copy(id: string) {
    const made = duplicatePage(id);
    if (made) ed.page = made;
  }
  function remove(id: string) {
    deleting = null;
    deletePage(id);
  }
</script>

{#snippet map(p: Page)}
  <div class="map" style:--cols={p.cols} style:--rows={p.rows} aria-hidden="true">
    {#each p.keys as k (k.id)}
      <span class="cell" style:background={k.color ? `var(--key-${k.color})` : undefined} style:grid-column="{k.x + 1} / span {k.w ?? 1}" style:grid-row="{k.y + 1} / span {k.h ?? 1}"></span>
    {/each}
  </div>
{/snippet}

{#snippet row(p: Page, i: number, total: number)}
  <li class="page" class:on={p.id === ed.page}>
    <button class="peek" onclick={() => open(p.id)} aria-label={t("Open {name}", { name: p.name })} title={t("Open {name}", { name: p.name })}>{@render map(p)}</button>
    <div class="info">
      <input class="name" value={p.name} maxlength="40" aria-label={t("Page name")} oninput={(e) => rename(p, e.currentTarget.value)} />
      <span class="meta">
        {#if p.folder}{openedFrom(p.id)}{:else}{t("{n} keys", { n: p.keys.length })} · {p.cols} × {p.rows}{/if}
        {#if !p.folder && pr?.homePage === p.id}<span class="home"><Icon name="house" size={14} />{t("Opens first")}</span>{/if}
      </span>
    </div>
    {#if deleting === p.id}
      <div class="confirm" role="group">
        <span>{p.keys.length ? t("Delete it and its {n} keys?", { n: p.keys.length }) : t("Delete it?")}</span>
        <button class="btn small danger" onclick={() => remove(p.id)}>{t("Delete")}</button>
        <button class="btn small" onclick={() => (deleting = null)}>{t("Cancel")}</button>
      </div>
    {:else}
      <div class="tools">
        {#if !p.folder}
          <button class="btn icon ghost" aria-label={t("Move up")} title={t("Move up")} disabled={i === 0} onclick={() => movePage(p.id, -1)}><Icon name="arrow-up" size={18} /></button>
          <button class="btn icon ghost" aria-label={t("Move down")} title={t("Move down")} disabled={i === total - 1} onclick={() => movePage(p.id, 1)}><Icon name="arrow-down" size={18} /></button>
          <button class="btn icon ghost" aria-label={t("Duplicate")} title={t("Duplicate")} onclick={() => copy(p.id)}><Icon name="copy" size={18} /></button>
        {/if}
        <button class="btn icon ghost danger" aria-label={t("Delete page")} title={t("Delete page")} disabled={!p.folder && total <= 1} onclick={() => (deleting = p.id)}><Icon name="trash-2" size={18} /></button>
      </div>
    {/if}
  </li>
{/snippet}

<div class="pages">
  <p class="muted">{t("Phones swipe through pages in this order. Click a page to open it.")}</p>
  <ol>
    {#each pages as p, i (p.id)}{@render row(p, i, pages.length)}{/each}
  </ol>
  {#if !fixed}
    <button class="btn add" onclick={add}><Icon name="plus" size={18} />{t("Add page")}</button>
  {/if}
  {#if folders.length}
    <h3>{t("Folders")}</h3>
    <ol>
      {#each folders as p, i (p.id)}{@render row(p, i, folders.length)}{/each}
    </ol>
  {/if}
</div>

<style>
  .pages {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .muted {
    margin: 0;
    color: var(--muted);
  }
  h3 {
    margin: 8px 0 0;
    font-size: 0.95rem;
    color: var(--muted);
  }
  ol {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .page {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 10px 12px;
    border-radius: var(--radius);
    border: 1px solid transparent;
    background: var(--bg);
  }
  .page.on {
    border-color: var(--accent);
  }
  .peek {
    flex-shrink: 0;
    width: 64px;
    height: 64px;
    padding: 6px;
    border: 1px solid var(--line);
    border-radius: 10px;
    background: var(--surface);
    cursor: pointer;
  }
  .peek:hover {
    border-color: var(--accent);
  }
  .map {
    display: grid;
    grid-template-columns: repeat(var(--cols), 1fr);
    grid-template-rows: repeat(var(--rows), 1fr);
    gap: 2px;
    width: 100%;
    height: 100%;
  }
  .cell {
    border-radius: 2px;
    background: var(--muted);
    opacity: 0.8;
  }
  .info {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .name {
    width: 100%;
    min-height: 36px;
    padding: 0 8px;
    margin-left: -8px;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    background: none;
    color: var(--text);
    font: inherit;
    font-weight: 600;
  }
  .name:hover,
  .name:focus {
    border-color: var(--line);
    background: var(--surface);
    outline: none;
  }
  .meta {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 10px;
    color: var(--muted);
    font-size: 0.85rem;
  }
  .home {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--accent);
  }
  .tools {
    display: flex;
    gap: 2px;
  }
  .danger:not(:disabled) {
    color: var(--danger);
  }
  .confirm {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
  }
  .add {
    align-self: flex-start;
  }
  @media (max-width: 560px) {
    .page {
      flex-wrap: wrap;
    }
    .tools,
    .confirm {
      width: 100%;
      justify-content: flex-end;
    }
  }
</style>

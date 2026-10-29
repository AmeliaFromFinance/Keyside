<script lang="ts">
  import Icon from "../components/Icon.svelte";
  import { ed, allActions, type ActionDef } from "./state.svelte.ts";
  import { t, mark } from "../lib/i18n.svelte.ts";
  import { actionMatches, searchWords } from "./search.ts";

  let { admin = false }: { admin?: boolean } = $props();
  let q = $state("");
  let showAll = $state<Record<string, boolean>>({});

  // Folded groups are remembered on this device.
  const foldKey = "palette.collapsed";
  let collapsed = $state<Set<string>>(loadFolded());

  function loadFolded(): Set<string> {
    try {
      const v = JSON.parse(localStorage.getItem(foldKey) ?? "[]");
      return new Set(Array.isArray(v) ? v.filter((g) => typeof g === "string") : []);
    } catch {
      return new Set();
    }
  }

  function setFolded(next: Set<string>) {
    collapsed = next;
    try {
      localStorage.setItem(foldKey, JSON.stringify([...next]));
    } catch {
      /* storage blocked: folding still works until the page reloads */
    }
  }

  function toggle(group: string) {
    const next = new Set(collapsed);
    if (next.has(group)) next.delete(group);
    else next.add(group);
    setFolded(next);
  }

  const layoutItems: ActionDef[] = [
    { id: "kind:folder", name: mark("Folder"), group: mark("Layout"), icon: "folder", description: mark("sub-page") },
    { id: "kind:slider", name: mark("Slider"), group: mark("Layout"), icon: "sliders-vertical", description: mark("value") },
    { id: "kind:widget", name: mark("Text or chat"), group: mark("Layout"), icon: "message-square", description: mark("live text") },
    { id: "kind:button", name: mark("Multi-action"), group: mark("Layout"), icon: "layers", description: mark("steps") },
  ];

  const words = $derived(searchWords(q));

  const groups = $derived.by(() => {
    const items = [...allActions().filter((a) => admin || !a.adminOnly), ...layoutItems];
    const map = new Map<string, ActionDef[]>();
    for (const a of items) {
      const g = a.group ?? mark("Other");
      if (!actionMatches({ ...a, group: g }, words)) continue;
      if (!map.has(g)) map.set(g, []);
      map.get(g)!.push(a);
    }
    return [...map.entries()];
  });

  const allFolded = $derived(groups.length > 0 && groups.every(([g]) => collapsed.has(g)));

  function drag(e: DragEvent, id: string) {
    e.dataTransfer?.setData("application/x-keyside-action", id);
    e.dataTransfer!.effectAllowed = "copy";
    ed.armed = id;
  }
</script>

<aside class="palette">
  <div class="head">
    <h2>{t("Button palette")}</h2>
    {#if !words.length && groups.length > 1}
      <button class="fold-all" onclick={() => setFolded(allFolded ? new Set() : new Set(groups.map(([g]) => g)))}>
        {allFolded ? t("Expand all") : t("Collapse all")}
      </button>
    {/if}
  </div>
  <label class="search">
    <Icon name="search" size={18} />
    <input type="search" bind:value={q} placeholder={t("Search plugins and actions")} aria-label={t("Search plugins and actions")} onkeydown={(e) => e.key === "Escape" && (q = "")} />
  </label>
  <p class="tip">{t("Drag an action onto an empty cell, or tap it and then tap a cell.")}</p>
  {#each groups as [group, items] (group)}
    {@const open = words.length > 0 || !collapsed.has(group)}
    {@const limit = words.length || showAll[group] ? items.length : 6}
    <section>
      <h3>
        <button class="group" aria-expanded={open} disabled={words.length > 0} onclick={() => toggle(group)}>
          <span class="chev" class:open><Icon name="chevron-right" size={16} /></span>
          <span class="title">{t(group)}</span>
          <span class="count">{items.length}</span>
        </button>
      </h3>
      {#if open}
        {#each items.slice(0, limit) as a (a.id)}
          <button
            class="item"
            class:armed={ed.armed === a.id}
            draggable="true"
            ondragstart={(e) => drag(e, a.id)}
            ondragend={() => (ed.armed = null)}
            onclick={() => (ed.armed = ed.armed === a.id ? null : a.id)}
            title={a.description ? t(a.description) : undefined}
          >
            <Icon name={a.icon ?? "bolt"} size={20} />
            <span class="name">{t(a.name)}</span>
            <span class="hint">{ed.armed === a.id ? t("tap a cell") : t(a.description ?? "")}</span>
          </button>
        {/each}
        {#if items.length > limit}
          <button class="more" onclick={() => (showAll[group] = true)}>{t("Show all {n} actions", { n: items.length })}</button>
        {/if}
      {/if}
    </section>
  {:else}
    <p class="none">{t("Nothing matches “{q}”.", { q: q.trim() })}</p>
  {/each}
</aside>

<style>
  .palette {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 20px 16px;
    overflow: auto;
    min-height: 0;
  }
  .head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 8px;
  }
  .fold-all {
    border: none;
    background: none;
    color: var(--muted);
    font-size: 0.8rem;
    padding: 4px 8px;
    border-radius: var(--radius-sm);
  }
  .fold-all:hover {
    color: var(--text);
    background: var(--hover);
  }
  .none {
    margin: 12px 8px;
    color: var(--muted);
  }
  h2 {
    margin: 0 8px 4px;
    font-size: 0.85rem;
    font-weight: 600;
    color: var(--muted);
    letter-spacing: 0.04em;
  }
  .search {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 12px;
    min-height: 44px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--line);
    background: var(--bg);
    color: var(--muted);
  }
  .search input {
    flex: 1;
    border: none;
    background: none;
    outline: none;
    min-width: 0;
    color: var(--text);
  }
  .tip {
    margin: 0 8px;
    color: var(--dim);
    font-size: 0.8rem;
  }
  section {
    display: flex;
    flex-direction: column;
    margin-top: 4px;
  }
  h3 {
    margin: 0 0 2px;
    font-size: 0.9rem;
    font-weight: 600;
  }
  .group {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    min-height: 36px;
    padding: 0 8px 0 4px;
    border: none;
    border-radius: var(--radius-sm);
    background: none;
    color: var(--muted);
    font: inherit;
    text-align: left;
  }
  .group:hover:not(:disabled) {
    background: var(--hover);
    color: var(--text);
  }
  .group:disabled {
    cursor: default;
    opacity: 1;
  }
  .chev {
    display: flex;
    transition: transform 120ms ease;
  }
  .chev.open {
    transform: rotate(90deg);
  }
  .title {
    flex: 1;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  @media (prefers-reduced-motion: reduce) {
    .chev {
      transition: none;
    }
  }
  .count {
    font-family: var(--mono);
    font-weight: 400;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 44px;
    padding: 0 8px;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    background: none;
    text-align: left;
    cursor: grab;
  }
  .item:hover {
    background: var(--hover);
  }
  .item.armed {
    background: var(--accent-bg);
    border: 1px dashed var(--accent);
  }
  .name {
    flex: 1;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .hint {
    color: var(--muted);
    font-size: 0.85rem;
    white-space: nowrap;
  }
  .armed .hint {
    color: var(--accent);
  }
  .more {
    border: none;
    background: none;
    color: var(--accent);
    text-decoration: underline;
    text-align: left;
    padding: 6px 8px;
  }
</style>

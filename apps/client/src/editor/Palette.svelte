<script lang="ts">
  import Icon from "../components/Icon.svelte";
  import { ed, allActions, type ActionDef } from "./state.svelte.ts";
  import { t } from "../lib/i18n.svelte.ts";

  let { admin = false }: { admin?: boolean } = $props();
  let q = $state("");
  let showAll = $state<Record<string, boolean>>({});

  const layoutItems: ActionDef[] = [
    { id: "kind:folder", name: "Folder", group: "Layout", icon: "folder", description: "sub-page" },
    { id: "kind:slider", name: "Slider", group: "Layout", icon: "sliders-vertical", description: "value" },
    { id: "kind:widget", name: "Text or chat", group: "Layout", icon: "message-square", description: "live text" },
    { id: "kind:button", name: "Multi-action", group: "Layout", icon: "layers", description: "steps" },
  ];

  const groups = $derived.by(() => {
    const needle = q.trim().toLowerCase();
    const items = [...allActions().filter((a) => admin || !a.adminOnly), ...layoutItems];
    const map = new Map<string, ActionDef[]>();
    for (const a of items) {
      if (needle && !`${a.name} ${a.group} ${a.description ?? ""}`.toLowerCase().includes(needle)) continue;
      const g = a.group ?? "Other";
      if (!map.has(g)) map.set(g, []);
      map.get(g)!.push(a);
    }
    return [...map.entries()];
  });

  function drag(e: DragEvent, id: string) {
    e.dataTransfer?.setData("application/x-phonedeck-action", id);
    e.dataTransfer!.effectAllowed = "copy";
    ed.armed = id;
  }
</script>

<aside class="palette">
  <h2>{t("Button palette")}</h2>
  <label class="search">
    <Icon name="search" size={18} />
    <input bind:value={q} placeholder={t("Search actions")} aria-label={t("Search actions")} />
  </label>
  <p class="tip">{t("Drag an action onto an empty cell, or tap it and then tap a cell.")}</p>
  {#each groups as [group, items] (group)}
    {@const limit = q || showAll[group] ? items.length : 6}
    <section>
      <h3><span>{t(group)}</span><span class="count">{items.length}</span></h3>
      {#each items.slice(0, limit) as a (a.id)}
        <button
          class="item"
          class:armed={ed.armed === a.id}
          draggable="true"
          ondragstart={(e) => drag(e, a.id)}
          ondragend={() => (ed.armed = null)}
          onclick={() => (ed.armed = ed.armed === a.id ? null : a.id)}
          title={a.description}
        >
          <Icon name={a.icon ?? "bolt"} size={20} />
          <span class="name">{t(a.name)}</span>
          <span class="hint">{ed.armed === a.id ? t("tap a cell") : t(a.description ?? "")}</span>
        </button>
      {/each}
      {#if items.length > limit}
        <button class="more" onclick={() => (showAll[group] = true)}>{t("Show all {n} actions", { n: items.length })}</button>
      {/if}
    </section>
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
    margin-top: 10px;
  }
  h3 {
    display: flex;
    justify-content: space-between;
    margin: 0 8px 4px;
    font-size: 0.9rem;
    font-weight: 600;
    color: var(--muted);
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

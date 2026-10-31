<script lang="ts">
  import Icon from "../components/Icon.svelte";
  import KeyIcon from "./KeyIcon.svelte";
  import { ed, allActions, templates, notify, type ActionDef } from "./state.svelte.ts";
  import { req } from "../lib/deck.svelte.ts";
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
    const items = [...allActions().filter((a) => (admin || !a.adminOnly) && !a.stepOnly), ...layoutItems];
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

  // Saved templates, placed like actions. Their palette id is "tpl:<id>".
  const TEMPLATES = "\u0000templates";
  const shownTemplates = $derived(templates.list.filter((tp) => words.every((w) => tp.name.toLowerCase().includes(w))));
  const templatesOpen = $derived(words.length > 0 || !collapsed.has(TEMPLATES));
  let renaming = $state<{ id: string; name: string } | null>(null);
  let deleting = $state<string | null>(null);

  async function rename() {
    const r = renaming;
    renaming = null;
    const tp = templates.list.find((x) => x.id === r?.id);
    if (!r || !tp || !r.name.trim() || r.name.trim() === tp.name) return;
    try {
      await req("templates.rename", { id: r.id, name: r.name });
      tp.name = r.name.trim();
    } catch (e: any) {
      notify(e.message);
    }
  }

  async function remove(id: string) {
    deleting = null;
    try {
      await req("templates.delete", { id });
      templates.list = templates.list.filter((x) => x.id !== id);
      if (ed.armed === `tpl:${id}`) ed.armed = null;
    } catch (e: any) {
      notify(e.message);
    }
  }

  function focus(el: HTMLInputElement) {
    el.focus();
    el.select();
  }

  let from: { id: string; x: number; y: number; at: number; was: string | null } | null = null;

  function drag(e: DragEvent, id: string) {
    from = { id, x: e.clientX, y: e.clientY, at: performance.now(), was: ed.armed };
    e.dataTransfer?.setData("application/x-keyside-action", id);
    e.dataTransfer!.effectAllowed = "copy";
    ed.armed = id;
  }

  /** A click that wobbles a few pixels starts a drag, and the browser drops the click. Count a short drag that landed nowhere as that click. */
  function dragEnd(e: DragEvent, id: string) {
    const f = from;
    from = null;
    const near = e.clientX || e.clientY ? Math.hypot(e.clientX - (f?.x ?? 0), e.clientY - (f?.y ?? 0)) < 16 : false;
    const quick = f ? performance.now() - f.at < 400 : false;
    if (f?.id === id && e.dataTransfer?.dropEffect === "none" && (near || quick)) ed.armed = f.was === id ? null : id;
    else ed.armed = null;
  }
</script>

<aside class="palette">
  <div class="head">
    <h2>{t("Button palette")}</h2>
    {#if !words.length && groups.length > 1}
      <button class="fold-all" onclick={() => setFolded(allFolded ? new Set() : new Set([...groups.map(([g]) => g), TEMPLATES]))}>
        {allFolded ? t("Expand all") : t("Collapse all")}
      </button>
    {/if}
  </div>
  <label class="search">
    <Icon name="search" size={18} />
    <input type="search" bind:value={q} placeholder={t("Search plugins and actions")} aria-label={t("Search plugins and actions")} onkeydown={(e) => e.key === "Escape" && (q = "")} />
  </label>
  <p class="tip">{t("Drag an action onto an empty cell, or tap it and then tap a cell.")}</p>
  {#if templates.list.length && (shownTemplates.length || !words.length)}
    <section class="templates">
      <h3>
        <button class="group" aria-expanded={templatesOpen} disabled={words.length > 0} onclick={() => toggle(TEMPLATES)}>
          <span class="chev" class:open={templatesOpen}><Icon name="chevron-right" size={16} /></span>
          <span class="title">{t("Templates")}</span>
          <span class="count">{templates.list.length}</span>
        </button>
      </h3>
      {#if templatesOpen}
        {#each shownTemplates as tp (tp.id)}
          {@const id = `tpl:${tp.id}`}
          {#if renaming?.id === tp.id}
            <form class="rename" onsubmit={(e) => (e.preventDefault(), rename())}>
              <input class="input" maxlength="40" bind:value={renaming.name} aria-label={t("Template name")} use:focus onkeydown={(e) => e.key === "Escape" && (e.stopPropagation(), (renaming = null))} onblur={rename} />
            </form>
          {:else if deleting === tp.id}
            <div class="confirm">
              <span>{t("Delete {name}?", { name: tp.name })}</span>
              <button class="tool danger" onclick={() => remove(tp.id)}>{t("Delete")}</button>
              <button class="tool" onclick={() => (deleting = null)}>{t("Cancel")}</button>
            </div>
          {:else}
            <div class="tpl">
              <button
                class="item"
                class:armed={ed.armed === id}
                draggable="true"
                ondragstart={(e) => drag(e, id)}
                ondragend={(e) => dragEnd(e, id)}
                onclick={() => (ed.armed = ed.armed === id ? null : id)}
                title={tp.keys.map((k) => k.label).join(", ")}
              >
                <KeyIcon icon={tp.keys[0]?.icon} size={20} fallback="bookmark" />
                <span class="name">{tp.name}</span>
                <span class="hint">{ed.armed === id ? t("tap a cell") : tp.keys.length > 1 ? t("{n} keys", { n: tp.keys.length }) : ""}</span>
              </button>
              <button class="tool" title={t("Rename")} aria-label={t("Rename {name}", { name: tp.name })} onclick={() => (renaming = { id: tp.id, name: tp.name })}><Icon name="pencil" size={16} /></button>
              <button class="tool" title={t("Delete")} aria-label={t("Delete {name}", { name: tp.name })} onclick={() => (deleting = tp.id)}><Icon name="trash-2" size={16} /></button>
            </div>
          {/if}
        {/each}
      {/if}
    </section>
  {/if}
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
            ondragend={(e) => dragEnd(e, a.id)}
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
    {#if !shownTemplates.length}<p class="none">{t("Nothing matches “{q}”.", { q: q.trim() })}</p>{/if}
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
  .item > :global(svg) {
    flex-shrink: 0;
  }
  .name {
    flex: 1;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  /* Long descriptions trail off instead of pushing the name out. The full text is in the tooltip. */
  .hint {
    flex: 0 1 auto;
    min-width: 0;
    max-width: 55%;
    color: var(--muted);
    font-size: 0.85rem;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .armed .hint {
    color: var(--accent);
  }
  .tpl {
    display: flex;
    align-items: center;
  }
  .tpl .item {
    flex: 1;
    min-width: 0;
  }
  .tool {
    display: grid;
    place-items: center;
    min-width: 32px;
    min-height: 32px;
    padding: 0 6px;
    border: none;
    border-radius: var(--radius-sm);
    background: none;
    color: var(--muted);
    opacity: 0;
  }
  .tpl:hover .tool,
  .tool:focus-visible,
  .confirm .tool {
    opacity: 1;
  }
  @media (hover: none) {
    .tool {
      opacity: 1;
    }
  }
  .tool:hover {
    color: var(--text);
    background: var(--hover);
  }
  .tool.danger {
    color: var(--danger-text);
  }
  .confirm {
    display: flex;
    align-items: center;
    gap: 4px;
    min-height: 44px;
    padding: 0 8px;
  }
  .confirm span {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .rename {
    padding: 4px 0;
  }
  .rename .input {
    width: 100%;
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

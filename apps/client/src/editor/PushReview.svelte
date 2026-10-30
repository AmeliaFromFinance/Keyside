<script lang="ts">
  import Icon from "../components/Icon.svelte";
  import { ed, lastPushed } from "./state.svelte.ts";
  import { diffLayouts, countChanges, type Item, type KeyPart, type PagePart } from "./changes.ts";
  import { t, mark } from "../lib/i18n.svelte.ts";
  import type { LayoutFile } from "@keyside/protocol";

  let { onpush, oncancel }: { onpush: () => void; oncancel: () => void } = $props();

  // Worked out once when the review opens.
  const changes = diffLayouts(lastPushed(), $state.snapshot(ed.layout) as LayoutFile | null);
  const total = countChanges(changes);

  const partNames: Record<KeyPart, string> = {
    label: mark("label"),
    sub: mark("second line"),
    icon: mark("icon"),
    color: mark("colour"),
    type: mark("type"),
    size: mark("size"),
    steps: mark("actions"),
    feedback: mark("live style"),
    settings: mark("settings"),
  };
  const pageParts: Record<PagePart, string> = {
    name: mark("Page renamed"),
    grid: mark("Grid size changed"),
    arrangement: mark("Arrangement for other screens changed"),
    home: mark("Start page changed"),
  };

  function line(it: Item): string {
    if (it.kind === "added") return t("Added “{name}”", { name: it.label });
    if (it.kind === "removed") return t("Removed “{name}”", { name: it.label });
    if (it.kind === "moved") return it.from !== undefined ? t("Moved “{name}” here from {page}", { name: it.label, page: it.from }) : t("Moved “{name}”", { name: it.label });
    if (it.kind !== "edited") return it.label;
    return t("Changed “{name}”: {what}", { name: it.label, what: it.parts.map((p) => t(partNames[p])).join(", ") });
  }
  const icons = { added: "plus", removed: "minus", moved: "move", edited: "pencil" } as const;
</script>

<div class="review">
  <p class="lead">
    {#if total}
      {t("These changes go to every phone and Stream Deck when you push:")}
    {:else}
      {t("Nothing changed since the last push.")}
    {/if}
  </p>

  <div class="list">
    {#each changes as l (l.id)}
      <section class="layout">
        <h3>
          <Icon name="layout-grid" size={18} />{l.name}
          {#if l.status === "added"}<span class="tag add">{t("New layout")}</span>{/if}
          {#if l.status === "removed"}<span class="tag del">{t("Layout removed")}</span>{/if}
        </h3>
        {#if l.renamed}<p class="row"><Icon name="pencil" size={16} />{t("Renamed from “{name}”", { name: l.renamed })}</p>{/if}
        {#each l.pages as p (p.id)}
          <div class="page">
            <h4>
              <Icon name="file" size={16} />{p.name}
              {#if p.status === "added"}<span class="tag add">{t("New page")}</span>{/if}
              {#if p.status === "removed"}<span class="tag del">{t("Page removed")}</span>{/if}
            </h4>
            {#if p.status !== "changed" && p.keys}
              <p class="row muted">{p.keys === 1 ? t("1 key") : t("{n} keys", { n: p.keys })}</p>
            {/if}
            {#each p.parts as part (part)}
              <p class="row"><Icon name="settings-2" size={16} />{t(pageParts[part])}</p>
            {/each}
            {#each p.items as it, i (i)}
              <p class="row {it.kind}"><Icon name={icons[it.kind]} size={16} />{line(it)}</p>
            {/each}
          </div>
        {/each}
      </section>
    {/each}
  </div>

  {#if ed.error}<p class="bad" role="alert">{ed.error}</p>{/if}
  <div class="buttons">
    <span class="count">{total === 1 ? t("1 change") : total ? t("{n} changes", { n: total }) : ""}</span>
    <button class="btn" onclick={oncancel}>{t("Keep editing")}</button>
    <button class="btn primary" onclick={onpush} disabled={ed.pushing}><Icon name="send" size={18} />{ed.pushing ? t("Pushing…") : t("Push to phones")}</button>
  </div>
</div>

<style>
  .review {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .lead {
    margin: 0;
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 12px;
    max-height: min(56dvh, 560px);
    overflow: auto;
  }
  .layout {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px 14px;
    border-radius: var(--radius-sm);
    background: var(--bg);
    border: 1px solid var(--line-soft);
  }
  h3,
  h4 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
  }
  h3 {
    font-size: 1rem;
  }
  h4 {
    font-size: 0.95rem;
    color: var(--muted);
    font-weight: 600;
  }
  .page {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding-left: 10px;
    border-left: 2px solid var(--line-soft);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
    padding-left: 4px;
  }
  .row :global(svg) {
    flex-shrink: 0;
  }
  .row.added :global(svg) {
    color: var(--success, #3dd68c);
  }
  .row.removed :global(svg) {
    color: var(--danger-text);
  }
  .row.moved :global(svg),
  .row.edited :global(svg) {
    color: var(--accent);
  }
  .muted {
    color: var(--muted);
  }
  .tag {
    padding: 1px 8px;
    border-radius: 10px;
    font-size: 0.75rem;
    font-weight: 600;
  }
  .tag.add {
    background: color-mix(in srgb, var(--accent) 20%, transparent);
    color: var(--accent);
  }
  .tag.del {
    background: var(--danger-bg);
    color: var(--danger-text);
  }
  .bad {
    margin: 0;
    color: var(--danger-text);
  }
  .buttons {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .count {
    flex: 1;
    color: var(--muted);
  }
</style>

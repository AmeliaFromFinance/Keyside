<script lang="ts">
  import Icon from "../components/Icon.svelte";
  import { icons } from "../lib/icons.gen.ts";
  import { req } from "../lib/deck.svelte.ts";
  import { ed, reloadCatalog } from "./state.svelte.ts";
  import { t } from "../lib/i18n.svelte.ts";

  let { value, onpick }: { value?: string; onpick: (icon: string | undefined) => void } = $props();
  let q = $state("");
  let error = $state("");
  const names = Object.keys(icons);
  const shown = $derived(q ? names.filter((n) => n.includes(q.trim().toLowerCase())) : names);
  const images = $derived(ed.catalog.assets.filter((a) => a.kind === "image"));

  async function upload(e: Event) {
    const file = (e.currentTarget as HTMLInputElement).files?.[0];
    if (!file) return;
    error = "";
    if (file.size > 512 * 1024) {
      error = t("Images can be up to 512 KB.");
      return;
    }
    const buf = new Uint8Array(await file.arrayBuffer());
    let bin = "";
    for (const b of buf) bin += String.fromCharCode(b);
    try {
      const meta = await req<{ id: string }>("assets.put", { name: file.name, data: btoa(bin) });
      await reloadCatalog();
      onpick(`asset:${meta.id}`);
    } catch (err: any) {
      error = err.message;
    }
  }
</script>

<div class="picker">
  <div class="row">
    <input class="input" bind:value={q} placeholder={t("Search icons")} aria-label={t("Search icons")} />
    <label class="btn">
      <Icon name="image" size={18} />{t("Upload image")}
      <input type="file" accept="image/png,image/jpeg,image/webp,image/gif,image/svg+xml" class="sr-only" onchange={upload} />
    </label>
    <button class="btn ghost" onclick={() => onpick(undefined)}>{t("No icon")}</button>
  </div>
  {#if error}<p class="bad">{error}</p>{/if}
  {#if images.length && !q}
    <p class="sub">{t("Your images")}</p>
    <div class="grid">
      {#each images as a (a.id)}
        <button class="cell" class:on={value === `asset:${a.id}`} title={a.name} onclick={() => onpick(`asset:${a.id}`)}><Icon name="image" /></button>
      {/each}
    </div>
  {/if}
  <div class="grid">
    {#each shown as n (n)}
      <button class="cell" class:on={value === n} title={n} aria-label={n} onclick={() => onpick(n)}><Icon name={n} /></button>
    {/each}
  </div>
</div>

<style>
  .picker {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .row {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }
  .row .input {
    flex: 1;
    min-width: 160px;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(48px, 1fr));
    gap: 6px;
  }
  .cell {
    aspect-ratio: 1;
    display: grid;
    place-items: center;
    border-radius: var(--radius-sm);
    border: 1px solid var(--line-soft);
    background: var(--bg);
  }
  .cell:hover {
    background: var(--hover);
  }
  .cell.on {
    border-color: var(--accent);
    color: var(--accent);
  }
  .sub {
    margin: 0;
    color: var(--muted);
  }
  .bad {
    margin: 0;
    color: var(--danger-text);
  }
</style>

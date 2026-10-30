<script lang="ts">
  import Icon from "../components/Icon.svelte";
  import KeyIcon from "./KeyIcon.svelte";
  import ImageCrop from "./ImageCrop.svelte";
  import { icons } from "../lib/icons.gen.ts";
  import { req, deck } from "../lib/deck.svelte.ts";
  import { ed, reloadCatalog } from "./state.svelte.ts";
  import { IMAGE_TYPES, isImage } from "./images.svelte.ts";
  import { t } from "../lib/i18n.svelte.ts";

  interface Pack {
    id: string;
    name: string;
    author?: string;
    icons: Array<{ id: string; name: string; tags?: string[] }>;
  }

  let { value, onpick }: { value?: string; onpick: (icon: string | undefined) => void } = $props();
  let q = $state("");
  let error = $state("");
  let info = $state("");
  let tab = $state("icons");
  let packs = $state<Pack[]>([]);
  let limit = $state(120);
  let cropFile = $state<File | null>(null);
  let importing = $state(false);
  let confirmDelete = $state(false);
  const names = Object.keys(icons);
  const images = $derived(ed.catalog.assets.filter((a) => a.kind === "image"));
  const pack = $derived(packs.find((p) => p.id === tab));
  const words = $derived(q.trim().toLowerCase().split(/\s+/).filter(Boolean));
  const has = (text: string) => words.every((w) => text.toLowerCase().includes(w));

  const shownIcons = $derived(names.filter((n) => has(n)));
  const shownImages = $derived(images.filter((a) => has(a.name)));
  const shownPack = $derived(pack ? pack.icons.filter((i) => has(`${i.name} ${(i.tags ?? []).join(" ")}`)) : []);

  $effect(() => {
    void loadPacks();
  });
  $effect(() => {
    void tab;
    void q;
    limit = 120;
    confirmDelete = false;
  });

  async function loadPacks() {
    try {
      packs = await req<Pack[]>("iconpacks.list");
    } catch {
      /* older companion */
    }
  }

  function chooseImage(e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    input.value = "";
    if (!file) return;
    error = "";
    if (!isImage(file)) error = t("Drop a PNG, JPEG, WebP, GIF or SVG image");
    else cropFile = file;
  }

  async function importPack(e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    input.value = "";
    if (!file) return;
    error = "";
    info = "";
    importing = true;
    try {
      const r = await fetch(`/admin/upload/iconpack?name=${encodeURIComponent(file.name)}`, { method: "POST", headers: { "x-keyside": "1" }, body: file });
      if (r.status === 403) throw new Error(t("Icon packs can be imported in the editor on the PC."));
      if (!r.ok) throw new Error((await r.text()) || r.statusText);
      const res = (await r.json()) as { id: string; name: string; icons: number; skipped: number };
      await loadPacks();
      tab = res.id;
      q = "";
      info = res.skipped ? t("Imported {n} icons from {name}. {skipped} files were skipped.", { n: res.icons, name: res.name, skipped: res.skipped }) : t("Imported {n} icons from {name}.", { n: res.icons, name: res.name });
    } catch (err: any) {
      error = err.message;
    } finally {
      importing = false;
    }
  }

  async function deletePack(id: string) {
    error = "";
    try {
      await req("iconpacks.delete", { id });
      await Promise.all([loadPacks(), reloadCatalog()]);
      tab = "icons";
    } catch (err: any) {
      error = err.message;
    }
  }
</script>

<div class="picker">
  {#if cropFile}
    <ImageCrop file={cropFile} oncancel={() => (cropFile = null)} onresult={(icon) => ((cropFile = null), onpick(icon))} />
  {:else}
    <div class="row">
      <input class="input" type="search" bind:value={q} placeholder={t("Search icons")} aria-label={t("Search icons")} />
      <label class="btn">
        <Icon name="image" size={18} />{t("Upload image")}
        <input type="file" accept={IMAGE_TYPES} class="sr-only" onchange={chooseImage} />
      </label>
      {#if deck.self?.admin !== false}
        <label class="btn" class:busy={importing} title={t("Stream Deck icon packs (.streamDeckIconPack) or a zip of PNG and SVG files")}>
          <Icon name="upload" size={18} />{importing ? t("Importing…") : t("Import icon pack")}
          <input type="file" accept=".streamDeckIconPack,.zip,application/zip" class="sr-only" disabled={importing} onchange={importPack} />
        </label>
      {/if}
      <button class="btn ghost" onclick={() => onpick(undefined)}>{t("No icon")}</button>
    </div>
    {#if error}<p class="bad" role="alert">{error}</p>{/if}
    {#if info}<p class="ok" role="status">{info}</p>{/if}

    <div class="tabs" role="tablist" aria-label={t("Icon sets")}>
      <button role="tab" aria-selected={tab === "icons"} class:on={tab === "icons"} onclick={() => (tab = "icons")}>{t("Icons")}</button>
      {#if images.length}
        <button role="tab" aria-selected={tab === "images"} class:on={tab === "images"} onclick={() => (tab = "images")}>{t("Your images")} <span class="n">{images.length}</span></button>
      {/if}
      {#each packs as p (p.id)}
        <button role="tab" aria-selected={tab === p.id} class:on={tab === p.id} onclick={() => (tab = p.id)}>{p.name} <span class="n">{p.icons.length}</span></button>
      {/each}
    </div>

    {#if tab === "images"}
      <div class="grid">
        {#each shownImages.slice(0, limit) as a (a.id)}
          <button class="cell" class:on={value === `asset:${a.id}`} title={a.name} aria-label={a.name} onclick={() => onpick(`asset:${a.id}`)}><KeyIcon icon="asset:{a.id}" size={36} /></button>
        {/each}
      </div>
      {#if shownImages.length > limit}<button class="more" onclick={() => (limit += 240)}>{t("Show more")}</button>{/if}
    {:else if pack}
      <div class="packhead">
        <span>{pack.author ? t("{name} by {author}", { name: pack.name, author: pack.author }) : pack.name}</span>
        {#if deck.self?.admin !== false}
          {#if confirmDelete}
            <span class="ask">{t("Delete this pack? Keys that use its icons keep them.")}</span>
            <button class="btn small danger" onclick={() => deletePack(pack.id)}>{t("Delete")}</button>
            <button class="btn small" onclick={() => (confirmDelete = false)}>{t("Cancel")}</button>
          {:else}
            <button class="btn small ghost" onclick={() => (confirmDelete = true)}><Icon name="trash-2" size={16} />{t("Delete pack")}</button>
          {/if}
        {/if}
      </div>
      <div class="grid">
        {#each shownPack.slice(0, limit) as i (i.id)}
          <button class="cell" class:on={value === `asset:${i.id}`} title={i.name} aria-label={i.name} onclick={() => onpick(`asset:${i.id}`)}><KeyIcon icon="asset:{i.id}" size={36} /></button>
        {:else}
          <p class="sub">{t("Nothing matches “{q}”.", { q: q.trim() })}</p>
        {/each}
      </div>
      {#if shownPack.length > limit}<button class="more" onclick={() => (limit += 240)}>{t("Show more")}</button>{/if}
    {:else}
      <div class="grid">
        {#each shownIcons as n (n)}
          <button class="cell" class:on={value === n} title={n} aria-label={n} onclick={() => onpick(n)}><Icon name={n} /></button>
        {/each}
      </div>
    {/if}
  {/if}
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
  .busy {
    opacity: 0.6;
    pointer-events: none;
  }
  .tabs {
    display: flex;
    gap: 4px;
    flex-wrap: wrap;
    border-bottom: 1px solid var(--line-soft);
    padding-bottom: 8px;
  }
  .tabs button {
    min-height: 34px;
    padding: 0 12px;
    border-radius: var(--radius-sm);
    border: 1px solid transparent;
    background: none;
    color: var(--muted);
    max-width: 220px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .tabs button:hover {
    color: var(--text);
  }
  .tabs button.on {
    color: var(--text);
    background: var(--accent-bg);
    border-color: var(--accent);
  }
  .n {
    font-family: var(--mono);
    font-size: 0.8rem;
    color: var(--muted);
  }
  .packhead {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    color: var(--muted);
  }
  .packhead > span:first-child {
    flex: 1;
  }
  .ask {
    color: var(--text);
  }
  .small {
    min-height: 32px;
    padding: 0 10px;
  }
  .danger {
    color: var(--danger-text);
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
    padding: 4px;
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
    grid-column: 1 / -1;
  }
  .bad {
    margin: 0;
    color: var(--danger-text);
  }
  .ok {
    margin: 0;
    color: var(--success, var(--accent));
  }
  .more {
    border: none;
    background: none;
    color: var(--accent);
    text-decoration: underline;
    align-self: flex-start;
  }
</style>

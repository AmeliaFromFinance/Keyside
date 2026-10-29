<script lang="ts">
  import type { Profile } from "@phonedeck/protocol";
  import Icon from "../components/Icon.svelte";
  import { req } from "../lib/deck.svelte.ts";
  import { ed, change, profile } from "./state.svelte.ts";
  import { t } from "../lib/i18n.svelte.ts";

  let { admin = false, onimported }: { admin?: boolean; onimported: () => void } = $props();

  let code = $state("");
  let incoming = $state("");
  let gallery = $state<Array<{ id: string; name: string; author: string; description: string }> | null>(null);
  let error = $state("");
  let busy = $state(false);

  async function exportCode() {
    error = "";
    try {
      code = (await req<{ code: string }>("share.export", { profile: ed.profile })).code;
    } catch (e: any) {
      error = e.message;
    }
  }

  function add(p: Profile) {
    change((l) => l.profiles.push(p));
    ed.profile = p.id;
    ed.page = p.homePage;
    ed.key = null;
    onimported();
  }

  async function importText(text: string) {
    busy = true;
    error = "";
    try {
      const trimmed = text.trim();
      if (trimmed.startsWith("{")) {
        const parsed = JSON.parse(trimmed);
        const profiles: unknown[] = Array.isArray(parsed.profiles) ? parsed.profiles : [parsed];
        for (const p of profiles) add(await req<Profile>("share.importJson", { profile: p }));
      } else {
        add(await req<Profile>("share.import", { code: trimmed }));
      }
    } catch (e: any) {
      error = e.message;
    } finally {
      busy = false;
    }
  }

  async function file(e: Event) {
    const f = (e.currentTarget as HTMLInputElement).files?.[0];
    if (f && f.size < 4 * 1024 * 1024) await importText(await f.text());
  }

  async function browse() {
    error = "";
    try {
      gallery = (await req<{ layouts: any[] }>("gallery.list")).layouts;
    } catch (e: any) {
      error = e.message;
    }
  }

  async function fromGallery(id: string) {
    busy = true;
    try {
      add(await req<Profile>("gallery.get", { id }));
    } catch (e: any) {
      error = e.message;
    } finally {
      busy = false;
    }
  }
</script>

{#if error}<p class="bad" role="alert">{error}</p>{/if}

<section class="card">
  <h3>{t("Share {name}", { name: profile()?.name ?? "" })}</h3>
  <p class="muted">{t("Anyone can import this code. Steps that control apps or the PC stay off on their side until they approve them.")}</p>
  {#if ed.dirty}<p class="warn">{t("Push your draft first, so the code has your latest changes.")}</p>{/if}
  <button class="btn" onclick={exportCode}><Icon name="share-2" size={18} />{t("Make a share code")}</button>
  {#if code}
    <textarea class="mono" readonly rows="4" value={code}></textarea>
    <button class="btn" onclick={() => navigator.clipboard?.writeText(code)}><Icon name="copy" size={16} />{t("Copy code")}</button>
  {/if}
</section>

<section class="card">
  <h3>{t("Import")}</h3>
  <p class="muted">{t("Paste a share code, or choose a layout file exported from PhoneDeck.")}</p>
  <textarea class="mono" rows="3" bind:value={incoming} placeholder="PD1.…"></textarea>
  <div class="row">
    <button class="btn primary" disabled={!incoming.trim() || busy} onclick={() => importText(incoming)}>{t("Import code")}</button>
    <label class="btn"><Icon name="file-up" size={18} />{t("Choose file")}<input type="file" accept=".json,.txt,application/json,text/plain" class="sr-only" onchange={file} /></label>
  </div>
  {#if !admin}<p class="muted">{t("Steps that start programs can only be imported on the PC.")}</p>{/if}
</section>

<section class="card">
  <h3>{t("Layout gallery")}</h3>
  {#if gallery}
    {#each gallery as g (g.id)}
      <div class="item">
        <div><b>{g.name}</b><p class="muted">{g.description} · {g.author}</p></div>
        <button class="btn" disabled={busy} onclick={() => fromGallery(g.id)}>{t("Add")}</button>
      </div>
    {:else}
      <p class="muted">{t("No shared layouts yet.")}</p>
    {/each}
  {:else}
    <button class="btn" onclick={browse}><Icon name="globe" size={18} />{t("Browse layouts")}</button>
  {/if}
</section>

<style>
  .card {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 16px;
    border-radius: var(--radius);
    background: var(--bg);
  }
  h3 {
    margin: 0;
    font-size: 1rem;
  }
  p {
    margin: 0;
  }
  textarea {
    width: 100%;
    border-radius: var(--radius-sm);
    border: 1px solid var(--line);
    background: var(--surface);
    padding: 10px;
    font-size: 0.8rem;
    resize: vertical;
  }
  .row {
    display: flex;
    gap: 8px;
  }
  .item {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
  }
  .muted {
    color: var(--muted);
  }
  .warn {
    color: var(--warning);
  }
  .bad {
    color: var(--danger-text);
  }
</style>

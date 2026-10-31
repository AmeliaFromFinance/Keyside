<script lang="ts">
  import type { Profile } from "@keyside/protocol";
  import Icon from "../components/Icon.svelte";
  import { req } from "../lib/deck.svelte.ts";
  import { ed, change, profile, newId, reloadCatalog } from "./state.svelte.ts";
  import { t } from "../lib/i18n.svelte.ts";

  let { admin = false, onimported }: { admin?: boolean; onimported: () => void } = $props();

  let code = $state("");
  let incoming = $state("");
  let gallery = $state<Array<{ id: string; name: string; author: string; description: string }> | null>(null);
  let error = $state("");
  let busy = $state(false);

  interface Missing {
    name: string;
    uuid: string;
    keys: number;
  }
  /** What came over from an Elgato profile. Written by crates/companion/src/sdprofile.rs. */
  interface SdReport {
    keys: number;
    plugins: Missing[];
    unsupported: Missing[];
    dials: number;
    images: number;
    trimmed: number;
    deck: string | null;
  }
  let report = $state<{ name: string; r: SdReport } | null>(null);
  const MAX_LAYOUTS = 20;

  async function exportCode() {
    error = "";
    try {
      code = (await req<{ code: string }>("share.export", { profile: ed.profile })).code;
    } catch (e: any) {
      error = e.message;
    }
  }

  function add(p: Profile, close = true) {
    change((l) => l.profiles.push(p));
    ed.profile = p.id;
    ed.page = p.homePage;
    ed.key = null;
    if (close) onimported();
  }

  /** "Gaming", or "Gaming 2" when that name is taken. */
  function freeName(name: string): string {
    const taken = new Set(ed.layout?.profiles.map((p) => p.name));
    if (!taken.has(name)) return name;
    const base = name.slice(0, 36);
    let n = 2;
    while (taken.has(`${base} ${n}`)) n++;
    return `${base} ${n}`;
  }

  /** Elgato profiles are read on the PC: images go to the image library there. */
  async function importStreamDeck(f: File) {
    report = null;
    if (!admin) return void (error = t("Stream Deck profiles can only be imported in the editor on the PC."));
    if ((ed.layout?.profiles.length ?? 0) >= MAX_LAYOUTS) return void (error = t("There are 20 layouts already. Delete one under Manage layouts first."));
    busy = true;
    try {
      const r = await fetch("/admin/upload/sdprofile", { method: "POST", headers: { "x-keyside": "1" }, body: f });
      if (!r.ok) throw new Error(await r.text());
      const { profile: p, report: r2 } = (await r.json()) as { profile: Profile; report: SdReport };
      p.name = freeName(p.name);
      while (ed.layout?.profiles.some((q) => q.id === p.id)) p.id = newId("l", ed.layout.profiles.map((q) => q.id));
      add(p, false);
      await reloadCatalog().catch(() => {});
      report = { name: p.name, r: r2 };
    } catch (e: any) {
      error = e.message;
    } finally {
      busy = false;
    }
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
    const input = e.currentTarget as HTMLInputElement;
    const f = input.files?.[0];
    input.value = "";
    error = "";
    if (f && /\.streamDeckProfile$/i.test(f.name)) await importStreamDeck(f);
    else if (f && f.size < 4 * 1024 * 1024) await importText(await f.text());
  }

  function count(m: Missing): string {
    return m.keys === 1 ? t("{name}: 1 key", { name: m.name }) : t("{name}: {n} keys", { name: m.name, n: m.keys });
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
  <p class="muted">{t("Paste a share code, or choose a layout file exported from Keyside or a profile exported from the Elgato Stream Deck app (.streamDeckProfile).")}</p>
  <textarea class="mono" rows="3" bind:value={incoming} placeholder="PD1.…"></textarea>
  <div class="row">
    <button class="btn primary" disabled={!incoming.trim() || busy} onclick={() => importText(incoming)}>{t("Import code")}</button>
    <label class="btn" class:disabled={busy}><Icon name="file-up" size={18} />{t("Choose file")}<input type="file" accept=".json,.txt,.streamDeckProfile,application/json,text/plain" class="sr-only" disabled={busy} onchange={file} /></label>
  </div>
  {#if !admin}<p class="muted">{t("Steps that start programs can only be imported on the PC.")}</p>{/if}
  {#if report}
    {@const r = report.r}
    <div class="report" role="status">
      <p><b>{t("Added the layout {name}.", { name: report.name })}</b> {t("Keys imported: {n}", { n: r.keys })}</p>
      {#if r.deck}<p>{t("It belongs to your {model} now. Under Manage layouts you can pick the apps it shows for, or unlink it to use it on phones.", { model: r.deck })}</p>{/if}
      {#if r.plugins.length}
        <p>{t("These keys need a Stream Deck plugin. Add it under Stream Deck plugins and the keys work:")}</p>
        <ul>{#each r.plugins as m (m.uuid)}<li>{count(m)}</li>{/each}</ul>
      {/if}
      {#if r.unsupported.length}
        <p>{t("Keyside has nothing for these actions yet. Their keys kept title and image, without steps:")}</p>
        <ul>{#each r.unsupported as m (m.uuid)}<li>{count(m)}</li>{/each}</ul>
      {/if}
      {#if r.dials}<p>{t("Dial actions left out: {n}. Dials need a Stream Deck + connected to this PC.", { n: r.dials })}</p>{/if}
      {#if r.images}<p>{t("Images left out: {n}. They were missing, larger than 512 KB or not an image.", { n: r.images })}</p>{/if}
      {#if r.trimmed}<p>{t("Multi actions cut to 32 steps: {n}", { n: r.trimmed })}</p>{/if}
      <p class="muted">{t("Imported steps that control apps or the PC stay off until you approve them.")}</p>
      <button class="btn primary" onclick={() => ((report = null), onimported())}>{t("Done")}</button>
    </div>
  {/if}
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
  .report {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--line);
    background: var(--surface);
  }
  .report ul {
    margin: 0;
    padding-left: 20px;
  }
  .report .btn {
    align-self: flex-start;
  }
  .disabled {
    opacity: 0.6;
    pointer-events: none;
  }
  .warn {
    color: var(--warning);
  }
  .bad {
    color: var(--danger-text);
  }
</style>

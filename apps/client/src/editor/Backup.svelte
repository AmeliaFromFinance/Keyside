<script lang="ts">
  // Backup and restore, in Settings. Only on the PC itself.
  import Icon from "../components/Icon.svelte";
  import { req } from "../lib/deck.svelte.ts";
  import { load, reloadCatalog } from "./state.svelte.ts";
  import { t } from "../lib/i18n.svelte.ts";

  interface Contents {
    layouts: number;
    pages: number;
    keys: number;
    plugins: number;
    pluginSettings: number;
    streamdeckPlugins: number;
    devices: number;
    tokens: number;
    assets: number;
    history: number;
    knownDecks: number;
    files: number;
    bytes: number;
  }
  interface Manifest {
    created: number;
    keyside: string;
    name: string;
    contents: Contents;
    secrets?: unknown;
  }

  interface Auto {
    id: string;
    created: number;
    size: number;
    keyside: string;
    contents: Contents;
  }

  // `settings` is the Settings form, so saving it later keeps this choice.
  let { onrestored, settings }: { onrestored?: () => void; settings?: Record<string, any> | null } = $props();

  const MIN = 8;
  let secrets = $state(false);
  let pass = $state("");
  let pass2 = $state("");
  let making = $state(false);
  let staged = $state<{ id: string; manifest: Manifest } | null>(null);
  let restorePass = $state("");
  let restoring = $state(false);
  let done = $state<{ restartNeeded: boolean; devicesDropped: number; secrets: boolean } | null>(null);
  let error = $state("");

  let autos = $state<Auto[]>([]);
  let autoFolder = $state("");
  let autoBusy = $state("");

  const passOk = $derived(!secrets || (pass.length >= MIN && pass === pass2));

  async function loadAutos() {
    const r = await req<{ backups: Auto[]; folder: string }>("backup.auto.list");
    autos = r.backups;
    autoFolder = r.folder;
  }
  $effect(() => void loadAutos().catch((e) => (error = e.message)));

  async function setAuto(on: boolean) {
    error = "";
    try {
      await req("settings.set", { autoBackup: on });
      if (settings) settings.autoBackup = on;
    } catch (e: any) {
      error = e.message;
    }
  }

  async function stageAuto(id: string) {
    error = "";
    done = null;
    staged = null;
    autoBusy = id;
    try {
      staged = await req("backup.auto.stage", { id }, 60_000);
    } catch (e: any) {
      error = e.message;
      await loadAutos().catch(() => {});
    } finally {
      autoBusy = "";
    }
  }

  async function downloadAuto(id: string) {
    error = "";
    try {
      const r = await fetch("/admin/backup/auto", {
        method: "POST",
        headers: { "x-keyside": "1", "content-type": "application/json" },
        body: JSON.stringify({ id }),
      });
      if (!r.ok) throw new Error((await r.text()) || r.statusText);
      save(await r.blob(), id);
    } catch (e: any) {
      error = e.message;
    }
  }

  async function showFolder() {
    error = "";
    await req("backup.auto.folder").catch((e) => (error = e.message));
  }

  function save(blob: Blob, name: string) {
    const a = document.createElement("a");
    a.href = URL.createObjectURL(blob);
    a.download = name;
    a.click();
    setTimeout(() => URL.revokeObjectURL(a.href), 1000);
  }

  async function download() {
    error = "";
    making = true;
    try {
      const r = await fetch("/admin/backup", {
        method: "POST",
        headers: { "x-keyside": "1", "content-type": "application/json" },
        body: JSON.stringify({ passphrase: secrets ? pass : null }),
      });
      if (!r.ok) throw new Error((await r.text()) || r.statusText);
      const name = /filename="([^"]+)"/.exec(r.headers.get("content-disposition") ?? "")?.[1] ?? "keyside-backup.zip";
      save(await r.blob(), name);
      pass = pass2 = "";
    } catch (e: any) {
      error = e.message;
    } finally {
      making = false;
    }
  }

  async function pick(e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    input.value = "";
    if (!file) return;
    error = "";
    done = null;
    staged = null;
    const r = await fetch("/admin/backup/inspect", { method: "POST", headers: { "x-keyside": "1" }, body: file }).catch((err) => ({ ok: false, text: async () => String(err) }) as Response);
    if (!r.ok) error = await r.text();
    else staged = await r.json();
  }

  async function cancel() {
    staged = null;
    restorePass = "";
    await req("backup.cancel").catch(() => {});
  }

  async function restore() {
    if (!staged) return;
    error = "";
    restoring = true;
    try {
      done = await req("backup.restore", { id: staged.id, passphrase: restorePass || null }, 120_000);
      staged = null;
      restorePass = "";
      // The layout changed under the editor; drafts of the old one no longer apply.
      await load(false);
      await reloadCatalog();
      await loadAutos().catch(() => {});
      onrestored?.();
    } catch (e: any) {
      error = e.message;
    } finally {
      restoring = false;
    }
  }

  const size = (b: number) => (b > 1024 * 1024 ? `${(b / 1024 / 1024).toFixed(1)} MB` : `${Math.max(1, Math.round(b / 1024))} KB`);
</script>

<section class="card">
  <h3>{t("Backup and restore")}</h3>
  <p class="muted">{t("One file with your layouts, settings, plugins and their settings, paired devices, API tokens, images and sounds, and Stream Deck plugins.")}</p>
  {#if error}<p class="bad" role="alert">{error}</p>{/if}

  <label class="check"><input type="checkbox" bind:checked={secrets} />{t("Include passwords and keys")}</label>
  {#if secrets}
    <p class="muted">{t("Plugin passwords and sign-ins, device keys and PINs go in encrypted with this passphrase. Without it they can't be restored, so keep it safe.")}</p>
    <div class="two">
      <label class="field"><span>{t("Passphrase")}</span><input type="password" autocomplete="new-password" bind:value={pass} /></label>
      <label class="field"><span>{t("Passphrase again")}</span><input type="password" autocomplete="new-password" bind:value={pass2} /></label>
    </div>
    {#if pass && pass.length < MIN}<small class="warn">{t("Use at least {n} characters.", { n: MIN })}</small>
    {:else if pass2 && pass !== pass2}<small class="warn">{t("The passphrases don't match.")}</small>{/if}
  {:else}
    <p class="muted">{t("Without passwords and keys, restored plugins need their passwords again and phones paired elsewhere pair again.")}</p>
  {/if}
  <div class="row">
    <button class="btn primary" disabled={making || !passOk} onclick={download}><Icon name="download" size={18} />{making ? t("Making the backup…") : t("Download backup")}</button>
    <label class="btn"><Icon name="file-up" size={18} />{t("Restore from file")}<input type="file" accept=".zip,application/zip" class="sr-only" onchange={pick} /></label>
  </div>

  <div class="auto">
    <h4>{t("Automatic backups")}</h4>
    {#if settings}
      <label class="check"><input type="checkbox" checked={settings.autoBackup} onchange={(e) => void setAuto(e.currentTarget.checked)} />{t("Save a backup every day")}</label>
    {/if}
    <p class="muted">{t("The newest 7 stay on this PC. A day without changes adds none. Automatic backups never include passwords and keys.")}</p>
    {#if autos.length}
      <ul class="list">
        {#each autos as b (b.id)}
          <li>
            <span class="what">
              <b>{new Date(b.created).toLocaleString()}</b>
              <span class="muted">{size(b.size)} · {t("Layouts: {layouts} · Pages: {pages} · Keys: {keys}", { layouts: b.contents.layouts, pages: b.contents.pages, keys: b.contents.keys })} · {t("Plugins: {n}", { n: b.contents.plugins })} · {t("Paired devices: {n}", { n: b.contents.devices })}</span>
            </span>
            <span class="acts">
              <button class="btn" disabled={!!autoBusy || restoring} onclick={() => stageAuto(b.id)}><Icon name="rotate-ccw" size={16} />{autoBusy === b.id ? t("Checking…") : t("Restore")}</button>
              <button class="btn" onclick={() => downloadAuto(b.id)}><Icon name="download" size={16} />{t("Download")}</button>
            </span>
          </li>
        {/each}
      </ul>
    {:else}
      <p class="muted">{t("None yet. The first one is saved a minute after Keyside starts.")}</p>
    {/if}
    <div class="row">
      <button class="btn" onclick={showFolder} title={autoFolder}><Icon name="folder-open" size={16} />{t("Show folder")}</button>
    </div>
  </div>

  {#if staged}
    {@const c = staged.manifest.contents}
    <div class="staged" role="region" aria-label={t("Backup contents")}>
      <h4>{t("Backup from {name}", { name: staged.manifest.name })}</h4>
      <p class="muted">{new Date(staged.manifest.created).toLocaleString()} · Keyside {staged.manifest.keyside} · {size(c.bytes)}</p>
      <ul>
        <li>{t("Layouts: {layouts} · Pages: {pages} · Keys: {keys}", { layouts: c.layouts, pages: c.pages, keys: c.keys })}</li>
        <li>{t("Plugins: {n} · With settings: {s}", { n: c.plugins, s: c.pluginSettings })}</li>
        <li>{t("Stream Deck plugins: {n}", { n: c.streamdeckPlugins })}</li>
        <li>{t("Paired devices: {n}", { n: c.devices })}</li>
        <li>{t("API tokens: {n}", { n: c.tokens })}</li>
        <li>{t("Images and sounds: {n}", { n: c.assets })}</li>
        <li>{t("Pushed versions in the history: {n}", { n: c.history })}</li>
        <li>{staged.manifest.secrets ? t("Passwords and keys, encrypted") : t("No passwords or keys")}</li>
      </ul>
      {#if staged.manifest.secrets}
        <label class="field"><span>{t("Passphrase of the backup")}</span><input type="password" autocomplete="off" bind:value={restorePass} /></label>
      {:else}
        <p class="muted">{t("Passwords and keys already on this PC stay. Paired devices this PC doesn't know have to pair again.")}</p>
      {/if}
      <p class="warn"><Icon name="triangle-alert" size={18} />{t("This replaces the layouts, settings, plugins, devices and tokens on this PC. Plugins restart.")}</p>
      <div class="row">
        <button class="btn danger" disabled={restoring || (!!staged.manifest.secrets && !restorePass)} onclick={restore}>{restoring ? t("Restoring…") : t("Replace and restore")}</button>
        <button class="btn" disabled={restoring} onclick={cancel}>{t("Cancel")}</button>
      </div>
    </div>
  {/if}

  {#if done}
    <div class="done" role="status">
      <p><Icon name="circle-check" size={18} />{t("Restored.")}</p>
      {#if done.devicesDropped}<p>{t("Devices that need to pair again: {n}", { n: done.devicesDropped })}</p>{/if}
      {#if done.restartNeeded}<p class="warn">{t("Restart Keyside to finish: network settings or the identity phones pair with changed.")}</p>{/if}
    </div>
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
    margin: 0 0 4px;
    font-size: 1rem;
  }
  h4 {
    margin: 0;
  }
  p {
    margin: 0;
    display: flex;
    gap: 8px;
    align-items: center;
    flex-wrap: wrap;
  }
  ul {
    margin: 0;
    padding-left: 1.2em;
  }
  .two {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }
  .row {
    display: flex;
    gap: 12px;
    align-items: center;
    flex-wrap: wrap;
  }
  .staged,
  .done {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--line);
  }
  .auto {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding-top: 12px;
    border-top: 1px solid var(--line);
  }
  .list {
    list-style: none;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .list li {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
  }
  .what {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .acts {
    display: flex;
    gap: 8px;
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

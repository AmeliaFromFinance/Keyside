<script lang="ts">
  import Icon from "../components/Icon.svelte";
  import { req } from "../lib/deck.svelte.ts";
  import { t, lang, setLang, languages, type Lang } from "../lib/i18n.svelte.ts";
  import { inDesktopApp, zoomTo } from "../lib/zoom.ts";
  import Backup from "./Backup.svelte";

  interface Info {
    name: string;
    version: string;
    addresses: string[];
    lanPort: number;
    adminPort: number;
    fingerprint: string;
    deno: boolean;
    zoom: number;
    zoomAuto: boolean;
  }
  interface Token {
    id: string;
    name: string;
    scopes: string[];
    created: number;
    lastUsed: number;
  }

  let s = $state<Record<string, any> | null>(null);
  let info = $state<Info | null>(null);
  let tokens = $state<Token[]>([]);
  let newName = $state("Streamer.bot");
  let newScopes = $state<string[]>(["press", "variables"]);
  let secret = $state("");
  let msg = $state("");
  let error = $state("");

  async function refresh() {
    [s, info, tokens] = await Promise.all([req("settings.get"), req<Info>("companion.info"), req<Token[]>("tokens.list")]);
  }
  $effect(() => void refresh().catch((e) => (error = e.message)));

  async function save() {
    error = "";
    try {
      await req("settings.set", $state.snapshot(s));
      msg = t("Saved. Restart Keyside for network changes to apply.");
    } catch (e: any) {
      error = e.message;
    }
  }

  async function create() {
    const r = await req<{ secret: string }>("tokens.create", { name: newName, scopes: newScopes });
    secret = r.secret;
    tokens = await req("tokens.list");
  }

  const sizes = [0.8, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2];
  const pct = (z: number) => `${Math.round(z * 100)}%`;
  const modKey = /Mac/i.test(navigator.userAgent) ? "Cmd" : "Ctrl";

  async function setZoom(v: string) {
    if (!info) return;
    const r = await zoomTo(v === "auto" ? null : Number(v));
    info.zoom = r.zoom;
    info.zoomAuto = r.auto;
  }

  const tailscale = $derived((info?.addresses ?? []).filter((a) => /^100\.(6[4-9]|[7-9]\d|1[01]\d|12[0-7])\./.test(a)));
</script>

{#if error}<p class="bad" role="alert">{error}</p>{/if}

{#if info}
  <section class="card">
    <h3>{t("This PC")}</h3>
    <p>Keyside <span class="mono">{info.version}</span></p>
    <p>{t("Phones connect to")} {#each info.addresses as a (a)}<span class="mono addr">{a}:{info.lanPort}</span>{:else}<span class="bad">{t("no network found")}</span>{/each}</p>
    <p class="muted">{t("Certificate")} <span class="mono fp">{info.fingerprint}</span></p>
    {#if !info.deno}<p class="bad"><Icon name="triangle-alert" size={18} />{t("The plugin runtime is missing, so plugins can't run. Reinstall Keyside.")}</p>{/if}
    {#if tailscale.length}
      <p class="ok"><Icon name="globe" size={18} />{t("Tailscale is on. Paired phones on your tailnet can reach this PC from anywhere.")}</p>
    {:else}
      <p class="muted">{t("To use your deck away from home, install Tailscale on this PC and your phone. See docs/remote-access.md.")}</p>
    {/if}
  </section>
{/if}

{#if s}
  <section class="card">
    <h3>{t("General")}</h3>
    <label class="field"><span>{t("Name shown on devices")}</span><input bind:value={s.name} maxlength="40" /></label>
    <label class="field">
      <span>{t("Editor language")}</span>
      <select value={lang()} onchange={(e) => setLang(e.currentTarget.value as Lang)}>
        {#each Object.entries(languages) as [code, name] (code)}<option value={code}>{name}</option>{/each}
      </select>
    </label>
    {#if info && inDesktopApp()}
      <label class="field">
        <span>{t("Interface size")}</span>
        <select value={info.zoomAuto ? "auto" : String(info.zoom)} onchange={(e) => void setZoom(e.currentTarget.value).catch((err) => (error = err.message))}>
          <option value="auto">{info.zoomAuto ? t("Automatic ({size})", { size: pct(info.zoom) }) : t("Automatic")}</option>
          {#each sizes as z (z)}<option value={String(z)}>{pct(z)}</option>{/each}
        </select>
        <small>{t("{key} and + or -, or {key} and the scroll wheel, also change it.", { key: modKey })}</small>
      </label>
    {/if}
    <label class="check"><input type="checkbox" bind:checked={s.followFocus} />{t("Switch layouts to match the app in front")}</label>
    <label class="check"><input type="checkbox" bind:checked={s.hardwareDecks} />{t("Use Stream Deck hardware plugged into this PC")}</label>
  </section>

  <section class="card">
    <h3>{t("Network")}</h3>
    <label class="check"><input type="checkbox" bind:checked={s.lanEnabled} />{t("Let paired devices connect")}</label>
    <label class="check"><input type="checkbox" bind:checked={s.mdns} />{t("Announce this PC on the local network")}</label>
    <label class="check"><input type="checkbox" bind:checked={s.apiOnLan} />{t("Allow the HTTP API from other computers (HTTPS, token needed)")}</label>
    <div class="two">
      <label class="field"><span>{t("Device port")}</span><input type="number" min="1024" max="65535" bind:value={s.lanPort} /></label>
      <label class="field"><span>{t("Editor port")}</span><input type="number" min="1024" max="65535" bind:value={s.adminPort} /></label>
    </div>
    <label class="field"><span>{t("Listen on")}</span><input class="mono" bind:value={s.lanBind} /><small>{t("0.0.0.0 means every network. Use one address to limit it, like your Tailscale address.")}</small></label>
    <label class="field"><span>{t("Plugin registry")}</span><input class="mono" bind:value={s.registryUrl} /></label>
  </section>
  <div class="row">
    <button class="btn primary" onclick={save}>{t("Save settings")}</button>
    {#if msg}<span class="muted">{msg}</span>{/if}
  </div>
{/if}

<section class="card">
  <h3>{t("API tokens")}</h3>
  <p class="muted">{t("Let other tools press keys and set variables over HTTP. Each token only gets the permissions you tick.")}</p>
  {#each tokens as tk (tk.id)}
    <div class="tok">
      <span><b>{tk.name}</b> <span class="muted">{tk.scopes.join(", ")}</span></span>
      <button class="btn danger" onclick={async () => (await req("tokens.revoke", { id: tk.id }), (tokens = await req("tokens.list")))}>{t("Revoke")}</button>
    </div>
  {/each}
  {#if secret}
    <div class="secret">
      <p>{t("Copy this token now. It won't be shown again.")}</p>
      <code class="mono">{secret}</code>
      <button class="btn" onclick={() => navigator.clipboard?.writeText(secret)}><Icon name="copy" size={16} />{t("Copy")}</button>
    </div>
  {/if}
  <div class="new">
    <input class="input" bind:value={newName} maxlength="40" aria-label={t("Token name")} />
    <label class="check"><input type="checkbox" value="press" bind:group={newScopes} />{t("Press keys")}</label>
    <label class="check"><input type="checkbox" value="variables" bind:group={newScopes} />{t("Variables")}</label>
    <label class="check"><input type="checkbox" value="display" bind:group={newScopes} />{t("Change keys and pages")}</label>
    <button class="btn" onclick={create} disabled={!newScopes.length}>{t("Create token")}</button>
  </div>
</section>

<Backup onrestored={() => void refresh().catch((e) => (error = e.message))} />

<style>
  .fp {
    overflow-wrap: anywhere;
  }
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
  p {
    margin: 0;
    display: flex;
    gap: 8px;
    align-items: center;
    flex-wrap: wrap;
  }
  .addr {
    padding: 2px 6px;
    border-radius: 6px;
    background: var(--raised);
  }
  .two {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }
  .row,
  .new {
    display: flex;
    gap: 12px;
    align-items: center;
    flex-wrap: wrap;
  }
  .new .input {
    flex: 1 1 180px;
  }
  .tok {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
  }
  .secret {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--accent);
  }
  code {
    word-break: break-all;
  }
  .muted {
    color: var(--muted);
  }
  .bad {
    color: var(--danger-text);
  }
  .ok {
    color: var(--success);
  }
</style>

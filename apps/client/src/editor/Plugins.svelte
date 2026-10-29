<script lang="ts">
  import Icon from "../components/Icon.svelte";
  import SettingsForm from "./SettingsForm.svelte";
  import type { Schema } from "./state.svelte.ts";
  import { reloadCatalog } from "./state.svelte.ts";
  import { req } from "../lib/deck.svelte.ts";
  import { t, mark } from "../lib/i18n.svelte.ts";

  interface Perms {
    net?: string[];
    udp?: boolean;
    ipc?: string[];
  }
  interface Info {
    id: string;
    name: string;
    version: string;
    author: string;
    prefix: string;
    description?: string;
    homepage?: string;
    enabled: boolean;
    status: string;
    source: "bundled" | "registry" | "file";
    signer?: string;
    requested: Perms;
    granted: Perms;
    needsApproval: boolean;
    connection?: Schema;
    values: Record<string, unknown>;
  }
  interface Staged {
    stageId: string;
    source: string;
    signer?: string;
    manifest: { id: string; name: string; version: string; author: string; description?: string; permissions: Perms };
    installed?: { version: string; signer?: string };
  }

  let { admin = false }: { admin?: boolean } = $props();

  const statusText: Record<string, string> = {
    connected: mark("connected"),
    connecting: mark("connecting"),
    offline: mark("offline"),
    error: mark("needs attention"),
    disabled: mark("off"),
  };

  let list = $state<Info[]>([]);
  let open = $state<string | null>(null);
  let edits = $state<Record<string, Record<string, unknown>>>({});
  let staged = $state<Staged | null>(null);
  let registry = $state<Array<{ id: string; name: string; version: string; author: string; description: string }> | null>(null);
  let error = $state("");
  let busy = $state(false);

  async function refresh() {
    list = await req<Info[]>("plugins.list");
  }
  $effect(() => void refresh().catch((e) => (error = e.message)));

  function describe(p: Perms): string[] {
    const out: string[] = [];
    for (const n of p.net ?? []) out.push(n.includes("{") ? t("Connect to the address set in its settings ({n})", { n }) : t("Connect to {n}", { n }));
    if (p.udp) out.push(t("Send UDP messages to those addresses"));
    if (p.ipc?.includes("discord")) out.push(t("Talk to the Discord app on this PC"));
    if (!out.length) out.push(t("No network access"));
    return out;
  }

  async function run(fn: () => Promise<unknown>) {
    busy = true;
    error = "";
    try {
      await fn();
      await refresh();
      await reloadCatalog();
    } catch (e: any) {
      error = e.message;
    } finally {
      busy = false;
    }
  }

  async function uploadFile(e: Event) {
    const file = (e.currentTarget as HTMLInputElement).files?.[0];
    if (!file) return;
    error = "";
    const r = await fetch("/admin/upload/plugin", { method: "POST", headers: { "x-keyside": "1" }, body: file });
    if (!r.ok) error = await r.text();
    else staged = await r.json();
  }

  async function browse() {
    error = "";
    try {
      registry = (await req<{ plugins: any[] }>("gallery.list")).plugins;
    } catch (e: any) {
      error = e.message;
    }
  }
</script>

{#if error}<p class="bad" role="alert">{error}</p>{/if}

{#if staged}
  <section class="staged">
    <h3>{t("Install {name} {v}?", { name: staged.manifest.name, v: staged.manifest.version })}</h3>
    <p>{t("By {author}", { author: staged.manifest.author })}{staged.manifest.description ? ` · ${staged.manifest.description}` : ""}</p>
    <p class:warn={!staged.signer}>
      <Icon name={staged.signer ? "badge-check" : "triangle-alert"} size={18} />
      {staged.signer ? t("Signed by key {k}", { k: staged.signer.slice(0, 12) }) : t("Not signed. Only install plugins from people you trust.")}
    </p>
    {#if staged.installed?.signer && staged.installed.signer !== staged.signer}<p class="bad">{t("Signed by a different key than the installed version.")}</p>{/if}
    <h4>{t("It will be able to")}</h4>
    <ul>{#each describe(staged.manifest.permissions) as d (d)}<li>{d}</li>{/each}</ul>
    <p class="muted">{t("Plugins can't read your files, start programs or see other plugins. They only reach what is listed here.")}</p>
    <div class="row">
      <button class="btn primary" disabled={busy} onclick={() => run(async () => (await req("plugins.install", { stageId: staged!.stageId, approve: true, source: staged!.source }), (staged = null)))}>{t("Install")}</button>
      <button class="btn" onclick={() => (staged = null)}>{t("Cancel")}</button>
    </div>
  </section>
{:else if registry}
  <section>
    <button class="btn ghost" onclick={() => (registry = null)}><Icon name="arrow-left" size={18} />{t("Back")}</button>
    {#each registry as r (r.id)}
      {@const have = list.find((p) => p.id === r.id)}
      <div class="item">
        <div><b>{r.name}</b> <span class="mono muted">{r.version}</span><p class="muted">{r.description} · {r.author}</p></div>
        <button class="btn" disabled={busy || have?.version === r.version} onclick={() => run(async () => (staged = await req("plugins.stage", { registryId: r.id })))}>
          {have ? (have.version === r.version ? t("Installed") : t("Update")) : t("Get")}
        </button>
      </div>
    {:else}
      <p class="muted">{t("The registry is empty right now.")}</p>
    {/each}
  </section>
{:else}
  {#if admin}
    <div class="row">
      <button class="btn" onclick={browse}><Icon name="download" size={18} />{t("Browse plugins")}</button>
      <label class="btn"><Icon name="file-up" size={18} />{t("Install from file")}<input type="file" accept=".pdp,application/zip" class="sr-only" onchange={uploadFile} /></label>
    </div>
  {/if}

  {#each list as p (p.id)}
    <div class="plugin" class:off={!p.enabled}>
      <button class="head" onclick={() => (open = open === p.id ? null : p.id)} aria-expanded={open === p.id}>
        <span class="dot" class:ok={p.status === "connected"} class:busy={p.status === "connecting"} class:bad={p.status === "error"}></span>
        <span class="name"><b>{p.name}</b> <span class="mono muted">{p.version}</span></span>
        {#if p.needsApproval}<span class="tag warn">{t("Needs approval")}</span>{/if}
        {#if p.source === "file" && !p.signer}<span class="tag">{t("Unsigned")}</span>{/if}
        <span class="muted status">{t(statusText[p.enabled ? p.status : "disabled"] ?? p.status)}</span>
      </button>
      {#if open === p.id}
        <div class="body">
          {#if p.description}<p class="muted">{p.description}</p>{/if}
          {#if p.connection && admin}
            <SettingsForm schema={p.connection} values={{ ...p.values, ...(edits[p.id] ?? {}) }} onchange={(v) => (edits[p.id] = v)} />
            <button class="btn primary" disabled={busy || !edits[p.id]} onclick={() => run(async () => (await req("plugins.configure", { id: p.id, values: edits[p.id] }), delete edits[p.id]))}>{t("Save settings")}</button>
          {/if}
          <h4>{t("Allowed to")}</h4>
          <ul>{#each describe(p.granted) as d (d)}<li>{d}</li>{/each}</ul>
          {#if p.needsApproval && admin}
            <p class="warn">{t("This version asks for more:")}</p>
            <ul>{#each describe(p.requested) as d (d)}<li>{d}</li>{/each}</ul>
            <button class="btn primary" onclick={() => run(() => req("plugins.approve", { id: p.id }))}>{t("Allow")}</button>
          {/if}
          <p class="muted">{t("Variables start with {prefix}.", { prefix: `${p.prefix}.` })}</p>
          {#if admin}
            <div class="row">
              <button class="btn" onclick={() => run(() => req("plugins.enable", { id: p.id, enabled: !p.enabled }))}>{p.enabled ? t("Turn off") : t("Turn on")}</button>
              <button class="btn" onclick={() => run(() => req("plugins.restart", { id: p.id }))}>{t("Restart")}</button>
              {#if p.source !== "bundled"}<button class="btn danger" onclick={() => run(() => req("plugins.uninstall", { id: p.id }))}>{t("Remove")}</button>{/if}
            </div>
          {/if}
        </div>
      {/if}
    </div>
  {/each}
{/if}

<style>
  .row {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }
  .plugin {
    border-radius: var(--radius);
    background: var(--bg);
    border: 1px solid var(--line-soft);
  }
  .plugin.off {
    opacity: 0.7;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    min-height: 56px;
    padding: 0 16px;
    border: none;
    background: none;
    text-align: left;
  }
  .name {
    flex: 1;
    min-width: 0;
  }
  .status {
    font-size: 0.9rem;
  }
  .body {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 0 16px 16px;
  }
  .tag {
    font-size: 0.75rem;
    padding: 2px 8px;
    border-radius: 6px;
    background: var(--raised);
    color: var(--muted);
  }
  .tag.warn {
    color: var(--bg);
    background: var(--warning);
  }
  h3,
  h4 {
    margin: 0;
  }
  h4 {
    font-size: 0.95rem;
    color: var(--muted);
    font-weight: 500;
  }
  ul {
    margin: 0;
    padding-left: 1.2em;
  }
  p {
    margin: 0;
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
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
  .staged,
  section {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .item {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
    padding: 12px 16px;
    border-radius: var(--radius-sm);
    background: var(--bg);
  }
</style>

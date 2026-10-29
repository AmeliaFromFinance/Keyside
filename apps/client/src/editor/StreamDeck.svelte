<script lang="ts">
  import Icon from "../components/Icon.svelte";
  import { req } from "../lib/deck.svelte.ts";
  import { reloadCatalog } from "./state.svelte.ts";
  import { t, mark } from "../lib/i18n.svelte.ts";

  type Runtime = "node" | "native" | "html";
  interface Info {
    uuid: string;
    name: string;
    author: string;
    version: string;
    description: string;
    runtime: Runtime | null;
    enabled: boolean;
    running: boolean;
    blocked: string | null;
    net: string[];
    allNet: boolean;
    unsandboxed: boolean;
    actions: number;
  }
  interface Staged {
    token: string;
    uuid: string;
    name: string;
    author: string;
    version: string;
    description: string;
    runtime: Runtime | null;
    actions: string[];
    update: string | null;
  }

  const runtimeText: Record<Runtime, string> = {
    node: mark("JavaScript, runs in the sandbox"),
    native: mark("Program for this system, runs unsandboxed"),
    html: mark("Web page plugin, can't run here"),
  };

  // Reasons the companion gives for a plugin that can't run, so they get translated.
  mark("This plugin has no version for this system");
  mark("Web-page plugins need Elgato's app and can't run in Keyside");
  mark("Needs your permission to run unsandboxed");
  mark("The plugin runtime (Deno) is missing");

  let list = $state<Info[]>([]);
  let deno = $state(true);
  let staged = $state<Staged | null>(null);
  let trust = $state(false);
  let open = $state<string | null>(null);
  let hosts = $state<Record<string, string>>({});
  let error = $state("");
  let busy = $state(false);

  async function refresh() {
    const r = await req<{ plugins: Info[]; deno: boolean }>("streamdeck.list");
    list = r.plugins;
    deno = r.deno;
  }
  $effect(() => {
    void refresh().catch((e) => (error = e.message));
    const timer = setInterval(() => void refresh().catch(() => {}), 2000);
    return () => clearInterval(timer);
  });

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

  async function upload(e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    input.value = "";
    if (!file) return;
    error = "";
    trust = false;
    const r = await fetch("/admin/upload/streamdeck", { method: "POST", headers: { "x-keyside": "1" }, body: file });
    if (!r.ok) error = await r.text();
    else staged = await r.json();
  }

  function access(p: Info): "none" | "list" | "all" {
    return p.allNet ? "all" : p.net.length ? "list" : "none";
  }

  function setAccess(p: Info, mode: "none" | "list" | "all") {
    if (mode === "list") {
      hosts[p.uuid] ??= p.net.join("\n");
      open = p.uuid;
      return;
    }
    void run(() => req("streamdeck.grant", { uuid: p.uuid, net: [], allNet: mode === "all" }));
  }

  function saveHosts(p: Info) {
    const net = (hosts[p.uuid] ?? "")
      .split(/[\s,]+/)
      .map((h) => h.trim().replace(/^https?:\/\//, "").replace(/\/.*$/, ""))
      .filter(Boolean);
    void run(() => req("streamdeck.grant", { uuid: p.uuid, net, allNet: false }));
  }
</script>

{#if error}<p class="bad" role="alert">{error}</p>{/if}
{#if !deno}<p class="warn"><Icon name="triangle-alert" size={18} />{t("The plugin runtime (Deno) is missing, so JavaScript plugins can't start.")}</p>{/if}

{#if staged}
  <section class="staged">
    <h3>{staged.update ? t("Update {name} to {v}?", { name: staged.name, v: staged.version }) : t("Install {name} {v}?", { name: staged.name, v: staged.version })}</h3>
    <p class="muted">{t("By {author}", { author: staged.author || "?" })}{staged.description ? ` · ${staged.description}` : ""}</p>
    <p>{t("Actions")}: {staged.actions.join(", ")}</p>
    {#if staged.runtime === "html"}
      <p class="bad">{t("This is a web page plugin. Those need Elgato's own app and can't run in Keyside.")}</p>
    {:else if staged.runtime === "native"}
      <p class="bad"><Icon name="shield-off" size={18} />{t("This plugin is a program, not a script. Keyside can't sandbox it: it can do anything you can do on this PC.")}</p>
      <label class="check"><input type="checkbox" bind:checked={trust} />{t("I trust where this plugin came from and want to run it anyway")}</label>
    {:else}
      <p><Icon name="shield-check" size={18} />{t("Runs in the sandbox: it can't read your files, start programs or reach the internet unless you allow it.")}</p>
    {/if}
    <div class="row">
      <button
        class="btn primary"
        disabled={busy || staged.runtime === "html" || (staged.runtime === "native" && !trust)}
        onclick={() => run(async () => (await req("streamdeck.install", { token: staged!.token, unsandboxed: staged!.runtime === "native" && trust }), (staged = null)))}
      >
        {t("Install")}
      </button>
      <button class="btn" onclick={() => (staged = null)}>{t("Cancel")}</button>
    </div>
  </section>
{:else}
  <p class="muted">{t("Plugins made for the Elgato Stream Deck app work here too. Download one (a .streamDeckPlugin file) and add it.")}</p>
  <div class="row">
    <label class="btn"><Icon name="file-up" size={18} />{t("Add Stream Deck plugin")}<input type="file" accept=".streamDeckPlugin,application/zip" class="sr-only" onchange={upload} /></label>
  </div>

  {#each list as p (p.uuid)}
    <div class="plugin" class:off={!p.enabled}>
      <button class="head" onclick={() => (open = open === p.uuid ? null : p.uuid)} aria-expanded={open === p.uuid}>
        <span class="dot" class:ok={p.running} class:bad={!!p.blocked}></span>
        <span class="name"><b>{p.name}</b> <span class="mono muted">{p.version}</span></span>
        {#if p.unsandboxed}<span class="tag warn">{t("Unsandboxed")}</span>{/if}
        <span class="muted status">{p.blocked ? t(p.blocked) : !p.enabled ? t("off") : p.running ? t("running") : t("starting")}</span>
      </button>
      {#if open === p.uuid}
        <div class="body">
          {#if p.description}<p class="muted">{p.description}</p>{/if}
          {#if p.runtime}<p class="muted">{t(runtimeText[p.runtime])} · {p.actions === 1 ? t("1 action") : t("{n} actions", { n: p.actions })}</p>{/if}
          {#if p.runtime === "node"}
            <fieldset>
              <legend>{t("Internet access")}</legend>
              <label class="check"><input type="radio" name="net-{p.uuid}" checked={access(p) === "none"} onchange={() => setAccess(p, "none")} />{t("None")}</label>
              <label class="check"><input type="radio" name="net-{p.uuid}" checked={access(p) === "list" || hosts[p.uuid] !== undefined} onchange={() => setAccess(p, "list")} />{t("Only these sites")}</label>
              {#if access(p) === "list" || hosts[p.uuid] !== undefined}
                <textarea class="mono" rows="3" placeholder="api.example.com" bind:value={hosts[p.uuid]}></textarea>
                <button class="btn small" disabled={busy} onclick={() => saveHosts(p)}>{t("Save sites")}</button>
              {/if}
              <label class="check"><input type="radio" name="net-{p.uuid}" checked={access(p) === "all"} onchange={() => setAccess(p, "all")} />{t("Any site, including devices on your network")}</label>
            </fieldset>
          {/if}
          <div class="row">
            <button class="btn" disabled={busy} onclick={() => run(() => req("streamdeck.grant", { uuid: p.uuid, enabled: !p.enabled }))}>{p.enabled ? t("Turn off") : t("Turn on")}</button>
            {#if p.runtime === "native" && p.unsandboxed}
              <button class="btn" disabled={busy} onclick={() => run(() => req("streamdeck.grant", { uuid: p.uuid, unsandboxed: false }))}>{t("Stop running it unsandboxed")}</button>
            {/if}
            <button class="btn danger" disabled={busy} onclick={() => run(() => req("streamdeck.uninstall", { uuid: p.uuid }))}>{t("Remove")}</button>
          </div>
        </div>
      {/if}
    </div>
  {:else}
    <p class="muted">{t("No Stream Deck plugins yet.")}</p>
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
    text-align: right;
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
    color: var(--bg);
    background: var(--warning);
  }
  fieldset {
    border: 1px solid var(--line-soft);
    border-radius: var(--radius-sm);
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
    margin: 0;
  }
  legend {
    color: var(--muted);
    padding: 0 4px;
  }
  textarea {
    width: 100%;
  }
  h3 {
    margin: 0;
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
  .staged {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .small {
    min-height: 34px;
    align-self: flex-start;
  }
</style>

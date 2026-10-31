<script lang="ts">
  import Icon from "../components/Icon.svelte";
  import { deck } from "../lib/deck.svelte.ts";
  import { t } from "../lib/i18n.svelte.ts";
  import { plain } from "../lib/rich.ts";

  type App = { id: string; name: string; status: string; port: number | null };
  let { apps, admin, phones, onsetup, ondevices }: { apps: App[]; admin: boolean; phones: number; onsetup: (id: string) => void; ondevices: () => void } = $props();

  let open = $state(false);
  // Hover opened it, so the click that follows keeps it open rather than closing it.
  let hovered = false;
  let root: HTMLElement;
  let timer: ReturnType<typeof setTimeout> | undefined;
  // Hover only opens on a mouse. Touch and keyboard use the click.
  const hoverable = matchMedia("(hover: hover) and (pointer: fine)").matches;

  const rows = $derived(
    apps.map((a) => {
      const live = deck.connections.find((c) => c.id === a.id);
      return { ...a, status: live?.status ?? a.status, detail: live?.detail ? plain(live.detail) : "" };
    }),
  );
  const up = $derived(rows.filter((r) => r.status === "connected").length);
  const tone = $derived(rows.some((r) => r.status === "error") ? "bad" : rows.some((r) => r.status === "connecting") ? "busy" : rows.length && up === rows.length ? "ok" : "");

  function label(s: string) {
    if (s === "connected") return t("Connected");
    if (s === "connecting") return t("Connecting…");
    if (s === "error") return t("Can't connect");
    return t("Not connected");
  }
  function enter() {
    if (!hoverable) return;
    clearTimeout(timer);
    if (!open) hovered = true;
    open = true;
  }
  function leave() {
    if (!hoverable) return;
    clearTimeout(timer);
    timer = setTimeout(() => ((open = false), (hovered = false)), 250);
  }
  function pick(fn: () => void) {
    open = false;
    hovered = false;
    fn();
  }
</script>

<svelte:window
  onpointerdown={(e) => open && !root.contains(e.target as Node) && (open = false)}
  onkeydown={(e) => open && e.key === "Escape" && (open = false)}
/>

<div class="conn" bind:this={root} role="group" onpointerenter={enter} onpointerleave={leave}>
  <button class="chip" aria-label={t("Connected apps and phones")} aria-expanded={open} aria-haspopup="true" onclick={() => ((open = hovered || !open), (hovered = false))}>
    <span class="dot" class:ok={tone === "ok"} class:busy={tone === "busy"} class:bad={tone === "bad"}></span>
    <span class="txt">{rows.length ? t("{n} of {total} apps", { n: up, total: rows.length }) : t("Apps")}</span>
    {#if admin}<span class="sep"></span><Icon name="smartphone" size={16} /><span class="mono">{phones}</span>{/if}
    <Icon name="chevron-down" size={16} />
  </button>
  {#if open}
    <div class="pop" role="menu">
      {#each rows as r (r.id)}
        <svelte:element this={admin ? "button" : "div"} class="row" role="menuitem" tabindex={admin ? 0 : -1} title={r.detail || undefined} onclick={admin ? () => pick(() => onsetup(r.id)) : undefined}>
          <span class="dot" class:ok={r.status === "connected"} class:busy={r.status === "connecting"} class:bad={r.status === "error"}></span>
          <span class="name">{r.name}</span>
          {#if r.port}<span class="mono muted">:{r.port}</span>{/if}
          <span class="state" class:bad={r.status === "error"}>{label(r.status)}</span>
        </svelte:element>
      {:else}
        <p class="muted empty">{t("No apps in this layout yet.")}</p>
      {/each}
      {#if admin}
        <hr />
        <button class="row" role="menuitem" onclick={() => pick(ondevices)}>
          <span class="dot" class:ok={phones > 0}></span>
          <span class="name">{t("{n} phones online", { n: phones })}</span>
          <span class="state"><Icon name="chevron-right" size={16} /></span>
        </button>
      {/if}
    </div>
  {/if}
</div>

<style>
  .conn {
    position: relative;
    flex-shrink: 0;
  }
  .chip {
    gap: 8px;
    background: var(--bg);
    font: inherit;
    color: inherit;
    white-space: nowrap;
    border: 1px solid transparent;
    cursor: pointer;
  }
  .chip:hover,
  .chip[aria-expanded="true"] {
    border-color: var(--line);
  }
  /* Just the dots and phone count on smaller screens. */
  @media (max-width: 1279px) {
    .txt {
      display: none;
    }
  }
  .sep {
    width: 1px;
    height: 16px;
    background: var(--line);
  }
  .pop {
    position: absolute;
    left: 0;
    top: calc(100% + 8px);
    z-index: 20;
    min-width: 300px;
    max-height: 70vh;
    overflow-y: auto;
    padding: 6px;
    border-radius: 12px;
    border: 1px solid var(--line);
    background: var(--surface);
    box-shadow: 0 16px 40px rgb(0 0 0 / 0.45);
  }
  /* Keeps the hover open while the pointer crosses the gap. */
  .pop::before {
    content: "";
    position: absolute;
    left: 0;
    right: 0;
    top: -10px;
    height: 10px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    min-height: 44px;
    padding: 0 12px;
    border: 0;
    border-radius: 8px;
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
  }
  button.row {
    cursor: pointer;
  }
  button.row:hover,
  button.row:focus-visible {
    background: var(--bg);
  }
  .name {
    white-space: nowrap;
  }
  .state {
    margin-left: auto;
    padding-left: 16px;
    color: var(--muted);
    font-size: 0.875em;
    white-space: nowrap;
  }
  .state.bad {
    color: var(--danger);
  }
  .muted {
    color: var(--muted);
  }
  .empty {
    margin: 0;
    padding: 10px 12px;
  }
  hr {
    margin: 6px 0;
    border: 0;
    border-top: 1px solid var(--line-soft);
  }
</style>

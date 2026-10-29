<script lang="ts">
  import Icon from "../components/Icon.svelte";
  import { req } from "../lib/deck.svelte.ts";
  import { ed } from "./state.svelte.ts";
  import { t } from "../lib/i18n.svelte.ts";

  interface Item {
    profile: string;
    profileId: string;
    page: string;
    key: string;
    type: string;
    settings: Record<string, unknown>;
  }
  let { onapproved }: { onapproved: () => void } = $props();
  let items = $state<Item[] | null>(null);
  let error = $state("");

  $effect(() => void req<Item[]>("review.list").then((l) => (items = l)).catch((e) => (error = e.message)));

  async function approve() {
    try {
      await req("review.approve", {});
      onapproved();
    } catch (e: any) {
      error = e.message;
    }
  }
</script>

<p class="muted">{t("These steps came from an imported layout. They stay off until you approve them, because they can control apps or this PC.")}</p>
{#if ed.dirty}<p class="warn">{t("Push or discard your draft first. Approving reloads the layout.")}</p>{/if}
{#if error}<p class="bad">{error}</p>{/if}
{#if items}
  {#each items as it, i (i)}
    <div class="item">
      <p><b>{it.key}</b> <span class="muted">{it.profile} › {it.page}</span></p>
      <p class="mono type">{it.type}</p>
      <pre class="mono">{JSON.stringify(it.settings, null, 2)}</pre>
    </div>
  {:else}
    <p>{t("Nothing to review.")}</p>
  {/each}
  {#if items.length}
    <button class="btn primary" disabled={ed.dirty} onclick={approve}><Icon name="shield-check" size={18} />{t("Approve all {n} steps", { n: items.length })}</button>
  {/if}
{/if}

<style>
  p {
    margin: 0;
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
  .item {
    padding: 12px 16px;
    border-radius: var(--radius-sm);
    background: var(--bg);
    border: 1px dashed var(--warning);
  }
  .type {
    color: var(--accent);
    margin: 4px 0;
  }
  pre {
    margin: 0;
    white-space: pre-wrap;
    word-break: break-all;
    font-size: 0.8rem;
    color: var(--muted);
    max-height: 200px;
    overflow: auto;
  }
</style>

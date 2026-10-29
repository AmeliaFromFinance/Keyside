<script lang="ts">
  import type { Snippet } from "svelte";
  import Icon from "./Icon.svelte";
  import { t } from "../lib/i18n.svelte.ts";

  let { open = $bindable(false), title, wide = false, onclose, children }: { open?: boolean; title: string; wide?: boolean; onclose?: () => void; children: Snippet } = $props();
  let dialog: HTMLDialogElement;

  $effect(() => {
    if (open && !dialog.open) dialog.showModal();
    if (!open && dialog.open) dialog.close();
  });
</script>

<dialog bind:this={dialog} class:wide onclose={() => ((open = false), onclose?.())} onclick={(e) => e.target === dialog && (open = false)}>
  <header>
    <h2>{title}</h2>
    <button class="btn icon ghost" aria-label={t("Close")} onclick={() => (open = false)}><Icon name="x" /></button>
  </header>
  <div class="body">
    {#if open}{@render children()}{/if}
  </div>
</dialog>

<style>
  dialog {
    width: min(520px, calc(100vw - 24px));
    max-height: min(86dvh, 900px);
    padding: 0;
    border: 1px solid var(--line);
    border-radius: 16px;
    background: var(--surface);
    color: var(--text);
    overflow: hidden;
  }
  dialog.wide {
    width: min(880px, calc(100vw - 24px));
  }
  dialog[open] {
    display: flex;
    flex-direction: column;
  }
  dialog::backdrop {
    background: rgb(8 8 12 / 0.7);
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 12px 12px 20px;
    border-bottom: 1px solid var(--line-soft);
  }
  h2 {
    margin: 0;
    font-size: 1.1rem;
  }
  .body {
    padding: 20px;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  @media (max-width: 600px) {
    dialog {
      margin: auto auto 0;
      width: 100vw;
      max-width: 100vw;
      border-radius: 16px 16px 0 0;
    }
  }
</style>

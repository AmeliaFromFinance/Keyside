<script lang="ts">
  // "Copy overlay link" under a Timer step: a browser source page for OBS that shows the timer.
  import Icon from "../components/Icon.svelte";
  import { t } from "../lib/i18n.svelte.ts";

  let { name }: { name: unknown } = $props();
  const valid = $derived(typeof name === "string" && /^[a-z0-9_-]{1,24}$/.test(name));
  const link = $derived(valid ? `${location.origin}/overlay/timer/${name}` : "");
  let copied = $state(false);
  let field = $state<HTMLInputElement>();

  async function copy() {
    try {
      await navigator.clipboard.writeText(link);
    } catch {
      // No clipboard access: select the link so it can be copied by hand.
      field?.select();
      return;
    }
    copied = true;
    setTimeout(() => (copied = false), 2000);
  }
</script>

<div class="overlay">
  <div class="row">
    <input bind:this={field} readonly value={link} aria-label={t("Overlay link")} />
    <button class="btn" disabled={!valid} onclick={copy}><Icon name={copied ? "check" : "copy"} size={16} />{copied ? t("Copied") : t("Copy overlay link")}</button>
  </div>
  <p>{t("Add it to OBS as a browser source. Add ?hide=1 to hide it while stopped, or ?size=80&color=ffcc00 to style it.")}</p>
</div>

<style>
  .overlay {
    margin-top: 12px;
    display: grid;
    gap: 6px;
  }
  .row {
    display: flex;
    gap: 8px;
  }
  input {
    flex: 1;
    min-width: 0;
    min-height: 40px;
    padding: 0 10px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--line);
    background: var(--bg);
    color: var(--muted);
    font: inherit;
    font-size: 0.85rem;
  }
  .btn {
    gap: 6px;
    white-space: nowrap;
  }
  p {
    margin: 0;
    color: var(--muted);
    font-size: 0.85rem;
  }
</style>

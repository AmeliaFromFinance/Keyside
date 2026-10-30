<script lang="ts">
  import { page, saveTemplate } from "./state.svelte.ts";
  import { t } from "../lib/i18n.svelte.ts";

  let { ids, onclose }: { ids: string[]; onclose: () => void } = $props();
  const keys = page()?.keys.filter((k) => ids.includes(k.id)) ?? [];
  let name = $state(keys.length === 1 ? keys[0].label : "");
  let error = $state("");
  let busy = $state(false);

  async function save(e: SubmitEvent) {
    e.preventDefault();
    if (!name.trim()) return;
    busy = true;
    error = "";
    try {
      await saveTemplate(name, ids);
      onclose();
    } catch (err: any) {
      error = err.message;
    } finally {
      busy = false;
    }
  }

  function focus(el: HTMLInputElement) {
    el.focus();
    el.select();
  }
</script>

<form class="save" onsubmit={save}>
  <p class="muted">
    {keys.length === 1 ? t("Saves this key with its actions and look. It shows up at the top of the button palette in every editor.") : t("Saves these {n} keys in their arrangement. They show up at the top of the button palette in every editor.", { n: keys.length })}
  </p>
  {#if keys.some((k) => k.kind === "folder")}<p class="warn">{t("Folders stay out of templates.")}</p>{/if}
  <label class="field">
    <span>{t("Template name")}</span>
    <input class="input" bind:value={name} maxlength="40" required use:focus />
  </label>
  {#if error}<p class="bad" role="alert">{error}</p>{/if}
  <div class="buttons">
    <button type="button" class="btn" onclick={onclose}>{t("Cancel")}</button>
    <button class="btn primary" disabled={busy || !name.trim()}>{t("Save template")}</button>
  </div>
</form>

<style>
  .save {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
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
  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>

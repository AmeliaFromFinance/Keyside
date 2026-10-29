<script lang="ts">
  import { req } from "../lib/deck.svelte.ts";
  import { t } from "../lib/i18n.svelte.ts";

  let { instance, dirty = false }: { instance: string; dirty?: boolean } = $props();

  let url = $state("");
  let error = $state("");

  $effect(() => {
    const i = instance;
    url = "";
    error = "";
    req<{ url: string }>("streamdeck.inspector", { instance: i }).then(
      (r) => (url = r.url),
      (e) => (error = e.message === "key not found" || e.message === "action not found" ? t("Save the layout to open this plugin's settings.") : e.message),
    );
  });
</script>

{#if url}
  <!-- Plugin code: its own origin, no access to this page. -->
  <iframe title={t("Plugin settings")} src={url} sandbox="allow-scripts allow-forms allow-same-origin allow-popups allow-modals" referrerpolicy="no-referrer"></iframe>
  {#if dirty}<p class="muted">{t("Changes here are saved right away.")}</p>{/if}
{:else if error}
  <p class="muted">{error}</p>
{/if}

<style>
  iframe {
    width: 100%;
    height: 380px;
    border: 1px solid var(--line-soft);
    border-radius: var(--radius-sm);
    background: #2d2d2d;
  }
  p {
    margin: 0;
  }
  .muted {
    color: var(--muted);
  }
</style>

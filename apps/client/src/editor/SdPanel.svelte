<script lang="ts">
  import { req } from "../lib/deck.svelte.ts";
  import { t } from "../lib/i18n.svelte.ts";
  import { untrack } from "svelte";

  let { instance, dirty = false, rev = 0 }: { instance: string; dirty?: boolean; rev?: number } = $props();

  let url = $state("");
  let error = $state("");

  function load(i: string) {
    req<{ url: string }>("streamdeck.inspector", { instance: i }).then(
      (r) => i === instance && ((url = r.url), (error = "")),
      (e) => i === instance && (error = e.message === "key not found" || e.message === "action not found" ? t("Push to phones (Ctrl+S) to open this plugin's settings.") : e.message),
    );
  }

  $effect(() => {
    url = "";
    error = "";
    load(instance);
  });

  // A new key only exists on the companion once pushed, so try again after each push.
  let seenRev = untrack(() => rev);
  $effect(() => {
    if (rev === seenRev) return;
    seenRev = rev;
    if (!untrack(() => url)) load(untrack(() => instance));
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

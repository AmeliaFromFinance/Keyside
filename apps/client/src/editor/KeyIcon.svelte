<script lang="ts">
  import Icon from "../components/Icon.svelte";
  import { assetImage } from "../lib/deck.svelte.ts";

  /** A key icon: a built-in icon name or an uploaded image ("asset:<id>"). */
  let { icon, size = 24, fallback = "square-dashed" }: { icon?: string; size?: number; fallback?: string } = $props();
  let src = $state<string | null>(null);

  $effect(() => {
    src = null;
    if (!icon?.startsWith("asset:")) return;
    let live = true;
    void assetImage(icon.slice(6)).then((u) => live && (src = u));
    return () => (live = false);
  });
</script>

{#if icon?.startsWith("asset:")}
  {#if src}<img {src} alt="" width={size} height={size} />{:else}<span class="blank" style:width="{size}px" style:height="{size}px"></span>{/if}
{:else}
  <Icon name={icon && !icon.includes(":") ? icon : fallback} {size} />
{/if}

<style>
  img {
    object-fit: contain;
    border-radius: 4px;
    flex-shrink: 0;
  }
  .blank {
    display: inline-block;
    flex-shrink: 0;
    border-radius: 4px;
    background: var(--hover);
  }
</style>

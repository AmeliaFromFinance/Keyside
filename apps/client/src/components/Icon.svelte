<script lang="ts">
  import { icons } from "../lib/icons.gen.ts";

  let { name, size = 24, stroke = 2 }: { name?: string; size?: number; stroke?: number } = $props();
</script>

{#if name && icons[name]}
  <svg
    xmlns="http://www.w3.org/2000/svg"
    width={size}
    height={size}
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width={stroke}
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true">{@html icons[name]}</svg
  >
{/if}

<script lang="ts" module>
  export interface MenuItem {
    label: string;
    icon: string;
    run: () => void;
    danger?: boolean;
    disabled?: boolean;
    /** Shortcut shown on the right, like Ctrl+D. */
    hint?: string;
  }
</script>

<script lang="ts">
  import Icon from "./Icon.svelte";

  let { x, y, title, items, onclose }: { x: number; y: number; title?: string; items: MenuItem[]; onclose: () => void } = $props();
  let el: HTMLElement;
  let pos = $state({ left: 0, top: 0 });

  // Opens at the pointer, nudged back inside the window.
  $effect(() => {
    const r = el.getBoundingClientRect();
    pos = { left: Math.max(8, Math.min(x, innerWidth - r.width - 8)), top: Math.max(8, Math.min(y, innerHeight - r.height - 8)) };
    el.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus();
  });

  $effect(() => {
    const away = (e: Event) => !el.contains(e.target as Node) && onclose();
    const key = (e: KeyboardEvent) => e.key === "Escape" && onclose();
    // The press that opened the menu must not close it.
    const t = setTimeout(() => addEventListener("pointerdown", away, true));
    addEventListener("keydown", key);
    addEventListener("resize", onclose);
    addEventListener("blur", onclose);
    return () => {
      clearTimeout(t);
      removeEventListener("pointerdown", away, true);
      removeEventListener("keydown", key);
      removeEventListener("resize", onclose);
      removeEventListener("blur", onclose);
    };
  });

  function arrows(e: KeyboardEvent) {
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
    e.preventDefault();
    const list = [...el.querySelectorAll<HTMLButtonElement>("button:not(:disabled)")];
    const i = list.indexOf(document.activeElement as HTMLButtonElement);
    list[(i + (e.key === "ArrowDown" ? 1 : list.length - 1)) % list.length]?.focus();
  }
</script>

<div class="menu" role="menu" tabindex="-1" aria-label={title} bind:this={el} style:left="{pos.left}px" style:top="{pos.top}px" onkeydown={arrows}>
  {#if title}<p class="title">{title}</p>{/if}
  {#each items as item (item.label)}
    <button
      role="menuitem"
      class:danger={item.danger}
      disabled={item.disabled}
      onclick={() => {
        onclose();
        item.run();
      }}
    >
      <Icon name={item.icon} size={18} />
      <span>{item.label}</span>
      {#if item.hint}<kbd>{item.hint}</kbd>{/if}
    </button>
  {/each}
</div>

<style>
  .menu {
    position: fixed;
    z-index: 50;
    min-width: 200px;
    max-width: min(300px, calc(100vw - 16px));
    padding: 6px;
    border: 1px solid var(--line);
    border-radius: var(--radius);
    background: var(--surface);
    box-shadow: 0 12px 32px rgb(0 0 0 / 0.35);
  }
  .title {
    margin: 4px 10px 6px;
    color: var(--muted);
    font-size: 0.8rem;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  button {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    min-height: 40px;
    padding: 0 10px;
    border: none;
    border-radius: var(--radius-sm);
    background: none;
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  button:hover,
  button:focus-visible {
    background: var(--hover);
    outline: none;
  }
  button:disabled {
    opacity: 0.45;
    cursor: default;
  }
  button.danger {
    color: var(--danger-text);
  }
  span {
    flex: 1;
  }
  kbd {
    color: var(--muted);
    font: inherit;
    font-size: 0.8rem;
  }
  /* Bigger targets for fingers. */
  @media (pointer: coarse) {
    button {
      min-height: 48px;
    }
  }
</style>

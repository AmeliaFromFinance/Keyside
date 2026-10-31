<script lang="ts">
  // A select with a search box, for long lists such as every key in every layout.
  import Icon from "../components/Icon.svelte";
  import KeyIcon from "./KeyIcon.svelte";
  import { t } from "../lib/i18n.svelte.ts";

  export interface Choice {
    value: string;
    label: string;
    hint?: string;
    icon?: string;
  }
  let { value, choices, placeholder = "", label, onpick }: { value: string; choices: Choice[]; placeholder?: string; label: string; onpick: (value: string) => void } = $props();

  let open = $state(false);
  let q = $state("");
  let at = $state(0);
  let root: HTMLElement;
  let box = $state<HTMLInputElement>();
  let listEl = $state<HTMLElement>();
  const id = `choose-${Math.random().toString(36).slice(2, 8)}`;

  const current = $derived(choices.find((c) => c.value === value));
  const words = $derived(q.toLowerCase().split(/\s+/).filter(Boolean));
  const shown = $derived(words.length ? choices.filter((c) => words.every((w) => `${c.label} ${c.hint ?? ""}`.toLowerCase().includes(w))) : choices);

  function show() {
    open = true;
    q = "";
    at = Math.max(0, choices.findIndex((c) => c.value === value));
    requestAnimationFrame(() => {
      box?.focus();
      listEl?.querySelector(".on")?.scrollIntoView({ block: "nearest" });
    });
  }
  function pick(c: Choice) {
    open = false;
    if (c.value !== value) onpick(c.value);
    root.querySelector<HTMLElement>(".pick")?.focus();
  }
  function keys(e: KeyboardEvent) {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      at = Math.min(shown.length - 1, Math.max(0, at + (e.key === "ArrowDown" ? 1 : -1)));
      requestAnimationFrame(() => listEl?.querySelector(".on")?.scrollIntoView({ block: "nearest" }));
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (shown[at]) pick(shown[at]);
    } else if (e.key === "Escape") {
      // Closes the list, not the sheet around it.
      e.preventDefault();
      e.stopPropagation();
      open = false;
    }
  }
</script>

<svelte:window onpointerdown={(e) => open && !root.contains(e.target as Node) && (open = false)} />

<div class="choose" bind:this={root}>
  <button type="button" class="pick" aria-haspopup="listbox" aria-expanded={open} aria-label={label} onclick={() => (open ? (open = false) : show())}>
    {#if current}
      <KeyIcon icon={current.icon} size={18} fallback="square" />
      <span class="text"><span class="name">{current.label}</span>{#if current.hint}<span class="hint">{current.hint}</span>{/if}</span>
    {:else}
      <span class="text muted">{placeholder}</span>
    {/if}
    <Icon name="chevron-down" size={18} />
  </button>
  {#if open}
    <div class="pop">
      <label class="search">
        <Icon name="search" size={18} />
        <input bind:this={box} bind:value={q} oninput={() => (at = 0)} onkeydown={keys} placeholder={t("Search")} aria-label={t("Search")} role="combobox" aria-controls={id} aria-expanded="true" aria-activedescendant={shown[at] ? `${id}-${at}` : undefined} />
      </label>
      <ul bind:this={listEl} id={id} role="listbox" aria-label={label}>
        {#each shown as c, i (c.value)}
          <li id="{id}-{i}" role="option" aria-selected={c.value === value} class:on={i === at} class:chosen={c.value === value} onpointerenter={() => (at = i)} onclick={() => pick(c)} onkeydown={() => {}}>
            <KeyIcon icon={c.icon} size={18} fallback="square" />
            <span class="text"><span class="name">{c.label}</span>{#if c.hint}<span class="hint">{c.hint}</span>{/if}</span>
            {#if c.value === value}<Icon name="check" size={16} />{/if}
          </li>
        {:else}
          <li class="none">{t("Nothing matches “{q}”.", { q: q.trim() })}</li>
        {/each}
      </ul>
    </div>
  {/if}
</div>

<style>
  .choose {
    position: relative;
  }
  .pick {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    min-height: 44px;
    padding: 6px 12px;
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    background: var(--surface);
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .pick:hover,
  .pick[aria-expanded="true"] {
    border-color: var(--accent);
  }
  .text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .name,
  .hint {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .hint,
  .muted {
    color: var(--muted);
    font-size: 0.85em;
  }
  .pop {
    position: absolute;
    left: 0;
    right: 0;
    top: calc(100% + 6px);
    z-index: 20;
    padding: 6px;
    border: 1px solid var(--line);
    border-radius: 12px;
    background: var(--surface);
    box-shadow: 0 16px 40px rgb(0 0 0 / 0.45);
  }
  .search:focus-within {
    border-color: var(--accent);
  }
  .search {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 10px;
    margin-bottom: 6px;
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    background: var(--bg);
    color: var(--muted);
  }
  /* Beats the global field input style. */
  .choose .search input {
    flex: 1;
    min-height: 40px;
    padding: 0;
    border: 0;
    outline: none;
    background: none;
    color: var(--text);
    font: inherit;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    max-height: min(320px, 45dvh);
    overflow-y: auto;
  }
  li {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 44px;
    padding: 4px 10px;
    border-radius: 8px;
    cursor: pointer;
  }
  li.on {
    background: var(--bg);
  }
  li.chosen .name {
    color: var(--accent);
  }
  li.none {
    color: var(--muted);
    cursor: default;
  }
</style>

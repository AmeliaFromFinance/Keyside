<script lang="ts">
  import Icon from "../components/Icon.svelte";
  import KeyIcon from "./KeyIcon.svelte";
  import { ed, actionDef, pick } from "./state.svelte.ts";
  import { searchKeys, type KeyHit } from "./keysearch.ts";
  import { t } from "../lib/i18n.svelte.ts";

  let { onclose }: { onclose: () => void } = $props();
  let q = $state("");
  let at = $state(0);
  let list = $state<HTMLElement>();

  const describe = (type: string) => {
    const d = actionDef(type);
    if (!d) return { name: type.split("/").pop() ?? type };
    return { name: t(d.name), group: d.group ? t(d.group) : undefined, words: [d.name, d.group ?? ""] };
  };
  const hits = $derived(searchKeys(ed.layout, q, describe));
  const many = $derived((ed.layout?.profiles.length ?? 0) > 1);

  $effect(() => {
    void q;
    at = 0;
  });
  $effect(() => {
    list?.querySelector(`[data-i="${at}"]`)?.scrollIntoView({ block: "nearest" });
  });

  function go(h: KeyHit) {
    ed.armed = null;
    ed.moving = null;
    ed.profile = h.profile;
    ed.page = h.page;
    pick(h.key);
    onclose();
  }

  function keys(e: KeyboardEvent) {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      if (hits.length) at = (at + (e.key === "ArrowDown" ? 1 : hits.length - 1)) % hits.length;
    } else if (e.key === "Escape") {
      // Closes right away, even with text in the box.
      e.preventDefault();
      onclose();
    } else if (e.key === "Enter" && hits[at]) {
      e.preventDefault();
      go(hits[at]);
    }
  }

  function focus(el: HTMLInputElement) {
    el.focus();
  }
</script>

<div class="search">
  <label class="box">
    <Icon name="search" size={20} />
    <input
      type="search"
      bind:value={q}
      use:focus
      placeholder={t("Label, second line, action or plugin")}
      aria-label={t("Search all keys")}
      aria-controls="key-hits"
      aria-activedescendant={hits[at] ? `hit-${at}` : undefined}
      onkeydown={keys}
    />
  </label>
  {#if q.trim()}
    <ul id="key-hits" role="listbox" bind:this={list} aria-label={t("Keys found")}>
      {#each hits as h, i (`${h.profile}/${h.page}/${h.key}`)}
        <li id="hit-{i}" role="option" aria-selected={i === at} data-i={i}>
          <button class:on={i === at} onclick={() => go(h)} onpointermove={() => (at = i)} tabindex="-1">
            <span class="icon"><KeyIcon icon={h.icon} size={28} /></span>
            <span class="text">
              <b>{h.label || t("(no label)")}</b>{#if h.sub}<span class="sub">{h.sub}</span>{/if}
              <span class="where">
                {#if many}<span>{h.profileName}</span><span aria-hidden="true">›</span>{/if}{#if h.folder}<Icon name="folder" size={13} />{/if}{h.pageName}{#if h.steps.length}<span class="steps"> · {h.steps.join(", ")}</span>{/if}
              </span>
            </span>
          </button>
        </li>
      {:else}
        <li class="none">{t("No key matches “{q}”.", { q: q.trim() })}</li>
      {/each}
    </ul>
  {:else}
    <p class="tip">{t("Finds keys in every layout and page. Use the arrow keys and Enter to jump to one.")}</p>
  {/if}
</div>

<style>
  .search {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .box {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 14px;
    min-height: 48px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--accent);
    background: var(--bg);
    color: var(--muted);
  }
  .box input {
    flex: 1;
    min-width: 0;
    border: none;
    background: none;
    outline: none;
    color: var(--text);
    font-size: 1.05rem;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    max-height: min(60dvh, 520px);
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  li button {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 10px;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    background: none;
    text-align: left;
  }
  li button.on {
    background: var(--accent-bg);
    border-color: var(--accent);
  }
  .icon {
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    border-radius: 8px;
    background: var(--bg);
    flex-shrink: 0;
  }
  .text {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .text > * {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .sub,
  .where {
    color: var(--muted);
    font-size: 0.85rem;
  }
  .where {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .steps {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .none,
  .tip {
    margin: 0;
    padding: 8px 4px;
    color: var(--muted);
  }
</style>

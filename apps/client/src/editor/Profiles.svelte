<script lang="ts">
  import Icon from "../components/Icon.svelte";
  import { ed, change, newId } from "./state.svelte.ts";
  import { t } from "../lib/i18n.svelte.ts";

  function add() {
    if (!ed.layout) return;
    const id = newId("l", ed.layout.profiles.map((p) => p.id));
    change((l) => l.profiles.push({ id, name: t("New layout"), homePage: "p1", pages: [{ id: "p1", name: "Page 1", cols: 3, rows: 5, keys: [] }] }));
    ed.profile = id;
    ed.page = "p1";
  }

  function remove(id: string) {
    if (!ed.layout || ed.layout.profiles.length <= 1) return;
    change((l) => {
      l.profiles = l.profiles.filter((p) => p.id !== id);
      if (l.defaultProfile === id) l.defaultProfile = l.profiles[0].id;
    });
    if (ed.profile === id) {
      ed.profile = ed.layout.defaultProfile;
      ed.page = ed.layout.profiles[0].homePage;
    }
  }

  function apps(id: string, text: string) {
    change((l) => {
      const p = l.profiles.find((q) => q.id === id);
      if (p) {
        const list = text.split(",").map((s) => s.trim()).filter(Boolean).slice(0, 20);
        if (list.length) p.apps = list;
        else delete p.apps;
      }
    });
  }
</script>

<p class="muted">{t("Each layout has its own pages. Keyside can switch layouts when an app comes to the front on your PC.")}</p>
{#each ed.layout?.profiles ?? [] as p (p.id)}
  <div class="prof">
    <label class="field"><span>{t("Name")}</span><input value={p.name} maxlength="40" oninput={(e) => change(() => (p.name = e.currentTarget.value))} /></label>
    {#if p.deck}
      <p class="deck">
        <Icon name="layout-grid" size={18} />
        <span>{t("Shown on your {model} ({cols} × {rows}).", { model: p.deck.model, cols: p.deck.cols, rows: p.deck.rows })}</span>
        <button class="btn small" onclick={() => change(() => delete p.deck)}>{t("Unlink")}</button>
      </p>
    {:else}
      <label class="field">
        <span>{t("Switch to it when these apps are in front")}</span>
        <input value={(p.apps ?? []).join(", ")} placeholder="obs64.exe, Photoshop" onchange={(e) => apps(p.id, e.currentTarget.value)} />
        <small>{t("Program names, separated by commas. Leave empty to switch by hand.")}</small>
      </label>
    {/if}
    <div class="row">
      <label class="check"><input type="radio" name="default" checked={ed.layout?.defaultProfile === p.id} onchange={() => change((l) => (l.defaultProfile = p.id))} />{t("Default layout")}</label>
      <button class="btn danger" disabled={(ed.layout?.profiles.length ?? 0) <= 1} onclick={() => remove(p.id)}><Icon name="trash-2" size={18} />{t("Delete")}</button>
    </div>
  </div>
{/each}
<button class="btn" onclick={add}><Icon name="plus" size={18} />{t("Add layout")}</button>

<style>
  .muted {
    margin: 0;
    color: var(--muted);
  }
  .prof {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 16px;
    border-radius: var(--radius);
    background: var(--bg);
  }
  .deck {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 0;
    color: var(--muted);
  }
  .deck span {
    flex: 1;
  }
  .small {
    min-height: 32px;
    padding: 0 12px;
  }
  .row {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
</style>

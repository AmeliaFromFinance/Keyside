<script lang="ts">
  import Icon from "../components/Icon.svelte";
  import type { DeckLink, Profile } from "@keyside/protocol";
  import { ed, change, newId, deckDials } from "./state.svelte.ts";
  import { t } from "../lib/i18n.svelte.ts";

  const MAX_LAYOUTS = 20;
  const SLEEP = [0, 1, 2, 5, 10, 15, 30, 60];

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

  /** Another layout for the same deck, for example one that shows while a game is in front. */
  function another(p: Profile) {
    if (!ed.layout || !p.deck) return;
    const d = $state.snapshot(p.deck) as DeckLink;
    const id = newId("l", ed.layout.profiles.map((q) => q.id));
    const names = new Set(ed.layout.profiles.map((q) => q.name));
    let n = 2;
    while (names.has(`${d.model} ${n}`)) n++;
    const rows = d.rows + (deckDials(d) ? 1 : 0);
    change((l) => l.profiles.push({ id, name: `${d.model} ${n}`, homePage: "p1", pages: [{ id: "p1", name: "Main", cols: d.cols, rows, keys: [] }], deck: d }));
    ed.profile = id;
    ed.page = "p1";
  }

  /** Brightness and sleep belong to the deck, so every layout of it gets the same. */
  function deckSet(serial: string, fn: (d: DeckLink) => void) {
    change((l) => {
      for (const q of l.profiles) if (q.deck?.serial === serial) fn(q.deck);
    });
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
      {@const d = p.deck}
      {@const sleep = d.sleep ?? 0}
      <p class="deck">
        <Icon name="layout-grid" size={18} />
        <span>{t("Shown on your {model} ({cols} × {rows}).", { model: d.model, cols: d.cols, rows: d.rows })}</span>
        <button class="btn small" onclick={() => change(() => delete p.deck)}>{t("Unlink")}</button>
      </p>
      <div class="pair">
        <label class="field">
          <span>{t("Brightness")} · {d.brightness ?? 70}%</span>
          <input type="range" min="0" max="100" step="5" value={d.brightness ?? 70} onchange={(e) => deckSet(d.serial, (x) => (x.brightness = Number(e.currentTarget.value)))} />
        </label>
        <label class="field">
          <span>{t("Turn dark when not used")}</span>
          <select value={sleep} onchange={(e) => deckSet(d.serial, (x) => (Number(e.currentTarget.value) ? (x.sleep = Number(e.currentTarget.value)) : delete x.sleep))}>
            {#each SLEEP.includes(sleep) ? SLEEP : [...SLEEP, sleep] as m (m)}
              <option value={m}>{m ? t("After {n} min", { n: m }) : t("Never")}</option>
            {/each}
          </select>
        </label>
      </div>
      <label class="field">
        <span>{t("Show it on the deck when these apps are in front")}</span>
        <input value={(p.apps ?? []).join(", ")} placeholder="Game.exe" onchange={(e) => apps(p.id, e.currentTarget.value)} />
        <small>{t("Program names, separated by commas. The deck shows its layout without apps the rest of the time.")}</small>
      </label>
      <button class="btn small add" disabled={(ed.layout?.profiles.length ?? 0) >= MAX_LAYOUTS} onclick={() => another(p)}><Icon name="plus" size={16} />{t("Add another layout for this deck")}</button>
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
<button class="btn" disabled={(ed.layout?.profiles.length ?? 0) >= MAX_LAYOUTS} onclick={add}><Icon name="plus" size={18} />{t("Add layout")}</button>

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
  .pair {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }
  .add {
    align-self: flex-start;
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

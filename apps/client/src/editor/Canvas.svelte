<script lang="ts">
  import Grid from "../components/Grid.svelte";
  import Icon from "../components/Icon.svelte";
  import type { FormFactor, KeyKind, KeyView } from "@phonedeck/protocol";
  import { ed, page, profile, change, addKey, addPage, moveKey, placements, previewView, actionDef } from "./state.svelte.ts";
  import { t, mark } from "../lib/i18n.svelte.ts";
  import { deck } from "../lib/deck.svelte.ts";

  const factors: Array<[FormFactor, string]> = [
    ["phone-portrait", mark("Phone")],
    ["phone-landscape", mark("Phone, sideways")],
    ["tablet", mark("Tablet")],
    ["desktop", mark("Desktop browser")],
  ];

  const pg = $derived(page());
  const place = $derived(pg ? placements(pg, ed.ff) : null);
  const views = $derived.by((): KeyView[] => {
    if (!pg || !place) return [];
    const live = new Map((ed.live?.id === pg.id ? ed.live.keys : []).map((k) => [k.id, k]));
    return pg.keys.filter((k) => place.at.has(k.id)).map((k) => previewView(k, live.get(k.id), place.at.get(k.id)));
  });
  const armedName = $derived(ed.armed ? (actionDef(ed.armed)?.name ?? ed.armed.replace("kind:", "")) : "");
  const variant = $derived(!!pg?.variants?.[ed.ff]);
  const tabs = $derived((profile()?.pages ?? []).filter((p) => !p.folder || p.id === ed.page));

  function place_(x: number, y: number, id: string | null) {
    if (!pg) return;
    if (ed.moving) {
      if (variant) moveInVariant(ed.moving, x, y);
      else moveKey(ed.moving, x, y);
      ed.moving = null;
      return;
    }
    if (!id || variant) return;
    if (id.startsWith("kind:")) {
      const kind = id.slice(5) as KeyKind;
      if (kind === "folder") {
        const target = addPage(true);
        addKey(x, y, null, "folder");
        change(() => {
          const k = pg.keys.find((q) => q.id === ed.key);
          if (k) {
            k.target = target;
            k.label = "Folder";
          }
        });
      } else {
        addKey(x, y, null, kind);
        change(() => {
          const k = pg.keys.find((q) => q.id === ed.key);
          if (!k) return;
          if (kind === "slider") {
            k.label = "Slider";
            k.slider = { min: 0, max: 1 };
          }
          if (kind === "widget") {
            k.label = "Text";
            k.widget = { text: "{user.message}" };
          }
          if (kind === "button") k.label = "Multi-action";
        });
      }
    } else addKey(x, y, id);
    ed.armed = null;
  }

  function moveInVariant(id: string, x: number, y: number) {
    change(() => {
      const v = pg?.variants?.[ed.ff];
      const cur = v?.at[id];
      if (v && cur) v.at[id] = [x, y, cur[2], cur[3]];
    });
  }

  function drop(e: DragEvent, x: number, y: number) {
    e.preventDefault();
    const action = e.dataTransfer?.getData("application/x-phonedeck-action");
    const key = e.dataTransfer?.getData("application/x-phonedeck-key");
    if (key) {
      ed.moving = key;
      place_(x, y, null);
    } else place_(x, y, action || ed.armed);
  }

  function customize() {
    if (!pg || !place) return;
    change(() => {
      pg.variants ??= {};
      pg.variants[ed.ff] = { cols: place.cols, rows: place.rows, at: Object.fromEntries([...place.at.entries()]) };
    });
  }

  function resetVariant() {
    change(() => {
      if (pg?.variants) delete pg.variants[ed.ff];
    });
  }

  function setGrid(cols: number, rows: number) {
    if (!pg) return;
    change(() => {
      if (variant && pg.variants?.[ed.ff]) {
        const v = pg.variants[ed.ff]!;
        v.cols = cols;
        v.rows = rows;
        for (const [id, p] of Object.entries(v.at)) if (p[0] + p[2] > cols || p[1] + p[3] > rows) delete v.at[id];
      } else {
        pg.cols = cols;
        pg.rows = rows;
        pg.keys = pg.keys.filter((k) => k.x + (k.w ?? 1) <= cols && k.y + (k.h ?? 1) <= rows);
      }
    });
  }

  function keydrag(e: DragEvent) {
    const id = (e.target as HTMLElement)?.closest?.("[data-key]")?.getAttribute("data-key");
    if (id) e.dataTransfer?.setData("application/x-phonedeck-key", id);
  }
</script>

<section class="canvas">
  <div class="bar">
    <div class="tabs" role="tablist">
      {#each tabs as p (p.id)}
        <button role="tab" class="tab" class:on={p.id === ed.page} aria-selected={p.id === ed.page} onclick={() => ((ed.page = p.id), (ed.key = null))}>
          {#if p.folder}<Icon name="folder" size={16} />{/if}{p.name}
        </button>
      {/each}
      <button class="tab add" aria-label={t("Add page")} title={t("Add page")} onclick={() => addPage()}><Icon name="plus" size={18} /></button>
    </div>
    <div class="controls">
      {#if pg && place}
        <label>
          <span>{t("Grid")}</span>
          <select value={`${place.cols}x${place.rows}`} onchange={(e) => setGrid(...(e.currentTarget.value.split("x").map(Number) as [number, number]))}>
            {#each [[2, 2], [3, 2], [2, 3], [3, 3], [3, 4], [4, 3], [3, 5], [5, 3], [4, 4], [4, 5], [5, 4], [5, 5], [6, 4], [8, 4], [6, 6], [8, 5], [10, 6], [12, 8]] as [c, r] (`${c}x${r}`)}
              <option value={`${c}x${r}`}>{c} × {r}</option>
            {/each}
            {#if ![2, 3, 4, 5, 6, 8, 10, 12].includes(place.cols)}<option value={`${place.cols}x${place.rows}`}>{place.cols} × {place.rows}</option>{/if}
          </select>
        </label>
      {/if}
      <label>
        <span>{t("Preview")}</span>
        <select bind:value={ed.ff}>
          {#each factors as [ff, name] (ff)}<option value={ff}>{t(name)}</option>{/each}
        </select>
      </label>
    </div>
  </div>

  {#if pg && place}
    <div class="stage">
      <div class="device ff-{ed.ff}" ondragstart={keydrag} role="presentation">
        <div class="device-head">
          <b>{pg.name}</b>
          <span class="live"><span class="dot" class:ok={deck.conn.s === "open"}></span>{t("live preview")}</span>
        </div>
        <Grid
          cols={place.cols}
          rows={place.rows}
          keys={views}
          preview
          selected={ed.key}
          onselect={(id) => {
            if (ed.moving === id) ed.moving = null;
            ed.key = id;
            ed.armed = null;
          }}
        >
          {#snippet empty(x, y)}
            <button
              class="drop"
              class:ready={!!ed.armed || !!ed.moving}
              ondragover={(e) => e.preventDefault()}
              ondrop={(e) => drop(e, x, y)}
              onclick={() => (ed.armed || ed.moving ? place_(x, y, ed.armed) : ((ed.key = null), undefined))}
              aria-label={ed.moving ? t("Move here") : ed.armed ? t("Drop {name} here", { name: armedName }) : t("Empty cell")}
            >
              {#if ed.moving}
                <Icon name="move" size={20} /><span>{t("Move here")}</span>
              {:else if ed.armed}
                <Icon name="plus" size={20} /><span>{t("Drop {name}", { name: t(armedName) })}</span>
              {/if}
            </button>
          {/snippet}
        </Grid>
      </div>
      <p class="caption">
        {#if variant}
          {t("This screen has its own arrangement.")} <button class="link" onclick={resetVariant}>{t("Use the main one")}</button>
        {:else if ed.ff !== "phone-portrait"}
          {t("Arranged automatically from the phone layout.")} <button class="link" onclick={customize}>{t("Arrange this screen yourself")}</button>
        {:else}
          {t("States are live from your apps. Changes reach devices when you push them.")}
        {/if}
      </p>
    </div>
  {/if}
</section>

<style>
  .canvas {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    background: var(--bg);
  }
  .bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: 12px;
    padding: 16px 24px;
    border-bottom: 1px solid var(--line-soft);
  }
  .tabs {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }
  .tab {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-height: 40px;
    padding: 0 16px;
    border-radius: var(--radius-sm);
    border: 1px solid transparent;
    background: none;
    color: var(--muted);
    font-weight: 500;
  }
  .tab:hover {
    color: var(--text);
  }
  .tab.on {
    color: var(--text);
    background: var(--accent-bg);
    border-color: var(--accent);
  }
  .tab.add {
    border: 1px dashed var(--line);
    padding: 0 12px;
  }
  .controls {
    display: flex;
    gap: 16px;
    flex-wrap: wrap;
  }
  .controls label {
    display: flex;
    align-items: center;
    gap: 10px;
    color: var(--muted);
  }
  .controls select {
    min-height: 40px;
    padding: 0 10px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--line);
    background: var(--surface);
  }
  .stage {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 16px;
    padding: 24px;
  }
  .device {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 24px 20px 28px;
    border-radius: 40px;
    border: 1px solid var(--line);
    background: #16151c;
    height: min(100%, 760px);
    aspect-ratio: 9 / 16;
    max-width: 100%;
  }
  .device.ff-phone-landscape {
    aspect-ratio: 16 / 9;
    height: auto;
    width: min(100%, 760px);
  }
  .device.ff-tablet {
    aspect-ratio: 4 / 3;
    height: auto;
    width: min(100%, 860px);
    border-radius: 28px;
  }
  .device.ff-desktop {
    aspect-ratio: 16 / 10;
    height: auto;
    width: min(100%, 980px);
    border-radius: 16px;
  }
  .device-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 0 6px;
  }
  .live {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--muted);
    font-size: 0.85rem;
  }
  .drop {
    width: 100%;
    height: 100%;
    border-radius: var(--radius);
    border: 1px dashed transparent;
    background: transparent;
    color: var(--accent);
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 4px;
    font-weight: 600;
    font-size: 0.85rem;
    text-align: center;
  }
  .drop:hover,
  .drop.ready {
    border-color: var(--line);
  }
  .drop.ready {
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 5%, transparent);
  }
  .caption {
    margin: 0;
    color: var(--muted);
    text-align: center;
  }
  .link {
    border: none;
    background: none;
    color: var(--accent);
    text-decoration: underline;
    padding: 0;
  }
</style>

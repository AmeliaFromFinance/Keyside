<script lang="ts">
  import Grid from "../components/Grid.svelte";
  import Icon from "../components/Icon.svelte";
  import type { FormFactor, KeyKind, KeyView } from "@keyside/protocol";
  import ContextMenu, { type MenuItem } from "../components/ContextMenu.svelte";
  import { ed, page, profile, change, addKey, addPage, moveKey, deleteKey, duplicateKey, copyKey, pasteKey, clipboard, swapKeys, notify, notice, placements, previewView, actionDef } from "./state.svelte.ts";
  import { t, mark } from "../lib/i18n.svelte.ts";
  import { deck, req } from "../lib/deck.svelte.ts";
  import { haptic } from "../lib/native.ts";

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

  function moveInVariant(id: string, x: number, y: number): boolean {
    const v = pg?.variants?.[ed.ff];
    const cur = v?.at[id];
    if (!v || !cur || !variantFits(id, x, y, cur[2], cur[3])) return false;
    change(() => (v.at[id] = [x, y, cur[2], cur[3]]));
    return true;
  }

  /** Same as fits() in the state, for this screen's own arrangement. */
  function variantFits(id: string, x: number, y: number, w: number, h: number): boolean {
    const v = pg?.variants?.[ed.ff];
    if (!v || x < 0 || y < 0 || x + w > v.cols || y + h > v.rows) return false;
    return Object.entries(v.at).every(([other, [ox, oy, ow, oh]]) => other === id || x + w <= ox || ox + ow <= x || y + h <= oy || oy + oh <= y);
  }

  function swapInVariant(a: string, b: string): boolean {
    const v = pg?.variants?.[ed.ff];
    const [pa, pb] = [v?.at[a], v?.at[b]];
    if (!v || !pa || !pb || pa[2] !== pb[2] || pa[3] !== pb[3]) return false;
    change(() => ([v.at[a], v.at[b]] = [[pb[0], pb[1], pa[2], pa[3]], [pa[0], pa[1], pb[2], pb[3]]]));
    return true;
  }

  // Dragging and long-press. Pointer events work the same for mouse, touch and pen.
  type Target = { key: string } | { cell: [number, number] } | null;
  const LONG_PRESS = 500;
  const DRAG_START = 8;
  let gesture: { target: Target; sx: number; sy: number; pointer: number; timer: ReturnType<typeof setTimeout> | null; dragging: boolean; menu: boolean } | null = null;
  let drag = $state<{ id: string; label: string; x: number; y: number; over: Target } | null>(null);
  let menu = $state<{ x: number; y: number; title?: string; items: MenuItem[] } | null>(null);
  /** Swallows the click that ends a drag or a long-press, so it doesn't also select or place. */
  let swallowClick = false;

  function hit(x: number, y: number): Target {
    const el = document.elementFromPoint(x, y) as HTMLElement | null;
    const key = el?.closest<HTMLElement>("[data-key]")?.dataset.key;
    if (key) return { key };
    const cell = el?.closest<HTMLElement>("[data-cell]")?.dataset.cell;
    return cell ? { cell: cell.split(",").map(Number) as [number, number] } : null;
  }

  function pdown(e: PointerEvent) {
    if (e.button !== 0 || ed.armed || ed.moving) return;
    const target = hit(e.clientX, e.clientY);
    if (!target) return;
    const g = { target, sx: e.clientX, sy: e.clientY, pointer: e.pointerId, timer: null as ReturnType<typeof setTimeout> | null, dragging: false, menu: false };
    // Touch has no right-click, so holding still opens the menu.
    if (e.pointerType !== "mouse")
      g.timer = setTimeout(() => {
        if (gesture !== g || g.dragging) return;
        g.menu = true;
        haptic("medium");
        openMenu(target, g.sx, g.sy);
      }, LONG_PRESS);
    gesture = g;
  }

  function pmove(e: PointerEvent) {
    const g = gesture;
    if (!g || e.pointerId !== g.pointer || g.menu) return;
    if (!g.dragging) {
      if (!g.target || !("key" in g.target) || Math.hypot(e.clientX - g.sx, e.clientY - g.sy) < DRAG_START) return;
      const k = pg?.keys.find((q) => q.id === (g.target as { key: string }).key);
      if (!k) return;
      if (g.timer) clearTimeout(g.timer);
      g.dragging = true;
      (e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId);
      drag = { id: k.id, label: k.label, x: e.clientX, y: e.clientY, over: null };
    }
    if (!drag) return;
    drag.x = e.clientX;
    drag.y = e.clientY;
    const over = hit(e.clientX, e.clientY);
    drag.over = over && "key" in over && over.key === drag.id ? null : over;
  }

  function pup(e: PointerEvent, cancel = false) {
    const g = gesture;
    if (!g || e.pointerId !== g.pointer) return;
    if (g.timer) clearTimeout(g.timer);
    gesture = null;
    if (g.menu || g.dragging) {
      // The click, if the browser sends one, comes right after this. Don't eat a later one.
      swallowClick = true;
      setTimeout(() => (swallowClick = false), 50);
    }
    if (!g.dragging || !drag) return;
    const { id, over } = drag;
    drag = null;
    if (cancel || !over) return;
    let ok: boolean;
    if ("cell" in over) ok = variant ? moveInVariant(id, ...over.cell) : moveKey(id, ...over.cell);
    else ok = variant ? swapInVariant(id, over.key) : swapKeys(id, over.key);
    if (ok) haptic("light");
    else notify("cell" in over ? t("It doesn't fit there") : t("These keys don't fit in each other's place"));
  }

  function swallow(e: MouseEvent) {
    if (!swallowClick) return;
    swallowClick = false;
    e.stopPropagation();
    e.preventDefault();
  }

  function contextmenu(e: MouseEvent) {
    const target = hit(e.clientX, e.clientY);
    if (!target) return;
    e.preventDefault();
    // Android also sends this on a long press. The timer may have opened the menu already.
    if (gesture) {
      if (gesture.timer) clearTimeout(gesture.timer);
      if (gesture.menu) return;
      gesture.menu = true;
    }
    openMenu(target, e.clientX, e.clientY);
  }

  const mod = /Mac|iPhone|iPad/.test(navigator.platform) ? "⌘" : "Ctrl+";

  function openMenu(target: Target, x: number, y: number) {
    if (!target || !pg) return;
    if ("key" in target) {
      const k = pg.keys.find((q) => q.id === target.key);
      if (!k) return;
      ed.key = k.id;
      const steps = [...k.actions, ...(k.offActions ?? [])];
      menu = {
        x,
        y,
        title: k.label,
        items: [
          { label: t("Edit"), icon: "pencil", run: () => (ed.key = k.id) },
          { label: t("Test run"), icon: "play", disabled: !k.actions.length || steps.some((a) => a.review), run: () => runNow(k.label, $state.snapshot(k.actions)) },
          { label: t("Duplicate"), icon: "copy-plus", hint: `${mod}D`, disabled: variant, run: () => duplicateKey(k.id) },
          { label: t("Copy"), icon: "copy", hint: `${mod}C`, run: () => copyKey(k.id) },
          { label: t("Cut"), icon: "scissors", hint: `${mod}X`, run: () => copyKey(k.id, true) },
          { label: t("Move"), icon: "move", run: () => (ed.moving = k.id) },
          { label: t("Delete"), icon: "trash-2", hint: t("Del"), danger: true, run: () => deleteKey(k.id) },
        ],
      };
    } else {
      const [cx, cy] = target.cell;
      menu = {
        x,
        y,
        items: [
          { label: t("Paste"), icon: "clipboard-paste", hint: `${mod}V`, disabled: !clipboard.key || variant, run: () => pasteKey(cx, cy) },
          { label: t("New key"), icon: "plus", disabled: variant, run: () => addKey(cx, cy, null) },
          { label: t("New folder"), icon: "folder", disabled: variant, run: () => place_(cx, cy, "kind:folder") },
        ],
      };
    }
  }

  async function runNow(label: string, actions: unknown[]) {
    try {
      await req("steps.test", { actions });
      notify(t("Ran {name}", { name: label }));
    } catch (e: any) {
      notify(e.message);
    }
  }

  /** Actions dragged in from the palette. */
  function drop(e: DragEvent, x: number, y: number) {
    e.preventDefault();
    place_(x, y, e.dataTransfer?.getData("application/x-keyside-action") || ed.armed);
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
      <div
        class="device ff-{ed.ff}"
        class:dragging={!!drag}
        role="presentation"
        onpointerdown={pdown}
        onpointermove={pmove}
        onpointerup={(e) => pup(e)}
        onpointercancel={(e) => pup(e, true)}
        onclickcapture={swallow}
        oncontextmenu={contextmenu}
      >
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
          dim={drag?.id}
          onselect={(id) => {
            if (ed.moving === id) ed.moving = null;
            ed.key = id;
            ed.armed = null;
          }}
        >
          {#snippet empty(x, y)}
            <button
              class="drop"
              data-cell="{x},{y}"
              class:ready={!!ed.armed || !!ed.moving || !!drag}
              class:target={!!drag?.over && "cell" in drag.over && drag.over.cell[0] === x && drag.over.cell[1] === y}
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
        {#if notice.text}
          <span class="notice" role="status">{notice.text}</span>
          {#if notice.action}<button class="link" onclick={() => (notice.action?.run(), (notice.text = ""))}>{notice.action.label}</button>{/if}
        {:else if variant}
          {t("This screen has its own arrangement.")} <button class="link" onclick={resetVariant}>{t("Use the main one")}</button>
        {:else if ed.ff !== "phone-portrait"}
          {t("Arranged automatically from the phone layout.")} <button class="link" onclick={customize}>{t("Arrange this screen yourself")}</button>
        {:else}
          {t("States are live from your apps. Changes reach devices when you push them.")}
        {/if}
      </p>
      <p class="tip">{t("Drag a key to move it, or onto another key to swap them. Right-click or press and hold for more.")}</p>
    </div>
  {/if}
</section>

{#if drag}
  <div class="ghost" style:left="{drag.x}px" style:top="{drag.y}px" aria-hidden="true">
    <Icon name="move" size={16} />
    {#if drag.over && "key" in drag.over}
      {t("Swap with {name}", { name: pg?.keys.find((k) => k.id === (drag?.over as { key: string }).key)?.label ?? "" })}
    {:else if drag.over}
      {t("Move {name} here", { name: drag.label })}
    {:else}
      {drag.label}
    {/if}
  </div>
{/if}

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} title={menu.title} items={menu.items} onclose={() => (menu = null)} />
{/if}

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
    /* Holding a finger still should open our menu, not the browser's. */
    touch-action: none;
    -webkit-touch-callout: none;
    user-select: none;
  }
  .device.dragging {
    cursor: grabbing;
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
  .drop.target {
    background: color-mix(in srgb, var(--accent) 18%, transparent);
  }
  .caption,
  .tip {
    margin: 0;
    color: var(--muted);
    text-align: center;
  }
  .tip {
    font-size: 0.8rem;
  }
  .notice {
    color: var(--text);
  }
  .ghost {
    position: fixed;
    z-index: 40;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 12px;
    border-radius: 999px;
    background: var(--accent);
    color: var(--bg);
    font-size: 0.85rem;
    font-weight: 600;
    white-space: nowrap;
    /* Off the finger, so it stays visible on touch screens. */
    transform: translate(12px, -140%);
    pointer-events: none;
  }
  .link {
    border: none;
    background: none;
    color: var(--accent);
    text-decoration: underline;
    padding: 0;
  }
</style>

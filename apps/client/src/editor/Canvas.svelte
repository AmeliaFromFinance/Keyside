<script lang="ts">
  import Grid from "../components/Grid.svelte";
  import Icon from "../components/Icon.svelte";
  import type { FormFactor, KeyKind, KeyView } from "@keyside/protocol";
  import ContextMenu, { type MenuItem } from "../components/ContextMenu.svelte";
  import Key from "../components/Key.svelte";
  import {
    ed,
    page,
    profile,
    change,
    addKey,
    addPage,
    moveKey,
    deleteKeys,
    duplicateKeys,
    copyKeys,
    pasteKey,
    clipboard,
    swapKeys,
    shiftKeys,
    moveToPage,
    selection,
    pick,
    clearSelection,
    onScreen,
    notify,
    notice,
    placements,
    previewView,
    actionDef,
  } from "./state.svelte.ts";
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
      if (!moveKey(ed.moving, x, y)) notify(t("It doesn't fit there"));
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

  // Dragging, long-press and right-click. Pointer events work the same for mouse, touch and pen.
  type Target = { tab: string } | { cell: [number, number]; key: string | null } | null;
  const LONG_PRESS = 500;
  const DRAG_START = 8;
  let deviceEl = $state<HTMLElement>();
  let gesture: { target: Target; sx: number; sy: number; pointer: number; timer: ReturnType<typeof setTimeout> | null; dragging: boolean; menu: boolean } | null = null;
  let drag = $state<{
    ids: string[];
    primary: string;
    /** Cell of the primary key the pointer grabbed, so it stays under the finger. */
    grab: [number, number];
    x: number;
    y: number;
    over: Target;
    ok: boolean;
    size: [number, number];
  } | null>(null);
  let menu = $state<{ x: number; y: number; title?: string; items: MenuItem[] } | null>(null);
  /** Swallows the click that ends a drag or a long-press, so it doesn't also select or place. */
  let swallowClick = false;
  const chosen = $derived(selection());

  /** The grid cell under a point, worked out from the grid's own column and row sizes. */
  function cellAt(px: number, py: number): [number, number] | null {
    const g = deviceEl?.querySelector<HTMLElement>(".grid");
    if (!g) return null;
    const r = g.getBoundingClientRect();
    const cs = getComputedStyle(g);
    const find = (track: string, gap: number, off: number) => {
      let pos = 0;
      const sizes = track.split(" ").map(parseFloat);
      for (let i = 0; i < sizes.length; i++) {
        if (off >= pos - gap / 2 && off < pos + sizes[i] + gap / 2) return i;
        pos += sizes[i] + gap;
      }
      return -1;
    };
    const x = find(cs.gridTemplateColumns, parseFloat(cs.columnGap) || 0, px - r.left);
    const y = find(cs.gridTemplateRows, parseFloat(cs.rowGap) || 0, py - r.top);
    return x < 0 || y < 0 ? null : [x, y];
  }

  function keyAt([cx, cy]: [number, number]): string | null {
    for (const [id, [x, y, w, h]] of place?.at ?? []) if (cx >= x && cx < x + w && cy >= y && cy < y + h) return id;
    return null;
  }

  function hit(px: number, py: number): Target {
    const el = document.elementFromPoint(px, py) as HTMLElement | null;
    const tab = el?.closest<HTMLElement>("[data-tab]")?.dataset.tab;
    if (tab) return { tab };
    if (!el || !deviceEl?.contains(el)) return null;
    const cell = cellAt(px, py);
    return cell ? { cell, key: keyAt(cell) } : null;
  }

  function pdown(e: PointerEvent) {
    if (e.button !== 0 || ed.armed || ed.moving) return;
    const target = hit(e.clientX, e.clientY);
    if (!target || "tab" in target) return;
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
      const t0 = g.target;
      if (!t0 || "tab" in t0 || !t0.key || Math.hypot(e.clientX - g.sx, e.clientY - g.sy) < DRAG_START) return;
      const at = onScreen(t0.key);
      if (!at) return;
      if (g.timer) clearTimeout(g.timer);
      g.dragging = true;
      (e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId);
      // Dragging a selected key takes the whole selection along. Any other key goes alone.
      const ids = chosen.length > 1 && chosen.includes(t0.key) ? chosen : [t0.key];
      if (ids.length === 1) pick(t0.key);
      const el = deviceEl?.querySelector<HTMLElement>(`[data-key="${CSS.escape(t0.key)}"]`)?.getBoundingClientRect();
      const scale = Math.min(0.7, 120 / Math.max(el?.width ?? 1, el?.height ?? 1));
      drag = {
        ids,
        primary: t0.key,
        grab: [t0.cell[0] - at[0], t0.cell[1] - at[1]],
        x: e.clientX,
        y: e.clientY,
        over: null,
        ok: false,
        size: [Math.round((el?.width ?? 96) * scale), Math.round((el?.height ?? 96) * scale)],
      };
    }
    if (!drag) return;
    drag.x = e.clientX;
    drag.y = e.clientY;
    drag.over = hit(e.clientX, e.clientY);
    drag.ok = plan(drag, true);
  }

  /** Carries out the drop for the current target, or with `dry`, only checks it would work. */
  function plan(d: NonNullable<typeof drag>, dry: boolean): boolean {
    const over = d.over;
    if (!over) return false;
    if ("tab" in over) return over.tab !== ed.page && moveToPage(d.ids, over.tab, dry);
    // A single key dropped on another one trades places with it.
    if (d.ids.length === 1 && over.key && over.key !== d.primary) return swapKeys(d.primary, over.key, dry);
    const at = onScreen(d.primary);
    if (!at) return false;
    const [dx, dy] = [over.cell[0] - d.grab[0] - at[0], over.cell[1] - d.grab[1] - at[1]];
    if (!dx && !dy) return false;
    return shiftKeys(d.ids, dx, dy, dry);
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
    const p = pending;
    pending = null;
    if (p && !g.dragging && !g.menu && !cancel) pick(p.id, p.add);
    const d = drag;
    drag = null;
    if (!g.dragging || !d || cancel || !d.over) return;
    if (plan(d, false)) haptic("light");
    else if (!("tab" in d.over)) {
      const at = onScreen(d.primary);
      const still = "cell" in d.over && at && d.over.cell[0] - d.grab[0] === at[0] && d.over.cell[1] - d.grab[1] === at[1];
      if (!still) notify(d.ids.length === 1 && d.over.key && d.over.key !== d.primary ? t("These keys don't fit in each other's place") : t("It doesn't fit there"));
    }
  }

  function swallow(e: MouseEvent) {
    if (!swallowClick) return;
    swallowClick = false;
    e.stopPropagation();
    e.preventDefault();
  }

  function contextmenu(e: MouseEvent) {
    const target = hit(e.clientX, e.clientY);
    if (!target || "tab" in target) return;
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

  /** Other pages of this layout, as menu entries that move keys there. */
  function pageItems(ids: string[]): MenuItem[] {
    return (profile()?.pages ?? [])
      .filter((p) => p.id !== ed.page)
      .map((p) => ({ label: p.name, icon: p.folder ? "folder" : "file", run: () => moveToPage(ids, p.id) }));
  }

  function openMenu(target: Target, x: number, y: number) {
    if (!target || "tab" in target || !pg) return;
    if (target.key) {
      const k = pg.keys.find((q) => q.id === target.key);
      if (!k) return;
      if (!chosen.includes(k.id)) pick(k.id);
      const ids = selection();
      const many = ids.length > 1;
      const steps = [...k.actions, ...(k.offActions ?? [])];
      const moveTo: MenuItem = { label: t("Move to page"), icon: "folder-input", disabled: !pageItems(ids).length, run: () => (menu = { x, y, title: t("Move to page"), items: pageItems(ids) }) };
      menu = {
        x,
        y,
        title: many ? t("{n} keys", { n: ids.length }) : k.label,
        items: many
          ? [
              { label: t("Duplicate"), icon: "copy-plus", hint: `${mod}D`, disabled: variant, run: () => duplicateKeys(ids) },
              { label: t("Copy"), icon: "copy", hint: `${mod}C`, run: () => copyKeys(ids) },
              { label: t("Cut"), icon: "scissors", hint: `${mod}X`, run: () => copyKeys(ids, true) },
              moveTo,
              { label: t("Clear selection"), icon: "x", hint: "Esc", run: clearSelection },
              { label: t("Delete {n} keys", { n: ids.length }), icon: "trash-2", hint: t("Del"), danger: true, run: () => deleteKeys(ids) },
            ]
          : [
              { label: t("Edit"), icon: "pencil", run: () => pick(k.id) },
              { label: t("Test run"), icon: "play", disabled: !k.actions.length || steps.some((a) => a.review), run: () => runNow(k.label, $state.snapshot(k.actions)) },
              { label: t("Select more"), icon: "square-check", run: () => ((ed.picking = true), pick(k.id)) },
              { label: t("Duplicate"), icon: "copy-plus", hint: `${mod}D`, disabled: variant, run: () => duplicateKeys([k.id]) },
              { label: t("Copy"), icon: "copy", hint: `${mod}C`, run: () => copyKeys([k.id]) },
              { label: t("Cut"), icon: "scissors", hint: `${mod}X`, run: () => copyKeys([k.id], true) },
              moveTo,
              { label: t("Delete"), icon: "trash-2", hint: t("Del"), danger: true, run: () => deleteKeys([k.id]) },
            ],
      };
    } else {
      const [cx, cy] = target.cell;
      menu = {
        x,
        y,
        items: [
          { label: t("Paste"), icon: "clipboard-paste", hint: `${mod}V`, disabled: !clipboard.keys.length || variant, run: () => pasteKey(cx, cy) },
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

  /** What the drag would do, shown next to the finger. */
  const dragText = $derived.by(() => {
    if (!drag) return "";
    const n = drag.ids.length;
    const name = n > 1 ? t("{n} keys", { n }) : (pg?.keys.find((k) => k.id === drag!.primary)?.label ?? "");
    const over = drag.over;
    if (!over) return name;
    if ("tab" in over) {
      const dest = profile()?.pages.find((p) => p.id === over.tab);
      if (over.tab === ed.page) return name;
      return drag.ok ? t("Move {name} to {page}", { name, page: dest?.name ?? "" }) : t("{page} has no room for {name}", { page: dest?.name ?? "", name });
    }
    if (!drag.ok) return t("It doesn't fit there");
    if (n === 1 && over.key && over.key !== drag.primary) return t("Swap with {name}", { name: pg?.keys.find((k) => k.id === over.key)?.label ?? "" });
    return t("Move {name} here", { name });
  });
  const dragView = $derived(drag ? views.find((v) => v.id === drag!.primary) : undefined);
  /** Cells the drop would cover, to light them up. */
  const landing = $derived.by(() => {
    const out = new Set<string>();
    if (!drag?.ok || !drag.over || "tab" in drag.over) return out;
    const at = onScreen(drag.primary);
    if (!at) return out;
    const swapWith = drag.ids.length === 1 && drag.over.key && drag.over.key !== drag.primary ? drag.over.key : null;
    const [dx, dy] = swapWith ? [0, 0] : [drag.over.cell[0] - drag.grab[0] - at[0], drag.over.cell[1] - drag.grab[1] - at[1]];
    for (const id of swapWith ? [swapWith] : drag.ids) {
      const [x, y, w, h] = swapWith ? at : onScreen(id)!;
      for (let j = 0; j < h; j++) for (let i = 0; i < w; i++) out.add(`${x + dx + i},${y + dy + j}`);
    }
    return out;
  });

  /**
   * A press may turn into a drag or a long-press, which shouldn't change the selection.
   * So pointer presses select on release.
   */
  let pending: { id: string; add: boolean } | null = null;

  function select(id: string, e?: PointerEvent | KeyboardEvent) {
    pending = null;
    const placing = !!ed.armed || !!ed.moving;
    if (ed.moving === id) ed.moving = null;
    ed.armed = null;
    const add = ed.picking || !!(e && (e.shiftKey || e.ctrlKey || e.metaKey));
    if (e instanceof PointerEvent && !placing) pending = { id, add };
    else pick(id, add);
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
        <button
          role="tab"
          class="tab"
          class:on={p.id === ed.page}
          class:droptarget={!!drag?.over && "tab" in drag.over && drag.over.tab === p.id && p.id !== ed.page}
          data-tab={p.id}
          aria-selected={p.id === ed.page}
          onclick={() => ((ed.page = p.id), clearSelection())}
        >
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
        bind:this={deviceEl}
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
          selected={chosen}
          dim={drag?.ids}
          onselect={select}
        >
          {#snippet empty(x, y)}
            <button
              class="drop"
              data-cell="{x},{y}"
              class:ready={!!ed.armed || !!ed.moving || !!drag}
              class:target={landing.has(`${x},${y}`)}
              ondragover={(e) => e.preventDefault()}
              ondrop={(e) => drop(e, x, y)}
              onclick={() => (ed.armed || ed.moving ? place_(x, y, ed.armed) : clearSelection())}
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
        {#if ed.picking}
          <span class="notice" role="status">{t("Tap keys to add or remove them. {n} selected.", { n: chosen.length })}</span>
          <button class="link" onclick={() => (ed.picking = false)}>{t("Done")}</button>
        {:else if notice.text}
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
      <p class="tip">
        {chosen.length > 1
          ? t("Drag one of the selected keys to move them all, or onto a page tab to move them there.")
          : t("Drag a key to move it, onto another key to swap them, or onto a page tab. Shift-click to select several. Right-click or press and hold for more.")}
      </p>
    </div>
  {/if}
</section>

{#if drag}
  <div class="ghost" class:bad={!!drag.over && !drag.ok && !("tab" in drag.over && drag.over.tab === ed.page)} style:left="{drag.x}px" style:top="{drag.y}px" aria-hidden="true">
    {#if dragView}
      <div class="mini" style:width="{drag.size[0]}px" style:height="{drag.size[1]}px">
        <Key view={dragView} preview />
        {#if drag.ids.length > 1}<span class="count">{drag.ids.length}</span>{/if}
      </div>
    {/if}
    <span class="what">{dragText}</span>
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
  .tab.droptarget {
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 18%, transparent);
    color: var(--text);
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
    flex-direction: column;
    align-items: flex-start;
    gap: 6px;
    /* Above and right of the finger, so the hand doesn't cover it. */
    transform: translate(14px, calc(-100% - 14px));
    pointer-events: none;
  }
  .mini {
    position: relative;
    container-type: size;
    display: grid;
    /* Keys are see-through by design. Underneath the preview is whatever is on the page. */
    border-radius: var(--radius);
    background: #16151c;
    box-shadow: 0 10px 24px rgb(0 0 0 / 0.5);
  }
  .count {
    position: absolute;
    top: -8px;
    right: -8px;
    min-width: 24px;
    height: 24px;
    padding: 0 6px;
    border-radius: 12px;
    background: var(--accent);
    color: var(--bg);
    font-size: 0.8rem;
    font-weight: 700;
    line-height: 24px;
    text-align: center;
  }
  .what {
    padding: 4px 10px;
    border-radius: 999px;
    background: var(--accent);
    color: var(--bg);
    font-size: 0.85rem;
    font-weight: 600;
    white-space: nowrap;
  }
  .ghost.bad .what {
    background: var(--danger, #e5484d);
    color: #fff;
  }
  .link {
    border: none;
    background: none;
    color: var(--accent);
    text-decoration: underline;
    padding: 0;
  }
</style>

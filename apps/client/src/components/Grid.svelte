<script lang="ts">
  import type { Snippet } from "svelte";
  import type { KeyView } from "@keyside/protocol";
  import Key from "./Key.svelte";

  let {
    cols,
    rows,
    keys,
    locked = false,
    preview = false,
    selected = null,
    ondown,
    onup,
    onslide,
    onselect,
    empty,
  }: {
    cols: number;
    rows: number;
    keys: KeyView[];
    locked?: boolean;
    preview?: boolean;
    selected?: string | null;
    ondown?: (id: string) => void;
    onup?: (id: string, cancel: boolean) => void;
    onslide?: (id: string, v: number) => void;
    onselect?: (id: string) => void;
    /** Editor only: what to draw in an empty cell. */
    empty?: Snippet<[number, number]>;
  } = $props();

  const free = $derived.by(() => {
    if (!empty) return [];
    const used = new Set<string>();
    for (const k of keys) for (let dy = 0; dy < k.h; dy++) for (let dx = 0; dx < k.w; dx++) used.add(`${k.x + dx},${k.y + dy}`);
    const out: Array<[number, number]> = [];
    for (let y = 0; y < rows; y++) for (let x = 0; x < cols; x++) if (!used.has(`${x},${y}`)) out.push([x, y]);
    return out;
  });
</script>

<div class="wrap">
  <div class="grid" style:--cols={cols} style:--rows={rows}>
    {#each keys as k (k.id)}
      <div class="cell" style:grid-column="{k.x + 1} / span {k.w}" style:grid-row="{k.y + 1} / span {k.h}">
        <Key view={k} disabled={locked} {preview} selected={selected === k.id} {ondown} {onup} {onslide} {onselect} />
      </div>
    {/each}
    {#each free as [x, y] (`${x},${y}`)}
      <div class="cell" style:grid-column={x + 1} style:grid-row={y + 1}>{@render empty?.(x, y)}</div>
    {/each}
  </div>
</div>

<style>
  .wrap {
    container-type: size;
    flex: 1;
    min-height: 0;
    min-width: 0;
    display: grid;
    place-items: center;
  }
  .grid {
    --gap: clamp(8px, 2.4cqmin, 16px);
    --col: calc((100cqw - (var(--cols) - 1) * var(--gap)) / var(--cols));
    --row: calc((100cqh - (var(--rows) - 1) * var(--gap)) / var(--rows));
    --h: min(var(--row), var(--col));
    --w: min(var(--col), var(--h) * 1.12);
    display: grid;
    grid-template-columns: repeat(var(--cols), var(--w));
    grid-template-rows: repeat(var(--rows), var(--h));
    gap: var(--gap);
  }
  .cell {
    /* Keys size their padding and type from their own cell. */
    container-type: size;
    display: grid;
    min-width: 0;
    min-height: 0;
  }
</style>

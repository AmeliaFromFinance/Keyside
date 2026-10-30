<script lang="ts">
  import type { Snippet } from "svelte";

  // `edge` is where the handle sits on the panel it sizes: "right" for a left column,
  // "left" for a right column, "top" for a panel at the bottom.
  let {
    value,
    min,
    max,
    edge,
    label,
    onchange,
    onreset,
    children,
  }: {
    value: number;
    min: number;
    max: number;
    edge: "left" | "right" | "top";
    label: string;
    onchange: (v: number) => void;
    onreset?: () => void;
    children?: Snippet;
  } = $props();

  let from = 0;
  let start = 0;
  let dragging = $state(false);

  const clamp = (v: number) => Math.round(Math.min(max, Math.max(min, v)));
  const pos = (e: PointerEvent) => (edge === "top" ? e.clientY : e.clientX);
  // Dragging towards the panel makes it smaller.
  const sign = $derived(edge === "right" ? 1 : -1);

  function down(e: PointerEvent) {
    if (e.button !== 0 || (e.target as HTMLElement).closest("button")) return;
    e.preventDefault();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    dragging = true;
    from = value;
    start = pos(e);
  }
  function move(e: PointerEvent) {
    if (dragging) onchange(clamp(from + sign * (pos(e) - start)));
  }
  function key(e: KeyboardEvent) {
    const grow = edge === "top" ? "ArrowUp" : edge === "right" ? "ArrowRight" : "ArrowLeft";
    const shrink = edge === "top" ? "ArrowDown" : edge === "right" ? "ArrowLeft" : "ArrowRight";
    const step = e.shiftKey ? 64 : 16;
    if (e.key === grow) onchange(clamp(value + step));
    else if (e.key === shrink) onchange(clamp(value - step));
    else if (e.key === "Home") onchange(min);
    else if (e.key === "End") onchange(max);
    else return;
    e.preventDefault();
  }
</script>

<!-- A focusable separator is a widget (it takes arrow keys), which the lint rule doesn't know. -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<div
  class="split at-{edge}"
  class:dragging
  role="separator"
  tabindex="0"
  aria-label={label}
  aria-orientation={edge === "top" ? "horizontal" : "vertical"}
  aria-valuenow={value}
  aria-valuemin={min}
  aria-valuemax={max}
  onpointerdown={down}
  onpointermove={move}
  onpointerup={() => (dragging = false)}
  onpointercancel={() => (dragging = false)}
  ondblclick={(e) => !(e.target as HTMLElement).closest("button") && onreset?.()}
  onkeydown={key}
>
  {@render children?.()}
</div>

<style>
  .split {
    position: absolute;
    z-index: 5;
    display: flex;
    align-items: center;
    justify-content: center;
    touch-action: none;
  }
  .split.at-left,
  .split.at-right {
    top: 0;
    bottom: 0;
    width: 10px;
    cursor: col-resize;
  }
  .split.at-right {
    right: -5px;
  }
  .split.at-left {
    left: -5px;
  }
  .split.at-top {
    left: 0;
    right: 0;
    top: -5px;
    height: 10px;
    cursor: row-resize;
  }
  /* The line itself lights up on hover, drag and focus. */
  .split::before {
    content: "";
    position: absolute;
    background: var(--accent);
    opacity: 0;
    transition: opacity 120ms ease;
  }
  .split.at-left::before,
  .split.at-right::before {
    top: 0;
    bottom: 0;
    left: 4px;
    width: 2px;
  }
  .split.at-top::before {
    left: 0;
    right: 0;
    top: 4px;
    height: 2px;
  }
  .split:hover::before,
  .split.dragging::before,
  .split:focus-visible::before {
    opacity: 0.7;
  }
  .split:focus-visible {
    outline: none;
  }
</style>

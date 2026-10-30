<script lang="ts">
  import Icon from "../components/Icon.svelte";
  import { t } from "../lib/i18n.svelte.ts";
  import { renderCrop, uploadImage } from "./images.svelte.ts";

  let { file, onresult, oncancel }: { file: File; onresult: (icon: string) => void; oncancel: () => void } = $props();

  /** Size of the saved image, in pixels. Enough for large phone keys and Stream Deck XL. */
  const SIZE = 288;
  /** Size of the preview square on screen. */
  const VIEW = 240;

  let img = $state<HTMLImageElement>();
  let url = $state("");
  let natural = $state<[number, number]>([0, 0]);
  let zoom = $state(1);
  let x = $state(0);
  let y = $state(0);
  let busy = $state(false);
  let error = $state("");

  $effect(() => {
    const u = URL.createObjectURL(file);
    url = u;
    zoom = 1;
    x = y = 0;
    error = "";
    return () => URL.revokeObjectURL(u);
  });

  // Zoom 1 fills the square. The lowest zoom shows the whole image.
  const minZoom = $derived(natural[0] && natural[1] ? Math.min(...natural) / Math.max(...natural) : 1);
  const scale = $derived(natural[0] ? (SIZE / Math.min(...natural)) * zoom : 1);
  const shown = $derived<[number, number]>([natural[0] * scale, natural[1] * scale]);

  const clamp = (v: number, side: number) => {
    const max = Math.abs(side - SIZE) / 2;
    return Math.max(-max, Math.min(max, v));
  };
  $effect(() => {
    x = clamp(x, shown[0]);
    y = clamp(y, shown[1]);
  });

  let drag: { px: number; py: number; x: number; y: number; id: number } | null = null;
  function down(e: PointerEvent) {
    drag = { px: e.clientX, py: e.clientY, x, y, id: e.pointerId };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }
  function move(e: PointerEvent) {
    if (!drag || e.pointerId !== drag.id) return;
    const k = SIZE / VIEW;
    x = clamp(drag.x + (e.clientX - drag.px) * k, shown[0]);
    y = clamp(drag.y + (e.clientY - drag.py) * k, shown[1]);
  }
  function wheel(e: WheelEvent) {
    e.preventDefault();
    zoom = Math.max(minZoom, Math.min(4, zoom * (e.deltaY < 0 ? 1.1 : 1 / 1.1)));
  }
  function keys(e: KeyboardEvent) {
    const step = e.shiftKey ? 20 : 5;
    const d = { ArrowLeft: [-step, 0], ArrowRight: [step, 0], ArrowUp: [0, -step], ArrowDown: [0, step] }[e.key];
    if (!d) return;
    e.preventDefault();
    x = clamp(x + d[0], shown[0]);
    y = clamp(y + d[1], shown[1]);
  }

  async function use() {
    if (!img) return;
    busy = true;
    error = "";
    try {
      const blob = await renderCrop(img, SIZE, zoom, x, y);
      onresult(await uploadImage(blob, file.name.replace(/\.[^.]+$/, "") + (blob.type === "image/webp" ? ".webp" : ".png")));
    } catch (e: any) {
      error = e.message;
    } finally {
      busy = false;
    }
  }
</script>

<div class="crop">
  <p class="tip">{t("Drag to move the image, use the slider or the mouse wheel to zoom. The square is what the key shows.")}</p>
  <div
    class="view"
    style:--view="{VIEW}px"
    role="slider"
    tabindex="0"
    aria-label={t("Image position")}
    aria-valuenow={Math.round(zoom * 100)}
    onpointerdown={down}
    onpointermove={move}
    onpointerup={() => (drag = null)}
    onpointercancel={() => (drag = null)}
    onwheel={wheel}
    onkeydown={keys}
  >
    {#if url}
      <img
        bind:this={img}
        src={url}
        alt=""
        draggable="false"
        onload={() => img && (natural = [img.naturalWidth || SIZE, img.naturalHeight || SIZE])}
        onerror={() => (error = t("Could not read that image"))}
        style:width="{(shown[0] * VIEW) / SIZE}px"
        style:height="{(shown[1] * VIEW) / SIZE}px"
        style:left="{VIEW / 2 + (x * VIEW) / SIZE - (shown[0] * VIEW) / SIZE / 2}px"
        style:top="{VIEW / 2 + (y * VIEW) / SIZE - (shown[1] * VIEW) / SIZE / 2}px"
      />
    {/if}
  </div>
  <label class="zoom">
    <Icon name="minus" size={18} />
    <input type="range" min={minZoom} max="4" step="any" bind:value={zoom} aria-label={t("Zoom")} />
    <Icon name="plus" size={18} />
  </label>
  {#if error}<p class="bad" role="alert">{error}</p>{/if}
  <div class="buttons">
    <button class="btn ghost" onclick={() => ((zoom = 1), (x = y = 0))}>{t("Reset")}</button>
    <span class="grow"></span>
    <button class="btn" onclick={oncancel}>{t("Cancel")}</button>
    <button class="btn primary" disabled={busy || !natural[0]} onclick={use}>{busy ? t("Uploading…") : t("Use image")}</button>
  </div>
</div>

<style>
  .crop {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 14px;
  }
  .tip {
    margin: 0;
    color: var(--muted);
    text-align: center;
  }
  .view {
    position: relative;
    width: var(--view);
    height: var(--view);
    overflow: hidden;
    border-radius: var(--radius);
    outline: 2px solid var(--accent);
    outline-offset: 2px;
    cursor: grab;
    touch-action: none;
    /* Checks show where the image will be see-through. */
    background: repeating-conic-gradient(#2a2933 0 25%, #1d1c24 0 50%) 0 0 / 20px 20px;
  }
  .view:active {
    cursor: grabbing;
  }
  .view img {
    position: absolute;
    max-width: none;
    pointer-events: none;
    user-select: none;
  }
  .zoom {
    display: flex;
    align-items: center;
    gap: 10px;
    width: var(--view, 240px);
    color: var(--muted);
  }
  .zoom input {
    flex: 1;
  }
  .buttons {
    display: flex;
    gap: 8px;
    width: 100%;
  }
  .grow {
    flex: 1;
  }
  .bad {
    margin: 0;
    color: var(--danger-text);
  }
</style>

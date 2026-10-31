<script lang="ts">
  // Overlay link builder for OBS browser sources: a timer (under a Timer step) or any text
  // with {variables} (editor menu, "Text overlay for OBS"). The preview frames the real overlay.
  import { untrack } from "svelte";
  import Icon from "../components/Icon.svelte";
  import { t, mark } from "../lib/i18n.svelte.ts";
  import { type Look, type OverlayKind, FONTS, FORMATS, defaults, loadLook, saveLook, overlayLink } from "./overlay.ts";

  let { kind = "timer", name }: { kind?: OverlayKind; name?: unknown } = $props();

  const fontNames: Record<(typeof FONTS)[number], string> = {
    sans: mark("Sans-serif"),
    serif: mark("Serif"),
    mono: mark("Monospace"),
    inter: "Inter",
    "jetbrains-mono": "JetBrains Mono",
    custom: mark("Installed font"),
  };
  const formatNames: Record<(typeof FORMATS)[number], string> = { auto: mark("Automatic"), "hh:mm:ss": "hh:mm:ss", "mm:ss": "mm:ss", ss: mark("Seconds only") };
  const aligns = [
    ["left", "text-align-start", mark("Left")],
    ["center", "text-align-center", mark("Center")],
    ["right", "text-align-end", mark("Right")],
  ] as const;

  let look = $state<Look>(loadLook(untrack(() => kind)));
  let template = $state(loadTemplate());
  let copied = $state(false);
  let field = $state<HTMLInputElement>();

  function loadTemplate() {
    try {
      return localStorage.getItem("overlayText") ?? t("Starting soon");
    } catch {
      return t("Starting soon");
    }
  }

  const what = $derived(kind === "timer" ? (typeof name === "string" && /^[a-z0-9_-]{1,24}$/.test(name) ? name : "") : template.trim() && template.length <= 500 ? template : "");
  const link = $derived(what ? overlayLink(location.origin, kind, what, look) : "");

  // Reload the preview once the controls have been still for a moment, not on every slider step.
  let preview = $state("");
  $effect(() => {
    const next = link;
    const id = setTimeout(() => (preview = next), 300);
    return () => clearTimeout(id);
  });
  $effect(() => saveLook(kind, $state.snapshot(look)));
  $effect(() => {
    const v = template;
    try {
      if (kind === "text") localStorage.setItem("overlayText", v);
    } catch {
      // Not saved; nothing else depends on it.
    }
  });

  // The preview renders a 960 × 270 page and scales it down to the width available.
  const W = 960;
  const H = 270;
  let width = $state(W);
  const scale = $derived(Math.min(1, width / W));

  async function copy() {
    try {
      await navigator.clipboard.writeText(link);
    } catch {
      // No clipboard access: select the link so it can be copied by hand.
      field?.select();
      return;
    }
    copied = true;
    setTimeout(() => (copied = false), 2000);
  }
</script>

<div class="overlay">
  {#if kind === "text"}
    <label class="field">
      <span>{t("Text to show")}</span>
      <input bind:value={template} maxlength="500" placeholder="{'{twitch.viewers}'} viewers" />
    </label>
  {/if}

  <div class="preview" bind:clientWidth={width} style:height="{H * scale}px">
    {#if preview}
      <iframe src={preview} title={t("Overlay preview")} width={W} height={H} style:transform="scale({scale})" tabindex="-1"></iframe>
    {/if}
  </div>

  <div class="grid">
    <label class="field">
      <span>{t("Font")}</span>
      <select bind:value={look.font}>
        {#each FONTS as f (f)}<option value={f}>{t(fontNames[f])}</option>{/each}
      </select>
    </label>
    {#if look.font === "custom"}
      <label class="field">
        <span>{t("Font name")}</span>
        <input bind:value={look.custom} maxlength="40" placeholder="Bebas Neue" pattern="[A-Za-z0-9 \-]+" />
      </label>
    {/if}
    <label class="field">
      <span>{t("Size")}</span>
      <input type="number" min="8" max="1000" bind:value={look.size} onchange={() => (look.size = Math.min(1000, Math.max(8, Math.round(look.size || 8))))} />
    </label>
    <label class="field">
      <span>{t("Weight")}</span>
      <select bind:value={look.weight}>
        {#each [100, 200, 300, 400, 500, 600, 700, 800, 900] as w (w)}<option value={w}>{w}</option>{/each}
      </select>
    </label>
    <label class="field">
      <span>{t("Colour")}</span>
      <input type="color" bind:value={look.color} />
    </label>
    <div class="field">
      <span>{t("Alignment")}</span>
      <div class="seg" role="group" aria-label={t("Alignment")}>
        {#each aligns as [v, icon, label] (v)}
          <button class="btn icon ghost" class:on={look.align === v} aria-pressed={look.align === v} aria-label={t(label)} title={t(label)} onclick={() => (look.align = v)}><Icon name={icon} size={18} /></button>
        {/each}
      </div>
    </div>
    <label class="field">
      <span>{t("Background")} <b>{look.bgAlpha}%</b></span>
      <span class="pair">
        <input type="color" bind:value={look.bg} aria-label={t("Background")} />
        <input type="range" min="0" max="100" step="5" bind:value={look.bgAlpha} aria-label={t("Opacity")} />
      </span>
    </label>
  </div>

  <details>
    <summary>{t("Effects")}</summary>
    <div class="effect">
      <label class="check"><input type="checkbox" bind:checked={look.shadow} />{t("Shadow")}</label>
      {#if look.shadow}
        <div class="grid">
          <label class="field">
            <span>{t("Colour")} <b>{look.shadowAlpha}%</b></span>
            <span class="pair">
              <input type="color" bind:value={look.shadowColor} aria-label={t("Colour")} />
              <input type="range" min="0" max="100" step="5" bind:value={look.shadowAlpha} aria-label={t("Opacity")} />
            </span>
          </label>
          <label class="field"><span>{t("Blur")} <b>{look.shadowBlur}px</b></span><input type="range" min="0" max="100" bind:value={look.shadowBlur} /></label>
          <label class="field"><span>{t("Offset right")} <b>{look.shadowX}px</b></span><input type="range" min="-50" max="50" bind:value={look.shadowX} /></label>
          <label class="field"><span>{t("Offset down")} <b>{look.shadowY}px</b></span><input type="range" min="-50" max="50" bind:value={look.shadowY} /></label>
        </div>
      {/if}
    </div>
    <div class="effect">
      <label class="check"><input type="checkbox" bind:checked={look.glow} />{t("Glow")}</label>
      {#if look.glow}
        <div class="grid">
          <label class="field"><span>{t("Colour")}</span><input type="color" bind:value={look.glowColor} /></label>
          <label class="field"><span>{t("Strength")} <b>{look.glowSize}</b></span><input type="range" min="1" max="100" bind:value={look.glowSize} /></label>
        </div>
      {/if}
    </div>
    <div class="effect">
      <label class="check"><input type="checkbox" bind:checked={look.outline} />{t("Outline")}</label>
      {#if look.outline}
        <div class="grid">
          <label class="field"><span>{t("Colour")}</span><input type="color" bind:value={look.outlineColor} /></label>
          <label class="field"><span>{t("Width")} <b>{look.outlineWidth}px</b></span><input type="range" min="1" max="20" bind:value={look.outlineWidth} /></label>
        </div>
      {/if}
    </div>
    <div class="grid">
      <label class="field">
        <span>{t("Padding")} <b>{look.pad === null ? t("Auto") : `${look.pad}px`}</b></span>
        <input type="range" min="0" max="200" value={look.pad ?? 0} oninput={(e) => (look.pad = Number(e.currentTarget.value))} />
      </label>
      <label class="field"><span>{t("Letter spacing")} <b>{look.spacing}px</b></span><input type="range" min="-20" max="50" bind:value={look.spacing} /></label>
      {#if kind === "timer"}
        <label class="field">
          <span>{t("Time format")}</span>
          <select bind:value={look.format}>
            {#each FORMATS as f (f)}<option value={f}>{t(formatNames[f])}</option>{/each}
          </select>
        </label>
      {/if}
    </div>
    <label class="check"><input type="checkbox" bind:checked={look.upper} />{t("Uppercase")}</label>
    <label class="check"><input type="checkbox" bind:checked={look.hide} />{kind === "timer" ? t("Hide while stopped") : t("Hide while empty")}</label>
  </details>

  <div class="row">
    <input bind:this={field} readonly value={link} aria-label={t("Overlay link")} />
    <button class="btn" onclick={() => (look = defaults(kind))}><Icon name="rotate-ccw" size={16} />{t("Reset")}</button>
    <button class="btn primary" disabled={!link} onclick={copy}><Icon name={copied ? "check" : "copy"} size={16} />{copied ? t("Copied") : t("Copy overlay link")}</button>
  </div>
  <p>{t("Add it to OBS as a browser source.")}</p>
</div>

<style>
  .overlay {
    margin-top: 12px;
    display: grid;
    gap: 10px;
  }
  .preview {
    position: relative;
    overflow: hidden;
    border-radius: var(--radius-sm);
    border: 1px solid var(--line);
    /* A checkerboard, so see-through parts of the overlay show as such. */
    background: repeating-conic-gradient(#3a3744 0 25%, #2a2833 0 50%) 0 0 / 16px 16px;
  }
  iframe {
    position: absolute;
    top: 0;
    left: 0;
    border: 0;
    transform-origin: 0 0;
    pointer-events: none;
    /* Must match the overlay page, or the browser paints the frame white instead of see-through. */
    color-scheme: light;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(130px, 1fr));
    gap: 10px;
  }
  .field b {
    font-weight: 500;
    color: var(--dim);
    font-size: 0.8rem;
  }
  input[type="color"] {
    padding: 2px 4px !important;
    cursor: pointer;
  }
  .pair {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .pair input[type="color"] {
    flex: 0 0 48px;
  }
  .pair input[type="range"] {
    flex: 1;
    min-width: 0;
  }
  input[type="range"] {
    accent-color: var(--accent);
  }
  .seg {
    display: flex;
    min-height: 42px;
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    overflow: hidden;
  }
  .seg .btn {
    flex: 1;
    width: auto;
    min-height: 40px;
    border-radius: 0;
  }
  .seg .btn.on {
    background: var(--accent-bg);
    color: var(--accent-text);
  }
  details {
    display: grid;
    gap: 8px;
  }
  summary {
    cursor: pointer;
    color: var(--muted);
    font-size: 0.9rem;
  }
  details[open] summary {
    margin-bottom: 8px;
  }
  .effect {
    margin-bottom: 8px;
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .row input {
    flex: 1 1 160px;
    min-width: 0;
    min-height: 40px;
    padding: 0 10px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--line);
    background: var(--bg);
    color: var(--muted);
    font: inherit;
    font-size: 0.85rem;
  }
  .row .btn {
    gap: 6px;
  }
  p {
    margin: 0;
    color: var(--muted);
    font-size: 0.85rem;
  }
</style>

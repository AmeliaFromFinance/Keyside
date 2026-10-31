<script lang="ts">
  import type { KeyView } from "@keyside/protocol";
  import Icon from "./Icon.svelte";
  import { assetImage, deck } from "../lib/deck.svelte.ts";
  import { clock, everySecond } from "../lib/format.ts";
  import { haptic } from "../lib/native.ts";
  import { t } from "../lib/i18n.svelte.ts";

  let {
    view,
    disabled = false,
    selected = false,
    lifted = false,
    preview = false,
    ondown,
    onup,
    onslide,
    onselect,
  }: {
    view: KeyView;
    disabled?: boolean;
    selected?: boolean;
    /** Editor: being dragged somewhere else. */
    lifted?: boolean;
    /** Editor preview: taps select instead of pressing. */
    preview?: boolean;
    ondown?: (id: string) => void;
    onup?: (id: string, cancel: boolean) => void;
    onslide?: (id: string, value: number) => void;
    /** The event says whether Shift, Ctrl or Cmd was held. */
    onselect?: (id: string, e?: PointerEvent | KeyboardEvent) => void;
  } = $props();

  let pressed = $state(false);
  /** The finger (or mouse) holding the key. Other pointers on the same key are ignored. */
  let pointer: number | null = null;
  /** Arrow or Enter keys are moving the slider. The value goes out when they're let go. */
  let keying = false;
  let holdStart = $state(0);
  let now = $state(Date.now());
  let image = $state<string | null>(null);
  let slideValue = $state<number | null>(null);
  let el: HTMLElement;

  $effect(() => {
    if (!view.since) return;
    return everySecond((n) => (now = n));
  });

  $effect(() => {
    const icon = view.icon;
    image = null;
    if (icon?.startsWith("asset:")) void assetImage(icon.slice(6)).then((url) => (image = url));
  });

  const plainIcon = $derived(view.icon && !view.icon.includes(":") ? view.icon : undefined);
  const timer = $derived(view.since ? clock(now - view.since) : null);
  const meta = $derived(timer ?? view.meta);
  const badgeTone = $derived(view.badgeTone ?? view.tone ?? "default");
  const filledBadge = $derived(badgeTone === "accent" || badgeTone === "danger");
  const value = $derived(slideValue ?? view.value ?? 0);

  // After a slide, keep showing the chosen value until the app reports back (or briefly, if it doesn't).
  let seen: number | undefined;
  let settle: ReturnType<typeof setTimeout> | null = null;
  $effect(() => {
    const v = view.value;
    if (v === seen) return;
    seen = v;
    if (!pressed && !keying) slideValue = null;
  });
  $effect(() => () => settle && clearTimeout(settle));

  /** Sends the slider's value once, when the person lets go. */
  function commit() {
    if (slideValue === null) return;
    onslide?.(view.id, slideValue);
    if (settle) clearTimeout(settle);
    settle = setTimeout(() => !pressed && !keying && (slideValue = null), 1500);
  }
  const holdMs = $derived(view.hold ?? 0);
  const tall = $derived(view.h > 1 && view.h >= view.w);

  function down(e: PointerEvent) {
    if (e.button > 0) return;
    if (preview) {
      onselect?.(view.id, e);
      return;
    }
    if (disabled || view.disabled || pressed) return;
    pointer = e.pointerId;
    el.setPointerCapture?.(e.pointerId);
    pressed = true;
    haptic("light");
    if (view.kind === "slider") {
      moveSlider(e);
      return;
    }
    holdStart = holdMs > 0 ? performance.now() : 0;
    ondown?.(view.id);
  }

  function up(e: PointerEvent, cancel = false) {
    if (!pressed || e.pointerId !== pointer) return;
    pressed = false;
    pointer = null;
    el.releasePointerCapture?.(e.pointerId);
    if (view.kind === "slider") {
      if (cancel) slideValue = null;
      else commit();
      return;
    }
    const held = holdStart ? performance.now() - holdStart : 0;
    holdStart = 0;
    if (holdMs > 0 && held >= holdMs) haptic("medium");
    onup?.(view.id, cancel);
  }

  /** Follows the finger on screen only. Nothing is sent until release. */
  function moveSlider(e: PointerEvent) {
    const r = el.getBoundingClientRect();
    const v = tall ? 1 - (e.clientY - r.top) / r.height : (e.clientX - r.left) / r.width;
    slideValue = Math.min(1, Math.max(0, v));
  }

  function move(e: PointerEvent) {
    if (pressed && e.pointerId === pointer && view.kind === "slider") moveSlider(e);
  }

  /** Focus left mid-press (Alt+Tab, say): finish what the keyboard started. */
  function blur() {
    if (keying) {
      keying = false;
      commit();
    } else if (pressed && pointer === null) {
      pressed = false;
      holdStart = 0;
      onup?.(view.id, true);
    }
  }

  function key(e: KeyboardEvent, isDown: boolean) {
    const arrows = view.kind === "slider" && ["ArrowUp", "ArrowRight", "ArrowDown", "ArrowLeft"].includes(e.key);
    if (e.key !== "Enter" && e.key !== " " && !arrows) return;
    e.preventDefault();
    if (preview) {
      if (isDown) onselect?.(view.id, e);
      return;
    }
    if (disabled || view.disabled) return;
    if (view.kind === "slider") {
      if (isDown) {
        const lower = e.key === "ArrowDown" || e.key === "ArrowLeft" || (!arrows && e.shiftKey);
        keying = true;
        slideValue = Math.min(1, Math.max(0, value + (lower ? -0.1 : 0.1)));
      } else if (keying) {
        keying = false;
        commit();
      }
      return;
    }
    if (isDown && !pressed) {
      pressed = true;
      holdStart = holdMs > 0 ? performance.now() : 0;
      ondown?.(view.id);
    } else if (!isDown && pressed && pointer === null) {
      pressed = false;
      holdStart = 0;
      onup?.(view.id, false);
    }
  }

  // A press that didn't work flashes the key red for a moment.
  let failed = $state(false);
  $effect(() => {
    const f = deck.failed;
    if (!f || f.key !== view.id || Date.now() - f.at > 2000) return;
    failed = true;
    haptic("medium");
    const timer = setTimeout(() => (failed = false), 900);
    return () => clearTimeout(timer);
  });
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<div
  bind:this={el}
  data-key={view.id}
  class="key kind-{view.kind} tone-{view.tone ?? 'default'} color-{view.color ?? 'default'}"
  class:pressed
  class:selected
  class:lifted
  class:dim={disabled || view.disabled}
  class:holding={holdStart > 0}
  class:tall
  class:feed={!!view.lines}
  class:trouble={!!view.trouble}
  class:failed
  role={view.kind === "slider" ? "slider" : "button"}
  tabindex="0"
  aria-label={[view.label, view.sub, view.badge, view.trouble && t("{app} isn't connected", { app: view.trouble })].filter(Boolean).join(", ")}
  title={view.trouble ? t("{app} isn't connected", { app: view.trouble }) : undefined}
  aria-valuenow={view.kind === "slider" ? Math.round(value * 100) : undefined}
  aria-valuemin={view.kind === "slider" ? 0 : undefined}
  aria-valuemax={view.kind === "slider" ? 100 : undefined}
  aria-disabled={disabled || view.disabled}
  style:--hold="{holdMs}ms"
  style:--value={value}
  onpointerdown={down}
  onpointerup={(e) => up(e)}
  onpointercancel={(e) => up(e, true)}
  onpointermove={move}
  onkeydown={(e) => key(e, true)}
  onkeyup={(e) => key(e, false)}
  onblur={blur}
  oncontextmenu={(e) => e.preventDefault()}
>
  {#if view.kind === "slider"}
    <div class="fill" aria-hidden="true"></div>
  {/if}
  {#if view.image || image}
    <img class="art" src={view.image ?? image} alt="" draggable="false" />
  {/if}
  <div class="top">
    <span class="icon">
      {#if plainIcon}<Icon name={plainIcon} />{/if}
    </span>
    {#if view.trouble}
      <span class="warn" aria-hidden="true"><Icon name="triangle-alert" size={18} /></span>
    {:else if view.badge}
      <span class="badge tone-{badgeTone}" class:filled={filledBadge}>{view.badge}</span>
    {:else if meta}
      <span class="meta mono">{meta}</span>
    {:else if view.kind === "folder"}
      <span class="chev"><Icon name="chevron-right" size={20} /></span>
    {/if}
  </div>

  {#if view.lines}
    <ol class="lines" aria-live="polite">
      {#each view.lines as line (line.at)}
        <li>
          {#if line.author}<b style:color={line.color}>{line.author}</b>{/if}
          {line.text}
        </li>
      {:else}
        <li class="empty">{t("Nothing yet")}</li>
      {/each}
    </ol>
  {/if}

  <!-- Feed widgets give their room to the lines. The label is still in aria-label. -->
  {#if !view.lines || view.sub}
    <div class="bottom">
      {#if !view.lines}<div class="label">{view.label}</div>{/if}
      {#if view.sub}<div class="sub">{view.sub}</div>{/if}
      {#if view.kind === "slider"}<div class="sub mono">{Math.round(value * 100)}%</div>{/if}
    </div>
  {/if}

  {#if view.progress !== undefined}
    <div class="progress" aria-hidden="true"><span style:width="{Math.round(view.progress * 100)}%"></span></div>
  {/if}
  {#if holdMs > 0}
    <div class="hold" aria-hidden="true"></div>
  {/if}
</div>

<style>
  .key {
    position: relative;
    container-type: size;
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    min-width: 0;
    min-height: 0;
    padding: clamp(7px, 9cqmin, 22px);
    border-radius: var(--radius);
    background: var(--surface);
    border: 1px solid var(--line-soft);
    overflow: hidden;
    touch-action: none;
    user-select: none;
    -webkit-user-select: none;
    /* iOS shows a "Save image" callout on a held picture, which cancels the press. */
    -webkit-touch-callout: none;
    cursor: pointer;
    transition:
      transform 80ms ease,
      background-color 120ms ease,
      border-color 120ms ease;
  }
  .key:hover {
    border-color: var(--line);
  }
  .pressed {
    transform: scale(0.97);
    background: var(--hover);
  }

  .color-teal {
    background: var(--key-teal);
  }
  .color-violet {
    background: var(--key-violet);
  }
  .color-green {
    background: var(--key-green);
  }
  .color-amber {
    background: var(--key-amber);
  }
  .color-red {
    background: var(--key-red);
  }
  .color-blue {
    background: var(--key-blue);
  }
  .kind-folder {
    background: var(--raised);
  }

  .tone-accent {
    background: var(--accent-bg);
    border: 2px solid var(--accent);
  }
  .tone-danger {
    background: var(--danger-bg);
    border: 2px solid var(--danger);
  }
  .tone-accent .icon,
  .tone-accent .sub {
    color: var(--accent-text);
  }
  .tone-danger .icon,
  .tone-danger .sub,
  .tone-danger .meta {
    color: var(--danger-text);
  }
  .tone-muted .label {
    color: var(--muted);
  }
  .dim {
    background: transparent;
    border: 1px dashed var(--line);
    cursor: default;
  }
  .dim .label,
  .dim .icon {
    color: var(--muted);
  }
  .dim .sub {
    color: var(--dim);
  }
  .selected {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .lifted {
    opacity: 0.3;
    outline-style: dashed;
  }

  .top {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 6px;
    position: relative;
  }
  .icon {
    display: flex;
    color: var(--text);
  }
  .icon :global(svg) {
    width: clamp(18px, 22cqmin, 34px);
    height: clamp(18px, 22cqmin, 34px);
  }
  .badge {
    font-size: clamp(9px, 9cqmin, 13px);
    font-weight: 700;
    letter-spacing: 0.06em;
    line-height: 1;
    padding: 0.35em 0;
    color: var(--muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }
  .badge.filled {
    padding: 0.45em 0.7em;
    border-radius: 6px;
    color: var(--bg);
  }
  .badge.filled.tone-accent {
    background: var(--accent);
  }
  .badge.filled.tone-danger {
    background: var(--danger);
  }
  .badge.tone-success {
    color: var(--success);
  }
  .badge.tone-warning {
    color: var(--warning);
  }
  .badge.tone-info {
    color: var(--info);
  }
  .meta {
    font-size: clamp(9px, 9cqmin, 13px);
    color: var(--muted);
    padding-top: 0.2em;
    white-space: nowrap;
  }
  .chev {
    color: var(--muted);
    display: flex;
  }

  .bottom {
    position: relative;
    width: 100%;
    min-width: 0;
  }
  .label {
    font-weight: 600;
    font-size: clamp(10px, 11.5cqmin, 20px);
    line-height: 1.2;
    width: 100%;
    overflow-wrap: break-word;
    hyphens: auto;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .sub {
    margin-top: 0.15em;
    font-size: clamp(10px, 10cqmin, 16px);
    color: var(--muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  @container (max-height: 56px) {
    .sub {
      display: none;
    }
  }
  /* Narrow keys: a filled badge like ON AIR matters more than the icon. */
  @container (max-width: 120px) {
    .top:has(.badge.filled) .icon {
      display: none;
    }
  }
  @container (max-width: 64px) {
    .badge,
    .meta {
      display: none;
    }
  }

  .art {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
    opacity: 0.9;
    pointer-events: none;
  }

  .progress {
    margin-top: 8px;
    height: 3px;
    border-radius: 2px;
    background: var(--line);
    overflow: hidden;
  }
  .progress span {
    display: block;
    height: 100%;
    background: var(--accent);
    transition: width 300ms ease;
  }

  .hold {
    position: absolute;
    left: 0;
    bottom: 0;
    height: 3px;
    width: 0;
    background: var(--accent);
  }
  .holding .hold {
    width: 100%;
    transition: width var(--hold) linear;
  }

  .kind-slider .fill {
    position: absolute;
    inset: auto 0 0 0;
    height: calc(var(--value) * 100%);
    background: color-mix(in srgb, var(--accent) 18%, transparent);
    border-top: 2px solid var(--accent);
    pointer-events: none;
  }
  .kind-slider:not(.tall) .fill {
    inset: 0 auto 0 0;
    height: 100%;
    width: calc(var(--value) * 100%);
    border-top: none;
    border-right: 2px solid var(--accent);
  }

  /* Newest line at the bottom. Older ones go off the top when the key is full. */
  .lines {
    list-style: none;
    margin: 4px 0 0;
    padding: 0;
    flex: 1;
    display: flex;
    flex-direction: column;
    justify-content: flex-end;
    gap: 2px;
    overflow: hidden;
    font-size: clamp(11px, 6cqmin, 15px);
    min-height: 0;
    /* The oldest line fades out at the top instead of being cut in half. */
    mask-image: linear-gradient(to bottom, transparent, #000 1.6em);
  }
  .lines li {
    flex: none;
    overflow-wrap: anywhere;
    line-height: 1.3;
  }
  .lines b {
    font-weight: 600;
    margin-right: 0.35em;
    color: var(--accent);
  }
  .lines .empty {
    color: var(--dim);
  }
  .feed .icon :global(svg) {
    width: clamp(14px, 12cqmin, 20px);
    height: clamp(14px, 12cqmin, 20px);
  }
  .feed .top:not(:has(.icon > *, .badge, .meta)) {
    display: none;
  }
  .trouble > :not(.top),
  .trouble .icon {
    opacity: 0.5;
  }
  .warn {
    display: flex;
    color: var(--warning);
  }
  .failed {
    animation: failed 0.9s ease-out;
  }
  @keyframes failed {
    0%,
    40% {
      border-color: var(--danger);
      box-shadow: 0 0 0 3px var(--danger-bg);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .failed {
      animation: none;
      border-color: var(--danger);
    }
  }
</style>

<script lang="ts">
  import Grid from "./Grid.svelte";
  import Icon from "./Icon.svelte";
  import PinPad from "./PinPad.svelte";
  import Sheet from "./Sheet.svelte";
  import { deck, nav, press, slide, sendViewport, connection, dismissToast, localToast } from "../lib/deck.svelte.ts";
  import { t, lang, setLang, languages, type Lang } from "../lib/i18n.svelte.ts";
  import { ago, clock, everySecond } from "../lib/format.ts";
  import { keepAwake, haptic } from "../lib/native.ts";

  let {
    onedit,
    onswitch,
    onforget,
    canEdit = false,
  }: {
    onedit?: () => void;
    onswitch?: () => void;
    onforget?: () => void;
    canEdit?: boolean;
  } = $props();

  let localLock = $state(false);
  let settings = $state(false);
  let now = $state(Date.now());
  let lockTimer: ReturnType<typeof setTimeout> | null = null;
  let lockArming = $state(false);
  let awake = $state(readAwake());

  function readAwake() {
    try {
      return localStorage.getItem("awake") !== "0";
    } catch {
      return true;
    }
  }

  $effect(() => everySecond((n) => (now = n)));

  $effect(() => {
    void keepAwake(awake);
    try {
      localStorage.setItem("awake", awake ? "1" : "0");
    } catch {
      /* storage blocked */
    }
    const vis = () => document.visibilityState === "visible" && (void keepAwake(awake), connection()?.nudge());
    document.addEventListener("visibilitychange", vis);
    return () => document.removeEventListener("visibilitychange", vis);
  });

  $effect(() => {
    const onResize = () => sendViewport();
    addEventListener("resize", onResize);
    return () => removeEventListener("resize", onResize);
  });

  function onKey(e: KeyboardEvent) {
    if (settings || e.target instanceof HTMLInputElement) return;
    if (e.key === "ArrowRight" || e.key === "PageDown") nav("next");
    if (e.key === "ArrowLeft" || e.key === "PageUp") nav("prev");
    if (e.key === "Escape" || e.key === "Backspace") nav("back");
  }

  // Locking needs a long press so a pocket tap can't change it.
  const LOCK_HOLD = 600;
  let lockStart = 0;
  function lockDown(e?: PointerEvent) {
    if (e && e.button > 0) return;
    // Keep getting this finger's events, even if it drifts a little.
    if (e) (e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId);
    lockArming = true;
    lockStart = performance.now();
    if (lockTimer) clearTimeout(lockTimer);
    lockTimer = setTimeout(() => {
      lockTimer = null;
      localLock = !localLock;
      lockArming = false;
      haptic("medium");
    }, LOCK_HOLD);
  }
  function lockUp(e?: PointerEvent) {
    // Phones may cancel a held touch for their own long-press. That isn't a release.
    if (e?.type === "pointercancel" && e.pointerType !== "mouse") return;
    const short = !!lockTimer && performance.now() - lockStart < LOCK_HOLD;
    lockArming = false;
    if (lockTimer) clearTimeout(lockTimer);
    lockTimer = null;
    // A quick tap does nothing, so say how it works.
    if (short && e?.type === "pointerup") localToast(localLock ? t("Hold to unlock keys") : t("Hold to lock keys"));
  }
  /** Keyboard users hold Enter or Space, like a finger on the button. */
  function lockKey(e: KeyboardEvent, isDown: boolean) {
    if (e.key !== "Enter" && e.key !== " ") return;
    e.preventDefault();
    if (isDown && !e.repeat) lockDown();
    else if (!isDown) lockUp();
  }

  const page = $derived(deck.page);
  const online = $derived(deck.conn.s === "open");
  const subtitle = $derived(page ? (page.folder ? page.subtitle ?? deck.profile?.name ?? "" : [t("Page {n} of {count}", { n: page.index + 1, count: page.count }), page.subtitle].filter(Boolean).join(" · ")) : "");
</script>

<svelte:window onkeydown={onKey} />

<main class="deck">
  {#if deck.locked}
    <PinPad />
  {:else}
    <header>
      <div class="title">
        {#if page?.folder}
          <button class="back" onclick={() => nav("back")} aria-label={t("Back")}><Icon name="arrow-left" /></button>
        {/if}
        <div>
          <h1>{page?.name ?? deck.profile?.name ?? "Keyside"}</h1>
          <p>{subtitle}</p>
        </div>
      </div>
      <div class="tools">
        <button
          class="btn icon tool"
          class:on={localLock}
          class:arming={lockArming}
          aria-pressed={localLock}
          aria-label={localLock ? t("Hold to unlock keys") : t("Hold to lock keys")}
          title={localLock ? t("Hold to unlock keys") : t("Hold to lock keys")}
          onpointerdown={lockDown}
          onpointerup={lockUp}
          onpointercancel={lockUp}
          oncontextmenu={(e) => e.preventDefault()}
          onkeydown={(e) => lockKey(e, true)}
          onkeyup={(e) => lockKey(e, false)}><Icon name={localLock ? "lock" : "lock-open"} /></button
        >
        <button class="btn icon tool" aria-label={t("Settings")} onclick={() => (settings = true)}><Icon name="sliders-horizontal" /></button>
      </div>
    </header>

    <section class="status" aria-live="polite">
      <div class="line">
        <span class="dot" class:ok={online} class:busy={deck.conn.s === "connecting"} class:bad={deck.conn.s === "closed"}></span>
        <span class="who">
          {#if online}
            {t("Connected to")} <span class="mono name">{deck.companion?.name}</span>
          {:else if deck.conn.s === "connecting"}
            {t("Connecting…")}
          {:else if deck.conn.s === "closed" && !deck.conn.retry}
            {t("Offline")}
          {:else}
            {t("Offline, retrying")}
          {/if}
        </span>
        {#if online}<span class="mono latency">{deck.latency} ms</span>{/if}
      </div>
      {#if deck.connections.length || deck.pills.length}
        <div class="chips">
          {#each deck.connections as c (c.id)}
            <span class="chip" title={c.detail}><span class="dot" class:ok={c.status === "connected"} class:busy={c.status === "connecting"} class:bad={c.status === "error"}></span>{c.name}</span>
          {/each}
          {#each deck.pills as p (p.id)}
            <span class="chip pill tone-{p.tone}"><span class="dot"></span><span>{p.label}</span>{#if p.since}<span class="mono">{clock(now - p.since)}</span>{/if}</span>
          {/each}
        </div>
      {/if}
    </section>

    {#if page}
      <Grid
        cols={page.cols}
        rows={page.rows}
        keys={page.keys}
        locked={localLock || !online}
        ondown={(id) => press(id, "down")}
        onup={(id, cancel) => press(id, cancel ? "cancel" : "up")}
        onslide={slide}
      />
    {:else}
      <div class="empty">
        {#if online}
          <p>{t("No pages to show. Add one in the editor on your PC.")}</p>
        {:else}
          <p>{t("Looking for your PC…")}</p>
        {/if}
      </div>
    {/if}

    {#if page && !page.folder && page.count > 1}
      <nav class="pager" aria-label={t("Pages")}>
        <button class="btn icon ghost" aria-label={t("Previous page")} disabled={page.index === 0} onclick={() => nav("prev")}><Icon name="chevron-left" /></button>
        <div class="dots">
          {#each { length: page.count } as _, i (i)}
            <span class:on={i === page.index}></span>
          {/each}
        </div>
        <button class="btn icon ghost" aria-label={t("Next page")} disabled={page.index === page.count - 1} onclick={() => nav("next")}><Icon name="chevron-right" /></button>
      </nav>
    {/if}

    <footer aria-live="polite">
      <button class="toast" class:show={!!deck.toast} disabled={!deck.toast} title={t("Dismiss")} onclick={dismissToast}>
        {#if deck.toast}
          <span class="ticon tone-{deck.toast.tone}">
            <Icon name={deck.toast.tone === "danger" ? "circle-alert" : deck.toast.tone === "warning" ? "triangle-alert" : "check"} />
          </span>
          <span class="text">{deck.toast.text}</span>
          <span class="mono when">{t("{t} ago", { t: ago(deck.toast.at, now) })}</span>
        {/if}
      </button>
    </footer>
  {/if}
</main>

<Sheet bind:open={settings} title={t("Settings")}>
  <div class="about">
    <p><b>{deck.companion?.name}</b> <span class="mono">v{deck.companion?.version}</span></p>
    <p>{t("This device")}: {deck.self?.name} ({deck.self?.scope === "editor" ? t("can edit") : deck.self?.scope === "moderator" ? t("moderator") : t("deck only")})</p>
  </div>
  <label class="field">
    <span>{t("Language")}</span>
    <select value={lang()} onchange={(e) => setLang(e.currentTarget.value as Lang)}>
      {#each Object.entries(languages) as [code, name] (code)}<option value={code}>{name}</option>{/each}
    </select>
  </label>
  <label class="check"><input type="checkbox" bind:checked={awake} /> {t("Keep the screen on")}</label>
  {#if canEdit && onedit}
    <button class="btn" onclick={() => ((settings = false), onedit())}><Icon name="pen" />{t("Edit layout")}</button>
  {/if}
  {#if onswitch}
    <button class="btn" onclick={() => ((settings = false), onswitch())}><Icon name="monitor" />{t("Other PCs")}</button>
  {/if}
  {#if onforget}
    <button class="btn danger" onclick={() => ((settings = false), onforget())}><Icon name="log-out" />{t("Forget this PC")}</button>
  {/if}
</Sheet>

<style>
  .deck {
    height: 100dvh;
    display: flex;
    flex-direction: column;
    gap: clamp(12px, 2.4vmin, 20px);
    padding: max(16px, env(safe-area-inset-top)) max(16px, env(safe-area-inset-right)) max(12px, env(safe-area-inset-bottom)) max(16px, env(safe-area-inset-left));
    max-width: 1400px;
    margin: 0 auto;
  }
  header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
  }
  .title {
    display: flex;
    gap: 8px;
    min-width: 0;
  }
  .back {
    border: none;
    background: none;
    padding: 4px;
    margin-left: -4px;
    color: var(--text);
    display: flex;
  }
  h1 {
    margin: 0;
    font-size: clamp(1.25rem, 3.4vmin, 1.8rem);
    letter-spacing: -0.01em;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  header p {
    margin: 2px 0 0;
    color: var(--muted);
  }
  .tools {
    display: flex;
    gap: 12px;
  }
  .tool {
    width: 48px;
    min-height: 48px;
    border-radius: 14px;
    background: var(--surface);
    border-color: var(--line-soft);
  }
  /* Long-pressing the lock is the point, so the phone must not treat it as its own long-press. */
  .tool {
    touch-action: none;
    user-select: none;
    -webkit-user-select: none;
    -webkit-touch-callout: none;
  }
  .tool.on {
    color: var(--accent);
    border-color: var(--accent);
  }
  .tool.arming {
    transform: scale(0.94);
    transition: transform 600ms ease;
  }

  .status {
    background: var(--surface);
    border: 1px solid var(--line-soft);
    border-radius: 14px;
    padding: 14px 16px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .line {
    display: flex;
    align-items: center;
    gap: 10px;
    font-weight: 500;
  }
  .line .dot {
    width: 10px;
    height: 10px;
  }
  .who {
    flex: 1;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .name {
    text-transform: uppercase;
  }
  .latency {
    color: var(--muted);
  }
  .chips {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }
  .pill.tone-danger {
    background: var(--danger-chip);
    color: var(--danger-text);
  }
  .pill.tone-danger .dot {
    background: var(--danger);
  }
  .pill.tone-accent {
    background: var(--accent-bg);
    color: var(--accent-text);
  }
  .pill.tone-accent .dot {
    background: var(--accent);
  }

  .empty {
    flex: 1;
    display: grid;
    place-items: center;
    color: var(--muted);
    text-align: center;
  }

  .pager {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 24px;
  }
  .pager .btn {
    color: var(--text);
  }
  .pager .btn:disabled {
    opacity: 0.3;
  }
  .dots {
    display: flex;
    align-items: center;
    gap: 14px;
  }
  .dots span {
    width: 8px;
    height: 8px;
    border-radius: 4px;
    background: var(--dim);
    transition: width 160ms ease;
  }
  .dots span.on {
    width: 24px;
    background: var(--accent);
  }

  .toast {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    min-height: 52px;
    padding: 0 16px;
    border-radius: 14px;
    background: var(--surface);
    border: 1px solid var(--line-soft);
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
    visibility: hidden;
  }
  .toast.show {
    visibility: visible;
  }
  .ticon {
    display: flex;
    color: var(--success);
  }
  .ticon.tone-danger {
    color: var(--danger);
  }
  .ticon.tone-warning {
    color: var(--warning);
  }
  .text {
    flex: 1;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .when {
    color: var(--muted);
  }
  .about p {
    margin: 0 0 4px;
    color: var(--muted);
  }
  .about b {
    color: var(--text);
  }
  @media (orientation: landscape) and (max-height: 500px) {
    .status {
      display: none;
    }
    .toast {
      min-height: 40px;
    }
  }
</style>

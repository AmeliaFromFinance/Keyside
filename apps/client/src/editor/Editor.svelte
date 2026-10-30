<script lang="ts">
  import Icon from "../components/Icon.svelte";
  import Sheet from "../components/Sheet.svelte";
  import Palette from "./Palette.svelte";
  import Canvas from "./Canvas.svelte";
  import Inspector from "./Inspector.svelte";
  import PairPanel from "./PairPanel.svelte";
  import Plugins from "./Plugins.svelte";
  import Settings from "./Settings.svelte";
  import Review from "./Review.svelte";
  import Share from "./Share.svelte";
  import StreamDeck from "./StreamDeck.svelte";
  import Profiles from "./Profiles.svelte";
  import { ed, load, push, discard, undo, redo, canUndo, canRedo, startLive, change, newId, page, deleteKeys, copyKeys, pasteKey, duplicateKeys, shiftKeys, selection, selectAll, clearSelection } from "./state.svelte.ts";
  import { deck, req } from "../lib/deck.svelte.ts";
  import { t } from "../lib/i18n.svelte.ts";
  import { ago, everySecond } from "../lib/format.ts";
  import { zoomControls } from "../lib/zoom.ts";
  import { plain } from "../lib/rich.ts";

  let { admin = false, ondeck }: { admin?: boolean; ondeck: () => void } = $props();

  type Panel = "plugins" | "settings" | "review" | "share" | "streamdeck" | "devices" | "profiles" | null;
  let panel = $state<Panel>(null);
  let menu = $state(false);
  let paletteOpen = $state(false);
  let now = $state(Date.now());
  let loaded = $state(false);
  let recording = $state(false);
  let plugins = $state<Array<{ id: string; name: string; status: string; values: Record<string, unknown> }>>([]);
  let devicesOnline = $state(0);
  let reviewCount = $state(0);

  $effect(() => everySecond((n) => (now = n)));
  $effect(() => {
    if (ed.setup) panel = "plugins";
  });
  $effect(() => (admin ? zoomControls() : undefined));

  $effect(() => {
    if (deck.conn.s === "open" && !loaded) {
      loaded = true;
      load().then(refreshMeta, (e) => {
        // Let the next connection try again instead of showing "Loading" forever.
        loaded = false;
        ed.error = e instanceof Error ? e.message : String(e);
      });
    }
    if (deck.conn.s !== "open") loaded = false;
  });

  $effect(() => {
    if (!loaded) return;
    return startLive();
  });

  async function refreshMeta() {
    try {
      plugins = await req("plugins.list");
      if (admin) {
        const devs = await req<Array<{ online: boolean }>>("devices.list");
        devicesOnline = devs.filter((d) => d.online).length;
        reviewCount = (await req<unknown[]>("review.list")).length;
      }
    } catch {
      /* reconnecting */
    }
  }

  $effect(() => {
    void deck.connections;
    if (loaded) void refreshMeta();
  });

  function keys(e: KeyboardEvent) {
    const mod = e.ctrlKey || e.metaKey;
    if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement || e.target instanceof HTMLSelectElement) return;
    if (mod && e.key.toLowerCase() === "z") {
      e.preventDefault();
      if (e.shiftKey) redo();
      else undo();
    } else if (mod && e.key.toLowerCase() === "y") {
      e.preventDefault();
      redo();
    } else if (mod && e.key.toLowerCase() === "s") {
      e.preventDefault();
      void push();
    } else if ((e.key === "Delete" || e.key === "Backspace") && ed.key) {
      deleteKeys(selection());
    } else if (mod && ["a", "c", "x", "v", "d"].includes(e.key.toLowerCase()) && !getSelection()?.toString()) {
      // Copy and paste keys, unless the person is copying text on the page.
      const k = e.key.toLowerCase();
      if (k === "a") selectAll();
      else if (k === "v") pasteKey();
      else if (!ed.key) return;
      else if (k === "c") copyKeys(selection());
      else if (k === "x") copyKeys(selection(), true);
      else duplicateKeys(selection());
      e.preventDefault();
    } else if (e.key.startsWith("Arrow") && ed.key && !mod && (document.activeElement === document.body || document.activeElement?.closest(".canvas"))) {
      e.preventDefault();
      const [dx, dy] = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] }[e.key] ?? [0, 0];
      shiftKeys(selection(), dx, dy);
    } else if (e.key === "Escape") {
      ed.armed = null;
      ed.moving = null;
      clearSelection();
    }
  }

  function pickProfile(sel: HTMLSelectElement) {
    if (sel.value === "__manage") {
      sel.value = ed.profile;
      panel = "profiles";
      return;
    }
    ed.profile = sel.value;
    ed.page = ed.layout?.profiles.find((p) => p.id === sel.value)?.homePage ?? "";
    ed.key = null;
  }

  function exportJson() {
    if (!ed.layout) return;
    const blob = new Blob([JSON.stringify($state.snapshot(ed.layout), null, 2)], { type: "application/json" });
    const a = document.createElement("a");
    a.href = URL.createObjectURL(blob);
    a.download = "keyside-layout.json";
    a.click();
    setTimeout(() => URL.revokeObjectURL(a.href), 1000);
  }

  async function toggleRecord() {
    if (!recording) {
      await req("recorder.start");
      recording = true;
      return;
    }
    recording = false;
    const steps = await req<any[]>("recorder.stop");
    const pg = page();
    if (!steps.length || !pg) return;
    for (let y = 0; y < pg.rows; y++)
      for (let x = 0; x < pg.cols; x++) {
        if (pg.keys.some((k) => x >= k.x && x < k.x + (k.w ?? 1) && y >= k.y && y < k.y + (k.h ?? 1))) continue;
        const id = newId("k", pg.keys.map((k) => k.id));
        change(() => pg.keys.push({ id, x, y, kind: "button", label: "Recorded", icon: "circle-dot", actions: steps }));
        ed.key = id;
        return;
      }
    ed.error = t("No free cell for the recorded key. Make room and record again.");
  }

  const usedPlugins = $derived(
    new Set(ed.layout?.profiles.flatMap((pr) => pr.pages.flatMap((pg) => pg.keys.flatMap((k) => [...k.actions, ...(k.offActions ?? [])].map((a) => a.type.split("/")[0])))) ?? []),
  );
  const pills = $derived(
    plugins
      .filter((p) => p.status !== "disabled" && usedPlugins.has(p.id))
      .slice(0, 3)
      .map((p) => ({ ...p, port: typeof p.values?.port === "number" ? p.values.port : null })),
  );
</script>

<svelte:window onkeydown={keys} />

<div class="editor">
  <header class="top">
    <div class="brand">
      <button class="btn icon ghost burger" aria-label={t("Button palette")} onclick={() => (paletteOpen = !paletteOpen)}><Icon name="layout-grid" /></button>
      <Icon name="layout-grid" size={30} />
      <b>Keyside</b><span class="sub">{t("Editor")}</span>
    </div>
    <label class="layout">
      <span>{t("Layout")}</span>
      <select value={ed.profile} onchange={(e) => pickProfile(e.currentTarget)}>
        {#each ed.layout?.profiles ?? [] as p (p.id)}<option value={p.id}>{p.name}</option>{/each}
        <option value="__manage">{t("Manage layouts…")}</option>
      </select>
    </label>
    <div class="pills">
      {#each pills as p (p.id)}
        {@const live = deck.connections.find((c) => c.id === p.id)}
        {@const st = live?.status ?? p.status}
        <svelte:element this={admin ? "button" : "span"} class="chip" title={live?.detail ? plain(live.detail) : undefined} onclick={admin ? () => (ed.setup = p.id) : undefined} role={admin ? undefined : "status"}>
          <span class="dot" class:ok={st === "connected"} class:busy={st === "connecting"} class:bad={st === "error"}></span>{p.name}{#if p.port}<span class="mono port">:{p.port}</span>{/if}
        </svelte:element>
      {/each}
      {#if admin}<span class="chip"><span class="dot" class:ok={devicesOnline > 0}></span>{t("{n} phones online", { n: devicesOnline })}</span>{/if}
    </div>
    <div class="actions">
      {#if ed.dirty}
        <span class="saved">{t("Draft saved {t} ago", { t: ago(ed.savedAt, now) })}</span>
      {:else if loaded}
        <span class="saved">{t("Up to date")}</span>
      {/if}
      <button class="btn icon ghost" aria-label={t("Undo")} title={t("Undo")} disabled={!canUndo()} onclick={undo}><Icon name="undo-2" /></button>
      <button class="btn icon ghost" aria-label={t("Redo")} title={t("Redo")} disabled={!canRedo()} onclick={redo}><Icon name="redo-2" /></button>
      <button class="btn" onclick={() => (panel = "share")}>{t("Import")}</button>
      <button class="btn" onclick={exportJson}>{t("Export JSON")}</button>
      <button class="btn primary" onclick={push} disabled={!ed.dirty || ed.pushing}>{ed.pushing ? t("Pushing…") : t("Push to phones")}</button>
      <button class="btn icon" aria-label={t("More")} aria-expanded={menu} onclick={() => (menu = !menu)}><Icon name="settings" /></button>
    </div>
  </header>

  {#if menu}
    <nav class="menu" aria-label={t("Editor menu")}>
      <button onclick={() => ((menu = false), ondeck())}><Icon name="smartphone" size={18} />{t("Open the deck")}</button>
      <button onclick={toggleRecord}><Icon name={recording ? "circle-stop" : "circle-dot"} size={18} />{recording ? t("Stop recording steps") : t("Record steps")}</button>
      <button onclick={() => ((menu = false), (panel = "share"))}><Icon name="share-2" size={18} />{t("Share and gallery")}</button>
      <button onclick={() => ((menu = false), (panel = "plugins"))}><Icon name="plug" size={18} />{t("Plugins")}</button>
      {#if admin}
        <button onclick={() => ((menu = false), (panel = "streamdeck"))}><Icon name="layout-grid" size={18} />{t("Stream Deck plugins")}</button>
        <button onclick={() => ((menu = false), (panel = "devices"))}><Icon name="smartphone" size={18} />{t("Devices")}</button>
        <button onclick={() => ((menu = false), (panel = "review"))}><Icon name="shield-check" size={18} />{t("Review imported steps")}{#if reviewCount}<span class="count">{reviewCount}</span>{/if}</button>
        <button onclick={() => ((menu = false), (panel = "settings"))}><Icon name="settings-2" size={18} />{t("Settings")}</button>
      {/if}
      {#if ed.dirty}<button class="danger" onclick={() => ((menu = false), discard())}><Icon name="undo-2" size={18} />{t("Discard draft")}</button>{/if}
    </nav>
  {/if}

  {#if recording}<div class="banner rec" role="status"><Icon name="circle-dot" size={18} />{t("Recording: press keys on any device. Stop to turn them into one key.")}<button class="btn small" onclick={toggleRecord}>{t("Stop")}</button></div>{/if}
  {#if ed.error}
    <div class="banner err" role="alert">
      {ed.error}
      {#if ed.conflict}<button class="btn small" onclick={() => load(false)}>{t("Reload")}</button>{/if}
      <button class="btn icon ghost" aria-label={t("Dismiss")} onclick={() => ((ed.error = ""), (ed.conflict = false))}><Icon name="x" size={18} /></button>
    </div>
  {/if}
  {#if reviewCount && admin}<div class="banner warn" role="status"><Icon name="shield" size={18} />{t("{n} imported steps wait for your review.", { n: reviewCount })}<button class="btn small" onclick={() => (panel = "review")}>{t("Review")}</button></div>{/if}

  <div class="panes" class:palette-open={paletteOpen}>
    <div class="left">
      <Palette {admin} />
      <footer class="serving">
        {#if admin}<span>{t("Companion on")}</span><span class="mono">{deck.companion?.name}</span>{/if}
      </footer>
    </div>
    {#if loaded && ed.layout}
      <Canvas />
    {:else}
      <div class="loading">{t("Loading layout…")}</div>
    {/if}
    <div class="right">
      {#if loaded}<Inspector {admin} />{/if}
      {#if admin && loaded}<PairPanel onmanage={() => (panel = "devices")} />{/if}
    </div>
  </div>
</div>

<Sheet open={panel === "plugins"} onclose={() => ((panel = null), (ed.setup = null))} title={t("Plugins")} wide><Plugins {admin} focus={ed.setup} /></Sheet>
<Sheet open={panel === "settings"} onclose={() => (panel = null)} title={t("Settings")} wide><Settings /></Sheet>
<Sheet open={panel === "review"} onclose={() => ((panel = null), refreshMeta())} title={t("Review imported steps")} wide><Review onapproved={() => ((panel = null), load(false), refreshMeta())} /></Sheet>
<Sheet open={panel === "share"} onclose={() => (panel = null)} title={t("Share and gallery")} wide><Share {admin} onimported={() => (panel = null)} /></Sheet>
<Sheet open={panel === "streamdeck"} onclose={() => (panel = null)} title={t("Stream Deck plugins")} wide><StreamDeck /></Sheet>
<Sheet open={panel === "devices"} onclose={() => (panel = null)} title={t("Devices")} wide><PairPanel /></Sheet>
<Sheet open={panel === "profiles"} onclose={() => (panel = null)} title={t("Layouts")} wide><Profiles /></Sheet>

<style>
  .editor {
    height: 100dvh;
    display: flex;
    flex-direction: column;
    background: var(--surface);
  }
  .top {
    display: flex;
    align-items: center;
    gap: 20px;
    padding: 12px 20px;
    border-bottom: 1px solid var(--line-soft);
    flex-wrap: wrap;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    color: var(--accent);
  }
  .brand b {
    color: var(--text);
    font-size: 1.2rem;
  }
  .brand .sub {
    color: var(--muted);
  }
  .burger {
    display: none;
  }
  .layout {
    display: flex;
    align-items: center;
    gap: 10px;
    color: var(--muted);
    padding-left: 20px;
    border-left: 1px solid var(--line-soft);
  }
  .layout select {
    min-height: 40px;
    min-width: 180px;
    padding: 0 10px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--line);
    background: var(--bg);
    color: var(--text);
  }
  .pills {
    display: flex;
    gap: 8px;
    flex: 1;
    min-width: 0;
    overflow: hidden;
  }
  .pills .chip {
    background: var(--bg);
    font: inherit;
    color: inherit;
    white-space: nowrap;
    min-width: 0;
    max-width: 200px;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  button.chip {
    border: 1px solid transparent;
    cursor: pointer;
  }
  button.chip:hover {
    border-color: var(--line);
  }
  .port {
    color: var(--muted);
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-left: auto;
  }
  .saved {
    color: var(--muted);
    white-space: nowrap;
    margin-right: 4px;
  }
  .menu {
    position: absolute;
    right: 16px;
    top: 72px;
    z-index: 10;
    display: flex;
    flex-direction: column;
    min-width: 260px;
    padding: 6px;
    border-radius: 12px;
    border: 1px solid var(--line);
    background: var(--surface);
    box-shadow: 0 16px 40px rgb(0 0 0 / 0.45);
  }
  .menu button {
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 44px;
    padding: 0 12px;
    border: none;
    border-radius: 8px;
    background: none;
    text-align: left;
  }
  .menu button:hover {
    background: var(--hover);
  }
  .menu .danger {
    color: var(--danger);
  }
  .count {
    margin-left: auto;
    padding: 1px 8px;
    border-radius: 10px;
    background: var(--warning);
    color: var(--bg);
    font-size: 0.8rem;
    font-weight: 700;
  }
  .banner {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 20px;
  }
  .banner.err {
    background: var(--danger-bg);
    color: var(--danger-text);
  }
  .banner.warn {
    background: #2e2a1f;
    color: var(--warning);
  }
  .banner.rec {
    background: var(--danger-chip);
    color: var(--danger-text);
  }
  .banner .btn {
    margin-left: auto;
  }
  .small {
    min-height: 32px;
    padding: 0 12px;
  }
  .panes {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: 340px 1fr 440px;
  }
  .left,
  .right {
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: auto;
  }
  .left {
    border-right: 1px solid var(--line-soft);
  }
  .right {
    border-left: 1px solid var(--line-soft);
  }
  .serving {
    margin-top: auto;
    padding: 16px 20px;
    border-top: 1px solid var(--line-soft);
    color: var(--muted);
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: 0.9rem;
  }
  .serving .mono {
    color: var(--text);
  }
  .loading {
    display: grid;
    place-items: center;
    color: var(--muted);
    background: var(--bg);
  }

  @media (max-width: 1199px) {
    .panes {
      grid-template-columns: 1fr 380px;
    }
    .left {
      display: none;
    }
    .palette-open .left {
      display: flex;
      position: fixed;
      inset: 0 auto 0 0;
      width: min(340px, 90vw);
      z-index: 20;
      background: var(--surface);
      box-shadow: 16px 0 40px rgb(0 0 0 / 0.5);
    }
    .burger {
      display: inline-flex;
    }
    .pills {
      display: none;
    }
  }
  @media (max-width: 767px) {
    .panes {
      grid-template-columns: 1fr;
      grid-auto-rows: minmax(60dvh, auto);
      overflow: auto;
    }
    .right {
      border-left: none;
      border-top: 1px solid var(--line-soft);
      overflow: visible;
    }
    .layout {
      border: none;
      padding: 0;
    }
    .saved {
      display: none;
    }
    .top {
      gap: 10px;
    }
  }
</style>

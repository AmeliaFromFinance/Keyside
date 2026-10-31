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
  import Styles from "./Styles.svelte";
  import Triggers from "./Triggers.svelte";
  import TimerOverlay from "./TimerOverlay.svelte";
  import Splitter from "./Splitter.svelte";
  import Connections from "./Connections.svelte";
  import Pages from "./Pages.svelte";
  import KeySearch from "./KeySearch.svelte";
  import PushReview from "./PushReview.svelte";
  import TemplateSave from "./TemplateSave.svelte";
  import ImageCrop from "./ImageCrop.svelte";
  import { cropping, applyCrop } from "./images.svelte.ts";
  import { ed, load, push, discard, undo, redo, canUndo, canRedo, startLive, change, newId, page, profile, deleteKeys, copyKeys, pasteKey, duplicateKeys, shiftKeys, selection, selectAll, clearSelection, keySteps, templates } from "./state.svelte.ts";
  import Wizard from "./Wizard.svelte";
  import History from "./History.svelte";
  import { deck, req } from "../lib/deck.svelte.ts";
  import { t } from "../lib/i18n.svelte.ts";
  import { ago, everySecond } from "../lib/format.ts";
  import { zoomControls } from "../lib/zoom.ts";

  let { admin = false, ondeck }: { admin?: boolean; ondeck: () => void } = $props();

  type Panel = "plugins" | "settings" | "review" | "share" | "streamdeck" | "devices" | "profiles" | "styles" | "search" | "push" | "triggers" | "wizard" | "history" | "pages" | "overlay" | null;
  let panel = $state<Panel>(null);
  let menu = $state(false);
  let paletteOpen = $state(false);
  let now = $state(Date.now());
  let loaded = $state(false);
  let recording = $state(false);
  let plugins = $state<Array<{ id: string; name: string; status: string; values: Record<string, unknown> }>>([]);
  let devicesOnline = $state(0);
  let reviewCount = $state(0);
  let firstRun = $state(false);
  let setupChecked = false;

  // Column widths and folds, remembered on this computer.
  const PANES = { left: 340, right: 440, leftOpen: true, rightOpen: true, pairOpen: true, pair: 0 };
  let panes = $state(readPanes());
  let rightH = $state(0);
  let pairH = $state(0);
  function readPanes(): typeof PANES {
    try {
      return { ...PANES, ...JSON.parse(localStorage.getItem("editorPanes") ?? "{}") };
    } catch {
      return { ...PANES };
    }
  }
  $effect(() => {
    const json = JSON.stringify(panes);
    try {
      localStorage.setItem("editorPanes", json);
    } catch {
      /* storage blocked */
    }
  });

  $effect(() => everySecond((n) => (now = n)));
  $effect(() => {
    if (ed.setup) panel = "plugins";
  });
  $effect(() => {
    if (ed.styling) panel = "styles";
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

  // A fresh install opens the setup wizard once the layout is in.
  $effect(() => {
    if (!loaded || !admin || setupChecked) return;
    setupChecked = true;
    req<{ fresh: boolean }>("setup.state").then(
      (s) => {
        firstRun = s.fresh;
        if (s.fresh && !panel) panel = "wizard";
      },
      () => (setupChecked = false),
    );
  });

  function closeWizard() {
    panel = null;
    firstRun = false;
    void req("setup.done").catch(() => {});
  }

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
    if (mod && e.key.toLowerCase() === "k" && loaded && !e.shiftKey && !e.altKey) {
      e.preventDefault();
      panel = panel === "search" ? null : "search";
      return;
    }
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
    new Set(ed.layout?.profiles.flatMap((pr) => pr.pages.flatMap((pg) => pg.keys.flatMap((k) => keySteps(k).map((a) => a.type.split("/")[0])))) ?? []),
  );
  const pills = $derived(
    plugins
      .filter((p) => p.status !== "disabled" && usedPlugins.has(p.id))
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
      <Connections apps={pills} {admin} phones={devicesOnline} onsetup={(id) => (ed.setup = id)} ondevices={() => (panel = "devices")} />
    </div>
    <div class="actions">
      {#if ed.dirty}
        <span class="saved">{t("Draft saved {t} ago", { t: ago(ed.savedAt, now) })}</span>
      {:else if loaded}
        <span class="saved quiet">{t("Up to date")}</span>
      {/if}
      <button class="btn icon ghost" aria-label={t("Search all keys")} title={t("Search all keys ({shortcut})", { shortcut: /Mac|iPhone|iPad/.test(navigator.platform) ? "⌘K" : "Ctrl+K" })} disabled={!loaded} onclick={() => (panel = "search")}><Icon name="search" /></button>
      <button class="btn icon ghost" aria-label={t("Undo")} title={t("Undo")} disabled={!canUndo()} onclick={undo}><Icon name="undo-2" /></button>
      <button class="btn icon ghost" aria-label={t("Redo")} title={t("Redo")} disabled={!canRedo()} onclick={redo}><Icon name="redo-2" /></button>
      <button class="btn import" onclick={() => (panel = "share")}>{t("Import")}</button>
      <button class="btn primary" onclick={() => (panel = "push")} disabled={!ed.dirty || ed.pushing}>{ed.pushing ? t("Pushing…") : t("Push to phones")}</button>
      <button class="btn icon" aria-label={t("More")} aria-expanded={menu} onclick={() => (menu = !menu)}><Icon name="settings" /></button>
    </div>
  </header>

  {#if menu}
    <nav class="menu" aria-label={t("Editor menu")}>
      <button onclick={() => ((menu = false), ondeck())}><Icon name="smartphone" size={18} />{t("Open the deck")}</button>
      <button onclick={toggleRecord}><Icon name={recording ? "circle-stop" : "circle-dot"} size={18} />{recording ? t("Stop recording steps") : t("Record steps")}</button>
      <button onclick={() => ((menu = false), (panel = "styles"))}><Icon name="palette" size={18} />{t("Key styles")}</button>
      <button onclick={() => ((menu = false), (panel = "share"))}><Icon name="share-2" size={18} />{t("Share and gallery")}</button>
      <button onclick={() => ((menu = false), exportJson())}><Icon name="file-down" size={18} />{t("Export JSON")}</button>
      <button onclick={() => ((menu = false), (panel = "pages"))}><Icon name="layers" size={18} />{t("All pages")}</button>
      <button onclick={() => ((menu = false), (panel = "history"))}><Icon name="history" size={18} />{t("History")}</button>
      <button onclick={() => ((menu = false), (panel = "plugins"))}><Icon name="plug" size={18} />{t("Plugins")}</button>
      {#if admin}
        <button onclick={() => ((menu = false), (panel = "streamdeck"))}><Icon name="layout-grid" size={18} />{t("Stream Deck plugins")}</button>
        <button onclick={() => ((menu = false), (panel = "triggers"))}><Icon name="messages-square" size={18} />{t("Chat triggers")}</button>
        <button onclick={() => ((menu = false), (panel = "overlay"))}><Icon name="monitor-play" size={18} />{t("Text overlay for OBS")}</button>
        <button onclick={() => ((menu = false), (panel = "devices"))}><Icon name="smartphone" size={18} />{t("Devices")}</button>
        <button onclick={() => ((menu = false), (panel = "review"))}><Icon name="shield-check" size={18} />{t("Review imported steps")}{#if reviewCount}<span class="count">{reviewCount}</span>{/if}</button>
        <button onclick={() => ((menu = false), (panel = "wizard"))}><Icon name="sparkles" size={18} />{t("Setup wizard")}</button>
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

  <div class="panes" class:palette-open={paletteOpen} style:--left="{panes.leftOpen ? panes.left : 44}px" style:--right="{panes.rightOpen ? panes.right : 44}px">
    <div class="left" class:folded={!panes.leftOpen}>
      {#if panes.leftOpen}
        <div class="scroll">
          <Palette {admin} />
          <footer class="serving">
            {#if admin}<span>{t("Companion on")}</span><span class="mono">{deck.companion?.name}</span>{/if}
          </footer>
        </div>
        <Splitter edge="right" value={panes.left} min={240} max={600} label={t("Resize the button palette")} onchange={(v) => (panes.left = v)} onreset={() => (panes.left = PANES.left)}>
          <button class="fold" title={t("Hide the button palette")} aria-label={t("Hide the button palette")} onclick={() => (panes.leftOpen = false)}><Icon name="chevron-left" size={14} /></button>
        </Splitter>
      {:else}
        <button class="rail" title={t("Show the button palette")} onclick={() => (panes.leftOpen = true)}><Icon name="chevron-right" size={18} /><span>{t("Button palette")}</span></button>
      {/if}
    </div>
    {#if loaded && ed.layout}
      <Canvas onpages={() => (panel = "pages")} />
    {:else}
      <div class="loading">{t("Loading layout…")}</div>
    {/if}
    <div class="right" class:folded={!panes.rightOpen} bind:clientHeight={rightH}>
      {#if panes.rightOpen}
        <Splitter edge="left" value={panes.right} min={320} max={760} label={t("Resize the side panel")} onchange={(v) => (panes.right = v)} onreset={() => (panes.right = PANES.right)}>
          <button class="fold" title={t("Hide the side panel")} aria-label={t("Hide the side panel")} onclick={() => (panes.rightOpen = false)}><Icon name="chevron-right" size={14} /></button>
        </Splitter>
        <div class="ins">{#if loaded}<Inspector {admin} onpages={() => (panel = "pages")} />{/if}</div>
        {#if admin && loaded}
          <div class="pairbox" class:sized={panes.pairOpen && panes.pair > 0} style:height={panes.pairOpen && panes.pair > 0 ? `${panes.pair}px` : undefined} bind:clientHeight={pairH}>
            {#if panes.pairOpen}
              <Splitter edge="top" value={panes.pair || pairH} min={140} max={Math.max(160, rightH - 160)} label={t("Resize Pair a phone")} onchange={(v) => (panes.pair = v)} onreset={() => (panes.pair = 0)} />
            {/if}
            <PairPanel foldable bind:expanded={panes.pairOpen} onmanage={() => (panel = "devices")} />
          </div>
        {/if}
      {:else}
        <button class="rail" title={t("Show the side panel")} onclick={() => (panes.rightOpen = true)}><Icon name="chevron-left" size={18} /><span>{t("Key settings")}</span></button>
      {/if}
    </div>
  </div>
</div>

<Sheet open={panel === "plugins"} onclose={() => ((panel = null), (ed.setup = null))} title={t("Plugins")} wide><Plugins {admin} focus={ed.setup} /></Sheet>
<Sheet open={panel === "settings"} onclose={() => (panel = null)} title={t("Settings")} wide><Settings /></Sheet>
<Sheet open={panel === "review"} onclose={() => ((panel = null), refreshMeta())} title={t("Review imported steps")} wide><Review onapproved={() => ((panel = null), load(false), refreshMeta())} /></Sheet>
<Sheet open={panel === "share"} onclose={() => (panel = null)} title={t("Share and gallery")} wide><Share {admin} onimported={() => (panel = null)} /></Sheet>
<Sheet open={panel === "streamdeck"} onclose={() => (panel = null)} title={t("Stream Deck plugins")} wide><StreamDeck /></Sheet>
<Sheet open={panel === "devices"} onclose={() => (panel = null)} title={t("Devices")} wide><PairPanel /></Sheet>
<Sheet open={panel === "pages"} onclose={() => (panel = null)} title={t("Pages in {name}", { name: profile()?.name ?? "" })}><Pages onopen={() => (panel = null)} /></Sheet>
<Sheet open={panel === "search"} onclose={() => (panel = null)} title={t("Search all keys")}><KeySearch onclose={() => (panel = null)} /></Sheet>
<Sheet open={panel === "push"} onclose={() => (panel = null)} title={t("Review before pushing")}>
  <PushReview oncancel={() => (panel = null)} onpush={async () => (await push(), !ed.error && (panel = null))} />
</Sheet>
<Sheet open={!!templates.asking} onclose={() => (templates.asking = null)} title={t("Save as template")}>
  {#if templates.asking}<TemplateSave ids={templates.asking} onclose={() => (templates.asking = null)} />{/if}
</Sheet>
<Sheet open={!!cropping.file} onclose={() => (cropping.file = null)} title={t("Crop image")}>
  {#if cropping.file}<ImageCrop file={cropping.file} oncancel={() => (cropping.file = null)} onresult={applyCrop} />{/if}
</Sheet>
<Sheet open={panel === "profiles"} onclose={() => (panel = null)} title={t("Layouts")} wide><Profiles /></Sheet>
<Sheet open={panel === "styles"} onclose={() => ((panel = null), (ed.styling = false))} title={t("Key styles")} wide><Styles /></Sheet>
<Sheet open={panel === "triggers"} onclose={() => (panel = null)} title={t("Chat triggers")} wide><Triggers /></Sheet>
<Sheet open={panel === "overlay"} onclose={() => (panel = null)} title={t("Text overlay for OBS")}><TimerOverlay kind="text" /></Sheet>
<Sheet open={panel === "wizard"} onclose={closeWizard} title={t("Set up Keyside")} wide><Wizard fresh={firstRun} onclose={closeWizard} /></Sheet>
<Sheet open={panel === "history"} onclose={() => (panel = null)} title={t("History")} wide><History onrestored={() => (panel = null)} /></Sheet>

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
    flex: 1;
    min-width: 0;
  }
  /* Hide the save note on smaller screens. The bright Push button already shows a draft. */
  @media (max-width: 1399px) {
    .saved {
      display: none;
    }
  }
  /* Import stays in the menu under Share and gallery. */
  @media (max-width: 1199px) {
    .import,
    .brand .sub {
      display: none;
    }
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
    grid-template-columns: var(--left) 1fr var(--right);
  }
  .left,
  .right {
    position: relative;
    display: flex;
    flex-direction: column;
    min-height: 0;
    min-width: 0;
  }
  .left {
    border-right: 1px solid var(--line-soft);
  }
  .right {
    border-left: 1px solid var(--line-soft);
  }
  .scroll,
  .ins {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: auto;
  }
  .ins > :global(.inspector) {
    flex: 1;
  }
  .pairbox {
    position: relative;
    flex: none;
    max-height: 70%;
    overflow: auto;
  }
  .pairbox.sized {
    max-height: none;
  }
  /* Fold button on the column edge. */
  .fold {
    position: relative;
    z-index: 1;
    width: 22px;
    height: 44px;
    padding: 0;
    display: grid;
    place-items: center;
    border-radius: 11px;
    border: 1px solid var(--line);
    background: var(--raised);
    color: var(--muted);
    cursor: pointer;
  }
  .fold:hover {
    color: var(--text);
    border-color: var(--accent);
  }
  .rail {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    padding: 14px 0;
    border: none;
    background: none;
    color: var(--muted);
    font: inherit;
    cursor: pointer;
  }
  .rail span {
    writing-mode: vertical-rl;
  }
  .rail:hover {
    color: var(--text);
    background: var(--hover);
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
      grid-template-columns: 1fr var(--right);
    }
    .left {
      display: none;
    }
    /* The palette is a drawer here, so it doesn't fold or resize. */
    .left :global(.split),
    .left .rail {
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
    }
    .right :global(.split) {
      display: none;
    }
    .rail span {
      writing-mode: horizontal-tb;
    }
    .layout {
      border: none;
      padding: 0;
    }
    .pills {
      display: none;
    }
    .top {
      gap: 10px;
    }
  }
</style>

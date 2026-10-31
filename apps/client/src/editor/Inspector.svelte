<script lang="ts">
  import type { Action, Key, KeyColor, KeyKind, KeyState, Tone } from "@keyside/protocol";
  import Icon from "../components/Icon.svelte";
  import Sheet from "../components/Sheet.svelte";
  import IconPicker from "./IconPicker.svelte";
  import SettingsForm from "./SettingsForm.svelte";
  import SdPanel from "./SdPanel.svelte";
  import TimerOverlay from "./TimerOverlay.svelte";
  import { ed, page, profile, selectedKey, change, actionDef, allActions, newAction, deleteKey, deletePage, fits, selection, duplicateKeys, copyKeys, deleteKeys, clearSelection, moveToPage, keySteps, plainPressReason } from "./state.svelte.ts";
  import { actionMatches, searchWords } from "./search.ts";
  import { req, deck } from "../lib/deck.svelte.ts";
  import { plain } from "../lib/rich.ts";
  import { t, mark } from "../lib/i18n.svelte.ts";

  let { admin = false, onpages }: { admin?: boolean; onpages?: () => void } = $props();

  /** A step list on the key: normal, toggle off, long press, double tap, or one state ("s0"…). */
  type Which = "on" | "off" | "long" | "double" | `s${number}`;

  /** Where the icon picker puts its choice: the key, or one of its states. */
  let picking = $state<"key" | number | null>(null);
  let adding = $state<Which | null>(null);
  let openState = $state<number | null>(0);
  let open = $state<string | null>(null);
  let testing = $state(false);
  let addQuery = $state("");

  const key = $derived(selectedKey());
  const group = $derived(selection());
  const others = $derived((profile()?.pages ?? []).filter((p) => p.id !== ed.page));
  const pg = $derived(page());

  /** Plugins this key uses that aren't connected, so setup is one click away. */
  const unready = $derived.by(() => {
    if (!key) return [];
    const used = new Set(keySteps(key).map((a) => a.type.split("/")[0]));
    return ed.catalog.plugins
      .filter((p) => used.has(p.id))
      .map((p) => ({ p, c: deck.connections.find((c) => c.id === p.id) }))
      .filter(({ c }) => c?.status !== "connected")
      .map(({ p, c }) => ({
        id: p.id,
        name: p.name,
        bad: c?.status === "error",
        text: c?.detail ? `${p.name}: ${plain(c.detail)}` : !c && !p.running ? t("{name} isn't running.", { name: p.name }) : t("{name} isn't connected yet.", { name: p.name }),
      }));
  });

  const colors: Array<[KeyColor, string]> = [
    ["default", mark("Default")],
    ["teal", mark("Teal")],
    ["violet", mark("Violet")],
    ["green", mark("Green")],
    ["amber", mark("Amber")],
    ["red", mark("Red")],
    ["blue", mark("Blue")],
  ];
  const kinds: Array<[KeyKind, string]> = [
    ["button", mark("Button")],
    ["toggle", mark("Toggle")],
    ["folder", mark("Folder")],
    ["back", mark("Back")],
    ["slider", mark("Slider")],
    ["widget", mark("Text or chat")],
    ["multi", mark("Multi-state")],
  ];
  const tones: Array<[Tone, string]> = [
    ["accent", mark("Highlight")],
    ["danger", mark("Red")],
    ["success", mark("Green text")],
    ["warning", mark("Amber text")],
  ];

  function edit(fn: (k: Key) => void) {
    const k = selectedKey();
    if (k) change(() => fn(k));
  }

  function stepsOf(k: Key | undefined, w: Which, create = false): Action[] {
    if (!k) return [];
    if (w === "on") return k.actions;
    if (w === "off") return create ? (k.offActions ??= []) : (k.offActions ?? []);
    if (w === "long") return create ? (k.longActions ??= []) : (k.longActions ?? []);
    if (w === "double") return create ? (k.doubleActions ??= []) : (k.doubleActions ?? []);
    return k.states?.[Number(w.slice(1))]?.actions ?? [];
  }

  function list(w: Which): Action[] {
    return stepsOf(key, w);
  }

  /** Empty long press and double tap lists are left out of the layout. */
  function tidy(k: Key) {
    if (!k.longActions?.length) delete k.longActions;
    if (!k.doubleActions?.length) delete k.doubleActions;
  }

  /** How far each step is indented inside If blocks. */
  function depths(steps: Action[]): number[] {
    let d = 0;
    return steps.map((a) => {
      if (a.type === "core.else") return Math.max(0, d - 1);
      if (a.type === "core.end") return (d = Math.max(0, d - 1));
      return a.type === "core.if" ? d++ : d;
    });
  }

  const OPS: Record<string, string> = { eq: mark("is"), ne: mark("is not"), contains: mark("contains"), gt: mark("is above"), lt: mark("is below"), truthy: mark("is on"), falsy: mark("is off") };

  function editStep(w: Which, i: number, fn: (a: Action) => void) {
    edit((k) => {
      const l = stepsOf(k, w);
      if (l[i]) fn(l[i]);
    });
  }

  function moveStep(w: Which, i: number, d: number) {
    edit((k) => {
      const l = stepsOf(k, w);
      const j = i + d;
      if (j < 0 || j >= l.length) return;
      [l[i], l[j]] = [l[j], l[i]];
    });
  }

  function removeStep(w: Which, i: number) {
    edit((k) => {
      stepsOf(k, w).splice(i, 1);
      tidy(k);
    });
  }

  function addStep(type: string) {
    const w = adding;
    if (!w) return;
    edit((k) => {
      const l = stepsOf(k, w, true);
      // An If comes with its End. Otherwise goes inside the last If block.
      const end = l.map((a) => a.type).lastIndexOf("core.end");
      if (type === "core.else" && end >= 0) l.splice(end, 0, newAction(type));
      else l.push(newAction(type), ...(type === "core.if" ? [newAction("core.end")] : []));
    });
    adding = null;
    addQuery = "";
  }

  /** Turns a key into a multi-state key, or back. Its steps go to the first state and back. */
  function setKind(k: Key, kind: KeyKind) {
    if (kind === "multi" && k.kind !== "multi" && !k.states?.length) {
      k.states = [
        { label: k.label || t("State 1"), sub: k.sub, actions: k.actions },
        { label: t("State 2"), actions: [] },
      ];
      k.actions = [];
    }
    if (kind !== "multi" && k.kind === "multi") {
      if (!k.actions.length && k.states?.length) k.actions = k.states[0].actions;
      delete k.states;
      delete k.follow;
    }
    k.kind = kind;
    if (kind === "slider") k.slider ??= { min: 0, max: 1 };
    if (kind === "widget") k.widget ??= { text: "" };
    if (kind === "folder") k.target ??= profile()?.pages.find((p) => p.folder)?.id;
    if (!["button", "toggle", "multi"].includes(kind)) {
      delete k.longActions;
      delete k.doubleActions;
      delete k.longMs;
      delete k.doubleMs;
    }
  }

  function editState(i: number, fn: (s: KeyState) => void) {
    edit((k) => {
      const st = k.states?.[i];
      if (st) fn(st);
    });
  }

  function addState() {
    edit((k) => {
      const n = (k.states ??= []).length;
      if (n < 8) k.states.push({ label: t("State {n}", { n: n + 1 }), actions: [] });
    });
    openState = (key?.states?.length ?? 1) - 1;
  }

  function moveState(i: number, d: number) {
    edit((k) => {
      const l = k.states ?? [];
      const j = i + d;
      if (j < 0 || j >= l.length) return;
      [l[i], l[j]] = [l[j], l[i]];
    });
    if (openState === i) openState = i + d;
  }

  function removeState(i: number) {
    edit((k) => {
      if ((k.states?.length ?? 0) > 2) k.states!.splice(i, 1);
    });
    openState = null;
  }

  /** Typed like the companion does for "set variable": true, false and numbers aren't text. */
  function typed(v: string): string | number | boolean | undefined {
    if (v === "") return undefined;
    if (v === "true" || v === "false") return v === "true";
    return v.trim() !== "" && !Number.isNaN(Number(v)) ? Number(v) : v;
  }

  function toggleStyle(id: string) {
    edit((k) => {
      const l = k.styles ?? [];
      k.styles = l.includes(id) ? l.filter((s) => s !== id) : [...l, id];
      if (!k.styles.length) delete k.styles;
    });
  }

  function badge(a: Action): string {
    if (a.type === "core.wait") return "WAIT";
    if (a.type === "core.if") return "IF";
    if (a.type === "core.else") return "ELSE";
    if (a.type === "core.end") return "END";
    const plugin = ed.catalog.plugins.find((p) => a.type.startsWith(`${p.id}/`));
    if (plugin) return plugin.prefix.toUpperCase().slice(0, 6);
    return "CORE";
  }

  function summary(a: Action): string {
    if (a.type === "core.wait") return t("Delay {ms} ms", { ms: String(a.settings.ms ?? 0) });
    if (a.type === "core.if") {
      const op = String(a.settings.op ?? "truthy");
      const value = op === "truthy" || op === "falsy" ? "" : ` ${a.settings.value ?? ""}`;
      return t("If {var} {check}{value}", { var: String(a.settings.var || "…"), check: t(OPS[op] ?? op), value });
    }
    if (a.type === "core.else") return t("Otherwise");
    if (a.type === "core.end") return t("End if");
    const def = actionDef(a.type);
    const name = t(def?.name ?? a.type);
    const detail = Object.entries(a.settings)
      .filter(([, v]) => typeof v === "string" || typeof v === "number")
      .slice(0, 2)
      .map(([, v]) => String(v))
      .join(" ");
    return detail ? `${name} ${detail}` : name;
  }

  async function test() {
    const k = selectedKey();
    if (!k) return;
    testing = true;
    try {
      await req("steps.test", { actions: $state.snapshot(mainSteps) });
    } catch (e: any) {
      ed.error = e.message;
    } finally {
      setTimeout(() => (testing = false), 600);
    }
  }

  /** Multi-state keys: the state the deck shows now. */
  const current = $derived(key?.kind === "multi" ? Math.min((key.states?.length ?? 1) - 1, ed.live?.keys.find((k) => k.id === key.id)?.state ?? 0) : 0);
  /** What "Test run" runs: the normal steps, or the current state's. */
  const mainSteps = $derived(key?.kind === "multi" ? (key.states?.[current]?.actions ?? []) : (key?.actions ?? []));
  const gestures = $derived(!!key && ["button", "toggle", "multi"].includes(key.kind));
  const hasGestures = $derived(!!key?.longActions?.length || !!key?.doubleActions?.length);
  const blocked = $derived(key ? plainPressReason(key) : "");
  const styles = $derived(ed.layout?.styles ?? []);
  const mainLists = $derived<Which[]>(key?.kind === "toggle" ? ["on", "off"] : ["on"]);
  const sameBatch = $derived(list("on").some((a, i, l) => i > 0 && a.type.includes("/") && l[i - 1].type.split("/")[0] === a.type.split("/")[0]));
  const varNames = $derived(Object.keys(ed.catalog.variables).sort());
  const fb = $derived(key?.feedback?.[0]);
  const feeds = $derived(ed.catalog.plugins.flatMap((p: any) => (p.feeds ?? []).map((f: any) => ({ value: `${p.prefix}.${f.name}`, label: `${p.name}: ${f.title}` }))));
  const choices = $derived(
    allActions()
      .filter((a) => admin || !a.adminOnly)
      .filter((a) => actionMatches(a, searchWords(addQuery))),
  );

  function resize(dw: number, dh: number) {
    const k = selectedKey();
    const p = page();
    if (!k || !p) return;
    const w = Math.max(1, (k.w ?? 1) + dw);
    const h = Math.max(1, (k.h ?? 1) + dh);
    if (!fits(p, k.x, k.y, w, h, k.id)) return;
    edit((q) => {
      q.w = w > 1 ? w : undefined;
      q.h = h > 1 ? h : undefined;
    });
  }

  function setFeedback(patch: Partial<{ var: string; op: string; value: string; tone: Tone; badge: string }>) {
    edit((k) => {
      const cur = k.feedback?.[0] ?? { when: { var: "", op: "eq" as const }, style: { tone: "accent" as Tone } };
      const next = {
        when: { var: patch.var ?? cur.when.var, op: (patch.op ?? cur.when.op) as any, value: patch.value ?? cur.when.value },
        style: { ...cur.style, ...(patch.tone ? { tone: patch.tone } : {}), ...(patch.badge !== undefined ? { badge: patch.badge || undefined } : {}) },
      };
      k.feedback = next.when.var ? [next] : [];
    });
  }
</script>

<aside class="inspector">
  {#if group.length > 1}
    <header>
      <div>
        <h2>{t("{n} keys", { n: group.length })}</h2>
        <span class="ref">{pg?.keys.filter((k) => group.includes(k.id)).map((k) => k.label || t("Untitled key")).join(", ")}</span>
      </div>
    </header>
    <p class="hint">{t("Drag one of the selected keys to move them all, or onto a page tab to move them there.")}</p>
    <div class="foot group">
      <button class="btn" onclick={() => duplicateKeys(group)}><Icon name="copy-plus" size={18} />{t("Duplicate")}</button>
      <button class="btn" onclick={() => copyKeys(group)}><Icon name="copy" size={18} />{t("Copy")}</button>
      <button class="btn" onclick={() => copyKeys(group, true)}><Icon name="scissors" size={18} />{t("Cut")}</button>
      <button class="btn" onclick={clearSelection}><Icon name="x" size={18} />{t("Clear selection")}</button>
      <button class="btn danger" onclick={() => deleteKeys(group)}><Icon name="trash-2" size={18} />{t("Delete {n} keys", { n: group.length })}</button>
    </div>
    {#if others.length}
      <label class="field">
        <span>{t("Move to page")}</span>
        <select value="" onchange={(e) => e.currentTarget.value && moveToPage(group, e.currentTarget.value)}>
          <option value="">{t("Choose a page")}</option>
          {#each others as p (p.id)}<option value={p.id}>{p.name}</option>{/each}
        </select>
      </label>
    {/if}
  {:else if key}
    <header>
      <div>
        <h2>{key.label || t("Untitled key")}</h2>
        {#if key.actions.length > 1}<span class="tag">{t("Multi-action")}</span>{/if}
        <span class="ref mono" title={t("Page and key id, for the HTTP API")}>{ed.page}/{key.id}</span>
      </div>
      <button class="btn" onclick={test} disabled={!mainSteps.length || testing}><Icon name="play" size={16} />{testing ? t("Running…") : t("Test run")}</button>
    </header>

    {#each unready as u (u.id)}
      <div class="setup" class:bad={u.bad} role="status">
        <Icon name="plug" size={18} />
        <span>{u.text}</span>
        {#if admin}<button class="btn" onclick={() => (ed.setup = u.id)}>{t("Set up")}</button>{/if}
      </div>
    {/each}

    <div class="two">
      <label class="field"><span>{t("Label")}</span><input value={key.label} maxlength="64" oninput={(e) => edit((k) => (k.label = e.currentTarget.value))} /></label>
      <div class="field">
        <span>{t("Icon")}</span>
        <button class="btn iconbtn" onclick={() => (picking = "key")}>
          {#if key.icon && !key.icon.includes(":")}<Icon name={key.icon} size={18} />{/if}
          {key.icon?.startsWith("asset:") ? t("Image") : (key.icon ?? t("None"))}
        </button>
      </div>
    </div>
    {#if key.kind !== "multi"}
      <label class="field"><span>{t("Second line")}</span><input value={key.sub ?? ""} maxlength="64" placeholder={t("Optional")} oninput={(e) => edit((k) => (k.sub = e.currentTarget.value || undefined))} /></label>
    {/if}

    <div class="two">
      <div class="field">
        <span>{t("Colour")}</span>
        <div class="swatches">
          {#each colors as [c, name] (c)}
            <button class="swatch c-{c}" class:on={(key.color ?? "default") === c} aria-label={t(name)} title={t(name)} onclick={() => edit((k) => (k.color = c === "default" ? undefined : c))}></button>
          {/each}
        </div>
      </div>
      <label class="field">
        <span>{t("Type")}</span>
        <select value={key.kind} onchange={(e) => edit((k) => setKind(k, e.currentTarget.value as KeyKind))}>
          {#each kinds as [k, name] (k)}<option value={k}>{t(name)}</option>{/each}
        </select>
      </label>
    </div>

    {#if key.kind === "folder"}
      <label class="field">
        <span>{t("Opens")}</span>
        <select value={key.target ?? ""} onchange={(e) => edit((k) => (k.target = e.currentTarget.value))}>
          {#each profile()?.pages ?? [] as p (p.id)}{#if p.id !== pg?.id}<option value={p.id}>{p.name}</option>{/if}{/each}
        </select>
      </label>
      <button class="btn" onclick={() => key.target && ((ed.page = key.target), (ed.key = null))}><Icon name="folder-open" size={18} />{t("Edit folder")}</button>
    {/if}

    {#if key.kind === "widget"}
      <label class="field">
        <span>{t("Text")}</span>
        <input value={key.widget?.text ?? ""} maxlength="200" placeholder={"{twitch.viewers} viewers"} oninput={(e) => edit((k) => (k.widget = { ...k.widget, text: e.currentTarget.value || undefined }))} />
        <small>{t("Use {name} to show a variable.", { name: "{name}" })}</small>
      </label>
      <label class="field">
        <span>{t("Chat or alert feed")}</span>
        <select value={key.widget?.feed ?? ""} onchange={(e) => edit((k) => (k.widget = { ...k.widget, feed: e.currentTarget.value || undefined }))}>
          <option value="">{t("None")}</option>
          {#each feeds as f (f.value)}<option value={f.value}>{f.label}</option>{/each}
        </select>
      </label>
      {#if key.widget?.feed}
        <label class="field">
          <span>{t("Max characters")}</span>
          <input
            type="number"
            min="1"
            max="500"
            value={key.widget?.maxChars ?? ""}
            placeholder={t("No limit")}
            oninput={(e) => {
              const n = Math.round(Number(e.currentTarget.value));
              edit((k) => (k.widget = { ...k.widget, maxChars: n >= 1 ? Math.min(n, 500) : undefined }));
            }}
          />
          <small>{t("Longer messages end in “…”.")}</small>
        </label>
      {/if}
    {/if}

    {#if key.kind === "slider"}
      <div class="two">
        <label class="field"><span>{t("Lowest")}</span><input type="number" value={key.slider?.min ?? 0} oninput={(e) => edit((k) => (k.slider = { ...k.slider!, min: Number(e.currentTarget.value) }))} /></label>
        <label class="field"><span>{t("Highest")}</span><input type="number" value={key.slider?.max ?? 1} oninput={(e) => edit((k) => (k.slider = { ...k.slider!, max: Number(e.currentTarget.value) }))} /></label>
      </div>
    {/if}

    <div class="field">
      <span>{t("Styles")}</span>
      {#if styles.length}
        <div class="chips">
          {#each styles as st (st.id)}
            {@const on = !!key.styles?.includes(st.id)}
            <button class="chip" class:on aria-pressed={on} disabled={!on && (key.styles?.length ?? 0) >= 8} onclick={() => toggleStyle(st.id)}>{#if on}<Icon name="check" size={14} />{/if}{st.name}</button>
          {/each}
          <button class="btn small ghost" onclick={() => (ed.styling = true)}><Icon name="palette" size={16} />{t("Manage styles")}</button>
        </div>
      {:else}
        <button class="btn" onclick={() => (ed.styling = true)}><Icon name="palette" size={18} />{t("Create a style")}</button>
        <small>{t("A style changes how keys look while a variable matches, like red while live. Make it once, use it on any key.")}</small>
      {/if}
    </div>

    {#if key.kind !== "folder" && key.kind !== "back"}
      {#if hasGestures && !key.hold && !key.repeat}<p class="note">{t("Hold to confirm and Repeat while held can't be combined with long press or double tap steps.")}</p>{/if}
      <div class="two">
        <label class="check" class:off={hasGestures && !key.hold}>
          <input type="checkbox" checked={!!key.hold} disabled={hasGestures && !key.hold} onchange={(e) => edit((k) => (k.hold = e.currentTarget.checked ? 1000 : undefined))} />
          {t("Hold to confirm")}
        </label>
        {#if key.hold}
          <label class="field inline"><input type="number" min="200" max="10000" step="100" value={key.hold} onchange={(e) => edit((k) => (k.hold = Math.min(10000, Math.max(200, Number(e.currentTarget.value) || 1000))))} /><span>ms</span></label>
        {/if}
      </div>
      {#if key.kind === "button"}
        <div class="two">
          <label class="check" class:off={hasGestures && !key.repeat}>
            <input type="checkbox" checked={!!key.repeat} disabled={hasGestures && !key.repeat} onchange={(e) => edit((k) => (k.repeat = e.currentTarget.checked ? 500 : undefined))} />
            {t("Repeat while held")}
          </label>
          {#if key.repeat}
            <label class="field inline"><span>{t("every")}</span><input type="number" min="100" max="10000" step="50" value={key.repeat} onchange={(e) => edit((k) => (k.repeat = Math.min(10000, Math.max(100, Number(e.currentTarget.value) || 500))))} /><span>ms</span></label>
          {/if}
        </div>
      {/if}

      <div class="field">
        <span>{t("Light up when")}</span>
        <div class="cond">
          <select value={fb?.when.var ?? ""} onchange={(e) => setFeedback({ var: e.currentTarget.value })}>
            <option value="">{t("Never")}</option>
            {#each varNames as v (v)}<option value={v}>{v}</option>{/each}
            {#if fb && !varNames.includes(fb.when.var)}<option value={fb.when.var}>{fb.when.var}</option>{/if}
          </select>
          {#if fb}
            <select value={fb.when.op} onchange={(e) => setFeedback({ op: e.currentTarget.value })}>
              <option value="eq">{t("is")}</option>
              <option value="ne">{t("is not")}</option>
              <option value="contains">{t("contains")}</option>
              <option value="gt">{t("is above")}</option>
              <option value="lt">{t("is below")}</option>
              <option value="truthy">{t("is on")}</option>
              <option value="falsy">{t("is off")}</option>
            </select>
            {#if !["truthy", "falsy"].includes(fb.when.op)}
              <input class="input" value={String(fb.when.value ?? "")} placeholder={String(ed.catalog.variables[fb.when.var] ?? "")} oninput={(e) => setFeedback({ value: e.currentTarget.value })} />
            {/if}
            <select value={fb.style.tone ?? "accent"} onchange={(e) => setFeedback({ tone: e.currentTarget.value as Tone })}>
              {#each tones as [tn, name] (tn)}<option value={tn}>{t(name)}</option>{/each}
            </select>
            <input class="input" value={fb.style.badge ?? ""} maxlength="24" placeholder={t("Badge, e.g. ON AIR")} oninput={(e) => setFeedback({ badge: e.currentTarget.value })} />
          {/if}
        </div>
      </div>

      {#if key.kind === "multi"}
        <section class="states">
          <h3>{t("States · each press runs the one showing, then shows the next")}</h3>
          <label class="field">
            <span>{t("Follow a variable")}</span>
            <select value={key.follow ?? ""} onchange={(e) => edit((k) => (k.follow = e.currentTarget.value || undefined))}>
              <option value="">{t("No, just step through")}</option>
              {#each varNames as v (v)}<option value={v}>{v}</option>{/each}
              {#if key.follow && !varNames.includes(key.follow)}<option value={key.follow}>{key.follow}</option>{/if}
            </select>
            <small>{t("The key shows the state whose value the variable has, even when it changes somewhere else.")}</small>
          </label>
          {#each key.states ?? [] as st, i (i)}
            <div class="state" class:open={openState === i}>
              <button class="summary" aria-expanded={openState === i} onclick={() => (openState = openState === i ? null : i)}>
                <span class="n mono">{i + 1}</span>
                <span class="dot c-{st.color ?? key.color ?? 'default'}"></span>
                {#if (st.icon ?? key.icon) && !(st.icon ?? key.icon)!.includes(":")}<Icon name={st.icon ?? key.icon} size={16} />{/if}
                <span class="what">{st.label || t("Untitled state")}</span>
                {#if key.follow && st.value !== undefined}<span class="tag mono">= {String(st.value)}</span>{/if}
                {#if i === current}<span class="tag now">{t("Showing")}</span>{/if}
              </button>
              <button class="btn icon ghost" aria-label={t("Move up")} disabled={i === 0} onclick={() => moveState(i, -1)}><Icon name="chevron-up" size={18} /></button>
              <button class="btn icon ghost" aria-label={t("Move down")} disabled={i === (key.states?.length ?? 0) - 1} onclick={() => moveState(i, 1)}><Icon name="chevron-down" size={18} /></button>
              <button class="btn icon ghost" aria-label={t("Remove state")} disabled={(key.states?.length ?? 0) <= 2} onclick={() => removeState(i)}><Icon name="x" size={18} /></button>
              {#if openState === i}
                <div class="form">
                  <div class="two">
                    <label class="field"><span>{t("Label")}</span><input value={st.label} maxlength="64" oninput={(e) => editState(i, (q) => (q.label = e.currentTarget.value))} /></label>
                    <div class="field">
                      <span>{t("Icon")}</span>
                      <button class="btn iconbtn" onclick={() => (picking = i)}>
                        {#if st.icon && !st.icon.includes(":")}<Icon name={st.icon} size={18} />{/if}
                        {st.icon?.startsWith("asset:") ? t("Image") : (st.icon ?? t("Same as the key"))}
                      </button>
                    </div>
                  </div>
                  <label class="field"><span>{t("Second line")}</span><input value={st.sub ?? ""} maxlength="64" placeholder={t("Optional")} oninput={(e) => editState(i, (q) => (q.sub = e.currentTarget.value || undefined))} /></label>
                  <div class="field">
                    <span>{t("Colour")}</span>
                    <div class="swatches">
                      {#each colors as [c, name] (c)}
                        <button class="swatch c-{c}" class:on={(st.color ?? "default") === c} aria-label={t(name)} title={t(name)} onclick={() => editState(i, (q) => (q.color = c === "default" ? undefined : c))}></button>
                      {/each}
                    </div>
                  </div>
                  {#if key.follow}
                    <label class="field">
                      <span>{t("Shown while {name} is", { name: key.follow })}</span>
                      <input value={st.value === undefined ? "" : String(st.value)} maxlength="200" placeholder={String(ed.catalog.variables[key.follow] ?? "")} oninput={(e) => editState(i, (q) => (q.value = typed(e.currentTarget.value)))} />
                    </label>
                  {/if}
                  {@render stepList(`s${i}`, t("Steps for this state"))}
                </div>
              {/if}
            </div>
          {/each}
          <button class="add" disabled={(key.states?.length ?? 0) >= 8} onclick={addState}><Icon name="plus" size={18} />{t("Add state")}</button>
        </section>
      {:else}
        {#each mainLists as w (w)}
          {@render stepList(w, w === "off" ? t("When turned off") : key.kind === "toggle" ? t("When turned on") : t("Steps · run in order"))}
        {/each}
      {/if}

      {#if gestures}
        <section class="gestures">
          <h3>{t("Long press and double tap")}</h3>
          {#if blocked}
            <p class="note">{blocked} {t("Long press and double tap need the key to wait before it acts, so they're off for this key.")}</p>
          {/if}
          {#if blocked && hasGestures}
            <p class="warn" role="alert">{t("Remove the long press and double tap steps below, or the layout can't be pushed.")}</p>
          {/if}
          {#if !blocked || hasGestures}
            <div class="gesture">
              <div class="ghead">
                <h4>{t("On long press")}</h4>
                <label class="field inline"><span>{t("after")}</span><input type="number" min="200" max="5000" step="50" value={key.longMs ?? 500} onchange={(e) => edit((k) => { const ms = Math.min(5000, Math.max(200, Number(e.currentTarget.value) || 500)); k.longMs = ms === 500 ? undefined : ms; })} /><span>ms</span></label>
              </div>
              {@render stepList("long", "")}
            </div>
            <div class="gesture">
              <div class="ghead">
                <h4>{t("On double tap")}</h4>
                <label class="field inline"><span>{t("within")}</span><input type="number" min="100" max="1000" step="25" value={key.doubleMs ?? 300} onchange={(e) => edit((k) => { const ms = Math.min(1000, Math.max(100, Number(e.currentTarget.value) || 300)); k.doubleMs = ms === 300 ? undefined : ms; })} /><span>ms</span></label>
              </div>
              {@render stepList("double", "")}
            </div>
            {#if key.longActions?.length}<p class="note">{t("A short tap runs the normal steps when the key is let go.")}</p>{/if}
            {#if key.doubleActions?.length}<p class="note">{t("A single tap waits {ms} ms to make sure no second tap follows.", { ms: key.doubleMs ?? 300 })}</p>{/if}
          {/if}
        </section>
      {/if}
    {/if}

    <div class="foot">
      <span class="size">
        {t("Size")}
        <button class="btn icon" aria-label={t("Narrower")} onclick={() => resize(-1, 0)}>−</button>
        <span class="mono">{key.w ?? 1}×{key.h ?? 1}</span>
        <button class="btn icon" aria-label={t("Wider")} onclick={() => resize(1, 0)}>+</button>
        <button class="btn icon" aria-label={t("Shorter")} onclick={() => resize(0, -1)}>↑</button>
        <button class="btn icon" aria-label={t("Taller")} onclick={() => resize(0, 1)}>↓</button>
      </span>
      <button class="btn" onclick={() => (ed.moving = key.id)}><Icon name="move" size={18} />{t("Move")}</button>
      <button class="btn danger" onclick={() => deleteKey(key.id)}><Icon name="trash-2" size={18} />{t("Delete")}</button>
    </div>
  {:else if pg}
    <header><h2>{t("Page settings")}</h2></header>
    <label class="field"><span>{t("Page name")}</span><input value={pg.name} maxlength="40" oninput={(e) => change(() => (pg.name = e.currentTarget.value))} /></label>
    <label class="field"><span>{t("Subtitle")}</span><input value={pg.subtitle ?? ""} maxlength="60" oninput={(e) => change(() => (pg.subtitle = e.currentTarget.value || undefined))} /></label>
    {#if !pg.folder}
      <label class="check">
        <input type="checkbox" checked={profile()?.homePage === pg.id} onchange={() => change(() => (profile()!.homePage = pg.id))} />
        {t("Open this page first")}
      </label>
    {/if}
    <p class="hint">{t("Select a key to edit it, or drop an action on an empty cell.")}</p>
    {#if onpages}<button class="btn" onclick={onpages}><Icon name="layers" size={18} />{t("All pages")}</button>{/if}
    <button class="btn danger" onclick={() => deletePage(pg.id)} disabled={!pg.folder && (profile()?.pages.filter((p) => !p.folder).length ?? 0) <= 1}>
      <Icon name="trash-2" size={18} />{t("Delete page")}
    </button>
  {/if}
</aside>

<Sheet open={picking !== null} onclose={() => (picking = null)} title={t("Choose an icon")} wide>
  {@const target = picking}
  <IconPicker
    value={typeof target === "number" ? key?.states?.[target]?.icon : key?.icon}
    onpick={(i) => {
      if (typeof target === "number") editState(target, (q) => (q.icon = i));
      else edit((k) => (k.icon = i));
      picking = null;
    }}
  />
</Sheet>

{#snippet stepList(w: Which, title: string)}
  {@const steps = list(w)}
  {@const depth = depths(steps)}
  <section class="steps">
    {#if title}<h3>{title}</h3>{/if}
    {#each steps as a, i (i)}
      {@const def = actionDef(a.type)}
      {@const id = `${w}:${i}`}
      <div class="step" class:open={open === id} class:review={a.review} class:flow={a.type === "core.if" || a.type === "core.else" || a.type === "core.end"} style:--depth={depth[i]}>
        <button class="summary" onclick={() => (open = open === id ? null : id)}>
          <span class="n mono">{i + 1}</span>
          <span class="kind">{badge(a)}</span>
          <span class="what">{summary(a)}</span>
        </button>
        <button class="btn icon ghost" aria-label={t("Move up")} disabled={i === 0} onclick={() => moveStep(w, i, -1)}><Icon name="chevron-up" size={18} /></button>
        <button class="btn icon ghost" aria-label={t("Remove step")} onclick={() => removeStep(w, i)}><Icon name="x" size={18} /></button>
        {#if open === id}
          <div class="form">
            {#if a.review}<p class="warn">{t("Imported step. It won't run until you approve it on the PC.")}</p>{/if}
            {#if !def}<p class="warn">{t("This action needs a plugin that is not installed.")}</p>{/if}
            {#if a.type.startsWith("sd.")}
              {#if admin && w === "on" && def && key}<SdPanel instance="{ed.profile}/{ed.page}/{key.id}/{i}" dirty={ed.dirty} rev={ed.baseRev} />{:else if def}<p class="muted">{t("Stream Deck plugin settings open in the editor on the PC.")}</p>{/if}
            {:else}
              <SettingsForm schema={def?.settings} values={a.settings} type={a.type} onchange={(v) => editStep(w, i, (q) => (q.settings = v))} />
              {#if admin && a.type === "core.timer"}<TimerOverlay name={a.settings.name} />{/if}
            {/if}
          </div>
        {/if}
      </div>
    {/each}
    <button class="add" onclick={() => (adding = w)}><Icon name="plus" size={18} />{t("Add step")}</button>
    {#if w === "on" && sameBatch}<p class="note">{t("Back-to-back steps for the same app go out together.")}</p>{/if}
  </section>
{/snippet}

<Sheet open={adding !== null} onclose={() => (adding = null)} title={t("Add step")} wide>
  <input class="input" bind:value={addQuery} placeholder={t("Search plugins and actions")} aria-label={t("Search plugins and actions")} />
  <div class="choices">
    {#each choices as a (a.id)}
      <button class="choice" onclick={() => addStep(a.id)}>
        <Icon name={a.icon ?? "bolt"} size={20} />
        <span><b>{t(a.name)}</b><small>{t(a.group ?? "")}</small></span>
      </button>
    {/each}
  </div>
  <button class="btn" onclick={() => (adding = null)}>{t("Cancel")}</button>
</Sheet>

<style>
  .inspector {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 20px;
    overflow: auto;
    min-height: 0;
  }
  header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
  }
  header h2 {
    display: inline;
    margin: 0 10px 0 0;
    font-size: 1.25rem;
  }
  .tag {
    font-size: 0.8rem;
    padding: 3px 8px;
    border-radius: 6px;
    background: var(--raised);
    color: var(--muted);
    white-space: nowrap;
  }
  .two {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
    align-items: end;
  }
  .iconbtn {
    justify-content: flex-start;
    min-height: 42px;
  }
  .swatches {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }
  .swatch {
    width: 32px;
    height: 32px;
    border-radius: 8px;
    border: 2px solid transparent;
    background: var(--raised);
  }
  .swatch.on {
    border-color: var(--accent);
  }
  .c-teal {
    background: var(--key-teal);
  }
  .c-violet {
    background: var(--key-violet);
  }
  .c-green {
    background: var(--key-green);
  }
  .c-amber {
    background: var(--key-amber);
  }
  .c-red {
    background: var(--key-red);
  }
  .c-blue {
    background: var(--key-blue);
  }
  .inline {
    flex-direction: row;
    align-items: center;
  }
  .cond {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .cond > * {
    flex: 1 1 140px;
    min-height: 40px;
  }
  .cond select {
    border-radius: var(--radius-sm);
    border: 1px solid var(--line);
    background: var(--bg);
    padding: 0 8px;
  }
  .steps {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  h3 {
    margin: 0;
    font-size: 0.9rem;
    color: var(--muted);
    font-weight: 500;
  }
  .step {
    margin-left: calc(var(--depth, 0) * 20px);
    display: grid;
    grid-template-columns: 1fr auto auto;
    align-items: center;
    border-radius: var(--radius-sm);
    border: 1px solid var(--line-soft);
    background: var(--surface);
  }
  .step.flow:not(.open) {
    background: var(--raised);
  }
  .step.open {
    border-color: var(--accent);
    background: var(--accent-bg);
  }
  .step.review {
    border-style: dashed;
    border-color: var(--warning);
  }
  .summary {
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 52px;
    padding: 0 12px;
    border: none;
    background: none;
    text-align: left;
    min-width: 0;
  }
  .n {
    color: var(--muted);
  }
  .kind {
    font-size: 0.75rem;
    font-weight: 700;
    padding: 4px 8px;
    border-radius: 6px;
    background: var(--bg);
    letter-spacing: 0.04em;
  }
  .what {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .form {
    grid-column: 1 / -1;
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 4px 12px 16px;
  }
  .warn {
    margin: 0;
    color: var(--warning);
  }
  .setup {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 12px;
    border-radius: var(--radius-sm);
    background: var(--accent-bg);
    color: var(--accent-text);
  }
  .setup.bad {
    background: var(--danger-bg);
    color: var(--danger-text);
  }
  .setup span {
    flex: 1;
    min-width: 0;
  }
  .ref {
    display: block;
    margin-top: 2px;
    font-size: 0.8rem;
    color: var(--dim);
    user-select: all;
  }
  .form .muted {
    margin: 0;
    color: var(--muted);
  }
  .add {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    min-height: 48px;
    border-radius: var(--radius-sm);
    border: 1px dashed var(--line);
    background: none;
  }
  .note,
  .hint {
    margin: 0;
    color: var(--muted);
    font-size: 0.9rem;
  }
  .foot {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
    padding-top: 8px;
    border-top: 1px solid var(--line-soft);
  }
  .size {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--muted);
    margin-right: auto;
  }
  .size .btn.icon {
    width: 36px;
    min-height: 36px;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    align-items: center;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    min-height: 34px;
    padding: 0 12px;
    border-radius: 999px;
    border: 1px solid var(--line);
    background: var(--bg);
  }
  .chip.on {
    border-color: var(--accent);
    background: var(--accent-bg);
    color: var(--accent-text);
  }
  .chip:disabled {
    opacity: 0.5;
  }
  .check.off {
    opacity: 0.55;
  }
  .states,
  .gestures {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .state {
    display: grid;
    grid-template-columns: 1fr auto auto auto;
    align-items: center;
    border-radius: var(--radius-sm);
    border: 1px solid var(--line-soft);
    background: var(--surface);
  }
  .state.open {
    border-color: var(--accent);
  }
  .state .summary {
    gap: 10px;
  }
  .dot {
    flex: none;
    width: 14px;
    height: 14px;
    border-radius: 4px;
    border: 1px solid var(--line);
  }
  .dot.c-default {
    background: var(--raised);
  }
  .tag.now {
    background: var(--accent-bg);
    color: var(--accent-text);
  }
  .gesture {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--line-soft);
  }
  .ghead {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }
  .ghead input {
    width: 90px;
  }
  h4 {
    margin: 0;
    font-size: 0.95rem;
    font-weight: 600;
  }
  .add:disabled {
    opacity: 0.5;
  }
  .choices {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: 8px;
  }
  .choice {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--line-soft);
    background: var(--bg);
    text-align: left;
  }
  .choice:hover {
    border-color: var(--accent);
  }
  .choice span {
    display: flex;
    flex-direction: column;
  }
  .choice small {
    color: var(--muted);
  }
</style>

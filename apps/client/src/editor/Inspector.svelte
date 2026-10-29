<script lang="ts">
  import type { Action, Key, KeyColor, KeyKind, Tone } from "@keyside/protocol";
  import Icon from "../components/Icon.svelte";
  import Sheet from "../components/Sheet.svelte";
  import IconPicker from "./IconPicker.svelte";
  import SettingsForm from "./SettingsForm.svelte";
  import SdPanel from "./SdPanel.svelte";
  import { ed, page, profile, selectedKey, change, actionDef, allActions, newAction, deleteKey, deletePage, fits } from "./state.svelte.ts";
  import { req } from "../lib/deck.svelte.ts";
  import { t, mark } from "../lib/i18n.svelte.ts";

  let { admin = false }: { admin?: boolean } = $props();

  let picking = $state(false);
  let adding = $state<"on" | "off" | null>(null);
  let open = $state<string | null>(null);
  let testing = $state(false);
  let addQuery = $state("");

  const key = $derived(selectedKey());
  const pg = $derived(page());

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

  function list(off: boolean): Action[] {
    return (off ? key?.offActions : key?.actions) ?? [];
  }

  function editStep(off: boolean, i: number, fn: (a: Action) => void) {
    edit((k) => {
      const l = off ? (k.offActions ??= []) : k.actions;
      if (l[i]) fn(l[i]);
    });
  }

  function moveStep(off: boolean, i: number, d: number) {
    edit((k) => {
      const l = off ? (k.offActions ??= []) : k.actions;
      const j = i + d;
      if (j < 0 || j >= l.length) return;
      [l[i], l[j]] = [l[j], l[i]];
    });
  }

  function removeStep(off: boolean, i: number) {
    edit((k) => {
      const l = off ? (k.offActions ??= []) : k.actions;
      l.splice(i, 1);
    });
  }

  function addStep(type: string) {
    const off = adding === "off";
    edit((k) => (off ? (k.offActions ??= []) : k.actions).push(newAction(type)));
    adding = null;
    addQuery = "";
  }

  function badge(a: Action): string {
    if (a.type === "core.wait") return "WAIT";
    const plugin = ed.catalog.plugins.find((p) => a.type.startsWith(`${p.id}/`));
    if (plugin) return plugin.prefix.toUpperCase().slice(0, 6);
    return "CORE";
  }

  function summary(a: Action): string {
    if (a.type === "core.wait") return t("Delay {ms} ms", { ms: String(a.settings.ms ?? 0) });
    const def = actionDef(a.type);
    const name = t(def?.name ?? a.type);
    const detail = Object.entries(a.settings)
      .filter(([, v]) => typeof v === "string" || typeof v === "number")
      .slice(0, 2)
      .map(([k, v]) => (k === "args" || k === "url" ? `${v}` : `${v}`))
      .join(" ");
    return detail ? `${name} ${detail}` : name;
  }

  async function test() {
    const k = selectedKey();
    if (!k) return;
    testing = true;
    try {
      for (const a of k.actions) await req("action.test", { action: $state.snapshot(a) });
    } catch (e: any) {
      ed.error = e.message;
    } finally {
      setTimeout(() => (testing = false), 600);
    }
  }

  const sameBatch = $derived(list(false).some((a, i, l) => i > 0 && a.type.includes("/") && l[i - 1].type.split("/")[0] === a.type.split("/")[0]));
  const varNames = $derived(Object.keys(ed.catalog.variables).sort());
  const fb = $derived(key?.feedback?.[0]);
  const feeds = $derived(ed.catalog.plugins.flatMap((p: any) => (p.feeds ?? []).map((f: any) => ({ value: `${p.prefix}.${f.name}`, label: `${p.name}: ${f.title}` }))));
  const choices = $derived(
    allActions()
      .filter((a) => admin || !a.adminOnly)
      .filter((a) => !addQuery || `${a.name} ${a.group}`.toLowerCase().includes(addQuery.toLowerCase())),
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
  {#if key}
    <header>
      <div>
        <h2>{key.label || t("Untitled key")}</h2>
        {#if key.actions.length > 1}<span class="tag">{t("Multi-action")}</span>{/if}
        <span class="ref mono" title={t("Page and key id, for the HTTP API")}>{ed.page}/{key.id}</span>
      </div>
      <button class="btn" onclick={test} disabled={!key.actions.length || testing}><Icon name="play" size={16} />{testing ? t("Running…") : t("Test run")}</button>
    </header>

    <div class="two">
      <label class="field"><span>{t("Label")}</span><input value={key.label} maxlength="64" oninput={(e) => edit((k) => (k.label = e.currentTarget.value))} /></label>
      <div class="field">
        <span>{t("Icon")}</span>
        <button class="btn iconbtn" onclick={() => (picking = true)}>
          {#if key.icon && !key.icon.includes(":")}<Icon name={key.icon} size={18} />{/if}
          {key.icon?.startsWith("asset:") ? t("Image") : (key.icon ?? t("None"))}
        </button>
      </div>
    </div>
    <label class="field"><span>{t("Second line")}</span><input value={key.sub ?? ""} maxlength="64" placeholder={t("Optional")} oninput={(e) => edit((k) => (k.sub = e.currentTarget.value || undefined))} /></label>

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
        <select value={key.kind} onchange={(e) => edit((k) => {
          k.kind = e.currentTarget.value as KeyKind;
          if (k.kind === "slider") k.slider ??= { min: 0, max: 1 };
          if (k.kind === "widget") k.widget ??= { text: "" };
          if (k.kind === "folder") k.target ??= profile()?.pages.find((p) => p.folder)?.id;
        })}>
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
    {/if}

    {#if key.kind === "slider"}
      <div class="two">
        <label class="field"><span>{t("Lowest")}</span><input type="number" value={key.slider?.min ?? 0} oninput={(e) => edit((k) => (k.slider = { ...k.slider!, min: Number(e.currentTarget.value) }))} /></label>
        <label class="field"><span>{t("Highest")}</span><input type="number" value={key.slider?.max ?? 1} oninput={(e) => edit((k) => (k.slider = { ...k.slider!, max: Number(e.currentTarget.value) }))} /></label>
      </div>
    {/if}

    {#if key.kind !== "folder" && key.kind !== "back"}
      <div class="two">
        <label class="check">
          <input type="checkbox" checked={!!key.hold} onchange={(e) => edit((k) => (k.hold = e.currentTarget.checked ? 1000 : undefined))} />
          {t("Hold to confirm")}
        </label>
        {#if key.hold}
          <label class="field inline"><input type="number" min="200" max="10000" step="100" value={key.hold} oninput={(e) => edit((k) => (k.hold = Math.min(10000, Math.max(200, Number(e.currentTarget.value)))))} /><span>ms</span></label>
        {/if}
      </div>

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

      {#each key.kind === "toggle" ? [false, true] : [false] as off (off)}
        <section class="steps">
          <h3>{off ? t("When turned off") : key.kind === "toggle" ? t("When turned on") : t("Steps · run in order")}</h3>
          {#each list(off) as a, i (i)}
            {@const def = actionDef(a.type)}
            <div class="step" class:open={open === `${off}${i}`} class:review={a.review}>
              <button class="summary" onclick={() => (open = open === `${off}${i}` ? null : `${off}${i}`)}>
                <span class="n mono">{i + 1}</span>
                <span class="kind">{badge(a)}</span>
                <span class="what">{summary(a)}</span>
              </button>
              <button class="btn icon ghost" aria-label={t("Move up")} disabled={i === 0} onclick={() => moveStep(off, i, -1)}><Icon name="chevron-up" size={18} /></button>
              <button class="btn icon ghost" aria-label={t("Remove step")} onclick={() => removeStep(off, i)}><Icon name="x" size={18} /></button>
              {#if open === `${off}${i}`}
                <div class="form">
                  {#if a.review}<p class="warn">{t("Imported step. It won't run until you approve it on the PC.")}</p>{/if}
                  {#if !def}<p class="warn">{t("This action needs a plugin that is not installed.")}</p>{/if}
                  {#if a.type.startsWith("sd.")}
                    {#if admin && !off && def}<SdPanel instance="{ed.profile}/{ed.page}/{key.id}/{i}" dirty={ed.dirty} />{:else if def}<p class="muted">{t("Stream Deck plugin settings open in the editor on the PC.")}</p>{/if}
                  {:else}
                    <SettingsForm schema={def?.settings} values={a.settings} type={a.type} onchange={(v) => editStep(off, i, (s) => (s.settings = v))} />
                  {/if}
                </div>
              {/if}
            </div>
          {/each}
          <button class="add" onclick={() => (adding = off ? "off" : "on")}><Icon name="plus" size={18} />{t("Add step")}</button>
          {#if !off && sameBatch}<p class="note">{t("Back-to-back steps for the same app go out together.")}</p>{/if}
        </section>
      {/each}
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
    <button class="btn danger" onclick={() => deletePage(pg.id)} disabled={!pg.folder && (profile()?.pages.filter((p) => !p.folder).length ?? 0) <= 1}>
      <Icon name="trash-2" size={18} />{t("Delete page")}
    </button>
  {/if}
</aside>

<Sheet bind:open={picking} title={t("Choose an icon")} wide>
  <IconPicker value={key?.icon} onpick={(i) => (edit((k) => (k.icon = i)), (picking = false))} />
</Sheet>

<Sheet open={adding !== null} onclose={() => (adding = null)} title={t("Add step")} wide>
  <input class="input" bind:value={addQuery} placeholder={t("Search actions")} aria-label={t("Search actions")} />
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
    display: grid;
    grid-template-columns: 1fr auto auto;
    align-items: center;
    border-radius: var(--radius-sm);
    border: 1px solid var(--line-soft);
    background: var(--surface);
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

<script lang="ts">
  import type { CompareOp, KeyColor, Style, Tone } from "@keyside/protocol";
  import Icon from "../components/Icon.svelte";
  import Sheet from "../components/Sheet.svelte";
  import IconPicker from "./IconPicker.svelte";
  import { ed, change, newId, deleteStyle, matches } from "./state.svelte.ts";
  import { t, mark } from "../lib/i18n.svelte.ts";

  let picking = $state<string | null>(null);
  let confirming = $state<string | null>(null);

  const styles = $derived(ed.layout?.styles ?? []);
  const varNames = $derived(Object.keys(ed.catalog.variables).sort());
  const OPS: Array<[CompareOp, string]> = [
    ["eq", mark("is")],
    ["ne", mark("is not")],
    ["contains", mark("contains")],
    ["gt", mark("is above")],
    ["lt", mark("is below")],
    ["truthy", mark("is on")],
    ["falsy", mark("is off")],
  ];
  const colors: Array<[KeyColor, string]> = [
    ["default", mark("Default")],
    ["teal", mark("Teal")],
    ["violet", mark("Violet")],
    ["green", mark("Green")],
    ["amber", mark("Amber")],
    ["red", mark("Red")],
    ["blue", mark("Blue")],
  ];
  const tones: Array<[Tone | "", string]> = [
    ["", mark("No highlight")],
    ["accent", mark("Highlight")],
    ["danger", mark("Red")],
    ["success", mark("Green text")],
    ["warning", mark("Amber text")],
  ];

  /** Keys that use each style, across all layouts. */
  const uses = $derived.by(() => {
    const n = new Map<string, number>();
    for (const p of ed.layout?.profiles ?? []) for (const pg of p.pages) for (const k of pg.keys) for (const s of k.styles ?? []) n.set(s, (n.get(s) ?? 0) + 1);
    return n;
  });

  function add() {
    const id = newId("st", styles.map((s) => s.id));
    change((l) => (l.styles ??= []).push({ id, name: t("New style"), when: { var: "", op: "truthy" }, style: { tone: "accent" } }));
  }

  function edit(id: string, fn: (s: Style) => void) {
    change((l) => {
      const s = l.styles?.find((q) => q.id === id);
      if (s) fn(s);
    });
  }

  /** Text, or a number or true/false when it looks like one, as the companion compares them. */
  function typed(v: string): string | number | boolean | undefined {
    if (v === "") return undefined;
    if (v === "true" || v === "false") return v === "true";
    return v.trim() !== "" && !Number.isNaN(Number(v)) ? Number(v) : v;
  }

  function look(id: string, field: "label" | "badge", value: string) {
    edit(id, (s) => (s.style[field] = value || undefined));
  }
</script>

<p class="muted">{t("A style changes how keys look while a variable matches, like red while live. Pick styles for a key in its settings.")}</p>

{#each styles as st (st.id)}
  {@const now = st.when.var ? matches(st.when, ed.vars) : false}
  <div class="style">
    <div class="row">
      <label class="field grow"><span>{t("Name")}</span><input value={st.name} maxlength="40" oninput={(e) => edit(st.id, (s) => (s.name = e.currentTarget.value))} /></label>
      <span class="state" class:on={now}>{now ? t("Matches now") : t("Doesn't match now")}</span>
    </div>
    <div class="field">
      <span>{t("When")}</span>
      <div class="cond">
        <select aria-label={t("Variable")} value={st.when.var} onchange={(e) => edit(st.id, (s) => (s.when.var = e.currentTarget.value))}>
          <option value="">{t("Choose a variable")}</option>
          {#each varNames as v (v)}<option value={v}>{v}</option>{/each}
          {#if st.when.var && !varNames.includes(st.when.var)}<option value={st.when.var}>{st.when.var}</option>{/if}
        </select>
        <select aria-label={t("Check")} value={st.when.op} onchange={(e) => edit(st.id, (s) => (s.when.op = e.currentTarget.value as CompareOp))}>
          {#each OPS as [op, name] (op)}<option value={op}>{t(name)}</option>{/each}
        </select>
        {#if st.when.op !== "truthy" && st.when.op !== "falsy"}
          <input class="input" aria-label={t("Value")} value={st.when.value === undefined ? "" : String(st.when.value)} maxlength="200" placeholder={String(ed.catalog.variables[st.when.var] ?? "")} oninput={(e) => edit(st.id, (s) => (s.when.value = typed(e.currentTarget.value)))} />
        {/if}
      </div>
    </div>
    <div class="field">
      <span>{t("Colour")}</span>
      <div class="swatches">
        {#each colors as [c, name] (c)}
          <button class="swatch c-{c}" class:on={(st.style.color ?? "default") === c} aria-label={t(name)} title={t(name)} onclick={() => edit(st.id, (s) => (s.style.color = c === "default" ? undefined : c))}></button>
        {/each}
      </div>
    </div>
    <div class="two">
      <label class="field">
        <span>{t("Light up")}</span>
        <select value={st.style.tone ?? ""} onchange={(e) => edit(st.id, (s) => (s.style.tone = (e.currentTarget.value || undefined) as Tone | undefined))}>
          {#each tones as [tn, name] (tn)}<option value={tn}>{t(name)}</option>{/each}
        </select>
      </label>
      <div class="field">
        <span>{t("Icon")}</span>
        <div class="iconrow">
          <button class="btn iconbtn" onclick={() => (picking = st.id)}>
            {#if st.style.icon && !st.style.icon.includes(":")}<Icon name={st.style.icon} size={18} />{/if}
            {st.style.icon?.startsWith("asset:") ? t("Image") : (st.style.icon ?? t("Keep the key icon"))}
          </button>
          {#if st.style.icon}<button class="btn icon ghost" aria-label={t("Keep the key icon")} onclick={() => edit(st.id, (s) => (s.style.icon = undefined))}><Icon name="x" size={16} /></button>{/if}
        </div>
      </div>
    </div>
    <div class="two">
      <label class="field"><span>{t("Badge")}</span><input value={st.style.badge ?? ""} maxlength="24" placeholder={t("Badge, e.g. ON AIR")} oninput={(e) => look(st.id, "badge", e.currentTarget.value)} /></label>
      <label class="field"><span>{t("Label")}</span><input value={st.style.label ?? ""} maxlength="64" placeholder={t("Keep the key label")} oninput={(e) => look(st.id, "label", e.currentTarget.value)} /></label>
    </div>
    <div class="row foot">
      <span class="muted small">{t("Keys using it: {n}", { n: uses.get(st.id) ?? 0 })}</span>
      {#if confirming === st.id}
        <span class="muted small">{t("Also takes it off those keys.")}</span>
        <button class="btn" onclick={() => (confirming = null)}>{t("Cancel")}</button>
        <button class="btn danger" onclick={() => ((confirming = null), deleteStyle(st.id))}><Icon name="trash-2" size={18} />{t("Delete style")}</button>
      {:else}
        <button class="btn danger" onclick={() => (uses.get(st.id) ? (confirming = st.id) : deleteStyle(st.id))}><Icon name="trash-2" size={18} />{t("Delete")}</button>
      {/if}
    </div>
  </div>
{/each}

<button class="add" disabled={styles.length >= 64} onclick={add}><Icon name="plus" size={18} />{t("New style")}</button>

<Sheet open={picking !== null} onclose={() => (picking = null)} title={t("Choose an icon")} wide>
  {@const id = picking}
  <IconPicker value={styles.find((s) => s.id === id)?.style.icon} onpick={(i) => (id && edit(id, (s) => (s.style.icon = i)), (picking = null))} />
</Sheet>

<style>
  .muted {
    margin: 0 0 12px;
    color: var(--muted);
  }
  .small {
    margin: 0;
    font-size: 0.85rem;
  }
  .style {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 16px;
    margin-bottom: 12px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--line-soft);
    background: var(--bg);
  }
  .row {
    display: flex;
    align-items: flex-end;
    gap: 12px;
    flex-wrap: wrap;
  }
  .grow {
    flex: 1;
  }
  .foot {
    align-items: center;
    justify-content: flex-end;
  }
  .foot > :first-child {
    margin-right: auto;
  }
  .state {
    padding: 6px 10px;
    border-radius: 999px;
    font-size: 0.8rem;
    background: var(--raised);
    color: var(--muted);
  }
  .state.on {
    background: var(--accent-bg);
    color: var(--accent-text);
  }
  .two {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
    align-items: end;
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
  select {
    min-height: 40px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--line);
    background: var(--bg);
    padding: 0 8px;
  }
  .iconrow {
    display: flex;
    gap: 6px;
  }
  .iconbtn {
    flex: 1;
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
  .add {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    width: 100%;
    min-height: 48px;
    border-radius: var(--radius-sm);
    border: 1px dashed var(--line);
    background: none;
  }
  @media (max-width: 560px) {
    .two {
      grid-template-columns: 1fr;
    }
  }
</style>

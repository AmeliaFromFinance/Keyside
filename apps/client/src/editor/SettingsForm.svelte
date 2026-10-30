<script lang="ts">
  import type { Schema, FieldSchema } from "./state.svelte.ts";
  import { ed, profile } from "./state.svelte.ts";
  import { req } from "../lib/deck.svelte.ts";
  import { t } from "../lib/i18n.svelte.ts";

  let {
    schema,
    values,
    onchange,
    type = "",
  }: {
    schema: Schema | undefined;
    values: Record<string, unknown>;
    onchange: (next: Record<string, unknown>) => void;
    /** Action type, used to ask the plugin for dynamic options. */
    type?: string;
  } = $props();

  let dynamic = $state<Record<string, Array<{ value: string; label: string }>>>({});
  let loading = $state<Record<string, boolean>>({});
  let failed = $state<Record<string, string>>({});

  const fields = $derived(
    Object.entries(schema?.properties ?? {}).filter(([, f]) => {
      if ((f as { "x-hidden"?: boolean })["x-hidden"]) return false;
      return Object.entries(f["x-showWhen"] ?? {}).every(([k, ok]) => ok.includes(String(values[k] ?? schema?.properties[k]?.default ?? "")));
    }),
  );

  function set(name: string, v: unknown) {
    const next = { ...values };
    if (v === "" || v === undefined || (typeof v === "number" && Number.isNaN(v))) delete next[name];
    else next[name] = v;
    onchange(next);
  }

  async function loadOptions(name: string) {
    loading[name] = true;
    failed[name] = "";
    try {
      dynamic[name] = await req("options.get", { type, field: name, settings: $state.snapshot(values) });
    } catch (e: any) {
      failed[name] = e.message;
    } finally {
      loading[name] = false;
    }
  }

  function builtinOptions(kind: string): Array<{ value: string; label: string }> {
    switch (kind) {
      case "pages":
        return (profile()?.pages ?? []).map((p) => ({ value: p.id, label: p.folder ? `${p.name} (folder)` : p.name }));
      case "profiles":
        return (ed.layout?.profiles ?? []).map((p) => ({ value: p.id, label: p.name }));
      case "sounds":
        return ed.catalog.assets.filter((a) => a.kind === "sound").map((a) => ({ value: a.id, label: a.name }));
      case "midi":
        return ed.catalog.midi.map((m) => ({ value: m, label: m }));
      case "variables":
        return Object.keys(ed.catalog.variables)
          .sort()
          .map((v) => ({ value: v, label: v }));
      default:
        return [];
    }
  }

  function options(name: string, f: FieldSchema): Array<{ value: string; label: string }> | null {
    if (f.enum) return f.enum.map((v, i) => ({ value: v, label: f.enumNames?.[i] ?? v }));
    const kind = f["x-options"];
    if (!kind) return null;
    if (kind === "dynamic") return dynamic[name] ?? null;
    return builtinOptions(kind);
  }

  $effect(() => {
    for (const [name, f] of fields) if (f["x-options"] === "dynamic" && !dynamic[name] && !loading[name] && type) void loadOptions(name);
  });

  function entries(v: unknown): Array<[string, string]> {
    return v && typeof v === "object" ? Object.entries(v as Record<string, string>) : [];
  }
  function setEntry(name: string, i: number, k: string, val: string) {
    const list = entries(values[name]);
    list[i] = [k, val];
    set(name, Object.fromEntries(list.filter(([kk]) => kk !== "")));
  }
</script>

{#each fields as [name, f] (name)}
  {@const opts = options(name, f)}
  {@const v = values[name]}
  {@const label = t(f.title ?? name)}
  {#if f.type === "boolean"}
    <label class="check"><input type="checkbox" checked={!!(v ?? f.default)} onchange={(e) => set(name, e.currentTarget.checked)} /> {label}</label>
  {:else if f.type === "object"}
    <div class="field">
      <span>{label}</span>
      {#each [...entries(v), ["", ""]] as [k, val], i (i)}
        <div class="pair">
          <input class="input" placeholder={t("Name")} value={k} oninput={(e) => setEntry(name, i, e.currentTarget.value, val)} />
          <input class="input" placeholder={t("Value")} value={val} oninput={(e) => setEntry(name, i, k, e.currentTarget.value)} />
        </div>
      {/each}
      {#if f.description}<small>{t(f.description)}</small>{/if}
    </div>
  {:else}
    <label class="field">
      <span>{label}</span>
      {#if opts}
        <div class="row">
          <select value={(v as string) ?? ""} onchange={(e) => set(name, e.currentTarget.value)}>
            <option value="">{loading[name] ? t("Loading…") : t("Choose…")}</option>
            {#each opts as o (o.value)}<option value={o.value}>{o.label}</option>{/each}
            {#if v && !opts.some((o) => o.value === v)}<option value={v as string}>{v as string}</option>{/if}
          </select>
          {#if f["x-options"] === "dynamic"}
            <button class="btn icon" title={t("Refresh")} aria-label={t("Refresh")} onclick={() => loadOptions(name)}>↻</button>
          {/if}
        </div>
        {#if failed[name]}<small class="bad">{failed[name]}</small>{/if}
        {#if f["x-options"] === "dynamic" && !opts.length && !loading[name]}
          <input value={(v as string) ?? ""} placeholder={t("Or type it")} oninput={(e) => set(name, e.currentTarget.value)} />
        {/if}
      {:else if f.type === "number" || f.type === "integer"}
        <input
          type="number"
          value={(v as number) ?? f.default ?? ""}
          min={f.minimum}
          max={f.maximum}
          step={f.type === "integer" ? 1 : "any"}
          oninput={(e) => set(name, e.currentTarget.value === "" ? undefined : Number(e.currentTarget.value))}
        />
      {:else if f.format === "textarea"}
        <textarea maxlength={f.maxLength} value={(v as string) ?? ""} oninput={(e) => set(name, e.currentTarget.value)}></textarea>
      {:else if f.format === "secret"}
        <input type="password" autocomplete="off" placeholder={v && typeof v === "object" && (v as any).set ? t("Saved, type to replace") : ""} oninput={(e) => set(name, e.currentTarget.value)} />
      {:else}
        <input
          type={f.format === "url" ? "url" : "text"}
          value={(v as string) ?? ""}
          maxlength={f.maxLength}
          placeholder={typeof f.default === "string" ? f.default : ""}
          oninput={(e) => set(name, e.currentTarget.value)}
        />
      {/if}
      {#if f.description}<small>{t(f.description)}</small>{/if}
    </label>
  {/if}
{/each}

<style>
  .row,
  .pair {
    display: flex;
    gap: 8px;
  }
  .row select {
    flex: 1;
    min-width: 0;
  }
  .pair .input {
    flex: 1;
    min-width: 0;
    min-height: 38px;
  }
  .bad {
    color: var(--danger-text) !important;
  }
</style>

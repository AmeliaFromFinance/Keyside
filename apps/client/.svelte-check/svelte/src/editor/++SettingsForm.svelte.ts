///<reference types="svelte" />
;
import type { Schema, FieldSchema } from "./state.svelte.ts";
import { ed, profile } from "./state.svelte.ts";
import { req } from "../lib/deck.svelte.ts";
import { t } from "../lib/i18n.svelte.ts";

;type $$ComponentProps =  {
    schema: Schema | undefined;
    values: Record<string, unknown>;
    onchange: (next: Record<string, unknown>) => void;
    /** Action type, used to ask the plugin for dynamic options. */
    type?: string;
  };function $$render() {

  
  
  
  

  let {
    schema,
    values,
    onchange,
    type = "",
  }:/*Ωignore_startΩ*/$$ComponentProps/*Ωignore_endΩ*/ = $props();

  let dynamic = $state<Record<string, Array<{ value: string; label: string }>>>({});
  let loading = $state<Record<string, boolean>>({});
  let failed = $state<Record<string, string>>({});

  const fields = $derived(Object.entries(schema?.properties ?? {}).filter(([, f]) => !(f as { "x-hidden"?: boolean })["x-hidden"]));

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
;
async () => {

   for(let [name, f] of __sveltets_2_ensureArray(fields)){name;
  const opts = options(name, f);
  const v = values[name];
  const label = t(f.title ?? name);
  if(f.type === "boolean"){
     { svelteHTML.createElement("label", { "class":`check`,}); { svelteHTML.createElement("input", {      "type":`checkbox`,"checked":!!(v ?? f.default),"onchange":(e) => set(name, e.currentTarget.checked),});} label; }
  } else if (f.type === "object"){
     { svelteHTML.createElement("div", { "class":`field`,});
       { svelteHTML.createElement("span", {});label; }
          for(let [k, val] of __sveltets_2_ensureArray(([...entries(v), ["", ""]]))){let i = 1;i;
         { svelteHTML.createElement("div", { "class":`pair`,});
           { svelteHTML.createElement("input", {        "class":`input`,"placeholder":t("Name"),"value":k,"oninput":(e) => setEntry(name, i, e.currentTarget.value, val),});}
           { svelteHTML.createElement("input", {        "class":`input`,"placeholder":t("Value"),"value":val,"oninput":(e) => setEntry(name, i, k, e.currentTarget.value),});}
         }
      }
     }
  }else{
     { svelteHTML.createElement("label", { "class":`field`,});
       { svelteHTML.createElement("span", {});label; }
      if(opts){
         { svelteHTML.createElement("div", { "class":`row`,});
           { svelteHTML.createElement("select", {   "value":(v as string) ?? "","onchange":(e) => set(name, e.currentTarget.value),});
             { svelteHTML.createElement("option", {"value":"",});loading[name] ? t("Loading…") : t("Choose…"); }
               for(let o of __sveltets_2_ensureArray(opts)){o.value; { svelteHTML.createElement("option", { "value":o.value,});o.label; }}
            if(v && !opts.some((o) => o.value === v)){ { svelteHTML.createElement("option", { "value":v as string,});v as string; }}
           }
          if(f["x-options"] === "dynamic"){
             { svelteHTML.createElement("button", {       "class":`btn icon`,"title":t("Refresh"),"aria-label":t("Refresh"),"onclick":() => loadOptions(name),});  }
          }
         }
        if(failed[name]){ { svelteHTML.createElement("small", { "class":`bad`,});failed[name]; }}
        if(f["x-options"] === "dynamic" && !opts.length && !loading[name]){
           { svelteHTML.createElement("input", {      "value":(v as string) ?? "","placeholder":t("Or type it"),"oninput":(e) => set(name, e.currentTarget.value),});}
        }
      } else if (f.type === "number" || f.type === "integer"){
         { svelteHTML.createElement("input", {             "type":`number`,"value":(v as number) ?? f.default ?? "","min":f.minimum,"max":f.maximum,"step":f.type === "integer" ? 1 : "any","oninput":(e) => set(name, e.currentTarget.value === "" ? undefined : Number(e.currentTarget.value)),});}
      } else if (f.format === "textarea"){
         { svelteHTML.createElement("textarea", {     "maxlength":f.maxLength,"value":(v as string) ?? "","oninput":(e) => set(name, e.currentTarget.value),}); }
      } else if (f.format === "secret"){
         { svelteHTML.createElement("input", {        "type":`password`,"autocomplete":`off`,"placeholder":v && typeof v === "object" && (v as any).set ? t("Saved, type to replace") : "","oninput":(e) => set(name, e.currentTarget.value),});}
      }else{
         { svelteHTML.createElement("input", {           "type":f.format === "url" ? "url" : "text","value":(v as string) ?? "","maxlength":f.maxLength,"placeholder":typeof f.default === "string" ? f.default : "","oninput":(e) => set(name, e.currentTarget.value),});}
      }
      if(f.description){ { svelteHTML.createElement("small", {});t(f.description); }}
     }
  }
}


};
return { props: {} as any as $$ComponentProps, exports: {}, bindings: __sveltets_$$bindings(''), slots: {}, events: {} }}
const SettingsForm__SvelteComponent_ = __sveltets_2_fn_component($$render());
/*Ωignore_startΩ*/type SettingsForm__SvelteComponent_ = ReturnType<typeof SettingsForm__SvelteComponent_>;
/*Ωignore_endΩ*/export default SettingsForm__SvelteComponent_;
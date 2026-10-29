///<reference types="svelte" />
;
import Icon from "../components/Icon.svelte";
import { icons } from "../lib/icons.gen.ts";
import { req } from "../lib/deck.svelte.ts";
import { ed, reloadCatalog } from "./state.svelte.ts";
import { t } from "../lib/i18n.svelte.ts";

;type $$ComponentProps =  { value?: string; onpick: (icon: string | undefined) => void };function $$render() {

  
  
  
  
  

  let { value, onpick }:/*Ωignore_startΩ*/$$ComponentProps/*Ωignore_endΩ*/ = $props();
  let q = $state("");
  let error = $state("");
  const names = Object.keys(icons);
  const shown = $derived(q ? names.filter((n) => n.includes(q.trim().toLowerCase())) : names);
  const images = $derived(ed.catalog.assets.filter((a) => a.kind === "image"));

  async function upload(e: Event) {
    const file = (e.currentTarget as HTMLInputElement).files?.[0];
    if (!file) return;
    error = "";
    if (file.size > 512 * 1024) {
      error = t("Images can be up to 512 KB.");
      return;
    }
    const buf = new Uint8Array(await file.arrayBuffer());
    let bin = "";
    for (const b of buf) bin += String.fromCharCode(b);
    try {
      const meta = await req<{ id: string }>("assets.put", { name: file.name, data: btoa(bin) });
      await reloadCatalog();
      onpick(`asset:${meta.id}`);
    } catch (err: any) {
      error = err.message;
    }
  }
;
async () => {

 { svelteHTML.createElement("div", { "class":`picker`,});
   { svelteHTML.createElement("div", { "class":`row`,});
     { svelteHTML.createElement("input", {        "class":`input`,"bind:value":q,"placeholder":t("Search icons"),"aria-label":t("Search icons"),});/*Ωignore_startΩ*/() => q = __sveltets_2_any(null);/*Ωignore_endΩ*/}
     { svelteHTML.createElement("label", { "class":`btn`,});
       { const $$_nocI3C = __sveltets_2_ensureComponent(Icon); new $$_nocI3C({ target: __sveltets_2_any(), props: {    "name":`image`,"size":18,}});}t("Upload image");
       { svelteHTML.createElement("input", {        "type":`file`,"accept":`image/png,image/jpeg,image/webp,image/gif,image/svg+xml`,"class":`sr-only`,"onchange":upload,});}
     }
     { svelteHTML.createElement("button", {   "class":`btn ghost`,"onclick":() => onpick(undefined),});t("No icon"); }
   }
  if(error){ { svelteHTML.createElement("p", { "class":`bad`,});error; }}
  if(images.length && !q){
     { svelteHTML.createElement("p", { "class":`sub`,});t("Your images"); }
     { svelteHTML.createElement("div", { "class":`grid`,});
         for(let a of __sveltets_2_ensureArray(images)){a.id;
         { svelteHTML.createElement("button", {      "class":`cell`,"title":a.name,"onclick":() => onpick(`asset:${a.id}`),});value === `asset:${a.id}`; { const $$_nocI3C = __sveltets_2_ensureComponent(Icon); new $$_nocI3C({ target: __sveltets_2_any(), props: {  "name":`image`,}});} }
      }
     }
  }
   { svelteHTML.createElement("div", { "class":`grid`,});
       for(let n of __sveltets_2_ensureArray(shown)){n;
       { svelteHTML.createElement("button", {        "class":`cell`,"title":n,"aria-label":n,"onclick":() => onpick(n),});value === n; { const $$_nocI3C = __sveltets_2_ensureComponent(Icon); new $$_nocI3C({ target: __sveltets_2_any(), props: {  "name":n,}});} }
    }
   }
 }


};
return { props: {} as any as $$ComponentProps, exports: {}, bindings: __sveltets_$$bindings(''), slots: {}, events: {} }}
const IconPicker__SvelteComponent_ = __sveltets_2_fn_component($$render());
/*Ωignore_startΩ*/type IconPicker__SvelteComponent_ = ReturnType<typeof IconPicker__SvelteComponent_>;
/*Ωignore_endΩ*/export default IconPicker__SvelteComponent_;
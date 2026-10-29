///<reference types="svelte" />
;
import type { Profile } from "@phonedeck/protocol";
import Icon from "../components/Icon.svelte";
import { req } from "../lib/deck.svelte.ts";
import { ed, change, profile } from "./state.svelte.ts";
import { t } from "../lib/i18n.svelte.ts";

;type $$ComponentProps =  { admin?: boolean; onimported: () => void };function $$render() {

  
  
  
  
  

  let { admin = false, onimported }:/*Ωignore_startΩ*/$$ComponentProps/*Ωignore_endΩ*/ = $props();

  let code = $state("");
  let incoming = $state("");
  let gallery = $state<Array<{ id: string; name: string; author: string; description: string }> | null>(null);
  let error = $state("");
  let busy = $state(false);

  async function exportCode() {
    error = "";
    try {
      code = (await req<{ code: string }>("share.export", { profile: ed.profile })).code;
    } catch (e: any) {
      error = e.message;
    }
  }

  function add(p: Profile) {
    change((l) => l.profiles.push(p));
    ed.profile = p.id;
    ed.page = p.homePage;
    ed.key = null;
    onimported();
  }

  async function importText(text: string) {
    busy = true;
    error = "";
    try {
      const trimmed = text.trim();
      if (trimmed.startsWith("{")) {
        const parsed = JSON.parse(trimmed);
        const profiles: unknown[] = Array.isArray(parsed.profiles) ? parsed.profiles : [parsed];
        for (const p of profiles) add(await req<Profile>("share.importJson", { profile: p }));
      } else {
        add(await req<Profile>("share.import", { code: trimmed }));
      }
    } catch (e: any) {
      error = e.message;
    } finally {
      busy = false;
    }
  }

  async function file(e: Event) {
    const f = (e.currentTarget as HTMLInputElement).files?.[0];
    if (f && f.size < 4 * 1024 * 1024) await importText(await f.text());
  }

  async function browse() {
    error = "";
    try {
      gallery = (await req<{ layouts: any[] }>("gallery.list")).layouts;
    } catch (e: any) {
      error = e.message;
    }
  }

  async function fromGallery(id: string) {
    busy = true;
    try {
      add(await req<Profile>("gallery.get", { id }));
    } catch (e: any) {
      error = e.message;
    } finally {
      busy = false;
    }
  }
;
async () => {

if(error){ { svelteHTML.createElement("p", {   "class":`bad`,"role":`alert`,});error; }}

 { svelteHTML.createElement("section", { "class":`card`,});
   { svelteHTML.createElement("h3", {});t("Share {name}", { name: profile()?.name ?? "" }); }
   { svelteHTML.createElement("p", { "class":`muted`,});t("Anyone can import this code. Steps that control apps or the PC stay off on their side until they approve them."); }
  if(ed.dirty){ { svelteHTML.createElement("p", { "class":`warn`,});t("Push your draft first, so the code has your latest changes."); }}
   { svelteHTML.createElement("button", {   "class":`btn`,"onclick":exportCode,}); { const $$_nocI2C = __sveltets_2_ensureComponent(Icon); new $$_nocI2C({ target: __sveltets_2_any(), props: {    "name":`share-2`,"size":18,}});}t("Make a share code"); }
  if(code){
     { svelteHTML.createElement("textarea", {     "class":`mono`,"readonly":true,"rows":4,"value":code,}); }
     { svelteHTML.createElement("button", {   "class":`btn`,"onclick":() => navigator.clipboard?.writeText(code),}); { const $$_nocI2C = __sveltets_2_ensureComponent(Icon); new $$_nocI2C({ target: __sveltets_2_any(), props: {    "name":`copy`,"size":16,}});}t("Copy code"); }
  }
 }

 { svelteHTML.createElement("section", { "class":`card`,});
   { svelteHTML.createElement("h3", {});t("Import"); }
   { svelteHTML.createElement("p", { "class":`muted`,});t("Paste a share code, or choose a layout file exported from PhoneDeck."); }
   { svelteHTML.createElement("textarea", {       "class":`mono`,"rows":3,"bind:value":incoming,"placeholder":`PD1.…`,});/*Ωignore_startΩ*/() => incoming = __sveltets_2_any(null);/*Ωignore_endΩ*/ }
   { svelteHTML.createElement("div", { "class":`row`,});
     { svelteHTML.createElement("button", {     "class":`btn primary`,"disabled":!incoming.trim() || busy,"onclick":() => importText(incoming),});t("Import code"); }
     { svelteHTML.createElement("label", { "class":`btn`,}); { const $$_nocI3C = __sveltets_2_ensureComponent(Icon); new $$_nocI3C({ target: __sveltets_2_any(), props: {    "name":`file-up`,"size":18,}});}t("Choose file"); { svelteHTML.createElement("input", {        "type":`file`,"accept":`.json,.txt,application/json,text/plain`,"class":`sr-only`,"onchange":file,});} }
   }
  if(!admin){ { svelteHTML.createElement("p", { "class":`muted`,});t("Steps that start programs can only be imported on the PC."); }}
 }

 { svelteHTML.createElement("section", { "class":`card`,});
   { svelteHTML.createElement("h3", {});t("Layout gallery"); }
  if(gallery){
       for(let g of __sveltets_2_ensureArray(gallery)){g.id;
       { svelteHTML.createElement("div", { "class":`item`,});
         { svelteHTML.createElement("div", {}); { svelteHTML.createElement("b", {});g.name; } { svelteHTML.createElement("p", { "class":`muted`,});g.description;  g.author; } }
         { svelteHTML.createElement("button", {     "class":`btn`,"disabled":busy,"onclick":() => fromGallery(g.id),});t("Add"); }
       }
    }
       { svelteHTML.createElement("p", { "class":`muted`,});t("No shared layouts yet."); }
    
  }else{
     { svelteHTML.createElement("button", {   "class":`btn`,"onclick":browse,}); { const $$_nocI2C = __sveltets_2_ensureComponent(Icon); new $$_nocI2C({ target: __sveltets_2_any(), props: {    "name":`globe`,"size":18,}});}t("Browse layouts"); }
  }
 }


};
return { props: {} as any as $$ComponentProps, exports: {}, bindings: __sveltets_$$bindings(''), slots: {}, events: {} }}
const Share__SvelteComponent_ = __sveltets_2_fn_component($$render());
/*Ωignore_startΩ*/type Share__SvelteComponent_ = ReturnType<typeof Share__SvelteComponent_>;
/*Ωignore_endΩ*/export default Share__SvelteComponent_;
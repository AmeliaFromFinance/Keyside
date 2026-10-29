///<reference types="svelte" />
;
import Icon from "../components/Icon.svelte";
import { req } from "../lib/deck.svelte.ts";
import { ed } from "./state.svelte.ts";
import { t } from "../lib/i18n.svelte.ts";
;

  interface Item {
    profile: string;
    profileId: string;
    page: string;
    key: string;
    type: string;
    settings: Record<string, unknown>;
  };
;type $$ComponentProps =  { onapproved: () => void };function $$render() {

  
  
  
  

  let { onapproved }:/*Ωignore_startΩ*/$$ComponentProps/*Ωignore_endΩ*/ = $props();
  let items = $state<Item[] | null>(null);
  let error = $state("");

  $effect(() => void req<Item[]>("review.list").then((l) => (items = l)).catch((e) => (error = e.message)));

  async function approve() {
    try {
      await req("review.approve", {});
      onapproved();
    } catch (e: any) {
      error = e.message;
    }
  }
;
async () => {

 { svelteHTML.createElement("p", { "class":`muted`,});t("These steps came from an imported layout. They stay off until you approve them, because they can control apps or this PC."); }
if(ed.dirty){ { svelteHTML.createElement("p", { "class":`warn`,});t("Push or discard your draft first. Approving reloads the layout."); }}
if(error){ { svelteHTML.createElement("p", { "class":`bad`,});error; }}
if(items){
      for(let it of __sveltets_2_ensureArray(items)){let i = 1;i;
     { svelteHTML.createElement("div", { "class":`item`,});
       { svelteHTML.createElement("p", {}); { svelteHTML.createElement("b", {});it.key; }  { svelteHTML.createElement("span", { "class":`muted`,});it.profile;  it.page; } }
       { svelteHTML.createElement("p", { "class":`mono type`,});it.type; }
       { svelteHTML.createElement("pre", { "class":`mono`,});JSON.stringify(it.settings, null, 2); }
     }
  }
     { svelteHTML.createElement("p", {});t("Nothing to review."); }
  
  if(items.length){
     { svelteHTML.createElement("button", {     "class":`btn primary`,"disabled":ed.dirty,"onclick":approve,}); { const $$_nocI1C = __sveltets_2_ensureComponent(Icon); new $$_nocI1C({ target: __sveltets_2_any(), props: {    "name":`shield-check`,"size":18,}});}t("Approve all {n} steps", { n: items.length }); }
  }
}


};
return { props: {} as any as $$ComponentProps, exports: {}, bindings: __sveltets_$$bindings(''), slots: {}, events: {} }}
const Review__SvelteComponent_ = __sveltets_2_fn_component($$render());
/*Ωignore_startΩ*/type Review__SvelteComponent_ = ReturnType<typeof Review__SvelteComponent_>;
/*Ωignore_endΩ*/export default Review__SvelteComponent_;
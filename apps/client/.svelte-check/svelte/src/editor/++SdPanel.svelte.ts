///<reference types="svelte" />
;
import { req } from "../lib/deck.svelte.ts";
import { t } from "../lib/i18n.svelte.ts";

;type $$ComponentProps =  { instance: string; dirty?: boolean };function $$render() {

  
  

  let { instance, dirty = false }:/*Ωignore_startΩ*/$$ComponentProps/*Ωignore_endΩ*/ = $props();

  let url = $state("");
  let error = $state("");

  $effect(() => {
    const i = instance;
    url = "";
    error = "";
    req<{ url: string }>("streamdeck.inspector", { instance: i }).then(
      (r) => (url = r.url),
      (e) => (error = e.message === "key not found" || e.message === "action not found" ? t("Save the layout to open this plugin's settings.") : e.message),
    );
  });
;
async () => {

if(url){
  
   { svelteHTML.createElement("iframe", {       "title":t("Plugin settings"),"src":url,"sandbox":`allow-scripts allow-forms allow-same-origin allow-popups allow-modals`,"referrerpolicy":`no-referrer`,}); }
  if(dirty){ { svelteHTML.createElement("p", { "class":`muted`,});t("Changes here are saved right away."); }}
} else if (error){
   { svelteHTML.createElement("p", { "class":`muted`,});error; }
}


};
return { props: {} as any as $$ComponentProps, exports: {}, bindings: __sveltets_$$bindings(''), slots: {}, events: {} }}
const SdPanel__SvelteComponent_ = __sveltets_2_fn_component($$render());
/*Ωignore_startΩ*/type SdPanel__SvelteComponent_ = ReturnType<typeof SdPanel__SvelteComponent_>;
/*Ωignore_endΩ*/export default SdPanel__SvelteComponent_;
///<reference types="svelte" />
;
import type { Snippet } from "svelte";
import Icon from "./Icon.svelte";
import { t } from "../lib/i18n.svelte.ts";

;type $$ComponentProps =  { open?: boolean; title: string; wide?: boolean; onclose?: () => void; children: Snippet };function $$render() {

  
  
  

  let { open = $bindable(false), title, wide = false, onclose, children }:/*Ωignore_startΩ*/$$ComponentProps/*Ωignore_endΩ*/ = $props()/*Ωignore_startΩ*/;open;/*Ωignore_endΩ*/;
  let dialog: HTMLDialogElement;

  $effect(() => {
    if (open && !dialog.open) dialog.showModal();
    if (!open && dialog.open) dialog.close();
  });
;
async () => {

 { const $$_dialog0 = svelteHTML.createElement("dialog", {     "onclose":() => ((open = false), onclose?.()),"onclick":(e) => e.target === dialog && (open = false),});dialog = $$_dialog0;wide;
   { svelteHTML.createElement("header", {});
     { svelteHTML.createElement("h2", {});title; }
     { svelteHTML.createElement("button", {     "class":`btn icon ghost`,"aria-label":t("Close"),"onclick":() => (open = false),}); { const $$_nocI3C = __sveltets_2_ensureComponent(Icon); new $$_nocI3C({ target: __sveltets_2_any(), props: {  "name":`x`,}});} }
   }
   { svelteHTML.createElement("div", { "class":`body`,});
    if(open){;__sveltets_2_ensureSnippet(children());}
   }
 }


};
return { props: {} as any as $$ComponentProps, exports: {}, bindings: __sveltets_$$bindings('open'), slots: {}, events: {} }}
const Sheet__SvelteComponent_ = __sveltets_2_fn_component($$render());
/*Ωignore_startΩ*/type Sheet__SvelteComponent_ = ReturnType<typeof Sheet__SvelteComponent_>;
/*Ωignore_endΩ*/export default Sheet__SvelteComponent_;
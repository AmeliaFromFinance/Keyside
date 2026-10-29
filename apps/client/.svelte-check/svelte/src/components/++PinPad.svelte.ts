///<reference types="svelte" />
;
import Icon from "./Icon.svelte";
import { deck, unlock } from "../lib/deck.svelte.ts";
import { t } from "../lib/i18n.svelte.ts";
import { haptic } from "../lib/native.ts";
function $$render() {

  
  
  
  

  let pin = $state("");
  let wait = $state(0);

  $effect(() => {
    if (deck.lockReason === "wrong-pin") {
      pin = "";
      haptic("heavy");
    }
    if (deck.retryAfter > 0) {
      wait = deck.retryAfter;
      const id = setInterval(() => (wait = Math.max(0, wait - 1)), 1000);
      return () => clearInterval(id);
    }
  });

  function add(d: string) {
    if (wait > 0 || pin.length >= 12) return;
    haptic("light");
    pin += d;
  }
  function submit() {
    if (pin.length >= 4) {
      unlock(pin);
      deck.lockReason = "pin";
    }
  }
;
async () => {

 { svelteHTML.createElement("section", { "class":`pin`,});
   { const $$_nocI1C = __sveltets_2_ensureComponent(Icon); new $$_nocI1C({ target: __sveltets_2_any(), props: {    "name":`lock`,"size":32,}});}
   { svelteHTML.createElement("h1", {});t("Enter PIN"); }
   { svelteHTML.createElement("p", { });deck.lockReason !== "pin";
    if(wait > 0){
      t("Too many tries. Wait {s} s.", { s: wait });
    } else if (deck.lockReason === "wrong-pin"){
      t("Wrong PIN, try again.");
    }else{
      t("This device needs its PIN to open {name}.", { name: deck.companion?.name ?? "PhoneDeck" });
    }
   }
   { svelteHTML.createElement("div", {   "class":`dots`,"aria-label":t("{n} digits entered", { n: pin.length }),});
        for(let _ of __sveltets_2_ensureArray(({ length: Math.max(4, pin.length) }))){let i = 1;i; { svelteHTML.createElement("span", { });i < pin.length; }}
   }
   { svelteHTML.createElement("div", { "class":`pad`,});
       for(let d of __sveltets_2_ensureArray((["1", "2", "3", "4", "5", "6", "7", "8", "9"]))){d;
       { svelteHTML.createElement("button", {     "class":`btn`,"onclick":() => add(d),"disabled":wait > 0,});d; }
    }
     { svelteHTML.createElement("button", {     "class":`btn ghost`,"aria-label":t("Delete"),"onclick":() => (pin = pin.slice(0, -1)),}); { const $$_nocI3C = __sveltets_2_ensureComponent(Icon); new $$_nocI3C({ target: __sveltets_2_any(), props: {  "name":`arrow-left`,}});} }
     { svelteHTML.createElement("button", {     "class":`btn`,"onclick":() => add("0"),"disabled":wait > 0,});  }
     { svelteHTML.createElement("button", {       "class":`btn primary`,"aria-label":t("Unlock"),"onclick":submit,"disabled":pin.length < 4 || wait > 0,}); { const $$_nocI3C = __sveltets_2_ensureComponent(Icon); new $$_nocI3C({ target: __sveltets_2_any(), props: {  "name":`check`,}});} }
   }
 }


};
return { props: {} as Record<string, never>, exports: {}, bindings: __sveltets_$$bindings(''), slots: {}, events: {} }}
const PinPad__SvelteComponent_ = __sveltets_2_fn_component($$render());
/*Ωignore_startΩ*/type PinPad__SvelteComponent_ = ReturnType<typeof PinPad__SvelteComponent_>;
/*Ωignore_endΩ*/export default PinPad__SvelteComponent_;
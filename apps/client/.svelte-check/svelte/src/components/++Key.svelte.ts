///<reference types="svelte" />
;
import type { KeyView } from "@keyside/protocol";
import Icon from "./Icon.svelte";
import { assetImage } from "../lib/deck.svelte.ts";
import { clock, everySecond } from "../lib/format.ts";
import { haptic } from "../lib/native.ts";
import { t } from "../lib/i18n.svelte.ts";

;type $$ComponentProps =  {
    view: KeyView;
    disabled?: boolean;
    selected?: boolean;
    /** Editor preview: taps select instead of pressing. */
    preview?: boolean;
    ondown?: (id: string) => void;
    onup?: (id: string, cancel: boolean) => void;
    onslide?: (id: string, value: number) => void;
    onselect?: (id: string) => void;
  };function $$render() {

  
  
  
  
  
  

  let {
    view,
    disabled = false,
    selected = false,
    preview = false,
    ondown,
    onup,
    onslide,
    onselect,
  }:/*Ωignore_startΩ*/$$ComponentProps/*Ωignore_endΩ*/ = $props();

  let pressed = $state(false);
  let holdStart = $state(0);
  let now = $state(Date.now());
  let image = $state<string | null>(null);
  let slideValue = $state<number | null>(null);
  let el: HTMLElement;

  $effect(() => {
    if (!view.since) return;
    return everySecond((n) => (now = n));
  });

  $effect(() => {
    const icon = view.icon;
    image = null;
    if (icon?.startsWith("asset:")) void assetImage(icon.slice(6)).then((url) => (image = url));
  });

  const plainIcon = $derived(view.icon && !view.icon.includes(":") ? view.icon : undefined);
  const timer = $derived(view.since ? clock(now - view.since) : null);
  const meta = $derived(timer ?? view.meta);
  const badgeTone = $derived(view.badgeTone ?? view.tone ?? "default");
  const filledBadge = $derived(badgeTone === "accent" || badgeTone === "danger");
  const value = $derived(slideValue ?? view.value ?? 0);
  const holdMs = $derived(view.hold ?? 0);
  const tall = $derived(view.h > 1 && view.h >= view.w);

  function down(e: PointerEvent) {
    if (e.button > 0) return;
    if (preview) {
      onselect?.(view.id);
      return;
    }
    if (disabled || view.disabled) return;
    el.setPointerCapture?.(e.pointerId);
    pressed = true;
    haptic("light");
    if (view.kind === "slider") {
      moveSlider(e);
      return;
    }
    holdStart = holdMs > 0 ? performance.now() : 0;
    ondown?.(view.id);
  }

  function up(e: PointerEvent, cancel = false) {
    if (!pressed) return;
    pressed = false;
    el.releasePointerCapture?.(e.pointerId);
    if (view.kind === "slider") {
      slideValue = null;
      return;
    }
    const held = holdStart ? performance.now() - holdStart : 0;
    holdStart = 0;
    if (holdMs > 0 && held >= holdMs) haptic("medium");
    onup?.(view.id, cancel);
  }

  function moveSlider(e: PointerEvent) {
    const r = el.getBoundingClientRect();
    const v = tall ? 1 - (e.clientY - r.top) / r.height : (e.clientX - r.left) / r.width;
    slideValue = Math.min(1, Math.max(0, v));
    onslide?.(view.id, slideValue);
  }

  function move(e: PointerEvent) {
    if (pressed && view.kind === "slider") moveSlider(e);
  }

  function key(e: KeyboardEvent, isDown: boolean) {
    if (e.key !== "Enter" && e.key !== " ") return;
    e.preventDefault();
    if (preview) {
      if (isDown) onselect?.(view.id);
      return;
    }
    if (view.kind === "slider") {
      if (isDown) onslide?.(view.id, Math.min(1, Math.max(0, (view.value ?? 0) + (e.shiftKey ? -0.1 : 0.1))));
      return;
    }
    if (isDown && !pressed) {
      pressed = true;
      holdStart = holdMs > 0 ? performance.now() : 0;
      ondown?.(view.id);
    } else if (!isDown && pressed) {
      pressed = false;
      holdStart = 0;
      onup?.(view.id, false);
    }
  }
;
async () => {


  { const $$_div0 = svelteHTML.createElement("div", {                                      "class":`key kind-${view.kind} tone-${view.tone ?? 'default'} color-${view.color ?? 'default'}`,"role":view.kind === "slider" ? "slider" : "button","tabindex":0,"aria-label":[view.label, view.sub, view.badge].filter(Boolean).join(", "),"aria-valuenow":view.kind === "slider" ? Math.round(value * 100) : undefined,"aria-valuemin":view.kind === "slider" ? 0 : undefined,"aria-valuemax":view.kind === "slider" ? 100 : undefined,"aria-disabled":disabled || view.disabled,"onpointerdown":down,"onpointerup":(e) => up(e),"onpointercancel":(e) => up(e, true),"onpointermove":move,"onkeydown":(e) => key(e, true),"onkeyup":(e) => key(e, false),"oncontextmenu":(e) => e.preventDefault(),});el = $$_div0;pressed;selected;disabled || view.disabled;holdStart > 0;tall;__sveltets_2_ensureType(String, Number, `${holdMs} `);__sveltets_2_ensureType(String, Number, value);
  if(view.kind === "slider"){
     { svelteHTML.createElement("div", {   "class":`fill`,"aria-hidden":`true`,}); }
  }
  if(view.image || image){
     { svelteHTML.createElement("img", {      "class":`art`,"src":view.image ?? image,"alt":"","draggable":`false`,});}
  }
   { svelteHTML.createElement("div", { "class":`top`,});
     { svelteHTML.createElement("span", { "class":`icon`,});
      if(plainIcon){ { const $$_nocI3C = __sveltets_2_ensureComponent(Icon); new $$_nocI3C({ target: __sveltets_2_any(), props: {  "name":plainIcon,}});}}
     }
    if(view.badge){
       { svelteHTML.createElement("span", {  "class":`badge tone-${badgeTone}`,});filledBadge;view.badge; }
    } else if (meta){
       { svelteHTML.createElement("span", { "class":`meta mono`,});meta; }
    } else if (view.kind === "folder"){
       { svelteHTML.createElement("span", { "class":`chev`,}); { const $$_nocI3C = __sveltets_2_ensureComponent(Icon); new $$_nocI3C({ target: __sveltets_2_any(), props: {    "name":`chevron-right`,"size":20,}});} }
    }
   }

  if(view.lines){
     { svelteHTML.createElement("ol", {   "class":`lines`,"aria-live":`polite`,});
         for(let line of __sveltets_2_ensureArray(view.lines)){line.at + line.text;
         { svelteHTML.createElement("li", {});
          if(line.author){ { svelteHTML.createElement("b", { });__sveltets_2_ensureType(String, Number, line.color);line.author; }}
          line.text;
         }
      }
         { svelteHTML.createElement("li", { "class":`empty`,});t("Nothing yet"); }
      
     }
  }

   { svelteHTML.createElement("div", { "class":`bottom`,});
     { svelteHTML.createElement("div", { "class":`label`,});view.label; }
    if(view.sub){ { svelteHTML.createElement("div", { "class":`sub`,});view.sub; }}
    if(view.kind === "slider"){ { svelteHTML.createElement("div", { "class":`sub mono`,});Math.round(value * 100);  }}
   }

  if(view.progress !== undefined){
     { svelteHTML.createElement("div", {   "class":`progress`,"aria-hidden":`true`,}); { svelteHTML.createElement("span", { });__sveltets_2_ensureType(String, Number, `${Math.round(view.progress * 100)} `); } }
  }
  if(holdMs > 0){
     { svelteHTML.createElement("div", {   "class":`hold`,"aria-hidden":`true`,}); }
  }
 }


};
return { props: {} as any as $$ComponentProps, exports: {}, bindings: __sveltets_$$bindings(''), slots: {}, events: {} }}
const Key__SvelteComponent_ = __sveltets_2_fn_component($$render());
/*Ωignore_startΩ*/type Key__SvelteComponent_ = ReturnType<typeof Key__SvelteComponent_>;
/*Ωignore_endΩ*/export default Key__SvelteComponent_;
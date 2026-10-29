///<reference types="svelte" />
;
import { icons } from "../lib/icons.gen.ts";

;type $$ComponentProps =  { name?: string; size?: number; stroke?: number };function $$render() {

  

  let { name, size = 24, stroke = 2 }:/*Ωignore_startΩ*/$$ComponentProps/*Ωignore_endΩ*/ = $props();
;
async () => {

if(name && icons[name]){
   { svelteHTML.createElement("svg", {                    "xmlns":`http://www.w3.org/2000/svg`,"width":size,"height":size,"viewBox":`0 0 24 24`,"fill":`none`,"stroke":`currentColor`,"stroke-width":stroke,"stroke-linecap":`round`,"stroke-linejoin":`round`,"aria-hidden":`true`,}); icons[name]; }
}
};
return { props: {} as any as $$ComponentProps, exports: {}, bindings: __sveltets_$$bindings(''), slots: {}, events: {} }}
const Icon__SvelteComponent_ = __sveltets_2_fn_component($$render());
/*Ωignore_startΩ*/type Icon__SvelteComponent_ = ReturnType<typeof Icon__SvelteComponent_>;
/*Ωignore_endΩ*/export default Icon__SvelteComponent_;
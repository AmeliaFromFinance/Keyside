///<reference types="svelte" />
;
import type { Snippet } from "svelte";
import type { KeyView } from "@phonedeck/protocol";
import Key from "./Key.svelte";

;type $$ComponentProps =  {
    cols: number;
    rows: number;
    keys: KeyView[];
    locked?: boolean;
    preview?: boolean;
    selected?: string | null;
    ondown?: (id: string) => void;
    onup?: (id: string, cancel: boolean) => void;
    onslide?: (id: string, v: number) => void;
    onselect?: (id: string) => void;
    /** Editor only: what to draw in an empty cell. */
    empty?: Snippet<[number, number]>;
  };function $$render() {

  
  
  

  let {
    cols,
    rows,
    keys,
    locked = false,
    preview = false,
    selected = null,
    ondown,
    onup,
    onslide,
    onselect,
    empty,
  }:/*Ωignore_startΩ*/$$ComponentProps/*Ωignore_endΩ*/ = $props();

  const free = $derived.by(() => {
    if (!empty) return [];
    const used = new Set<string>();
    for (const k of keys) for (let dy = 0; dy < k.h; dy++) for (let dx = 0; dx < k.w; dx++) used.add(`${k.x + dx},${k.y + dy}`);
    const out: Array<[number, number]> = [];
    for (let y = 0; y < rows; y++) for (let x = 0; x < cols; x++) if (!used.has(`${x},${y}`)) out.push([x, y]);
    return out;
  });
;
async () => {

 { svelteHTML.createElement("div", { "class":`wrap`,});
   { svelteHTML.createElement("div", {   "class":`grid`,});__sveltets_2_ensureType(String, Number, cols);__sveltets_2_ensureType(String, Number, rows);
       for(let k of __sveltets_2_ensureArray(keys)){k.id;
       { svelteHTML.createElement("div", {   "class":`cell`,});__sveltets_2_ensureType(String, Number, `${k.x + 1}   ${k.w}`);__sveltets_2_ensureType(String, Number, `${k.y + 1}   ${k.h}`);
         { const $$_yeK3C = __sveltets_2_ensureComponent(Key); new $$_yeK3C({ target: __sveltets_2_any(), props: {           "view":k,"disabled":locked,preview,"selected":selected === k.id,ondown,onup,onslide,onselect,}});}
       }
    }
       for(let [x, y] of __sveltets_2_ensureArray(free)){`${x},${y}`;
       { svelteHTML.createElement("div", {   "class":`cell`,});__sveltets_2_ensureType(String, Number, x + 1);__sveltets_2_ensureType(String, Number, y + 1);;__sveltets_2_ensureSnippet(empty?.(x, y)); }
    }
   }
 }


};
return { props: {} as any as $$ComponentProps, exports: {}, bindings: __sveltets_$$bindings(''), slots: {}, events: {} }}
const Grid__SvelteComponent_ = __sveltets_2_fn_component($$render());
/*Ωignore_startΩ*/type Grid__SvelteComponent_ = ReturnType<typeof Grid__SvelteComponent_>;
/*Ωignore_endΩ*/export default Grid__SvelteComponent_;
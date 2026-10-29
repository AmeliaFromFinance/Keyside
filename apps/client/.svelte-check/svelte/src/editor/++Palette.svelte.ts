///<reference types="svelte" />
;
import Icon from "../components/Icon.svelte";
import { ed, allActions, type ActionDef } from "./state.svelte.ts";
import { t, mark } from "../lib/i18n.svelte.ts";

;type $$ComponentProps =  { admin?: boolean };function $$render() {

  
  
  

  let { admin = false }:/*Ωignore_startΩ*/$$ComponentProps/*Ωignore_endΩ*/ = $props();
  let q = $state("");
  let showAll = $state<Record<string, boolean>>({});

  const layoutItems: ActionDef[] = [
    { id: "kind:folder", name: mark("Folder"), group: mark("Layout"), icon: "folder", description: mark("sub-page") },
    { id: "kind:slider", name: mark("Slider"), group: mark("Layout"), icon: "sliders-vertical", description: mark("value") },
    { id: "kind:widget", name: mark("Text or chat"), group: mark("Layout"), icon: "message-square", description: mark("live text") },
    { id: "kind:button", name: mark("Multi-action"), group: mark("Layout"), icon: "layers", description: mark("steps") },
  ];

  const groups = $derived.by(() => {
    const needle = q.trim().toLowerCase();
    const items = [...allActions().filter((a) => admin || !a.adminOnly), ...layoutItems];
    const map = new Map<string, ActionDef[]>();
    for (const a of items) {
      if (needle && !`${a.name} ${a.group} ${a.description ?? ""}`.toLowerCase().includes(needle)) continue;
      const g = a.group ?? mark("Other");
      if (!map.has(g)) map.set(g, []);
      map.get(g)!.push(a);
    }
    return [...map.entries()];
  });

  function drag(e: DragEvent, id: string) {
    e.dataTransfer?.setData("application/x-keyside-action", id);
    e.dataTransfer!.effectAllowed = "copy";
    ed.armed = id;
  }
;
async () => {

 { svelteHTML.createElement("aside", { "class":`palette`,});
   { svelteHTML.createElement("h2", {});t("Button palette"); }
   { svelteHTML.createElement("label", { "class":`search`,});
     { const $$_nocI2C = __sveltets_2_ensureComponent(Icon); new $$_nocI2C({ target: __sveltets_2_any(), props: {    "name":`search`,"size":18,}});}
     { svelteHTML.createElement("input", {      "bind:value":q,"placeholder":t("Search actions"),"aria-label":t("Search actions"),});/*Ωignore_startΩ*/() => q = __sveltets_2_any(null);/*Ωignore_endΩ*/}
   }
   { svelteHTML.createElement("p", { "class":`tip`,});t("Drag an action onto an empty cell, or tap it and then tap a cell."); }
     for(let [group, items] of __sveltets_2_ensureArray(groups)){group;
    const limit = q || showAll[group] ? items.length : 6;
     { svelteHTML.createElement("section", {});
       { svelteHTML.createElement("h3", {}); { svelteHTML.createElement("span", {});t(group); } { svelteHTML.createElement("span", { "class":`count`,});items.length; } }
         for(let a of __sveltets_2_ensureArray((items.slice(0, limit)))){a.id;
         { svelteHTML.createElement("button", {              "class":`item`,"draggable":`true`,"ondragstart":(e) => drag(e, a.id),"ondragend":() => (ed.armed = null),"onclick":() => (ed.armed = ed.armed === a.id ? null : a.id),"title":a.description,});ed.armed === a.id;
           { const $$_nocI3C = __sveltets_2_ensureComponent(Icon); new $$_nocI3C({ target: __sveltets_2_any(), props: {    "name":a.icon ?? "bolt","size":20,}});}
           { svelteHTML.createElement("span", { "class":`name`,});t(a.name); }
           { svelteHTML.createElement("span", { "class":`hint`,});ed.armed === a.id ? t("tap a cell") : t(a.description ?? ""); }
         }
      }
      if(items.length > limit){
         { svelteHTML.createElement("button", {   "class":`more`,"onclick":() => (showAll[group] = true),});t("Show all {n} actions", { n: items.length }); }
      }
     }
  }
 }


};
return { props: {} as any as $$ComponentProps, exports: {}, bindings: __sveltets_$$bindings(''), slots: {}, events: {} }}
const Palette__SvelteComponent_ = __sveltets_2_fn_component($$render());
/*Ωignore_startΩ*/type Palette__SvelteComponent_ = ReturnType<typeof Palette__SvelteComponent_>;
/*Ωignore_endΩ*/export default Palette__SvelteComponent_;
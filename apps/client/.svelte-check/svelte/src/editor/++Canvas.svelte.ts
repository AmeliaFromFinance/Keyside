///<reference types="svelte" />
;
import Grid from "../components/Grid.svelte";
import Icon from "../components/Icon.svelte";
import type { FormFactor, KeyKind, KeyView } from "@keyside/protocol";
import { ed, page, profile, change, addKey, addPage, moveKey, placements, previewView, actionDef } from "./state.svelte.ts";
import { t, mark } from "../lib/i18n.svelte.ts";
import { deck } from "../lib/deck.svelte.ts";
function $$render() {

  
  
  
  
  
  

  const factors: Array<[FormFactor, string]> = [
    ["phone-portrait", mark("Phone")],
    ["phone-landscape", mark("Phone, sideways")],
    ["tablet", mark("Tablet")],
    ["desktop", mark("Desktop browser")],
  ];

  const pg = $derived(page());
  const place = $derived(pg ? placements(pg, ed.ff) : null);
  const views = $derived.by((): KeyView[] => {
    if (!pg || !place) return [];
    const live = new Map((ed.live?.id === pg.id ? ed.live.keys : []).map((k) => [k.id, k]));
    return pg.keys.filter((k) => place.at.has(k.id)).map((k) => previewView(k, live.get(k.id), place.at.get(k.id)));
  });
  const armedName = $derived(ed.armed ? (actionDef(ed.armed)?.name ?? ed.armed.replace("kind:", "")) : "");
  const variant = $derived(!!pg?.variants?.[ed.ff]);
  const tabs = $derived((profile()?.pages ?? []).filter((p) => !p.folder || p.id === ed.page));

  function place_(x: number, y: number, id: string | null) {
    if (!pg) return;
    if (ed.moving) {
      if (variant) moveInVariant(ed.moving, x, y);
      else moveKey(ed.moving, x, y);
      ed.moving = null;
      return;
    }
    if (!id || variant) return;
    if (id.startsWith("kind:")) {
      const kind = id.slice(5) as KeyKind;
      if (kind === "folder") {
        const target = addPage(true);
        addKey(x, y, null, "folder");
        change(() => {
          const k = pg.keys.find((q) => q.id === ed.key);
          if (k) {
            k.target = target;
            k.label = "Folder";
          }
        });
      } else {
        addKey(x, y, null, kind);
        change(() => {
          const k = pg.keys.find((q) => q.id === ed.key);
          if (!k) return;
          if (kind === "slider") {
            k.label = "Slider";
            k.slider = { min: 0, max: 1 };
          }
          if (kind === "widget") {
            k.label = "Text";
            k.widget = { text: "{user.message}" };
          }
          if (kind === "button") k.label = "Multi-action";
        });
      }
    } else addKey(x, y, id);
    ed.armed = null;
  }

  function moveInVariant(id: string, x: number, y: number) {
    change(() => {
      const v = pg?.variants?.[ed.ff];
      const cur = v?.at[id];
      if (v && cur) v.at[id] = [x, y, cur[2], cur[3]];
    });
  }

  function drop(e: DragEvent, x: number, y: number) {
    e.preventDefault();
    const action = e.dataTransfer?.getData("application/x-keyside-action");
    const key = e.dataTransfer?.getData("application/x-keyside-key");
    if (key) {
      ed.moving = key;
      place_(x, y, null);
    } else place_(x, y, action || ed.armed);
  }

  function customize() {
    if (!pg || !place) return;
    change(() => {
      pg.variants ??= {};
      pg.variants[ed.ff] = { cols: place.cols, rows: place.rows, at: Object.fromEntries([...place.at.entries()]) };
    });
  }

  function resetVariant() {
    change(() => {
      if (pg?.variants) delete pg.variants[ed.ff];
    });
  }

  function setGrid(cols: number, rows: number) {
    if (!pg) return;
    change(() => {
      if (variant && pg.variants?.[ed.ff]) {
        const v = pg.variants[ed.ff]!;
        v.cols = cols;
        v.rows = rows;
        for (const [id, p] of Object.entries(v.at)) if (p[0] + p[2] > cols || p[1] + p[3] > rows) delete v.at[id];
      } else {
        pg.cols = cols;
        pg.rows = rows;
        pg.keys = pg.keys.filter((k) => k.x + (k.w ?? 1) <= cols && k.y + (k.h ?? 1) <= rows);
      }
    });
  }

  function keydrag(e: DragEvent) {
    const id = (e.target as HTMLElement)?.closest?.("[data-key]")?.getAttribute("data-key");
    if (id) e.dataTransfer?.setData("application/x-keyside-key", id);
  }
;
async () => {

 { svelteHTML.createElement("section", { "class":`canvas`,});
   { svelteHTML.createElement("div", { "class":`bar`,});
     { svelteHTML.createElement("div", {   "class":`tabs`,"role":`tablist`,});
         for(let p of __sveltets_2_ensureArray(tabs)){p.id;
         { svelteHTML.createElement("button", {        "role":`tab`,"class":`tab`,"aria-selected":p.id === ed.page,"onclick":() => ((ed.page = p.id), (ed.key = null)),});p.id === ed.page;
          if(p.folder){ { const $$_nocI4C = __sveltets_2_ensureComponent(Icon); new $$_nocI4C({ target: __sveltets_2_any(), props: {    "name":`folder`,"size":16,}});}}p.name;
         }
      }
       { svelteHTML.createElement("button", {       "class":`tab add`,"aria-label":t("Add page"),"title":t("Add page"),"onclick":() => addPage(),}); { const $$_nocI4C = __sveltets_2_ensureComponent(Icon); new $$_nocI4C({ target: __sveltets_2_any(), props: {    "name":`plus`,"size":18,}});} }
     }
     { svelteHTML.createElement("div", { "class":`controls`,});
      if(pg && place){
         { svelteHTML.createElement("label", {});
           { svelteHTML.createElement("span", {});t("Grid"); }
           { svelteHTML.createElement("select", {   "value":`${place.cols}x${place.rows}`,"onchange":(e) => setGrid(...(e.currentTarget.value.split("x").map(Number) as [number, number])),});
               for(let [c, r] of __sveltets_2_ensureArray(([[2, 2], [3, 2], [2, 3], [3, 3], [3, 4], [4, 3], [3, 5], [5, 3], [4, 4], [4, 5], [5, 4], [5, 5], [6, 4], [8, 4], [6, 6], [8, 5], [10, 6], [12, 8]]))){`${c}x${r}`;
               { svelteHTML.createElement("option", { "value":`${c}x${r}`,});c;  r; }
            }
            if(![2, 3, 4, 5, 6, 8, 10, 12].includes(place.cols)){ { svelteHTML.createElement("option", { "value":`${place.cols}x${place.rows}`,});place.cols;  place.rows; }}
           }
         }
      }
       { svelteHTML.createElement("label", {});
         { svelteHTML.createElement("span", {});t("Preview"); }
         { svelteHTML.createElement("select", { "bind:value":ed.ff,});/*Ωignore_startΩ*/() => ed.ff = __sveltets_2_any(null);/*Ωignore_endΩ*/
             for(let [ff, name] of __sveltets_2_ensureArray(factors)){ff; { svelteHTML.createElement("option", { "value":ff,});t(name); }}
         }
       }
     }
   }

  if(pg && place){
     { svelteHTML.createElement("div", { "class":`stage`,});
       { svelteHTML.createElement("div", {     "class":`device ff-${ed.ff}`,"ondragstart":keydrag,"role":`presentation`,});
         { svelteHTML.createElement("div", { "class":`device-head`,});
           { svelteHTML.createElement("b", {});pg.name; }
           { svelteHTML.createElement("span", { "class":`live`,}); { svelteHTML.createElement("span", {  "class":`dot`,});deck.conn.s === "open"; }t("live preview"); }
         }
         { const $$_dirG3C = __sveltets_2_ensureComponent(Grid); const $$_dirG3 = new $$_dirG3C({ target: __sveltets_2_any(), props: {            "cols":place.cols,"rows":place.rows,"keys":views,"preview":true,"selected":ed.key,"onselect":(id) => {
            if (ed.moving === id) ed.moving = null;
            ed.key = id;
            ed.armed = null;
          },empty:(x, y) => { async ()/*Ωignore_positionΩ*/ => {
             { svelteHTML.createElement("button", {            "class":`drop`,"ondragover":(e) => e.preventDefault(),"ondrop":(e) => drop(e, x, y),"onclick":() => (ed.armed || ed.moving ? place_(x, y, ed.armed) : ((ed.key = null), undefined)),"aria-label":ed.moving ? t("Move here") : ed.armed ? t("Drop {name} here", { name: armedName }) : t("Empty cell"),});!!ed.armed || !!ed.moving;
              if(ed.moving){
                 { const $$_nocI1C = __sveltets_2_ensureComponent(Icon); new $$_nocI1C({ target: __sveltets_2_any(), props: {    "name":`move`,"size":20,}});} { svelteHTML.createElement("span", {});t("Move here"); }
              } else if (ed.armed){
                 { const $$_nocI1C = __sveltets_2_ensureComponent(Icon); new $$_nocI1C({ target: __sveltets_2_any(), props: {    "name":`plus`,"size":20,}});} { svelteHTML.createElement("span", {});t("Drop {name}", { name: t(armedName) }); }
              }
             }
          };return __sveltets_2_any(0)},}});/*Ωignore_startΩ*/const {empty} = $$_dirG3.$$prop_def;/*Ωignore_endΩ*/
          
         Grid}
       }
       { svelteHTML.createElement("p", { "class":`caption`,});
        if(variant){
          t("This screen has its own arrangement.");  { svelteHTML.createElement("button", {   "class":`link`,"onclick":resetVariant,});t("Use the main one"); }
        } else if (ed.ff !== "phone-portrait"){
          t("Arranged automatically from the phone layout.");  { svelteHTML.createElement("button", {   "class":`link`,"onclick":customize,});t("Arrange this screen yourself"); }
        }else{
          t("States are live from your apps. Changes reach devices when you push them.");
        }
       }
     }
  }
 }


};
return { props: {} as Record<string, never>, exports: {}, bindings: __sveltets_$$bindings(''), slots: {}, events: {} }}
const Canvas__SvelteComponent_ = __sveltets_2_fn_component($$render());
/*Ωignore_startΩ*/type Canvas__SvelteComponent_ = ReturnType<typeof Canvas__SvelteComponent_>;
/*Ωignore_endΩ*/export default Canvas__SvelteComponent_;
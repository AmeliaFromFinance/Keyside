///<reference types="svelte" />
;
import Icon from "../components/Icon.svelte";
import Sheet from "../components/Sheet.svelte";
import Palette from "./Palette.svelte";
import Canvas from "./Canvas.svelte";
import Inspector from "./Inspector.svelte";
import PairPanel from "./PairPanel.svelte";
import Plugins from "./Plugins.svelte";
import Settings from "./Settings.svelte";
import Review from "./Review.svelte";
import Share from "./Share.svelte";
import StreamDeck from "./StreamDeck.svelte";
import Profiles from "./Profiles.svelte";
import { ed, load, push, discard, undo, redo, canUndo, canRedo, startLive, change, newId, page } from "./state.svelte.ts";
import { deck, req } from "../lib/deck.svelte.ts";
import { t } from "../lib/i18n.svelte.ts";
import { ago, everySecond } from "../lib/format.ts";
;

  type Panel = "plugins" | "settings" | "review" | "share" | "streamdeck" | "devices" | "profiles" | null;;
;type $$ComponentProps =  { admin?: boolean; ondeck: () => void };function $$render() {

  
  
  
  
  
  
  
  
  
  
  
  
  
  
  
  

  let { admin = false, ondeck }:/*Ωignore_startΩ*/$$ComponentProps/*Ωignore_endΩ*/ = $props();

  let panel = $state<Panel>(null);
  let menu = $state(false);
  let paletteOpen = $state(false);
  let now = $state(Date.now());
  let loaded = $state(false);
  let recording = $state(false);
  let plugins = $state<Array<{ id: string; name: string; status: string; values: Record<string, unknown> }>>([]);
  let devicesOnline = $state(0);
  let reviewCount = $state(0);

  $effect(() => everySecond((n) => (now = n)));

  $effect(() => {
    if (deck.conn.s === "open" && !loaded) {
      loaded = true;
      void load().then(refreshMeta);
    }
    if (deck.conn.s !== "open") loaded = false;
  });

  $effect(() => {
    if (!loaded) return;
    return startLive();
  });

  async function refreshMeta() {
    try {
      plugins = await req("plugins.list");
      if (admin) {
        const devs = await req<Array<{ online: boolean }>>("devices.list");
        devicesOnline = devs.filter((d) => d.online).length;
        reviewCount = (await req<unknown[]>("review.list")).length;
      }
    } catch {
      /* reconnecting */
    }
  }

  $effect(() => {
    void deck.connections;
    if (loaded) void refreshMeta();
  });

  function keys(e: KeyboardEvent) {
    const mod = e.ctrlKey || e.metaKey;
    if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement || e.target instanceof HTMLSelectElement) return;
    if (mod && e.key.toLowerCase() === "z") {
      e.preventDefault();
      if (e.shiftKey) redo();
      else undo();
    } else if (mod && e.key.toLowerCase() === "y") {
      e.preventDefault();
      redo();
    } else if (mod && e.key.toLowerCase() === "s") {
      e.preventDefault();
      void push();
    } else if ((e.key === "Delete" || e.key === "Backspace") && ed.key) {
      const pg = page();
      const id = ed.key;
      if (pg) {
        change(() => (pg.keys = pg.keys.filter((k) => k.id !== id)));
        ed.key = null;
      }
    } else if (e.key === "Escape") {
      ed.armed = null;
      ed.moving = null;
      ed.key = null;
    }
  }

  function pickProfile(sel: HTMLSelectElement) {
    if (sel.value === "__manage") {
      sel.value = ed.profile;
      panel = "profiles";
      return;
    }
    ed.profile = sel.value;
    ed.page = ed.layout?.profiles.find((p) => p.id === sel.value)?.homePage ?? "";
    ed.key = null;
  }

  function exportJson() {
    if (!ed.layout) return;
    const blob = new Blob([JSON.stringify($state.snapshot(ed.layout), null, 2)], { type: "application/json" });
    const a = document.createElement("a");
    a.href = URL.createObjectURL(blob);
    a.download = "phonedeck-layout.json";
    a.click();
    setTimeout(() => URL.revokeObjectURL(a.href), 1000);
  }

  async function toggleRecord() {
    if (!recording) {
      await req("recorder.start");
      recording = true;
      return;
    }
    recording = false;
    const steps = await req<any[]>("recorder.stop");
    const pg = page();
    if (!steps.length || !pg) return;
    for (let y = 0; y < pg.rows; y++)
      for (let x = 0; x < pg.cols; x++) {
        if (pg.keys.some((k) => x >= k.x && x < k.x + (k.w ?? 1) && y >= k.y && y < k.y + (k.h ?? 1))) continue;
        const id = newId("k", pg.keys.map((k) => k.id));
        change(() => pg.keys.push({ id, x, y, kind: "button", label: "Recorded", icon: "circle-dot", actions: steps }));
        ed.key = id;
        return;
      }
    ed.error = t("No free cell for the recorded key. Make room and record again.");
  }

  const usedPlugins = $derived(
    new Set(ed.layout?.profiles.flatMap((pr) => pr.pages.flatMap((pg) => pg.keys.flatMap((k) => [...k.actions, ...(k.offActions ?? [])].map((a) => a.type.split("/")[0])))) ?? []),
  );
  const pills = $derived(
    plugins
      .filter((p) => p.status !== "disabled" && usedPlugins.has(p.id))
      .slice(0, 3)
      .map((p) => ({ ...p, port: typeof p.values?.port === "number" ? p.values.port : null })),
  );
;
async () => {

 { svelteHTML.createElement("svelte:window", {  "onkeydown":keys,});}

 { svelteHTML.createElement("div", { "class":`editor`,});
   { svelteHTML.createElement("header", { "class":`top`,});
     { svelteHTML.createElement("div", { "class":`brand`,});
       { svelteHTML.createElement("button", {     "class":`btn icon ghost burger`,"aria-label":t("Button palette"),"onclick":() => (paletteOpen = !paletteOpen),}); { const $$_nocI4C = __sveltets_2_ensureComponent(Icon); new $$_nocI4C({ target: __sveltets_2_any(), props: {  "name":`layout-grid`,}});} }
       { const $$_nocI3C = __sveltets_2_ensureComponent(Icon); new $$_nocI3C({ target: __sveltets_2_any(), props: {    "name":`layout-grid`,"size":30,}});}
       { svelteHTML.createElement("b", {});  } { svelteHTML.createElement("span", { "class":`sub`,});t("Editor"); }
     }
     { svelteHTML.createElement("label", { "class":`layout`,});
       { svelteHTML.createElement("span", {});t("Layout"); }
       { svelteHTML.createElement("select", {   "value":ed.profile,"onchange":(e) => pickProfile(e.currentTarget),});
           for(let p of __sveltets_2_ensureArray(ed.layout?.profiles ?? [])){p.id; { svelteHTML.createElement("option", { "value":p.id,});p.name; }}
         { svelteHTML.createElement("option", { "value":`__manage`,});t("Manage layouts…"); }
       }
     }
     { svelteHTML.createElement("div", { "class":`pills`,});
         for(let p of __sveltets_2_ensureArray(pills)){p.id;
         { svelteHTML.createElement("span", { "class":`chip`,}); { svelteHTML.createElement("span", {    "class":`dot`,});p.status === "connected";p.status === "connecting";p.status === "error"; }p.name;if(p.port){ { svelteHTML.createElement("span", { "class":`mono port`,}); p.port; }} }
      }
      if(admin){ { svelteHTML.createElement("span", { "class":`chip`,}); { svelteHTML.createElement("span", {  "class":`dot`,});devicesOnline > 0; }t("{n} phones online", { n: devicesOnline }); }}
     }
     { svelteHTML.createElement("div", { "class":`actions`,});
      if(ed.dirty){
         { svelteHTML.createElement("span", { "class":`saved`,});t("Draft saved {t} ago", { t: ago(ed.savedAt, now) }); }
      } else if (loaded){
         { svelteHTML.createElement("span", { "class":`saved`,});t("Up to date"); }
      }
       { svelteHTML.createElement("button", {         "class":`btn icon ghost`,"aria-label":t("Undo"),"title":t("Undo"),"disabled":!canUndo(),"onclick":undo,}); { const $$_nocI4C = __sveltets_2_ensureComponent(Icon); new $$_nocI4C({ target: __sveltets_2_any(), props: {  "name":`undo-2`,}});} }
       { svelteHTML.createElement("button", {         "class":`btn icon ghost`,"aria-label":t("Redo"),"title":t("Redo"),"disabled":!canRedo(),"onclick":redo,}); { const $$_nocI4C = __sveltets_2_ensureComponent(Icon); new $$_nocI4C({ target: __sveltets_2_any(), props: {  "name":`redo-2`,}});} }
       { svelteHTML.createElement("button", {   "class":`btn`,"onclick":() => (panel = "share"),});t("Import"); }
       { svelteHTML.createElement("button", {   "class":`btn`,"onclick":exportJson,});t("Export JSON"); }
       { svelteHTML.createElement("button", {     "class":`btn primary`,"onclick":push,"disabled":!ed.dirty || ed.pushing,});ed.pushing ? t("Pushing…") : t("Push to phones"); }
       { svelteHTML.createElement("button", {       "class":`btn icon`,"aria-label":t("More"),"aria-expanded":menu,"onclick":() => (menu = !menu),}); { const $$_nocI4C = __sveltets_2_ensureComponent(Icon); new $$_nocI4C({ target: __sveltets_2_any(), props: {  "name":`settings`,}});} }
     }
   }

  if(menu){
     { svelteHTML.createElement("nav", {   "class":`menu`,"aria-label":t("Editor menu"),});
       { svelteHTML.createElement("button", { "onclick":() => ((menu = false), ondeck()),}); { const $$_nocI3C = __sveltets_2_ensureComponent(Icon); new $$_nocI3C({ target: __sveltets_2_any(), props: {    "name":`smartphone`,"size":18,}});}t("Open the deck"); }
       { svelteHTML.createElement("button", { "onclick":toggleRecord,}); { const $$_nocI3C = __sveltets_2_ensureComponent(Icon); new $$_nocI3C({ target: __sveltets_2_any(), props: {    "name":recording ? "circle-stop" : "circle-dot","size":18,}});}recording ? t("Stop recording steps") : t("Record steps"); }
       { svelteHTML.createElement("button", { "onclick":() => ((menu = false), (panel = "share")),}); { const $$_nocI3C = __sveltets_2_ensureComponent(Icon); new $$_nocI3C({ target: __sveltets_2_any(), props: {    "name":`share-2`,"size":18,}});}t("Share and gallery"); }
       { svelteHTML.createElement("button", { "onclick":() => ((menu = false), (panel = "plugins")),}); { const $$_nocI3C = __sveltets_2_ensureComponent(Icon); new $$_nocI3C({ target: __sveltets_2_any(), props: {    "name":`plug`,"size":18,}});}t("Plugins"); }
      if(admin){
         { svelteHTML.createElement("button", { "onclick":() => ((menu = false), (panel = "streamdeck")),}); { const $$_nocI3C = __sveltets_2_ensureComponent(Icon); new $$_nocI3C({ target: __sveltets_2_any(), props: {    "name":`layout-grid`,"size":18,}});}t("Stream Deck plugins"); }
         { svelteHTML.createElement("button", { "onclick":() => ((menu = false), (panel = "devices")),}); { const $$_nocI3C = __sveltets_2_ensureComponent(Icon); new $$_nocI3C({ target: __sveltets_2_any(), props: {    "name":`smartphone`,"size":18,}});}t("Devices"); }
         { svelteHTML.createElement("button", { "onclick":() => ((menu = false), (panel = "review")),}); { const $$_nocI3C = __sveltets_2_ensureComponent(Icon); new $$_nocI3C({ target: __sveltets_2_any(), props: {    "name":`shield-check`,"size":18,}});}t("Review imported steps");if(reviewCount){ { svelteHTML.createElement("span", { "class":`count`,});reviewCount; }} }
         { svelteHTML.createElement("button", { "onclick":() => ((menu = false), (panel = "settings")),}); { const $$_nocI3C = __sveltets_2_ensureComponent(Icon); new $$_nocI3C({ target: __sveltets_2_any(), props: {    "name":`settings-2`,"size":18,}});}t("Settings"); }
      }
      if(ed.dirty){ { svelteHTML.createElement("button", {   "class":`danger`,"onclick":() => ((menu = false), discard()),}); { const $$_nocI3C = __sveltets_2_ensureComponent(Icon); new $$_nocI3C({ target: __sveltets_2_any(), props: {    "name":`undo-2`,"size":18,}});}t("Discard draft"); }}
     }
  }

  if(recording){ { svelteHTML.createElement("div", {   "class":`banner rec`,"role":`status`,}); { const $$_nocI2C = __sveltets_2_ensureComponent(Icon); new $$_nocI2C({ target: __sveltets_2_any(), props: {    "name":`circle-dot`,"size":18,}});}t("Recording: press keys on any device. Stop to turn them into one key."); { svelteHTML.createElement("button", {   "class":`btn small`,"onclick":toggleRecord,});t("Stop"); } }}
  if(ed.error){
     { svelteHTML.createElement("div", {   "class":`banner err`,"role":`alert`,});
      ed.error;
      if(ed.conflict){ { svelteHTML.createElement("button", {   "class":`btn small`,"onclick":() => load(false),});t("Reload"); }}
       { svelteHTML.createElement("button", {     "class":`btn icon ghost`,"aria-label":t("Dismiss"),"onclick":() => ((ed.error = ""), (ed.conflict = false)),}); { const $$_nocI3C = __sveltets_2_ensureComponent(Icon); new $$_nocI3C({ target: __sveltets_2_any(), props: {    "name":`x`,"size":18,}});} }
     }
  }
  if(reviewCount && admin){ { svelteHTML.createElement("div", {   "class":`banner warn`,"role":`status`,}); { const $$_nocI2C = __sveltets_2_ensureComponent(Icon); new $$_nocI2C({ target: __sveltets_2_any(), props: {    "name":`shield`,"size":18,}});}t("{n} imported steps wait for your review.", { n: reviewCount }); { svelteHTML.createElement("button", {   "class":`btn small`,"onclick":() => (panel = "review"),});t("Review"); } }}

   { svelteHTML.createElement("div", {  "class":`panes`,});paletteOpen;
     { svelteHTML.createElement("div", { "class":`left`,});
       { const $$_ettelaP3C = __sveltets_2_ensureComponent(Palette); new $$_ettelaP3C({ target: __sveltets_2_any(), props: { admin,}});}
       { svelteHTML.createElement("footer", { "class":`serving`,});
        if(admin){ { svelteHTML.createElement("span", {});t("Companion on"); } { svelteHTML.createElement("span", { "class":`mono`,});deck.companion?.name; }}
       }
     }
    if(loaded && ed.layout){
       { const $$_savnaC2C = __sveltets_2_ensureComponent(Canvas); new $$_savnaC2C({ target: __sveltets_2_any(), props: {}});}
    }else{
       { svelteHTML.createElement("div", { "class":`loading`,});t("Loading layout…"); }
    }
     { svelteHTML.createElement("div", { "class":`right`,});
      if(loaded){ { const $$_rotcepsnI3C = __sveltets_2_ensureComponent(Inspector); new $$_rotcepsnI3C({ target: __sveltets_2_any(), props: { admin,}});}}
      if(admin && loaded){ { const $$_lenaPriaP3C = __sveltets_2_ensureComponent(PairPanel); new $$_lenaPriaP3C({ target: __sveltets_2_any(), props: {  "onmanage":() => (panel = "devices"),}});}}
     }
   }
 }

 { const $$_teehS0C = __sveltets_2_ensureComponent(Sheet); new $$_teehS0C({ target: __sveltets_2_any(), props: {      children:() => { return __sveltets_2_any(0); },"open":panel === "plugins","onclose":() => (panel = null),"title":t("Plugins"),"wide":true,}}); { const $$_snigulP1C = __sveltets_2_ensureComponent(Plugins); new $$_snigulP1C({ target: __sveltets_2_any(), props: { admin,}});} Sheet}
 { const $$_teehS0C = __sveltets_2_ensureComponent(Sheet); new $$_teehS0C({ target: __sveltets_2_any(), props: {      children:() => { return __sveltets_2_any(0); },"open":panel === "settings","onclose":() => (panel = null),"title":t("Settings"),"wide":true,}}); { const $$_sgnitteS1C = __sveltets_2_ensureComponent(Settings); new $$_sgnitteS1C({ target: __sveltets_2_any(), props: {}});} Sheet}
 { const $$_teehS0C = __sveltets_2_ensureComponent(Sheet); new $$_teehS0C({ target: __sveltets_2_any(), props: {      children:() => { return __sveltets_2_any(0); },"open":panel === "review","onclose":() => ((panel = null), refreshMeta()),"title":t("Review imported steps"),"wide":true,}}); { const $$_weiveR1C = __sveltets_2_ensureComponent(Review); new $$_weiveR1C({ target: __sveltets_2_any(), props: {  "onapproved":() => ((panel = null), load(false), refreshMeta()),}});} Sheet}
 { const $$_teehS0C = __sveltets_2_ensureComponent(Sheet); new $$_teehS0C({ target: __sveltets_2_any(), props: {      children:() => { return __sveltets_2_any(0); },"open":panel === "share","onclose":() => (panel = null),"title":t("Share and gallery"),"wide":true,}}); { const $$_erahS1C = __sveltets_2_ensureComponent(Share); new $$_erahS1C({ target: __sveltets_2_any(), props: {   admin,"onimported":() => (panel = null),}});} Sheet}
 { const $$_teehS0C = __sveltets_2_ensureComponent(Sheet); new $$_teehS0C({ target: __sveltets_2_any(), props: {      children:() => { return __sveltets_2_any(0); },"open":panel === "streamdeck","onclose":() => (panel = null),"title":t("Stream Deck plugins"),"wide":true,}}); { const $$_kceDmaertS1C = __sveltets_2_ensureComponent(StreamDeck); new $$_kceDmaertS1C({ target: __sveltets_2_any(), props: {}});} Sheet}
 { const $$_teehS0C = __sveltets_2_ensureComponent(Sheet); new $$_teehS0C({ target: __sveltets_2_any(), props: {      children:() => { return __sveltets_2_any(0); },"open":panel === "devices","onclose":() => (panel = null),"title":t("Devices"),"wide":true,}}); { const $$_lenaPriaP1C = __sveltets_2_ensureComponent(PairPanel); new $$_lenaPriaP1C({ target: __sveltets_2_any(), props: {}});} Sheet}
 { const $$_teehS0C = __sveltets_2_ensureComponent(Sheet); new $$_teehS0C({ target: __sveltets_2_any(), props: {      children:() => { return __sveltets_2_any(0); },"open":panel === "profiles","onclose":() => (panel = null),"title":t("Layouts"),"wide":true,}}); { const $$_seliforP1C = __sveltets_2_ensureComponent(Profiles); new $$_seliforP1C({ target: __sveltets_2_any(), props: {}});} Sheet}


};
return { props: {} as any as $$ComponentProps, exports: {}, bindings: __sveltets_$$bindings(''), slots: {}, events: {} }}
const Editor__SvelteComponent_ = __sveltets_2_fn_component($$render());
/*Ωignore_startΩ*/type Editor__SvelteComponent_ = ReturnType<typeof Editor__SvelteComponent_>;
/*Ωignore_endΩ*/export default Editor__SvelteComponent_;
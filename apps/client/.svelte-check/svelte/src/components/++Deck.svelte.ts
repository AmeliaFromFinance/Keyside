///<reference types="svelte" />
;
import Grid from "./Grid.svelte";
import Icon from "./Icon.svelte";
import PinPad from "./PinPad.svelte";
import Sheet from "./Sheet.svelte";
import { deck, nav, press, slide, sendViewport, connection } from "../lib/deck.svelte.ts";
import { t, lang, setLang, languages, type Lang } from "../lib/i18n.svelte.ts";
import { ago, clock, everySecond } from "../lib/format.ts";
import { keepAwake, haptic } from "../lib/native.ts";

;type $$ComponentProps =  {
    onedit?: () => void;
    onswitch?: () => void;
    onforget?: () => void;
    canEdit?: boolean;
  };function $$render() {

  
  
  
  
  
  
  
  

  let {
    onedit,
    onswitch,
    onforget,
    canEdit = false,
  }:/*Ωignore_startΩ*/$$ComponentProps/*Ωignore_endΩ*/ = $props();

  let localLock = $state(false);
  let settings = $state(false);
  let now = $state(Date.now());
  let lockTimer: ReturnType<typeof setTimeout> | null = null;
  let lockArming = $state(false);
  let awake = $state(readAwake());

  function readAwake() {
    try {
      return localStorage.getItem("awake") !== "0";
    } catch {
      return true;
    }
  }

  $effect(() => everySecond((n) => (now = n)));

  $effect(() => {
    void keepAwake(awake);
    try {
      localStorage.setItem("awake", awake ? "1" : "0");
    } catch {
      /* storage blocked */
    }
    const vis = () => document.visibilityState === "visible" && (void keepAwake(awake), connection()?.nudge());
    document.addEventListener("visibilitychange", vis);
    return () => document.removeEventListener("visibilitychange", vis);
  });

  $effect(() => {
    const onResize = () => sendViewport();
    addEventListener("resize", onResize);
    return () => removeEventListener("resize", onResize);
  });

  function onKey(e: KeyboardEvent) {
    if (settings || e.target instanceof HTMLInputElement) return;
    if (e.key === "ArrowRight" || e.key === "PageDown") nav("next");
    if (e.key === "ArrowLeft" || e.key === "PageUp") nav("prev");
    if (e.key === "Escape" || e.key === "Backspace") nav("back");
  }

  // Locking needs a long press so a pocket tap can't change it.
  function lockDown() {
    lockArming = true;
    lockTimer = setTimeout(() => {
      localLock = !localLock;
      lockArming = false;
      haptic("medium");
    }, 600);
  }
  function lockUp() {
    lockArming = false;
    if (lockTimer) clearTimeout(lockTimer);
  }

  const page = $derived(deck.page);
  const online = $derived(deck.conn.s === "open");
  const subtitle = $derived(page ? (page.folder ? page.subtitle ?? deck.profile?.name ?? "" : [t("Page {n} of {count}", { n: page.index + 1, count: page.count }), page.subtitle].filter(Boolean).join(" · ")) : "");
;
async () => {

 { svelteHTML.createElement("svelte:window", {  "onkeydown":onKey,});}

 { svelteHTML.createElement("main", { "class":`deck`,});
  if(deck.locked){
     { const $$_daPniP1C = __sveltets_2_ensureComponent(PinPad); new $$_daPniP1C({ target: __sveltets_2_any(), props: {}});}
  }else{
     { svelteHTML.createElement("header", {});
       { svelteHTML.createElement("div", { "class":`title`,});
        if(page?.folder){
           { svelteHTML.createElement("button", {     "class":`back`,"onclick":() => nav("back"),"aria-label":t("Back"),}); { const $$_nocI4C = __sveltets_2_ensureComponent(Icon); new $$_nocI4C({ target: __sveltets_2_any(), props: {  "name":`arrow-left`,}});} }
        }
         { svelteHTML.createElement("div", {});
           { svelteHTML.createElement("h1", {});page?.name ?? deck.profile?.name ?? "PhoneDeck"; }
           { svelteHTML.createElement("p", {});subtitle; }
         }
       }
       { svelteHTML.createElement("div", { "class":`tools`,});
         { svelteHTML.createElement("button", {                  "class":`btn icon tool`,"aria-pressed":localLock,"aria-label":localLock ? t("Hold to unlock keys") : t("Hold to lock keys"),"title":localLock ? t("Hold to unlock keys") : t("Hold to lock keys"),"onpointerdown":lockDown,"onpointerup":lockUp,"onpointerleave":lockUp,"onpointercancel":lockUp,});localLock;lockArming; { const $$_nocI4C = __sveltets_2_ensureComponent(Icon); new $$_nocI4C({ target: __sveltets_2_any(), props: {  "name":localLock ? "lock" : "lock-open",}});} }
         { svelteHTML.createElement("button", {     "class":`btn icon tool`,"aria-label":t("Settings"),"onclick":() => (settings = true),}); { const $$_nocI4C = __sveltets_2_ensureComponent(Icon); new $$_nocI4C({ target: __sveltets_2_any(), props: {  "name":`sliders-horizontal`,}});} }
       }
     }

     { svelteHTML.createElement("section", {   "class":`status`,"aria-live":`polite`,});
       { svelteHTML.createElement("div", { "class":`line`,});
         { svelteHTML.createElement("span", {    "class":`dot`,});online;deck.conn.s === "connecting";deck.conn.s === "closed"; }
         { svelteHTML.createElement("span", { "class":`who`,});
          if(online){
            t("Connected to");  { svelteHTML.createElement("span", { "class":`mono name`,});deck.companion?.name; }
          } else if (deck.conn.s === "connecting"){
            t("Connecting…");
          }else{
            t("Offline, retrying");
          }
         }
        if(online){ { svelteHTML.createElement("span", { "class":`mono latency`,});deck.latency;  }}
       }
      if(deck.connections.length || deck.pills.length){
         { svelteHTML.createElement("div", { "class":`chips`,});
             for(let c of __sveltets_2_ensureArray(deck.connections)){c.id;
             { svelteHTML.createElement("span", {   "class":`chip`,"title":c.detail,}); { svelteHTML.createElement("span", {    "class":`dot`,});c.status === "connected";c.status === "connecting";c.status === "error"; }c.name; }
          }
             for(let p of __sveltets_2_ensureArray(deck.pills)){p.id;
             { svelteHTML.createElement("span", { "class":`chip pill tone-${p.tone}`,}); { svelteHTML.createElement("span", { "class":`dot`,}); } { svelteHTML.createElement("span", {});p.label; }if(p.since){ { svelteHTML.createElement("span", { "class":`mono`,});clock(now - p.since); }} }
          }
         }
      }
     }

    if(page){
       { const $$_dirG1C = __sveltets_2_ensureComponent(Grid); new $$_dirG1C({ target: __sveltets_2_any(), props: {               "cols":page.cols,"rows":page.rows,"keys":page.keys,"locked":localLock || !online,"ondown":(id) => press(id, "down"),"onup":(id, cancel) => press(id, cancel ? "cancel" : "up"),"onslide":slide,}});}
    }else{
       { svelteHTML.createElement("div", { "class":`empty`,});
        if(online){
           { svelteHTML.createElement("p", {});t("No pages to show. Add one in the editor on your PC."); }
        }else{
           { svelteHTML.createElement("p", {});t("Looking for your PC…"); }
        }
       }
    }

    if(page && !page.folder && page.count > 1){
       { svelteHTML.createElement("nav", {   "class":`pager`,"aria-label":t("Pages"),});
         { svelteHTML.createElement("button", {       "class":`btn icon ghost`,"aria-label":t("Previous page"),"disabled":page.index === 0,"onclick":() => nav("prev"),}); { const $$_nocI3C = __sveltets_2_ensureComponent(Icon); new $$_nocI3C({ target: __sveltets_2_any(), props: {  "name":`chevron-left`,}});} }
         { svelteHTML.createElement("div", { "class":`dots`,});
              for(let _ of __sveltets_2_ensureArray({ length: page.count })){let i = 1;i;
             { svelteHTML.createElement("span", { });i === page.index; }
          }
         }
         { svelteHTML.createElement("button", {       "class":`btn icon ghost`,"aria-label":t("Next page"),"disabled":page.index === page.count - 1,"onclick":() => nav("next"),}); { const $$_nocI3C = __sveltets_2_ensureComponent(Icon); new $$_nocI3C({ target: __sveltets_2_any(), props: {  "name":`chevron-right`,}});} }
       }
    }

     { svelteHTML.createElement("footer", {    "class":`toast`,"aria-live":`polite`,});!!deck.toast;
      if(deck.toast){
         { svelteHTML.createElement("span", { "class":`ticon tone-${deck.toast.tone}`,});
           { const $$_nocI3C = __sveltets_2_ensureComponent(Icon); new $$_nocI3C({ target: __sveltets_2_any(), props: {  "name":deck.toast.tone === "danger" ? "circle-alert" : deck.toast.tone === "warning" ? "triangle-alert" : "check",}});}
         }
         { svelteHTML.createElement("span", { "class":`text`,});deck.toast.text; }
         { svelteHTML.createElement("span", { "class":`mono when`,});t("{t} ago", { t: ago(deck.toast.at, now) }); }
      }
     }
  }
 }

 { const $$_teehS0C = __sveltets_2_ensureComponent(Sheet); const $$_teehS0 = new $$_teehS0C({ target: __sveltets_2_any(), props: {    children:() => { return __sveltets_2_any(0); },open:settings,"title":t("Settings"),}});/*Ωignore_startΩ*/() => settings = __sveltets_2_any(null);/*Ωignore_endΩ*/$$_teehS0.$$bindings = 'open';
   { svelteHTML.createElement("div", { "class":`about`,});
     { svelteHTML.createElement("p", {}); { svelteHTML.createElement("b", {});deck.companion?.name; }  { svelteHTML.createElement("span", { "class":`mono`,}); deck.companion?.version; } }
     { svelteHTML.createElement("p", {});t("This device"); deck.self?.name; deck.self?.scope === "editor" ? t("can edit") : deck.self?.scope === "moderator" ? t("moderator") : t("deck only");  }
   }
   { svelteHTML.createElement("label", { "class":`field`,});
     { svelteHTML.createElement("span", {});t("Language"); }
     { svelteHTML.createElement("select", {   "value":lang(),"onchange":(e) => setLang(e.currentTarget.value as Lang),});
         for(let [code, name] of __sveltets_2_ensureArray(Object.entries(languages))){code; { svelteHTML.createElement("option", { "value":code,});name; }}
     }
   }
   { svelteHTML.createElement("label", { "class":`check`,}); { svelteHTML.createElement("input", {    "type":`checkbox`,"bind:checked":awake,});/*Ωignore_startΩ*/() => awake = __sveltets_2_any(null);/*Ωignore_endΩ*/} t("Keep the screen on"); }
  if(canEdit && onedit){
     { svelteHTML.createElement("button", {   "class":`btn`,"onclick":() => ((settings = false), onedit()),}); { const $$_nocI2C = __sveltets_2_ensureComponent(Icon); new $$_nocI2C({ target: __sveltets_2_any(), props: {  "name":`pen`,}});}t("Edit layout"); }
  }
  if(onswitch){
     { svelteHTML.createElement("button", {   "class":`btn`,"onclick":() => ((settings = false), onswitch()),}); { const $$_nocI2C = __sveltets_2_ensureComponent(Icon); new $$_nocI2C({ target: __sveltets_2_any(), props: {  "name":`monitor`,}});}t("Other PCs"); }
  }
  if(onforget){
     { svelteHTML.createElement("button", {   "class":`btn danger`,"onclick":() => ((settings = false), onforget()),}); { const $$_nocI2C = __sveltets_2_ensureComponent(Icon); new $$_nocI2C({ target: __sveltets_2_any(), props: {  "name":`log-out`,}});}t("Forget this PC"); }
  }
 Sheet}


};
return { props: {} as any as $$ComponentProps, exports: {}, bindings: __sveltets_$$bindings(''), slots: {}, events: {} }}
const Deck__SvelteComponent_ = __sveltets_2_fn_component($$render());
/*Ωignore_startΩ*/type Deck__SvelteComponent_ = ReturnType<typeof Deck__SvelteComponent_>;
/*Ωignore_endΩ*/export default Deck__SvelteComponent_;
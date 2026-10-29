///<reference types="svelte" />
;
import Icon from "./Icon.svelte";
import Scanner from "./Scanner.svelte";
import { parsePairLink, type PairParams, type StoredCompanion } from "../lib/companions.ts";
import { connect, deck } from "../lib/deck.svelte.ts";
import { deviceKind, deviceName, nativeScan, platform } from "../lib/native.ts";
import { t, mark } from "../lib/i18n.svelte.ts";

;type $$ComponentProps =  {
    params?: PairParams | null;
    saved?: StoredCompanion[];
    onpaired: (c: StoredCompanion) => void;
    onopen: (c: StoredCompanion) => void;
  };function $$render() {

  
  
  
  
  
  

  let { params = null, saved = [], onpaired, onopen }:/*Ωignore_startΩ*/$$ComponentProps/*Ωignore_endΩ*/ = $props();

  // Starts from the link that opened the page; later changes come from scanning.
  // svelte-ignore state_referenced_locally
  let pending = $state<PairParams | null>(params);
  let scanning = $state(false);
  let link = $state("");
  let name = $state(deviceName());
  let started = $state(false);
  let problem = $state("");
  let problemVars = $state<Record<string, string | number>>({});

  const reasons: Record<string, string> = {
    "pairing-expired": mark("This code expired or was already used. Make a new one on your PC."),
    "wrong-companion": mark("This code belongs to a different PC."),
    "bad-proof": mark("The code did not match. Scan it again."),
    denied: mark("Pairing was turned down on the PC."),
    unreachable: mark("Can't reach your PC at {addresses}. Check that both are on the same network and that the PC's firewall allows port {port}."),
  };

  $effect(() => {
    if (started && deck.conn.s === "closed") {
      problem = reasons[deck.conn.reason] ?? mark("Pairing stopped. Try again.");
      problemVars = pending ? { addresses: pending.hosts.join(", "), port: pending.port } : {};
      started = false;
    }
  });

  async function scan() {
    problem = "";
    const native = await nativeScan();
    if (native === undefined) scanning = true;
    else if (native) use(native);
  }

  function use(text: string) {
    scanning = false;
    const p = parsePairLink(text.trim());
    if (!p) problem = mark("That is not a Keyside pairing code.");
    else pending = p;
  }

  function pair() {
    if (!pending) return;
    problem = "";
    started = true;
    const app = platform() === "web" ? "web" : platform();
    connect({ kind: "pair", params: pending, device: { name: name.trim().slice(0, 40) || deviceName(), kind: deviceKind(), app } }, onpaired);
  }
;
async () => {

 { svelteHTML.createElement("main", { "class":`pair`,});
   { svelteHTML.createElement("div", {   "class":`mark`,"aria-hidden":`true`,}); { const $$_nocI2C = __sveltets_2_ensureComponent(Icon); new $$_nocI2C({ target: __sveltets_2_any(), props: {    "name":`layout-grid`,"size":36,}});} }

  if(started && deck.conn.s === "waiting"){
     { svelteHTML.createElement("h1", {});t("Approve on your PC"); }
     { svelteHTML.createElement("p", {});t("Check that your PC shows this code, then approve this device there."); }
     { svelteHTML.createElement("div", {   "class":`sas mono`,"aria-label":t("Code {code}", { code: deck.sas }),});deck.sas.slice(0, 3); deck.sas.slice(3); }
     { svelteHTML.createElement("p", { "class":`hint`,});t("Waiting for approval…"); }
  } else if (started){
     { svelteHTML.createElement("h1", {});t("Connecting…"); }
     { svelteHTML.createElement("p", {});t("Pairing with {name}", { name: pending?.name ?? "" }); }
  } else if (scanning){
     { svelteHTML.createElement("h1", {});t("Scan the code"); }
     { const $$_rennacS1C = __sveltets_2_ensureComponent(Scanner); new $$_rennacS1C({ target: __sveltets_2_any(), props: {    "onresult":use,"oncancel":() => (scanning = false),}});}
  } else if (pending){
     { svelteHTML.createElement("h1", {});t("Pair with {name}", { name: pending.name }); }
     { svelteHTML.createElement("label", { "class":`field`,});
       { svelteHTML.createElement("span", {});t("Name this device"); }
       { svelteHTML.createElement("input", {      "bind:value":name,"maxlength":40,"autocomplete":`off`,});/*Ωignore_startΩ*/() => name = __sveltets_2_any(null);/*Ωignore_endΩ*/}
     }
     { svelteHTML.createElement("button", {   "class":`btn primary`,"onclick":pair,}); { const $$_nocI2C = __sveltets_2_ensureComponent(Icon); new $$_nocI2C({ target: __sveltets_2_any(), props: {  "name":`link`,}});}t("Pair"); }
     { svelteHTML.createElement("button", {   "class":`btn ghost`,"onclick":() => (pending = null),});t("Use a different code"); }
  }else{
     { svelteHTML.createElement("h1", {});t("Pair with your PC"); }
     { svelteHTML.createElement("ol", { "class":`steps`,});
       { svelteHTML.createElement("li", {});t("Open Keyside on your PC."); }
       { svelteHTML.createElement("li", {});t("Choose Pair a phone."); }
       { svelteHTML.createElement("li", {});t("Scan the code it shows."); }
     }
     { svelteHTML.createElement("button", {   "class":`btn primary`,"onclick":scan,}); { const $$_nocI2C = __sveltets_2_ensureComponent(Icon); new $$_nocI2C({ target: __sveltets_2_any(), props: {  "name":`scan`,}});}t("Scan code"); }
     { svelteHTML.createElement("details", {});
       { svelteHTML.createElement("summary", {});t("Paste the pairing link instead"); }
       { svelteHTML.createElement("div", { "class":`paste`,});
         { svelteHTML.createElement("input", {        "class":`input`,"bind:value":link,"placeholder":`https://…#pair&…`,"autocomplete":`off`,});/*Ωignore_startΩ*/() => link = __sveltets_2_any(null);/*Ωignore_endΩ*/}
         { svelteHTML.createElement("button", {     "class":`btn`,"onclick":() => use(link),"disabled":!link,});t("Continue"); }
       }
     }
  }

  if(problem){ { svelteHTML.createElement("p", {   "class":`problem`,"role":`alert`,});t(problem, problemVars); }}

  if(saved.length && !started){
     { svelteHTML.createElement("section", { "class":`saved`,});
       { svelteHTML.createElement("h2", {});t("Paired PCs"); }
         for(let c of __sveltets_2_ensureArray(saved)){c.cid;
         { svelteHTML.createElement("button", {   "class":`btn`,"onclick":() => onopen(c),}); { const $$_nocI3C = __sveltets_2_ensureComponent(Icon); new $$_nocI3C({ target: __sveltets_2_any(), props: {  "name":`monitor`,}});}c.name; }
      }
     }
  }
 }


};
return { props: {} as any as $$ComponentProps, exports: {}, bindings: __sveltets_$$bindings(''), slots: {}, events: {} }}
const Pair__SvelteComponent_ = __sveltets_2_fn_component($$render());
/*Ωignore_startΩ*/type Pair__SvelteComponent_ = ReturnType<typeof Pair__SvelteComponent_>;
/*Ωignore_endΩ*/export default Pair__SvelteComponent_;
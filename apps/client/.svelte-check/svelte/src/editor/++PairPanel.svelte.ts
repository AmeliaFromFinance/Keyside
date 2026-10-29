///<reference types="svelte" />
;
import QRCode from "qrcode";
import Icon from "../components/Icon.svelte";
import { req, listen } from "../lib/deck.svelte.ts";
import { ed } from "./state.svelte.ts";
import { t } from "../lib/i18n.svelte.ts";
import { ago, everySecond } from "../lib/format.ts";
;

  interface Device {
    id: string;
    name: string;
    kind: string;
    scope: "deck" | "moderator" | "editor";
    pages: string[];
    hasPin: boolean;
    lastSeen: number;
    online: boolean;
  };;
  interface Pending {
    pid: string;
    name: string;
    kind: string;
    sas: string;
    address: string;
  };
;type $$ComponentProps =  { onmanage?: () => void };function $$render() {

  
  
  
  
  
  



  let { onmanage }:/*Ωignore_startΩ*/$$ComponentProps/*Ωignore_endΩ*/ = $props();

  let qr = $state("");
  let url = $state("");
  let fingerprint = $state("");
  let expires = $state(0);
  let now = $state(Date.now());
  let requirePin = $state(true);
  let pin = $state(randomPin());
  let scope = $state<"deck" | "editor" | "moderator">("deck");
  let modPages = $state<string[]>([]);
  let devices = $state<Device[]>([]);
  let pending = $state<Pending[]>([]);
  let error = $state("");

  function randomPin() {
    return String(crypto.getRandomValues(new Uint32Array(1))[0] % 10000).padStart(4, "0");
  }

  async function refresh() {
    try {
      [devices, pending] = await Promise.all([req<Device[]>("devices.list"), req<Pending[]>("pending.list")]);
    } catch {
      /* reconnecting */
    }
  }

  $effect(() => {
    void refresh();
    const off = listen((m) => m.t === "event" && m.name === "admin.changed" && void refresh());
    const tick = everySecond((n) => (now = n));
    return () => (off(), tick());
  });

  async function start() {
    error = "";
    try {
      const pages = scope === "moderator" ? modPages.map((p) => `${ed.profile}/${p}`) : [];
      const p = await req<{ url: string; expiresIn: number; fingerprint: string }>("pairing.start", { scope, pages, pin: requirePin ? pin : null });
      url = p.url;
      fingerprint = p.fingerprint;
      expires = Date.now() + p.expiresIn * 1000;
      qr = await QRCode.toString(p.url, { type: "svg", margin: 1, errorCorrectionLevel: "M", color: { dark: "#121118", light: "#edebf2" } });
    } catch (e: any) {
      error = e.message;
    }
  }

  async function answer(pid: string, approve: boolean) {
    await req("pending.answer", { pid, approve }).catch((e) => (error = e.message));
    if (approve) {
      qr = "";
      pin = randomPin();
    }
    void refresh();
  }

  async function revoke(id: string) {
    await req("devices.revoke", { id });
    void refresh();
  }

  const left = $derived(Math.max(0, Math.round((expires - now) / 1000)));
  const pages = $derived(ed.layout?.profiles.find((p) => p.id === ed.profile)?.pages.filter((p) => !p.folder) ?? []);
;
async () => {

 { svelteHTML.createElement("section", { "class":`pair`,});
   { svelteHTML.createElement("header", {});
     { svelteHTML.createElement("h2", {});t("Pair a phone"); }
    if(qr && left > 0){ { svelteHTML.createElement("span", { "class":`exp`,});t("Code expires in");  { svelteHTML.createElement("b", { "class":`mono`,});Math.floor(left / 60); String(left % 60).padStart(2, "0"); } }}
   }

     for(let p of __sveltets_2_ensureArray(pending)){p.pid;
     { svelteHTML.createElement("div", {   "class":`pending`,"role":`alert`,});
       { svelteHTML.createElement("p", {}); { svelteHTML.createElement("b", {});p.name; } t("wants to pair");  { svelteHTML.createElement("span", { "class":`mono addr`,});p.address; } }
       { svelteHTML.createElement("p", {});t("Check the phone shows");  { svelteHTML.createElement("b", { "class":`sas mono`,});p.sas.slice(0, 3); p.sas.slice(3); } }
       { svelteHTML.createElement("div", { "class":`row`,});
         { svelteHTML.createElement("button", {   "class":`btn primary`,"onclick":() => answer(p.pid, true),}); { const $$_nocI4C = __sveltets_2_ensureComponent(Icon); new $$_nocI4C({ target: __sveltets_2_any(), props: {    "name":`check`,"size":18,}});}t("Approve"); }
         { svelteHTML.createElement("button", {   "class":`btn danger`,"onclick":() => answer(p.pid, false),});t("Deny"); }
       }
     }
  }

  if(qr && left > 0){
     { svelteHTML.createElement("div", { "class":`code`,});
       { svelteHTML.createElement("div", {   "class":`qr`,"aria-label":t("Pairing QR code"),}); qr; }
       { svelteHTML.createElement("div", { "class":`info`,});
         { svelteHTML.createElement("p", {});t("Scan with the phone camera, then approve the phone here. Single use."); }
        if(fingerprint){
           { svelteHTML.createElement("p", { "class":`fp`,});t("Certificate (browsers show this)"); { svelteHTML.createElement("br", {});} { svelteHTML.createElement("span", { "class":`mono`,});fingerprint.slice(0, 11);  fingerprint.slice(-8); } }
        }
        if(requirePin){ { svelteHTML.createElement("p", {});t("PIN for this phone");  { svelteHTML.createElement("b", { "class":`mono`,});pin; } }}
         { svelteHTML.createElement("button", {   "class":`btn ghost small`,"onclick":() => navigator.clipboard?.writeText(url),}); { const $$_nocI4C = __sveltets_2_ensureComponent(Icon); new $$_nocI4C({ target: __sveltets_2_any(), props: {    "name":`copy`,"size":16,}});}t("Copy link"); }
         { svelteHTML.createElement("a", {       "class":`link mono`,"href":url,"target":`_blank`,"rel":`noreferrer`,});t("Open link"); }
       }
     }
  }else{
     { svelteHTML.createElement("div", { "class":`setup`,});
       { svelteHTML.createElement("label", { "class":`field`,});
         { svelteHTML.createElement("span", {});t("Access"); }
         { svelteHTML.createElement("select", { "bind:value":scope,});/*Ωignore_startΩ*/() => scope = __sveltets_2_any(null);/*Ωignore_endΩ*/
           { svelteHTML.createElement("option", { "value":`deck`,});t("Use the deck"); }
           { svelteHTML.createElement("option", { "value":`editor`,});t("Use and edit the deck"); }
           { svelteHTML.createElement("option", { "value":`moderator`,});t("Moderator: chosen pages only"); }
         }
       }
      if(scope === "moderator"){
         { svelteHTML.createElement("div", { "class":`mods`,});
             for(let p of __sveltets_2_ensureArray(pages)){p.id;
             { svelteHTML.createElement("label", { "class":`check`,}); { svelteHTML.createElement("input", {     "type":`checkbox`,"value":p.id,});modPages = __sveltets_2_any(null);}p.name; }
          }
         }
      }
       { svelteHTML.createElement("label", { "class":`check`,}); { svelteHTML.createElement("input", {    "type":`checkbox`,"bind:checked":requirePin,});/*Ωignore_startΩ*/() => requirePin = __sveltets_2_any(null);/*Ωignore_endΩ*/}t("Require a PIN on this phone"); }
      if(requirePin){
         { svelteHTML.createElement("label", { "class":`field`,}); { svelteHTML.createElement("span", {});  } { svelteHTML.createElement("input", {          "class":`mono`,"bind:value":pin,"inputmode":`numeric`,"maxlength":12,"pattern":`[0-9]*`,});/*Ωignore_startΩ*/() => pin = __sveltets_2_any(null);/*Ωignore_endΩ*/} }
      }
       { svelteHTML.createElement("button", {     "class":`btn primary`,"onclick":start,"disabled":(requirePin && !/^\d{4,12}$/.test(pin)) || (scope === "moderator" && !modPages.length),});
         { const $$_nocI3C = __sveltets_2_ensureComponent(Icon); new $$_nocI3C({ target: __sveltets_2_any(), props: {    "name":`qr-code`,"size":18,}});}t("Show pairing code");
       }
     }
  }
  if(error){ { svelteHTML.createElement("p", { "class":`bad`,});error; }}

   { svelteHTML.createElement("h3", {});t("Paired devices");  devices.length; }
     for(let d of __sveltets_2_ensureArray(devices)){d.id;
     { svelteHTML.createElement("div", { "class":`dev`,});
       { svelteHTML.createElement("span", {  "class":`dot`,});d.online; }
       { svelteHTML.createElement("span", { "class":`name`,});d.name; }
       { svelteHTML.createElement("span", { "class":`meta`,});
        d.online ? t("online") : t("seen {t} ago", { t: ago(d.lastSeen * 1000, now) });d.hasPin ? " · PIN" : "";d.scope !== "deck" ? ` · ${d.scope === "editor" ? t("editor") : t("moderator")}` : "";
       }
       { svelteHTML.createElement("button", {   "class":`btn danger small`,"onclick":() => revoke(d.id),});t("Revoke"); }
     }
  }
     { svelteHTML.createElement("p", { "class":`muted`,});t("No devices yet."); }
  
  if(onmanage && devices.length){ { svelteHTML.createElement("button", {   "class":`btn ghost small`,"onclick":onmanage,});t("Manage devices"); }}
 }


};
return { props: {} as any as $$ComponentProps, exports: {}, bindings: __sveltets_$$bindings(''), slots: {}, events: {} }}
const PairPanel__SvelteComponent_ = __sveltets_2_fn_component($$render());
/*Ωignore_startΩ*/type PairPanel__SvelteComponent_ = ReturnType<typeof PairPanel__SvelteComponent_>;
/*Ωignore_endΩ*/export default PairPanel__SvelteComponent_;
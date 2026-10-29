///<reference types="svelte" />
;
import Icon from "../components/Icon.svelte";
import { req } from "../lib/deck.svelte.ts";
import { t, lang, setLang, languages, type Lang } from "../lib/i18n.svelte.ts";
function $$render() {

  
  
  

  interface Info {
    name: string;
    version: string;
    addresses: string[];
    lanPort: number;
    adminPort: number;
    fingerprint: string;
    deno: boolean;
  }
  interface Token {
    id: string;
    name: string;
    scopes: string[];
    created: number;
    lastUsed: number;
  }

  let s = $state<Record<string, any> | null>(null);
  let info = $state<Info | null>(null);
  let tokens = $state<Token[]>([]);
  let newName = $state("Streamer.bot");
  let newScopes = $state<string[]>(["press", "variables"]);
  let secret = $state("");
  let msg = $state("");
  let error = $state("");

  async function refresh() {
    [s, info, tokens] = await Promise.all([req("settings.get"), req<Info>("companion.info"), req<Token[]>("tokens.list")]);
  }
  $effect(() => void refresh().catch((e) => (error = e.message)));

  async function save() {
    error = "";
    try {
      await req("settings.set", $state.snapshot(s));
      msg = t("Saved. Restart PhoneDeck for network changes to apply.");
    } catch (e: any) {
      error = e.message;
    }
  }

  async function create() {
    const r = await req<{ secret: string }>("tokens.create", { name: newName, scopes: newScopes });
    secret = r.secret;
    tokens = await req("tokens.list");
  }

  const tailscale = $derived((info?.addresses ?? []).filter((a) => /^100\.(6[4-9]|[7-9]\d|1[01]\d|12[0-7])\./.test(a)));
;
async () => {

if(error){ { svelteHTML.createElement("p", {   "class":`bad`,"role":`alert`,});error; }}

if(info){
   { svelteHTML.createElement("section", { "class":`card`,});
     { svelteHTML.createElement("h3", {});t("This PC"); }
     { svelteHTML.createElement("p", {});  { svelteHTML.createElement("span", { "class":`mono`,});info.version; } }
     { svelteHTML.createElement("p", {});t("Phones connect to");    for(let a of __sveltets_2_ensureArray(info.addresses)){a; { svelteHTML.createElement("span", { "class":`mono addr`,});a; info.lanPort; }} { svelteHTML.createElement("span", { "class":`bad`,});t("no network found"); } }
     { svelteHTML.createElement("p", { "class":`muted`,});t("Certificate");  { svelteHTML.createElement("span", { "class":`mono`,});info.fingerprint; } }
    if(!info.deno){ { svelteHTML.createElement("p", { "class":`bad`,}); { const $$_nocI2C = __sveltets_2_ensureComponent(Icon); new $$_nocI2C({ target: __sveltets_2_any(), props: {    "name":`triangle-alert`,"size":18,}});}t("The plugin runtime is missing, so plugins can't run. Reinstall PhoneDeck."); }}
    if(tailscale.length){
       { svelteHTML.createElement("p", { "class":`ok`,}); { const $$_nocI2C = __sveltets_2_ensureComponent(Icon); new $$_nocI2C({ target: __sveltets_2_any(), props: {    "name":`globe`,"size":18,}});}t("Tailscale is on. Paired phones on your tailnet can reach this PC from anywhere."); }
    }else{
       { svelteHTML.createElement("p", { "class":`muted`,});t("To use your deck away from home, install Tailscale on this PC and your phone. See docs/remote-access.md."); }
    }
   }
}

if(s){
   { svelteHTML.createElement("section", { "class":`card`,});
     { svelteHTML.createElement("h3", {});t("General"); }
     { svelteHTML.createElement("label", { "class":`field`,}); { svelteHTML.createElement("span", {});t("Name shown on devices"); } { svelteHTML.createElement("input", {    "bind:value":s.name,"maxlength":40,});/*Ωignore_startΩ*/() => s.name = __sveltets_2_any(null);/*Ωignore_endΩ*/} }
     { svelteHTML.createElement("label", { "class":`field`,});
       { svelteHTML.createElement("span", {});t("Editor language"); }
       { svelteHTML.createElement("select", {   "value":lang(),"onchange":(e) => setLang(e.currentTarget.value as Lang),});
           for(let [code, name] of __sveltets_2_ensureArray(Object.entries(languages))){code; { svelteHTML.createElement("option", { "value":code,});name; }}
       }
     }
     { svelteHTML.createElement("label", { "class":`check`,}); { svelteHTML.createElement("input", {    "type":`checkbox`,"bind:checked":s.followFocus,});/*Ωignore_startΩ*/() => s.followFocus = __sveltets_2_any(null);/*Ωignore_endΩ*/}t("Switch layouts to match the app in front"); }
     { svelteHTML.createElement("label", { "class":`check`,}); { svelteHTML.createElement("input", {    "type":`checkbox`,"bind:checked":s.hardwareDecks,});/*Ωignore_startΩ*/() => s.hardwareDecks = __sveltets_2_any(null);/*Ωignore_endΩ*/}t("Use Stream Deck hardware plugged into this PC"); }
   }

   { svelteHTML.createElement("section", { "class":`card`,});
     { svelteHTML.createElement("h3", {});t("Network"); }
     { svelteHTML.createElement("label", { "class":`check`,}); { svelteHTML.createElement("input", {    "type":`checkbox`,"bind:checked":s.lanEnabled,});/*Ωignore_startΩ*/() => s.lanEnabled = __sveltets_2_any(null);/*Ωignore_endΩ*/}t("Let paired devices connect"); }
     { svelteHTML.createElement("label", { "class":`check`,}); { svelteHTML.createElement("input", {    "type":`checkbox`,"bind:checked":s.mdns,});/*Ωignore_startΩ*/() => s.mdns = __sveltets_2_any(null);/*Ωignore_endΩ*/}t("Announce this PC on the local network"); }
     { svelteHTML.createElement("label", { "class":`check`,}); { svelteHTML.createElement("input", {    "type":`checkbox`,"bind:checked":s.apiOnLan,});/*Ωignore_startΩ*/() => s.apiOnLan = __sveltets_2_any(null);/*Ωignore_endΩ*/}t("Allow the HTTP API from other computers (HTTPS, token needed)"); }
     { svelteHTML.createElement("div", { "class":`two`,});
       { svelteHTML.createElement("label", { "class":`field`,}); { svelteHTML.createElement("span", {});t("Device port"); } { svelteHTML.createElement("input", {        "type":`number`,"min":`1024`,"max":`65535`,"bind:value":s.lanPort,});/*Ωignore_startΩ*/() => s.lanPort = __sveltets_2_any(null);/*Ωignore_endΩ*/} }
       { svelteHTML.createElement("label", { "class":`field`,}); { svelteHTML.createElement("span", {});t("Editor port"); } { svelteHTML.createElement("input", {        "type":`number`,"min":`1024`,"max":`65535`,"bind:value":s.adminPort,});/*Ωignore_startΩ*/() => s.adminPort = __sveltets_2_any(null);/*Ωignore_endΩ*/} }
     }
     { svelteHTML.createElement("label", { "class":`field`,}); { svelteHTML.createElement("span", {});t("Listen on"); } { svelteHTML.createElement("input", {    "class":`mono`,"bind:value":s.lanBind,});/*Ωignore_startΩ*/() => s.lanBind = __sveltets_2_any(null);/*Ωignore_endΩ*/} { svelteHTML.createElement("small", {});t("0.0.0.0 means every network. Use one address to limit it, like your Tailscale address."); } }
     { svelteHTML.createElement("label", { "class":`field`,}); { svelteHTML.createElement("span", {});t("Plugin registry"); } { svelteHTML.createElement("input", {    "class":`mono`,"bind:value":s.registryUrl,});/*Ωignore_startΩ*/() => s.registryUrl = __sveltets_2_any(null);/*Ωignore_endΩ*/} }
   }
   { svelteHTML.createElement("div", { "class":`row`,});
     { svelteHTML.createElement("button", {   "class":`btn primary`,"onclick":save,});t("Save settings"); }
    if(msg){ { svelteHTML.createElement("span", { "class":`muted`,});msg; }}
   }
}

 { svelteHTML.createElement("section", { "class":`card`,});
   { svelteHTML.createElement("h3", {});t("API tokens"); }
   { svelteHTML.createElement("p", { "class":`muted`,});t("Let other tools press keys and set variables over HTTP. Each token only gets the permissions you tick."); }
     for(let tk of __sveltets_2_ensureArray(tokens)){tk.id;
     { svelteHTML.createElement("div", { "class":`tok`,});
       { svelteHTML.createElement("span", {}); { svelteHTML.createElement("b", {});tk.name; }  { svelteHTML.createElement("span", { "class":`muted`,});tk.scopes.join(", "); } }
       { svelteHTML.createElement("button", {   "class":`btn danger`,"onclick":async () => (await req("tokens.revoke", { id: tk.id }), (tokens = await req("tokens.list"))),});t("Revoke"); }
     }
  }
  if(secret){
     { svelteHTML.createElement("div", { "class":`secret`,});
       { svelteHTML.createElement("p", {});t("Copy this token now. It won't be shown again."); }
       { svelteHTML.createElement("code", { "class":`mono`,});secret; }
       { svelteHTML.createElement("button", {   "class":`btn`,"onclick":() => navigator.clipboard?.writeText(secret),}); { const $$_nocI3C = __sveltets_2_ensureComponent(Icon); new $$_nocI3C({ target: __sveltets_2_any(), props: {    "name":`copy`,"size":16,}});}t("Copy"); }
     }
  }
   { svelteHTML.createElement("div", { "class":`new`,});
     { svelteHTML.createElement("input", {        "class":`input`,"bind:value":newName,"maxlength":40,"aria-label":t("Token name"),});/*Ωignore_startΩ*/() => newName = __sveltets_2_any(null);/*Ωignore_endΩ*/}
     { svelteHTML.createElement("label", { "class":`check`,}); { svelteHTML.createElement("input", {     "type":`checkbox`,"value":`press`,});newScopes = __sveltets_2_any(null);}t("Press keys"); }
     { svelteHTML.createElement("label", { "class":`check`,}); { svelteHTML.createElement("input", {     "type":`checkbox`,"value":`variables`,});newScopes = __sveltets_2_any(null);}t("Variables"); }
     { svelteHTML.createElement("label", { "class":`check`,}); { svelteHTML.createElement("input", {     "type":`checkbox`,"value":`display`,});newScopes = __sveltets_2_any(null);}t("Change keys and pages"); }
     { svelteHTML.createElement("button", {     "class":`btn`,"onclick":create,"disabled":!newScopes.length,});t("Create token"); }
   }
 }


};
return { props: {} as Record<string, never>, exports: {}, bindings: __sveltets_$$bindings(''), slots: {}, events: {} }}
const Settings__SvelteComponent_ = __sveltets_2_fn_component($$render());
/*Ωignore_startΩ*/type Settings__SvelteComponent_ = ReturnType<typeof Settings__SvelteComponent_>;
/*Ωignore_endΩ*/export default Settings__SvelteComponent_;
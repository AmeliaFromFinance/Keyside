///<reference types="svelte" />
;
import Icon from "../components/Icon.svelte";
import { req } from "../lib/deck.svelte.ts";
import { reloadCatalog } from "./state.svelte.ts";
import { t, mark } from "../lib/i18n.svelte.ts";
function $$render() {

  
  
  
  

  type Runtime = "node" | "native" | "html";
  interface Info {
    uuid: string;
    name: string;
    author: string;
    version: string;
    description: string;
    runtime: Runtime | null;
    enabled: boolean;
    running: boolean;
    blocked: string | null;
    net: string[];
    allNet: boolean;
    unsandboxed: boolean;
    actions: number;
  }
  interface Staged {
    token: string;
    uuid: string;
    name: string;
    author: string;
    version: string;
    description: string;
    runtime: Runtime | null;
    actions: string[];
    update: string | null;
  }

  const runtimeText: Record<Runtime, string> = {
    node: mark("JavaScript, runs in the sandbox"),
    native: mark("Program for this system, runs unsandboxed"),
    html: mark("Web page plugin, can't run here"),
  };

  // Reasons the companion gives for a plugin that can't run, so they get translated.
  mark("This plugin has no version for this system");
  mark("Web-page plugins need Elgato's app and can't run in PhoneDeck");
  mark("Needs your permission to run unsandboxed");
  mark("The plugin runtime (Deno) is missing");

  let list = $state<Info[]>([]);
  let deno = $state(true);
  let staged = $state<Staged | null>(null);
  let trust = $state(false);
  let open = $state<string | null>(null);
  let hosts = $state<Record<string, string>>({});
  let error = $state("");
  let busy = $state(false);

  async function refresh() {
    const r = await req<{ plugins: Info[]; deno: boolean }>("streamdeck.list");
    list = r.plugins;
    deno = r.deno;
  }
  $effect(() => {
    void refresh().catch((e) => (error = e.message));
    const timer = setInterval(() => void refresh().catch(() => {}), 2000);
    return () => clearInterval(timer);
  });

  async function run(fn: () => Promise<unknown>) {
    busy = true;
    error = "";
    try {
      await fn();
      await refresh();
      await reloadCatalog();
    } catch (e: any) {
      error = e.message;
    } finally {
      busy = false;
    }
  }

  async function upload(e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    input.value = "";
    if (!file) return;
    error = "";
    trust = false;
    const r = await fetch("/admin/upload/streamdeck", { method: "POST", headers: { "x-phonedeck": "1" }, body: file });
    if (!r.ok) error = await r.text();
    else staged = await r.json();
  }

  function access(p: Info): "none" | "list" | "all" {
    return p.allNet ? "all" : p.net.length ? "list" : "none";
  }

  function setAccess(p: Info, mode: "none" | "list" | "all") {
    if (mode === "list") {
      hosts[p.uuid] ??= p.net.join("\n");
      open = p.uuid;
      return;
    }
    void run(() => req("streamdeck.grant", { uuid: p.uuid, net: [], allNet: mode === "all" }));
  }

  function saveHosts(p: Info) {
    const net = (hosts[p.uuid] ?? "")
      .split(/[\s,]+/)
      .map((h) => h.trim().replace(/^https?:\/\//, "").replace(/\/.*$/, ""))
      .filter(Boolean);
    void run(() => req("streamdeck.grant", { uuid: p.uuid, net, allNet: false }));
  }
;
async () => {

if(error){ { svelteHTML.createElement("p", {   "class":`bad`,"role":`alert`,});error; }}
if(!deno){ { svelteHTML.createElement("p", { "class":`warn`,}); { const $$_nocI1C = __sveltets_2_ensureComponent(Icon); new $$_nocI1C({ target: __sveltets_2_any(), props: {    "name":`triangle-alert`,"size":18,}});}t("The plugin runtime (Deno) is missing, so JavaScript plugins can't start."); }}

if(staged){
   { svelteHTML.createElement("section", { "class":`staged`,});
     { svelteHTML.createElement("h3", {});staged.update ? t("Update {name} to {v}?", { name: staged.name, v: staged.version }) : t("Install {name} {v}?", { name: staged.name, v: staged.version }); }
     { svelteHTML.createElement("p", { "class":`muted`,});t("By {author}", { author: staged.author || "?" });staged.description ? ` · ${staged.description}` : ""; }
     { svelteHTML.createElement("p", {});t("Actions"); staged.actions.join(", "); }
    if(staged.runtime === "html"){
       { svelteHTML.createElement("p", { "class":`bad`,});t("This is a web page plugin. Those need Elgato's own app and can't run in PhoneDeck."); }
    } else if (staged.runtime === "native"){
       { svelteHTML.createElement("p", { "class":`bad`,}); { const $$_nocI2C = __sveltets_2_ensureComponent(Icon); new $$_nocI2C({ target: __sveltets_2_any(), props: {    "name":`shield-off`,"size":18,}});}t("This plugin is a program, not a script. PhoneDeck can't sandbox it: it can do anything you can do on this PC."); }
       { svelteHTML.createElement("label", { "class":`check`,}); { svelteHTML.createElement("input", {    "type":`checkbox`,"bind:checked":trust,});/*Ωignore_startΩ*/() => trust = __sveltets_2_any(null);/*Ωignore_endΩ*/}t("I trust where this plugin came from and want to run it anyway"); }
    }else{
       { svelteHTML.createElement("p", {}); { const $$_nocI2C = __sveltets_2_ensureComponent(Icon); new $$_nocI2C({ target: __sveltets_2_any(), props: {    "name":`shield-check`,"size":18,}});}t("Runs in the sandbox: it can't read your files, start programs or reach the internet unless you allow it."); }
    }
     { svelteHTML.createElement("div", { "class":`row`,});
       { svelteHTML.createElement("button", {       "class":`btn primary`,"disabled":busy || staged.runtime === "html" || (staged.runtime === "native" && !trust),"onclick":() => run(async () => (await req("streamdeck.install", { token: staged!.token, unsandboxed: staged!.runtime === "native" && trust }), (staged = null))),});
        t("Install");
       }
       { svelteHTML.createElement("button", {   "class":`btn`,"onclick":() => (staged = null),});t("Cancel"); }
     }
   }
}else{
   { svelteHTML.createElement("p", { "class":`muted`,});t("Plugins made for the Elgato Stream Deck app work here too. Download one (a .streamDeckPlugin file) and add it."); }
   { svelteHTML.createElement("div", { "class":`row`,});
     { svelteHTML.createElement("label", { "class":`btn`,}); { const $$_nocI2C = __sveltets_2_ensureComponent(Icon); new $$_nocI2C({ target: __sveltets_2_any(), props: {    "name":`file-up`,"size":18,}});}t("Add Stream Deck plugin"); { svelteHTML.createElement("input", {        "type":`file`,"accept":`.streamDeckPlugin,application/zip`,"class":`sr-only`,"onchange":upload,});} }
   }

     for(let p of __sveltets_2_ensureArray(list)){p.uuid;
     { svelteHTML.createElement("div", {  "class":`plugin`,});!p.enabled;
       { svelteHTML.createElement("button", {     "class":`head`,"onclick":() => (open = open === p.uuid ? null : p.uuid),"aria-expanded":open === p.uuid,});
         { svelteHTML.createElement("span", {   "class":`dot`,});p.running;!!p.blocked; }
         { svelteHTML.createElement("span", { "class":`name`,}); { svelteHTML.createElement("b", {});p.name; }  { svelteHTML.createElement("span", { "class":`mono muted`,});p.version; } }
        if(p.unsandboxed){ { svelteHTML.createElement("span", { "class":`tag warn`,});t("Unsandboxed"); }}
         { svelteHTML.createElement("span", { "class":`muted status`,});p.blocked ? t(p.blocked) : !p.enabled ? t("off") : p.running ? t("running") : t("starting"); }
       }
      if(open === p.uuid){
         { svelteHTML.createElement("div", { "class":`body`,});
          if(p.description){ { svelteHTML.createElement("p", { "class":`muted`,});p.description; }}
          if(p.runtime){ { svelteHTML.createElement("p", { "class":`muted`,});t(runtimeText[p.runtime]);  p.actions === 1 ? t("1 action") : t("{n} actions", { n: p.actions }); }}
          if(p.runtime === "node"){
             { svelteHTML.createElement("fieldset", {});
               { svelteHTML.createElement("legend", {});t("Internet access"); }
               { svelteHTML.createElement("label", { "class":`check`,}); { svelteHTML.createElement("input", {        "type":`radio`,"name":`net-${p.uuid}`,"checked":access(p) === "none","onchange":() => setAccess(p, "none"),});}t("None"); }
               { svelteHTML.createElement("label", { "class":`check`,}); { svelteHTML.createElement("input", {        "type":`radio`,"name":`net-${p.uuid}`,"checked":access(p) === "list" || hosts[p.uuid] !== undefined,"onchange":() => setAccess(p, "list"),});}t("Only these sites"); }
              if(access(p) === "list" || hosts[p.uuid] !== undefined){
                 { svelteHTML.createElement("textarea", {       "class":`mono`,"rows":3,"placeholder":`api.example.com`,"bind:value":hosts[p.uuid],});/*Ωignore_startΩ*/() => hosts[p.uuid] = __sveltets_2_any(null);/*Ωignore_endΩ*/ }
                 { svelteHTML.createElement("button", {     "class":`btn small`,"disabled":busy,"onclick":() => saveHosts(p),});t("Save sites"); }
              }
               { svelteHTML.createElement("label", { "class":`check`,}); { svelteHTML.createElement("input", {        "type":`radio`,"name":`net-${p.uuid}`,"checked":access(p) === "all","onchange":() => setAccess(p, "all"),});}t("Any site, including devices on your network"); }
             }
          }
           { svelteHTML.createElement("div", { "class":`row`,});
             { svelteHTML.createElement("button", {     "class":`btn`,"disabled":busy,"onclick":() => run(() => req("streamdeck.grant", { uuid: p.uuid, enabled: !p.enabled })),});p.enabled ? t("Turn off") : t("Turn on"); }
            if(p.runtime === "native" && p.unsandboxed){
               { svelteHTML.createElement("button", {     "class":`btn`,"disabled":busy,"onclick":() => run(() => req("streamdeck.grant", { uuid: p.uuid, unsandboxed: false })),});t("Stop running it unsandboxed"); }
            }
             { svelteHTML.createElement("button", {     "class":`btn danger`,"disabled":busy,"onclick":() => run(() => req("streamdeck.uninstall", { uuid: p.uuid })),});t("Remove"); }
           }
         }
      }
     }
  }
     { svelteHTML.createElement("p", { "class":`muted`,});t("No Stream Deck plugins yet."); }
  
}


};
return { props: {} as Record<string, never>, exports: {}, bindings: __sveltets_$$bindings(''), slots: {}, events: {} }}
const StreamDeck__SvelteComponent_ = __sveltets_2_fn_component($$render());
/*Ωignore_startΩ*/type StreamDeck__SvelteComponent_ = ReturnType<typeof StreamDeck__SvelteComponent_>;
/*Ωignore_endΩ*/export default StreamDeck__SvelteComponent_;
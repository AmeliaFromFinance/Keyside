///<reference types="svelte" />
;
import Icon from "../components/Icon.svelte";
import SettingsForm from "./SettingsForm.svelte";
import type { Schema } from "./state.svelte.ts";
import { reloadCatalog } from "./state.svelte.ts";
import { req } from "../lib/deck.svelte.ts";
import { t, mark } from "../lib/i18n.svelte.ts";
;

  interface Perms {
    net?: string[];
    udp?: boolean;
    ipc?: string[];
  };;
  interface Info {
    id: string;
    name: string;
    version: string;
    author: string;
    prefix: string;
    description?: string;
    homepage?: string;
    enabled: boolean;
    status: string;
    source: "bundled" | "registry" | "file";
    signer?: string;
    requested: Perms;
    granted: Perms;
    needsApproval: boolean;
    connection?: Schema;
    values: Record<string, unknown>;
  };;
  interface Staged {
    stageId: string;
    source: string;
    signer?: string;
    manifest: { id: string; name: string; version: string; author: string; description?: string; permissions: Perms };
    installed?: { version: string; signer?: string };
  };
;type $$ComponentProps =  { admin?: boolean };function $$render() {

  
  
  
  
  
  




  let { admin = false }:/*Ωignore_startΩ*/$$ComponentProps/*Ωignore_endΩ*/ = $props();

  const statusText: Record<string, string> = {
    connected: mark("connected"),
    connecting: mark("connecting"),
    offline: mark("offline"),
    error: mark("needs attention"),
    disabled: mark("off"),
  };

  let list = $state<Info[]>([]);
  let open = $state<string | null>(null);
  let edits = $state<Record<string, Record<string, unknown>>>({});
  let staged = $state<Staged | null>(null);
  let registry = $state<Array<{ id: string; name: string; version: string; author: string; description: string }> | null>(null);
  let error = $state("");
  let busy = $state(false);

  async function refresh() {
    list = await req<Info[]>("plugins.list");
  }
  $effect(() => void refresh().catch((e) => (error = e.message)));

  function describe(p: Perms): string[] {
    const out: string[] = [];
    for (const n of p.net ?? []) out.push(n.includes("{") ? t("Connect to the address set in its settings ({n})", { n }) : t("Connect to {n}", { n }));
    if (p.udp) out.push(t("Send UDP messages to those addresses"));
    if (p.ipc?.includes("discord")) out.push(t("Talk to the Discord app on this PC"));
    if (!out.length) out.push(t("No network access"));
    return out;
  }

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

  async function uploadFile(e: Event) {
    const file = (e.currentTarget as HTMLInputElement).files?.[0];
    if (!file) return;
    error = "";
    const r = await fetch("/admin/upload/plugin", { method: "POST", headers: { "x-phonedeck": "1" }, body: file });
    if (!r.ok) error = await r.text();
    else staged = await r.json();
  }

  async function browse() {
    error = "";
    try {
      registry = (await req<{ plugins: any[] }>("gallery.list")).plugins;
    } catch (e: any) {
      error = e.message;
    }
  }
;
async () => {

if(error){ { svelteHTML.createElement("p", {   "class":`bad`,"role":`alert`,});error; }}

if(staged){
   { svelteHTML.createElement("section", { "class":`staged`,});
     { svelteHTML.createElement("h3", {});t("Install {name} {v}?", { name: staged.manifest.name, v: staged.manifest.version }); }
     { svelteHTML.createElement("p", {});t("By {author}", { author: staged.manifest.author });staged.manifest.description ? ` · ${staged.manifest.description}` : ""; }
     { svelteHTML.createElement("p", { });!staged.signer;
       { const $$_nocI2C = __sveltets_2_ensureComponent(Icon); new $$_nocI2C({ target: __sveltets_2_any(), props: {    "name":staged.signer ? "badge-check" : "triangle-alert","size":18,}});}
      staged.signer ? t("Signed by key {k}", { k: staged.signer.slice(0, 12) }) : t("Not signed. Only install plugins from people you trust.");
     }
    if(staged.installed?.signer && staged.installed.signer !== staged.signer){ { svelteHTML.createElement("p", { "class":`bad`,});t("Signed by a different key than the installed version."); }}
     { svelteHTML.createElement("h4", {});t("It will be able to"); }
     { svelteHTML.createElement("ul", {});   for(let d of __sveltets_2_ensureArray(describe(staged.manifest.permissions))){d; { svelteHTML.createElement("li", {});d; }} }
     { svelteHTML.createElement("p", { "class":`muted`,});t("Plugins can't read your files, start programs or see other plugins. They only reach what is listed here."); }
     { svelteHTML.createElement("div", { "class":`row`,});
       { svelteHTML.createElement("button", {     "class":`btn primary`,"disabled":busy,"onclick":() => run(async () => (await req("plugins.install", { stageId: staged!.stageId, approve: true, source: staged!.source }), (staged = null))),});t("Install"); }
       { svelteHTML.createElement("button", {   "class":`btn`,"onclick":() => (staged = null),});t("Cancel"); }
     }
   }
} else if (registry){
   { svelteHTML.createElement("section", {});
     { svelteHTML.createElement("button", {   "class":`btn ghost`,"onclick":() => (registry = null),}); { const $$_nocI2C = __sveltets_2_ensureComponent(Icon); new $$_nocI2C({ target: __sveltets_2_any(), props: {    "name":`arrow-left`,"size":18,}});}t("Back"); }
       for(let r of __sveltets_2_ensureArray(registry)){r.id;
      const have = list.find((p) => p.id === r.id);
       { svelteHTML.createElement("div", { "class":`item`,});
         { svelteHTML.createElement("div", {}); { svelteHTML.createElement("b", {});r.name; }  { svelteHTML.createElement("span", { "class":`mono muted`,});r.version; } { svelteHTML.createElement("p", { "class":`muted`,});r.description;  r.author; } }
         { svelteHTML.createElement("button", {     "class":`btn`,"disabled":busy || have?.version === r.version,"onclick":() => run(async () => (staged = await req("plugins.stage", { registryId: r.id }))),});
          have ? (have.version === r.version ? t("Installed") : t("Update")) : t("Get");
         }
       }
    }
       { svelteHTML.createElement("p", { "class":`muted`,});t("The registry is empty right now."); }
    
   }
}else{
  if(admin){
     { svelteHTML.createElement("div", { "class":`row`,});
       { svelteHTML.createElement("button", {   "class":`btn`,"onclick":browse,}); { const $$_nocI2C = __sveltets_2_ensureComponent(Icon); new $$_nocI2C({ target: __sveltets_2_any(), props: {    "name":`download`,"size":18,}});}t("Browse plugins"); }
       { svelteHTML.createElement("label", { "class":`btn`,}); { const $$_nocI2C = __sveltets_2_ensureComponent(Icon); new $$_nocI2C({ target: __sveltets_2_any(), props: {    "name":`file-up`,"size":18,}});}t("Install from file"); { svelteHTML.createElement("input", {        "type":`file`,"accept":`.pdp,application/zip`,"class":`sr-only`,"onchange":uploadFile,});} }
     }
  }

     for(let p of __sveltets_2_ensureArray(list)){p.id;
     { svelteHTML.createElement("div", {  "class":`plugin`,});!p.enabled;
       { svelteHTML.createElement("button", {     "class":`head`,"onclick":() => (open = open === p.id ? null : p.id),"aria-expanded":open === p.id,});
         { svelteHTML.createElement("span", {    "class":`dot`,});p.status === "connected";p.status === "connecting";p.status === "error"; }
         { svelteHTML.createElement("span", { "class":`name`,}); { svelteHTML.createElement("b", {});p.name; }  { svelteHTML.createElement("span", { "class":`mono muted`,});p.version; } }
        if(p.needsApproval){ { svelteHTML.createElement("span", { "class":`tag warn`,});t("Needs approval"); }}
        if(p.source === "file" && !p.signer){ { svelteHTML.createElement("span", { "class":`tag`,});t("Unsigned"); }}
         { svelteHTML.createElement("span", { "class":`muted status`,});t(statusText[p.enabled ? p.status : "disabled"] ?? p.status); }
       }
      if(open === p.id){
         { svelteHTML.createElement("div", { "class":`body`,});
          if(p.description){ { svelteHTML.createElement("p", { "class":`muted`,});p.description; }}
          if(p.connection && admin){
             { const $$_mroFsgnitteS2C = __sveltets_2_ensureComponent(SettingsForm); new $$_mroFsgnitteS2C({ target: __sveltets_2_any(), props: {      "schema":p.connection,"values":{ ...p.values, ...(edits[p.id] ?? {}) },"onchange":(v) => (edits[p.id] = v),}});}
             { svelteHTML.createElement("button", {     "class":`btn primary`,"disabled":busy || !edits[p.id],"onclick":() => run(async () => (await req("plugins.configure", { id: p.id, values: edits[p.id] }), delete edits[p.id])),});t("Save settings"); }
          }
           { svelteHTML.createElement("h4", {});t("Allowed to"); }
           { svelteHTML.createElement("ul", {});   for(let d of __sveltets_2_ensureArray(describe(p.granted))){d; { svelteHTML.createElement("li", {});d; }} }
          if(p.needsApproval && admin){
             { svelteHTML.createElement("p", { "class":`warn`,});t("This version asks for more:"); }
             { svelteHTML.createElement("ul", {});   for(let d of __sveltets_2_ensureArray(describe(p.requested))){d; { svelteHTML.createElement("li", {});d; }} }
             { svelteHTML.createElement("button", {   "class":`btn primary`,"onclick":() => run(() => req("plugins.approve", { id: p.id })),});t("Allow"); }
          }
           { svelteHTML.createElement("p", { "class":`muted`,});t("Variables start with {prefix}.", { prefix: `${p.prefix}.` }); }
          if(admin){
             { svelteHTML.createElement("div", { "class":`row`,});
               { svelteHTML.createElement("button", {   "class":`btn`,"onclick":() => run(() => req("plugins.enable", { id: p.id, enabled: !p.enabled })),});p.enabled ? t("Turn off") : t("Turn on"); }
               { svelteHTML.createElement("button", {   "class":`btn`,"onclick":() => run(() => req("plugins.restart", { id: p.id })),});t("Restart"); }
              if(p.source !== "bundled"){ { svelteHTML.createElement("button", {   "class":`btn danger`,"onclick":() => run(() => req("plugins.uninstall", { id: p.id })),});t("Remove"); }}
             }
          }
         }
      }
     }
  }
}


};
return { props: {} as any as $$ComponentProps, exports: {}, bindings: __sveltets_$$bindings(''), slots: {}, events: {} }}
const Plugins__SvelteComponent_ = __sveltets_2_fn_component($$render());
/*Ωignore_startΩ*/type Plugins__SvelteComponent_ = ReturnType<typeof Plugins__SvelteComponent_>;
/*Ωignore_endΩ*/export default Plugins__SvelteComponent_;
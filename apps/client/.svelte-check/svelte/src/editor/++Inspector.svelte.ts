///<reference types="svelte" />
;
import type { Action, Key, KeyColor, KeyKind, Tone } from "@phonedeck/protocol";
import Icon from "../components/Icon.svelte";
import Sheet from "../components/Sheet.svelte";
import IconPicker from "./IconPicker.svelte";
import SettingsForm from "./SettingsForm.svelte";
import SdPanel from "./SdPanel.svelte";
import { ed, page, profile, selectedKey, change, actionDef, allActions, newAction, deleteKey, deletePage, fits } from "./state.svelte.ts";
import { req } from "../lib/deck.svelte.ts";
import { t, mark } from "../lib/i18n.svelte.ts";

;type $$ComponentProps =  { admin?: boolean };function $$render() {

  
  
  
  
  
  
  
  
  

  let { admin = false }:/*Ωignore_startΩ*/$$ComponentProps/*Ωignore_endΩ*/ = $props();

  let picking = $state(false);
  let adding = $state<"on" | "off" | null>(null);
  let open = $state<string | null>(null);
  let testing = $state(false);
  let addQuery = $state("");

  const key = $derived(selectedKey());
  const pg = $derived(page());

  const colors: Array<[KeyColor, string]> = [
    ["default", mark("Default")],
    ["teal", mark("Teal")],
    ["violet", mark("Violet")],
    ["green", mark("Green")],
    ["amber", mark("Amber")],
    ["red", mark("Red")],
    ["blue", mark("Blue")],
  ];
  const kinds: Array<[KeyKind, string]> = [
    ["button", mark("Button")],
    ["toggle", mark("Toggle")],
    ["folder", mark("Folder")],
    ["back", mark("Back")],
    ["slider", mark("Slider")],
    ["widget", mark("Text or chat")],
  ];
  const tones: Array<[Tone, string]> = [
    ["accent", mark("Highlight")],
    ["danger", mark("Red")],
    ["success", mark("Green text")],
    ["warning", mark("Amber text")],
  ];

  function edit(fn: (k: Key) => void) {
    const k = selectedKey();
    if (k) change(() => fn(k));
  }

  function list(off: boolean): Action[] {
    return (off ? key?.offActions : key?.actions) ?? [];
  }

  function editStep(off: boolean, i: number, fn: (a: Action) => void) {
    edit((k) => {
      const l = off ? (k.offActions ??= []) : k.actions;
      if (l[i]) fn(l[i]);
    });
  }

  function moveStep(off: boolean, i: number, d: number) {
    edit((k) => {
      const l = off ? (k.offActions ??= []) : k.actions;
      const j = i + d;
      if (j < 0 || j >= l.length) return;
      [l[i], l[j]] = [l[j], l[i]];
    });
  }

  function removeStep(off: boolean, i: number) {
    edit((k) => {
      const l = off ? (k.offActions ??= []) : k.actions;
      l.splice(i, 1);
    });
  }

  function addStep(type: string) {
    const off = adding === "off";
    edit((k) => (off ? (k.offActions ??= []) : k.actions).push(newAction(type)));
    adding = null;
    addQuery = "";
  }

  function badge(a: Action): string {
    if (a.type === "core.wait") return "WAIT";
    const plugin = ed.catalog.plugins.find((p) => a.type.startsWith(`${p.id}/`));
    if (plugin) return plugin.prefix.toUpperCase().slice(0, 6);
    return "CORE";
  }

  function summary(a: Action): string {
    if (a.type === "core.wait") return t("Delay {ms} ms", { ms: String(a.settings.ms ?? 0) });
    const def = actionDef(a.type);
    const name = t(def?.name ?? a.type);
    const detail = Object.entries(a.settings)
      .filter(([, v]) => typeof v === "string" || typeof v === "number")
      .slice(0, 2)
      .map(([k, v]) => (k === "args" || k === "url" ? `${v}` : `${v}`))
      .join(" ");
    return detail ? `${name} ${detail}` : name;
  }

  async function test() {
    const k = selectedKey();
    if (!k) return;
    testing = true;
    try {
      for (const a of k.actions) await req("action.test", { action: $state.snapshot(a) });
    } catch (e: any) {
      ed.error = e.message;
    } finally {
      setTimeout(() => (testing = false), 600);
    }
  }

  const sameBatch = $derived(list(false).some((a, i, l) => i > 0 && a.type.includes("/") && l[i - 1].type.split("/")[0] === a.type.split("/")[0]));
  const varNames = $derived(Object.keys(ed.catalog.variables).sort());
  const fb = $derived(key?.feedback?.[0]);
  const feeds = $derived(ed.catalog.plugins.flatMap((p: any) => (p.feeds ?? []).map((f: any) => ({ value: `${p.prefix}.${f.name}`, label: `${p.name}: ${f.title}` }))));
  const choices = $derived(
    allActions()
      .filter((a) => admin || !a.adminOnly)
      .filter((a) => !addQuery || `${a.name} ${a.group}`.toLowerCase().includes(addQuery.toLowerCase())),
  );

  function resize(dw: number, dh: number) {
    const k = selectedKey();
    const p = page();
    if (!k || !p) return;
    const w = Math.max(1, (k.w ?? 1) + dw);
    const h = Math.max(1, (k.h ?? 1) + dh);
    if (!fits(p, k.x, k.y, w, h, k.id)) return;
    edit((q) => {
      q.w = w > 1 ? w : undefined;
      q.h = h > 1 ? h : undefined;
    });
  }

  function setFeedback(patch: Partial<{ var: string; op: string; value: string; tone: Tone; badge: string }>) {
    edit((k) => {
      const cur = k.feedback?.[0] ?? { when: { var: "", op: "eq" as const }, style: { tone: "accent" as Tone } };
      const next = {
        when: { var: patch.var ?? cur.when.var, op: (patch.op ?? cur.when.op) as any, value: patch.value ?? cur.when.value },
        style: { ...cur.style, ...(patch.tone ? { tone: patch.tone } : {}), ...(patch.badge !== undefined ? { badge: patch.badge || undefined } : {}) },
      };
      k.feedback = next.when.var ? [next] : [];
    });
  }
;
async () => {

 { svelteHTML.createElement("aside", { "class":`inspector`,});
  if(key){
     { svelteHTML.createElement("header", {});
       { svelteHTML.createElement("div", {});
         { svelteHTML.createElement("h2", {});key.label || t("Untitled key"); }
        if(key.actions.length > 1){ { svelteHTML.createElement("span", { "class":`tag`,});t("Multi-action"); }}
         { svelteHTML.createElement("span", {   "class":`ref mono`,"title":t("Page and key id, for the HTTP API"),});ed.page; key.id; }
       }
       { svelteHTML.createElement("button", {     "class":`btn`,"onclick":test,"disabled":!key.actions.length || testing,}); { const $$_nocI3C = __sveltets_2_ensureComponent(Icon); new $$_nocI3C({ target: __sveltets_2_any(), props: {    "name":`play`,"size":16,}});}testing ? t("Running…") : t("Test run"); }
     }

     { svelteHTML.createElement("div", { "class":`two`,});
       { svelteHTML.createElement("label", { "class":`field`,}); { svelteHTML.createElement("span", {});t("Label"); } { svelteHTML.createElement("input", {      "value":key.label,"maxlength":64,"oninput":(e) => edit((k) => (k.label = e.currentTarget.value)),});} }
       { svelteHTML.createElement("div", { "class":`field`,});
         { svelteHTML.createElement("span", {});t("Icon"); }
         { svelteHTML.createElement("button", {   "class":`btn iconbtn`,"onclick":() => (picking = true),});
          if(key.icon && !key.icon.includes(":")){ { const $$_nocI4C = __sveltets_2_ensureComponent(Icon); new $$_nocI4C({ target: __sveltets_2_any(), props: {    "name":key.icon,"size":18,}});}}
          key.icon?.startsWith("asset:") ? t("Image") : (key.icon ?? t("None"));
         }
       }
     }
     { svelteHTML.createElement("label", { "class":`field`,}); { svelteHTML.createElement("span", {});t("Second line"); } { svelteHTML.createElement("input", {        "value":key.sub ?? "","maxlength":64,"placeholder":t("Optional"),"oninput":(e) => edit((k) => (k.sub = e.currentTarget.value || undefined)),});} }

     { svelteHTML.createElement("div", { "class":`two`,});
       { svelteHTML.createElement("div", { "class":`field`,});
         { svelteHTML.createElement("span", {});t("Colour"); }
         { svelteHTML.createElement("div", { "class":`swatches`,});
             for(let [c, name] of __sveltets_2_ensureArray(colors)){c;
             { svelteHTML.createElement("button", {        "class":`swatch c-${c}`,"aria-label":t(name),"title":t(name),"onclick":() => edit((k) => (k.color = c === "default" ? undefined : c)),});(key.color ?? "default") === c; }
          }
         }
       }
       { svelteHTML.createElement("label", { "class":`field`,});
         { svelteHTML.createElement("span", {});t("Type"); }
         { svelteHTML.createElement("select", {   "value":key.kind,"onchange":(e) => edit((k) => {
          k.kind = e.currentTarget.value as KeyKind;
          if (k.kind === "slider") k.slider ??= { min: 0, max: 1 };
          if (k.kind === "widget") k.widget ??= { text: "" };
          if (k.kind === "folder") k.target ??= profile()?.pages.find((p) => p.folder)?.id;
        }),});
             for(let [k, name] of __sveltets_2_ensureArray(kinds)){k; { svelteHTML.createElement("option", { "value":k,});t(name); }}
         }
       }
     }

    if(key.kind === "folder"){
       { svelteHTML.createElement("label", { "class":`field`,});
         { svelteHTML.createElement("span", {});t("Opens"); }
         { svelteHTML.createElement("select", {   "value":key.target ?? "","onchange":(e) => edit((k) => (k.target = e.currentTarget.value)),});
             for(let p of __sveltets_2_ensureArray(profile()?.pages ?? [])){p.id;if(p.id !== pg?.id){ { svelteHTML.createElement("option", { "value":p.id,});p.name; }}}
         }
       }
       { svelteHTML.createElement("button", {   "class":`btn`,"onclick":() => key.target && ((ed.page = key.target), (ed.key = null)),}); { const $$_nocI2C = __sveltets_2_ensureComponent(Icon); new $$_nocI2C({ target: __sveltets_2_any(), props: {    "name":`folder-open`,"size":18,}});}t("Edit folder"); }
    }

    if(key.kind === "widget"){
       { svelteHTML.createElement("label", { "class":`field`,});
         { svelteHTML.createElement("span", {});t("Text"); }
         { svelteHTML.createElement("input", {        "value":key.widget?.text ?? "","maxlength":200,"placeholder":"{twitch.viewers} viewers","oninput":(e) => edit((k) => (k.widget = { ...k.widget, text: e.currentTarget.value || undefined })),});}
         { svelteHTML.createElement("small", {});t("Use {name} to show a variable.", { name: "{name}" }); }
       }
       { svelteHTML.createElement("label", { "class":`field`,});
         { svelteHTML.createElement("span", {});t("Chat or alert feed"); }
         { svelteHTML.createElement("select", {   "value":key.widget?.feed ?? "","onchange":(e) => edit((k) => (k.widget = { ...k.widget, feed: e.currentTarget.value || undefined })),});
           { svelteHTML.createElement("option", {"value":"",});t("None"); }
             for(let f of __sveltets_2_ensureArray(feeds)){f.value; { svelteHTML.createElement("option", { "value":f.value,});f.label; }}
         }
       }
    }

    if(key.kind === "slider"){
       { svelteHTML.createElement("div", { "class":`two`,});
         { svelteHTML.createElement("label", { "class":`field`,}); { svelteHTML.createElement("span", {});t("Lowest"); } { svelteHTML.createElement("input", {      "type":`number`,"value":key.slider?.min ?? 0,"oninput":(e) => edit((k) => (k.slider = { ...k.slider!, min: Number(e.currentTarget.value) })),});} }
         { svelteHTML.createElement("label", { "class":`field`,}); { svelteHTML.createElement("span", {});t("Highest"); } { svelteHTML.createElement("input", {      "type":`number`,"value":key.slider?.max ?? 1,"oninput":(e) => edit((k) => (k.slider = { ...k.slider!, max: Number(e.currentTarget.value) })),});} }
       }
    }

    if(key.kind !== "folder" && key.kind !== "back"){
       { svelteHTML.createElement("div", { "class":`two`,});
         { svelteHTML.createElement("label", { "class":`check`,});
           { svelteHTML.createElement("input", {      "type":`checkbox`,"checked":!!key.hold,"onchange":(e) => edit((k) => (k.hold = e.currentTarget.checked ? 1000 : undefined)),});}
          t("Hold to confirm");
         }
        if(key.hold){
           { svelteHTML.createElement("label", { "class":`field inline`,}); { svelteHTML.createElement("input", {            "type":`number`,"min":`200`,"max":`10000`,"step":`100`,"value":key.hold,"oninput":(e) => edit((k) => (k.hold = Math.min(10000, Math.max(200, Number(e.currentTarget.value))))),});} { svelteHTML.createElement("span", {});  } }
        }
       }

       { svelteHTML.createElement("div", { "class":`field`,});
         { svelteHTML.createElement("span", {});t("Light up when"); }
         { svelteHTML.createElement("div", { "class":`cond`,});
           { svelteHTML.createElement("select", {   "value":fb?.when.var ?? "","onchange":(e) => setFeedback({ var: e.currentTarget.value }),});
             { svelteHTML.createElement("option", {"value":"",});t("Never"); }
               for(let v of __sveltets_2_ensureArray(varNames)){v; { svelteHTML.createElement("option", { "value":v,});v; }}
            if(fb && !varNames.includes(fb.when.var)){ { svelteHTML.createElement("option", { "value":fb.when.var,});fb.when.var; }}
           }
          if(fb){
             { svelteHTML.createElement("select", {   "value":fb.when.op,"onchange":(e) => setFeedback({ op: e.currentTarget.value }),});
               { svelteHTML.createElement("option", { "value":`eq`,});t("is"); }
               { svelteHTML.createElement("option", { "value":`ne`,});t("is not"); }
               { svelteHTML.createElement("option", { "value":`contains`,});t("contains"); }
               { svelteHTML.createElement("option", { "value":`gt`,});t("is above"); }
               { svelteHTML.createElement("option", { "value":`lt`,});t("is below"); }
               { svelteHTML.createElement("option", { "value":`truthy`,});t("is on"); }
               { svelteHTML.createElement("option", { "value":`falsy`,});t("is off"); }
             }
            if(!["truthy", "falsy"].includes(fb.when.op)){
               { svelteHTML.createElement("input", {        "class":`input`,"value":String(fb.when.value ?? ""),"placeholder":String(ed.catalog.variables[fb.when.var] ?? ""),"oninput":(e) => setFeedback({ value: e.currentTarget.value }),});}
            }
             { svelteHTML.createElement("select", {   "value":fb.style.tone ?? "accent","onchange":(e) => setFeedback({ tone: e.currentTarget.value as Tone }),});
                 for(let [tn, name] of __sveltets_2_ensureArray(tones)){tn; { svelteHTML.createElement("option", { "value":tn,});t(name); }}
             }
             { svelteHTML.createElement("input", {          "class":`input`,"value":fb.style.badge ?? "","maxlength":24,"placeholder":t("Badge, e.g. ON AIR"),"oninput":(e) => setFeedback({ badge: e.currentTarget.value }),});}
          }
         }
       }

         for(let off of __sveltets_2_ensureArray((key.kind === "toggle" ? [false, true] : [false]))){off;
         { svelteHTML.createElement("section", { "class":`steps`,});
           { svelteHTML.createElement("h3", {});off ? t("When turned off") : key.kind === "toggle" ? t("When turned on") : t("Steps · run in order"); }
              for(let a of __sveltets_2_ensureArray(list(off))){let i = 1;i;
            const def = actionDef(a.type);
             { svelteHTML.createElement("div", {   "class":`step`,});open === `${off}${i}`;a.review;
               { svelteHTML.createElement("button", {   "class":`summary`,"onclick":() => (open = open === `${off}${i}` ? null : `${off}${i}`),});
                 { svelteHTML.createElement("span", { "class":`n mono`,});i + 1; }
                 { svelteHTML.createElement("span", { "class":`kind`,});badge(a); }
                 { svelteHTML.createElement("span", { "class":`what`,});summary(a); }
               }
               { svelteHTML.createElement("button", {       "class":`btn icon ghost`,"aria-label":t("Move up"),"disabled":i === 0,"onclick":() => moveStep(off, i, -1),}); { const $$_nocI4C = __sveltets_2_ensureComponent(Icon); new $$_nocI4C({ target: __sveltets_2_any(), props: {    "name":`chevron-up`,"size":18,}});} }
               { svelteHTML.createElement("button", {     "class":`btn icon ghost`,"aria-label":t("Remove step"),"onclick":() => removeStep(off, i),}); { const $$_nocI4C = __sveltets_2_ensureComponent(Icon); new $$_nocI4C({ target: __sveltets_2_any(), props: {    "name":`x`,"size":18,}});} }
              if(open === `${off}${i}`){
                 { svelteHTML.createElement("div", { "class":`form`,});
                  if(a.review){ { svelteHTML.createElement("p", { "class":`warn`,});t("Imported step. It won't run until you approve it on the PC."); }}
                  if(!def){ { svelteHTML.createElement("p", { "class":`warn`,});t("This action needs a plugin that is not installed."); }}
                  if(a.type.startsWith("sd.")){
                    if(admin && !off && def){ { const $$_lenaPdS4C = __sveltets_2_ensureComponent(SdPanel); new $$_lenaPdS4C({ target: __sveltets_2_any(), props: {    "instance":`${ed.profile}/${ed.page}/${key.id}/${i}`,"dirty":ed.dirty,}});}} else if (def){ { svelteHTML.createElement("p", { "class":`muted`,});t("Stream Deck plugin settings open in the editor on the PC."); }}
                  }else{
                     { const $$_mroFsgnitteS4C = __sveltets_2_ensureComponent(SettingsForm); new $$_mroFsgnitteS4C({ target: __sveltets_2_any(), props: {        "schema":def?.settings,"values":a.settings,"type":a.type,"onchange":(v) => editStep(off, i, (s) => (s.settings = v)),}});}
                  }
                 }
              }
             }
          }
           { svelteHTML.createElement("button", {   "class":`add`,"onclick":() => (adding = off ? "off" : "on"),}); { const $$_nocI3C = __sveltets_2_ensureComponent(Icon); new $$_nocI3C({ target: __sveltets_2_any(), props: {    "name":`plus`,"size":18,}});}t("Add step"); }
          if(!off && sameBatch){ { svelteHTML.createElement("p", { "class":`note`,});t("Back-to-back steps for the same app go out together."); }}
         }
      }
    }

     { svelteHTML.createElement("div", { "class":`foot`,});
       { svelteHTML.createElement("span", { "class":`size`,});
        t("Size");
         { svelteHTML.createElement("button", {     "class":`btn icon`,"aria-label":t("Narrower"),"onclick":() => resize(-1, 0),});  }
         { svelteHTML.createElement("span", { "class":`mono`,});key.w ?? 1; key.h ?? 1; }
         { svelteHTML.createElement("button", {     "class":`btn icon`,"aria-label":t("Wider"),"onclick":() => resize(1, 0),});  }
         { svelteHTML.createElement("button", {     "class":`btn icon`,"aria-label":t("Shorter"),"onclick":() => resize(0, -1),});  }
         { svelteHTML.createElement("button", {     "class":`btn icon`,"aria-label":t("Taller"),"onclick":() => resize(0, 1),});  }
       }
       { svelteHTML.createElement("button", {   "class":`btn`,"onclick":() => (ed.moving = key.id),}); { const $$_nocI3C = __sveltets_2_ensureComponent(Icon); new $$_nocI3C({ target: __sveltets_2_any(), props: {    "name":`move`,"size":18,}});}t("Move"); }
       { svelteHTML.createElement("button", {   "class":`btn danger`,"onclick":() => deleteKey(key.id),}); { const $$_nocI3C = __sveltets_2_ensureComponent(Icon); new $$_nocI3C({ target: __sveltets_2_any(), props: {    "name":`trash-2`,"size":18,}});}t("Delete"); }
     }
  } else if (pg){
     { svelteHTML.createElement("header", {}); { svelteHTML.createElement("h2", {});t("Page settings"); } }
     { svelteHTML.createElement("label", { "class":`field`,}); { svelteHTML.createElement("span", {});t("Page name"); } { svelteHTML.createElement("input", {      "value":pg.name,"maxlength":40,"oninput":(e) => change(() => (pg.name = e.currentTarget.value)),});} }
     { svelteHTML.createElement("label", { "class":`field`,}); { svelteHTML.createElement("span", {});t("Subtitle"); } { svelteHTML.createElement("input", {      "value":pg.subtitle ?? "","maxlength":60,"oninput":(e) => change(() => (pg.subtitle = e.currentTarget.value || undefined)),});} }
    if(!pg.folder){
       { svelteHTML.createElement("label", { "class":`check`,});
         { svelteHTML.createElement("input", {      "type":`checkbox`,"checked":profile()?.homePage === pg.id,"onchange":() => change(() => (profile()!.homePage = pg.id)),});}
        t("Open this page first");
       }
    }
     { svelteHTML.createElement("p", { "class":`hint`,});t("Select a key to edit it, or drop an action on an empty cell."); }
     { svelteHTML.createElement("button", {     "class":`btn danger`,"onclick":() => deletePage(pg.id),"disabled":!pg.folder && (profile()?.pages.filter((p) => !p.folder).length ?? 0) <= 1,});
       { const $$_nocI2C = __sveltets_2_ensureComponent(Icon); new $$_nocI2C({ target: __sveltets_2_any(), props: {    "name":`trash-2`,"size":18,}});}t("Delete page");
     }
  }
 }

 { const $$_teehS0C = __sveltets_2_ensureComponent(Sheet); const $$_teehS0 = new $$_teehS0C({ target: __sveltets_2_any(), props: {     children:() => { return __sveltets_2_any(0); },open:picking,"title":t("Choose an icon"),"wide":true,}});/*Ωignore_startΩ*/() => picking = __sveltets_2_any(null);/*Ωignore_endΩ*/$$_teehS0.$$bindings = 'open';
   { const $$_rekciPnocI1C = __sveltets_2_ensureComponent(IconPicker); new $$_rekciPnocI1C({ target: __sveltets_2_any(), props: {    "value":key?.icon,"onpick":(i) => (edit((k) => (k.icon = i)), (picking = false)),}});}
 Sheet}

 { const $$_teehS0C = __sveltets_2_ensureComponent(Sheet); new $$_teehS0C({ target: __sveltets_2_any(), props: {      children:() => { return __sveltets_2_any(0); },"open":adding !== null,"onclose":() => (adding = null),"title":t("Add step"),"wide":true,}});
   { svelteHTML.createElement("input", {        "class":`input`,"bind:value":addQuery,"placeholder":t("Search actions"),"aria-label":t("Search actions"),});/*Ωignore_startΩ*/() => addQuery = __sveltets_2_any(null);/*Ωignore_endΩ*/}
   { svelteHTML.createElement("div", { "class":`choices`,});
       for(let a of __sveltets_2_ensureArray(choices)){a.id;
       { svelteHTML.createElement("button", {   "class":`choice`,"onclick":() => addStep(a.id),});
         { const $$_nocI3C = __sveltets_2_ensureComponent(Icon); new $$_nocI3C({ target: __sveltets_2_any(), props: {    "name":a.icon ?? "bolt","size":20,}});}
         { svelteHTML.createElement("span", {}); { svelteHTML.createElement("b", {});t(a.name); } { svelteHTML.createElement("small", {});t(a.group ?? ""); } }
       }
    }
   }
   { svelteHTML.createElement("button", {   "class":`btn`,"onclick":() => (adding = null),});t("Cancel"); }
 Sheet}


};
return { props: {} as any as $$ComponentProps, exports: {}, bindings: __sveltets_$$bindings(''), slots: {}, events: {} }}
const Inspector__SvelteComponent_ = __sveltets_2_fn_component($$render());
/*Ωignore_startΩ*/type Inspector__SvelteComponent_ = ReturnType<typeof Inspector__SvelteComponent_>;
/*Ωignore_endΩ*/export default Inspector__SvelteComponent_;
///<reference types="svelte" />
;
import Icon from "../components/Icon.svelte";
import { ed, change, newId } from "./state.svelte.ts";
import { t } from "../lib/i18n.svelte.ts";
function $$render() {

  
  
  

  function add() {
    if (!ed.layout) return;
    const id = newId("l", ed.layout.profiles.map((p) => p.id));
    change((l) => l.profiles.push({ id, name: t("New layout"), homePage: "p1", pages: [{ id: "p1", name: "Page 1", cols: 3, rows: 5, keys: [] }] }));
    ed.profile = id;
    ed.page = "p1";
  }

  function remove(id: string) {
    if (!ed.layout || ed.layout.profiles.length <= 1) return;
    change((l) => {
      l.profiles = l.profiles.filter((p) => p.id !== id);
      if (l.defaultProfile === id) l.defaultProfile = l.profiles[0].id;
    });
    if (ed.profile === id) {
      ed.profile = ed.layout.defaultProfile;
      ed.page = ed.layout.profiles[0].homePage;
    }
  }

  function apps(id: string, text: string) {
    change((l) => {
      const p = l.profiles.find((q) => q.id === id);
      if (p) {
        const list = text.split(",").map((s) => s.trim()).filter(Boolean).slice(0, 20);
        if (list.length) p.apps = list;
        else delete p.apps;
      }
    });
  }
;
async () => {

 { svelteHTML.createElement("p", { "class":`muted`,});t("Each layout has its own pages. Keyside can switch layouts when an app comes to the front on your PC."); }
   for(let p of __sveltets_2_ensureArray(ed.layout?.profiles ?? [])){p.id;
   { svelteHTML.createElement("div", { "class":`prof`,});
     { svelteHTML.createElement("label", { "class":`field`,}); { svelteHTML.createElement("span", {});t("Name"); } { svelteHTML.createElement("input", {      "value":p.name,"maxlength":40,"oninput":(e) => change(() => (p.name = e.currentTarget.value)),});} }
     { svelteHTML.createElement("label", { "class":`field`,});
       { svelteHTML.createElement("span", {});t("Switch to it when these apps are in front"); }
       { svelteHTML.createElement("input", {      "value":(p.apps ?? []).join(", "),"placeholder":`obs64.exe, Photoshop`,"onchange":(e) => apps(p.id, e.currentTarget.value),});}
       { svelteHTML.createElement("small", {});t("Program names, separated by commas. Leave empty to switch by hand."); }
     }
     { svelteHTML.createElement("div", { "class":`row`,});
       { svelteHTML.createElement("label", { "class":`check`,}); { svelteHTML.createElement("input", {        "type":`radio`,"name":`default`,"checked":ed.layout?.defaultProfile === p.id,"onchange":() => change((l) => (l.defaultProfile = p.id)),});}t("Default layout"); }
       { svelteHTML.createElement("button", {     "class":`btn danger`,"disabled":(ed.layout?.profiles.length ?? 0) <= 1,"onclick":() => remove(p.id),}); { const $$_nocI3C = __sveltets_2_ensureComponent(Icon); new $$_nocI3C({ target: __sveltets_2_any(), props: {    "name":`trash-2`,"size":18,}});}t("Delete"); }
     }
   }
}
 { svelteHTML.createElement("button", {   "class":`btn`,"onclick":add,}); { const $$_nocI1C = __sveltets_2_ensureComponent(Icon); new $$_nocI1C({ target: __sveltets_2_any(), props: {    "name":`plus`,"size":18,}});}t("Add layout"); }


};
return { props: {} as Record<string, never>, exports: {}, bindings: "", slots: {}, events: {} }}
const Profiles__SvelteComponent_ = __sveltets_2_isomorphic_component(__sveltets_2_with_any_event($$render()));
/*Ωignore_startΩ*/type Profiles__SvelteComponent_ = InstanceType<typeof Profiles__SvelteComponent_>;
/*Ωignore_endΩ*/export default Profiles__SvelteComponent_;
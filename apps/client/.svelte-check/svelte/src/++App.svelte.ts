///<reference types="svelte" />
;
import Deck from "./components/Deck.svelte";
import Pair from "./components/Pair.svelte";
import Editor from "./editor/Editor.svelte";
import { companions, parsePairLink, type PairParams, type StoredCompanion } from "./lib/companions.ts";
import { connect, deck, disconnect } from "./lib/deck.svelte.ts";
import { isNative } from "./lib/native.ts";
import { t, mark } from "./lib/i18n.svelte.ts";
function $$render() {

  
  
  
  
  
  
  

  type View = "loading" | "signin" | "editor" | "deck" | "pair";
  let view = $state<View>("loading");
  let admin = $state(false);
  let pairParams = $state<PairParams | null>(null);
  let saved = $state<StoredCompanion[]>([]);
  let current = $state<StoredCompanion | null>(null);
  let notice = $state("");

  const local = ["127.0.0.1", "localhost", "[::1]"].includes(location.hostname) && location.protocol === "http:" && !isNative();

  async function startAdmin() {
    const hash = new URLSearchParams(location.hash.slice(1));
    const token = hash.get("login");
    if (token) {
      history.replaceState(null, "", location.pathname);
      await fetch("/admin/login", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ token }) }).catch(() => null);
    }
    const st = await fetch("/admin/status").then((r) => (r.ok ? r.json() : null)).catch(() => null);
    if (!st?.signedIn) {
      view = "signin";
      return;
    }
    admin = true;
    connect({ kind: "admin" });
    view = "editor";
  }

  async function startDevice() {
    // The pairing code is in the fragment; read it and remove it from the address bar.
    if (location.hash.includes("pair")) {
      pairParams = parsePairLink(location.hash);
      history.replaceState(null, "", location.pathname);
    }
    saved = await companions.all().catch(() => []);
    if (pairParams || !saved.length) {
      view = "pair";
      return;
    }
    open(saved[0]);
  }

  function open(c: StoredCompanion) {
    current = c;
    notice = "";
    connect({ kind: "device", companion: c });
    view = "deck";
  }

  function paired(c: StoredCompanion) {
    current = c;
    pairParams = null;
    void companions.all().then((l) => (saved = l));
    view = "deck";
  }

  async function forget() {
    if (current) await companions.remove(current.cid);
    disconnect();
    current = null;
    saved = await companions.all();
    view = "pair";
  }

  // A device that was removed on the PC can't reconnect, so forget it and offer pairing.
  $effect(() => {
    if (view === "deck" && !admin && deck.conn.s === "closed" && ["unknown-device", "revoked", "wrong-companion"].includes(deck.conn.reason)) {
      notice = mark("This device was removed on the PC. Pair it again to keep using it.");
      void forget();
    }
  });

  $effect(() => {
    if (local) void startAdmin();
    else void startDevice();
  });
;
async () => {

if(view === "loading"){
   { svelteHTML.createElement("div", { "class":`center`,}); { svelteHTML.createElement("p", {});t("Loading…"); } }
} else if (view === "signin"){
   { svelteHTML.createElement("div", { "class":`center`,});
     { svelteHTML.createElement("h1", {});t("Open the editor from Keyside"); }
     { svelteHTML.createElement("p", {});t("For your security the editor only opens from the Keyside app or the keyside open command on this PC."); }
   }
} else if (view === "pair"){
  if(notice){ { svelteHTML.createElement("p", {   "class":`notice`,"role":`alert`,});t(notice); }}
   { const $$_riaP0C = __sveltets_2_ensureComponent(Pair); new $$_riaP0C({ target: __sveltets_2_any(), props: {       "params":pairParams,saved,"onpaired":paired,"onopen":open,}});}
} else if (view === "deck"){
   { const $$_kceD0C = __sveltets_2_ensureComponent(Deck); new $$_kceD0C({ target: __sveltets_2_any(), props: {         "canEdit":admin || deck.self?.scope === "editor","onedit":() => (view = "editor"),"onswitch":admin ? undefined : () => (view = "pair"),"onforget":admin ? undefined : forget,}});}
} else if (view === "editor"){
   { const $$_rotidE0C = __sveltets_2_ensureComponent(Editor); new $$_rotidE0C({ target: __sveltets_2_any(), props: {   admin,"ondeck":() => (view = "deck"),}});}
}


};
return { props: {} as Record<string, never>, exports: {}, bindings: __sveltets_$$bindings(''), slots: {}, events: {} }}
const App__SvelteComponent_ = __sveltets_2_fn_component($$render());
/*Ωignore_startΩ*/type App__SvelteComponent_ = ReturnType<typeof App__SvelteComponent_>;
/*Ωignore_endΩ*/export default App__SvelteComponent_;
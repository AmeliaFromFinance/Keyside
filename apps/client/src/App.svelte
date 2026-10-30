<script lang="ts">
  import Deck from "./components/Deck.svelte";
  import Pair from "./components/Pair.svelte";
  import { companions, parsePairLink, type PairParams, type StoredCompanion } from "./lib/companions.ts";
  import { connect, deck, disconnect } from "./lib/deck.svelte.ts";
  import { isNative } from "./lib/native.ts";
  import { t, mark } from "./lib/i18n.svelte.ts";

  type View = "loading" | "signin" | "editor" | "deck" | "pair";
  let view = $state<View>("loading");
  let admin = $state(false);
  let pairParams = $state<PairParams | null>(null);
  let saved = $state<StoredCompanion[]>([]);
  let current = $state<StoredCompanion | null>(null);
  let notice = $state("");

  const local = ["127.0.0.1", "localhost", "[::1]"].includes(location.hostname) && location.protocol === "http:" && !isNative();

  // Phones only load the editor when someone opens it, so the deck starts faster.
  let editorChunk: Promise<typeof import("./editor/Editor.svelte")> | null = null;
  const loadEditor = () => (editorChunk ??= import("./editor/Editor.svelte"));
  // The PC opens the editor right away: fetch it while signing in.
  if (local) void loadEditor();

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
</script>

{#if view === "loading"}
  <div class="center"><p>{t("Loading…")}</p></div>
{:else if view === "signin"}
  <div class="center">
    <h1>{t("Open the editor from Keyside")}</h1>
    <p>{t("For your security the editor only opens from the Keyside app or the keyside open command on this PC.")}</p>
  </div>
{:else if view === "pair"}
  {#if notice}<p class="notice" role="alert">{t(notice)}</p>{/if}
  <Pair params={pairParams} {saved} onpaired={paired} onopen={open} />
{:else if view === "deck"}
  <Deck
    canEdit={admin || deck.self?.scope === "editor"}
    onedit={() => (view = "editor")}
    onswitch={admin ? undefined : () => (view = "pair")}
    onforget={admin ? undefined : forget}
  />
{:else if view === "editor"}
  {#await loadEditor()}
    <div class="center"><p>{t("Loading…")}</p></div>
  {:then { default: Editor }}
    <Editor {admin} ondeck={() => (view = "deck")} />
  {:catch}
    <div class="center">
      <p>{t("The editor didn't load. Check the connection and try again.")}</p>
      <button class="btn" onclick={() => location.reload()}>{t("Reload")}</button>
    </div>
  {/await}
{/if}

<style>
  .center {
    min-height: 100dvh;
    display: grid;
    place-content: center;
    gap: 8px;
    padding: 24px;
    text-align: center;
    max-width: 520px;
    margin: 0 auto;
  }
  .center h1 {
    margin: 0;
    font-size: 1.4rem;
  }
  .center p {
    margin: 0;
    color: var(--muted);
  }
  .notice {
    margin: 16px auto 0;
    max-width: 440px;
    padding: 12px 16px;
    border-radius: 12px;
    background: var(--danger-bg);
    color: var(--danger-text);
  }
</style>

<script lang="ts">
  import Icon from "./Icon.svelte";
  import Scanner from "./Scanner.svelte";
  import { parsePairLink, type PairParams, type StoredCompanion } from "../lib/companions.ts";
  import { connect, deck } from "../lib/deck.svelte.ts";
  import { deviceKind, deviceName, nativeScan, platform } from "../lib/native.ts";
  import { t, mark } from "../lib/i18n.svelte.ts";

  let { params = null, saved = [], onpaired, onopen }: {
    params?: PairParams | null;
    saved?: StoredCompanion[];
    onpaired: (c: StoredCompanion) => void;
    onopen: (c: StoredCompanion) => void;
  } = $props();

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
</script>

<main class="pair">
  <div class="mark" aria-hidden="true"><Icon name="layout-grid" size={36} /></div>

  {#if started && deck.conn.s === "waiting"}
    <h1>{t("Approve on your PC")}</h1>
    <p>{t("Check that your PC shows this code, then approve this device there.")}</p>
    <div class="sas mono" aria-label={t("Code {code}", { code: deck.sas })}>{deck.sas.slice(0, 3)} {deck.sas.slice(3)}</div>
    <p class="hint">{t("Waiting for approval…")}</p>
  {:else if started}
    <h1>{t("Connecting…")}</h1>
    <p>{t("Pairing with {name}", { name: pending?.name ?? "" })}</p>
  {:else if scanning}
    <h1>{t("Scan the code")}</h1>
    <Scanner onresult={use} oncancel={() => (scanning = false)} />
  {:else if pending}
    <h1>{t("Pair with {name}", { name: pending.name })}</h1>
    <label class="field">
      <span>{t("Name this device")}</span>
      <input bind:value={name} maxlength="40" autocomplete="off" />
    </label>
    <button class="btn primary" onclick={pair}><Icon name="link" />{t("Pair")}</button>
    <button class="btn ghost" onclick={() => (pending = null)}>{t("Use a different code")}</button>
  {:else}
    <h1>{t("Pair with your PC")}</h1>
    <ol class="steps">
      <li>{t("Open Keyside on your PC.")}</li>
      <li>{t("Choose Pair a phone.")}</li>
      <li>{t("Scan the code it shows.")}</li>
    </ol>
    <button class="btn primary" onclick={scan}><Icon name="scan" />{t("Scan code")}</button>
    <details>
      <summary>{t("Paste the pairing link instead")}</summary>
      <div class="paste">
        <input class="input" bind:value={link} placeholder="https://…#pair&…" autocomplete="off" />
        <button class="btn" onclick={() => use(link)} disabled={!link}>{t("Continue")}</button>
      </div>
    </details>
  {/if}

  {#if problem}<p class="problem" role="alert">{t(problem, problemVars)}</p>{/if}

  {#if saved.length && !started}
    <section class="saved">
      <h2>{t("Paired PCs")}</h2>
      {#each saved as c (c.cid)}
        <button class="btn" onclick={() => onopen(c)}><Icon name="monitor" />{c.name}</button>
      {/each}
    </section>
  {/if}
</main>

<style>
  .pair {
    min-height: 100dvh;
    max-width: 440px;
    margin: 0 auto;
    padding: max(32px, env(safe-area-inset-top)) 20px max(24px, env(safe-area-inset-bottom));
    display: flex;
    flex-direction: column;
    gap: 16px;
    justify-content: center;
  }
  .mark {
    color: var(--accent);
  }
  h1 {
    margin: 0;
    font-size: 1.6rem;
    letter-spacing: -0.01em;
  }
  h2 {
    font-size: 1rem;
    color: var(--muted);
    margin: 16px 0 0;
  }
  p {
    margin: 0;
    color: var(--muted);
  }
  .steps {
    margin: 0;
    padding-left: 1.3em;
    color: var(--muted);
    display: grid;
    gap: 6px;
  }
  .sas {
    font-size: 2.8rem;
    letter-spacing: 0.08em;
    padding: 16px 0;
    color: var(--accent);
  }
  .hint {
    color: var(--dim);
  }
  details summary {
    color: var(--muted);
    cursor: pointer;
  }
  .paste {
    display: flex;
    gap: 8px;
    margin-top: 10px;
  }
  .paste .input {
    flex: 1;
    min-width: 0;
  }
  .problem {
    color: var(--danger-text);
  }
  .saved {
    display: grid;
    gap: 8px;
  }
  .saved .btn {
    justify-content: flex-start;
  }
</style>

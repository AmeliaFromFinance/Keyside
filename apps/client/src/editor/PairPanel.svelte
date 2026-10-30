<script lang="ts">
  import QRCode from "qrcode";
  import Icon from "../components/Icon.svelte";
  import Sheet from "../components/Sheet.svelte";
  import DeviceDetails, { type Device } from "./DeviceDetails.svelte";
  import { req, listen } from "../lib/deck.svelte.ts";
  import { ed } from "./state.svelte.ts";
  import { t } from "../lib/i18n.svelte.ts";
  import { ago, everySecond } from "../lib/format.ts";

  interface Pending {
    pid: string;
    name: string;
    kind: string;
    sas: string;
    address: string;
  }

  let { onmanage }: { onmanage?: () => void } = $props();

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
  let open = $state<string | null>(null);
  const opened = $derived(devices.find((d) => d.id === open));
  /** PINs looked up on hover, by device. */
  let pins = $state<Record<string, string | null>>({});

  function peek(d: Device) {
    if (!d.hasPin || d.id in pins) return;
    pins[d.id] = null;
    void req<{ pin: string | null }>("devices.pin", { id: d.id }).then((r) => (pins[d.id] = r.pin), () => {});
  }

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
    open = null;
    void refresh();
  }

  const left = $derived(Math.max(0, Math.round((expires - now) / 1000)));
  const pages = $derived(ed.layout?.profiles.find((p) => p.id === ed.profile)?.pages.filter((p) => !p.folder) ?? []);
</script>

<section class="pair">
  <header>
    <h2>{t("Pair a phone")}</h2>
    {#if qr && left > 0}<span class="exp">{t("Code expires in")} <b class="mono">{Math.floor(left / 60)}:{String(left % 60).padStart(2, "0")}</b></span>{/if}
  </header>

  {#each pending as p (p.pid)}
    <div class="pending" role="alert">
      <p><b>{p.name}</b> {t("wants to pair")} <span class="mono addr">{p.address}</span></p>
      <p>{t("Check the phone shows")} <b class="sas mono">{p.sas.slice(0, 3)} {p.sas.slice(3)}</b></p>
      <div class="row">
        <button class="btn primary" onclick={() => answer(p.pid, true)}><Icon name="check" size={18} />{t("Approve")}</button>
        <button class="btn danger" onclick={() => answer(p.pid, false)}>{t("Deny")}</button>
      </div>
    </div>
  {/each}

  {#if qr && left > 0}
    <div class="code">
      <div class="qr" aria-label={t("Pairing QR code")}>{@html qr}</div>
      <div class="info">
        <p>{t("Scan with the phone camera, then approve the phone here. Single use.")}</p>
        {#if fingerprint}
          <p class="fp">{t("Certificate (browsers show this)")}<br /><span class="mono">{fingerprint.slice(0, 11)} … {fingerprint.slice(-8)}</span></p>
        {/if}
        {#if requirePin}<p>{t("PIN for this phone")}: <b class="mono">{pin}</b></p>{/if}
        <button class="btn ghost small" onclick={() => navigator.clipboard?.writeText(url)}><Icon name="copy" size={16} />{t("Copy link")}</button>
        <a class="link mono" href={url} target="_blank" rel="noreferrer">{t("Open link")}</a>
      </div>
    </div>
  {:else}
    <div class="setup">
      <label class="field">
        <span>{t("Access")}</span>
        <select bind:value={scope}>
          <option value="deck">{t("Use the deck")}</option>
          <option value="editor">{t("Use and edit the deck")}</option>
          <option value="moderator">{t("Moderator: chosen pages only")}</option>
        </select>
      </label>
      {#if scope === "moderator"}
        <div class="mods">
          {#each pages as p (p.id)}
            <label class="check"><input type="checkbox" value={p.id} bind:group={modPages} />{p.name}</label>
          {/each}
        </div>
      {/if}
      <label class="check"><input type="checkbox" bind:checked={requirePin} />{t("Require a PIN on this phone")}</label>
      {#if requirePin}
        <label class="field"><span>PIN</span><input class="mono" bind:value={pin} inputmode="numeric" maxlength="12" pattern="[0-9]*" /></label>
      {/if}
      <button class="btn primary" onclick={start} disabled={(requirePin && !/^\d{4,12}$/.test(pin)) || (scope === "moderator" && !modPages.length)}>
        <Icon name="qr-code" size={18} />{t("Show pairing code")}
      </button>
    </div>
  {/if}
  {#if error}<p class="bad">{error}</p>{/if}

  <h3>{t("Paired devices")} · {devices.length}</h3>
  {#each devices as d (d.id)}
    <button class="dev" onclick={() => (open = d.id)} onpointerenter={() => peek(d)} onfocus={() => peek(d)} title={t("Show details")}>
      <span class="dot" class:ok={d.online}></span>
      <span class="name">{d.name}</span>
      <span class="meta">
        {d.online ? t("online") : t("seen {t} ago", { t: ago(d.lastSeen * 1000, now) })}{d.scope !== "deck" ? ` · ${d.scope === "editor" ? t("editor") : t("moderator")}` : ""}
        {#if d.hasPin}
          · PIN <span class="pin mono"><span class="masked">••••</span><span class="plain">{pins[d.id] ?? "••••"}</span></span>
        {/if}
      </span>
      <Icon name="chevron-right" size={18} />
    </button>
  {:else}
    <p class="muted">{t("No devices yet.")}</p>
  {/each}
  {#if onmanage && devices.length}<button class="btn ghost small" onclick={onmanage}>{t("Manage devices")}</button>{/if}
</section>

<Sheet open={!!opened} title={opened?.name ?? ""} onclose={() => (open = null)}>
  {#if opened}<DeviceDetails device={opened} {now} onchanged={refresh} onrevoke={revoke} />{/if}
</Sheet>

<style>
  .pair {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 20px;
    border-top: 1px solid var(--line-soft);
  }
  header {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 8px;
  }
  h2 {
    margin: 0;
    font-size: 1.15rem;
  }
  h3 {
    margin: 8px 0 0;
    font-size: 0.95rem;
    color: var(--muted);
    font-weight: 500;
  }
  .exp {
    color: var(--muted);
    font-size: 0.9rem;
  }
  .exp b {
    color: var(--text);
  }
  .code {
    display: grid;
    grid-template-columns: 160px 1fr;
    gap: 16px;
  }
  .qr {
    border-radius: 12px;
    overflow: hidden;
    background: #edebf2;
    aspect-ratio: 1;
  }
  .qr :global(svg) {
    width: 100%;
    height: 100%;
    display: block;
  }
  .info p {
    margin: 0 0 10px;
    color: var(--text);
  }
  .fp {
    color: var(--muted) !important;
    font-size: 0.85rem;
  }
  .setup {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .mods {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 16px;
  }
  .pending {
    border: 1px solid var(--accent);
    background: var(--accent-bg);
    border-radius: var(--radius);
    padding: 14px;
  }
  .pending p {
    margin: 0 0 8px;
  }
  .addr {
    color: var(--muted);
  }
  .sas {
    font-size: 1.3rem;
    color: var(--accent);
  }
  .row {
    display: flex;
    gap: 8px;
  }
  .dev {
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: 4px 12px;
    padding: 12px 14px;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    background: var(--surface);
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .dev:hover,
  .dev:focus-visible {
    border-color: var(--line);
  }
  .dev .meta {
    grid-column: 2;
    grid-row: 2;
    color: var(--muted);
    font-size: 0.85rem;
  }
  .dev > :global(svg) {
    grid-row: 1 / span 2;
    grid-column: 3;
    color: var(--muted);
  }
  /* The PIN shows while the pointer is on the row. */
  .pin .plain,
  .dev:hover .pin .masked,
  .dev:focus-visible .pin .masked {
    display: none;
  }
  .dev:hover .pin .plain,
  .dev:focus-visible .pin .plain {
    display: inline;
  }
  .small {
    min-height: 34px;
    padding: 0 12px;
    font-size: 0.9rem;
  }
  .link {
    display: block;
    margin-top: 6px;
    font-size: 0.8rem;
    color: var(--accent);
  }
  .muted {
    margin: 0;
    color: var(--muted);
  }
  .bad {
    margin: 0;
    color: var(--danger-text);
  }
  @media (max-width: 420px) {
    .code {
      grid-template-columns: 1fr;
    }
  }
</style>

<script lang="ts" module>
  export interface Device {
    id: string;
    name: string;
    kind: string;
    app: string;
    scope: "deck" | "moderator" | "editor";
    pages: string[];
    hasPin: boolean;
    created: number;
    lastSeen: number;
    online: boolean;
  }
</script>

<script lang="ts">
  import Icon from "../components/Icon.svelte";
  import { req } from "../lib/deck.svelte.ts";
  import { ed } from "./state.svelte.ts";
  import { t, mark } from "../lib/i18n.svelte.ts";
  import { ago } from "../lib/format.ts";

  let { device, now, onchanged, onrevoke }: { device: Device; now: number; onchanged: () => void; onrevoke: (id: string) => void } = $props();

  let name = $state("");
  $effect(() => {
    name = device.name;
  });
  let pin = $state<string | null>(null);
  let shown = $state(false);
  let newPin = $state("");
  let error = $state("");
  let saved = $state("");
  let sure = $state(false);

  const KINDS: Record<string, string> = { phone: mark("Phone"), tablet: mark("Tablet"), desktop: mark("Computer"), browser: mark("Browser") };
  const APPS: Record<string, string> = { web: mark("Browser"), android: mark("Android app"), ios: mark("iPhone app"), desktop: mark("Desktop app") };
  const pages = $derived(ed.layout?.profiles.flatMap((p) => p.pages.filter((pg) => !pg.folder).map((pg) => ({ id: `${p.id}/${pg.id}`, name: ed.layout!.profiles.length > 1 ? `${p.name} › ${pg.name}` : pg.name }))) ?? []);

  $effect(() => {
    if (device.hasPin) void req<{ pin: string | null }>("devices.pin", { id: device.id }).then((r) => (pin = r.pin), () => (pin = null));
    else pin = null;
  });

  /** Changes go out straight away. The device reconnects to pick them up. */
  async function update(args: Record<string, unknown>, done = t("Saved. The device reconnects to apply it.")) {
    error = "";
    saved = "";
    try {
      await req("devices.update", { id: device.id, ...args });
      saved = done;
      onchanged();
    } catch (e: any) {
      error = e.message;
    }
  }

  async function setPin(value: string | null) {
    await update({ pin: value });
    if (!error) {
      pin = value;
      newPin = "";
      shown = !!value;
    }
  }

  function date(secs: number) {
    return new Date(secs * 1000).toLocaleDateString(undefined, { year: "numeric", month: "short", day: "numeric" });
  }
</script>

<div class="details">
  <label class="field">
    <span>{t("Name")}</span>
    <input bind:value={name} maxlength="40" onchange={() => name.trim() && name.trim() !== device.name && update({ name: name.trim() }, t("Saved"))} />
  </label>

  <dl>
    <dt>{t("Status")}</dt>
    <dd><span class="dot" class:ok={device.online}></span>{device.online ? t("Online") : t("Seen {t} ago", { t: ago(device.lastSeen * 1000, now) })}</dd>
    <dt>{t("Device")}</dt>
    <dd>{t(KINDS[device.kind] ?? KINDS.browser)} · {t(APPS[device.app] ?? APPS.web)}</dd>
    <dt>{t("Paired")}</dt>
    <dd>{date(device.created)}</dd>
    <dt>{t("ID")}</dt>
    <dd class="mono">{device.id.slice(0, 8)}</dd>
  </dl>

  <label class="field">
    <span>{t("Access")}</span>
    <select value={device.scope} onchange={(e) => update({ scope: e.currentTarget.value })}>
      <option value="deck">{t("Use the deck")}</option>
      <option value="editor">{t("Use and edit the deck")}</option>
      <option value="moderator">{t("Moderator: chosen pages only")}</option>
    </select>
  </label>
  {#if device.scope === "moderator"}
    <div class="mods">
      {#each pages as p (p.id)}
        <label class="check">
          <input
            type="checkbox"
            checked={device.pages.includes(p.id)}
            onchange={(e) => update({ pages: e.currentTarget.checked ? [...device.pages, p.id] : device.pages.filter((x) => x !== p.id) })}
          />{p.name}
        </label>
      {/each}
    </div>
  {/if}

  <div class="field">
    <span>PIN</span>
    {#if device.hasPin && pin}
      <!-- Hover shows it on a desktop. A tap or Enter keeps it shown, for touch screens. -->
      <button class="pin mono" class:shown title={t("Show PIN")} aria-label={shown ? pin : t("Show PIN")} onclick={() => (shown = !shown)}>
        <span class="masked">{"•".repeat(pin.length)}</span><span class="plain">{pin}</span>
        <Icon name={shown ? "eye-off" : "eye"} size={16} />
      </button>
    {:else if device.hasPin}
      <small>{t("This PIN was set before Keyside kept PINs, so it can't be shown. Set a new one to see it here.")}</small>
    {:else}
      <small>{t("No PIN. Anyone holding the device can use the deck.")}</small>
    {/if}
    <div class="row">
      <input class="input mono" bind:value={newPin} inputmode="numeric" maxlength="12" pattern="[0-9]*" placeholder={device.hasPin ? t("New PIN") : t("4 to 12 digits")} aria-label={t("New PIN")} />
      <button class="btn small" disabled={!/^\d{4,12}$/.test(newPin)} onclick={() => setPin(newPin)}>{device.hasPin ? t("Change") : t("Set PIN")}</button>
      {#if device.hasPin}<button class="btn ghost small" onclick={() => setPin(null)}>{t("Remove")}</button>{/if}
    </div>
  </div>

  {#if error}<p class="bad">{error}</p>{:else if saved}<p class="ok">{saved}</p>{/if}

  <div class="revoke">
    {#if sure}
      <span>{t("Disconnect {name} for good? It has to pair again.", { name: device.name })}</span>
      <button class="btn danger small" onclick={() => onrevoke(device.id)}>{t("Revoke")}</button>
      <button class="btn ghost small" onclick={() => (sure = false)}>{t("Cancel")}</button>
    {:else}
      <button class="btn danger small" onclick={() => (sure = true)}><Icon name="unlink" size={16} />{t("Revoke")}</button>
    {/if}
  </div>
</div>

<style>
  .details {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  dl {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 6px 16px;
    margin: 0;
  }
  dt {
    color: var(--muted);
  }
  dd {
    margin: 0;
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .mods {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 16px;
  }
  .pin {
    display: inline-flex;
    align-items: center;
    gap: 10px;
    width: fit-content;
    padding: 6px 12px;
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    background: var(--surface);
    color: var(--text);
    font-size: 1.1rem;
    letter-spacing: 0.15em;
    cursor: pointer;
  }
  .pin .plain,
  .pin:hover .masked,
  .pin.shown .masked {
    display: none;
  }
  .pin:hover .plain,
  .pin.shown .plain {
    display: inline;
  }
  .row {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .row .input {
    width: 10ch;
  }
  .small {
    min-height: 34px;
    padding: 0 12px;
    font-size: 0.9rem;
  }
  .revoke {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    padding-top: 12px;
    border-top: 1px solid var(--line-soft);
  }
  .bad,
  .ok {
    margin: 0;
  }
  .bad {
    color: var(--danger-text);
  }
  .ok {
    color: var(--muted);
  }
</style>

<script lang="ts">
  // First-run setup: pick apps, get a starter layout for them, set up each plugin, pair a phone.
  import Icon from "../components/Icon.svelte";
  import Plugins from "./Plugins.svelte";
  import PairPanel from "./PairPanel.svelte";
  import { ed, change, push } from "./state.svelte.ts";
  import { STARTER_APPS, LOOKUPS, buildProfile, buildPage, fillable, lookupKey, app, type Found, type Lookup } from "./starter.ts";
  import { req } from "../lib/deck.svelte.ts";
  import { t } from "../lib/i18n.svelte.ts";

  let { fresh = false, onclose }: { fresh?: boolean; onclose: () => void } = $props();

  type Step = "apps" | "create" | "setup" | "pair";
  let step = $state<Step>("apps");
  let picked = $state<string[]>([]);
  let name = $state(t("My setup"));
  let replace = $state(false);
  let busy = $state("");
  let error = $state("");
  let made = $state<string | null>(null);
  let index = $state(0);
  let filled = $state<Record<string, string>>({});

  const chosen = $derived(STARTER_APPS.filter((a) => picked.includes(a.id)));
  const current = $derived(chosen[index]);

  function toggle(id: string) {
    picked = picked.includes(id) ? picked.filter((x) => x !== id) : [...picked, id];
  }

  /** Asks the apps for scenes, actions and such, giving each a few seconds. */
  async function look(apps: string[]): Promise<Found> {
    const found: Found = {};
    const list: Lookup[] = apps.flatMap((a) => LOOKUPS[a] ?? []);
    await Promise.all(
      list.map(async (l) => {
        try {
          const r = await Promise.race([req<Array<{ value: string; label: string }>>("options.get", { type: l.type, field: l.field, settings: {} }), new Promise<never>((_, no) => setTimeout(() => no(new Error("timeout")), 12_000))]);
          if (r.length) found[lookupKey(l)] = r;
        } catch {
          /* not connected yet: the page gets keys to finish later */
        }
      }),
    );
    return found;
  }

  async function create() {
    if (!ed.layout) return;
    error = "";
    if (!replace && ed.layout.profiles.length >= 20) {
      error = t("There are 20 layouts already. Delete one under Manage layouts first.");
      return;
    }
    busy = t("Looking at your apps…");
    try {
      const found = await look(picked);
      const profile = buildProfile(picked, found, name, replace ? [] : ed.layout.profiles.map((p) => p.id), t);
      change((l) => {
        l.profiles = replace ? [profile] : [...l.profiles, profile];
        l.defaultProfile = profile.id;
      });
      ed.profile = profile.id;
      ed.page = profile.homePage;
      ed.key = null;
      busy = t("Pushing to phones…");
      await push();
      if (ed.error) throw new Error(ed.error);
      // Phones and the PC switch to the new layout now, not only after a restart.
      await req("action.test", { action: { type: "core.profile", settings: { profile: profile.id } } }).catch(() => {});
      made = profile.id;
      index = 0;
      step = chosen.length ? "setup" : "pair";
    } catch (e: any) {
      error = e.message;
    } finally {
      busy = "";
    }
  }

  /** Rebuilds the page of one app now that its plugin can reach it, e.g. with the OBS scenes. */
  async function fill(id: string) {
    error = "";
    busy = t("Reading from {app}…", { app: app(id)?.name ?? id });
    try {
      const found = await look([id]);
      if (!Object.keys(found).length) {
        filled[id] = t("Nothing found yet. Check that the app is open and connected.");
        return;
      }
      const page = buildPage(id, found, t);
      change((l) => {
        const pg = l.profiles.find((p) => p.id === made)?.pages.find((p) => p.id === id);
        if (pg) pg.keys = page.keys;
      });
      await push();
      if (ed.error) throw new Error(ed.error);
      filled[id] = t("Added what {app} has to its page.", { app: app(id)?.name ?? id });
    } catch (e: any) {
      error = e.message;
    } finally {
      busy = "";
    }
  }

  function next() {
    if (index < chosen.length - 1) index++;
    else step = "pair";
  }

  function back() {
    if (index > 0) index--;
    else step = "create";
  }
</script>

<div class="wizard">
  <ol class="steps" aria-label={t("Steps")}>
    <li class:on={step === "apps"}>{t("Apps")}</li>
    <li class:on={step === "create"}>{t("Layout")}</li>
    <li class:on={step === "setup"}>{t("Connect")}</li>
    <li class:on={step === "pair"}>{t("Phone")}</li>
  </ol>

  {#if error}<p class="bad" role="alert">{error}</p>{/if}

  {#if step === "apps"}
    <h3>{fresh ? t("Welcome to Keyside") : t("Set up Keyside")}</h3>
    <p class="muted">{t("Pick the apps you use. Keyside makes a layout with a page of keys for each, then helps you connect them.")}</p>
    <div class="apps" role="group" aria-label={t("Apps")}>
      {#each STARTER_APPS as a (a.id)}
        <button class="app" class:on={picked.includes(a.id)} aria-pressed={picked.includes(a.id)} onclick={() => toggle(a.id)}>
          <Icon name={a.icon} size={22} /><span>{a.name}</span>{#if picked.includes(a.id)}<Icon name="check" size={18} />{/if}
        </button>
      {/each}
    </div>
    <div class="row end">
      <button class="btn ghost" onclick={onclose}>{t("Skip setup")}</button>
      <button class="btn primary" onclick={() => (step = picked.length ? "create" : "pair")}>{picked.length ? t("Next") : t("Skip to pairing")}</button>
    </div>
  {:else if step === "create"}
    <h3>{t("Your starter layout")}</h3>
    <p class="muted">{t("A new layout with a page for each app: {apps}.", { apps: chosen.map((a) => a.name).join(", ") })}</p>
    <label class="field"><span>{t("Layout name")}</span><input bind:value={name} maxlength="40" /></label>
    {#if fresh}
      <label class="check"><input type="checkbox" bind:checked={replace} />{t("Remove the example layout")}</label>
    {/if}
    <p class="muted">{replace ? t("The example layout is replaced. Push history keeps a copy.") : t("Your current layouts stay as they are. Switch between them from the Layout menu.")}</p>
    {#if ed.dirty && !made}<p class="warn"><Icon name="triangle-alert" size={18} />{t("Your unsaved changes are pushed together with the new layout.")}</p>{/if}
    <div class="row end">
      <button class="btn ghost" disabled={!!busy} onclick={() => (step = "apps")}>{t("Back")}</button>
      {#if made}
        <button class="btn primary" onclick={() => (step = "setup")}>{t("Next")}</button>
      {:else}
        <button class="btn primary" disabled={!!busy} onclick={create}>{busy || t("Create layout")}</button>
      {/if}
    </div>
  {:else if step === "setup" && current}
    <h3>{t("Connect {app}", { app: current.name })} <span class="muted count">{index + 1} / {chosen.length}</span></h3>
    {#key current.id}
      <Plugins admin focus={current.plugin} only />
    {/key}
    {#if fillable(current.id)}
      <div class="fill">
        <p class="muted">{t("Once it shows connected, read the scenes, actions or devices from the app into its page.")}</p>
        <button class="btn" disabled={!!busy} onclick={() => fill(current.id)}><Icon name="refresh-cw" size={18} />{busy || t("Fill the page from {app}", { app: current.name })}</button>
        {#if filled[current.id]}<p role="status">{filled[current.id]}</p>{/if}
      </div>
    {/if}
    <div class="row end">
      <button class="btn ghost" onclick={back}>{t("Back")}</button>
      <button class="btn primary" onclick={next}>{index < chosen.length - 1 ? t("Next app") : t("Next")}</button>
    </div>
  {:else if step === "pair"}
    <p class="muted">{t("Scan the code with the phone's camera to use it as your deck. You can pair more phones later under Devices.")}</p>
    <PairPanel />
    <div class="row end">
      {#if chosen.length}<button class="btn ghost" onclick={() => ((step = "setup"), (index = chosen.length - 1))}>{t("Back")}</button>{/if}
      <button class="btn primary" onclick={onclose}>{t("Finish")}</button>
    </div>
  {/if}
</div>

<style>
  .wizard {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .steps {
    display: flex;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
    counter-reset: step;
  }
  .steps li {
    flex: 1;
    padding: 6px 10px;
    border-radius: 8px;
    background: var(--bg);
    color: var(--muted);
    font-size: 0.9rem;
    counter-increment: step;
  }
  .steps li::before {
    content: counter(step) ". ";
  }
  .steps li.on {
    background: var(--accent-bg);
    color: var(--accent-text);
    font-weight: 600;
  }
  h3 {
    margin: 0;
    display: flex;
    align-items: baseline;
    gap: 10px;
  }
  .count {
    font-size: 0.9rem;
    font-weight: 400;
  }
  p {
    margin: 0;
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .apps {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(190px, 1fr));
    gap: 8px;
  }
  .app {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 52px;
    padding: 0 14px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--line);
    background: var(--bg);
    text-align: left;
    color: var(--text);
  }
  .app span {
    flex: 1;
  }
  .app.on {
    border-color: var(--accent);
    background: var(--accent-bg);
    color: var(--accent-text);
  }
  .row {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }
  .end {
    justify-content: flex-end;
  }
  .fill {
    display: flex;
    flex-direction: column;
    gap: 8px;
    align-items: flex-start;
  }
  .muted {
    color: var(--muted);
  }
  .warn {
    color: var(--warning);
  }
  .bad {
    color: var(--danger-text);
  }
</style>

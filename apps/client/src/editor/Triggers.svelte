<script lang="ts">
  // Chat triggers: a chat command, keyword or channel point reward presses a key.
  import type { Trigger } from "@keyside/protocol";
  import Icon from "../components/Icon.svelte";
  import { ed, change, newId } from "./state.svelte.ts";
  import { t, mark } from "../lib/i18n.svelte.ts";

  const kinds: Array<[Trigger["kind"], string, string]> = [
    ["command", mark("Chat command"), "!brb"],
    ["keyword", mark("Words in chat"), "good game"],
    ["redeem", mark("Channel point reward"), "Hydrate"],
  ];
  const sources: Array<[NonNullable<Trigger["source"]>, string]> = [
    ["any", mark("Any platform")],
    ["twitch", "Twitch"],
    ["youtube", "YouTube"],
    ["kick", mark("Kick (through Streamer.bot)")],
  ];
  const audiences: Array<[NonNullable<Trigger["who"]>, string]> = [
    ["everyone", mark("Everyone")],
    ["subs", mark("Subscribers and members")],
    ["mods", mark("Moderators")],
    ["broadcaster", mark("Only me")],
  ];

  /** Every key with steps, as "Layout · Page · Key". */
  const keys = $derived(
    (ed.layout?.profiles ?? []).flatMap((p) =>
      p.pages.flatMap((pg) =>
        pg.keys
          .filter((k) => k.actions.length || k.offActions?.length)
          .map((k) => ({ value: `${p.id}/${pg.id}/${k.id}`, label: [p.name, pg.name, k.label || t("Untitled key")].join(" · ") })),
      ),
    ),
  );
  const list = $derived(ed.layout?.triggers ?? []);
  const target = (tr: Trigger) => `${tr.target.profile}/${tr.target.page}/${tr.target.key}`;

  function edit(id: string, fn: (tr: Trigger) => void) {
    change((l) => {
      const tr = l.triggers?.find((x) => x.id === id);
      if (tr) fn(tr);
    });
  }

  function add() {
    // The key being edited, or else the first one with steps.
    const here = ed.key ? `${ed.profile}/${ed.page}/${ed.key}` : "";
    const pick = keys.find((k) => k.value === here) ?? keys[0];
    if (!pick) return;
    const [profile, page, key] = pick.value.split("/");
    const id = newId("t", list.map((x) => x.id));
    change((l) => (l.triggers ??= []).push({ id, kind: "command", match: "", target: { profile, page, key } }));
  }

  function remove(id: string) {
    change((l) => {
      l.triggers = (l.triggers ?? []).filter((x) => x.id !== id);
      if (!l.triggers.length) delete l.triggers;
    });
  }

  function retarget(id: string, value: string) {
    const [profile, page, key] = value.split("/");
    edit(id, (tr) => (tr.target = { profile, page, key }));
  }
</script>

<p class="muted">{t("Run a key's steps when someone types a command in chat or redeems a channel point reward. Twitch and YouTube work directly; Kick works through Streamer.bot.")}</p>
{#each list as tr (tr.id)}
  {@const kind = kinds.find((k) => k[0] === tr.kind) ?? kinds[0]}
  {@const missing = !keys.some((k) => k.value === target(tr))}
  <div class="trigger" class:off={tr.disabled}>
    <div class="grid">
      <label class="field">
        <span>{t("When")}</span>
        <select value={tr.kind} onchange={(e) => edit(tr.id, (x) => (x.kind = e.currentTarget.value as Trigger["kind"]))}>
          {#each kinds as [v, name] (v)}<option value={v}>{t(name)}</option>{/each}
        </select>
      </label>
      <label class="field">
        <span>{tr.kind === "redeem" ? t("Reward name") : tr.kind === "keyword" ? t("Words") : t("Command")}</span>
        <input value={tr.match} maxlength="100" placeholder={kind[2]} oninput={(e) => edit(tr.id, (x) => (x.match = e.currentTarget.value))} />
      </label>
      <label class="field">
        <span>{t("From")}</span>
        <select value={tr.source ?? "any"} onchange={(e) => edit(tr.id, (x) => (e.currentTarget.value === "any" ? delete x.source : (x.source = e.currentTarget.value as Trigger["source"])))}>
          {#each sources as [v, name] (v)}<option value={v}>{t(name)}</option>{/each}
        </select>
      </label>
      <label class="field">
        <span>{t("Who can use it")}</span>
        <select value={tr.who ?? "everyone"} onchange={(e) => edit(tr.id, (x) => (e.currentTarget.value === "everyone" ? delete x.who : (x.who = e.currentTarget.value as Trigger["who"])))}>
          {#each audiences as [v, name] (v)}<option value={v}>{t(name)}</option>{/each}
        </select>
      </label>
      <label class="field">
        <span>{t("Wait before it can run again (seconds)")}</span>
        <input
          type="number"
          min="0"
          max="86400"
          value={tr.cooldown ?? 0}
          oninput={(e) => {
            const n = Math.min(86400, Math.max(0, Math.round(Number(e.currentTarget.value) || 0)));
            edit(tr.id, (x) => (n ? (x.cooldown = n) : delete x.cooldown));
          }}
        />
      </label>
      <label class="field key">
        <span>{t("Press this key")}</span>
        <select value={target(tr)} onchange={(e) => retarget(tr.id, e.currentTarget.value)}>
          {#if missing}<option value={target(tr)}>{t("Deleted key")}</option>{/if}
          {#each keys as k (k.value)}<option value={k.value}>{k.label}</option>{/each}
        </select>
      </label>
    </div>
    {#if missing}<p class="warn">{t("The key for this trigger is gone. Pick another one.")}</p>{/if}
    {#if !tr.match.trim()}<p class="warn">{t("Fill in what to listen for.")}</p>{/if}
    <div class="row">
      <label class="check"><input type="checkbox" checked={!tr.disabled} onchange={(e) => edit(tr.id, (x) => (e.currentTarget.checked ? delete x.disabled : (x.disabled = true)))} />{t("Turned on")}</label>
      <button class="btn danger" onclick={() => remove(tr.id)}><Icon name="trash-2" size={18} />{t("Delete")}</button>
    </div>
  </div>
{:else}
  <p class="empty">{t("No chat triggers yet.")}</p>
{/each}
{#if keys.length}
  <button class="btn" onclick={add}><Icon name="plus" size={18} />{t("Add chat trigger")}</button>
{:else}
  <p class="muted">{t("Add a key with steps first. Triggers press keys.")}</p>
{/if}
<p class="muted small">{t("Steps can use {core.trigger.user} and {core.trigger.input} (the words after the command, or the text sent with a reward). Changes go live with Push to phones.")}</p>

<style>
  .muted {
    margin: 0;
    color: var(--muted);
  }
  .small {
    font-size: 0.9rem;
  }
  .empty {
    margin: 0;
    padding: 16px;
    border-radius: var(--radius);
    background: var(--bg);
    color: var(--muted);
    text-align: center;
  }
  .trigger {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 16px;
    border-radius: var(--radius);
    background: var(--bg);
  }
  .trigger.off .grid {
    opacity: 0.55;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: 12px;
  }
  .key {
    grid-column: 1 / -1;
  }
  .warn {
    margin: 0;
    color: var(--warning);
  }
  .row {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
</style>

<script lang="ts">
  import Icon from "./Icon.svelte";
  import { deck, unlock } from "../lib/deck.svelte.ts";
  import { t } from "../lib/i18n.svelte.ts";
  import { haptic } from "../lib/native.ts";

  let pin = $state("");
  let wait = $state(0);

  $effect(() => {
    if (deck.lockReason === "wrong-pin") {
      pin = "";
      haptic("heavy");
    }
    if (deck.retryAfter > 0) {
      wait = deck.retryAfter;
      const id = setInterval(() => (wait = Math.max(0, wait - 1)), 1000);
      return () => clearInterval(id);
    }
  });

  function add(d: string) {
    if (wait > 0 || pin.length >= 12) return;
    haptic("light");
    pin += d;
  }
  function submit() {
    if (pin.length >= 4) {
      unlock(pin);
      deck.lockReason = "pin";
    }
  }
</script>

<section class="pin">
  <Icon name="lock" size={32} />
  <h1>{t("Enter PIN")}</h1>
  <p class:bad={deck.lockReason !== "pin"}>
    {#if wait > 0}
      {t("Too many tries. Wait {s} s.", { s: wait })}
    {:else if deck.lockReason === "wrong-pin"}
      {t("Wrong PIN, try again.")}
    {:else}
      {t("This device needs its PIN to open {name}.", { name: deck.companion?.name ?? "Keyside" })}
    {/if}
  </p>
  <div class="dots" aria-label={t("{n} digits entered", { n: pin.length })}>
    {#each { length: Math.max(4, pin.length) } as _, i (i)}<span class:on={i < pin.length}></span>{/each}
  </div>
  <div class="pad">
    {#each ["1", "2", "3", "4", "5", "6", "7", "8", "9"] as d (d)}
      <button class="btn" onclick={() => add(d)} disabled={wait > 0}>{d}</button>
    {/each}
    <button class="btn ghost" aria-label={t("Delete")} onclick={() => (pin = pin.slice(0, -1))}><Icon name="arrow-left" /></button>
    <button class="btn" onclick={() => add("0")} disabled={wait > 0}>0</button>
    <button class="btn primary" aria-label={t("Unlock")} onclick={submit} disabled={pin.length < 4 || wait > 0}><Icon name="check" /></button>
  </div>
</section>

<style>
  .pin {
    margin: auto;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    padding: 24px;
    text-align: center;
    max-width: 360px;
  }
  h1 {
    margin: 0;
    font-size: 1.4rem;
  }
  p {
    margin: 0;
    color: var(--muted);
    min-height: 2.8em;
  }
  p.bad {
    color: var(--danger-text);
  }
  .dots {
    display: flex;
    gap: 12px;
    margin: 8px 0 16px;
  }
  .dots span {
    width: 12px;
    height: 12px;
    border-radius: 50%;
    border: 2px solid var(--line);
  }
  .dots span.on {
    background: var(--accent);
    border-color: var(--accent);
  }
  .pad {
    display: grid;
    grid-template-columns: repeat(3, 72px);
    gap: 12px;
  }
  .pad .btn {
    height: 64px;
    font-size: 1.4rem;
  }
</style>

<script lang="ts">
  // How a plugin is doing: uptime, restarts and crashes in the last hour, its last error and
  // recent status messages, plus a connection test. Works for Stream Deck plugins ("sd.<uuid>") too.
  import Icon from "../components/Icon.svelte";
  import Rich from "./Rich.svelte";
  import { req, deck } from "../lib/deck.svelte.ts";
  import { t, mark } from "../lib/i18n.svelte.ts";
  import { ago, everySecond } from "../lib/format.ts";

  interface Event {
    at: number;
    state: string;
    detail?: string;
  }
  interface Report {
    runningSince: number | null;
    restarts: number;
    crashes: number;
    lastError: { at: number; text: string } | null;
    recent: Event[];
    inUse: boolean;
    running: boolean;
  }
  interface TestResult {
    status: string;
    detail?: string;
    ms: number;
    timedOut?: boolean;
  }

  let { id }: { id: string } = $props();

  const stateText: Record<string, string> = {
    connected: mark("connected"),
    connecting: mark("connecting"),
    offline: mark("offline"),
    error: mark("needs attention"),
    started: mark("started"),
    stopped: mark("stopped"),
    crashed: mark("stopped unexpectedly"),
    disabled: mark("off"),
  };

  let report = $state<Report | null>(null);
  let now = $state(Date.now());
  let testing = $state(false);
  let result = $state<TestResult | null>(null);
  let error = $state("");

  async function refresh() {
    try {
      const all = await req<Record<string, Report>>("plugins.health");
      report = all[id] ?? null;
    } catch (e: any) {
      error = e.message;
    }
  }

  $effect(() => everySecond((n) => (now = n)));
  // Status changes arrive live; refresh with them, and now and then for the uptime.
  $effect(() => {
    void deck.connections;
    void refresh();
  });
  $effect(() => {
    const timer = setInterval(refresh, 10_000);
    return () => clearInterval(timer);
  });

  async function test() {
    testing = true;
    result = null;
    error = "";
    try {
      result = await req<TestResult>("plugins.test", { id });
    } catch (e: any) {
      error = e.message;
    } finally {
      testing = false;
      void refresh();
    }
  }

  const time = (at: number) => new Date(at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" });
  const secs = (ms: number) => (ms / 1000).toFixed(1);
</script>

<section class="health" aria-label={t("Health")}>
  <h4>{t("Health")}</h4>
  {#if report}
    <dl>
      <dt>{t("Running")}</dt>
      <dd>
        {#if report.runningSince}{t("for {time}", { time: ago(report.runningSince, now) })}
        {:else if !report.inUse}{t("No, no key uses it yet")}
        {:else}{t("No")}{/if}
      </dd>
      <dt>{t("Last hour")}</dt>
      <dd class:warn={report.crashes > 0}>{t("Restarts: {restarts} · Crashes: {crashes}", { restarts: report.restarts, crashes: report.crashes })}</dd>
      <dt>{t("Last error")}</dt>
      <dd>
        {#if report.lastError}<span class="bad"><Rich text={report.lastError.text} /></span> <span class="muted">{t("{time} ago", { time: ago(report.lastError.at, now) })}</span>
        {:else}<span class="muted">{t("None")}</span>{/if}
      </dd>
    </dl>
    {#if report.recent.length}
      <details>
        <summary>{t("Recent messages ({n})", { n: report.recent.length })}</summary>
        <ol class="log">
          {#each report.recent as e, i (i)}
            <li><span class="mono muted">{time(e.at)}</span> <b>{t(stateText[e.state] ?? e.state)}</b>{#if e.detail} <Rich text={e.detail} />{/if}</li>
          {/each}
        </ol>
      </details>
    {/if}
  {/if}
  <div class="row">
    <button class="btn" disabled={testing} onclick={test}><Icon name="activity" size={18} />{testing ? t("Testing…") : t("Test connection")}</button>
    {#if result}
      <span class="result" class:ok={result.status === "connected"} class:bad={result.status !== "connected"} role="status">
        {#if result.status === "connected"}{t("Connected in {s} s", { s: secs(result.ms) })}
        {:else if result.timedOut}{t("Still connecting after {s} s", { s: secs(result.ms) })}{#if result.detail}: <Rich text={result.detail} />{/if}
        {:else if result.status === "disabled"}{t("The plugin is turned off.")}
        {:else}{t("Couldn't connect")}{#if result.detail}: <Rich text={result.detail} />{/if}{/if}
      </span>
    {/if}
  </div>
  {#if error}<p class="bad" role="alert">{error}</p>{/if}
</section>

<style>
  .health {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--line-soft);
  }
  h4 {
    margin: 0;
    font-size: 0.95rem;
    color: var(--muted);
    font-weight: 500;
  }
  dl {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 4px 16px;
    margin: 0;
  }
  dt {
    color: var(--muted);
  }
  dd {
    margin: 0;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  details summary {
    cursor: pointer;
    color: var(--muted);
  }
  .log {
    margin: 8px 0 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 4px;
    max-height: 220px;
    overflow: auto;
    font-size: 0.9rem;
  }
  .row {
    display: flex;
    gap: 12px;
    align-items: center;
    flex-wrap: wrap;
  }
  .result {
    overflow-wrap: anywhere;
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
  .ok {
    color: var(--success);
  }
  p {
    margin: 0;
  }
</style>

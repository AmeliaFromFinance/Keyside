<script lang="ts">
  // A report for bug reports, made by the companion so it reads the same everywhere.
  import Icon from "../components/Icon.svelte";
  import { req } from "../lib/deck.svelte.ts";
  import { copyText } from "../lib/rich.ts";
  import { t } from "../lib/i18n.svelte.ts";

  let text = $state("");
  let copied = $state(false);
  let error = $state("");
  let timer: ReturnType<typeof setTimeout> | undefined;

  async function get() {
    error = "";
    try {
      text = (await req<{ text: string }>("diagnostics.get", undefined, 30_000)).text;
      return text;
    } catch (e: any) {
      error = e.message;
      return null;
    }
  }

  async function copy() {
    const r = await get();
    if (r === null) return;
    if (!(await copyText(r))) {
      error = t("Couldn't copy. Use Save as file instead.");
      return;
    }
    copied = true;
    clearTimeout(timer);
    timer = setTimeout(() => (copied = false), 2000);
  }

  async function saveFile() {
    const r = await get();
    if (r === null) return;
    const d = new Date();
    const day = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
    const a = document.createElement("a");
    a.href = URL.createObjectURL(new Blob([r], { type: "text/plain" }));
    a.download = `keyside-diagnostics-${day}.txt`;
    a.click();
    setTimeout(() => URL.revokeObjectURL(a.href), 1000);
  }
</script>

<section class="card">
  <h3>{t("Diagnostics")}</h3>
  <p class="muted">{t("A text report for bug reports: versions, settings, plugin status and recent log messages. It leaves out passwords, tokens, names, addresses and what is on your keys. It only leaves this PC if you paste or send it somewhere.")}</p>
  {#if error}<p class="bad" role="alert">{error}</p>{/if}
  <div class="row">
    <button class="btn" onclick={copy}><Icon name={copied ? "check" : "copy"} size={16} />{copied ? t("Copied") : t("Copy diagnostics")}</button>
    <button class="btn" onclick={saveFile}><Icon name="file-text" size={16} />{t("Save as file")}</button>
  </div>
  {#if text}
    <details>
      <summary>{t("Show report")}</summary>
      <pre class="mono">{text}</pre>
    </details>
  {/if}
</section>

<style>
  .card {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 16px;
    border-radius: var(--radius);
    background: var(--bg);
  }
  h3 {
    margin: 0 0 4px;
    font-size: 1rem;
  }
  p {
    margin: 0;
  }
  .row {
    display: flex;
    gap: 12px;
    flex-wrap: wrap;
  }
  pre {
    max-height: 320px;
    overflow: auto;
    margin: 8px 0 0;
    padding: 10px;
    border-radius: var(--radius-sm);
    background: var(--raised);
    font-size: 0.8rem;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  summary {
    cursor: pointer;
  }
  .muted {
    color: var(--muted);
  }
  .bad {
    color: var(--danger-text);
  }
</style>

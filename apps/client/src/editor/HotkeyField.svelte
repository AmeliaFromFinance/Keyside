<script lang="ts">
  import Icon from "../components/Icon.svelte";
  import { t } from "../lib/i18n.svelte.ts";

  let { value = "", onchange }: { value?: string; onchange: (v: string) => void } = $props();

  let listening = $state(false);
  let typing = $state(false);
  let held = $state("");
  let box = $state<HTMLButtonElement>();

  // KeyboardEvent.code to the names the companion understands. Layout-independent, so
  // Shift+1 records as shift+1, not "!".
  const NAMED: Record<string, string> = {
    Enter: "enter",
    NumpadEnter: "enter",
    Tab: "tab",
    Space: "space",
    Escape: "esc",
    Backspace: "backspace",
    Delete: "delete",
    ArrowUp: "up",
    ArrowDown: "down",
    ArrowLeft: "left",
    ArrowRight: "right",
    Home: "home",
    End: "end",
    PageUp: "pageup",
    PageDown: "pagedown",
    Insert: "insert",
    CapsLock: "capslock",
    Minus: "-",
    Equal: "=",
    BracketLeft: "[",
    BracketRight: "]",
    Semicolon: ";",
    Quote: "'",
    Comma: ",",
    Period: ".",
    Slash: "/",
    Backslash: "\\",
    Backquote: "`",
  };
  const MODS = new Set(["ControlLeft", "ControlRight", "ShiftLeft", "ShiftRight", "AltLeft", "AltRight", "MetaLeft", "MetaRight", "OSLeft", "OSRight"]);

  function mainKey(code: string): string | null {
    if (/^Key[A-Z]$/.test(code)) return code.slice(3).toLowerCase();
    if (/^(Digit|Numpad)[0-9]$/.test(code)) return code.slice(-1);
    if (/^F([1-9]|1[0-9]|20)$/.test(code)) return code.toLowerCase();
    return NAMED[code] ?? null;
  }

  function mods(e: KeyboardEvent): string[] {
    const m: string[] = [];
    if (e.ctrlKey) m.push("ctrl");
    if (e.shiftKey) m.push("shift");
    if (e.altKey) m.push("alt");
    if (e.metaKey) m.push("super");
    return m;
  }

  function down(e: KeyboardEvent) {
    if (!listening) return;
    // Plain Tab and Escape leave the box, so the keyboard never gets stuck here.
    const plain = !e.ctrlKey && !e.altKey && !e.metaKey && !e.shiftKey;
    if (plain && (e.code === "Tab" || e.code === "Escape")) {
      stop();
      return;
    }
    e.preventDefault();
    e.stopPropagation();
    if (MODS.has(e.code)) {
      held = pretty(mods(e).join("+"));
      return;
    }
    const key = mainKey(e.code);
    if (!key) {
      held = t("That key can't be sent");
      return;
    }
    onchange([...mods(e), key].join("+"));
    stop();
  }

  function up(e: KeyboardEvent) {
    if (!listening) return;
    e.preventDefault();
    held = pretty(mods(e).join("+"));
  }

  function start() {
    listening = true;
    held = "";
  }
  function stop() {
    listening = false;
    held = "";
  }
  const pretty = (v: string) =>
    v
      .split("+")
      .map((p) => (p.length === 1 ? p.toUpperCase() : p.charAt(0).toUpperCase() + p.slice(1)))
      .join(" + ");
</script>

{#if typing}
  <div class="row">
    <input value={value} maxlength="60" placeholder="ctrl+shift+f13" oninput={(e) => onchange(e.currentTarget.value)} />
    <button type="button" class="btn icon" title={t("Record keys")} aria-label={t("Record keys")} onclick={() => ((typing = false), start(), box?.focus())}><Icon name="keyboard" size={18} /></button>
  </div>
{:else}
  <div class="row">
    <button
      bind:this={box}
      type="button"
      class="record"
      class:listening
      onclick={start}
      onblur={stop}
      onkeydown={down}
      onkeyup={up}
    >
      {#if listening}
        <span class="hint">{held || t("Press the keys now")}</span>
      {:else if value}
        <span class="mono">{pretty(value)}</span>
      {:else}
        <span class="hint">{t("Click, then press the keys")}</span>
      {/if}
    </button>
    <button type="button" class="btn icon" title={t("Type it instead")} aria-label={t("Type it instead")} onclick={() => ((typing = true), stop())}><Icon name="pencil" size={18} /></button>
  </div>
{/if}

<style>
  .row {
    display: flex;
    gap: 8px;
  }
  .row input {
    flex: 1;
    min-width: 0;
  }
  .record {
    flex: 1;
    min-width: 0;
    min-height: 42px;
    padding: 8px 12px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--line);
    background: var(--bg);
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .record.listening {
    border-color: var(--accent);
    box-shadow: 0 0 0 1px var(--accent);
  }
  .hint {
    color: var(--muted);
  }
</style>

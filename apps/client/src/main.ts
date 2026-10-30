import "@fontsource-variable/inter";
import "@fontsource/jetbrains-mono/400.css";
import "./app.css";
import { mount } from "svelte";
import App from "./App.svelte";
import { lang, loadLang } from "./lib/i18n.svelte.ts";

// Text in the right language from the first frame. English needs no download.
await loadLang(lang()).catch(() => {});
mount(App, { target: document.getElementById("app")! });

// Offline shell for the browser deck. Browsers skip this when the certificate isn't trusted.
if ("serviceWorker" in navigator && location.protocol === "https:" && !(globalThis as any).Capacitor) {
  navigator.serviceWorker.register("/sw.js").catch(() => {});
}

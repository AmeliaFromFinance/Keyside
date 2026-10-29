import "@fontsource-variable/inter";
import "@fontsource/jetbrains-mono/400.css";
import "./app.css";
import { mount } from "svelte";
import App from "./App.svelte";

mount(App, { target: document.getElementById("app")! });

// Offline shell for the browser deck. Browsers skip this when the certificate isn't trusted.
if ("serviceWorker" in navigator && location.protocol === "https:" && !(globalThis as any).Capacitor) {
  navigator.serviceWorker.register("/sw.js").catch(() => {});
}

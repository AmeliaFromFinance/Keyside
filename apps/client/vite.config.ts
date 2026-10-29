import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

export default defineConfig({
  plugins: [svelte()],
  build: {
    outDir: "dist",
    assetsDir: "assets",
    target: "es2022",
    // No inline assets: the companion's CSP only allows files from the same origin.
    assetsInlineLimit: 0,
    modulePreload: { polyfill: false },
  },
  server: {
    // `pnpm dev` talks to a running companion on the default editor port.
    proxy: { "/admin": { target: "http://127.0.0.1:7442", ws: true, changeOrigin: true, headers: { origin: "http://127.0.0.1:7442" } } },
  },
});

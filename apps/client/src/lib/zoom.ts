// Editor zoom in the desktop app: Ctrl (Cmd on a Mac) with + - 0, or with the scroll wheel.
// The app zooms its own window, so this only asks it to. Browsers zoom on their own.

import { req } from "./deck.svelte.ts";

export const inDesktopApp = () => !!(globalThis as { __KEYSIDE_DESKTOP__?: boolean }).__KEYSIDE_DESKTOP__;

export interface Zoom {
  zoom: number;
  auto: boolean;
}

/** One step bigger (1) or smaller (-1), or back to automatic (0). */
export const zoomStep = (step: -1 | 0 | 1) => req<Zoom>("editor.zoom", { step });

/** A fixed level, or null for automatic. */
export const zoomTo = (value: number | null) => req<Zoom>("editor.zoom", { value });

/** Listens for zoom keys and Ctrl + scroll. Returns a function that stops listening. */
export function zoomControls(): () => void {
  if (!inDesktopApp()) return () => {};
  const mac = /Mac/i.test(navigator.userAgent);
  let last = 0;
  const run = (step: -1 | 0 | 1) => void zoomStep(step).catch(() => {});

  const key = (e: KeyboardEvent) => {
    if (!(mac ? e.metaKey : e.ctrlKey) || e.altKey) return;
    const step = e.key === "+" || e.key === "=" ? 1 : e.key === "-" ? -1 : e.key === "0" ? 0 : null;
    if (step === null) return;
    e.preventDefault();
    run(step);
  };
  // Trackpad pinches also arrive as Ctrl + scroll, many per gesture, so allow one step at a time.
  const wheel = (e: WheelEvent) => {
    if (!e.ctrlKey) return;
    e.preventDefault();
    const now = performance.now();
    if (e.deltaY === 0 || now - last < 150) return;
    last = now;
    run(e.deltaY < 0 ? 1 : -1);
  };
  addEventListener("keydown", key);
  addEventListener("wheel", wheel, { passive: false });
  return () => {
    removeEventListener("keydown", key);
    removeEventListener("wheel", wheel);
  };
}

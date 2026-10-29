// Phone features. Inside the native app they come from Capacitor plugins registered on
// window.Capacitor; in a browser the web APIs are used where they exist.

type Plugins = Record<string, any>;

function cap(): { isNativePlatform?: () => boolean; getPlatform?: () => string; Plugins?: Plugins } | undefined {
  return (globalThis as any).Capacitor;
}

export function isNative(): boolean {
  return !!cap()?.isNativePlatform?.();
}

export function platform(): "android" | "ios" | "web" {
  const p = cap()?.getPlatform?.();
  return p === "android" || p === "ios" ? p : "web";
}

export function haptic(kind: "light" | "medium" | "heavy" = "light") {
  const h = cap()?.Plugins?.Haptics;
  if (h) {
    void h.impact({ style: kind.toUpperCase() }).catch(() => {});
    return;
  }
  navigator.vibrate?.(kind === "heavy" ? 25 : kind === "medium" ? 15 : 8);
}

let wakeLock: any = null;

/** Keeps the screen on while the deck is open. */
export async function keepAwake(on: boolean) {
  const k = cap()?.Plugins?.KeepAwake;
  if (k) {
    await (on ? k.keepAwake() : k.allowSleep()).catch(() => {});
    return;
  }
  try {
    if (on && !wakeLock && "wakeLock" in navigator) {
      wakeLock = await (navigator as any).wakeLock.request("screen");
      wakeLock.addEventListener?.("release", () => (wakeLock = null));
    } else if (!on && wakeLock) {
      await wakeLock.release();
      wakeLock = null;
    }
  } catch {
    /* not allowed right now, e.g. page hidden */
  }
}

/** Native QR scanner when available. Returns null when the person cancels or there is none. */
export async function nativeScan(): Promise<string | null | undefined> {
  const s = cap()?.Plugins?.BarcodeScanner;
  if (!s) return undefined;
  try {
    const perm = await s.requestPermissions();
    if (perm?.camera !== "granted" && perm?.camera !== "limited") return null;
    const r = await s.scan({ formats: ["QR_CODE"] });
    return r?.barcodes?.[0]?.rawValue ?? null;
  } catch {
    return null;
  }
}

export function deviceName(): string {
  const ua = navigator.userAgent;
  if (/iPad/.test(ua) || (/Macintosh/.test(ua) && navigator.maxTouchPoints > 1)) return "iPad";
  if (/iPhone/.test(ua)) return "iPhone";
  if (/Android/.test(ua)) return /Mobile/.test(ua) ? "Android phone" : "Android tablet";
  if (/Windows/.test(ua)) return "Windows browser";
  if (/Mac/.test(ua)) return "Mac browser";
  if (/Linux/.test(ua)) return "Linux browser";
  return "Browser";
}

export function deviceKind(): "phone" | "tablet" | "desktop" | "browser" {
  const s = Math.min(screen.width, screen.height);
  if (navigator.maxTouchPoints > 0) return s < 600 ? "phone" : "tablet";
  return isNative() ? "phone" : "browser";
}

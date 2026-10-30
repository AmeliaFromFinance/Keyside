export function clock(ms: number): string {
  const s = Math.max(0, Math.floor(ms / 1000));
  const p = (n: number) => String(n).padStart(2, "0");
  return `${p(Math.floor(s / 3600))}:${p(Math.floor(s / 60) % 60)}:${p(s % 60)}`;
}

export function ago(at: number, now: number): string {
  const s = Math.max(0, Math.round((now - at) / 1000));
  if (s < 60) return `${s} s`;
  if (s < 3600) return `${Math.floor(s / 60)} min`;
  if (s < 48 * 3600) return `${Math.floor(s / 3600)} h`;
  if (s < 365 * 86400) return `${Math.floor(s / 86400)} d`;
  return `${Math.floor(s / (365 * 86400))} y`;
}

/** Ticks once a second while something on screen needs a live clock. */
let listeners = new Set<(now: number) => void>();
let timer: ReturnType<typeof setInterval> | null = null;
export function everySecond(fn: (now: number) => void): () => void {
  listeners.add(fn);
  timer ??= setInterval(() => {
    const now = Date.now();
    for (const l of listeners) l(now);
  }, 1000);
  return () => {
    listeners.delete(fn);
    if (!listeners.size && timer) {
      clearInterval(timer);
      timer = null;
    }
  };
}
